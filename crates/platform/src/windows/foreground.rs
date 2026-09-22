//! Which application has keyboard focus.

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumChildWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};
use windows::core::{BOOL, PWSTR};

use crate::ForegroundApp;

fn pid_of(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid
}

fn exe_of(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        path.rsplit(['\\', '/']).next().map(|s| s.to_string())
    }
}

pub(crate) fn process_elevated(process: HANDLE) -> Option<bool> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(token);
        ok.then_some(elevation.TokenIsElevated != 0)
    }
}

fn pid_elevated(pid: u32) -> bool {
    unsafe {
        match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => {
                // Token queries on elevated processes are allowed from medium
                // integrity (Task Manager relies on this), so this is definite.
                let e = process_elevated(h);
                let _ = CloseHandle(h);
                e.unwrap_or(false)
            }
            Err(_) => false,
        }
    }
}

struct ChildSearch {
    host_pid: u32,
    found: u32,
}

unsafe extern "system" fn find_child(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let s = &mut *(lparam.0 as *mut ChildSearch);
    let pid = pid_of(hwnd);
    if pid != 0 && pid != s.host_pid {
        s.found = pid;
        return BOOL(0);
    }
    BOOL(1)
}

pub fn foreground_app() -> Option<ForegroundApp> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let mut pid = pid_of(hwnd);
    let mut exe = exe_of(pid).unwrap_or_default();
    // UWP apps (Mail, WhatsApp, Calculator...) live inside ApplicationFrameHost.
    if exe.eq_ignore_ascii_case("ApplicationFrameHost.exe") {
        let mut s = ChildSearch { host_pid: pid, found: 0 };
        unsafe {
            let _ = EnumChildWindows(Some(hwnd), Some(find_child), LPARAM(&mut s as *mut _ as isize));
        }
        if s.found != 0 {
            pid = s.found;
            exe = exe_of(pid).unwrap_or(exe);
        }
    }
    let mut title = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, &mut title) };
    let title = String::from_utf16_lossy(&title[..n.max(0) as usize]);
    Some(ForegroundApp { exe, title, pid, elevated: pid_elevated(pid) })
}

//! Misc. system integration: autostart, GPU inventory, memory, shell.

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE};
use windows::Win32::System::ProcessStatus::{EmptyWorkingSet, GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE,
    REG_SZ, RRF_RT_REG_SZ,
};
use windows::Win32::System::Threading::GetCurrentProcess;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::wide;
use crate::GpuInfo;

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: PCWSTR = w!("HushType");

/// Register/unregister "start with Windows" for the current user.
pub fn set_autostart(enabled: bool, exe: &str) -> Result<(), String> {
    unsafe {
        let mut key = HKEY::default();
        let r = RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, KEY_SET_VALUE, &mut key);
        if r != ERROR_SUCCESS {
            return Err(format!("cannot open Run key ({})", r.0));
        }
        let res = if enabled {
            let cmd = format!("\"{exe}\" --autostart");
            let data: Vec<u8> = wide(&cmd).iter().flat_map(|u| u.to_le_bytes()).collect();
            RegSetValueExW(key, VALUE, None, REG_SZ, Some(&data))
        } else {
            let r = RegDeleteValueW(key, VALUE);
            if r.0 == 2 { ERROR_SUCCESS } else { r } // not found is fine
        };
        let _ = RegCloseKey(key);
        if res == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("registry error {}", res.0))
        }
    }
}

pub fn autostart_enabled() -> bool {
    unsafe {
        let mut size = 0u32;
        RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE, RRF_RT_REG_SZ, None, None, Some(&mut size)) == ERROR_SUCCESS
    }
}

pub fn gpus() -> Vec<GpuInfo> {
    let mut out = Vec::new();
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else { return out };
        let mut i = 0;
        while let Ok(adapter) = factory.EnumAdapters1(i) {
            i += 1;
            let Ok(desc) = adapter.GetDesc1() else { continue };
            if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                continue;
            }
            let name = String::from_utf16_lossy(&desc.Description).trim_end_matches('\0').to_string();
            let vram = desc.DedicatedVideoMemory as u64 / (1 << 20);
            out.push(GpuInfo { name, dedicated_vram_mb: vram, integrated: vram < 512 });
        }
    }
    out
}

/// (working set, private bytes) of this process.
pub fn process_memory() -> (u64, u64) {
    let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
    unsafe {
        let _ = GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        );
    }
    (c.WorkingSetSize as u64, c.PrivateUsage as u64)
}

/// Release unused pages back to Windows (after closing the UI or unloading).
pub fn trim_memory() {
    unsafe {
        let _ = EmptyWorkingSet(GetCurrentProcess());
    }
}

pub fn is_elevated() -> bool {
    super::foreground::process_elevated(unsafe { GetCurrentProcess() }).unwrap_or(false)
}

/// Open a URL, settings page ("ms-settings:privacy-microphone") or folder.
pub fn open_external(target: &str) -> Result<(), String> {
    let allowed = target.starts_with("https://") || target.starts_with("ms-settings:") || std::path::Path::new(target).is_dir();
    if !allowed {
        return Err("refusing to open this target".into());
    }
    let h = unsafe { ShellExecuteW(None, w!("open"), &HSTRING::from(target), None, None, SW_SHOWNORMAL) };
    if h.0 as isize > 32 {
        Ok(())
    } else {
        Err(format!("ShellExecute failed ({})", h.0 as isize))
    }
}

//! Clipboard access with full save/restore so paste-based insertion doesn't
//! destroy what the user had copied. Dictated text placed on the clipboard is
//! flagged so Windows clipboard history and cloud sync ignore it.

use std::time::Duration;

use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE};

const CF_UNICODETEXT: u32 = 13;
/// Upper bound on clipboard data we are willing to copy for restoration.
const MAX_SAVE_BYTES: usize = 64 * 1024 * 1024;

/// A message-only window to own the clipboard (SetClipboardData fails when
/// the clipboard was opened without an owner window).
pub(crate) struct Owner(HWND);

impl Owner {
    pub fn new() -> Result<Owner, String> {
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("HushTypeClipboard"),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
            .map(Owner)
            .map_err(|e| e.to_string())
        }
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

/// RAII clipboard session with retries (other apps hold it briefly).
struct Session;

impl Session {
    fn open(owner: &Owner) -> Result<Session, String> {
        for _ in 0..25 {
            if unsafe { OpenClipboard(Some(owner.0)) }.is_ok() {
                return Ok(Session);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        Err("could not open the clipboard".into())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseClipboard();
        }
    }
}

pub(crate) struct Saved {
    formats: Vec<(u32, Vec<u8>)>,
}

fn restorable(fmt: u32) -> bool {
    // GDI handles and owner-drawn/private formats aren't plain memory.
    !matches!(fmt, 2 | 3 | 9 | 14 | 0x80 | 0x82 | 0x83 | 0x8E | 0x0200..=0x03FF)
}

unsafe fn read_global(h: HANDLE) -> Option<Vec<u8>> {
    let g = HGLOBAL(h.0);
    let size = GlobalSize(g);
    if size == 0 || size > MAX_SAVE_BYTES {
        return None;
    }
    let p = GlobalLock(g) as *const u8;
    if p.is_null() {
        return None;
    }
    let v = std::slice::from_raw_parts(p, size).to_vec();
    let _ = GlobalUnlock(g);
    Some(v)
}

unsafe fn put(fmt: u32, bytes: &[u8]) -> bool {
    let Ok(g) = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) else { return false };
    let p = GlobalLock(g) as *mut u8;
    if p.is_null() {
        let _ = GlobalFree(Some(g));
        return false;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
    let _ = GlobalUnlock(g);
    if SetClipboardData(fmt, Some(HANDLE(g.0))).is_err() {
        let _ = GlobalFree(Some(g));
        return false;
    }
    true
}

/// Flags that keep an entry out of Win+V history and cloud clipboard.
unsafe fn put_privacy_flags() {
    let zero = 0u32.to_le_bytes();
    let exclude = RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
    let history = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
    let cloud = RegisterClipboardFormatW(w!("CanUploadToCloudClipboard"));
    if exclude != 0 {
        put(exclude, &zero);
    }
    if history != 0 {
        put(history, &zero);
    }
    if cloud != 0 {
        put(cloud, &zero);
    }
}

fn utf16z(text: &str) -> Vec<u8> {
    text.encode_utf16().chain(std::iter::once(0)).flat_map(|u| u.to_le_bytes()).collect()
}

/// Snapshot every restorable clipboard format. None if the clipboard is too
/// large or unreadable (callers then avoid touching the clipboard).
pub(crate) fn save(owner: &Owner) -> Result<Option<Saved>, String> {
    let _s = Session::open(owner)?;
    let mut formats = Vec::new();
    let mut total = 0usize;
    let mut fmt = 0u32;
    unsafe {
        loop {
            fmt = EnumClipboardFormats(fmt);
            if fmt == 0 {
                break;
            }
            if !restorable(fmt) {
                continue;
            }
            if let Ok(h) = GetClipboardData(fmt) {
                if h.is_invalid() {
                    continue;
                }
                if let Some(bytes) = read_global(h) {
                    total += bytes.len();
                    if total > MAX_SAVE_BYTES {
                        return Ok(None);
                    }
                    formats.push((fmt, bytes));
                }
            }
        }
    }
    Ok(Some(Saved { formats }))
}

/// Put `text` on the clipboard. Returns the clipboard sequence number after
/// the change so the caller can tell whether someone else changed it since.
pub(crate) fn set_text_private(owner: &Owner, text: &str) -> Result<u32, String> {
    let _s = Session::open(owner)?;
    unsafe {
        EmptyClipboard().map_err(|e| e.to_string())?;
        if !put(CF_UNICODETEXT, &utf16z(text)) {
            return Err("SetClipboardData failed".into());
        }
        put_privacy_flags();
    }
    drop(_s);
    Ok(unsafe { GetClipboardSequenceNumber() })
}

pub(crate) fn restore(owner: &Owner, saved: &Saved, expected_seq: u32) -> Result<(), String> {
    if unsafe { GetClipboardSequenceNumber() } != expected_seq {
        // The user (or an app) copied something new meanwhile; keep it.
        return Ok(());
    }
    let _s = Session::open(owner)?;
    unsafe {
        EmptyClipboard().map_err(|e| e.to_string())?;
        for (fmt, bytes) in &saved.formats {
            put(*fmt, bytes);
        }
        if !saved.formats.is_empty() {
            // Restoring shouldn't create a duplicate Win+V history entry.
            put_privacy_flags();
        }
    }
    Ok(())
}

/// Copy text to the clipboard for the user (visible in clipboard history).
pub fn copy_text(text: &str) -> Result<(), String> {
    let owner = Owner::new()?;
    let _s = Session::open(&owner)?;
    unsafe {
        EmptyClipboard().map_err(|e| e.to_string())?;
        if put(CF_UNICODETEXT, &utf16z(text)) {
            Ok(())
        } else {
            Err("SetClipboardData failed".into())
        }
    }
}

/// Current clipboard text, if any (used by tests and the insertion check).
pub fn read_text() -> Option<String> {
    let owner = Owner::new().ok()?;
    let _s = Session::open(&owner).ok()?;
    unsafe {
        let h = GetClipboardData(CF_UNICODETEXT).ok()?;
        let bytes = read_global(h)?;
        let units: Vec<u16> = bytes.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).take_while(|u| *u != 0).collect();
        Some(String::from_utf16_lossy(&units))
    }
}

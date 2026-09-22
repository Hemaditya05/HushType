//! Integration checks against the real Windows APIs (no UI interaction).
#![cfg(windows)]

use hushtype_platform::*;

#[test]
fn autostart_roundtrip() {
    let was = autostart_enabled();
    set_autostart(true, r"C:\Program Files\HushType\hushtype.exe").unwrap();
    assert!(autostart_enabled());
    set_autostart(false, "").unwrap();
    assert!(!autostart_enabled());
    if was {
        // Leave the machine as we found it.
        set_autostart(true, &std::env::current_exe().unwrap().to_string_lossy()).unwrap();
    }
}

#[test]
fn clipboard_copy_and_read() {
    let before = read_text();
    copy_text("hushtype clipboard test ✓").unwrap();
    assert_eq!(read_text().as_deref(), Some("hushtype clipboard test ✓"));
    if let Some(b) = before {
        copy_text(&b).unwrap();
    }
}

#[test]
fn system_queries() {
    let (ws, private) = process_memory();
    assert!(ws > 0 && private > 0);
    let _ = gpus();
    let _ = is_elevated();
    let _ = foreground_app();
}

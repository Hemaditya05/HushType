//! Inserting text into the focused application.
//!
//! Strategy:
//! 1. Wait for the user to release the hotkey's modifier keys (a held Ctrl
//!    would turn typed letters into shortcuts).
//! 2. Default: paste through the clipboard (Ctrl+V, Shift+Insert in
//!    terminals) and put the user's previous clipboard contents back — all
//!    formats — afterwards. Atomic and fast in every kind of text field.
//! 3. Terminals (and the "Always typing" setting): synthesize Unicode key
//!    events (SendInput/KEYEVENTF_UNICODE), which never touch the clipboard.
//!    Falls back to pasting if typing fails.
//! The text is never executed: line breaks become Shift+Enter in chat apps
//! and spaces in terminals.

use std::time::{Duration, Instant};

use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_CONTROL, VK_INSERT, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL,
    VK_RETURN, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use super::clipboard;
use crate::{InsertError, InsertMethod, InsertOptions, InsertOutcome, NewlineMode};

/// Above this many characters, pasting is faster and more reliable than typing.
const TYPE_LIMIT: usize = 200;

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn unicode(unit: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: if up { KEYEVENTF_UNICODE | KEYEVENTF_KEYUP } else { KEYEVENTF_UNICODE },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> Result<(), InsertError> {
    if inputs.is_empty() {
        return Ok(());
    }
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(InsertError::Input(format!("SendInput sent {sent}/{} events", inputs.len())));
    }
    Ok(())
}

fn down(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0 }
}

const MODIFIERS: [VIRTUAL_KEY; 10] =
    [VK_LCONTROL, VK_RCONTROL, VK_LSHIFT, VK_RSHIFT, VK_LMENU, VK_RMENU, VK_LWIN, VK_RWIN, VK_CONTROL, VK_SHIFT];

/// Wait (bounded) until no modifier keys are physically held.
fn wait_modifiers_released() {
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_millis(1500) {
        if !MODIFIERS.iter().any(|vk| down(*vk)) && !down(VK_MENU) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    // Still held: release them logically so typing isn't read as shortcuts.
    let ups: Vec<INPUT> = MODIFIERS.iter().filter(|vk| down(**vk)).map(|vk| key(*vk, true)).collect();
    log::warn!("modifier keys still held after 1.5 s; releasing {} synthetically", ups.len());
    let _ = send(&ups);
}

fn normalize(text: &str, newline: NewlineMode) -> String {
    let t = text.replace("\r\n", "\n");
    match newline {
        NewlineMode::Space => t.split('\n').map(str::trim).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "),
        _ => t,
    }
}

fn type_text(text: &str, newline: NewlineMode) -> Result<(), InsertError> {
    let mut batch: Vec<INPUT> = Vec::with_capacity(64);
    let flush = |batch: &mut Vec<INPUT>| -> Result<(), InsertError> {
        send(batch)?;
        batch.clear();
        // Give the target's message loop a moment; Electron/Chromium apps can
        // reorder very large bursts.
        std::thread::sleep(Duration::from_millis(4));
        Ok(())
    };
    for ch in text.chars() {
        match ch {
            '\n' => {
                flush(&mut batch)?;
                let seq: Vec<INPUT> = match newline {
                    NewlineMode::ShiftEnter => {
                        vec![key(VK_SHIFT, false), key(VK_RETURN, false), key(VK_RETURN, true), key(VK_SHIFT, true)]
                    }
                    _ => vec![key(VK_RETURN, false), key(VK_RETURN, true)],
                };
                send(&seq)?;
                std::thread::sleep(Duration::from_millis(15));
            }
            '\t' => batch.extend([key(VK_TAB, false), key(VK_TAB, true)]),
            '\r' => {}
            c => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    batch.push(unicode(*unit, false));
                    batch.push(unicode(*unit, true));
                }
            }
        }
        if batch.len() >= 48 {
            flush(&mut batch)?;
        }
    }
    flush(&mut batch)
}

fn paste_text(text: &str, opts: &InsertOptions) -> Result<(), InsertError> {
    let owner = clipboard::Owner::new().map_err(InsertError::Clipboard)?;
    let saved = if opts.restore_clipboard { clipboard::save(&owner).map_err(InsertError::Clipboard)? } else { None };
    let seq = clipboard::set_text_private(&owner, text).map_err(InsertError::Clipboard)?;
    let keys = if opts.terminal_paste {
        vec![key(VK_SHIFT, false), key(VK_INSERT, false), key(VK_INSERT, true), key(VK_SHIFT, true)]
    } else {
        vec![key(VK_CONTROL, false), key(VIRTUAL_KEY(0x56), false), key(VIRTUAL_KEY(0x56), true), key(VK_CONTROL, true)]
    };
    let res = send(&keys);
    if let Some(saved) = saved {
        // The target reads the clipboard asynchronously after the keystroke.
        std::thread::sleep(Duration::from_millis(400));
        if let Err(e) = clipboard::restore(&owner, &saved, seq) {
            log::warn!("clipboard restore failed: {e}");
        }
    }
    res
}

/// Insert `text` into whatever currently has keyboard focus.
pub fn insert_text(text: &str, opts: &InsertOptions) -> Result<InsertOutcome, InsertError> {
    let text = normalize(text, opts.newline);
    if text.is_empty() {
        return Ok(InsertOutcome::Typed);
    }
    wait_modifiers_released();
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_invalid() {
        super::clipboard::copy_text(&text).map_err(InsertError::Clipboard)?;
        return Ok(InsertOutcome::Copied);
    }
    if let Some(app) = super::foreground::foreground_app() {
        if app.elevated && !super::system::is_elevated() {
            let _ = super::clipboard::copy_text(&text);
            return Err(InsertError::TargetElevated);
        }
    }
    // Auto: paste (fast, atomic, clipboard restored). Several modern text
    // controls (Win11 Notepad's WinUI editor among them) scramble bursts of
    // injected Unicode keystrokes. Terminals get typed input instead: they
    // handle it reliably and it avoids multi-line paste warnings.
    let use_paste = match opts.method {
        InsertMethod::Paste => true,
        InsertMethod::Type => false,
        InsertMethod::Auto => !opts.terminal_paste || text.chars().count() > TYPE_LIMIT,
    };
    if !use_paste {
        match type_text(&text, opts.newline) {
            Ok(()) => return Ok(InsertOutcome::Typed),
            Err(e) => log::warn!("typing failed ({e:?}), falling back to paste"),
        }
    }
    paste_text(&text, opts)?;
    Ok(InsertOutcome::Pasted)
}

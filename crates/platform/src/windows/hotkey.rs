//! Global hotkey via RegisterHotKey on a dedicated message-loop thread.
//!
//! RegisterHotKey consumes the key combination (it never reaches the focused
//! app) and reports conflicts with other applications. Windows has no
//! "released" notification for hotkeys, so while — and only while — the
//! combination is held, a 15 ms thread timer polls the key state to detect the
//! release for push-to-talk. When idle the thread sleeps in GetMessage.

use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::{GetLastError, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT,
    MOD_WIN, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, KillTimer, PeekMessageW, PostThreadMessageW, SetTimer, TranslateMessage, MSG,
    PM_NOREMOVE, WM_APP, WM_HOTKEY, WM_TIMER,
};

use crate::{Hotkey, HotkeyEvent};

const ID_MAIN: i32 = 1;
const ID_ESCAPE: i32 = 2;
const WM_REGISTER: u32 = WM_APP + 1;
const WM_ESCAPE: u32 = WM_APP + 2;
const WM_QUIT_LOOP: u32 = WM_APP + 3;
const WM_SUSPEND: u32 = WM_APP + 4;

type Callback = Box<dyn Fn(HotkeyEvent) + Send + Sync>;

struct Pending {
    hotkey: Option<Hotkey>,
    reply: Option<Sender<Result<(), String>>>,
}

pub struct HotkeyManager {
    thread_id: u32,
    pending: Arc<Mutex<Pending>>,
}

fn is_down(vk: u16) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

impl HotkeyManager {
    pub fn start(cb: impl Fn(HotkeyEvent) + Send + Sync + 'static) -> HotkeyManager {
        let pending = Arc::new(Mutex::new(Pending { hotkey: None, reply: None }));
        let p = pending.clone();
        let (tx, rx) = channel();
        let cb: Callback = Box::new(cb);
        std::thread::Builder::new()
            .name("hotkey".into())
            .spawn(move || unsafe {
                let mut msg = MSG::default();
                // Create the thread's message queue before announcing it.
                let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                let _ = tx.send(GetCurrentThreadId());
                run_loop(p, cb);
            })
            .expect("spawn hotkey thread");
        let thread_id = rx.recv().expect("hotkey thread id");
        HotkeyManager { thread_id, pending }
    }

    /// Register (or replace) the global shortcut. Fails if another app owns it.
    pub fn register(&self, hk: &Hotkey) -> Result<(), String> {
        let (tx, rx) = channel();
        {
            let mut p = self.pending.lock().unwrap();
            p.hotkey = Some(hk.clone());
            p.reply = Some(tx);
        }
        unsafe {
            PostThreadMessageW(self.thread_id, WM_REGISTER, WPARAM(0), LPARAM(0)).map_err(|e| e.to_string())?;
        }
        rx.recv_timeout(std::time::Duration::from_secs(3)).map_err(|_| "hotkey thread not responding".to_string())?
    }

    /// Temporarily release the shortcut (while the user records a new one).
    pub fn set_suspended(&self, suspended: bool) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_SUSPEND, WPARAM(suspended as usize), LPARAM(0));
        }
    }

    /// Capture Escape (to cancel) only while a recording is active.
    pub fn set_escape_enabled(&self, enabled: bool) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_ESCAPE, WPARAM(enabled as usize), LPARAM(0));
        }
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT_LOOP, WPARAM(0), LPARAM(0));
        }
    }
}

fn modifiers(hk: &Hotkey) -> HOT_KEY_MODIFIERS {
    let mut m = MOD_NOREPEAT;
    if hk.ctrl {
        m |= MOD_CONTROL;
    }
    if hk.alt {
        m |= MOD_ALT;
    }
    if hk.shift {
        m |= MOD_SHIFT;
    }
    if hk.win {
        m |= MOD_WIN;
    }
    m
}

fn still_held(hk: &Hotkey) -> bool {
    if !is_down(hk.vk as u16) {
        return false;
    }
    (!hk.ctrl || is_down(VK_CONTROL.0))
        && (!hk.alt || is_down(VK_MENU.0))
        && (!hk.shift || is_down(VK_SHIFT.0))
        && (!hk.win || is_down(VK_LWIN.0) || is_down(VK_RWIN.0))
}

unsafe fn run_loop(pending: Arc<Mutex<Pending>>, cb: Callback) {
    let mut current: Option<Hotkey> = None;
    let mut poll_timer: usize = 0;
    let mut escape_on = false;
    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        match msg.message {
            WM_HOTKEY if msg.wParam.0 as i32 == ID_MAIN => {
                cb(HotkeyEvent::Pressed);
                if poll_timer == 0 {
                    poll_timer = SetTimer(None, 0, 15, None);
                }
            }
            WM_HOTKEY if msg.wParam.0 as i32 == ID_ESCAPE => cb(HotkeyEvent::Cancel),
            WM_TIMER if poll_timer != 0 && msg.wParam.0 == poll_timer => {
                let held = current.as_ref().map(still_held).unwrap_or(false);
                if !held {
                    let _ = KillTimer(None, poll_timer);
                    poll_timer = 0;
                    cb(HotkeyEvent::Released);
                }
            }
            WM_REGISTER => {
                let (hk, reply) = {
                    let mut p = pending.lock().unwrap();
                    (p.hotkey.take(), p.reply.take())
                };
                let Some(hk) = hk else { continue };
                let _ = UnregisterHotKey(None, ID_MAIN);
                let res = match RegisterHotKey(None, ID_MAIN, modifiers(&hk), hk.vk) {
                    Ok(()) => {
                        log::info!("global shortcut registered: {}", hk.label());
                        current = Some(hk);
                        Ok(())
                    }
                    Err(_) => {
                        let code = GetLastError().0;
                        let msg = if code == 1409 {
                            format!("{} is already used by another application. Choose a different shortcut.", hk.label())
                        } else {
                            format!("{} could not be registered (error {code}).", hk.label())
                        };
                        // Keep the previous shortcut working.
                        if let Some(prev) = &current {
                            let _ = RegisterHotKey(None, ID_MAIN, modifiers(prev), prev.vk);
                        }
                        Err(msg)
                    }
                };
                if let Some(r) = reply {
                    let _ = r.send(res);
                }
            }
            WM_ESCAPE => {
                let want = msg.wParam.0 != 0;
                if want && !escape_on {
                    escape_on = RegisterHotKey(None, ID_ESCAPE, MOD_NOREPEAT, VK_ESCAPE.0 as u32).is_ok();
                } else if !want && escape_on {
                    let _ = UnregisterHotKey(None, ID_ESCAPE);
                    escape_on = false;
                }
            }
            WM_SUSPEND => {
                if msg.wParam.0 != 0 {
                    let _ = UnregisterHotKey(None, ID_MAIN);
                } else if let Some(hk) = &current {
                    let _ = RegisterHotKey(None, ID_MAIN, modifiers(hk), hk.vk);
                }
            }
            WM_QUIT_LOOP => break,
            _ => {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
    let _ = UnregisterHotKey(None, ID_MAIN);
    let _ = UnregisterHotKey(None, ID_ESCAPE);
}

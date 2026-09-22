use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::state::{self, AppState};

pub struct TrayItems {
    start: MenuItem<Wry>,
    stop: MenuItem<Wry>,
    last_phase: &'static str,
}

const ICON_IDLE: &[u8] = include_bytes!("../icons/tray-idle.png");
const ICON_REC: &[u8] = include_bytes!("../icons/tray-recording.png");
const ICON_BUSY: &[u8] = include_bytes!("../icons/tray-processing.png");

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let start = MenuItem::with_id(app, "start", "Start Listening", true, None::<&str>)?;
    let stop = MenuItem::with_id(app, "stop", "Stop Listening", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "dashboard", "Open HushType", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let mic = MenuItem::with_id(app, "microphone", "Microphone", true, None::<&str>)?;
    let model = MenuItem::with_id(app, "models", "Model", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "History", true, None::<&str>)?;
    let about = MenuItem::with_id(app, "about", "About", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit HushType", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &start,
            &stop,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &settings,
            &mic,
            &model,
            &history,
            &about,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let hotkey = app.state::<AppState>().settings().hotkey;
    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(ICON_IDLE)?)
        .tooltip(format!("HushType — ready ({hotkey})"))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "start" => state::start(app, false),
            "stop" => state::stop(app),
            "quit" => {
                state::cancel(app);
                app.exit(0);
            }
            route => crate::open_window(app, route),
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                crate::open_window(tray.app_handle(), "dashboard");
            }
        })
        .build(app)?;
    *app.state::<AppState>().tray.lock().unwrap() = Some(TrayItems { start, stop, last_phase: "idle" });
    Ok(())
}

/// Reflect the recording state in the tray icon, tooltip and menu.
pub fn set_phase(app: &AppHandle, phase: &'static str, hotkey: &str) {
    let st = app.state::<AppState>();
    let mut guard = st.tray.lock().unwrap();
    let Some(items) = guard.as_mut() else { return };
    let (icon, tip) = match phase {
        "recording" => (ICON_REC, "HushType — listening… (Esc to cancel)".to_string()),
        "processing" => (ICON_BUSY, "HushType — processing…".to_string()),
        _ => (ICON_IDLE, format!("HushType — ready ({hotkey})")),
    };
    if let Some(tray) = app.tray_by_id("main") {
        if items.last_phase != phase {
            if let Ok(img) = Image::from_bytes(icon) {
                let _ = tray.set_icon(Some(img));
            }
        }
        let _ = tray.set_tooltip(Some(tip));
    }
    let _ = items.start.set_enabled(phase == "idle");
    let _ = items.stop.set_enabled(phase == "recording");
    items.last_phase = phase;
}

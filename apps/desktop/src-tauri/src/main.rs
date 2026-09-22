// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod context;
mod history;
mod logging;
mod paths;
mod settings;
mod state;
mod tray;

use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use hushtype_platform::{self as platform, HotkeyManager, Indicator, IndicatorState};

use crate::paths::Paths;
use crate::settings::Settings;
use crate::state::AppState;

const ROUTES: &[&str] = &["dashboard", "welcome", "settings", "microphone", "models", "dictionary", "history", "shortcut", "about"];

/// Show the settings window on `route`, creating it if needed. The window is
/// destroyed (not hidden) when closed so its WebView memory is released.
pub fn open_window(app: &AppHandle, route: &str) {
    let route = if ROUTES.contains(&route) { route } else { "dashboard" };
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.emit("navigate", route);
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let url = WebviewUrl::App(format!("index.html#/{route}").into());
    let res = WebviewWindowBuilder::new(app, "main", url)
        .title("HushType")
        .inner_size(1000.0, 700.0)
        .min_inner_size(780.0, 540.0)
        .center()
        .focused(true)
        .build();
    if let Err(e) = res {
        log::error!("could not open window: {e}");
    }
}

fn main() {
    let autostart = std::env::args().any(|a| a == "--autostart");
    let paths = Paths::new();
    logging::init(paths.logs.clone());
    log::info!("HushType {} starting (autostart: {autostart})", env!("CARGO_PKG_VERSION"));
    let settings = Settings::load(&paths);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| open_window(app, "dashboard")))
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::save_settings,
            commands::check_hotkey,
            commands::suspend_hotkey,
            commands::toggle_dictation,
            commands::list_microphones,
            commands::start_mic_test,
            commands::stop_mic_test,
            commands::list_models,
            commands::download_model,
            commands::cancel_download,
            commands::delete_model,
            commands::load_model,
            commands::unload_model,
            commands::get_dictionary,
            commands::save_dictionary,
            commands::reset_dictionary,
            commands::preview_text,
            commands::get_history,
            commands::delete_history,
            commands::clear_history,
            commands::copy_text,
            commands::open_external,
            commands::diagnostics,
            commands::finish_onboarding,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let hk_handle = handle.clone();
            let hotkeys = HotkeyManager::start(move |ev| state::on_hotkey(&hk_handle, ev));
            let indicator = Indicator::start();
            let st = AppState::new(paths, settings, hotkeys, indicator);
            let s = st.settings();
            let status_handle = handle.clone();
            st.engine.on_status(move |m| {
                let _ = status_handle.emit("model-status", m.clone());
            });
            app.manage(st);
            let st = app.state::<AppState>();

            tray::setup(&handle)?;

            match platform::parse_hotkey(&s.hotkey).and_then(|hk| st.hotkeys.register(&hk)) {
                Ok(()) => {}
                Err(e) => {
                    log::warn!("hotkey registration failed: {e}");
                    st.indicator.set(IndicatorState::Error(e.clone()));
                    *st.hotkey_error.lock().unwrap() = Some(e);
                }
            }
            // Keep the startup entry pointing at this executable (e.g. after an update).
            if s.launch_at_startup {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = platform::set_autostart(true, &exe.to_string_lossy());
                }
            }
            if s.load_model_at_startup {
                st.engine.preload();
            }
            if !s.onboarded {
                open_window(&handle, "welcome");
            } else if st.hotkey_error.lock().unwrap().is_some() {
                open_window(&handle, "shortcut");
            } else if !autostart && !s.start_minimized {
                open_window(&handle, "dashboard");
            }
            state::emit_status(&handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::Destroyed = event {
                let app = window.app_handle().clone();
                commands::stop_mic_test_inner(&app.state::<AppState>());
                // Give WebView2 a moment to exit, then hand pages back to Windows.
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_secs(3));
                    platform::trim_memory();
                });
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to start HushType")
        .run(|_app, event| {
            // Closing the last window keeps HushType running in the tray.
            if let RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
            }
        });
}

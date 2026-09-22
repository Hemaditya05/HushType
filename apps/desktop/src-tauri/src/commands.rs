//! Commands invoked by the settings UI. All run locally; none touch the network
//! except `download_model`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use hushtype_engine::audio::{list_input_devices, Capture, InputDevice};
use hushtype_engine::{models, sys, ModelStatus};
use hushtype_platform as platform;
use hushtype_text::{default_terms, process, AppContext, Dictionary, ProcessOptions, Term};

use crate::history::Page;
use crate::settings::Settings;
use crate::state::{self, emit_status, AppState, StatusPayload};

type Res<T> = Result<T, String>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    settings: Settings,
    status: StatusPayload,
    version: &'static str,
}

#[tauri::command]
pub fn get_state(st: State<AppState>) -> Snapshot {
    Snapshot { settings: st.settings(), status: st.status(), version: env!("CARGO_PKG_VERSION") }
}

#[derive(Serialize)]
pub struct SaveResult {
    warnings: Vec<String>,
}

#[tauri::command]
pub fn save_settings(app: AppHandle, st: State<AppState>, settings: Settings) -> Res<SaveResult> {
    let new = settings.sanitized();
    let old = st.settings();
    let mut warnings = Vec::new();

    if new.hotkey != old.hotkey || st.hotkey_error.lock().unwrap().is_some() {
        let hk = platform::parse_hotkey(&new.hotkey)?;
        st.hotkeys.register(&hk)?;
        *st.hotkey_error.lock().unwrap() = None;
        if let Some(w) = hk.conflict_warning() {
            warnings.push(w);
        }
    }
    if new.launch_at_startup != old.launch_at_startup || new.launch_at_startup != platform::autostart_enabled() {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        platform::set_autostart(new.launch_at_startup, &exe.to_string_lossy())
            .map_err(|e| format!("Could not change the startup setting: {e}"))?;
    }
    if new.model != old.model {
        st.engine.set_model(&new.model);
        if let Some(m) = models::find(&new.model) {
            if m.english_only && new.language != "en" {
                warnings.push(format!("{} only understands English. Pick a multilingual model for other languages.", m.name));
            }
        }
    }
    if new.unload_after_min != old.unload_after_min {
        st.engine.set_idle_unload(new.idle_unload());
    }
    st.indicator.set_top(new.indicator_top);
    new.save(&st.paths)?;
    *st.settings.write().unwrap() = new.clone();
    let _ = app.emit("settings", new);
    emit_status(&app);
    Ok(SaveResult { warnings })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyCheck {
    label: String,
    warning: Option<String>,
}

#[tauri::command]
pub fn check_hotkey(label: String) -> Res<HotkeyCheck> {
    let hk = platform::parse_hotkey(&label)?;
    Ok(HotkeyCheck { label: hk.label(), warning: hk.conflict_warning() })
}

#[tauri::command]
pub fn suspend_hotkey(st: State<AppState>, suspended: bool) {
    st.hotkeys.set_suspended(suspended);
}

#[tauri::command]
pub fn toggle_dictation(app: AppHandle) {
    state::toggle(&app);
}

// ---------------------------------------------------------------------------
// Microphone
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_microphones() -> Res<Vec<InputDevice>> {
    list_input_devices().map_err(|e| e.to_string())
}

pub fn stop_mic_test_inner(st: &AppState) {
    if let Some(stop) = st.mic_test_stop.lock().unwrap().take() {
        stop.store(true, Ordering::SeqCst);
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Level {
    db: f32,
    device: String,
    error: Option<String>,
}

/// Live input level for the microphone page (stops by itself after 60 s).
#[tauri::command]
pub fn start_mic_test(app: AppHandle, st: State<AppState>, device: String) -> Res<()> {
    if st.phase.lock().unwrap().name() != "idle" {
        return Err("Busy dictating; try again in a moment.".into());
    }
    stop_mic_test_inner(&st);
    let stop = Arc::new(AtomicBool::new(false));
    *st.mic_test_stop.lock().unwrap() = Some(stop.clone());
    std::thread::Builder::new()
        .name("mic-test".into())
        .spawn(move || {
            let dev = (!device.is_empty()).then_some(device.as_str());
            let mut cap = match Capture::open(dev) {
                Ok(c) => c,
                Err(e) => {
                    let _ = app.emit("mic-level", Level { db: -100.0, device: String::new(), error: Some(e.to_string()) });
                    return;
                }
            };
            let name = cap.device_name.clone();
            let t0 = Instant::now();
            let mut buf = Vec::with_capacity(4096);
            let mut last = Instant::now();
            while !stop.load(Ordering::SeqCst) && t0.elapsed() < Duration::from_secs(60) {
                if let Err(e) = cap.read(&mut buf, Duration::from_millis(40)) {
                    let _ = app.emit("mic-level", Level { db: -100.0, device: name.clone(), error: Some(e.to_string()) });
                    return;
                }
                if last.elapsed() >= Duration::from_millis(70) && !buf.is_empty() {
                    let rms = (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt().max(1e-7);
                    let _ = app.emit("mic-level", Level { db: 20.0 * rms.log10(), device: name.clone(), error: None });
                    buf.clear();
                    last = Instant::now();
                }
            }
            let _ = app.emit("mic-level", Level { db: -100.0, device: name, error: Some("stopped".into()) });
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn stop_mic_test(st: State<AppState>) {
    stop_mic_test_inner(&st);
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelEntry {
    #[serde(flatten)]
    info: models::ModelInfo,
    installed: bool,
    selected: bool,
    recommended: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    cpu: sys::CpuInfo,
    threads: usize,
    ram_total_mb: u64,
    ram_available_mb: u64,
    gpus: Vec<platform::GpuInfo>,
    gpu_backends: Vec<&'static str>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsPayload {
    models: Vec<ModelEntry>,
    hardware: Hardware,
    status: ModelStatus,
    models_dir: String,
    downloading: bool,
}

#[tauri::command]
pub fn list_models(st: State<AppState>) -> ModelsPayload {
    let s = st.settings();
    let (total, avail) = sys::memory().unwrap_or((0, 0));
    let cpu = sys::cpu_info();
    let backends = sys::gpu_backends();
    let rec = models::recommended(Some(total), cpu.logical_cores, !backends.is_empty());
    ModelsPayload {
        models: models::CATALOG
            .iter()
            .map(|m| ModelEntry {
                installed: models::is_installed(&st.paths.models, m.id),
                selected: m.id == s.model,
                recommended: m.id == rec,
                info: m.clone(),
            })
            .collect(),
        hardware: Hardware {
            threads: sys::inference_threads(),
            cpu,
            ram_total_mb: total / (1 << 20),
            ram_available_mb: avail / (1 << 20),
            gpus: platform::gpus(),
            gpu_backends: backends,
        },
        status: st.engine.status(),
        models_dir: st.paths.models.to_string_lossy().into_owned(),
        downloading: st.download_cancel.lock().unwrap().is_some(),
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DownloadEvent {
    id: String,
    downloaded: u64,
    total: u64,
    bytes_per_sec: u64,
    done: bool,
    error: Option<String>,
}

#[tauri::command]
pub fn download_model(app: AppHandle, st: State<AppState>, id: String) -> Res<()> {
    let info = models::find(&id).ok_or("unknown model")?;
    let mut slot = st.download_cancel.lock().unwrap();
    if slot.is_some() {
        return Err("Another download is in progress.".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *slot = Some(cancel.clone());
    drop(slot);
    let dir = st.paths.models.clone();
    let total = info.size_bytes;
    std::thread::Builder::new()
        .name("download".into())
        .spawn(move || {
            let app2 = app.clone();
            let id2 = id.clone();
            let res = models::download(&dir, &id, &cancel, move |p| {
                let _ = app2.emit(
                    "model-download",
                    DownloadEvent { id: id2.clone(), downloaded: p.downloaded, total: p.total, bytes_per_sec: p.bytes_per_sec, done: false, error: None },
                );
            });
            let st = app.state::<AppState>();
            *st.download_cancel.lock().unwrap() = None;
            let error = res.err().map(|e| e.to_string());
            if let Some(e) = &error {
                log::warn!("model download failed: {e}");
            }
            let _ = app.emit(
                "model-download",
                DownloadEvent { id, downloaded: if error.is_none() { total } else { 0 }, total, bytes_per_sec: 0, done: true, error },
            );
            emit_status(&app);
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cancel_download(st: State<AppState>) {
    if let Some(c) = st.download_cancel.lock().unwrap().as_ref() {
        c.store(true, Ordering::SeqCst);
    }
}

#[tauri::command]
pub fn delete_model(app: AppHandle, st: State<AppState>, id: String) -> Res<()> {
    if st.settings().model == id {
        st.engine.unload();
        std::thread::sleep(Duration::from_millis(300));
    }
    models::delete(&st.paths.models, &id).map_err(|e| format!("Could not delete the model: {e}"))?;
    emit_status(&app);
    Ok(())
}

#[tauri::command]
pub fn load_model(st: State<AppState>) {
    st.engine.preload();
}

#[tauri::command]
pub fn unload_model(st: State<AppState>) {
    st.engine.unload();
}

// ---------------------------------------------------------------------------
// Dictionary
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_dictionary(st: State<AppState>) -> Vec<Term> {
    state::load_terms(&st.paths)
}

#[tauri::command]
pub fn save_dictionary(st: State<AppState>, terms: Vec<Term>) -> Res<()> {
    let mut clean: Vec<Term> = Vec::new();
    for t in terms {
        let term = t.term.trim().to_string();
        if term.is_empty() || term.len() > 100 || clean.iter().any(|c| c.term.eq_ignore_ascii_case(&term)) {
            continue;
        }
        let aliases = t.aliases.into_iter().map(|a| a.trim().to_string()).filter(|a| !a.is_empty() && a.len() <= 100).take(20).collect();
        clean.push(Term { term, aliases });
    }
    clean.truncate(2000);
    state::save_terms(&st.paths, &clean)?;
    *st.dict.write().unwrap() = Arc::new(Dictionary::new(&clean));
    Ok(())
}

#[tauri::command]
pub fn reset_dictionary(st: State<AppState>) -> Res<Vec<Term>> {
    let t = default_terms();
    state::save_terms(&st.paths, &t)?;
    *st.dict.write().unwrap() = Arc::new(Dictionary::new(&t));
    Ok(t)
}

/// Run the cleanup pipeline on sample text (dictionary/settings preview).
#[tauri::command]
pub fn preview_text(st: State<AppState>, raw: String, context: AppContext) -> String {
    let s = st.settings();
    let opts = ProcessOptions {
        remove_fillers: s.remove_fillers,
        smart_punctuation: s.smart_punctuation,
        auto_capitalize: s.auto_capitalize,
        spoken_punctuation: s.spoken_punctuation,
        aggressive: s.aggressive_cleanup,
        language: if s.language == "auto" { "en".into() } else { s.language.clone() },
        context,
    };
    process(&raw, &opts, &st.dict.read().unwrap())
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_history(st: State<AppState>, offset: usize, limit: usize, query: String) -> Page {
    st.history.page(offset, limit.min(200), &query)
}

#[tauri::command]
pub fn delete_history(st: State<AppState>, id: u64) {
    if let Some(e) = st.history.delete(id) {
        if let Some(f) = e.audio_file {
            let _ = std::fs::remove_file(f);
        }
    }
}

#[tauri::command]
pub fn clear_history(st: State<AppState>) {
    for f in st.history.clear() {
        let _ = std::fs::remove_file(f);
    }
    let _ = std::fs::remove_dir_all(&st.paths.recordings);
}

#[tauri::command]
pub fn copy_text(text: String) -> Res<()> {
    platform::copy_text(&text)
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn open_external(st: State<AppState>, target: String) -> Res<()> {
    let t = match target.as_str() {
        "models" => st.paths.models.to_string_lossy().into_owned(),
        "logs" => st.paths.logs.to_string_lossy().into_owned(),
        "data" => st.paths.data.to_string_lossy().into_owned(),
        "mic-privacy" => "ms-settings:privacy-microphone".into(),
        "sound-settings" => "ms-settings:sound".into(),
        other if other.starts_with("https://") => other.to_string(),
        _ => return Err("not allowed".into()),
    };
    platform::open_external(&t)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    working_set_mb: f64,
    private_mb: f64,
    whisper: String,
    paths: crate::paths::Paths,
    elevated: bool,
}

#[tauri::command]
pub fn diagnostics(st: State<AppState>) -> Diagnostics {
    let (ws, private) = platform::process_memory();
    Diagnostics {
        working_set_mb: ws as f64 / 1048576.0,
        private_mb: private as f64 / 1048576.0,
        whisper: hushtype_engine::transcriber::system_info(),
        paths: st.paths.clone(),
        elevated: platform::is_elevated(),
    }
}

#[tauri::command]
pub fn finish_onboarding(app: AppHandle, st: State<AppState>) -> Res<()> {
    let mut s = st.settings();
    s.onboarded = true;
    s.save(&st.paths)?;
    *st.settings.write().unwrap() = s;
    emit_status(&app);
    Ok(())
}

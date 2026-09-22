//! Application state and the dictation state machine.
//!
//!   Idle --hotkey--> Recording --release/tap/silence--> Processing --> Idle
//!
//! Hotkey callbacks arrive on the hotkey thread and dictation events on the
//! dictation thread; the phase mutex is never held across slow work.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use hushtype_engine::{models, Dictation, DictationEvent, DictationOptions, DictationResult, Engine};
use hushtype_platform::{self as platform, ForegroundApp, HotkeyEvent, HotkeyManager, Indicator, IndicatorState, InsertOptions, InsertOutcome, Sound};
use hushtype_text::{default_terms, process, AppContext, Dictionary, ProcessOptions, Term};

use crate::context;
use crate::history::{now_ms, Entry, History};
use crate::paths::{write_atomic, Paths};
use crate::settings::{HotkeyMode, Settings};

/// Taps shorter than this switch hybrid mode into hands-free recording.
const TAP_MS: u128 = 400;
/// In hands-free mode, give up if nothing is said for this long.
const HANDS_FREE_NO_SPEECH: Duration = Duration::from_secs(8);

pub enum Phase {
    Idle,
    Recording { d: Dictation, pressed_at: Instant, holding: bool },
    Processing { _d: Option<Dictation>, since: Instant },
}

impl Phase {
    pub fn name(&self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Recording { .. } => "recording",
            Phase::Processing { .. } => "processing",
        }
    }
}

struct Session {
    target: Option<ForegroundApp>,
}

pub struct AppState {
    pub paths: Paths,
    pub settings: RwLock<Settings>,
    pub engine: Arc<Engine>,
    pub dict: RwLock<Arc<Dictionary>>,
    pub history: History,
    pub hotkeys: HotkeyManager,
    pub indicator: Indicator,
    pub phase: Mutex<Phase>,
    session: Mutex<Option<Session>>,
    last_insert: Mutex<Option<(u32, Instant)>>,
    pub download_cancel: Mutex<Option<Arc<AtomicBool>>>,
    pub mic_test_stop: Mutex<Option<Arc<AtomicBool>>>,
    pub hotkey_error: Mutex<Option<String>>,
    pub tray: Mutex<Option<crate::tray::TrayItems>>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatusPayload {
    pub phase: &'static str,
    pub hotkey: String,
    pub hotkey_error: Option<String>,
    pub model: hushtype_engine::ModelStatus,
    pub model_id: String,
    pub model_installed: bool,
    pub cpu_supported: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ResultPayload {
    text: String,
    app: String,
    outcome: String,
    audio_ms: u64,
    final_latency_ms: u64,
}

impl AppState {
    pub fn new(paths: Paths, settings: Settings, hotkeys: HotkeyManager, indicator: Indicator) -> AppState {
        let engine = Arc::new(Engine::new(paths.models.clone(), &settings.model, settings.idle_unload()));
        let terms = load_terms(&paths);
        let history = History::new(paths.history());
        indicator.set_top(settings.indicator_top);
        AppState {
            dict: RwLock::new(Arc::new(Dictionary::new(&terms))),
            settings: RwLock::new(settings),
            engine,
            history,
            hotkeys,
            indicator,
            paths,
            phase: Mutex::new(Phase::Idle),
            session: Mutex::new(None),
            last_insert: Mutex::new(None),
            download_cancel: Mutex::new(None),
            mic_test_stop: Mutex::new(None),
            hotkey_error: Mutex::new(None),
            tray: Mutex::new(None),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap().clone()
    }

    pub fn status(&self) -> StatusPayload {
        let s = self.settings();
        StatusPayload {
            phase: self.phase.lock().unwrap().name(),
            hotkey: s.hotkey.clone(),
            hotkey_error: self.hotkey_error.lock().unwrap().clone(),
            model: self.engine.status(),
            model_installed: models::is_installed(&self.paths.models, &s.model),
            model_id: s.model,
            cpu_supported: hushtype_engine::sys::cpu_supported(),
        }
    }

    fn show(&self, st: IndicatorState) {
        let enabled = self.settings.read().unwrap().show_indicator;
        // Errors are always shown; everything else respects the setting.
        if enabled || matches!(st, IndicatorState::Error(_) | IndicatorState::Hidden) {
            self.indicator.set(st);
        }
    }

    fn sound(&self, s: Sound) {
        if self.settings.read().unwrap().play_sounds {
            platform::play(s);
        }
    }
}

pub fn load_terms(paths: &Paths) -> Vec<Term> {
    match std::fs::read_to_string(paths.dictionary()) {
        Ok(s) => serde_json::from_str(s.trim_start_matches('\u{feff}')).unwrap_or_else(|_| default_terms()),
        Err(_) => {
            let t = default_terms();
            let _ = save_terms(paths, &t);
            t
        }
    }
}

pub fn save_terms(paths: &Paths, terms: &[Term]) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(terms).map_err(|e| e.to_string())?;
    write_atomic(&paths.dictionary(), &json).map_err(|e| e.to_string())
}

pub fn emit_status(app: &AppHandle) {
    let st = app.state::<AppState>();
    let status = st.status();
    crate::tray::set_phase(app, status.phase, &status.hotkey);
    let _ = app.emit("status", status);
}

// ---------------------------------------------------------------------------
// Hotkey handling
// ---------------------------------------------------------------------------

pub fn on_hotkey(app: &AppHandle, ev: HotkeyEvent) {
    let st = app.state::<AppState>();
    let mode = st.settings.read().unwrap().hotkey_mode;
    log::info!("hotkey {ev:?} while {}", st.phase.lock().unwrap().name());
    match ev {
        HotkeyEvent::Pressed => {
            let action = {
                let mut phase = st.phase.lock().unwrap();
                match &mut *phase {
                    Phase::Idle => 1,
                    Phase::Recording { holding: false, .. } => 2,
                    Phase::Recording { .. } => 0,
                    Phase::Processing { since, .. } => {
                        if since.elapsed() > Duration::from_secs(45) {
                            log::warn!("processing stuck for 45 s; resetting");
                            *phase = Phase::Idle;
                            1
                        } else {
                            3
                        }
                    }
                }
            };
            match action {
                1 => start(app, true),
                2 => stop(app),
                3 => st.show(IndicatorState::Info("Still processing the last dictation…".into())),
                _ => {}
            }
        }
        HotkeyEvent::Released => {
            let mut do_stop = false;
            {
                let mut phase = st.phase.lock().unwrap();
                if let Phase::Recording { d, pressed_at, holding } = &mut *phase {
                    if *holding {
                        let tap = pressed_at.elapsed().as_millis() < TAP_MS;
                        match mode {
                            HotkeyMode::Hold => do_stop = true,
                            HotkeyMode::Toggle => *holding = false,
                            HotkeyMode::Hybrid if tap => {
                                // Hands-free: keep listening until a pause.
                                *holding = false;
                                let silence = st.settings.read().unwrap().silence_timeout_ms;
                                d.set_auto_stop(Some(Duration::from_millis(silence as u64)), Some(HANDS_FREE_NO_SPEECH));
                            }
                            HotkeyMode::Hybrid => do_stop = true,
                        }
                    }
                }
            }
            if do_stop {
                stop(app);
            }
        }
        HotkeyEvent::Cancel => cancel(app),
    }
}

// ---------------------------------------------------------------------------
// Recording lifecycle
// ---------------------------------------------------------------------------

/// Start a dictation. `holding` = started by pressing (not yet released) the
/// hotkey; tray/UI starts are hands-free.
pub fn start(app: &AppHandle, holding: bool) {
    let st = app.state::<AppState>();
    let s = st.settings();
    if !hushtype_engine::sys::cpu_supported() {
        st.show(IndicatorState::Error(hushtype_engine::EngineError::UnsupportedCpu.to_string()));
        return;
    }
    if !models::is_installed(&st.paths.models, &s.model) {
        st.sound(Sound::Error);
        st.show(IndicatorState::Error("No speech model installed yet. Opening Models…".into()));
        crate::open_window(app, "models");
        return;
    }
    crate::commands::stop_mic_test_inner(&st);

    let target = platform::foreground_app();
    *st.session.lock().unwrap() = Some(Session { target });
    st.sound(Sound::Start);
    st.show(IndicatorState::Listening(None));
    st.hotkeys.set_escape_enabled(true);

    let hands_free = !holding || s.hotkey_mode == HotkeyMode::Toggle;
    let silence = Duration::from_millis(s.silence_timeout_ms as u64);
    let opts = DictationOptions {
        device: (!s.microphone.is_empty()).then(|| s.microphone.clone()),
        vad_sensitivity: s.vad_sensitivity as f32 / 100.0,
        auto_stop_silence: hands_free.then_some(silence),
        no_speech_timeout: hands_free.then_some(HANDS_FREE_NO_SPEECH),
        max_duration: Duration::from_secs(s.max_recording_sec as u64),
        language: (s.language != "auto").then(|| s.language.clone()),
        prompt: st.dict.read().unwrap().prompt(240),
        partials: s.live_preview && (s.show_indicator || app.get_webview_window("main").is_some()),
        keep_audio: s.store_audio && s.save_history,
    };
    let app2 = app.clone();
    let d = Dictation::start(st.engine.clone(), opts, move |ev| on_dictation_event(&app2, ev));
    *st.phase.lock().unwrap() = Phase::Recording { d, pressed_at: Instant::now(), holding: holding && s.hotkey_mode != HotkeyMode::Toggle };
    emit_status(app);
}

pub fn stop(app: &AppHandle) {
    let st = app.state::<AppState>();
    {
        let mut phase = st.phase.lock().unwrap();
        if !matches!(*phase, Phase::Recording { .. }) {
            return;
        }
        if let Phase::Recording { d, .. } = std::mem::replace(&mut *phase, Phase::Idle) {
            d.stop();
            *phase = Phase::Processing { _d: Some(d), since: Instant::now() };
        }
    }
    st.hotkeys.set_escape_enabled(false);
    st.sound(Sound::Stop);
    st.show(IndicatorState::Processing);
    emit_status(app);
}

pub fn cancel(app: &AppHandle) {
    let st = app.state::<AppState>();
    let mut phase = st.phase.lock().unwrap();
    if let Phase::Recording { d, .. } = std::mem::replace(&mut *phase, Phase::Idle) {
        d.cancel();
        *phase = Phase::Processing { _d: Some(d), since: Instant::now() };
    } else if let Phase::Processing { .. } = &*phase {
        // Nothing to cancel mid-transcription; keep state.
    }
    drop(phase);
    st.hotkeys.set_escape_enabled(false);
}

/// Toggle from the tray or the UI.
pub fn toggle(app: &AppHandle) {
    let recording = matches!(*app.state::<AppState>().phase.lock().unwrap(), Phase::Recording { .. });
    if recording {
        stop(app)
    } else {
        start(app, false)
    }
}

fn set_idle(app: &AppHandle) {
    let st = app.state::<AppState>();
    *st.phase.lock().unwrap() = Phase::Idle;
    st.hotkeys.set_escape_enabled(false);
    *st.session.lock().unwrap() = None;
    emit_status(app);
}

fn on_dictation_event(app: &AppHandle, ev: DictationEvent) {
    let st = app.state::<AppState>();
    match ev {
        DictationEvent::Listening { mic_latency_ms, .. } => {
            log::info!("listening (mic ready in {mic_latency_ms} ms)");
        }
        DictationEvent::Partial(text) => {
            if st.settings.read().unwrap().show_indicator {
                st.indicator.set(IndicatorState::Listening(Some(text.clone())));
            }
            let _ = app.emit("partial", text);
        }
        DictationEvent::AutoStopped => {
            {
                let mut phase = st.phase.lock().unwrap();
                if let Phase::Recording { d, .. } = std::mem::replace(&mut *phase, Phase::Idle) {
                    *phase = Phase::Processing { _d: Some(d), since: Instant::now() };
                }
            }
            st.hotkeys.set_escape_enabled(false);
            st.sound(Sound::Stop);
            emit_status(app);
        }
        DictationEvent::Processing => st.show(IndicatorState::Processing),
        DictationEvent::Done(r) => {
            finish(app, r);
            set_idle(app);
        }
        DictationEvent::NoSpeech => {
            st.show(IndicatorState::Info("No speech detected".into()));
            set_idle(app);
        }
        DictationEvent::Cancelled => {
            st.show(IndicatorState::Info("Cancelled".into()));
            set_idle(app);
        }
        DictationEvent::Failed(e) => {
            log::error!("dictation failed: {}", e.detail());
            st.sound(Sound::Error);
            st.show(IndicatorState::Error(e.to_string()));
            let _ = app.emit("dictation-error", e.to_string());
            set_idle(app);
        }
    }
}

fn finish(app: &AppHandle, r: DictationResult) {
    let st = app.state::<AppState>();
    let s = st.settings();
    let target = st.session.lock().unwrap().take().and_then(|s| s.target);
    let now_fg = platform::foreground_app();
    let info = context::classify(now_fg.as_ref().or(target.as_ref()));
    let ctx = if s.context_aware { info.context } else { AppContext::General };
    let opts = ProcessOptions {
        remove_fillers: s.remove_fillers,
        smart_punctuation: s.smart_punctuation,
        auto_capitalize: s.auto_capitalize,
        spoken_punctuation: s.spoken_punctuation,
        aggressive: s.aggressive_cleanup,
        language: if r.language.is_empty() { s.language.clone() } else { r.language.clone() },
        context: ctx,
    };
    let dict = st.dict.read().unwrap().clone();
    let text = process(&r.raw, &opts, &dict);
    if text.trim().is_empty() {
        st.show(IndicatorState::Info("Nothing to insert".into()));
        return;
    }
    // Consecutive dictations into the same app: separate with a space.
    let pid = now_fg.as_ref().map(|a| a.pid).unwrap_or(0);
    let mut to_insert = text.clone();
    if ctx != AppContext::Terminal && !text.starts_with('\n') {
        if let Some((last_pid, at)) = *st.last_insert.lock().unwrap() {
            if last_pid == pid && pid != 0 && at.elapsed() < Duration::from_secs(90) {
                to_insert.insert(0, ' ');
            }
        }
    }
    let t0 = Instant::now();
    // HUSHTYPE_DRY_RUN: skip insertion (profiling/leak tests on a live desktop).
    let outcome = if std::env::var_os("HUSHTYPE_DRY_RUN").is_some() {
        Ok(InsertOutcome::Typed)
    } else {
        platform::insert_text(
            &to_insert,
            &InsertOptions {
                method: s.insert_method,
                newline: context::newline_mode(ctx),
                terminal_paste: ctx == AppContext::Terminal,
                restore_clipboard: s.restore_clipboard,
            },
        )
    };
    let outcome_str = match &outcome {
        Ok(InsertOutcome::Typed) | Ok(InsertOutcome::Pasted) => {
            *st.last_insert.lock().unwrap() = Some((pid, Instant::now()));
            st.show(IndicatorState::Success("Inserted".into()));
            "inserted"
        }
        Ok(InsertOutcome::Copied) => {
            st.show(IndicatorState::Success("Copied to clipboard — press Ctrl+V to paste".into()));
            "copied"
        }
        Err(e) => {
            log::warn!("insertion failed: {e:?}");
            st.sound(Sound::Error);
            st.show(IndicatorState::Error(e.to_string()));
            "failed"
        }
    };
    log::info!(
        "dictation: {} ms audio, {} ms speech, {} chunk(s), final {} ms, insert {} ms via {:?} into {}",
        r.audio_ms,
        r.speech_ms,
        r.chunks,
        r.final_latency_ms,
        t0.elapsed().as_millis(),
        outcome.as_ref().ok(),
        info.name
    );

    if s.save_history {
        let id = now_ms() * 1000 + (r.audio_ms % 1000);
        let mut audio_file = None;
        if let Some(audio) = &r.audio {
            let _ = std::fs::create_dir_all(&st.paths.recordings);
            let path = st.paths.recordings.join(format!("{id}.wav"));
            if hushtype_engine::wav::write_mono_16(&path, 16_000, audio).is_ok() {
                audio_file = Some(path.to_string_lossy().into_owned());
            }
        }
        let entry = Entry {
            id,
            timestamp: now_ms(),
            app: info.name.clone(),
            raw: r.raw.clone(),
            text: text.clone(),
            language: opts.language.clone(),
            duration_ms: r.audio_ms,
            audio_file,
        };
        for f in st.history.append(&entry, s.history_limit as usize) {
            let _ = std::fs::remove_file(f);
        }
    }
    let _ = app.emit(
        "dictation-result",
        ResultPayload { text, app: info.name, outcome: outcome_str.into(), audio_ms: r.audio_ms, final_latency_ms: r.final_latency_ms },
    );
}

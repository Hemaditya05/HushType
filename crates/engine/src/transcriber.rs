//! The whisper.cpp worker.
//!
//! A single long-lived thread owns the model. It blocks on a channel while
//! idle (zero CPU), loads the model on first use, keeps it resident between
//! dictations and unloads it after a configurable period of inactivity.
//!
//! Final transcriptions have priority over partial previews: a running
//! partial is aborted as soon as a final job is queued.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState};

use crate::error::EngineError;
use crate::models;

pub const SAMPLE_RATE: usize = 16_000;

#[derive(Debug, Clone)]
pub struct Request {
    /// 16 kHz mono samples. Returned unchanged in the transcript so callers
    /// can keep it without copying.
    pub audio: Vec<f32>,
    /// ISO code, or None for automatic detection.
    pub language: Option<String>,
    /// Vocabulary/context prompt to bias recognition.
    pub prompt: String,
}

#[derive(Debug, Clone)]
pub struct Transcript {
    pub text: String,
    pub language: String,
    pub audio: Vec<f32>,
    pub infer_ms: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ModelStatus {
    Unloaded,
    Loading { model: String },
    Loaded { model: String, load_ms: u64, backend: String },
    Error { model: String, message: String },
}

type Reply = Sender<Result<Transcript, EngineError>>;
type PartialCallback = Box<dyn FnOnce(Result<Transcript, EngineError>) + Send>;
type StatusCallback = Box<dyn Fn(&ModelStatus) + Send + Sync>;

enum Cmd {
    SetModel(String),
    Load,
    Unload,
    SetIdleUnload(Option<Duration>),
    Final(Request, Reply),
    PartialWake,
}

struct Shared {
    status: Mutex<ModelStatus>,
    finals_pending: AtomicUsize,
    cancel_partial: AtomicBool,
    partial: Mutex<Option<(Request, PartialCallback)>>,
    on_status: Mutex<Option<StatusCallback>>,
}

impl Shared {
    fn set_status(&self, s: ModelStatus) {
        *self.status.lock().unwrap() = s.clone();
        if let Some(cb) = self.on_status.lock().unwrap().as_ref() {
            cb(&s);
        }
    }
}

/// Handle to the transcription worker. Cheap to share via `Arc`.
pub struct Engine {
    tx: Mutex<Sender<Cmd>>,
    shared: Arc<Shared>,
    pub models_dir: PathBuf,
}

impl Engine {
    pub fn new(models_dir: PathBuf, model_id: &str, idle_unload: Option<Duration>) -> Engine {
        whisper_rs::install_logging_hooks();
        let (tx, rx) = channel();
        let shared = Arc::new(Shared {
            status: Mutex::new(ModelStatus::Unloaded),
            finals_pending: AtomicUsize::new(0),
            cancel_partial: AtomicBool::new(false),
            partial: Mutex::new(None),
            on_status: Mutex::new(None),
        });
        let worker_shared = shared.clone();
        let dir = models_dir.clone();
        let model = model_id.to_string();
        std::thread::Builder::new()
            .name("whisper".into())
            .spawn(move || worker(rx, worker_shared, dir, model, idle_unload))
            .expect("spawn whisper worker");
        Engine { tx: Mutex::new(tx), shared, models_dir }
    }

    fn send(&self, c: Cmd) {
        let _ = self.tx.lock().unwrap().send(c);
    }

    pub fn on_status(&self, cb: impl Fn(&ModelStatus) + Send + Sync + 'static) {
        *self.shared.on_status.lock().unwrap() = Some(Box::new(cb));
    }

    pub fn status(&self) -> ModelStatus {
        self.shared.status.lock().unwrap().clone()
    }

    pub fn set_model(&self, id: &str) {
        self.send(Cmd::SetModel(id.to_string()));
    }

    /// Load the model in the background if it isn't loaded yet.
    pub fn preload(&self) {
        self.send(Cmd::Load);
    }

    pub fn unload(&self) {
        self.send(Cmd::Unload);
    }

    pub fn set_idle_unload(&self, d: Option<Duration>) {
        self.send(Cmd::SetIdleUnload(d));
    }

    /// Queue a final transcription; the result arrives on the returned channel.
    pub fn submit(&self, req: Request) -> Receiver<Result<Transcript, EngineError>> {
        let (tx, rx) = channel();
        self.shared.finals_pending.fetch_add(1, Ordering::SeqCst);
        self.send(Cmd::Final(req, tx));
        rx
    }

    /// Blocking convenience wrapper around [`submit`].
    pub fn transcribe(&self, req: Request) -> Result<Transcript, EngineError> {
        self.submit(req).recv().unwrap_or_else(|_| Err(EngineError::Transcription("worker stopped".into())))
    }

    /// Offer a partial (preview) job. Replaces any partial not yet started.
    pub fn submit_partial(&self, req: Request, cb: impl FnOnce(Result<Transcript, EngineError>) + Send + 'static) {
        self.shared.cancel_partial.store(false, Ordering::SeqCst);
        *self.shared.partial.lock().unwrap() = Some((req, Box::new(cb)));
        self.send(Cmd::PartialWake);
    }

    /// Drop the queued partial and abort a running one.
    pub fn cancel_partial(&self) {
        self.shared.cancel_partial.store(true, Ordering::SeqCst);
        self.shared.partial.lock().unwrap().take();
    }
}

/// whisper.cpp / ggml build features (AVX2, FMA, ...).
pub fn system_info() -> String {
    whisper_rs::print_system_info().to_string()
}

struct Loaded {
    id: String,
    english_only: bool,
    _ctx: WhisperContext,
    state: WhisperState,
}

fn worker(rx: Receiver<Cmd>, shared: Arc<Shared>, dir: PathBuf, mut model_id: String, mut idle: Option<Duration>) {
    let threads = crate::sys::inference_threads();
    let mut loaded: Option<Loaded> = None;
    let mut last_used = Instant::now();
    loop {
        let cmd = match (&loaded, idle) {
            (Some(_), Some(idle)) => {
                let wait = (last_used + idle).saturating_duration_since(Instant::now());
                match rx.recv_timeout(wait) {
                    Ok(c) => c,
                    Err(RecvTimeoutError::Timeout) => {
                        log::info!("model idle for {}s, unloading", idle.as_secs());
                        unload(&mut loaded, &shared);
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            _ => match rx.recv() {
                Ok(c) => c,
                Err(_) => break,
            },
        };
        match cmd {
            Cmd::SetModel(id) => {
                if loaded.as_ref().map(|l| l.id != id).unwrap_or(false) {
                    unload(&mut loaded, &shared);
                }
                model_id = id;
            }
            Cmd::Load => {
                if let Err(e) = ensure_loaded(&mut loaded, &shared, &dir, &model_id, threads) {
                    log::error!("model load failed: {}", e.detail());
                }
                last_used = Instant::now();
            }
            Cmd::Unload => unload(&mut loaded, &shared),
            Cmd::SetIdleUnload(d) => idle = d,
            Cmd::Final(req, reply) => {
                shared.finals_pending.fetch_sub(1, Ordering::SeqCst);
                let res = ensure_loaded(&mut loaded, &shared, &dir, &model_id, threads)
                    .and_then(|_| run(loaded.as_mut().unwrap(), req, threads, false, &shared));
                last_used = Instant::now();
                let _ = reply.send(res);
            }
            Cmd::PartialWake => {
                if shared.finals_pending.load(Ordering::SeqCst) > 0 {
                    continue; // finals first; the session will offer a fresh partial
                }
                let Some((req, cb)) = shared.partial.lock().unwrap().take() else { continue };
                let res = ensure_loaded(&mut loaded, &shared, &dir, &model_id, threads)
                    .and_then(|_| run(loaded.as_mut().unwrap(), req, threads, true, &shared));
                last_used = Instant::now();
                cb(res);
            }
        }
    }
}

fn unload(loaded: &mut Option<Loaded>, shared: &Shared) {
    if loaded.take().is_some() {
        shared.set_status(ModelStatus::Unloaded);
        crate::sys_trim_memory();
    }
}

fn ensure_loaded(
    loaded: &mut Option<Loaded>,
    shared: &Shared,
    dir: &std::path::Path,
    id: &str,
    threads: usize,
) -> Result<(), EngineError> {
    if loaded.as_ref().map(|l| l.id == id).unwrap_or(false) {
        return Ok(());
    }
    unload(loaded, shared);
    let fail = |e: EngineError| {
        shared.set_status(ModelStatus::Error { model: id.to_string(), message: e.to_string() });
        Err(e)
    };
    if !crate::sys::cpu_supported() {
        return fail(EngineError::UnsupportedCpu);
    }
    let Some(info) = models::find(id) else { return fail(EngineError::ModelMissing(id.to_string())) };
    let path = dir.join(info.file);
    if !models::is_installed(dir, id) {
        return fail(EngineError::ModelMissing(info.name.to_string()));
    }
    if let Some((_, avail)) = crate::sys::memory() {
        let avail_mb = avail / (1 << 20);
        let needed_mb = info.ram_mb + 100;
        if avail_mb < needed_mb {
            return fail(EngineError::NotEnoughMemory { needed_mb, available_mb: avail_mb });
        }
    }
    shared.set_status(ModelStatus::Loading { model: id.to_string() });
    let t0 = Instant::now();
    let mut params = WhisperContextParameters::default();
    params.use_gpu = !crate::sys::gpu_backends().is_empty();
    // Flash attention is slower than the default kernels on AVX2 CPUs.
    params.flash_attn = std::env::var("HUSHTYPE_FLASH_ATTN").map(|v| v == "1").unwrap_or(false);
    let ctx = match WhisperContext::new_with_params(&path, params) {
        Ok(c) => c,
        Err(e) => return fail(EngineError::ModelLoad(format!("{e:?}"))),
    };
    let state = match ctx.create_state() {
        Ok(s) => s,
        Err(e) => return fail(EngineError::ModelLoad(format!("create_state: {e:?}"))),
    };
    let mut l = Loaded { id: id.to_string(), english_only: info.english_only, _ctx: ctx, state };
    // Warm-up: the first inference allocates compute buffers; do it now
    // (typically while the user is still speaking) rather than on their audio.
    let warm = Request { audio: vec![0.0; SAMPLE_RATE], language: Some("en".into()), prompt: String::new() };
    let _ = run(&mut l, warm, threads, true, &Shared::dummy());
    let load_ms = t0.elapsed().as_millis() as u64;
    let backend = match crate::sys::gpu_backends().first() {
        Some(b) => format!("{b} (GPU)"),
        None => format!("CPU, {threads} threads"),
    };
    log::info!("model {id} loaded in {load_ms} ms on {backend}");
    *loaded = Some(l);
    shared.set_status(ModelStatus::Loaded { model: id.to_string(), load_ms, backend });
    Ok(())
}

impl Shared {
    fn dummy() -> Shared {
        Shared {
            status: Mutex::new(ModelStatus::Unloaded),
            finals_pending: AtomicUsize::new(0),
            cancel_partial: AtomicBool::new(false),
            partial: Mutex::new(None),
            on_status: Mutex::new(None),
        }
    }
}

unsafe extern "C" fn abort_partial(data: *mut std::ffi::c_void) -> bool {
    let s = &*(data as *const Shared);
    s.finals_pending.load(Ordering::Relaxed) > 0 || s.cancel_partial.load(Ordering::Relaxed)
}

fn run(l: &mut Loaded, req: Request, threads: usize, partial: bool, shared: &Shared) -> Result<Transcript, EngineError> {
    let t0 = Instant::now();
    let Request { audio, language, prompt } = req;
    // whisper.cpp ignores input shorter than 1 s; pad with silence.
    let min = SAMPLE_RATE + SAMPLE_RATE / 10;
    let padded;
    let input: &[f32] = if audio.len() < min {
        let mut p = Vec::with_capacity(min);
        p.extend_from_slice(&audio);
        p.resize(min, 0.0);
        padded = p;
        &padded
    } else {
        &audio
    };

    let lang = if l.english_only { Some("en".to_string()) } else { language.filter(|s| !s.is_empty() && s != "auto") };
    // The encoder normally always processes a 30 s window (1500 frames) and
    // its cost grows quadratically with it. Dictation clips are short, so size
    // the window to the audio (50 frames/s) plus a safety margin: ~3x faster
    // with identical output in our tests. If the decoder then loops (a known
    // failure mode of reduced windows, mostly on tiny models), redo it with
    // the full window.
    let secs = input.len() as f32 / SAMPLE_RATE as f32;
    let margin = if partial { 128.0 } else { 256.0 };
    let mut ctx = (((secs * 50.0 + margin) / 64.0).ceil() as i32 * 64).clamp(256, 1500);
    if std::env::var_os("HUSHTYPE_FULL_CTX").is_some() {
        ctx = 1500;
    }
    if let Some(n) = std::env::var("HUSHTYPE_AUDIO_CTX").ok().and_then(|s| s.parse::<i32>().ok()) {
        ctx = n;
    }
    let mut text = decode(l, input, lang.as_deref(), &prompt, threads, partial, ctx, shared)?;
    if !partial && ctx < 1500 && looks_repetitive(&text, secs) {
        log::info!("repetitive output with audio_ctx {ctx}; retrying with the full window");
        text = decode(l, input, lang.as_deref(), &prompt, threads, partial, 1500, shared)?;
    }
    let language = match &lang {
        Some(c) => c.clone(),
        None => whisper_rs::get_lang_str(l.state.full_lang_id_from_state()).unwrap_or("").to_string(),
    };
    Ok(Transcript { text, language, audio, infer_ms: t0.elapsed().as_millis() as u64 })
}

/// A sentence repeated verbatim, or far more text than anyone can say in the
/// time, means the decoder got stuck in a loop.
fn looks_repetitive(text: &str, secs: f32) -> bool {
    if text.chars().count() as f32 > 30.0 * secs.max(1.0) + 20.0 {
        return true;
    }
    let sentences: Vec<String> = text
        .split(['.', '?', '!'])
        .map(|s| s.trim().to_lowercase())
        .filter(|s| s.len() >= 12)
        .collect();
    sentences.windows(2).any(|w| w[0] == w[1])
}

#[allow(clippy::too_many_arguments)]
fn decode(
    l: &mut Loaded,
    input: &[f32],
    lang: Option<&str>,
    prompt: &str,
    threads: usize,
    partial: bool,
    ctx: i32,
    shared: &Shared,
) -> Result<String, EngineError> {
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(threads as i32);
    params.set_translate(false);
    params.set_no_context(true);
    params.set_no_timestamps(true);
    params.set_single_segment(partial);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    params.set_temperature(0.0);
    params.set_language(Some(lang.unwrap_or("auto")));
    if !prompt.is_empty() {
        params.set_initial_prompt(prompt);
    }
    params.set_audio_ctx(ctx);
    if partial {
        // Previews trade a little accuracy for speed: no temperature fallback.
        params.set_temperature_inc(0.0);
        // whisper-rs' `set_abort_callback_safe` casts its boxed closure back to
        // the wrong type (0.16), so use the raw callback with `Shared` as data.
        // SAFETY: `shared` outlives this call (it is the engine's Arc'd state or
        // a stack value in the caller) and whisper only calls back during `full`.
        unsafe {
            params.set_abort_callback(Some(abort_partial));
            params.set_abort_callback_user_data(shared as *const Shared as *mut std::ffi::c_void);
        }
    }
    l.state.full(params, input).map_err(|e| EngineError::Transcription(format!("{e:?}")))?;
    if std::env::var_os("HUSHTYPE_WHISPER_TIMINGS").is_some() {
        l._ctx.print_timings();
        l._ctx.reset_timings();
    }
    let mut text = String::new();
    for seg in l.state.as_iter() {
        if let Ok(s) = seg.to_str_lossy() {
            text.push_str(&s);
        }
    }
    Ok(text.trim().to_string())
}

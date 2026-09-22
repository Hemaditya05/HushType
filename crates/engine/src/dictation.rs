//! One dictation: microphone -> VAD -> chunked transcription -> raw text.
//!
//! Streaming strategy: audio accumulates in the current chunk. Every ~0.5 s
//! of new speech a *partial* preview of the chunk is transcribed (low-cost,
//! droppable). Once a chunk is long enough and the speaker pauses, it is
//! committed as a *final* job that runs while the user keeps talking, so
//! releasing the hotkey only has to wait for the last few seconds of audio.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::audio::Capture;
use crate::error::EngineError;
use crate::transcriber::{Engine, Request, Transcript, SAMPLE_RATE};
use crate::vad::Vad;

#[derive(Debug, Clone)]
pub struct DictationOptions {
    pub device: Option<String>,
    /// 0..1, higher = picks up quieter speech.
    pub vad_sensitivity: f32,
    /// Stop automatically after this much silence following speech (toggle mode).
    pub auto_stop_silence: Option<Duration>,
    /// Give up if nothing is said within this time (toggle mode).
    pub no_speech_timeout: Option<Duration>,
    pub max_duration: Duration,
    pub language: Option<String>,
    pub prompt: String,
    pub partials: bool,
    pub keep_audio: bool,
}

impl Default for DictationOptions {
    fn default() -> Self {
        DictationOptions {
            device: None,
            vad_sensitivity: 0.5,
            auto_stop_silence: None,
            no_speech_timeout: None,
            max_duration: Duration::from_secs(300),
            language: Some("en".into()),
            prompt: String::new(),
            partials: true,
            keep_audio: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DictationResult {
    pub raw: String,
    pub language: String,
    /// Full 16 kHz recording, only when `keep_audio` was set.
    pub audio: Option<Vec<f32>>,
    pub audio_ms: u64,
    pub speech_ms: u32,
    pub mic_latency_ms: u64,
    pub first_partial_ms: Option<u64>,
    /// From the end of recording to the final text.
    pub final_latency_ms: u64,
    pub chunks: usize,
}

#[derive(Debug, Clone)]
pub enum DictationEvent {
    /// The microphone is live.
    Listening { device: String, mic_latency_ms: u64 },
    /// Preview of the text so far (raw recogniser output).
    Partial(String),
    /// Silence/limit reached; recording ended without the user stopping it.
    AutoStopped,
    /// Recording finished, waiting for the final transcription.
    Processing,
    Done(DictationResult),
    NoSpeech,
    Cancelled,
    Failed(EngineError),
}

struct Flags {
    stop: AtomicBool,
    cancel: AtomicBool,
    finished: AtomicBool,
    /// Auto-stop after this much silence (ms, 0 = off). Adjustable while recording.
    auto_stop_ms: AtomicU64,
    /// Give up if no speech within this time (ms, 0 = off).
    no_speech_ms: AtomicU64,
}

fn ms(d: Option<Duration>) -> u64 {
    d.map(|d| d.as_millis().max(1) as u64).unwrap_or(0)
}

pub struct Dictation {
    flags: Arc<Flags>,
    thread: Option<JoinHandle<()>>,
}

const CHUNK_MIN_SECS: usize = 12;
const CHUNK_MAX_SECS: usize = 28;
const PARTIAL_INTERVAL: Duration = Duration::from_millis(450);
const KEEP_MARGIN: usize = SAMPLE_RATE * 3 / 10; // 300 ms around speech

impl Dictation {
    pub fn start(
        engine: Arc<Engine>,
        opts: DictationOptions,
        on_event: impl Fn(DictationEvent) + Send + Sync + 'static,
    ) -> Dictation {
        let flags = Arc::new(Flags {
            stop: AtomicBool::new(false),
            cancel: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            auto_stop_ms: AtomicU64::new(ms(opts.auto_stop_silence)),
            no_speech_ms: AtomicU64::new(ms(opts.no_speech_timeout)),
        });
        let f = flags.clone();
        let thread = std::thread::Builder::new()
            .name("dictation".into())
            .spawn(move || {
                let ev = on_event;
                run(&engine, &opts, &f, &ev);
                f.finished.store(true, Ordering::SeqCst);
            })
            .expect("spawn dictation");
        Dictation { flags, thread: Some(thread) }
    }

    /// Stop recording and transcribe what was said.
    pub fn stop(&self) {
        self.flags.stop.store(true, Ordering::SeqCst);
    }

    /// Change the automatic stop rules of a running dictation (e.g. when a
    /// short tap switches push-to-talk into hands-free mode).
    pub fn set_auto_stop(&self, silence: Option<Duration>, no_speech: Option<Duration>) {
        self.flags.auto_stop_ms.store(ms(silence), Ordering::SeqCst);
        self.flags.no_speech_ms.store(ms(no_speech), Ordering::SeqCst);
    }

    /// Stop recording and discard everything.
    pub fn cancel(&self) {
        self.flags.cancel.store(true, Ordering::SeqCst);
    }

    pub fn is_finished(&self) -> bool {
        self.flags.finished.load(Ordering::SeqCst)
    }

    pub fn join(mut self) {
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Dictation {
    fn drop(&mut self) {
        // Never leave a recording running behind a dropped handle.
        if !self.is_finished() {
            self.cancel();
        }
    }
}

fn tail(s: &str, max_chars: usize) -> &str {
    let n = s.chars().count();
    if n <= max_chars {
        return s;
    }
    let skip = s.char_indices().nth(n - max_chars).map(|(i, _)| i).unwrap_or(0);
    &s[skip..]
}

fn build_prompt(base: &str, previous: &str) -> String {
    let prev = tail(previous, 200);
    match (base.is_empty(), prev.is_empty()) {
        (true, true) => String::new(),
        (false, true) => base.to_string(),
        (true, false) => prev.to_string(),
        (false, false) => format!("{base} {prev}"),
    }
}

fn run(engine: &Arc<Engine>, opts: &DictationOptions, flags: &Flags, emit: &(dyn Fn(DictationEvent) + Send + Sync)) {
    engine.preload();
    let mut cap = match Capture::open(opts.device.as_deref()) {
        Ok(c) => c,
        Err(e) => {
            emit(DictationEvent::Failed(e));
            return;
        }
    };
    let started = Instant::now();
    let mut vad = Vad::new(opts.vad_sensitivity);
    let mut chunk: Vec<f32> = Vec::with_capacity(SAMPLE_RATE * CHUNK_MAX_SECS);
    let mut chunk_start: usize = 0; // absolute sample index of chunk[0]
    let mut chunk_speech_start_ms = 0u32;
    let mut buf: Vec<f32> = Vec::with_capacity(8192);
    let mut listening_sent = false;
    let mut mic_latency_ms = 0;

    let mut finals: Vec<Receiver<Result<Transcript, EngineError>>> = Vec::new();
    let mut committed: Vec<String> = Vec::new(); // texts of finished finals, in order
    let mut language = String::new();
    let mut kept_audio: Vec<f32> = Vec::new();

    let partial_busy = Arc::new(AtomicBool::new(false));
    let partial_text: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let mut last_partial_at = Instant::now();
    let mut last_partial_sample = 0usize;
    let mut last_shown = String::new();
    let mut first_partial_ms: Option<u64> = None;
    let mut auto_stopped = false;

    loop {
        if flags.cancel.load(Ordering::SeqCst) {
            drop(cap);
            engine.cancel_partial();
            emit(DictationEvent::Cancelled);
            return;
        }
        if flags.stop.load(Ordering::SeqCst) {
            break;
        }
        buf.clear();
        if let Err(e) = cap.read(&mut buf, Duration::from_millis(30)) {
            drop(cap);
            engine.cancel_partial();
            emit(DictationEvent::Failed(e));
            return;
        }
        if !buf.is_empty() && !listening_sent {
            listening_sent = true;
            mic_latency_ms = cap.first_audio.map(|d| d.as_millis() as u64).unwrap_or(0);
            emit(DictationEvent::Listening { device: cap.device_name.clone(), mic_latency_ms });
        }
        vad.push(&buf);
        chunk.extend_from_slice(&buf);

        let elapsed = started.elapsed();
        let auto_stop = flags.auto_stop_ms.load(Ordering::Relaxed);
        if auto_stop > 0 && vad.ever_speech && vad.silence_ms as u64 >= auto_stop {
            auto_stopped = true;
            break;
        }
        let no_speech = flags.no_speech_ms.load(Ordering::Relaxed);
        if no_speech > 0 {
            if !vad.ever_speech && elapsed.as_millis() as u64 >= no_speech {
                drop(cap);
                engine.cancel_partial();
                emit(DictationEvent::NoSpeech);
                return;
            }
        }
        if elapsed >= opts.max_duration {
            auto_stopped = true;
            break;
        }

        // Commit a chunk at a pause once it is long enough (or when it must).
        let secs = chunk.len() / SAMPLE_RATE;
        if (secs >= CHUNK_MIN_SECS && !vad.in_speech() && vad.silence_ms >= 400) || secs >= CHUNK_MAX_SECS {
            let audio = std::mem::replace(&mut chunk, Vec::with_capacity(SAMPLE_RATE * CHUNK_MAX_SECS));
            let had_speech = vad.speech_ms.saturating_sub(chunk_speech_start_ms) >= 200;
            chunk_start += audio.len();
            chunk_speech_start_ms = vad.speech_ms;
            last_partial_sample = chunk_start;
            if had_speech {
                engine.cancel_partial();
                let previous = committed.join(" ") + " " + partial_text.lock().unwrap().as_deref().unwrap_or("");
                finals.push(engine.submit(Request {
                    audio,
                    language: opts.language.clone(),
                    prompt: build_prompt(&opts.prompt, previous.trim()),
                }));
            } else if opts.keep_audio {
                kept_audio.extend_from_slice(&audio);
            }
            *partial_text.lock().unwrap() = None;
        }

        // Collect finished finals without blocking.
        while committed.len() < finals.len() {
            match finals[committed.len()].try_recv() {
                Ok(Ok(t)) => {
                    if language.is_empty() {
                        language = t.language.clone();
                    }
                    if opts.keep_audio {
                        kept_audio.extend_from_slice(&t.audio);
                    }
                    committed.push(t.text);
                }
                Ok(Err(e)) => {
                    drop(cap);
                    engine.cancel_partial();
                    emit(DictationEvent::Failed(e));
                    return;
                }
                Err(_) => break,
            }
        }

        // Offer a new partial preview.
        let new_speech = vad.last_speech_sample > last_partial_sample;
        if opts.partials
            && vad.ever_speech
            && new_speech
            && chunk.len() >= SAMPLE_RATE / 2
            && last_partial_at.elapsed() >= PARTIAL_INTERVAL
            && !partial_busy.swap(true, Ordering::SeqCst)
        {
            last_partial_at = Instant::now();
            last_partial_sample = vad.last_speech_sample;
            let busy = partial_busy.clone();
            let slot = partial_text.clone();
            let previous = committed.join(" ");
            let req = Request {
                audio: chunk.clone(),
                language: opts.language.clone(),
                prompt: build_prompt(&opts.prompt, &previous),
            };
            engine.submit_partial(req, move |res| {
                if let Ok(t) = res {
                    *slot.lock().unwrap() = Some(t.text);
                }
                busy.store(false, Ordering::SeqCst);
            });
        }
        let current = partial_text.lock().unwrap().clone();
        if let Some(p) = current {
            let shown = format!("{} {}", committed.join(" "), p).trim().to_string();
            if shown != last_shown && !shown.is_empty() {
                if first_partial_ms.is_none() {
                    first_partial_ms = Some(started.elapsed().as_millis() as u64);
                }
                emit(DictationEvent::Partial(shown.clone()));
                last_shown = shown;
            }
        }
    }

    // Recording is over: release the microphone right away.
    let stop_at = Instant::now();
    let total_samples = chunk_start + chunk.len();
    drop(cap);
    engine.cancel_partial();
    if auto_stopped {
        emit(DictationEvent::AutoStopped);
    }
    emit(DictationEvent::Processing);

    // Final chunk, trimmed to the speech plus a small margin.
    let chunk_has_speech = vad.speech_ms.saturating_sub(chunk_speech_start_ms) >= 150;
    if chunk_has_speech {
        let end_abs = (vad.last_speech_sample + KEEP_MARGIN).min(total_samples);
        let end = end_abs.saturating_sub(chunk_start).clamp(0, chunk.len());
        let begin = if finals.is_empty() {
            vad.first_speech_sample.unwrap_or(0).saturating_sub(KEEP_MARGIN).saturating_sub(chunk_start).min(end)
        } else {
            0
        };
        if opts.keep_audio {
            kept_audio.extend_from_slice(&chunk[..begin]);
        }
        chunk.truncate(end);
        let audio: Vec<f32> = if begin > 0 { chunk.split_off(begin) } else { std::mem::take(&mut chunk) };
        let previous = committed.join(" ");
        finals.push(engine.submit(Request {
            audio,
            language: opts.language.clone(),
            prompt: build_prompt(&opts.prompt, &previous),
        }));
    }
    drop(chunk);

    for rx in finals.iter().skip(committed.len()) {
        if flags.cancel.load(Ordering::SeqCst) {
            emit(DictationEvent::Cancelled);
            return;
        }
        match rx.recv() {
            Ok(Ok(t)) => {
                if language.is_empty() {
                    language = t.language.clone();
                }
                if opts.keep_audio {
                    kept_audio.extend_from_slice(&t.audio);
                }
                committed.push(t.text);
            }
            Ok(Err(e)) => {
                emit(DictationEvent::Failed(e));
                return;
            }
            Err(_) => {
                emit(DictationEvent::Failed(EngineError::Transcription("worker stopped".into())));
                return;
            }
        }
    }
    let raw = committed.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
    let speech_ms = vad.speech_ms;
    if speech_ms < 250 || raw.is_empty() || (speech_ms < 1500 && hushtype_text::is_hallucination(&raw)) {
        emit(DictationEvent::NoSpeech);
        return;
    }
    emit(DictationEvent::Done(DictationResult {
        raw,
        language,
        audio: if opts.keep_audio { Some(kept_audio) } else { None },
        audio_ms: (total_samples * 1000 / SAMPLE_RATE) as u64,
        speech_ms,
        mic_latency_ms,
        first_partial_ms,
        final_latency_ms: stop_at.elapsed().as_millis() as u64,
        chunks: finals.len(),
    }));
}

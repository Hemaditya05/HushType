//! hushtype-bench: measure the speech engine on this machine.
//!
//!   hushtype-bench download <model>
//!   hushtype-bench transcribe <file.wav> [--model id] [--context terminal|code|general]
//!   hushtype-bench run [--models base.en,small.en] [--fixtures dir] [--out docs/BENCHMARKS.md]
//!   hushtype-bench leak [--model id] [--cycles 100]
//!
//! All numbers are measured live; nothing is estimated.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::{Duration, Instant};

use hushtype_engine::{models, sys, wav, Dictation, DictationEvent, DictationOptions, Engine, ModelStatus, Request};
use hushtype_text::{default_terms, process, AppContext, Dictionary, ProcessOptions};

struct Logger;
impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= if std::env::var_os("HUSHTYPE_WHISPER_TIMINGS").is_some() { log::Level::Info } else { log::Level::Warn }
    }
    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            eprintln!("[{}] {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}

// ---------------------------------------------------------------------------
// Process metrics
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Default)]
struct Mem {
    working_set: u64,
    private: u64,
    peak_working_set: u64,
}

#[cfg(windows)]
fn mem() -> Mem {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut c = PROCESS_MEMORY_COUNTERS_EX::default();
    unsafe {
        let _ = GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
        );
    }
    Mem { working_set: c.WorkingSetSize as u64, private: c.PrivateUsage as u64, peak_working_set: c.PeakWorkingSetSize as u64 }
}

#[cfg(not(windows))]
fn mem() -> Mem {
    Mem::default()
}

/// Total CPU time (user + kernel) consumed by this process.
#[cfg(windows)]
fn cpu_time() -> Duration {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let (mut a, mut b, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut k, &mut u);
    }
    let t = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) * 100;
    Duration::from_nanos(t(k) + t(u))
}

#[cfg(not(windows))]
fn cpu_time() -> Duration {
    Duration::ZERO
}

fn mb(b: u64) -> f64 {
    b as f64 / (1024.0 * 1024.0)
}

/// Average CPU utilisation (% of the whole machine) over an interval.
fn cpu_pct(cpu: Duration, wall: Duration) -> f64 {
    let cores = sys::cpu_info().logical_cores as f64;
    100.0 * cpu.as_secs_f64() / wall.as_secs_f64().max(1e-6) / cores
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

/// Word error rate between reference and hypothesis.
fn wer(reference: &str, hypothesis: &str) -> f64 {
    let r = words(reference);
    let h = words(hypothesis);
    if r.is_empty() {
        return if h.is_empty() { 0.0 } else { 1.0 };
    }
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    let mut cur = vec![0; h.len() + 1];
    for i in 1..=r.len() {
        cur[0] = i;
        for j in 1..=h.len() {
            let cost = usize::from(r[i - 1] != h[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[h.len()] as f64 / r.len() as f64
}

fn wait_loaded(engine: &Engine, timeout: Duration) -> Result<u64, String> {
    let t0 = Instant::now();
    loop {
        match engine.status() {
            ModelStatus::Loaded { load_ms, .. } => return Ok(load_ms),
            ModelStatus::Error { message, .. } => return Err(message),
            _ => {}
        }
        if t0.elapsed() > timeout {
            return Err("timeout loading model".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn wait_unloaded(engine: &Engine) {
    for _ in 0..500 {
        if engine.status() == ModelStatus::Unloaded {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn ensure_model(dir: &Path, id: &str) -> Result<(), String> {
    if models::is_installed(dir, id) {
        return Ok(());
    }
    eprintln!("downloading {id} ...");
    let cancel = AtomicBool::new(false);
    models::download(dir, id, &cancel, |p| {
        eprint!("\r  {:.1}/{:.1} MB  {:.1} MB/s   ", mb(p.downloaded), mb(p.total), mb(p.bytes_per_sec));
    })
    .map(|_| eprintln!())
    .map_err(|e| e.to_string())
}

#[derive(serde::Deserialize, Clone)]
struct Fixture {
    id: String,
    file: String,
    text: String,
}

fn load_fixtures(dir: &Path) -> Vec<Fixture> {
    let raw = std::fs::read_to_string(dir.join("fixtures.json")).expect("fixtures.json (run scripts/make-fixtures.ps1)");
    serde_json::from_str(raw.trim_start_matches('\u{feff}')).expect("parse fixtures.json")
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn whisper_rs_sys_info() -> String {
    hushtype_engine::transcriber::system_info()
}

fn cmd_transcribe(args: &[String]) {
    let path = PathBuf::from(args.get(2).expect("usage: transcribe <file.wav>"));
    let id = arg(args, "--model").unwrap_or_else(|| models::DEFAULT_MODEL.into());
    let context = match arg(args, "--context").as_deref() {
        Some("terminal") => AppContext::Terminal,
        Some("code") => AppContext::Code,
        _ => AppContext::General,
    };
    let dir = models::default_models_dir();
    ensure_model(&dir, &id).unwrap();
    let engine = Engine::new(dir, &id, None);
    let w = wav::read(&path).expect("read wav");
    let audio = wav::to_16k_mono(&w);
    let dict = Dictionary::new(&default_terms());
    let t = engine
        .transcribe(Request { audio, language: Some("en".into()), prompt: if args.iter().any(|a| a == "--no-prompt") { String::new() } else { dict.prompt(300) } })
        .expect("transcribe");
    let opts = ProcessOptions { context, language: t.language.clone(), ..Default::default() };
    println!("raw:       {}", t.text);
    println!("processed: {}", process(&t.text, &opts, &dict));
    println!("language:  {}   inference: {} ms", t.language, t.infer_ms);
    println!("system:    {}", whisper_rs_sys_info());
}

struct Row {
    model: String,
    load_ms: u64,
    ram_idle_mb: f64,
    ram_loaded_mb: f64,
    ram_peak_mb: f64,
    ram_unloaded_mb: f64,
    per_fixture: Vec<(String, f64, u64, u64, f64, f64)>, // id, audio s, partial ms, final ms, cpu%, wer
    stream: Vec<(String, u64, Option<u64>, u64)>,        // id, mic ms, first partial ms, final latency ms
}

fn bench_model(id: &str, dir: &Path, fixtures: &[Fixture], fdir: &Path) -> Row {
    let dict = Dictionary::new(&default_terms());
    let prompt = dict.prompt(300);
    let before = mem();
    let engine = Arc::new(Engine::new(dir.to_path_buf(), id, None));
    engine.preload();
    let load_ms = wait_loaded(&engine, Duration::from_secs(120)).expect("load");
    std::thread::sleep(Duration::from_millis(300));
    let loaded = mem();
    let mut per_fixture = Vec::new();
    for f in fixtures {
        let audio = wav::to_16k_mono(&wav::read(&fdir.join(&f.file)).unwrap());
        let secs = audio.len() as f64 / 16_000.0;
        // Partial-style job on the first 1.5 s (what the preview does).
        let head: Vec<f32> = audio[..audio.len().min(24_000)].to_vec();
        let (ptx, prx) = channel();
        let t0 = Instant::now();
        engine.submit_partial(Request { audio: head, language: Some("en".into()), prompt: prompt.clone() }, move |r| {
            let _ = ptx.send(r.is_ok());
        });
        let _ = prx.recv();
        let partial_ms = t0.elapsed().as_millis() as u64;
        // Final job on the whole utterance.
        let c0 = cpu_time();
        let t0 = Instant::now();
        let t = engine.transcribe(Request { audio, language: Some("en".into()), prompt: prompt.clone() }).unwrap();
        let wall = t0.elapsed();
        let cpu = cpu_pct(cpu_time() - c0, wall);
        let e = wer(&f.text, &t.text);
        eprintln!("  {id} {:<9} {:>5.1}s audio  partial {:>5} ms  final {:>5} ms  WER {:>4.1}%  | {}", f.id, secs, partial_ms, wall.as_millis(), e * 100.0, t.text);
        per_fixture.push((f.id.clone(), secs, partial_ms, wall.as_millis() as u64, cpu, e));
    }
    // Real-time streaming through the full dictation pipeline (file-fed mic).
    let mut stream = Vec::new();
    for f in fixtures.iter().filter(|f| ["short", "medium", "long"].contains(&f.id.as_str())) {
        std::env::set_var("HUSHTYPE_TEST_AUDIO", fdir.join(&f.file));
        let (tx, rx) = channel();
        let opts = DictationOptions {
            auto_stop_silence: Some(Duration::from_millis(800)),
            prompt: prompt.clone(),
            ..Default::default()
        };
        let d = Dictation::start(engine.clone(), opts, move |e| {
            let _ = tx.send(e);
        });
        let mut mic = 0;
        let mut result = None;
        while let Ok(e) = rx.recv_timeout(Duration::from_secs(120)) {
            match e {
                DictationEvent::Listening { mic_latency_ms, .. } => mic = mic_latency_ms,
                DictationEvent::Done(r) => {
                    result = Some(r);
                    break;
                }
                DictationEvent::Failed(e) => panic!("{e}"),
                DictationEvent::NoSpeech | DictationEvent::Cancelled => break,
                _ => {}
            }
        }
        d.join();
        if let Some(r) = result {
            eprintln!("  {id} stream {:<7} first partial {:?} ms, final {} ms after stop", f.id, r.first_partial_ms, r.final_latency_ms);
            stream.push((f.id.clone(), mic, r.first_partial_ms, r.final_latency_ms));
        }
    }
    std::env::remove_var("HUSHTYPE_TEST_AUDIO");
    let peak = mem();
    engine.unload();
    wait_unloaded(&engine);
    std::thread::sleep(Duration::from_millis(500));
    let after = mem();
    Row {
        model: id.to_string(),
        load_ms,
        // Private (committed) bytes: unaffected by working-set trimming.
        ram_idle_mb: mb(before.private),
        ram_loaded_mb: mb(loaded.private),
        ram_peak_mb: mb(peak.peak_working_set),
        ram_unloaded_mb: mb(after.private),
        per_fixture,
        stream,
    }
}

fn mic_latency() -> Vec<u64> {
    let mut v = Vec::new();
    for _ in 0..5 {
        match hushtype_engine::audio::Capture::open(None) {
            Ok(mut c) => {
                let mut buf = Vec::new();
                let t0 = Instant::now();
                while c.first_audio.is_none() && t0.elapsed() < Duration::from_secs(3) {
                    let _ = c.read(&mut buf, Duration::from_millis(20));
                }
                if let Some(d) = c.first_audio {
                    v.push(d.as_millis() as u64);
                }
            }
            Err(e) => {
                eprintln!("microphone unavailable: {e}");
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    v
}

fn cmd_run(args: &[String]) {
    let dir = models::default_models_dir();
    let fdir = PathBuf::from(arg(args, "--fixtures").unwrap_or_else(|| "tests/fixtures".into()));
    let fixtures = load_fixtures(&fdir);
    let ids: Vec<String> = arg(args, "--models").unwrap_or_else(|| "tiny.en,base.en,small.en".into()).split(',').map(|s| s.to_string()).collect();
    for id in &ids {
        ensure_model(&dir, id).unwrap();
    }
    let cpu = sys::cpu_info();
    let (total_ram, _) = sys::memory().unwrap_or((0, 0));
    eprintln!("microphone latency ...");
    let mic = mic_latency();
    let mut rows = Vec::new();
    for id in &ids {
        eprintln!("benchmarking {id} ...");
        rows.push(bench_model(id, &dir, &fixtures, &fdir));
    }
    let mut md = String::new();
    use std::fmt::Write;
    let _ = writeln!(md, "## Engine benchmark\n");
    let _ = writeln!(
        md,
        "Machine: {} logical cores, AVX2 {}, AVX-512 {}, {:.1} GB RAM. Backend: {}. Inference threads: {}.\n",
        cpu.logical_cores,
        cpu.avx2,
        cpu.avx512,
        total_ram as f64 / (1u64 << 30) as f64,
        if sys::gpu_backends().is_empty() { "CPU (no GPU backend compiled)".to_string() } else { sys::gpu_backends().join(", ") },
        sys::inference_threads()
    );
    let _ = writeln!(md, "Measured with `hushtype-bench run` (release build). Audio: synthesized speech from `scripts/make-fixtures.ps1` (Windows SAPI voice). WER is computed on the raw recogniser output against the script text, ignoring case and punctuation.\n");
    if mic.is_empty() {
        let _ = writeln!(md, "Microphone capture latency: no microphone available.\n");
    } else {
        let _ = writeln!(md, "Microphone capture latency (open device -> first audio buffer, 5 runs): {:?} ms\n", mic);
    }
    let _ = writeln!(md, "### Model load and memory (benchmark process working set)\n");
    let _ = writeln!(md, "Private = committed memory of the benchmark process (the models are processed one after another in the same process).\n");
    let _ = writeln!(md, "| Model | Load (incl. warm-up) | Private before load | Private loaded | Peak working set during transcription | Private after unload |");
    let _ = writeln!(md, "|---|---|---|---|---|---|");
    for r in &rows {
        let _ = writeln!(md, "| {} | {} ms | {:.0} MB | {:.0} MB | {:.0} MB | {:.0} MB |", r.model, r.load_ms, r.ram_idle_mb, r.ram_loaded_mb, r.ram_peak_mb, r.ram_unloaded_mb);
    }
    let _ = writeln!(md, "\n### Latency per utterance\n");
    let _ = writeln!(md, "`partial` = preview of the first 1.5 s; `final` = full utterance transcribed at once (worst case, no streaming); CPU = average utilisation of the whole machine during the final job.\n");
    let _ = writeln!(md, "| Model | Clip | Audio | Partial | Final | CPU | WER |");
    let _ = writeln!(md, "|---|---|---|---|---|---|---|");
    for r in &rows {
        for (id, secs, p, f, c, e) in &r.per_fixture {
            let _ = writeln!(md, "| {} | {} | {:.1} s | {} ms | {} ms | {:.0}% | {:.1}% |", r.model, id, secs, p, f, c, e * 100.0);
        }
    }
    let _ = writeln!(md, "\n### Streaming dictation (real-time audio through the full pipeline)\n");
    let _ = writeln!(md, "Audio is fed at real-time speed; recording auto-stops after 0.8 s of silence. `after stop` is the delay between the end of recording and the final text.\n");
    let _ = writeln!(md, "| Model | Clip | First partial (from start) | Final text after stop |");
    let _ = writeln!(md, "|---|---|---|---|");
    for r in &rows {
        for (id, _mic, fp, fl) in &r.stream {
            let _ = writeln!(md, "| {} | {} | {} | {} ms |", r.model, id, fp.map(|v| format!("{v} ms")).unwrap_or("-".into()), fl);
        }
    }
    println!("{md}");
    if let Some(out) = arg(args, "--out") {
        std::fs::write(&out, &md).expect("write output");
        eprintln!("wrote {out}");
    }
}

fn cmd_leak(args: &[String]) {
    let id = arg(args, "--model").unwrap_or_else(|| models::DEFAULT_MODEL.into());
    let cycles: usize = arg(args, "--cycles").and_then(|s| s.parse().ok()).unwrap_or(100);
    let fdir = PathBuf::from(arg(args, "--fixtures").unwrap_or_else(|| "tests/fixtures".into()));
    let dir = models::default_models_dir();
    ensure_model(&dir, &id).unwrap();
    let engine = Arc::new(Engine::new(dir, &id, None));
    std::env::set_var("HUSHTYPE_TEST_AUDIO", fdir.join("short.wav"));
    let mut samples = Vec::new();
    for i in 0..cycles {
        let (tx, rx) = channel();
        let opts = DictationOptions { auto_stop_silence: Some(Duration::from_millis(300)), ..Default::default() };
        let d = Dictation::start(engine.clone(), opts, move |e| {
            let _ = tx.send(e);
        });
        let mut ok = false;
        while let Ok(e) = rx.recv_timeout(Duration::from_secs(60)) {
            match e {
                DictationEvent::Done(_) => {
                    ok = true;
                    break;
                }
                DictationEvent::Failed(_) | DictationEvent::NoSpeech | DictationEvent::Cancelled => break,
                _ => {}
            }
        }
        d.join();
        let m = mem();
        samples.push(m.private);
        println!("cycle {:>3}: ok={} working set {:.1} MB, private {:.1} MB", i + 1, ok, mb(m.working_set), mb(m.private));
    }
    let n = samples.len();
    if n >= 20 {
        let early: u64 = samples[5..15].iter().sum::<u64>() / 10;
        let late: u64 = samples[n - 10..].iter().sum::<u64>() / 10;
        println!("private bytes: cycles 6-15 avg {:.1} MB, last 10 avg {:.1} MB, growth {:+.2} MB", mb(early), mb(late), mb(late) - mb(early));
    }
}

fn main() {
    log::set_logger(&Logger).ok();
    log::set_max_level(log::LevelFilter::Info);
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("download") => {
            let id = args.get(2).expect("usage: download <model>");
            ensure_model(&models::default_models_dir(), id).unwrap();
        }
        Some("transcribe") => cmd_transcribe(&args),
        Some("run") => cmd_run(&args),
        Some("leak") => cmd_leak(&args),
        _ => eprintln!("usage: hushtype-bench <download|transcribe|run|leak> ..."),
    }
}

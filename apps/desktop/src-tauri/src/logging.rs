//! Small file logger. Logs contain timings and errors only — never
//! transcribed text or audio.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_BYTES: u64 = 1024 * 1024;

struct FileLogger {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl FileLogger {
    fn open(&self) -> Option<File> {
        if std::fs::metadata(&self.path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
            let _ = std::fs::rename(&self.path, self.path.with_extension("old.log"));
        }
        OpenOptions::new().create(true).append(true).open(&self.path).ok()
    }
}

impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        if m.target().starts_with("whisper_rs") {
            return m.level() <= log::Level::Warn;
        }
        m.level() <= log::Level::Info
    }

    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let msg = r.args().to_string();
        // Aborted preview jobs are expected; don't report them as errors.
        if r.target().starts_with("whisper_rs") && (msg.contains("failed to encode") || msg.contains("failed to decode")) {
            return;
        }
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let line = format!("{secs} {:<5} [{}] {}\n", r.level(), r.target(), msg.trim_end());
        if cfg!(debug_assertions) {
            eprint!("{line}");
        }
        let mut g = self.file.lock().unwrap();
        if g.is_none() {
            *g = self.open();
        }
        if let Some(f) = g.as_mut() {
            if f.write_all(line.as_bytes()).is_err() {
                *g = None;
            }
        }
    }

    fn flush(&self) {
        if let Some(f) = self.file.lock().unwrap().as_mut() {
            let _ = f.flush();
        }
    }
}

pub fn init(dir: PathBuf) {
    let logger: &'static FileLogger = Box::leak(Box::new(FileLogger { path: dir.join("hushtype.log"), file: Mutex::new(None) }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

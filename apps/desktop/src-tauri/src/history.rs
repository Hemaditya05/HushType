//! Local transcription history (JSON lines). Read from disk on demand and
//! never kept in memory.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::paths::write_atomic;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: u64,
    /// Unix time in milliseconds.
    pub timestamp: u64,
    pub app: String,
    pub raw: String,
    pub text: String,
    pub language: String,
    pub duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_file: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub entries: Vec<Entry>,
    pub total: usize,
}

pub struct History {
    path: PathBuf,
    lock: Mutex<()>,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

impl History {
    pub fn new(path: PathBuf) -> History {
        History { path, lock: Mutex::new(()) }
    }

    fn read_all(&self) -> Vec<Entry> {
        std::fs::read_to_string(&self.path)
            .map(|s| s.lines().filter_map(|l| serde_json::from_str::<Entry>(l).ok()).collect())
            .unwrap_or_default()
    }

    fn write_all(&self, entries: &[Entry]) -> std::io::Result<()> {
        let mut out = Vec::new();
        for e in entries {
            serde_json::to_writer(&mut out, e)?;
            out.push(b'\n');
        }
        write_atomic(&self.path, &out)
    }

    /// Append an entry; returns audio files of entries dropped by the limit.
    pub fn append(&self, e: &Entry, limit: usize) -> Vec<String> {
        let _g = self.lock.lock().unwrap();
        let mut line = serde_json::to_vec(e).unwrap_or_default();
        line.push(b'\n');
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.path) {
            let _ = f.write_all(&line);
        }
        // Compact occasionally rather than on every append.
        let size = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if size > 64 * 1024 {
            let all = self.read_all();
            if all.len() > limit + limit / 5 {
                let cut = all.len() - limit;
                let dropped = all[..cut].iter().filter_map(|e| e.audio_file.clone()).collect();
                let _ = self.write_all(&all[cut..]);
                return dropped;
            }
        }
        Vec::new()
    }

    pub fn page(&self, offset: usize, limit: usize, query: &str) -> Page {
        let _g = self.lock.lock().unwrap();
        let q = query.trim().to_lowercase();
        let mut all: Vec<Entry> = self
            .read_all()
            .into_iter()
            .filter(|e| q.is_empty() || e.text.to_lowercase().contains(&q) || e.app.to_lowercase().contains(&q))
            .collect();
        all.reverse();
        let total = all.len();
        let entries = all.into_iter().skip(offset).take(limit).collect();
        Page { entries, total }
    }

    pub fn delete(&self, id: u64) -> Option<Entry> {
        let _g = self.lock.lock().unwrap();
        let mut all = self.read_all();
        let pos = all.iter().position(|e| e.id == id)?;
        let removed = all.remove(pos);
        let _ = self.write_all(&all);
        Some(removed)
    }

    pub fn clear(&self) -> Vec<String> {
        let _g = self.lock.lock().unwrap();
        let files = self.read_all().into_iter().filter_map(|e| e.audio_file).collect();
        let _ = std::fs::remove_file(&self.path);
        files
    }
}

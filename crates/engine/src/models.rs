//! Whisper model catalog, local cache and verified downloads.
//!
//! Models are whisper.cpp GGML files (MIT licensed, converted from OpenAI's
//! Whisper weights). Quantized variants are used by default: they are 2-3x
//! smaller in memory with a negligible accuracy cost.

use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::EngineError;

#[derive(Debug, Clone, Serialize)]
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub file: &'static str,
    pub size_bytes: u64,
    #[serde(skip)]
    pub sha256: &'static str,
    pub english_only: bool,
    /// Approximate resident memory once loaded (measured on x64, CPU build).
    pub ram_mb: u64,
    /// 1 (fastest) .. 5 (slowest)
    pub speed: u8,
    /// 1 (lowest) .. 5 (best)
    pub accuracy: u8,
    pub description: &'static str,
}

pub const DEFAULT_BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

pub const CATALOG: &[ModelInfo] = &[
    ModelInfo {
        id: "tiny.en",
        name: "Tiny (English)",
        file: "ggml-tiny.en-q5_1.bin",
        size_bytes: 32_166_155,
        sha256: "c77c5766f1cef09b6b7d47f21b546cbddd4157886b3b5d6d4f709e91e66c7c2b",
        english_only: true,
        ram_mb: 90,
        speed: 1,
        accuracy: 1,
        description: "Fastest and smallest. Fine for short, clear English dictation on older PCs.",
    },
    ModelInfo {
        id: "tiny",
        name: "Tiny (Multilingual)",
        file: "ggml-tiny-q5_1.bin",
        size_bytes: 32_152_673,
        sha256: "818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7",
        english_only: false,
        ram_mb: 90,
        speed: 1,
        accuracy: 1,
        description: "Fastest multilingual model. Lower accuracy.",
    },
    ModelInfo {
        id: "base.en",
        name: "Base (English)",
        file: "ggml-base.en-q5_1.bin",
        size_bytes: 59_721_011,
        sha256: "4baf70dd0d7c4247ba2b81fafd9c01005ac77c2f9ef064e00dcf195d0e2fdd2f",
        english_only: true,
        ram_mb: 150,
        speed: 2,
        accuracy: 3,
        description: "Recommended for most laptops: near-instant and accurate for everyday English.",
    },
    ModelInfo {
        id: "base",
        name: "Base (Multilingual)",
        file: "ggml-base-q5_1.bin",
        size_bytes: 59_707_625,
        sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
        english_only: false,
        ram_mb: 150,
        speed: 2,
        accuracy: 2,
        description: "Fast, supports 99 languages with automatic language detection.",
    },
    ModelInfo {
        id: "small.en",
        name: "Small (English)",
        file: "ggml-small.en-q5_1.bin",
        size_bytes: 190_098_681,
        sha256: "bfdff4894dcb76bbf647d56263ea2a96645423f1669176f4844a1bf8e478ad30",
        english_only: true,
        ram_mb: 330,
        speed: 3,
        accuracy: 4,
        description: "Noticeably more accurate, especially with accents and technical words. ~3x slower than Base.",
    },
    ModelInfo {
        id: "small",
        name: "Small (Multilingual)",
        file: "ggml-small-q5_1.bin",
        size_bytes: 190_085_487,
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        english_only: false,
        ram_mb: 330,
        speed: 3,
        accuracy: 3,
        description: "Good multilingual accuracy. Best choice for non-English dictation on CPU.",
    },
    ModelInfo {
        id: "medium.en",
        name: "Medium (English)",
        file: "ggml-medium.en-q5_0.bin",
        size_bytes: 539_225_533,
        sha256: "76733e26ad8fe1c7a5bf7531a9d41917b2adc0f20f2e4f5531688a8c6cd88eb0",
        english_only: true,
        ram_mb: 800,
        speed: 5,
        accuracy: 5,
        description: "Highest accuracy. Needs a fast CPU or a GPU build; slow on typical laptops.",
    },
    ModelInfo {
        id: "medium",
        name: "Medium (Multilingual)",
        file: "ggml-medium-q5_0.bin",
        size_bytes: 539_212_467,
        sha256: "19fea4b380c3a618ec4723c3eef2eb785ffba0d0538cf43f8f235e7b3b34220f",
        english_only: false,
        ram_mb: 800,
        speed: 5,
        accuracy: 5,
        description: "Highest multilingual accuracy. Needs a fast CPU or a GPU build.",
    },
];

pub const DEFAULT_MODEL: &str = "base.en";

/// Per-user model cache: %LOCALAPPDATA%\HushType\models on Windows.
pub fn default_models_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("HushType").join("models")
}

pub fn find(id: &str) -> Option<&'static ModelInfo> {
    CATALOG.iter().find(|m| m.id == id)
}

pub fn model_path(models_dir: &Path, id: &str) -> Option<PathBuf> {
    find(id).map(|m| models_dir.join(m.file))
}

pub fn is_installed(models_dir: &Path, id: &str) -> bool {
    match find(id) {
        Some(m) => fs::metadata(models_dir.join(m.file)).map(|md| md.len() == m.size_bytes).unwrap_or(false),
        None => false,
    }
}

pub fn delete(models_dir: &Path, id: &str) -> std::io::Result<()> {
    if let Some(m) = find(id) {
        for p in [models_dir.join(m.file), models_dir.join(format!("{}.part", m.file))] {
            if p.exists() {
                fs::remove_file(p)?;
            }
        }
    }
    Ok(())
}

/// Suggest a default model for this machine.
pub fn recommended(total_ram: Option<u64>, logical_cores: usize, has_gpu_backend: bool) -> &'static str {
    let ram_gb = total_ram.map(|b| b / (1 << 30)).unwrap_or(8);
    if has_gpu_backend && ram_gb >= 8 {
        "small.en"
    } else if ram_gb < 4 || logical_cores < 4 {
        "tiny.en"
    } else {
        DEFAULT_MODEL
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
    pub bytes_per_sec: u64,
}

fn base_url() -> String {
    std::env::var("HUSHTYPE_MODEL_MIRROR").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| DEFAULT_BASE_URL.into())
}

/// Download (or resume) a model into `models_dir`, verifying its SHA-256.
/// `progress` is called at most ~5 times per second. Set `cancel` to abort.
pub fn download(
    models_dir: &Path,
    id: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<PathBuf, EngineError> {
    let m = find(id).ok_or_else(|| EngineError::Download(format!("unknown model {id}")))?;
    fs::create_dir_all(models_dir).map_err(|e| EngineError::Download(format!("cannot create models folder: {e}")))?;
    let final_path = models_dir.join(m.file);
    let part_path = models_dir.join(format!("{}.part", m.file));
    let url = format!("{}/{}", base_url(), m.file);

    let mut hasher = Sha256::new();
    let mut have: u64 = 0;
    if let Ok(mut f) = fs::File::open(&part_path) {
        // Resume: re-hash what we already have.
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = f.read(&mut buf).map_err(|e| EngineError::Download(e.to_string()))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            have += n as u64;
        }
        if have > m.size_bytes {
            have = 0;
            hasher = Sha256::new();
            let _ = fs::remove_file(&part_path);
        }
    }

    let tls = ureq::tls::TlsConfig::builder()
        .provider(ureq::tls::TlsProvider::NativeTls)
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_body(None)
        .timeout_global(None)
        .http_status_as_error(false)
        .build()
        .into();
    let mut req = agent.get(&url).header("User-Agent", concat!("HushType/", env!("CARGO_PKG_VERSION")));
    if have > 0 && have < m.size_bytes {
        req = req.header("Range", &format!("bytes={have}-"));
    }
    let resp = if have == m.size_bytes {
        None
    } else {
        Some(req.call().map_err(|e| EngineError::Download(friendly_net_error(&e.to_string())))?)
    };

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(false)
        .write(true)
        .truncate(false)
        .open(&part_path)
        .map_err(|e| EngineError::Download(format!("cannot write model file: {e}")))?;

    if let Some(resp) = resp {
        let status = resp.status().as_u16();
        match status {
            206 => {}
            200 => {
                // Server ignored the range: start over.
                have = 0;
                hasher = Sha256::new();
                file.set_len(0).map_err(|e| EngineError::Download(e.to_string()))?;
            }
            404 => return Err(EngineError::Download("model not found on the server".into())),
            s => return Err(EngineError::Download(format!("server returned HTTP {s}"))),
        }
        file.seek(SeekFrom::Start(have)).map_err(|e| EngineError::Download(e.to_string()))?;
        let mut body = resp.into_body();
        let mut reader = body.as_reader();
        let mut buf = vec![0u8; 256 * 1024];
        let started = Instant::now();
        let start_bytes = have;
        let mut last_report = Instant::now() - Duration::from_secs(1);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(EngineError::Download("cancelled".into()));
            }
            let n = reader.read(&mut buf).map_err(|e| EngineError::Download(friendly_net_error(&e.to_string())))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| EngineError::Download(format!("disk write failed: {e}")))?;
            hasher.update(&buf[..n]);
            have += n as u64;
            if last_report.elapsed() >= Duration::from_millis(200) {
                last_report = Instant::now();
                let secs = started.elapsed().as_secs_f64().max(0.001);
                progress(Progress {
                    downloaded: have,
                    total: m.size_bytes,
                    bytes_per_sec: ((have - start_bytes) as f64 / secs) as u64,
                });
            }
        }
        file.flush().map_err(|e| EngineError::Download(e.to_string()))?;
    }
    drop(file);
    progress(Progress { downloaded: have, total: m.size_bytes, bytes_per_sec: 0 });

    if have != m.size_bytes {
        return Err(EngineError::Download("the connection closed early; try again to resume".into()));
    }
    let digest: String = hasher.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if digest != m.sha256 {
        let _ = fs::remove_file(&part_path);
        return Err(EngineError::Download("the downloaded file is corrupted (checksum mismatch)".into()));
    }
    fs::rename(&part_path, &final_path).map_err(|e| EngineError::Download(e.to_string()))?;
    Ok(final_path)
}

fn friendly_net_error(e: &str) -> String {
    let l = e.to_lowercase();
    if l.contains("dns") || l.contains("resolve") || l.contains("no such host") {
        "no internet connection (could not reach huggingface.co)".into()
    } else if l.contains("timed out") || l.contains("timeout") {
        "the connection timed out".into()
    } else if l.contains("certificate") || l.contains("tls") {
        format!("secure connection failed ({e})")
    } else {
        e.to_string()
    }
}

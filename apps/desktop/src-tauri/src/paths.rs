use std::path::PathBuf;

/// Per-user locations. Nothing is written outside these folders.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Paths {
    /// %APPDATA%\HushType — settings.json, dictionary.json
    pub config: PathBuf,
    /// %LOCALAPPDATA%\HushType — history, logs, recordings, models
    pub data: PathBuf,
    pub models: PathBuf,
    pub logs: PathBuf,
    pub recordings: PathBuf,
}

impl Paths {
    pub fn new() -> Paths {
        let models = hushtype_engine::models::default_models_dir();
        // HUSHTYPE_HOME relocates settings/history (used by the test harness);
        // models stay shared.
        let (config, data) = match std::env::var_os("HUSHTYPE_HOME") {
            Some(home) => (PathBuf::from(&home).join("config"), PathBuf::from(&home).join("data")),
            None => {
                let roaming = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
                (roaming.join("HushType"), models.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from(".")))
            }
        };
        let p = Paths { logs: data.join("logs"), recordings: data.join("recordings"), config, data, models };
        for d in [&p.config, &p.data, &p.models, &p.logs] {
            let _ = std::fs::create_dir_all(d);
        }
        p
    }

    pub fn settings(&self) -> PathBuf {
        self.config.join("settings.json")
    }

    pub fn dictionary(&self) -> PathBuf {
        self.config.join("dictionary.json")
    }

    pub fn history(&self) -> PathBuf {
        self.data.join("history.jsonl")
    }
}

/// Write via a temp file + rename so a crash never leaves a truncated file.
pub fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

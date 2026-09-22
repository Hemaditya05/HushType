use std::fmt;

/// Errors surfaced to the user. `Display` is written for non-technical users;
/// technical details go to the log via `detail()`.
#[derive(Debug, Clone)]
pub enum EngineError {
    MicNotFound,
    MicPermissionDenied,
    MicBusy,
    MicFailed(String),
    ModelMissing(String),
    ModelLoad(String),
    NotEnoughMemory { needed_mb: u64, available_mb: u64 },
    Download(String),
    Transcription(String),
    UnsupportedCpu,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::MicNotFound => {
                write!(f, "No microphone found. Plug one in or pick another one in Settings > Microphone.")
            }
            EngineError::MicPermissionDenied => write!(
                f,
                "Windows is blocking microphone access. Turn on \"Let desktop apps access your microphone\" in Windows Settings > Privacy & security > Microphone."
            ),
            EngineError::MicBusy => {
                write!(f, "The microphone is being used exclusively by another app. Close that app and try again.")
            }
            EngineError::MicFailed(_) => {
                write!(f, "The microphone could not be started. Try another device in Settings > Microphone.")
            }
            EngineError::ModelMissing(name) => {
                write!(f, "The speech model \"{name}\" is not installed. Download it in Models.")
            }
            EngineError::ModelLoad(_) => write!(
                f,
                "The speech model could not be loaded. It may be damaged; delete it and download it again in Models."
            ),
            EngineError::NotEnoughMemory { needed_mb, available_mb } => write!(
                f,
                "Not enough free memory to load this model (needs about {needed_mb} MB, {available_mb} MB free). Close some apps or choose a smaller model."
            ),
            EngineError::Download(msg) => write!(f, "Model download failed: {msg}"),
            EngineError::Transcription(_) => write!(f, "Transcription failed. Please try again."),
            EngineError::UnsupportedCpu => {
                write!(f, "This processor lacks AVX2 support, which the speech engine requires.")
            }
        }
    }
}

impl EngineError {
    /// Technical detail for logs (never contains transcribed speech).
    pub fn detail(&self) -> String {
        match self {
            EngineError::MicFailed(d) | EngineError::ModelLoad(d) | EngineError::Transcription(d) => {
                format!("{}: {d}", self.code())
            }
            other => format!("{other:?}"),
        }
    }

    /// Short machine-readable code for the UI.
    pub fn code(&self) -> &'static str {
        match self {
            EngineError::MicNotFound => "mic_not_found",
            EngineError::MicPermissionDenied => "mic_permission",
            EngineError::MicBusy => "mic_busy",
            EngineError::MicFailed(_) => "mic_failed",
            EngineError::ModelMissing(_) => "model_missing",
            EngineError::ModelLoad(_) => "model_load",
            EngineError::NotEnoughMemory { .. } => "low_memory",
            EngineError::Download(_) => "download",
            EngineError::Transcription(_) => "transcription",
            EngineError::UnsupportedCpu => "unsupported_cpu",
        }
    }
}

impl std::error::Error for EngineError {}

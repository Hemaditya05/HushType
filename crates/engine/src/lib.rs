//! HushType speech engine: local microphone capture, voice activity
//! detection and whisper.cpp transcription. No audio ever leaves the process.

pub mod audio;
pub mod dictation;
pub mod error;
pub mod models;
pub mod resample;
pub mod sys;
pub mod transcriber;
pub mod vad;
pub mod wav;

pub use dictation::{Dictation, DictationEvent, DictationOptions, DictationResult};
pub use error::EngineError;
pub use transcriber::{Engine, ModelStatus, Request, Transcript};

/// Return freed pages to the OS so idle memory drops after a model unload.
pub(crate) fn sys_trim_memory() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
        use windows::Win32::System::Threading::GetCurrentProcess;
        let _ = EmptyWorkingSet(GetCurrentProcess());
    }
}

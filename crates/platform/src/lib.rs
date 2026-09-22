//! Operating-system integration. Windows is the primary target; the public
//! API is platform neutral so other backends can be added later.

mod hotkey_parse;
pub use hotkey_parse::{parse_hotkey, Hotkey};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use crate::windows::*;

#[cfg(not(windows))]
mod unsupported;
#[cfg(not(windows))]
pub use crate::unsupported::*;

/// How dictated line breaks are typed into the target app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewlineMode {
    /// Plain Enter.
    Enter,
    /// Shift+Enter (chat apps where Enter sends the message).
    ShiftEnter,
    /// Replace line breaks with spaces (terminals: never press Enter).
    Space,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum InsertMethod {
    /// Paste (restoring the clipboard); type into terminals.
    #[default]
    Auto,
    /// Always simulate typing (Unicode key events).
    Type,
    /// Always paste via the clipboard (the clipboard is restored afterwards).
    Paste,
}

#[derive(Debug, Clone)]
pub struct InsertOptions {
    pub method: InsertMethod,
    pub newline: NewlineMode,
    /// Paste with Shift+Insert instead of Ctrl+V (terminals).
    pub terminal_paste: bool,
    pub restore_clipboard: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InsertOutcome {
    Typed,
    Pasted,
    /// No text field could receive the text; it was left on the clipboard.
    Copied,
}

#[derive(Debug, Clone)]
pub enum InsertError {
    /// The focused window belongs to an administrator process; Windows blocks
    /// input from normal apps. The text was copied to the clipboard.
    TargetElevated,
    Clipboard(String),
    Input(String),
}

impl std::fmt::Display for InsertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InsertError::TargetElevated => write!(
                f,
                "Windows doesn't allow typing into apps running as administrator. The text is on your clipboard; press Ctrl+V."
            ),
            InsertError::Clipboard(_) => write!(f, "The clipboard is busy (another app is using it). Please try again."),
            InsertError::Input(_) => write!(f, "The text could not be typed into the active app."),
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ForegroundApp {
    /// Executable file name, e.g. "Code.exe".
    pub exe: String,
    /// Window title. Used transiently for classification only; never stored.
    #[serde(skip)]
    pub title: String,
    pub pid: u32,
    pub elevated: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub dedicated_vram_mb: u64,
    pub integrated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
    /// Escape pressed while a recording is active.
    Cancel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IndicatorState {
    Hidden,
    /// Listening, with an optional live preview of the transcription.
    Listening(Option<String>),
    Processing,
    Success(String),
    Error(String),
    Info(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Start,
    Stop,
    Error,
}

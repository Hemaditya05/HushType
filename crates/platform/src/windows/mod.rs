mod clipboard;
mod foreground;
mod hotkey;
mod indicator;
mod input;
mod sound;
mod system;

pub use clipboard::{copy_text, read_text};
pub use foreground::foreground_app;
pub use hotkey::HotkeyManager;
pub use indicator::Indicator;
pub use input::insert_text;
pub use sound::play;
pub use system::{autostart_enabled, gpus, is_elevated, open_external, process_memory, set_autostart, trim_memory};

pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

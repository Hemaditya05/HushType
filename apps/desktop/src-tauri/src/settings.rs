use serde::{Deserialize, Serialize};

use hushtype_platform::InsertMethod;

use crate::paths::{write_atomic, Paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HotkeyMode {
    /// Hold to talk, release to insert.
    Hold,
    /// Press to start, press again (or pause) to insert.
    Toggle,
    /// Hold for push-to-talk, or tap once for hands-free.
    Hybrid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    // General
    pub launch_at_startup: bool,
    pub start_minimized: bool,
    pub show_indicator: bool,
    pub indicator_top: bool,
    pub play_sounds: bool,
    /// "system" | "light" | "dark"
    pub theme: String,

    // Speech
    pub model: String,
    /// ISO-639-1 code, or "auto" for automatic detection.
    pub language: String,
    /// Device name; empty = system default.
    pub microphone: String,
    pub silence_timeout_ms: u32,
    /// 0..100
    pub vad_sensitivity: u32,
    pub live_preview: bool,
    /// Minutes of inactivity before the model is unloaded; 0 = never.
    pub unload_after_min: u32,
    pub load_model_at_startup: bool,
    pub max_recording_sec: u32,

    // Text
    pub remove_fillers: bool,
    pub smart_punctuation: bool,
    pub auto_capitalize: bool,
    pub spoken_punctuation: bool,
    pub aggressive_cleanup: bool,
    pub context_aware: bool,
    pub insert_method: InsertMethod,
    pub restore_clipboard: bool,

    // Shortcut
    pub hotkey: String,
    pub hotkey_mode: HotkeyMode,

    // Privacy
    pub save_history: bool,
    pub store_audio: bool,
    pub history_limit: u32,

    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            launch_at_startup: false,
            start_minimized: false,
            show_indicator: true,
            indicator_top: false,
            play_sounds: true,
            theme: "system".into(),
            model: hushtype_engine::models::DEFAULT_MODEL.into(),
            language: "en".into(),
            microphone: String::new(),
            silence_timeout_ms: 2000,
            vad_sensitivity: 50,
            live_preview: true,
            unload_after_min: 15,
            load_model_at_startup: false,
            max_recording_sec: 300,
            remove_fillers: true,
            smart_punctuation: true,
            auto_capitalize: true,
            spoken_punctuation: true,
            aggressive_cleanup: false,
            context_aware: true,
            insert_method: InsertMethod::Auto,
            restore_clipboard: true,
            hotkey: "Ctrl+Shift+Space".into(),
            hotkey_mode: HotkeyMode::Hybrid,
            save_history: true,
            store_audio: false,
            history_limit: 500,
            onboarded: false,
        }
    }
}

impl Settings {
    pub fn load(paths: &Paths) -> Settings {
        match std::fs::read_to_string(paths.settings()) {
            Ok(s) => serde_json::from_str::<Settings>(&s).unwrap_or_else(|e| {
                log::warn!("settings.json unreadable ({e}); using defaults");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
        .sanitized()
    }

    pub fn save(&self, paths: &Paths) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        write_atomic(&paths.settings(), &json).map_err(|e| format!("could not save settings: {e}"))
    }

    /// Clamp values that come from the UI or a hand-edited file.
    pub fn sanitized(mut self) -> Settings {
        self.silence_timeout_ms = self.silence_timeout_ms.clamp(500, 10_000);
        self.vad_sensitivity = self.vad_sensitivity.min(100);
        self.max_recording_sec = self.max_recording_sec.clamp(10, 1800);
        self.history_limit = self.history_limit.clamp(10, 10_000);
        if !matches!(self.unload_after_min, 0 | 5 | 15 | 30 | 60) {
            self.unload_after_min = 15;
        }
        if hushtype_engine::models::find(&self.model).is_none() {
            self.model = hushtype_engine::models::DEFAULT_MODEL.into();
        }
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            self.theme = "system".into();
        }
        if self.language.trim().is_empty() {
            self.language = "en".into();
        }
        self
    }

    pub fn idle_unload(&self) -> Option<std::time::Duration> {
        // Test/profiling override in seconds.
        if let Some(secs) = std::env::var("HUSHTYPE_UNLOAD_SECS").ok().and_then(|s| s.parse::<u64>().ok()) {
            return Some(std::time::Duration::from_secs(secs));
        }
        (self.unload_after_min > 0).then(|| std::time::Duration::from_secs(self.unload_after_min as u64 * 60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_persist_across_restart() {
        let dir = std::env::temp_dir().join(format!("hushtype-settings-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths = Paths {
            config: dir.clone(),
            data: dir.clone(),
            models: dir.clone(),
            logs: dir.clone(),
            recordings: dir.join("rec"),
        };
        let mut s = Settings::default();
        s.hotkey = "Alt+F9".into();
        s.hotkey_mode = HotkeyMode::Toggle;
        s.model = "small.en".into();
        s.unload_after_min = 30;
        s.save_history = false;
        s.save(&paths).unwrap();
        let loaded = Settings::load(&paths);
        assert_eq!(loaded.hotkey, "Alt+F9");
        assert_eq!(loaded.hotkey_mode, HotkeyMode::Toggle);
        assert_eq!(loaded.model, "small.en");
        assert_eq!(loaded.unload_after_min, 30);
        assert!(!loaded.save_history);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn invalid_values_are_sanitized() {
        let s = Settings { unload_after_min: 7, model: "nope".into(), silence_timeout_ms: 1, ..Default::default() }.sanitized();
        assert_eq!(s.unload_after_min, 15);
        assert_eq!(s.model, hushtype_engine::models::DEFAULT_MODEL);
        assert_eq!(s.silence_timeout_ms, 500);
    }
}

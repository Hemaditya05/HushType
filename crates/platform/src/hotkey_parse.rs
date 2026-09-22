//! Parsing and validation of shortcut strings such as "Ctrl+Shift+Space".
//! Key codes are Windows virtual-key codes, which is also what the UI records.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    /// Windows virtual-key code of the main key.
    pub vk: u32,
}

fn key_name(vk: u32) -> String {
    match vk {
        0x20 => "Space".into(),
        0x09 => "Tab".into(),
        0x0D => "Enter".into(),
        0x08 => "Backspace".into(),
        0x13 => "Pause".into(),
        0x14 => "CapsLock".into(),
        0x91 => "ScrollLock".into(),
        0x2D => "Insert".into(),
        0x2E => "Delete".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        0x21 => "PageUp".into(),
        0x22 => "PageDown".into(),
        0x25 => "Left".into(),
        0x26 => "Up".into(),
        0x27 => "Right".into(),
        0x28 => "Down".into(),
        0xC0 => "`".into(),
        0xBD => "-".into(),
        0xBB => "=".into(),
        0xDB => "[".into(),
        0xDD => "]".into(),
        0xDC => "\\".into(),
        0xBA => ";".into(),
        0xDE => "'".into(),
        0xBC => ",".into(),
        0xBE => ".".into(),
        0xBF => "/".into(),
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).unwrap().to_string(),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        other => format!("0x{other:02X}"),
    }
}

fn key_code(name: &str) -> Option<u32> {
    let n = name.to_ascii_lowercase();
    let vk = match n.as_str() {
        "space" => 0x20,
        "tab" => 0x09,
        "enter" | "return" => 0x0D,
        "backspace" => 0x08,
        "pause" => 0x13,
        "capslock" => 0x14,
        "scrolllock" => 0x91,
        "insert" | "ins" => 0x2D,
        "delete" | "del" => 0x2E,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "`" | "backquote" => 0xC0,
        "-" | "minus" => 0xBD,
        "=" | "equal" => 0xBB,
        "[" => 0xDB,
        "]" => 0xDD,
        "\\" => 0xDC,
        ";" => 0xBA,
        "'" => 0xDE,
        "," => 0xBC,
        "." => 0xBE,
        "/" => 0xBF,
        _ => {
            if n.len() == 1 {
                let c = n.chars().next().unwrap().to_ascii_uppercase();
                if c.is_ascii_alphanumeric() {
                    return Some(c as u32);
                }
            }
            if let Some(num) = n.strip_prefix("num") {
                return num.parse::<u32>().ok().filter(|d| *d <= 9).map(|d| 0x60 + d);
            }
            if let Some(f) = n.strip_prefix('f') {
                return f.parse::<u32>().ok().filter(|d| (1..=24).contains(d)).map(|d| 0x6F + d);
            }
            if let Some(hex) = n.strip_prefix("0x") {
                return u32::from_str_radix(hex, 16).ok();
            }
            return None;
        }
    };
    Some(vk)
}

impl Hotkey {
    pub fn label(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if self.shift {
            parts.push("Shift".into());
        }
        if self.win {
            parts.push("Win".into());
        }
        parts.push(key_name(self.vk));
        parts.join("+")
    }

    fn modifier_count(&self) -> usize {
        [self.ctrl, self.alt, self.shift, self.win].iter().filter(|b| **b).count()
    }

    /// Reasons this shortcut is a bad idea, if any (known system/app conflicts).
    pub fn conflict_warning(&self) -> Option<String> {
        let l = self.label();
        let reserved: &[(&str, &str)] = &[
            ("Ctrl+C", "Copy"),
            ("Ctrl+V", "Paste"),
            ("Ctrl+X", "Cut"),
            ("Ctrl+Z", "Undo"),
            ("Ctrl+Y", "Redo"),
            ("Ctrl+A", "Select all"),
            ("Ctrl+S", "Save"),
            ("Ctrl+F", "Find"),
            ("Ctrl+Space", "code completion in editors"),
            ("Alt+Tab", "app switching"),
            ("Alt+F4", "closing windows"),
            ("Ctrl+Shift+Escape", "Task Manager"),
            ("Win+Space", "switching keyboard layouts"),
            ("Win+H", "Windows voice typing"),
            ("Win+L", "locking the PC"),
            ("Win+D", "showing the desktop"),
            ("Win+V", "clipboard history"),
            ("Ctrl+Shift+V", "paste as plain text"),
            ("Ctrl+Shift+P", "the command palette in VS Code"),
            ("Ctrl+Shift+T", "reopening closed tabs"),
            ("Ctrl+Shift+N", "new private window / new folder"),
        ];
        if let Some((_, what)) = reserved.iter().find(|(k, _)| *k == l) {
            return Some(format!("{l} is normally used for {what}."));
        }
        None
    }
}

/// Parse "Ctrl+Shift+Space". Requires at least one modifier unless the key
/// is a function key F13-F24 or Pause/ScrollLock.
pub fn parse_hotkey(s: &str) -> Result<Hotkey, String> {
    let mut hk = Hotkey { ctrl: false, alt: false, shift: false, win: false, vk: 0 };
    for part in s.split('+').map(|p| p.trim()).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => hk.ctrl = true,
            "alt" | "option" => hk.alt = true,
            "shift" => hk.shift = true,
            "win" | "super" | "meta" | "cmd" => hk.win = true,
            other => {
                if hk.vk != 0 {
                    return Err(format!("\"{s}\" has more than one main key"));
                }
                hk.vk = key_code(other).ok_or_else(|| format!("unknown key \"{part}\""))?;
            }
        }
    }
    // "Ctrl++" style: a trailing '+' is the key itself.
    if hk.vk == 0 && s.trim_end().ends_with("++") {
        hk.vk = 0xBB;
    }
    if hk.vk == 0 {
        return Err("choose a key to go with the modifiers".into());
    }
    let standalone_ok = matches!(hk.vk, 0x7C..=0x87 | 0x13 | 0x91);
    if hk.modifier_count() == 0 && !standalone_ok {
        return Err("add at least one modifier (Ctrl, Alt, Shift or Win) so normal typing isn't affected".into());
    }
    if hk.modifier_count() == 1 && hk.shift && !standalone_ok && !(0x70..=0x87).contains(&hk.vk) {
        return Err("Shift alone changes normal typing; add Ctrl, Alt or Win".into());
    }
    Ok(hk)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_label() {
        let h = parse_hotkey("Ctrl+Shift+Space").unwrap();
        assert!(h.ctrl && h.shift && !h.alt && h.vk == 0x20);
        assert_eq!(h.label(), "Ctrl+Shift+Space");
        assert_eq!(parse_hotkey("alt + f9").unwrap().label(), "Alt+F9");
        assert_eq!(parse_hotkey("F13").unwrap().label(), "F13");
        assert_eq!(parse_hotkey("Win+Ctrl+H").unwrap().label(), "Ctrl+Win+H");
    }

    #[test]
    fn rejects_bad_shortcuts() {
        assert!(parse_hotkey("A").is_err());
        assert!(parse_hotkey("Shift+A").is_err());
        assert!(parse_hotkey("Ctrl+Shift").is_err());
        assert!(parse_hotkey("Ctrl+Foo").is_err());
        assert!(parse_hotkey("Ctrl+A+B").is_err());
    }

    #[test]
    fn warns_about_known_conflicts() {
        assert!(parse_hotkey("Ctrl+C").unwrap().conflict_warning().is_some());
        assert!(parse_hotkey("Ctrl+Shift+Space").unwrap().conflict_warning().is_none());
    }
}

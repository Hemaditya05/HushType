//! Map the focused application to a formatting context. Used only to adjust
//! formatting (e.g. no trailing period in terminals) — never to act on text.

use hushtype_platform::{ForegroundApp, NewlineMode};
use hushtype_text::AppContext;

pub struct AppInfo {
    pub name: String,
    pub context: AppContext,
}

const TERMINALS: &[(&str, &str)] = &[
    ("windowsterminal.exe", "Windows Terminal"),
    ("wt.exe", "Windows Terminal"),
    ("openconsole.exe", "Windows Terminal"),
    ("cmd.exe", "Command Prompt"),
    ("powershell.exe", "PowerShell"),
    ("pwsh.exe", "PowerShell"),
    ("conhost.exe", "Console"),
    ("mintty.exe", "Git Bash"),
    ("bash.exe", "Bash"),
    ("wsl.exe", "WSL"),
    ("alacritty.exe", "Alacritty"),
    ("wezterm-gui.exe", "WezTerm"),
    ("putty.exe", "PuTTY"),
    ("kitty.exe", "kitty"),
    ("hyper.exe", "Hyper"),
    ("tabby.exe", "Tabby"),
    ("warp.exe", "Warp"),
];

const EDITORS: &[(&str, &str)] = &[
    ("code.exe", "VS Code"),
    ("code - insiders.exe", "VS Code Insiders"),
    ("vscodium.exe", "VSCodium"),
    ("cursor.exe", "Cursor"),
    ("windsurf.exe", "Windsurf"),
    ("zed.exe", "Zed"),
    ("devenv.exe", "Visual Studio"),
    ("idea64.exe", "IntelliJ IDEA"),
    ("pycharm64.exe", "PyCharm"),
    ("webstorm64.exe", "WebStorm"),
    ("rider64.exe", "Rider"),
    ("clion64.exe", "CLion"),
    ("goland64.exe", "GoLand"),
    ("rustrover64.exe", "RustRover"),
    ("studio64.exe", "Android Studio"),
    ("sublime_text.exe", "Sublime Text"),
    ("notepad++.exe", "Notepad++"),
];

const CHAT: &[(&str, &str)] = &[
    ("discord.exe", "Discord"),
    ("slack.exe", "Slack"),
    ("whatsapp.exe", "WhatsApp"),
    ("whatsapp.root.exe", "WhatsApp"),
    ("ms-teams.exe", "Microsoft Teams"),
    ("teams.exe", "Microsoft Teams"),
    ("telegram.exe", "Telegram"),
    ("signal.exe", "Signal"),
    ("element.exe", "Element"),
    ("zoom.exe", "Zoom"),
    ("messenger.exe", "Messenger"),
];

const DOCUMENTS: &[(&str, &str)] = &[
    ("winword.exe", "Word"),
    ("notepad.exe", "Notepad"),
    ("wordpad.exe", "WordPad"),
    ("outlook.exe", "Outlook"),
    ("olk.exe", "Outlook"),
    ("onenote.exe", "OneNote"),
    ("soffice.bin", "LibreOffice"),
    ("obsidian.exe", "Obsidian"),
    ("notion.exe", "Notion"),
    ("excel.exe", "Excel"),
    ("powerpnt.exe", "PowerPoint"),
];

const BROWSERS: &[(&str, &str)] = &[
    ("chrome.exe", "Chrome"),
    ("msedge.exe", "Edge"),
    ("firefox.exe", "Firefox"),
    ("brave.exe", "Brave"),
    ("opera.exe", "Opera"),
    ("vivaldi.exe", "Vivaldi"),
    ("arc.exe", "Arc"),
];

/// Web chat apps detected from the browser tab title.
const WEB_CHAT_TITLES: &[&str] = &["WhatsApp", "Discord", "Slack", "Messenger", "Telegram", "Microsoft Teams", "Google Chat"];

fn lookup(table: &[(&str, &str)], exe: &str) -> Option<String> {
    table.iter().find(|(e, _)| *e == exe).map(|(_, n)| n.to_string())
}

pub fn classify(app: Option<&ForegroundApp>) -> AppInfo {
    let Some(app) = app else { return AppInfo { name: "Unknown".into(), context: AppContext::General } };
    let exe = app.exe.to_lowercase();
    if exe == "hushtype.exe" {
        return AppInfo { name: "HushType".into(), context: AppContext::General };
    }
    if let Some(n) = lookup(TERMINALS, &exe) {
        return AppInfo { name: n, context: AppContext::Terminal };
    }
    if let Some(n) = lookup(EDITORS, &exe) {
        return AppInfo { name: n, context: AppContext::Code };
    }
    if let Some(n) = lookup(CHAT, &exe) {
        return AppInfo { name: n, context: AppContext::Chat };
    }
    if let Some(n) = lookup(DOCUMENTS, &exe) {
        return AppInfo { name: n, context: AppContext::Document };
    }
    if let Some(n) = lookup(BROWSERS, &exe) {
        if let Some(site) = WEB_CHAT_TITLES.iter().find(|t| app.title.contains(*t)) {
            return AppInfo { name: format!("{site} ({n})"), context: AppContext::Chat };
        }
        return AppInfo { name: n, context: AppContext::Browser };
    }
    let stem = app.exe.trim_end_matches(".exe").trim_end_matches(".EXE");
    AppInfo { name: if stem.is_empty() { "Unknown".into() } else { stem.to_string() }, context: AppContext::General }
}

pub fn newline_mode(ctx: AppContext) -> NewlineMode {
    match ctx {
        AppContext::Terminal => NewlineMode::Space,
        AppContext::Chat => NewlineMode::ShiftEnter,
        _ => NewlineMode::Enter,
    }
}

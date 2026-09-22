//! Removal of recogniser artifacts: non-speech tags and the well-known Whisper
//! hallucinations on near-silent audio.

const NON_SPEECH: &[&str] = &[
    "music", "applause", "laughter", "laughs", "silence", "inaudible", "blank_audio", "blank audio", "noise", "sound",
    "upbeat music", "background noise", "coughs", "cough", "sighs", "clears throat", "static", "beep", "wind", "typing",
    "keyboard clicking", "no speech", "speaking in foreign language", "foreign language",
];

/// Strip "[BLANK_AUDIO]", "(music)", "*laughs*", "♪" and similar tags.
pub fn strip_artifacts(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let close = match c {
            '[' => Some(']'),
            '(' => Some(')'),
            '*' => Some('*'),
            _ => None,
        };
        if let Some(close) = close {
            if let Some(len) = chars[i + 1..].iter().position(|x| *x == close) {
                let inner: String = chars[i + 1..i + 1 + len].iter().collect::<String>().trim().to_lowercase();
                let is_tag = c == '[' || NON_SPEECH.iter().any(|t| inner == *t || inner.trim_end_matches('s') == *t);
                if is_tag {
                    i += len + 2;
                    continue;
                }
            }
        }
        if c == '♪' || c == '♫' {
            i += 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

const HALLUCINATIONS: &[&str] = &[
    "thank you", "thanks for watching", "thank you for watching", "thank you so much for watching", "you", "bye",
    "bye bye", "thank you very much", "subtitles by the amaraorg community", "please subscribe", "see you next time",
    "thanks", "so", "the end", "okay", "oh", "hmm", "uh", "um",
];

/// True if `text` is a typical Whisper hallucination produced on (near)
/// silence. Only meaningful for very short amounts of detected speech.
pub fn is_hallucination(text: &str) -> bool {
    let key: String = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    key.is_empty() || HALLUCINATIONS.contains(&key.as_str())
}

use serde::{Deserialize, Serialize};

use crate::dictionary::Dictionary;
use crate::english;
use crate::numbers;
use crate::tokens::{capitalize_first, join, lowercase_first, tokenize, Tok};

/// What kind of application will receive the text. Only used to pick
/// formatting conventions — never to change what was said.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppContext {
    #[default]
    General,
    Code,
    Terminal,
    Chat,
    Document,
    Browser,
}

#[derive(Debug, Clone)]
pub struct ProcessOptions {
    pub remove_fillers: bool,
    pub smart_punctuation: bool,
    pub auto_capitalize: bool,
    pub spoken_punctuation: bool,
    pub aggressive: bool,
    /// ISO-639-1 code of the spoken language ("en", "de", ...); empty = unknown.
    pub language: String,
    pub context: AppContext,
}

impl Default for ProcessOptions {
    fn default() -> Self {
        ProcessOptions {
            remove_fillers: true,
            smart_punctuation: true,
            auto_capitalize: true,
            spoken_punctuation: true,
            aggressive: false,
            language: "en".into(),
            context: AppContext::General,
        }
    }
}

pub(crate) fn sentence_start(t: &[Tok], i: usize) -> bool {
    let mut k = i;
    while k > 0 {
        match &t[k - 1] {
            Tok::Punct(p) if matches!(p.as_str(), "\"" | "(" | "“" | "‘" | "¿" | "¡") => k -= 1,
            Tok::Break(_) => return true,
            other => return other.is_terminal(),
        }
    }
    true
}

fn punct_rank(p: &str) -> u8 {
    match p {
        "?" | "!" => 4,
        "." => 3,
        ":" | ";" => 2,
        "," => 1,
        _ => 0,
    }
}

/// Remove doubled or dangling punctuation left behind by the other passes.
fn cleanup_punct(t: &mut Vec<Tok>) {
    if t.iter().all(|x| !x.is_word()) {
        // Whole utterance was punctuation ("period" -> ".").
        t.dedup();
        return;
    }
    let mut i = 0;
    while i < t.len() {
        let at_start = i == 0 || matches!(t[i - 1], Tok::Break(_));
        if at_start && t[i].is_clause_punct() {
            t.remove(i);
            continue;
        }
        if i > 0 && t[i].is_clause_punct() && t[i - 1].is_clause_punct() {
            let a = punct_rank(t[i - 1].punct().unwrap());
            let b = punct_rank(t[i].punct().unwrap());
            if b > a {
                t.remove(i - 1);
            } else {
                t.remove(i);
            }
            continue;
        }
        i += 1;
    }
    // Comma or colon right before a line break reads as a mistake.
    let mut i = 1;
    while i < t.len() {
        if matches!(t[i], Tok::Break(_)) && matches!(t[i - 1].punct(), Some(",")) {
            t.remove(i - 1);
            continue;
        }
        i += 1;
    }
}

fn uses_latin_punctuation(t: &[Tok]) -> bool {
    t.iter().rev().find_map(|x| x.text()).and_then(|w| w.chars().last()).map(|c| (c as u32) < 0x0250).unwrap_or(false)
}

fn sentences(t: &[Tok]) -> Vec<(usize, usize)> {
    // [start, end) ranges; end excludes the terminal punctuation token.
    let mut out = Vec::new();
    let mut start = 0;
    for (i, tok) in t.iter().enumerate() {
        if tok.is_terminal() || matches!(tok, Tok::Break(_)) {
            if i > start {
                out.push((start, i));
            }
            start = i + 1;
        }
    }
    if start < t.len() {
        out.push((start, t.len()));
    }
    out
}

fn smart_punctuation(t: &mut Vec<Tok>, english: bool) {
    // Questions: fix "." -> "?" and pick the terminal mark for the last sentence.
    if english {
        for (s, e) in sentences(t) {
            let words: Vec<String> = t[s..e].iter().filter_map(|x| x.lower()).collect();
            if words.is_empty() || !english::is_question(&words) {
                continue;
            }
            if t.get(e).map(|x| x.is_punct(".")).unwrap_or(false) {
                t[e] = Tok::Punct("?".into());
            }
        }
    }
    // Ensure the text ends with sentence punctuation.
    let last_idx = t.iter().rposition(|x| !matches!(x, Tok::Break(_)));
    let Some(li) = last_idx else { return };
    let is_end = |p: &str| matches!(p, "." | "?" | "!" | "…" | "..." | ":" | ";" | "。" | "？" | "！");
    let is_closer = |p: &str| matches!(p, ")" | "\"" | "”" | "’" | "]");
    let ends_ok = match &t[li] {
        Tok::Punct(p) if is_end(p) => true,
        // `(later)` / `"hello"`: fine only if the sentence ended inside.
        Tok::Punct(p) if is_closer(p) => li > 0 && t[li - 1].punct().map(is_end).unwrap_or(false),
        _ => false,
    };
    if ends_ok || !uses_latin_punctuation(&t[..=li]) {
        return;
    }
    // Strip a trailing comma first.
    let mut li = li;
    if matches!(t[li].punct(), Some(",")) {
        t.remove(li);
        li -= 1;
    }
    let closer = t[li].punct().map(is_closer).unwrap_or(false);
    if !t[li].is_word() && !closer {
        return;
    }
    let sent_start = t[..=li].iter().rposition(|x| x.is_terminal() || matches!(x, Tok::Break(_))).map(|p| p + 1).unwrap_or(0);
    let words: Vec<String> = t[sent_start..=li].iter().filter_map(|x| x.lower()).collect();
    let mark = if english && english::is_question(&words) { "?" } else { "." };
    t.insert(li + 1, Tok::Punct(mark.into()));
}

fn capitalize(t: &mut [Tok]) {
    for i in 0..t.len() {
        if sentence_start(t, i) {
            if let Tok::Word { text, locked: false } = &mut t[i] {
                *text = capitalize_first(text);
            }
        }
    }
}

/// Clean up a raw transcript for insertion.
pub fn process(raw: &str, opts: &ProcessOptions, dict: &Dictionary) -> String {
    let english = opts.language.is_empty() || opts.language.starts_with("en");
    let terminal = opts.context == AppContext::Terminal;
    let cleaned = crate::artifacts::strip_artifacts(raw);
    let mut t = tokenize(&cleaned);
    if t.is_empty() {
        return String::new();
    }
    if english && opts.spoken_punctuation {
        english::spoken_punctuation(&mut t);
    }
    if english && opts.remove_fillers {
        english::remove_fillers(&mut t, opts.aggressive);
    }
    english::remove_stutters(&mut t, english, opts.aggressive);
    dict.apply(&mut t, opts.context == AppContext::Code, terminal);
    if english {
        numbers::convert(&mut t);
    }
    cleanup_punct(&mut t);
    if terminal {
        // Commands: no sentence punctuation, keep the user's lowercase.
        if let Some(li) = t.iter().rposition(|x| !matches!(x, Tok::Break(_))) {
            if t[li].is_punct(".") && t[..li].iter().filter(|x| x.is_terminal()).count() == 0 {
                t.remove(li);
            }
        }
        if let Some(Tok::Word { text, locked: false }) = t.iter_mut().find(|x| x.is_word()) {
            let rest_lower = text.chars().skip(1).all(|c| !c.is_uppercase());
            if rest_lower && text.chars().count() > 1 {
                *text = lowercase_first(text);
            }
        }
    } else if opts.smart_punctuation {
        smart_punctuation(&mut t, english);
    }
    if english {
        english::fix_pronoun_i(&mut t);
    }
    if opts.auto_capitalize && !terminal {
        capitalize(&mut t);
    }
    join(&t)
}

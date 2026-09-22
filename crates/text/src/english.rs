//! English-specific cleanup rules. Other languages skip these and only get the
//! language-neutral passes (spacing, capitalization, dictionary).

use crate::tokens::{capitalize_first, starts_upper, Tok};

const FILLERS: &[&str] = &[
    "um", "umm", "ummm", "uhm", "uh", "uhh", "uhhh", "er", "erm", "ah", "ahh", "hmm", "hmmm", "hm", "mm", "mmm", "eh",
];

/// Words after which a spoken punctuation name is meant literally
/// ("add a comma", "the trial period").
const DETERMINERS: &[&str] = &[
    "a", "an", "the", "this", "that", "these", "those", "my", "your", "his", "her", "its", "our", "their", "each", "every", "any",
    "some", "no", "another", "which", "what", "one", "per",
];

const PERIOD_LITERAL_PREV: &[&str] = &[
    "trial", "grace", "time", "waiting", "cooling", "free", "billing", "notice", "same", "long", "short", "rest", "test",
    "probation", "probationary", "warranty", "holding", "first", "second", "third", "last", "next", "whole", "entire",
    "reporting", "sample", "study", "given", "certain", "transition", "introductory", "honeymoon", "evaluation", "lock",
];

const COLON_LITERAL_NEXT: &[&str] = &["cancer", "surgery", "polyp", "polyps", "cleanse", "health"];

/// Words after which "like" is a real verb/preposition, never a filler.
const LIKE_TAKERS: &[&str] = &[
    "look", "looks", "looked", "looking", "seem", "seems", "seemed", "feel", "feels", "felt", "sound", "sounds", "sounded",
    "taste", "tastes", "smell", "smells", "would", "i'd", "you'd", "we'd", "they'd", "he'd", "she'd", "something",
    "anything", "nothing", "more", "less", "much", "just", "exactly", "not", "don't", "didn't", "doesn't", "do", "does",
    "did", "really", "i", "we", "they", "people", "who", "to", "was", "is", "be", "been", "are", "were", "and", "or",
    "very", "also", "still", "wouldn't", "shouldn't", "kinda", "most", "least",
];

/// Common base-form verbs: "you like create a function" -> filler "like".
const BASE_VERBS: &[&str] = &[
    "create", "make", "get", "go", "do", "have", "add", "write", "build", "remove", "delete", "fix", "update", "change",
    "send", "open", "close", "run", "show", "check", "use", "put", "set", "find", "give", "take", "move", "call", "try",
    "start", "stop", "help", "tell", "say", "ask", "see", "look", "want", "need", "think", "know", "install", "deploy",
    "test", "return", "fetch", "print", "read", "save", "load", "copy", "paste", "type", "click", "select", "rename",
    "refactor", "implement", "convert", "generate", "explain", "summarize", "list", "sort", "filter", "push", "pull",
    "commit", "merge", "clone", "compile", "debug", "log", "handle", "parse", "format", "clean", "wrap", "split", "join",
    "replace", "insert", "append", "render", "display", "draw", "play", "pause", "turn", "switch", "keep", "leave",
    "bring", "buy", "pay", "eat", "drink", "come", "walk", "talk", "speak", "listen", "hear", "watch", "wait", "finish",
    "email", "reply", "schedule", "book", "cancel", "review", "edit", "share", "upload", "download", "sign", "store",
    "calculate", "compute", "define", "declare", "import", "export", "configure", "setup", "reset", "restart", "refresh",
];

/// Words that are often legitimately doubled ("that that", "had had").
const KEEP_REPEATS: &[&str] = &[
    "that", "had", "is", "very", "really", "no", "yes", "so", "bye", "ha", "haha", "go", "knock", "well", "now", "please",
    "too", "more", "far", "again", "over", "round", "on", "many", "much", "long", "blah", "la", "ho", "hey", "tick", "tock",
    "yeah", "okay", "ok", "do", "come", "on", "wow", "oh", "boo", "chop", "din", "bla", "beep", "hip", "hear", "there",
];

/// A comma after these sentence openers is kept even when a following filler
/// is removed ("Yes, um, I agree" -> "Yes, I agree").
const KEEP_COMMA_AFTER: &[&str] = &[
    "yes", "no", "well", "okay", "ok", "so", "hi", "hello", "hey", "oh", "right", "sure", "thanks", "yeah", "now", "also",
    "however", "first", "second", "finally", "actually",
];

// ---------------------------------------------------------------------------
// Spoken punctuation
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Mark {
    /// Punctuation that attaches to the left (",", "?", ")" ...).
    Close(&'static str),
    /// Punctuation that attaches to the right ("(", opening quote).
    Open(&'static str),
    Quote,
    Break(u8),
}

const SPOKEN: &[(&[&str], Mark)] = &[
    (&["question", "mark"], Mark::Close("?")),
    (&["exclamation", "mark"], Mark::Close("!")),
    (&["exclamation", "point"], Mark::Close("!")),
    (&["full", "stop"], Mark::Close(".")),
    (&["period"], Mark::Close(".")),
    (&["comma"], Mark::Close(",")),
    (&["semicolon"], Mark::Close(";")),
    (&["semi", "colon"], Mark::Close(";")),
    (&["colon"], Mark::Close(":")),
    (&["open", "parenthesis"], Mark::Open("(")),
    (&["open", "parentheses"], Mark::Open("(")),
    (&["open", "paren"], Mark::Open("(")),
    (&["left", "parenthesis"], Mark::Open("(")),
    (&["close", "parenthesis"], Mark::Close(")")),
    (&["close", "parentheses"], Mark::Close(")")),
    (&["close", "paren"], Mark::Close(")")),
    (&["closing", "parenthesis"], Mark::Close(")")),
    (&["right", "parenthesis"], Mark::Close(")")),
    (&["end", "parenthesis"], Mark::Close(")")),
    (&["end", "paren"], Mark::Close(")")),
    (&["open", "quote"], Mark::Quote),
    (&["begin", "quote"], Mark::Quote),
    (&["start", "quote"], Mark::Quote),
    (&["open", "quotes"], Mark::Quote),
    (&["close", "quote"], Mark::Quote),
    (&["close", "quotes"], Mark::Quote),
    (&["end", "quote"], Mark::Quote),
    (&["end", "quotes"], Mark::Quote),
    (&["unquote"], Mark::Quote),
    (&["new", "paragraph"], Mark::Break(2)),
    (&["next", "paragraph"], Mark::Break(2)),
    (&["new", "line"], Mark::Break(1)),
    (&["newline"], Mark::Break(1)),
    (&["next", "line"], Mark::Break(1)),
];

fn norm(t: &Tok) -> Option<String> {
    t.lower().map(|w| w.replace('-', ""))
}

fn prev_word(t: &[Tok], i: usize) -> Option<String> {
    if i == 0 {
        return None;
    }
    t[i - 1].lower()
}

fn next_word(t: &[Tok], i: usize) -> Option<String> {
    t.get(i).and_then(|x| x.lower())
}

/// Replace spoken punctuation ("comma", "question mark", "new line") with the
/// actual characters, unless context says the word is meant literally.
pub fn spoken_punctuation(t: &mut Vec<Tok>) {
    // A bare "quote" only counts as a mark when the speaker also closes it.
    let has_quote_closer = {
        let words: Vec<String> = t.iter().filter_map(norm).collect();
        words.iter().any(|w| w == "unquote")
            || words.windows(2).any(|w| (w[0] == "end" || w[0] == "close") && w[1].starts_with("quote"))
    };
    let mut i = 0;
    while i < t.len() {
        if !t[i].is_word() {
            i += 1;
            continue;
        }
        let mut matched: Option<(usize, Mark)> = None;
        for (words, mark) in SPOKEN {
            let n = words.len();
            if i + n > t.len() {
                continue;
            }
            if (0..n).all(|k| norm(&t[i + k]).as_deref() == Some(words[k])) {
                matched = Some((n, *mark));
                break;
            }
        }
        // Bare "quote".
        if matched.is_none() && norm(&t[i]).as_deref() == Some("quote") && has_quote_closer {
            matched = Some((1, Mark::Quote));
        }
        let Some((n, mark)) = matched else {
            i += 1;
            continue;
        };
        let first = norm(&t[i]).unwrap_or_default();
        let prev = prev_word(t, i);
        let next = next_word(t, i + n);
        let literal = prev.as_deref().map(|p| DETERMINERS.contains(&p)).unwrap_or(false)
            || next.as_deref() == Some("of")
            || (first == "period" && prev.as_deref().map(|p| PERIOD_LITERAL_PREV.contains(&p)).unwrap_or(false))
            || (first == "colon" && next.as_deref().map(|w| COLON_LITERAL_NEXT.contains(&w)).unwrap_or(false));
        if literal {
            i += n;
            continue;
        }
        // Drop the matched words and any punctuation the recogniser put right
        // after them ("comma," -> ",").
        t.drain(i..i + n);
        while i < t.len() && t[i].is_clause_punct() {
            t.remove(i);
        }
        let replacement = match mark {
            Mark::Close(p) => {
                // "you? Question mark" -> keep only the spoken mark.
                while i > 0 && t[i - 1].is_clause_punct() {
                    t.remove(i - 1);
                    i -= 1;
                }
                Tok::Punct(p.into())
            }
            Mark::Open(p) => Tok::Punct(p.into()),
            Mark::Quote => {
                // The joiner decides open vs close by counting quotes.
                Tok::Punct("\"".into())
            }
            Mark::Break(b) => {
                if i > 0 && matches!(t[i - 1].punct(), Some("," | ";" | ":")) {
                    t.remove(i - 1);
                    i -= 1;
                }
                Tok::Break(b)
            }
        };
        t.insert(i, replacement);
        i += 1;
    }
}

// ---------------------------------------------------------------------------
// Fillers
// ---------------------------------------------------------------------------

fn at_sentence_start(t: &[Tok], i: usize) -> bool {
    let mut k = i;
    while k > 0 {
        match &t[k - 1] {
            Tok::Punct(p) if matches!(p.as_str(), "\"" | "(" | "“" | "‘") => k -= 1,
            Tok::Break(_) => return true,
            other => return other.is_terminal(),
        }
    }
    true
}

fn like_is_filler(t: &[Tok], i: usize, aggressive: bool) -> bool {
    let prev_tok = if i > 0 { t.get(i - 1) } else { None };
    let next_tok = t.get(i + 1);
    let prev_comma = prev_tok.map(|x| x.is_punct(",")).unwrap_or(false);
    let next_comma = next_tok.map(|x| x.is_punct(",")).unwrap_or(false);
    if prev_comma && next_comma {
        return true;
    }
    if next_comma && at_sentence_start(t, i) {
        return true;
    }
    let pw = prev_tok.and_then(|x| x.lower());
    let nw = next_tok.and_then(|x| x.lower());
    let Some(nw) = nw else { return false };
    if FILLERS.contains(&nw.as_str()) {
        return true;
    }
    if let Some(pw) = &pw {
        if LIKE_TAKERS.contains(&pw.as_str()) {
            return false;
        }
    }
    if BASE_VERBS.contains(&nw.as_str()) && pw.is_some() {
        return true;
    }
    if aggressive {
        let subject = matches!(pw.as_deref(), Some("you" | "i" | "we" | "they" | "he" | "she" | "it"));
        let loose = matches!(
            nw.as_str(),
            "the" | "a" | "an" | "so" | "really" | "totally" | "literally" | "super" | "very" | "maybe" | "about" | "just"
        );
        return loose && !subject && pw.is_some();
    }
    false
}

/// Number of words starting at `i` that form a filler, 0 if none.
fn filler_span(t: &[Tok], i: usize, aggressive: bool) -> usize {
    let Some(w) = t[i].lower() else { return 0 };
    if FILLERS.contains(&w.as_str()) {
        return 1;
    }
    if w == "like" && like_is_filler(t, i, aggressive) {
        return 1;
    }
    if !aggressive {
        return 0;
    }
    let comma_after = |k: usize| t.get(k).map(|x| x.is_punct(",")).unwrap_or(false);
    let comma_before = i > 0 && t[i - 1].is_punct(",");
    let next = t.get(i + 1).and_then(|x| x.lower());
    match w.as_str() {
        "you" if next.as_deref() == Some("know") && (comma_after(i + 2) || comma_before) => 2,
        "i" if next.as_deref() == Some("mean") && comma_after(i + 2) && at_sentence_start(t, i) => 2,
        "basically" | "literally" | "actually" | "honestly" if comma_after(i + 1) || comma_before => 1,
        "so" | "well" | "okay" | "ok" | "anyway" if comma_after(i + 1) && at_sentence_start(t, i) => 1,
        _ => 0,
    }
}

pub fn remove_fillers(t: &mut Vec<Tok>, aggressive: bool) {
    let mut i = 0;
    while i < t.len() {
        let span = filler_span(t, i, aggressive);
        if span == 0 {
            i += 1;
            continue;
        }
        let was_cap = t[i].text().map(starts_upper).unwrap_or(false);
        let sentence_start = at_sentence_start(t, i);
        t.drain(i..i + span);
        while i < t.len() && matches!(t[i].punct(), Some("," | "..." | "…")) {
            t.remove(i);
        }
        if i > 0 && t[i - 1].is_punct(",") {
            let next_lower_word = t.get(i).and_then(|x| x.text()).map(|w| !starts_upper(w)).unwrap_or(false);
            let next_is_end = t.get(i).map(|x| x.is_terminal()).unwrap_or(true);
            let before = if i >= 2 { t[i - 2].lower() } else { None };
            let keep = before.as_deref().map(|b| KEEP_COMMA_AFTER.contains(&b)).unwrap_or(false);
            if next_is_end || (next_lower_word && !keep) {
                t.remove(i - 1);
                i -= 1;
            }
        }
        if was_cap && sentence_start {
            if let Some(Tok::Word { text, locked: false }) = t.get_mut(i) {
                *text = capitalize_first(text);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Stutters and repeats
// ---------------------------------------------------------------------------

pub fn remove_stutters(t: &mut Vec<Tok>, english: bool, aggressive: bool) {
    // Cut-off words: "th- the", "I- I".
    let mut i = 0;
    while i + 1 < t.len() {
        if let (Some(a), Some(b)) = (t[i].text(), t[i + 1].text()) {
            if a.len() >= 2 && a.ends_with('-') && !a.ends_with("--") {
                let stem = a.trim_end_matches('-').to_lowercase();
                if b.to_lowercase().starts_with(&stem) || stem.chars().count() <= 2 {
                    let cap = starts_upper(a);
                    t.remove(i);
                    if cap {
                        if let Tok::Word { text, locked: false } = &mut t[i] {
                            *text = capitalize_first(text);
                        }
                    }
                    continue;
                }
            }
        }
        i += 1;
    }
    // Immediate duplicates: "the the".
    let mut i = 0;
    while i + 1 < t.len() {
        if let (Some(a), Some(b)) = (t[i].lower(), t[i + 1].lower()) {
            let keep = english && KEEP_REPEATS.contains(&a.as_str());
            if a == b && !keep && a.chars().any(|c| c.is_alphabetic()) {
                t.remove(i + 1);
                continue;
            }
        }
        i += 1;
    }
    if aggressive {
        remove_phrase_repeats(t);
    }
}

/// "I want to, I want to go" -> "I want to go" (2–4 word phrases).
fn remove_phrase_repeats(t: &mut Vec<Tok>) {
    for n in (2..=4).rev() {
        let mut i = 0;
        while i + 2 * n <= t.len() {
            let a: Vec<Option<String>> = (0..n).map(|k| t[i + k].lower()).collect();
            let mut j = i + n;
            let comma = t.get(j).map(|x| x.is_punct(",")).unwrap_or(false);
            if comma {
                j += 1;
            }
            if j + n <= t.len() && a.iter().all(|x| x.is_some()) {
                let b: Vec<Option<String>> = (0..n).map(|k| t[j + k].lower()).collect();
                if a == b {
                    t.drain(i + n..j + n);
                    continue;
                }
            }
            i += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Questions and pronoun "I"
// ---------------------------------------------------------------------------

const BE_MODAL: &[&str] = &[
    "can", "could", "would", "will", "should", "shall", "is", "are", "was", "were", "am", "isn't", "aren't", "wasn't",
    "weren't", "can't", "couldn't", "wouldn't", "won't", "shouldn't",
];
const HAVE_DO: &[&str] = &[
    "do", "does", "did", "don't", "doesn't", "didn't", "have", "has", "had", "haven't", "hasn't", "may",
];
const WH: &[&str] = &["what", "why", "how", "when", "where", "who", "which", "whose", "whom"];
const WH_CONTRACTED: &[&str] = &[
    "what's", "where's", "how's", "who's", "why's", "when's", "what're", "who're", "how're", "where're", "whats", "hows",
    "wheres", "what'd", "how'd", "where'd", "who'd", "why'd",
];
const PRONOUNS: &[&str] = &[
    "i", "you", "we", "they", "he", "she", "it", "there", "anyone", "anybody", "everyone", "everybody", "someone",
    "somebody", "people", "y'all", "ya",
];
const DETS: &[&str] = &["the", "this", "that", "these", "those", "my", "your", "our", "their", "his", "her", "its", "a", "an", "any", "all"];
const OPENERS: &[&str] = &["so", "and", "but", "okay", "ok", "hey", "well", "also", "then", "now", "oh", "also", "alright", "and", "or"];
const HOW_QUANT: &[&str] = &["many", "much", "long", "often", "far", "old", "big", "about", "come"];

pub fn is_question(words: &[String]) -> bool {
    let mut w: &[String] = words;
    if w.len() > 1 && OPENERS.contains(&w[0].as_str()) {
        w = &w[1..];
    }
    let (Some(w0), w1) = (w.first(), w.get(1)) else { return false };
    let w1 = w1.map(|s| s.as_str()).unwrap_or("");
    if WH_CONTRACTED.contains(&w0.as_str()) {
        return true;
    }
    if WH.contains(&w0.as_str()) {
        return BE_MODAL.contains(&w1) || HAVE_DO.contains(&w1) || (w0 == "how" && HOW_QUANT.contains(&w1))
            || (w0 == "which" || w0 == "what") && w.len() >= 3 && (BE_MODAL.contains(&w[2].as_str()) || HAVE_DO.contains(&w[2].as_str()));
    }
    if BE_MODAL.contains(&w0.as_str()) {
        return PRONOUNS.contains(&w1) || DETS.contains(&w1);
    }
    if HAVE_DO.contains(&w0.as_str()) {
        // "Do it now" is an instruction, "Does it work" a question.
        if w0 == "do" && matches!(w1, "it" | "there" | "this" | "that") {
            return false;
        }
        return PRONOUNS.contains(&w1);
    }
    false
}

pub fn fix_pronoun_i(t: &mut [Tok]) {
    for tok in t.iter_mut() {
        if let Tok::Word { text, locked: false } = tok {
            let lower = text.to_lowercase();
            if lower == "i" || lower.starts_with("i'") && matches!(&lower[2..], "m" | "ll" | "ve" | "d") {
                *text = capitalize_first(text);
            }
        }
    }
}

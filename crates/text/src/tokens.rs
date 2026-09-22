//! A tiny tokenizer that separates words from punctuation so the rules can
//! reason about both, and a joiner that restores natural spacing.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    /// A word. `locked` words were produced by the dictionary and keep their
    /// exact casing.
    Word { text: String, locked: bool },
    Punct(String),
    /// 1 = line break, 2 = paragraph break.
    Break(u8),
}

impl Tok {
    pub fn word(s: impl Into<String>) -> Tok {
        Tok::Word { text: s.into(), locked: false }
    }
    pub fn text(&self) -> Option<&str> {
        match self {
            Tok::Word { text, .. } => Some(text),
            _ => None,
        }
    }
    /// Lower-cased word without a trailing cut-off hyphen.
    pub fn lower(&self) -> Option<String> {
        self.text().map(|t| t.trim_end_matches('-').to_lowercase())
    }
    pub fn is_word(&self) -> bool {
        matches!(self, Tok::Word { .. })
    }
    pub fn is_punct(&self, p: &str) -> bool {
        matches!(self, Tok::Punct(x) if x == p)
    }
    pub fn punct(&self) -> Option<&str> {
        match self {
            Tok::Punct(p) => Some(p),
            _ => None,
        }
    }
    /// `.` `?` `!` — ends a sentence.
    pub fn is_terminal(&self) -> bool {
        matches!(self.punct(), Some("." | "?" | "!" | "…" | "..."))
    }
    /// Punctuation that separates clauses or ends sentences.
    pub fn is_clause_punct(&self) -> bool {
        matches!(self.punct(), Some("," | "." | "?" | "!" | ":" | ";"))
    }
}

const LEADING: &[char] = &['"', '(', '[', '{', '“', '‘', '¿', '¡', '«'];
const TRAILING: &[char] = &['.', ',', '?', '!', ':', ';', ')', ']', '}', '"', '”', '’', '»', '…', '。', '，', '？', '！'];
const ABBREVIATIONS: &[&str] = &[
    "e.g.", "i.e.", "etc.", "vs.", "mr.", "mrs.", "ms.", "dr.", "prof.", "st.", "jr.", "sr.", "a.m.", "p.m.", "u.s.", "approx.", "no.",
];

pub fn tokenize(s: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    for (li, line) in s.split('\n').enumerate() {
        if li > 0 {
            out.push(Tok::Break(1));
        }
        for chunk in line.split_whitespace() {
            tokenize_chunk(chunk, &mut out);
        }
    }
    out
}

fn tokenize_chunk(chunk: &str, out: &mut Vec<Tok>) {
    let chars: Vec<char> = chunk.chars().collect();
    let mut start = 0;
    let mut end = chars.len();
    while start < end && LEADING.contains(&chars[start]) {
        out.push(Tok::Punct(chars[start].to_string()));
        start += 1;
    }
    let mut trailing: Vec<String> = Vec::new();
    while end > start && TRAILING.contains(&chars[end - 1]) {
        // Group runs of dots into a single ellipsis token.
        if chars[end - 1] == '.' {
            let mut k = end;
            while k > start && chars[k - 1] == '.' {
                k -= 1;
            }
            let n = end - k;
            if n >= 2 {
                trailing.push("...".into());
                end = k;
                continue;
            }
        }
        trailing.push(chars[end - 1].to_string());
        end -= 1;
    }
    trailing.reverse();
    let mut core: String = chars[start..end].iter().collect();
    // Keep abbreviation dots attached ("e.g.") so they don't end sentences.
    if trailing.first().map(|p| p == ".").unwrap_or(false) {
        let candidate = format!("{}.", core.to_lowercase());
        if ABBREVIATIONS.contains(&candidate.as_str()) {
            core.push('.');
            trailing.remove(0);
        }
    }
    if !core.is_empty() {
        out.push(Tok::word(core));
    }
    out.extend(trailing.into_iter().map(Tok::Punct));
}

enum Attach {
    /// Attaches to the previous token ("," ")" closing quote).
    Left,
    /// Attaches to the following token ("(" opening quote).
    Right,
}

pub fn join(toks: &[Tok]) -> String {
    let mut out = String::new();
    let mut quote_open = false;
    let mut glue_next = true;
    for t in toks {
        match t {
            Tok::Break(n) => {
                while out.ends_with(' ') {
                    out.pop();
                }
                for _ in 0..*n {
                    out.push('\n');
                }
                glue_next = true;
            }
            Tok::Punct(p) => {
                let attach = match p.as_str() {
                    "\"" => {
                        quote_open = !quote_open;
                        if quote_open { Attach::Right } else { Attach::Left }
                    }
                    "(" | "[" | "{" | "“" | "‘" | "¿" | "¡" | "«" => Attach::Right,
                    _ => Attach::Left,
                };
                match attach {
                    Attach::Left => {
                        out.push_str(p);
                        glue_next = false;
                    }
                    Attach::Right => {
                        if !glue_next && !out.is_empty() {
                            out.push(' ');
                        }
                        out.push_str(p);
                        glue_next = true;
                    }
                }
            }
            Tok::Word { text, .. } => {
                if !glue_next {
                    out.push(' ');
                }
                out.push_str(text);
                glue_next = false;
            }
        }
    }
    out
}

pub fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn lowercase_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

pub fn starts_upper(s: &str) -> bool {
    s.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for s in [
            "Hello, how are you?",
            "He said \"hi\" (quietly).",
            "Use e.g. Node.js 3.5 today...",
            "Wait... what?",
        ] {
            assert_eq!(join(&tokenize(s)), s);
        }
    }
}

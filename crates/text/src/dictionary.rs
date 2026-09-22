//! User-editable vocabulary. Terms fix the spelling/casing of technical words
//! the recogniser tends to split or lowercase ("type script" -> "TypeScript")
//! and are also fed to the recogniser as a prompt to bias recognition.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::tokens::Tok;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Term {
    /// Exact output spelling, e.g. "PostgreSQL".
    pub term: String,
    /// Extra spellings/misrecognitions that should map to `term`.
    #[serde(default)]
    pub aliases: Vec<String>,
}

impl Term {
    pub fn new(term: &str, aliases: &[&str]) -> Self {
        Term { term: term.into(), aliases: aliases.iter().map(|s| s.to_string()).collect() }
    }
}

pub fn default_terms() -> Vec<Term> {
    vec![
        Term::new("React", &[]),
        Term::new("React Native", &[]),
        Term::new("TypeScript", &[]),
        Term::new("JavaScript", &[]),
        Term::new("Supabase", &["super base", "supa base", "soup a base", "super bass"]),
        Term::new("PostgreSQL", &["postgres sql", "postgre sql", "postgres q l", "post gres q l", "post gress sql", "postgres equal"]),
        Term::new("Kubernetes", &["cooper netties", "kuber netes", "kubernetis", "kubernetz", "kubernets", "kubernet", "cube a netties"]),
        Term::new("pull request", &["poll request", "pool request"]),
        Term::new("Docker", &[]),
        Term::new("FastAPI", &["fast a p i"]),
        Term::new("Spring Boot", &[]),
        Term::new("GitHub", &["get hub", "git hub"]),
        Term::new("VS Code", &["visual studio code", "vs cold", "v s code"]),
        Term::new("Claude", &["clawed", "claud"]),
        Term::new("OpenAI", &["open a i", "open ai"]),
        Term::new("Whisper", &[]),
        Term::new("faster-whisper", &["faster whisper"]),
        Term::new("Node.js", &["node js", "node j s", "node dot js"]),
        Term::new("npm", &["n p m"]),
        Term::new("API", &["a p i"]),
        Term::new("JSON", &["jason file"]),
        Term::new("SQL", &["s q l"]),
        Term::new("Next.js", &["next js", "next dot js"]),
        Term::new("Tailwind CSS", &["tailwind css"]),
        Term::new("Python", &[]),
        Term::new("Rust", &[]),
        Term::new("Git", &[]),
    ]
}

/// Single words that are also ordinary English. Outside code-oriented apps
/// these are only re-cased when the recogniser already capitalised them.
const COMMON_WORDS: &[&str] = &[
    "react", "whisper", "rust", "go", "swift", "spring", "express", "flutter", "node", "next", "remix", "ruby", "python",
    "java", "dart", "elm", "ember", "mocha", "jest", "vite", "angular", "git", "claude", "gatsby", "svelte",
    "apple", "windows", "office", "teams", "word", "excel", "slack", "notion", "linear", "figma", "bun", "deno", "nest",
    "prism", "redux", "sentry", "vercel", "render", "heroku", "cloud", "shell", "bash", "code", "chrome", "edge",
];

#[derive(Debug, Clone)]
struct Entry {
    output: String,
    /// Output split into words; used to keep a multi-word term together.
    common: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Dictionary {
    entries: Vec<Entry>,
    /// normalized key -> entry index
    keys: HashMap<String, usize>,
    /// short keys (< 3 chars) must match the exact lowercase phrase
    short: HashMap<String, usize>,
    max_words: usize,
    terms: Vec<String>,
}

fn normalize(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

impl Dictionary {
    pub fn new(terms: &[Term]) -> Self {
        let mut d = Dictionary { max_words: 1, ..Default::default() };
        for t in terms {
            let output = t.term.trim();
            if output.is_empty() {
                continue;
            }
            let key = normalize(output);
            if key.is_empty() {
                continue;
            }
            let idx = d.entries.len();
            d.entries.push(Entry { output: output.to_string(), common: COMMON_WORDS.contains(&key.as_str()) });
            d.terms.push(output.to_string());
            for spelling in std::iter::once(output).chain(t.aliases.iter().map(|s| s.trim())) {
                let words = spelling.split_whitespace().count().max(1);
                // A spoken alias may be split into more words than it has letters groups.
                d.max_words = d.max_words.max(words + 2).min(6);
                let k = normalize(spelling);
                if k.is_empty() {
                    continue;
                }
                if k.chars().count() < 3 {
                    d.short.insert(spelling.to_lowercase(), idx);
                } else {
                    d.keys.entry(k).or_insert(idx);
                }
            }
        }
        d
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Terms suitable for the recogniser prompt, joined with commas and
    /// limited to `max_chars`.
    pub fn prompt(&self, max_chars: usize) -> String {
        let mut out = String::new();
        for t in &self.terms {
            if out.len() + t.len() + 2 > max_chars {
                break;
            }
            if !out.is_empty() {
                out.push_str(", ");
            }
            out.push_str(t);
        }
        out
    }

    fn lookup(&self, words: &[&str]) -> Option<usize> {
        let joined_lower = words.join(" ").to_lowercase();
        if let Some(i) = self.short.get(&joined_lower) {
            return Some(*i);
        }
        let key: String = words.iter().map(|w| normalize(w)).collect();
        if key.chars().count() < 3 {
            return None;
        }
        if let Some(i) = self.keys.get(&key) {
            return Some(*i);
        }
        // Possessive/plural forms of multi-word matches are rare; fuzzy
        // matching only for long keys where a 1-2 letter slip is unambiguous.
        let n = key.chars().count();
        if n >= 8 {
            let max = if n >= 12 { 2 } else { 1 };
            let first = key.chars().next();
            let mut best: Option<(usize, usize)> = None;
            for (k, idx) in &self.keys {
                if k.chars().next() != first || k.chars().count().abs_diff(n) > max {
                    continue;
                }
                let d = levenshtein(k, &key);
                if d <= max && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                    best = Some((d, *idx));
                }
            }
            return best.map(|(_, i)| i);
        }
        None
    }

    pub(crate) fn apply(&self, t: &mut Vec<Tok>, code_context: bool, terminal: bool) {
        if self.is_empty() {
            return;
        }
        let mut i = 0;
        while i < t.len() {
            if !t[i].is_word() {
                i += 1;
                continue;
            }
            // Longest window of consecutive words first.
            let mut run = 0;
            while i + run < t.len() && t[i + run].is_word() && run < self.max_words {
                run += 1;
            }
            let mut replaced = false;
            for n in (1..=run).rev() {
                let texts: Vec<String> = (0..n).map(|k| t[i + k].text().unwrap_or_default().to_string()).collect();
                let mut refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
                // "React's" -> match "React", keep the suffix.
                let mut suffix = "";
                if let Some(last) = refs.last().copied() {
                    for s in ["'s", "’s", "s'"] {
                        if let Some(stripped) = last.strip_suffix(s) {
                            if self.lookup(&refs).is_none() {
                                let mut alt = refs.clone();
                                *alt.last_mut().unwrap() = stripped;
                                if self.lookup(&alt).is_some() {
                                    refs = alt;
                                    suffix = s;
                                }
                            }
                            break;
                        }
                    }
                }
                let Some(idx) = self.lookup(&refs) else { continue };
                let entry = &self.entries[idx];
                // Terminals: commands are lowercase, never re-case a single word.
                if terminal && n == 1 && normalize(refs[0]) == normalize(&entry.output) {
                    continue;
                }
                if n == 1 && entry.common && !code_context {
                    // Only fix casing of ordinary words when the recogniser
                    // itself treated it as a proper noun.
                    let w = refs[0];
                    let recogniser_capitalised = w.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
                        && !crate::pipeline::sentence_start(t, i);
                    if !recogniser_capitalised {
                        continue;
                    }
                }
                let output = format!("{}{}", entry.output, suffix);
                // Lowercase phrase corrections ("pull request") still get
                // sentence capitalization; brand spellings ("npm") never do.
                let locked = !(output.contains(' ') && output.chars().all(|c| !c.is_uppercase()));
                t.splice(i..i + n, [Tok::Word { text: output, locked }]);
                replaced = true;
                break;
            }
            let _ = replaced;
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance() {
        assert_eq!(levenshtein("kubernetis", "kubernetes"), 1);
        assert_eq!(levenshtein("", "abc"), 3);
    }
}

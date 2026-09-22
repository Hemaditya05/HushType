//! Spelled-out English numbers -> digits ("twenty five" -> "25",
//! "three point five percent" -> "3.5%"). Single small numbers ("one of them")
//! are left alone because they read better as words.

use crate::tokens::Tok;

fn unit(w: &str) -> Option<u64> {
    Some(match w {
        "zero" => 0,
        "one" => 1,
        "two" => 2,
        "three" => 3,
        "four" => 4,
        "five" => 5,
        "six" => 6,
        "seven" => 7,
        "eight" => 8,
        "nine" => 9,
        "ten" => 10,
        "eleven" => 11,
        "twelve" => 12,
        "thirteen" => 13,
        "fourteen" => 14,
        "fifteen" => 15,
        "sixteen" => 16,
        "seventeen" => 17,
        "eighteen" => 18,
        "nineteen" => 19,
        _ => return None,
    })
}

fn tens(w: &str) -> Option<u64> {
    Some(match w {
        "twenty" => 20,
        "thirty" => 30,
        "forty" => 40,
        "fifty" => 50,
        "sixty" => 60,
        "seventy" => 70,
        "eighty" => 80,
        "ninety" => 90,
        _ => return None,
    })
}

fn scale(w: &str) -> Option<u64> {
    Some(match w {
        "hundred" => 100,
        "thousand" => 1_000,
        "million" => 1_000_000,
        "billion" => 1_000_000_000,
        _ => return None,
    })
}

fn is_number_word(w: &str) -> bool {
    unit(w).is_some() || tens(w).is_some() || scale(w).is_some()
}

/// Parse an integer phrase. Returns None when the words don't form a single
/// well-formed number (e.g. "one two three").
fn parse_int(words: &[&str]) -> Option<u64> {
    let mut total: u64 = 0;
    let mut current: u64 = 0;
    // What may legally follow: 0 = anything, 1 = only unit (after tens), 2 = only scale.
    #[derive(PartialEq, Clone, Copy)]
    enum Prev {
        Start,
        Unit,
        Teen,
        Tens,
        Hundred,
        Big,
    }
    let mut prev = Prev::Start;
    let mut last_big = u64::MAX;
    for w in words {
        if *w == "and" {
            if !matches!(prev, Prev::Hundred | Prev::Big) {
                return None;
            }
            continue;
        }
        if let Some(u) = unit(w) {
            if matches!(prev, Prev::Unit | Prev::Teen) {
                return None;
            }
            if prev == Prev::Tens && u >= 10 {
                return None;
            }
            current += u;
            prev = if u >= 10 { Prev::Teen } else { Prev::Unit };
        } else if let Some(t) = tens(w) {
            if matches!(prev, Prev::Unit | Prev::Teen | Prev::Tens) {
                return None;
            }
            current += t;
            prev = Prev::Tens;
        } else if let Some(s) = scale(w) {
            if prev == Prev::Start || prev == Prev::Hundred && s == 100 {
                return None;
            }
            if s == 100 {
                if current == 0 || current >= 100 {
                    return None;
                }
                current *= 100;
                prev = Prev::Hundred;
            } else {
                if s >= last_big {
                    return None;
                }
                total += current.max(1) * s;
                current = 0;
                last_big = s;
                prev = Prev::Big;
            }
        } else {
            return None;
        }
    }
    Some(total + current)
}

/// "nineteen ninety nine" -> 1999, "twenty twenty four" -> 2024.
fn parse_year(words: &[&str]) -> Option<u64> {
    for split in 1..words.len() {
        let (a, b) = words.split_at(split);
        let (Some(x), Some(y)) = (parse_int(a), parse_int(b)) else { continue };
        if (10..=99).contains(&x) && (10..=99).contains(&y) {
            return Some(x * 100 + y);
        }
    }
    None
}

pub fn convert(t: &mut Vec<Tok>) {
    let mut i = 0;
    while i < t.len() {
        let Some(w0) = t[i].lower() else {
            i += 1;
            continue;
        };
        if !is_number_word(&w0) || scale(&w0).is_some() {
            i += 1;
            continue;
        }
        // Collect the run of number words (allowing inner "and").
        let mut j = i;
        let mut words: Vec<String> = Vec::new();
        while j < t.len() {
            let Some(w) = t[j].lower() else { break };
            if is_number_word(&w) || (w == "and" && !words.is_empty() && t.get(j + 1).and_then(|x| x.lower()).map(|n| is_number_word(&n)).unwrap_or(false) && matches!(words.last().map(|s| s.as_str()), Some("hundred" | "thousand" | "million" | "billion"))) {
                words.push(w);
                j += 1;
            } else {
                break;
            }
        }
        // Decimal part: "point five", "point two five".
        let mut decimals = String::new();
        if t.get(j).and_then(|x| x.lower()).as_deref() == Some("point") {
            let mut k = j + 1;
            while let Some(d) = t.get(k).and_then(|x| x.lower()).and_then(|w| unit(&w)).filter(|d| *d < 10) {
                decimals.push(char::from(b'0' + d as u8));
                k += 1;
            }
            if !decimals.is_empty() {
                j = k;
            }
        }
        let refs: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
        let value = parse_int(&refs).or_else(|| if decimals.is_empty() { parse_year(&refs) } else { None });
        let Some(value) = value else {
            i += 1;
            continue;
        };
        let word_count = refs.iter().filter(|w| **w != "and").count();
        let worth_it = word_count >= 2 || !decimals.is_empty() || tens(refs[0]).is_some();
        if !worth_it {
            i += 1;
            continue;
        }
        let mut s = value.to_string();
        if !decimals.is_empty() {
            s.push('.');
            s.push_str(&decimals);
        }
        let mut end = j;
        match t.get(j).and_then(|x| x.lower()).as_deref() {
            Some("percent") => {
                s.push('%');
                end += 1;
            }
            Some("per") if t.get(j + 1).and_then(|x| x.lower()).as_deref() == Some("cent") => {
                s.push('%');
                end += 2;
            }
            _ => {}
        }
        t.splice(i..end, [Tok::word(s)]);
        i += 1;
    }
}

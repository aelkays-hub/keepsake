//! What the text on a plane says (ADR-041): its registration, the airline
//! and, when it is painted on, the type.
//!
//! - A **registration** is a word shaped like one country's marks
//!   ([`registration`]): `C-FJZS`, `D-AIKR`, `N123AB`, `JA801A`. Most
//!   countries write a prefix, a hyphen and three or four letters; the
//!   hyphen is required, so that ordinary words are not taken for marks.
//! - An **airline** is a line that is one of the names in `airlines.tsv`
//!   as painted ([`Airlines::find`]): the same words, or the name with a
//!   letter missing or wrong when it is long enough to be sure. Titles
//!   split in two by a wing or an engine (`AIR` … `CANADA`) are joined.
//! - A **painted type** is what some airlines write by the door:
//!   `BOEING 777-300ER`, `AIRBUS A220-300` ([`painted_type`]).

use super::ocr::TextLine;

/// A line is trusted for a registration from this confidence.
pub const REGISTRATION_MIN: f32 = 0.8;
/// And for an airline.
pub const NAME_MIN: f32 = 0.85;
/// And for a painted type: its shape is checked closely (`BOEING`, a model
/// `7x7`, a known variant), so a less clear line will do.
pub const TYPE_MIN: f32 = 0.7;
/// At most this many of each are kept for one picture.
pub const MAX_EACH: usize = 3;
/// Only this many lines (the biggest) are tried in pairs for split titles.
const PAIR_LINES: usize = 12;

/// The airline list: `Name<TAB>ALIAS|ALIAS` (aliases as painted, upper
/// case; the name itself is always one).
const AIRLINES_TSV: &str = include_str!("airlines.tsv");

/// Marks written `PREFIX-…`: `@` a letter, `#` a digit, `*` either; other
/// characters stand for themselves.
const DASHED: &[&str] = &[
    // North America and the Caribbean
    "C-F@@@",
    "C-G@@@",
    "C-I@@@",
    "XA-@@@",
    "XB-@@@",
    "XC-@@@",
    "6Y-@@@",
    "9Y-@@@",
    "C6-@@@",
    "VP-B@@",
    "VQ-B@@",
    "VP-C@@",
    "P4-@@@",
    "CU-T####",
    "TI-@@@",
    "TG-@@@",
    "HP-####@@@",
    // South America
    "PP-@@@",
    "PR-@@@",
    "PS-@@@",
    "PT-@@@",
    "PU-@@@",
    "LV-@@@",
    "CC-@@@",
    "HK-####",
    "HK-####@",
    "HC-@@@",
    "OB-####",
    "CP-####",
    "CX-@@@",
    "ZP-@@@",
    "YV-###@",
    // Europe
    "G-@@@@",
    "M-@@@@",
    "2-@@@@",
    "ZJ-@@@",
    "D-@@@@",
    "F-@@@@",
    "I-@@@@",
    "EC-@@@",
    "EI-@@@",
    "EJ-@@@@",
    "HB-@@@",
    "OE-@@@",
    "OO-@@@",
    "PH-@@@",
    "LX-@@@",
    "SE-@@@",
    "OY-@@@",
    "LN-@@@",
    "OH-@@@",
    "TF-@@@",
    "ES-@@@",
    "YL-@@@",
    "LY-@@@",
    "CS-@@@",
    "SP-@@@",
    "OK-@@@",
    "OM-@@@",
    "HA-@@@",
    "YR-@@@",
    "LZ-@@@",
    "SX-@@@",
    "TC-@@@",
    "9H-@@@",
    "9A-@@@",
    "S5-@@@",
    "YU-@@@",
    "E7-@@@",
    "Z3-@@@",
    "ZA-@@@",
    "4O-@@@",
    "5B-@@@",
    "UR-@@@",
    "UR-#####",
    "RA-#####",
    "RF-#####",
    "EW-###@@",
    "ER-@@@",
    "T7-@@@",
    "3A-@@@",
    // Middle East and Central Asia
    "4X-@@@",
    "A6-@@@",
    "A7-@@@",
    "A9C-@@",
    "A9C-@@@",
    "A4O-@@",
    "9K-@@@",
    "HZ-@@@",
    "HZ-@@##",
    "HZ-@##",
    "JY-@@@",
    "OD-@@@",
    "EP-@@@",
    "YI-@@@",
    "YK-@@@",
    "AP-@@@",
    "4K-@@##",
    "4L-@@@",
    "EK-#####",
    "UK-#####",
    "UP-@####",
    "EX-#####",
    "EY-###",
    "EZ-@###",
    // Africa
    "SU-@@@",
    "CN-@@@",
    "7T-@@@",
    "TS-@@@",
    "5A-@@@",
    "ET-@@@",
    "5Y-@@@",
    "5H-@@@",
    "5X-@@@",
    "5N-@@@",
    "9G-@@@",
    "ZS-@@@",
    "V5-@@@",
    "A2-@@@",
    "Z-@@@",
    "9J-@@@",
    "7Q-@@@",
    "C9-@@@",
    "D2-@@@",
    "D4-@@@",
    "3B-@@@",
    "5R-@@@",
    "ST-@@@",
    "TU-@@@",
    "TZ-@@@",
    "6V-@@@",
    "TJ-@@@",
    "TR-@@@",
    "9Q-@@@",
    "9XR-@@",
    // Asia and the Pacific
    "VT-@@@",
    "B-####",
    "B-###@",
    "B-##@@",
    "B-#####",
    "B-@@@",
    "9V-@@@",
    "9M-@@@",
    "HS-@@@",
    "VN-@###",
    "RP-C####",
    "PK-@@@",
    "4R-@@@",
    "S2-@@@",
    "9N-@@@",
    "A5-@@@",
    "XY-@@@",
    "XU-@@@",
    "RDPL-#####",
    "JU-####",
    "VH-@@@",
    "ZK-@@@",
    "DQ-@@@",
    "P2-@@@",
    "V8-@@@",
];

/// Marks written without a hyphen.
const PLAIN: &[&str] = &["JA####", "JA###@", "JA##@@", "HL####"];

fn fits(pattern: &str, word: &str) -> bool {
    pattern.chars().count() == word.chars().count()
        && pattern.chars().zip(word.chars()).all(|(p, c)| match p {
            '@' => c.is_ascii_uppercase(),
            '#' => c.is_ascii_digit(),
            '*' => c.is_ascii_uppercase() || c.is_ascii_digit(),
            p => p == c,
        })
}

/// A United States mark: `N`, one to five characters, digits first (not
/// starting with 0), ending with at most two letters (never I or O).
/// Marks shorter than `N` and three characters are left out: too easily
/// something else.
fn us_mark(word: &str) -> bool {
    let Some(rest) = word.strip_prefix('N') else {
        return false;
    };
    if !(3..=5).contains(&rest.len()) || !rest.is_ascii() {
        return false;
    }
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let letters = &rest[digits..];
    digits >= 1
        && !rest.starts_with('0')
        && letters.len() <= 2
        && letters
            .bytes()
            .all(|b| b.is_ascii_uppercase() && b != b'I' && b != b'O')
}

/// `word` as a registration, if it is shaped like one: upper case, with
/// what stands for the hyphen made a hyphen.
pub fn registration(word: &str) -> Option<String> {
    let w: String = word
        .trim()
        .chars()
        .map(|c| match c {
            '–' | '—' | '_' | '·' | '~' | '=' => '-',
            c => c.to_ascii_uppercase(),
        })
        .collect();
    let w = w.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    if w.is_empty() {
        return None;
    }
    let ok = if w.contains('-') {
        DASHED.iter().any(|p| fits(p, w))
    } else {
        us_mark(w) || PLAIN.iter().any(|p| fits(p, w))
    };
    if ok {
        return Some(w.to_owned());
    }
    // Letters read as digits that look like them (`0O-SFO` for OO-SFO):
    // the prefix, then the rest, then both, as letters; kept only when it
    // is then a country's shape.
    let (prefix, rest) = w.split_once('-')?;
    [
        format!("{}-{rest}", letters_again(prefix)),
        format!("{prefix}-{}", letters_again(rest)),
        format!("{}-{}", letters_again(prefix), letters_again(rest)),
    ]
    .into_iter()
    .find(|c| c != w && DASHED.iter().any(|p| fits(p, c)))
}

/// Digits that look like letters, read as letters: `0O` → `OO`. Only in a
/// part that has a letter already: a row of zeros (`00-000`) is not read
/// as `OO-OOO`.
fn letters_again(s: &str) -> String {
    if !s.chars().any(|c| c.is_ascii_alphabetic()) {
        return s.to_owned();
    }
    s.chars()
        .map(|c| match c {
            '0' => 'O',
            '1' => 'I',
            '5' => 'S',
            '8' => 'B',
            c => c,
        })
        .collect()
}

/// The registrations in one line of text: each word, and two words next
/// to each other read as one (`C-GUP G`).
pub fn registrations_in(text: &str) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if let Some(r) = registration(words[i]) {
            out.push(r);
        } else if let Some(r) = words
            .get(i + 1)
            .and_then(|next| registration(&format!("{}{next}", words[i])))
        {
            out.push(r);
            i += 1;
        }
        i += 1;
    }
    out.dedup();
    out
}

/// Upper-case words of letters and digits: `Air-Canada ✈` → `AIR CANADA`.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_uppercase)
        .collect()
}

/// Is `read` the start of `name` with the rest hidden (by an engine, a
/// wing or the frame), but for one character changed, added or missing?
/// Only for names of seven or more, with at least seven tenths of the name
/// read: `TRAINSA` for `TRANSAT` (its `T` behind the engine).
fn cut_off(read: &str, name: &str) -> bool {
    let (r, n): (Vec<char>, Vec<char>) = (read.chars().collect(), name.chars().collect());
    if n.len() < 7 || r.len() < 6 || r.len() > n.len() {
        return false;
    }
    let least = (n.len() * 7).div_ceil(10);
    (r.len().saturating_sub(1)..=r.len() + 1)
        .filter(|&k| k >= least && k < n.len())
        .any(|k| within_one(read, &n[..k].iter().collect::<String>()))
}

/// Words a company's registered name adds to the airline's: `Emirates
/// Airline`, `JetBlue Airways`, `TUI Airways Ltd`.
const COMPANY_WORDS: [&str; 18] = [
    "AIR",
    "AIRLINE",
    "AIRLINES",
    "AIRWAYS",
    "LINES",
    "AVIATION",
    "INTERNATIONAL",
    "GROUP",
    "INC",
    "LTD",
    "LIMITED",
    "LLC",
    "CORP",
    "CORPORATION",
    "CO",
    "COMPANY",
    "PLC",
    "AG",
];

/// Are the two the same but for one character changed, added or missing?
fn within_one(a: &str, b: &str) -> bool {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let (short, long) = if a.len() <= b.len() {
        (&a, &b)
    } else {
        (&b, &a)
    };
    if long.len() - short.len() > 1 {
        return false;
    }
    let start = short
        .iter()
        .zip(long.iter())
        .take_while(|(x, y)| x == y)
        .count();
    if long.len() == short.len() {
        short[start..]
            .iter()
            .skip(1)
            .eq(long[start..].iter().skip(1))
    } else {
        short[start..].iter().eq(long[start..].iter().skip(1))
    }
}

/// The airline list.
#[derive(Debug, Clone)]
pub struct Airlines {
    /// (name, aliases as upper-case words).
    entries: Vec<(String, Vec<Vec<String>>)>,
}

impl Airlines {
    pub fn builtin() -> Self {
        Self::parse(AIRLINES_TSV)
    }

    pub fn parse(tsv: &str) -> Self {
        let mut entries = Vec::new();
        for line in tsv.lines() {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let mut cols = line.split('\t');
            let name = cols.next().unwrap_or_default().trim();
            if name.is_empty() {
                continue;
            }
            let mut aliases = vec![words(name)];
            aliases.retain(|a| !a.is_empty());
            for a in cols.next().unwrap_or_default().split('|') {
                let w = words(a);
                if !w.is_empty() && !aliases.contains(&w) {
                    aliases.push(w);
                }
            }
            entries.push((name.to_owned(), aliases));
        }
        Self { entries }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The airline a company's registered name is (an online registry's
    /// owner): the airline's name followed only by company words
    /// ([`COMPANY_WORDS`]). A leasing company or a bank is none.
    pub fn owner(&self, owner: &str) -> Option<&str> {
        let mut w = words(owner);
        while w.len() > 1
            && w.last()
                .is_some_and(|x| COMPANY_WORDS.contains(&x.as_str()))
        {
            w.pop();
        }
        let joined = w.concat();
        self.entries
            .iter()
            .find(|(_, aliases)| aliases.iter().any(|a| a.concat() == joined))
            .map(|(name, _)| name.as_str())
    }

    /// The airline a line of text names, if any. The longest name wins
    /// (`Air Canada Rouge` over `Air Canada`).
    pub fn find(&self, text: &str) -> Option<&str> {
        let line = words(text);
        if line.is_empty() {
            return None;
        }
        let joined = line.concat();
        let mut best: Option<(usize, &str)> = None;
        // A name with its end hidden counts only when nothing else does.
        let mut cut: Option<(usize, &str)> = None;
        for (name, aliases) in &self.entries {
            for alias in aliases {
                let compact = alias.concat();
                let n = compact.chars().count();
                // The whole line is the name: exactly; or with one stray
                // character after a name of five or more; or, for a name
                // of seven or more, with one character wrong.
                let whole = joined == compact
                    || (n >= 5
                        && joined
                            .strip_prefix(compact.as_str())
                            .is_some_and(|rest| rest.chars().count() == 1))
                    || (n >= 7 && within_one(&joined, &compact));
                if line.len() == 1
                    && cut_off(&joined, &compact)
                    && cut.is_none_or(|(len, _)| n < len)
                {
                    cut = Some((n, name));
                }
                // Or a name of several words is somewhere in the line. One
                // word inside a longer line (UNITED STATES OF AMERICA) is
                // not enough.
                let inside =
                    alias.len() >= 2 && line.windows(alias.len()).any(|w| w == alias.as_slice());
                let hit = whole || inside;
                if hit && best.is_none_or(|(len, _)| n > len) {
                    best = Some((n, name));
                }
            }
        }
        best.or(cut).map(|(_, name)| name)
    }
}

/// Digits misread as letters, read as digits again: `30D` → `300`.
fn digits_again(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'O' | 'D' | 'Q' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .collect()
}

/// A Boeing variant as written after the model: `300ER`, `8`, `9`, `800`.
/// The digits come first; a misread one among them is put right.
fn boeing_variant(s: &str) -> Option<String> {
    let s: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let first = s.chars().next()?;
    if !first.is_ascii_digit() || first == '0' {
        return None;
    }
    // One digit, or a digit and two zeros, then letters (ER, LR, F, …).
    let (num, rest) = if s.len() >= 3 && digits_again(&s[1..3]) == "00" {
        (format!("{first}00"), &s[3..])
    } else {
        (first.to_string(), &s[1..])
    };
    (rest.len() <= 3 && rest.chars().all(|c| c.is_ascii_uppercase()))
        .then(|| format!("{num}{rest}"))
}

/// The aircraft type a line of text spells out, as it is usually written:
/// `BOEING 777-30DER` → `Boeing 777-300ER`.
pub fn painted_type(text: &str) -> Option<String> {
    let upper = text.to_uppercase();
    let toks: Vec<&str> = upper
        .split(|c: char| c.is_whitespace())
        .filter(|t| !t.is_empty())
        .collect();
    let after = |name: &str| toks.iter().position(|t| *t == name).map(|i| &toks[i + 1..]);

    if let Some(rest) = after("BOEING") {
        let first = rest.first()?;
        let (model, variant) = match first.split_once('-') {
            Some((m, v)) => (m, Some(v.to_owned())),
            None => (*first, None),
        };
        let model = digits_again(model);
        if !(model.len() == 3 && model.starts_with('7') && model.ends_with('7'))
            || !model.chars().all(|c| c.is_ascii_digit())
        {
            return None;
        }
        if rest.get(1) == Some(&"MAX") {
            let n = rest
                .get(2)
                .filter(|n| matches!(**n, "7" | "8" | "9" | "10"));
            return Some(match n {
                Some(n) => format!("Boeing {model} MAX {n}"),
                None => format!("Boeing {model} MAX"),
            });
        }
        return Some(match variant.as_deref().and_then(boeing_variant) {
            Some(v) => format!("Boeing {model}-{v}"),
            None => format!("Boeing {model}"),
        });
    }
    if let Some(rest) = after("AIRBUS") {
        let first = rest.first()?;
        let (model, variant) = match first.split_once('-') {
            Some((m, v)) => (m, Some(v)),
            None => (*first, None),
        };
        let digits = digits_again(model.strip_prefix('A')?);
        if digits.len() != 3 || !digits.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        // The variant's digits (at most four), then letters: `300`, `200F`.
        let variant = variant
            .map(|v| {
                let chars: Vec<char> = v.chars().collect();
                let n = chars
                    .iter()
                    .take(4)
                    .take_while(|c| c.is_ascii_digit() || "ODQIL".contains(**c))
                    .count();
                let head: String = chars[..n].iter().collect();
                let tail: String = chars[n..].iter().collect();
                format!("{}{tail}", digits_again(&head))
            })
            .filter(|v| {
                !v.is_empty() && v.len() <= 7 && v.chars().all(|c| c.is_ascii_alphanumeric())
            });
        return Some(match variant {
            Some(v) => format!("Airbus A{digits}-{v}"),
            None => format!("Airbus A{digits}"),
        });
    }
    if toks.contains(&"DREAMLINER") {
        return Some("Boeing 787 Dreamliner".to_owned());
    }
    if let Some(rest) = after("EMBRAER") {
        let n = rest.first()?.trim_start_matches('E');
        return matches!(n, "170" | "175" | "190" | "195").then(|| format!("Embraer E{n}"));
    }
    if toks.len() <= 3 {
        match toks.as_slice() {
            ["Q400"] | ["DASH", "8", "Q400"] | ["Q400", "NEXTGEN"] => {
                return Some("De Havilland Dash 8 Q400".to_owned())
            }
            ["ATR", m] if m.starts_with("72") || m.starts_with("42") => {
                return Some(format!("ATR {}", &m[..2]))
            }
            ["A380"]
            | ["A350"]
            | ["A350", "XWB"]
            | ["A340"]
            | ["A330"]
            | ["A320"]
            | ["A321"]
            | ["A220"] => return Some(format!("Airbus {}", toks[0])),
            ["A320NEO"] | ["A321NEO"] | ["A330NEO"] => {
                return Some(format!("Airbus {}neo", &toks[0][..4]))
            }
            _ => {}
        }
    }
    None
}

/// What the text in a picture says about the aircraft in it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Marks {
    /// Registrations, the most surely read first.
    pub registrations: Vec<String>,
    /// Airlines, biggest titles first.
    pub airlines: Vec<String>,
    /// Types spelt out on the aircraft.
    pub types: Vec<String>,
}

impl Marks {
    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty() && self.airlines.is_empty() && self.types.is_empty()
    }
}

fn add(list: &mut Vec<String>, v: String) {
    if list.len() < MAX_EACH && !list.contains(&v) {
        list.push(v);
    }
}

/// How sure the line a mark was read from must be, for each kind of mark
/// (0–1). The user sets them in the review window; the defaults are
/// [`REGISTRATION_MIN`], [`NAME_MIN`] and [`TYPE_MIN`].
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Confidence {
    pub registration: f32,
    pub airline: f32,
    pub aircraft_type: f32,
}

impl Default for Confidence {
    fn default() -> Self {
        Self {
            registration: REGISTRATION_MIN,
            airline: NAME_MIN,
            aircraft_type: TYPE_MIN,
        }
    }
}

impl Confidence {
    /// Each level kept between 0.5 (what is read at all) and 1.
    pub fn clamped(self) -> Self {
        let c = |v: f32| {
            if v.is_finite() {
                v.clamp(TEXT_FLOOR, 1.0)
            } else {
                TEXT_FLOOR
            }
        };
        Self {
            registration: c(self.registration),
            airline: c(self.airline),
            aircraft_type: c(self.aircraft_type),
        }
    }
}

/// The lowest confidence a level can be set to: lines read less surely
/// than this are not kept ([`super::ocr::TEXT_MIN`]).
pub const TEXT_FLOOR: f32 = super::ocr::TEXT_MIN;

/// Every mark the text could give, each with the confidence of the surest
/// line it was read from (for a title in two parts, the less sure part).
/// [`Candidates::at`] keeps those sure enough.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Candidates {
    /// Most surely read first.
    pub registrations: Vec<(String, f32)>,
    /// Biggest titles first.
    pub airlines: Vec<(String, f32)>,
    /// Most surely read first.
    pub types: Vec<(String, f32)>,
}

fn note(list: &mut Vec<(String, f32)>, v: String, score: f32) {
    match list.iter_mut().find(|(n, _)| *n == v) {
        Some(e) => e.1 = e.1.max(score),
        None => list.push((v, score)),
    }
}

/// Every mark in a picture's text lines (as [`super::ocr`] gives them,
/// biggest first), whatever its confidence.
pub fn candidates(lines: &[TextLine], airlines: &Airlines) -> Candidates {
    let mut c = Candidates::default();
    let mut by_score: Vec<&TextLine> = lines.iter().collect();
    by_score.sort_by(|a, b| b.score.total_cmp(&a.score));
    for l in &by_score {
        for r in registrations_in(&l.text) {
            note(&mut c.registrations, r, l.score);
        }
        if let Some(t) = painted_type(&l.text) {
            note(&mut c.types, t, l.score);
        }
    }
    for l in lines {
        if let Some(a) = airlines.find(&l.text) {
            note(&mut c.airlines, a.to_owned(), l.score);
        }
    }
    // Titles in two parts: lines of about the same size, left then right.
    let top = &lines[..lines.len().min(PAIR_LINES)];
    for a in top {
        for b in top {
            let same_size = {
                let r = a.height() / b.height().max(f32::MIN_POSITIVE);
                (0.6..=1.6).contains(&r)
            };
            if !std::ptr::eq(a, b) && same_size && a.center()[0] < b.center()[0] {
                if let Some(name) = airlines.find(&format!("{} {}", a.text, b.text)) {
                    note(&mut c.airlines, name.to_owned(), a.score.min(b.score));
                }
            }
        }
    }
    c
}

impl Candidates {
    /// The marks read at least as surely as `conf` asks, at most
    /// [`MAX_EACH`] of each.
    pub fn at(&self, conf: &Confidence) -> Marks {
        let keep = |list: &[(String, f32)], min: f32| -> Vec<String> {
            let mut out = Vec::new();
            for (name, score) in list {
                if *score >= min {
                    add(&mut out, name.clone());
                }
            }
            out
        };
        let mut m = Marks {
            registrations: keep(&self.registrations, conf.registration),
            airlines: keep(&self.airlines, conf.airline),
            types: keep(&self.types, conf.aircraft_type),
        };
        // A name that is part of a longer one found too says nothing more.
        let all = m.airlines.clone();
        m.airlines.retain(|a| {
            !all.iter()
                .any(|o| o != a && o.to_uppercase().contains(&a.to_uppercase()))
        });
        m
    }

    /// The confidence a mark was read with (`None`: not a candidate).
    pub fn score_of(&self, kind: MarkKind, name: &str) -> Option<f32> {
        let list = match kind {
            MarkKind::Registration => &self.registrations,
            MarkKind::Airline => &self.airlines,
            MarkKind::Type => &self.types,
        };
        list.iter().find(|(n, _)| n == name).map(|(_, s)| *s)
    }
}

/// The kinds of mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkKind {
    Registration,
    Airline,
    Type,
}

/// Read the marks out of a picture's text lines at the default confidence.
pub fn interpret(lines: &[TextLine], airlines: &Airlines) -> Marks {
    candidates(lines, airlines).at(&Confidence::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, score: f32, x: f32, h: f32) -> TextLine {
        TextLine {
            text: text.to_owned(),
            score,
            quad: [[x, 0.0], [x + 100.0, 0.0], [x + 100.0, h], [x, h]],
        }
    }

    #[test]
    fn registrations_are_recognised_by_their_shape() {
        for (word, want) in [
            ("C-FJZS", Some("C-FJZS")),
            ("*C-FJZS", Some("C-FJZS")),
            ("c-gtsj", Some("C-GTSJ")),
            ("D-AIKR", Some("D-AIKR")),
            ("PH-BQB", Some("PH-BQB")),
            ("G-EUUU", Some("G-EUUU")),
            ("A6-EDA", Some("A6-EDA")),
            ("9V-SKA", Some("9V-SKA")),
            ("B-18901", Some("B-18901")),
            ("VH–OQA", Some("VH-OQA")),
            ("N123AB", Some("N123AB")),
            ("N12345", Some("N12345")),
            ("N731", Some("N731")),
            ("JA801A", Some("JA801A")),
            ("HL7402", Some("HL7402")),
            // Not marks.
            ("C-ABCD", None),
            ("CFJZS", None),
            ("DELTA", None),
            ("NO", None),
            ("N1", None),
            ("N0123", None),
            ("N12I", None),
            ("NEWS", None),
            ("777-300", None),
            ("A220-300", None),
            ("EL-KAYS", None),
            ("", None),
        ] {
            assert_eq!(registration(word).as_deref(), want, "{word}");
        }
    }

    #[test]
    fn a_line_can_hold_a_registration_among_other_things() {
        assert_eq!(registrations_in("I* C-GUPG"), ["C-GUPG"]);
        assert_eq!(registrations_in("C-GUP G"), ["C-GUPG"]);
        assert_eq!(registrations_in("* C-FJZS ="), ["C-FJZS"]);
        assert!(registrations_in("AIR CANADA").is_empty());
        assert!(registrations_in("STAR ALLIANCE").is_empty());
    }

    #[test]
    fn airlines_are_found_as_painted() {
        let a = Airlines::builtin();
        assert!(a.len() >= 100);
        for (text, want) in [
            ("AIR CANADA", Some("Air Canada")),
            ("Lufthansa", Some("Lufthansa")),
            ("ufthansa", Some("Lufthansa")),
            ("KLM", Some("KLM")),
            ("KLN", None),
            ("AIRFRANCEK", Some("Air France")),
            ("flairo", Some("Flair Airlines")),
            ("AIR CANADA rouge", Some("Air Canada Rouge")),
            ("BRITISH AIRWAYS", Some("British Airways")),
            ("STAR ALLIANCE", None),
            ("UNITED", Some("United Airlines")),
            ("UNITED STATES OF AMERICA", None),
            ("KLM Royal Dutch Airlines", Some("KLM")),
            ("NO STEP", None),
            ("AIR", None),
            ("749", None),
            ("", None),
        ] {
            assert_eq!(a.find(text), want, "{text}");
        }
    }

    #[test]
    fn one_wrong_character_is_close_enough_only_for_long_names() {
        assert!(within_one("LUFTHANSA", "UFTHANSA"));
        assert!(within_one("LUFTHANSA", "LUFTHANZA"));
        assert!(within_one("LUFTHANSA", "LUFTHANSAS"));
        assert!(!within_one("LUFTHANSA", "UFTHANZA"));
        assert!(!within_one("ABC", "CBA"));
    }

    #[test]
    fn a_painted_type_is_written_the_usual_way() {
        for (text, want) in [
            ("BOEING 777-30DER", Some("Boeing 777-300ER")),
            ("BOEING 777-300ER", Some("Boeing 777-300ER")),
            ("BOEING 787-9", Some("Boeing 787-9")),
            ("BOEING 737-800", Some("Boeing 737-800")),
            ("BOEING 737 MAX 8", Some("Boeing 737 MAX 8")),
            ("BOEING 767", Some("Boeing 767")),
            ("AIRBUS A220-300", Some("Airbus A220-300")),
            ("AIRBUS A32O", Some("Airbus A320")),
            ("AIRBUS A350-900", Some("Airbus A350-900")),
            ("DREAMLINER", Some("Boeing 787 Dreamliner")),
            ("787 DREAMLINER", Some("Boeing 787 Dreamliner")),
            ("A380", Some("Airbus A380")),
            ("Q400", Some("De Havilland Dash 8 Q400")),
            ("EMBRAER 175", Some("Embraer E175")),
            ("BOEING", None),
            ("BOEING FIELD", None),
            ("AIR CANADA", None),
            ("A380 IS GREAT TODAY", None),
        ] {
            assert_eq!(painted_type(text).as_deref(), want, "{text}");
        }
    }

    #[test]
    fn a_picture_s_lines_give_its_marks() {
        let a = Airlines::builtin();
        let lines = [
            line("AIR CANADA", 1.0, 100.0, 78.0),
            line("748", 1.0, 900.0, 45.0),
            line("*C-FJZS", 0.93, 800.0, 38.0),
            line("=", 0.76, 700.0, 30.0),
            line("BOEING 777-30DER", 0.92, 300.0, 24.0),
        ];
        let m = interpret(&lines, &a);
        assert_eq!(m.registrations, ["C-FJZS"]);
        assert_eq!(m.airlines, ["Air Canada"]);
        assert_eq!(m.types, ["Boeing 777-300ER"]);
        assert!(interpret(&[line("ahmad elkays", 0.96, 0.0, 100.0)], &a).is_empty());
    }

    #[test]
    fn a_title_in_two_parts_is_joined_and_unsure_lines_are_left_out() {
        let a = Airlines::builtin();
        // CANADA is left of AIR here: not a title.
        let wrong_way = [
            line("CANADA", 1.0, 0.0, 53.0),
            line("AIR", 0.96, 500.0, 44.0),
        ];
        assert!(interpret(&wrong_way, &a).airlines.is_empty());
        let split = [
            line("CANADA", 1.0, 500.0, 53.0),
            line("AIR", 0.96, 0.0, 44.0),
        ];
        assert_eq!(interpret(&split, &a).airlines, ["Air Canada"]);
        // Too unsure to trust.
        let faint = [line("C-FJZS", 0.6, 0.0, 30.0), line("KLM", 0.7, 0.0, 90.0)];
        assert!(interpret(&faint, &a).is_empty());
        // The longer name is kept alone.
        let rouge = [
            line("AIR CANADA", 1.0, 0.0, 60.0),
            line("AIR CANADA rouge", 0.9, 0.0, 50.0),
        ];
        assert_eq!(interpret(&rouge, &a).airlines, ["Air Canada Rouge"]);
    }

    #[test]
    fn letters_read_as_digits_in_a_registration_are_put_right() {
        // A7202143.jpg: Brussels Airlines' OO-SFO read as `0O-SFO`.
        assert_eq!(registration("0O-SFO").as_deref(), Some("OO-SFO"));
        assert_eq!(registration("OO-SF0").as_deref(), Some("OO-SFO"));
        // Real prefixes with digits stay as they are.
        assert_eq!(registration("A6-EDA").as_deref(), Some("A6-EDA"));
        assert_eq!(registration("9V-SKA").as_deref(), Some("9V-SKA"));
        assert_eq!(registration("B-18901").as_deref(), Some("B-18901"));
        // No country's shape either way: nothing.
        assert_eq!(registration("00-000"), None);
        assert_eq!(registration("12-345"), None);
    }

    #[test]
    fn a_name_cut_off_by_the_engine_is_still_read() {
        let a = Airlines::builtin();
        // DSC08634: Air Transat's `transat`, its `t` behind the engine.
        assert_eq!(a.find("trainsa"), Some("Air Transat"));
        assert_eq!(a.find("SUNWIN"), Some("Sunwing Airlines"));
        // Too little of the name, or an ordinary word: nothing.
        assert_eq!(a.find("TRANS"), None);
        assert_eq!(a.find("BRITISH"), None);
        assert_eq!(a.find("PARKING"), None);
        assert_eq!(a.find("EMIRATESX"), Some("Emirates"));
    }

    #[test]
    fn a_registered_owner_is_its_airline_without_the_company_words() {
        let a = Airlines::builtin();
        assert_eq!(a.owner("Emirates Airline"), Some("Emirates"));
        assert_eq!(a.owner("JetBlue Airways"), Some("JetBlue"));
        assert_eq!(a.owner("TUI Airways Ltd"), Some("TUI"));
        assert_eq!(a.owner("Air Canada"), Some("Air Canada"));
        assert_eq!(a.owner("WestJet"), Some("WestJet"));
        assert_eq!(a.owner("Wells Fargo Trust Co"), None);
        assert_eq!(a.owner("Delta Leasing"), None);
        assert_eq!(a.owner("Newton Air Inc"), None);
    }

    #[test]
    fn a_jet_airways_777_is_read_from_what_the_owner_s_photo_gave() {
        // DSC05972 (24 December 2017), as read: the airline with its J cut
        // off, the type in mixed case and a little unsure.
        let a = Airlines::builtin();
        let lines = [
            line("ETAIRWAYS", 1.0, 100.0, 66.0),
            line("000000000000000", 0.83, 400.0, 48.0),
            line("VT-JEX", 1.0, 700.0, 44.0),
            line("BoeinG 777-300eR", 0.81, 300.0, 30.0),
        ];
        let m = interpret(&lines, &a);
        assert_eq!(m.registrations, ["VT-JEX"]);
        assert_eq!(m.airlines, ["Jet Airways"]);
        assert_eq!(m.types, ["Boeing 777-300ER"]);
        // A type is trusted from a little less certainty than a name, but
        // not from a guess.
        assert_eq!(
            interpret(&[line("BOEING 787-9", 0.72, 0.0, 30.0)], &a).types,
            ["Boeing 787-9"]
        );
        assert!(interpret(&[line("BOEING 787-9", 0.6, 0.0, 30.0)], &a).is_empty());
        assert!(interpret(&[line("BOEING", 0.99, 0.0, 30.0)], &a).is_empty());
        // Airlines that no longer fly are known, for older photos.
        for (painted, name) in [
            ("JET AIRWAYS", "Jet Airways"),
            ("airberlin", "Air Berlin"),
            ("US AIRWAYS", "US Airways"),
            ("kingfisher", "Kingfisher Airlines"),
        ] {
            assert_eq!(a.find(painted), Some(name), "{painted}");
        }
    }
}

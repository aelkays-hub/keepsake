//! File-naming templates.
//!
//! Tokens: `{yyyy} {mm} {dd} {hh}` (capture time in the camera's local zone),
//! `{month}` (the month's name in English: `October`),
//! `{event} {camera} {lens} {seq} {orig}`. `/` separates folders. The original
//! extension is always appended by the caller. Every rendered folder and file
//! name is made safe for Windows and macOS.

use std::fmt;

use chrono::{Datelike, NaiveDateTime, Timelike};
use thiserror::Error;

use crate::relpath::{nfc, RelPath};

/// The default: one folder per year, one per day, camera file names kept.
pub const DEFAULT_TEMPLATE: &str = "{yyyy}/{yyyy}-{mm}-{dd}/{orig}";

/// Year, month name, day: `2026/October/2026-10-30/A7400123`.
pub const YEAR_MONTH_DAY_TEMPLATE: &str = "{yyyy}/{month}/{yyyy}-{mm}-{dd}/{orig}";

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Longest folder or base name produced (bytes), leaving room for a
/// collision suffix, a Sony suffix and an extension within 255.
pub const MAX_COMPONENT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TemplateError {
    #[error("unknown template token {{{0}}}")]
    UnknownToken(String),
    #[error("unclosed '{{' in template")]
    Unclosed,
    #[error("template is empty")]
    Empty,
    #[error("template must produce a relative path without '..'")]
    NotRelative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token {
    Year,
    Month,
    MonthName,
    Day,
    Hour,
    Event,
    Camera,
    Lens,
    Seq,
    Orig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(String),
    Token(Token),
    Sep,
}

/// A parsed template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    source: String,
    parts: Vec<Part>,
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.source)
    }
}

/// Values for one asset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemplateInput {
    /// Capture time on the camera's clock (local time).
    pub local_time: Option<NaiveDateTime>,
    pub event: Option<String>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    /// Original file stem, e.g. `C0001` or `DSC00001`.
    pub orig: String,
}

impl Template {
    pub fn parse(s: &str) -> Result<Self, TemplateError> {
        let t = s.trim();
        if t.is_empty() {
            return Err(TemplateError::Empty);
        }
        if t.starts_with(['/', '\\']) {
            return Err(TemplateError::NotRelative);
        }
        let mut parts = Vec::new();
        let mut text = String::new();
        let mut chars = t.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '{' => {
                    if !text.is_empty() {
                        parts.push(Part::Text(std::mem::take(&mut text)));
                    }
                    let mut name = String::new();
                    loop {
                        match chars.next() {
                            Some('}') => break,
                            Some(c) => name.push(c),
                            None => return Err(TemplateError::Unclosed),
                        }
                    }
                    let tok = match name.as_str() {
                        "yyyy" => Token::Year,
                        "mm" => Token::Month,
                        "month" => Token::MonthName,
                        "dd" => Token::Day,
                        "hh" => Token::Hour,
                        "event" => Token::Event,
                        "camera" => Token::Camera,
                        "lens" => Token::Lens,
                        "seq" => Token::Seq,
                        "orig" => Token::Orig,
                        _ => return Err(TemplateError::UnknownToken(name)),
                    };
                    parts.push(Part::Token(tok));
                }
                '/' | '\\' => {
                    if !text.is_empty() {
                        parts.push(Part::Text(std::mem::take(&mut text)));
                    }
                    parts.push(Part::Sep);
                }
                _ => text.push(c),
            }
        }
        if !text.is_empty() {
            parts.push(Part::Text(text));
        }
        let tpl = Self {
            source: t.to_owned(),
            parts,
        };
        // Literal ".." folders are rejected outright.
        let probe = tpl.render_raw(&TemplateInput::default(), None);
        if probe.iter().any(|c| c == "..") {
            return Err(TemplateError::NotRelative);
        }
        Ok(tpl)
    }

    pub fn uses_seq(&self) -> bool {
        self.parts.contains(&Part::Token(Token::Seq))
    }

    /// Components before sanitising. `seq = None` renders `{seq}` as empty.
    fn render_raw(&self, v: &TemplateInput, seq: Option<u32>) -> Vec<String> {
        let mut out = vec![String::new()];
        let date = v.local_time;
        let or_unknown = |s: &Option<String>| {
            s.as_deref()
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .unwrap_or("unknown")
                .to_owned()
        };
        for p in &self.parts {
            let piece = match p {
                Part::Sep => {
                    out.push(String::new());
                    continue;
                }
                Part::Text(t) => t.clone(),
                Part::Token(Token::Year) => {
                    date.map_or("0000".into(), |d| format!("{:04}", d.year()))
                }
                Part::Token(Token::Month) => {
                    date.map_or("00".into(), |d| format!("{:02}", d.month()))
                }
                Part::Token(Token::MonthName) => date.map_or("Undated".into(), |d| {
                    MONTHS[d.month0() as usize % 12].to_owned()
                }),
                Part::Token(Token::Day) => date.map_or("00".into(), |d| format!("{:02}", d.day())),
                Part::Token(Token::Hour) => {
                    date.map_or("00".into(), |d| format!("{:02}", d.hour()))
                }
                Part::Token(Token::Event) => or_unknown(&v.event),
                Part::Token(Token::Camera) => or_unknown(&v.camera),
                Part::Token(Token::Lens) => or_unknown(&v.lens),
                Part::Token(Token::Seq) => seq.map_or(String::new(), |n| format!("{n:04}")),
                Part::Token(Token::Orig) => v.orig.clone(),
            };
            if let Some(last) = out.last_mut() {
                last.push_str(&piece);
            }
        }
        out
    }

    /// Render to a relative path *without* extension: sanitised folders and a
    /// sanitised base name. Undated assets go under `0000-undated` when the
    /// template starts with a date folder.
    pub fn render(&self, v: &TemplateInput, seq: Option<u32>) -> Result<RelPath, TemplateError> {
        let mut comps: Vec<String> = self
            .render_raw(v, seq)
            .into_iter()
            .map(|c| sanitize_component(&c))
            .filter(|c| !c.is_empty())
            .collect();
        if v.local_time.is_none() {
            if let Some(first) = comps.first_mut() {
                if first == "0000" {
                    *first = "0000-undated".into();
                }
            }
        }
        if comps.is_empty() {
            return Err(TemplateError::Empty);
        }
        RelPath::parse(&comps.join("/")).map_err(|_| TemplateError::NotRelative)
    }

    /// The folder part of [`Self::render`] with `{seq}` left empty: assets
    /// sharing it are numbered together.
    pub fn seq_scope(&self, v: &TemplateInput) -> String {
        let r = self.render_raw(v, None);
        r[..r.len().saturating_sub(1)]
            .iter()
            .map(|c| sanitize_component(c))
            .collect::<Vec<_>>()
            .join("/")
    }
}

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Make one folder or file name safe on Windows and macOS: NFC, no
/// `<>:"/\|?*` or control characters, no trailing dots or spaces, no reserved
/// device names, at most [`MAX_COMPONENT`] bytes. `.` and `..` become `_`.
pub fn sanitize_component(s: &str) -> String {
    let mut out: String = nfc(s)
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    out = out.trim_start().trim_end_matches([' ', '.']).to_owned();
    if out.len() > MAX_COMPONENT {
        let mut cut = MAX_COMPONENT;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        out = out.trim_end_matches([' ', '.']).to_owned();
    }
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        out.push('_');
    }
    if out.is_empty() || out.chars().all(|c| c == '.') {
        if s.is_empty() {
            return String::new();
        }
        return "_".into();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn input() -> TemplateInput {
        TemplateInput {
            local_time: NaiveDate::from_ymd_opt(2024, 6, 15)
                .unwrap()
                .and_hms_opt(13, 30, 0),
            event: Some("Okinawa trip".into()),
            camera: Some("ILCE-7M4".into()),
            lens: Some("FE 24-70mm F2.8 GM II".into()),
            orig: "C0001".into(),
        }
    }

    #[test]
    fn year_month_name_day_template() {
        let t = Template::parse(YEAR_MONTH_DAY_TEMPLATE).unwrap();
        assert_eq!(
            t.render(&input(), None).unwrap().as_str(),
            "2024/June/2024-06-15/C0001"
        );
        let jan = TemplateInput {
            local_time: NaiveDate::from_ymd_opt(2026, 1, 3)
                .unwrap()
                .and_hms_opt(8, 0, 0),
            ..input()
        };
        assert_eq!(
            t.render(&jan, None).unwrap().as_str(),
            "2026/January/2026-01-03/C0001"
        );
        let undated = TemplateInput {
            local_time: None,
            ..input()
        };
        assert_eq!(
            t.render(&undated, None).unwrap().as_str(),
            "0000-undated/Undated/0000-00-00/C0001"
        );
    }

    #[test]
    fn default_template() {
        let t = Template::parse(DEFAULT_TEMPLATE).unwrap();
        assert_eq!(
            t.render(&input(), None).unwrap().as_str(),
            "2024/2024-06-15/C0001"
        );
    }

    #[test]
    fn every_token() {
        let t =
            Template::parse("{yyyy}{mm}{dd}-{hh}/{event}/{camera}_{lens}_{seq}_{orig}").unwrap();
        assert!(t.uses_seq());
        assert_eq!(
            t.render(&input(), Some(7)).unwrap().as_str(),
            "20240615-13/Okinawa trip/ILCE-7M4_FE 24-70mm F2.8 GM II_0007_C0001"
        );
        assert_eq!(t.seq_scope(&input()), "20240615-13/Okinawa trip");
    }

    #[test]
    fn missing_values() {
        let t = Template::parse("{yyyy}/{yyyy}-{mm}-{dd}/{event}/{orig}").unwrap();
        let v = TemplateInput {
            orig: "DSC00001".into(),
            ..Default::default()
        };
        assert_eq!(
            t.render(&v, None).unwrap().as_str(),
            "0000-undated/0000-00-00/unknown/DSC00001"
        );
    }

    #[test]
    fn bad_templates() {
        assert_eq!(
            Template::parse("{nope}"),
            Err(TemplateError::UnknownToken("nope".into()))
        );
        assert_eq!(Template::parse("{yyyy"), Err(TemplateError::Unclosed));
        assert_eq!(Template::parse("  "), Err(TemplateError::Empty));
        assert_eq!(
            Template::parse("/abs/{orig}"),
            Err(TemplateError::NotRelative)
        );
        assert_eq!(
            Template::parse("../{orig}"),
            Err(TemplateError::NotRelative)
        );
    }

    #[test]
    fn sanitizing() {
        assert_eq!(sanitize_component("a<b>c:d\"e|f?g*h"), "a_b_c_d_e_f_g_h");
        assert_eq!(sanitize_component("trail. . "), "trail");
        assert_eq!(sanitize_component("CON"), "CON_");
        assert_eq!(sanitize_component("nul.txt"), "nul.txt_");
        assert_eq!(sanitize_component(".."), "_");
        assert_eq!(sanitize_component("tab\there"), "tab_here");
        assert_eq!(sanitize_component("Cafe\u{301}"), "Caf\u{e9}");
        let long = "é".repeat(300);
        let s = sanitize_component(&long);
        assert!(s.len() <= MAX_COMPONENT && s.is_char_boundary(s.len()));
        // An event named with a slash cannot create folders.
        let mut v = input();
        v.event = Some("a/b\\c".into());
        let t = Template::parse("{event}/{orig}").unwrap();
        assert_eq!(t.render(&v, None).unwrap().as_str(), "a_b_c/C0001");
    }
}

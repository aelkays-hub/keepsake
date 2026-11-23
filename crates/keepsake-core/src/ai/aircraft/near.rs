//! A registration read one letter off (ADR-051): small letters are read as
//! others that look like them (`JA02NJ` for JA02WJ, A7405081.ARW: at about
//! 8 px the W is read as N). When what was read is not a known aircraft,
//! the marks one look-alike letter away ([`variants`]) that keep a
//! country's shape are checked against what is known: the national
//! registers ([`super::register::near`]) and the online lookups' answers
//! (adsbdb, then Flightradar24). One known one is offered in place of what was read, which is
//! offered beside it unticked ([`fix`]).
//!
//! - The aircraft worker asks the lookup about those marks after a read
//!   ([`ask_variants`]): only when what was read is not known, at most
//!   [`MAX_ASKS`] of them (more and the guess says too little), each answer
//!   kept like any lookup. When adsbdb knows none of them (nor what was
//!   read), Flightradar24's lookup asks about them all
//!   ([`super::fr24::near_due`]); one it saw fly counts as known.
//! - When more than one is known, the one whose type or airline agrees with
//!   what the photo shows is offered; otherwise none.

use super::lookup::{self, Found, Lookup};
use super::marks;
use crate::ai::AiError;
use crate::catalog::Catalog;

/// The most marks asked about for one read.
pub const MAX_ASKS: usize = 6;

/// Characters the text reader takes for one another.
pub(crate) fn look_alikes(c: u8) -> &'static [u8] {
    match c {
        b'0' => b"ODQ",
        b'O' => b"0DQ",
        b'D' => b"0O",
        b'Q' => b"0O",
        b'1' => b"IL",
        b'I' => b"1L",
        b'L' => b"1I",
        b'2' => b"Z",
        b'Z' => b"2",
        b'5' => b"S",
        b'S' => b"5",
        b'8' => b"B",
        b'B' => b"8",
        b'6' => b"G",
        b'G' => b"6",
        b'7' => b"T",
        b'T' => b"7",
        b'4' => b"A",
        b'A' => b"4",
        b'U' => b"V",
        b'V' => b"UW",
        // Small letters with strokes run together (A7405081: W read as N).
        b'N' => b"WMH",
        b'W' => b"NV",
        b'M' => b"NW",
        b'H' => b"N",
        _ => b"",
    }
}

/// How much of a mark is its country (left as read): up to the hyphen;
/// `N` of a US mark; `JA` and `HL`.
fn country_len(r: &str) -> usize {
    if let Some(i) = r.find('-') {
        i + 1
    } else if r.starts_with("JA") || r.starts_with("HL") {
        2
    } else {
        usize::from(r.starts_with('N'))
    }
}

/// The marks one look-alike character away from `registration` that are
/// still shaped like a registration (the country left as read), in order
/// along the mark.
pub fn variants(registration: &str) -> Vec<String> {
    let r = registration.trim().to_ascii_uppercase();
    if !r.is_ascii() {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    let mut b = r.clone().into_bytes();
    for i in country_len(&r)..b.len() {
        let was = b[i];
        for &c in look_alikes(was) {
            b[i] = c;
            if let Ok(s) = std::str::from_utf8(&b) {
                if marks::registration(s).as_deref() == Some(s) && !out.iter().any(|o| o == s) {
                    out.push(s.to_owned());
                }
            }
        }
        b[i] = was;
    }
    out
}

/// What the photo shows besides the registration, to choose between known
/// marks: its types (painted, or the one it looks like) and airlines.
#[derive(Debug, Clone, Default)]
pub struct Hints {
    pub types: Vec<String>,
    pub airlines: Vec<String>,
}

impl Hints {
    fn agree(&self, f: &Found) -> bool {
        let ty = f.aircraft_type.as_deref().is_some_and(|t| {
            self.types
                .iter()
                .any(|h| super::scan::same_type(h, t) || super::scan::same_type(t, h))
        });
        let al = f
            .airline
            .as_deref()
            .is_some_and(|a| self.airlines.iter().any(|h| h.eq_ignore_ascii_case(a)));
        ty || al
    }
}

/// Who knows `registration`, from the answers kept: adsbdb, else
/// Flightradar24 (it saw it fly, in the last 30 days of answers kept), with
/// what they say of it.
fn kept_known(cat: &Catalog, registration: &str) -> Result<Option<(Found, &'static str)>, AiError> {
    let Some(f) = lookup::kept(cat, registration)? else {
        return Ok(None);
    };
    let by = if f.known {
        lookup::SERVICE
    } else {
        super::fr24::FR24_NAME
    };
    Ok(super::fr24::with_fr24(&f, crate::jobs::now_ms()).map(|f| (f, by)))
}

/// Is `registration` a known aircraft: in a national register, or known
/// to adsbdb or Flightradar24?
fn known(cat: &Catalog, registration: &str) -> Result<bool, AiError> {
    if super::register::get(cat, registration)?.is_some() {
        return Ok(true);
    }
    Ok(kept_known(cat, registration)?.is_some())
}

/// The known aircraft `registration` was most likely read off, with who
/// knows it (`register`, [`lookup::SERVICE`] or Flightradar24); `None` when
/// what was read is known itself, or no one mark stands out.
pub fn fix(
    cat: &Catalog,
    registration: &str,
    hints: &Hints,
) -> Result<Option<(String, &'static str)>, AiError> {
    if known(cat, registration)? {
        return Ok(None);
    }
    if let Some(real) = super::register::near(cat, registration)? {
        return Ok(Some((real, "register")));
    }
    let mut found: Vec<(Found, &'static str)> = Vec::new();
    for v in variants(registration) {
        if let Some(k) = kept_known(cat, &v)? {
            found.push(k);
        }
    }
    if found.len() > 1 {
        found.retain(|(f, _)| hints.agree(f));
    }
    Ok((found.len() == 1).then(|| {
        let (f, by) = found.remove(0);
        (f.registration, by)
    }))
}

/// After a read: when `registration` is not known (the lookup was asked
/// and does not know it, and no register has it), ask the lookup about its
/// [`variants`] not asked before, at most [`MAX_ASKS`] (none when there
/// are more). `pause` runs between two asks; it returns false to stop.
/// Returns how many were asked.
pub fn ask_variants(
    cat: &mut Catalog,
    lookup: &dyn Lookup,
    registration: &str,
    pause: &mut dyn FnMut() -> bool,
) -> Result<usize, AiError> {
    if lookup::kept(cat, registration)?.is_none() || known(cat, registration)? {
        return Ok(0);
    }
    if super::register::near(cat, registration)?.is_some() {
        return Ok(0);
    }
    let vs = variants(registration);
    if vs.len() > MAX_ASKS {
        return Ok(0);
    }
    let mut n = 0;
    for v in vs {
        if lookup::kept(cat, &v)?.is_some_and(|f| !f.stale()) {
            continue;
        }
        if n > 0 && !pause() {
            break;
        }
        lookup::lookup_cached(cat, Some(lookup), &v)?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_change_one_look_alike_and_keep_the_country_and_its_shape() {
        // A7405081: the W of JA02WJ read as N.
        let v = variants("JA02NJ");
        assert!(v.contains(&"JA02WJ".to_owned()), "{v:?}");
        assert!(v.contains(&"JA02MJ".to_owned()));
        // The digits after JA stay digits: no JAO2NJ, JA0ZNJ.
        assert!(
            v.iter().all(|s| s.starts_with("JA0") && &s[3..4] == "2"),
            "{v:?}"
        );
        assert!(v.len() <= MAX_ASKS, "{v:?}");
        // The country stays: C-FJZS gives no G-… or CS-….
        assert!(variants("C-FJZS").iter().all(|s| s.starts_with("C-F")));
        assert!(!variants("C-FJZS").contains(&"C-FJ2S".to_owned()));
        // Nothing for what is not a mark.
        assert!(variants("ÉTÉ").is_empty());
    }
}

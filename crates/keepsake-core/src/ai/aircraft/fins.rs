//! Fin (fleet) numbers (ADR-052): which registration an airline's fin stood
//! for. An airline paints a number on its aircraft, unique in that airline
//! at one time: Air Canada on the tail and the nose gear doors, US
//! airlines on the tail and under the cockpit windows. Read aircraft keeps
//! the fins it reads ([`super::marks::fin`]); this keeps the table:
//!
//! - **Learned** ([`learn`]): a photo accepted with one registration and one
//!   airline, whose read gave one fin surely enough, teaches that pair, its
//!   dates stretched to the photo's time.
//! - **Imported** ([`parse_csv`], [`import`]): a CSV file the user chose,
//!   `airline,fin,registration[,from][,to]`, for example pasted from the
//!   airline's fleet page. Nothing is shipped with Keepsake: no free source
//!   gives fin to registration.
//!
//! A fin is reused when its aircraft leaves, so a row holds when it was so.
//! Two registrations for one fin at overlapping times are a conflict
//! ([`conflicts`]): neither is offered ticked.

use chrono::{NaiveDate, NaiveTime};

use super::marks::{self, Airlines, Candidates};
use crate::ai::AiError;
use crate::catalog::{AssetId, Catalog, FinRow, TagPath};

/// A fin is learned only when read at least this surely.
pub const LEARN_MIN: f32 = 0.9;
/// `learned`.
pub const LEARNED: &str = "learned";
/// `imported`.
pub const IMPORTED: &str = "imported";
/// The CSV columns, as exported (the first three are needed to import).
pub const CSV_HEADER: &str = "Airline,Fin,Registration,From,To,Source,Seen";

/// The one registration and one airline among accepted `paths`, if so.
fn accepted_pair(paths: &[TagPath]) -> Option<(String, String)> {
    // The same keyword can come twice (typed and given by the reader).
    let of = |kind: &str| -> Vec<&String> {
        let mut v: Vec<&String> = paths
            .iter()
            .map(TagPath::segments)
            .filter(|s| s.len() == 3 && s[0] == super::scan::TAG_PARENT && s[1] == kind)
            .map(|s| &s[2])
            .collect();
        v.sort();
        v.dedup();
        v
    };
    match (of("Registration").as_slice(), of("Airline").as_slice()) {
        ([r], [a]) => Some(((*r).clone(), (*a).clone())),
        _ => None,
    }
}

/// The one fin read at least [`LEARN_MIN`] surely (`None`: none, or two
/// different ones).
fn sure_fin(c: &Candidates) -> Option<&str> {
    let mut sure = c
        .fins
        .iter()
        .filter(|(_, s)| *s >= LEARN_MIN)
        .map(|(f, _)| f);
    let first = sure.next()?;
    sure.all(|f| f == first).then_some(first.as_str())
}

/// Learn from photos `ids` accepted with `paths`: each whose read gave one
/// sure fin teaches it, when one registration and one airline were
/// accepted. Returns how many photos taught one.
pub fn learn(cat: &mut Catalog, ids: &[AssetId], paths: &[TagPath]) -> Result<u64, AiError> {
    let Some((registration, airline)) = accepted_pair(paths) else {
        return Ok(0);
    };
    let mut taught: Vec<(String, Option<i64>)> = Vec::new();
    for id in ids {
        let Some(read) = cat.read(|tx| tx.ai().aircraft_read(*id))? else {
            continue;
        };
        let c: Candidates = serde_json::from_str(&read.candidates).unwrap_or_default();
        if let Some(fin) = sure_fin(&c) {
            let at = cat.read(|tx| tx.ai().taken_ms(*id))?;
            taught.push((fin.to_owned(), at));
        }
    }
    if taught.is_empty() {
        return Ok(0);
    }
    let now = crate::jobs::now_ms();
    cat.write(|tx| {
        for (fin, at) in &taught {
            tx.ai()
                .learn_fin(&airline, fin, &registration, *at, true, now)?;
        }
        Ok::<_, crate::catalog::CatalogError>(())
    })?;
    Ok(taught.len() as u64)
}

/// Go over the library: every photo whose read kept a sure fin and that
/// has one registration and one airline keyword (accepted, typed or given
/// by its burst) teaches that pair. Photos read before fins were kept have
/// none: Read again gives them theirs. Gone over again, nothing is counted
/// twice. Returns how many photos taught one.
pub fn learn_from_library(cat: &mut Catalog) -> Result<u64, AiError> {
    let reads = cat.read(|tx| tx.ai().aircraft_reads_with_fins())?;
    let mut taught: Vec<(String, String, String, Option<i64>)> = Vec::new();
    for (id, json) in reads {
        let c: Candidates = serde_json::from_str(&json).unwrap_or_default();
        let Some(fin) = sure_fin(&c) else {
            continue;
        };
        let paths: Vec<TagPath> = cat
            .read(|tx| tx.tags().for_asset(id))?
            .into_iter()
            .map(|t| t.path)
            .collect();
        let Some((registration, airline)) = accepted_pair(&paths) else {
            continue;
        };
        let at = cat.read(|tx| tx.ai().taken_ms(id))?;
        taught.push((airline, fin.to_owned(), registration, at));
    }
    let now = crate::jobs::now_ms();
    cat.write(|tx| {
        for (airline, fin, registration, at) in &taught {
            tx.ai()
                .learn_fin(airline, fin, registration, *at, false, now)?;
        }
        Ok::<_, crate::catalog::CatalogError>(())
    })?;
    Ok(taught.len() as u64)
}

/// What a CSV file held: the rows that can be kept, and what is wrong with
/// the others (`line 3: …`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Parsed {
    pub rows: Vec<FinRow>,
    pub problems: Vec<String>,
}

/// The fields of one line: by tab, else by semicolon when it has no comma,
/// else by comma (quoted fields may hold commas).
fn fields(line: &str) -> Vec<String> {
    if line.contains('\t') {
        line.split('\t').map(|f| f.trim().to_owned()).collect()
    } else if !line.contains(',') && line.contains(';') {
        line.split(';').map(|f| f.trim().to_owned()).collect()
    } else {
        super::register::csv_fields(line)
    }
}

/// `2016`, `2016-03` or `2016-03-05`, as the first (`end` false) or last
/// millisecond of that year, month or day, UTC.
pub fn date_ms(text: &str, end: bool) -> Option<i64> {
    let t = text.trim();
    let parts: Vec<&str> = t.split('-').collect();
    let num = |s: &str| s.parse::<u32>().ok();
    let (y, m, d) = match parts.as_slice() {
        [y] if y.len() == 4 => (num(y)? as i32, None, None),
        [y, m] if y.len() == 4 => (num(y)? as i32, Some(num(m)?), None),
        [y, m, d] if y.len() == 4 => (num(y)? as i32, Some(num(m)?), Some(num(d)?)),
        _ => return None,
    };
    let first = NaiveDate::from_ymd_opt(y, m.unwrap_or(1), d.unwrap_or(1))?;
    let day = if !end {
        first
    } else {
        match (m, d) {
            (_, Some(_)) => first,
            (Some(m), None) => {
                let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
                NaiveDate::from_ymd_opt(ny, nm, 1)?.pred_opt()?
            }
            (None, _) => NaiveDate::from_ymd_opt(y, 12, 31)?,
        }
    };
    let time = if end {
        NaiveTime::from_hms_milli_opt(23, 59, 59, 999)?
    } else {
        NaiveTime::MIN
    };
    Some(day.and_time(time).and_utc().timestamp_millis())
}

/// `2016-03-05` (UTC) for a time kept.
pub fn day_text(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|t| t.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// Read a CSV file's text: `airline,fin,registration[,from][,to]`, one per
/// line, a header line (naming `fin`) skipped, `#` lines and blank ones
/// too. The airline must be one Keepsake knows (as it names it after); the
/// registration is put in its usual form.
pub fn parse_csv(text: &str, airlines: &Airlines) -> Parsed {
    let mut out = Parsed::default();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let f = fields(t);
        if i == 0 && f.iter().any(|x| x.eq_ignore_ascii_case("fin")) {
            continue;
        }
        let mut bad = |why: String| out.problems.push(format!("line {n}: {why}"));
        if f.len() < 3 {
            bad("needs airline, fin and registration".into());
            continue;
        }
        let Some(airline) = airlines.find(&f[0]) else {
            bad(format!("{} is not an airline Keepsake knows", f[0]));
            continue;
        };
        let Some(fin) = marks::fin(&f[1]) else {
            bad(format!("{} is not a fin (2 to 4 digits)", f[1]));
            continue;
        };
        let Some(registration) = marks::registration(&f[2].to_uppercase()) else {
            bad(format!("{} is not a registration", f[2]));
            continue;
        };
        let date = |k: usize, end: bool| -> Result<Option<i64>, String> {
            match f.get(k).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                None => Ok(None),
                Some(s) => date_ms(s, end)
                    .map(Some)
                    .ok_or_else(|| format!("{s} is not a date (2016, 2016-03 or 2016-03-05)")),
            }
        };
        let (first_ms, last_ms) = match (date(3, false), date(4, true)) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                bad(e);
                continue;
            }
        };
        if let (Some(a), Some(b)) = (first_ms, last_ms) {
            if a > b {
                bad("from is after to".into());
                continue;
            }
        }
        out.rows.push(FinRow {
            airline: airline.to_owned(),
            fin,
            registration,
            first_ms,
            last_ms,
            source: IMPORTED.into(),
            seen: 0,
        });
    }
    out
}

/// Keep imported `rows`: each added, or its dates replaced. Returns how
/// many were added and how many updated.
pub fn import(cat: &mut Catalog, rows: &[FinRow]) -> Result<(u64, u64), AiError> {
    let now = crate::jobs::now_ms();
    Ok(cat.write(|tx| {
        let (mut added, mut updated) = (0, 0);
        for r in rows {
            if tx.ai().import_fin(r, now)? {
                added += 1;
            } else {
                updated += 1;
            }
        }
        Ok::<_, crate::catalog::CatalogError>((added, updated))
    })?)
}

/// The table as CSV ([`CSV_HEADER`]; CRLF line ends), which
/// [`parse_csv`] reads back.
pub fn to_csv(rows: &[FinRow]) -> String {
    let mut out = String::from(CSV_HEADER);
    out.push_str("\r\n");
    let day = |ms: Option<i64>| ms.map(day_text).unwrap_or_default();
    for r in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\r\n",
            crate::report::field(&r.airline),
            r.fin,
            crate::report::field(&r.registration),
            day(r.first_ms),
            day(r.last_ms),
            r.source,
            r.seen
        ));
    }
    out
}

/// Whether two rows' times overlap (`None`: open at that end).
fn overlap(a: &FinRow, b: &FinRow) -> bool {
    let starts_before_end = |x: &FinRow, y: &FinRow| match (x.first_ms, y.last_ms) {
        (Some(f), Some(l)) => f <= l,
        _ => true,
    };
    starts_before_end(a, b) && starts_before_end(b, a)
}

/// For each row, whether another registration has the same airline and fin
/// at an overlapping time.
pub fn conflicts(rows: &[FinRow]) -> Vec<bool> {
    rows.iter()
        .map(|a| {
            rows.iter().any(|b| {
                b.airline == a.airline
                    && b.fin == a.fin
                    && b.registration != a.registration
                    && overlap(a, b)
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(fin: &str, reg: &str, first: Option<i64>, last: Option<i64>) -> FinRow {
        FinRow {
            airline: "Air Canada".into(),
            fin: fin.into(),
            registration: reg.into(),
            first_ms: first,
            last_ms: last,
            source: LEARNED.into(),
            seen: 1,
        }
    }

    #[test]
    fn dates_are_a_year_a_month_or_a_day() {
        let d = |t, end| date_ms(t, end).map(day_text);
        assert_eq!(d("2016", false).as_deref(), Some("2016-01-01"));
        assert_eq!(d("2016", true).as_deref(), Some("2016-12-31"));
        assert_eq!(d("2016-02", true).as_deref(), Some("2016-02-29"));
        assert_eq!(d("2016-12", true).as_deref(), Some("2016-12-31"));
        assert_eq!(d("2016-03-05", true).as_deref(), Some("2016-03-05"));
        assert!(date_ms("2016-03-05", true) > date_ms("2016-03-05", false));
        for no in ["16", "2016-13", "2016-02-30", "March 2016", "2016/03/05"] {
            assert_eq!(date_ms(no, false), None, "{no}");
        }
    }

    #[test]
    fn a_csv_file_is_read_with_its_problems_said() {
        let text = "\u{feff}Airline,Fin,Registration,From,To\n\
                    Air Canada,147,c-gjyi,2019,\n\
                    # a note\n\
                    \n\
                    AIR CANADA;401;C-FPWD\n\
                    Air Canada\t951\tC-GHKR\t2020-03\t2024-06\n\
                    Nowhere Air,12,C-FAAA\n\
                    Air Canada,A12,C-FAAA\n\
                    Air Canada,12,HELLO\n\
                    Air Canada,12\n\
                    Air Canada,12,C-FAAA,2020,2019\n\
                    Air Canada,12,C-FAAA,soon\n";
        let p = parse_csv(text, &Airlines::builtin());
        let got: Vec<(&str, &str, &str)> = p
            .rows
            .iter()
            .map(|r| (r.airline.as_str(), r.fin.as_str(), r.registration.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("Air Canada", "147", "C-GJYI"),
                ("Air Canada", "401", "C-FPWD"),
                ("Air Canada", "951", "C-GHKR"),
            ]
        );
        assert_eq!(p.rows[0].first_ms, date_ms("2019", false));
        assert_eq!(p.rows[0].last_ms, None);
        assert_eq!(p.rows[2].last_ms, date_ms("2024-06", true));
        assert!(p.rows.iter().all(|r| r.source == IMPORTED));
        let lines: Vec<&str> = p
            .problems
            .iter()
            .map(|s| s.split(':').next().unwrap_or_default())
            .collect();
        assert_eq!(
            lines,
            ["line 7", "line 8", "line 9", "line 10", "line 11", "line 12"],
            "{:?}",
            p.problems
        );
    }

    #[test]
    fn the_table_is_written_as_csv_that_reads_back() {
        let rows = vec![
            row(
                "147",
                "C-GJYI",
                date_ms("2019-05-05", false),
                date_ms("2024-05-05", true),
            ),
            row("401", "C-FPWD", None, None),
        ];
        let csv = to_csv(&rows);
        assert!(csv.starts_with(CSV_HEADER));
        assert!(csv.contains("Air Canada,147,C-GJYI,2019-05-05,2024-05-05,learned,1\r\n"));
        let back = parse_csv(&csv, &Airlines::builtin());
        assert!(back.problems.is_empty(), "{:?}", back.problems);
        assert_eq!(back.rows.len(), 2);
        assert_eq!(back.rows[0].first_ms, rows[0].first_ms);
    }

    #[test]
    fn two_registrations_for_one_fin_at_once_are_a_conflict() {
        let y = |t| date_ms(t, false);
        let rows = vec![
            // A fin reused: one aircraft after the other.
            row("147", "C-GJYI", y("2019"), y("2020")),
            row("147", "C-FAAA", y("2021"), y("2022")),
            // At the same time: a conflict, also with an open end.
            row("401", "C-FPWD", y("2019"), y("2022")),
            row("401", "C-GBBB", y("2021"), None),
            // The same registration twice (learned and imported): fine.
            row("951", "C-GHKR", None, None),
            FinRow {
                source: IMPORTED.into(),
                ..row("951", "C-GHKR", None, None)
            },
        ];
        assert_eq!(conflicts(&rows), [false, false, true, true, false, false]);
    }

    #[test]
    fn only_one_sure_fin_is_learned() {
        let c = |fins: &[(&str, f32)]| Candidates {
            fins: fins.iter().map(|(f, s)| ((*f).to_owned(), *s)).collect(),
            ..Candidates::default()
        };
        assert_eq!(sure_fin(&c(&[("147", 0.97)])), Some("147"));
        assert_eq!(sure_fin(&c(&[("147", 0.97), ("41", 0.6)])), Some("147"));
        assert_eq!(sure_fin(&c(&[("147", 0.8)])), None);
        assert_eq!(sure_fin(&c(&[("147", 0.97), ("401", 0.95)])), None);
        assert_eq!(sure_fin(&c(&[])), None);
    }

    #[test]
    fn one_registration_and_one_airline_must_be_accepted() {
        let p = |s: &str| TagPath::parse_lightroom(s).unwrap();
        let reg = p("Aircraft|Registration|C-GJYI");
        let air = p("Aircraft|Airline|Air Canada");
        let typ = p("Aircraft|Type|Airbus A220-300");
        assert_eq!(
            accepted_pair(&[reg.clone(), air.clone(), typ.clone()]),
            Some(("C-GJYI".into(), "Air Canada".into()))
        );
        assert_eq!(
            accepted_pair(&[reg.clone(), air.clone(), reg.clone()]),
            Some(("C-GJYI".into(), "Air Canada".into()))
        );
        assert_eq!(accepted_pair(&[reg.clone(), typ]), None);
        let two = p("Aircraft|Registration|C-FAAA");
        assert_eq!(accepted_pair(&[reg, two, air]), None);
    }
}

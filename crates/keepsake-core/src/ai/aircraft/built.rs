//! When an aircraft was built (ADR-044), so its age can be worked out when
//! a photo is selected: today, and on the day of the photo.
//!
//! - The build year is a fact kept per registration (`aircraft_built`):
//!   from OpenSky's aircraft database, or typed by the user (which wins).
//!   The age is never stored: it would be wrong a year later.
//! - OpenSky's database is downloaded once to this computer when the user
//!   asks (Settings → Read aircraft), pinned to one snapshot and kept only
//!   if its BLAKE3 matches, then cut down to `aircraft-built.tsv`: each
//!   aircraft's transponder code and registration, and its build date when
//!   it has one. Looking a year or a code up there sends nothing. A file
//!   kept before the codes were (no [`HEADER`]) is offered again in
//!   Settings.
//! - Accepting `Aircraft|Built|<year>` in the review window puts it on a
//!   photo; typing it puts it on every photo of that registration.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use super::lookup::Found;
use super::scan::{lib_err, TAG_PARENT};
use crate::ai::install::{download_checked, remove_model_at, ModelFile};
use crate::ai::AiError;
use crate::catalog::{Catalog, TagPath};
use crate::library::Library;
use crate::metadata::MetadataService;

/// The keyword kind: `Aircraft|Built|2014`.
pub const KIND: &str = "Built";
/// Where a build year came from.
pub const FROM_OPENSKY: &str = "opensky";
pub const FROM_TYPED: &str = "typed";
/// For people.
pub const OPENSKY_NAME: &str = "OpenSky";

/// OpenSky's aircraft database, the snapshot of April 2024 (the one with
/// the most build dates: a quarter of airliners), as published at
/// <https://opensky-network.org/datasets/metadata/>.
pub const DATABASE: ModelFile = ModelFile {
    dir: "opensky-aircraft",
    file: "aircraftDatabase-2024-04.csv",
    url: "https://s3.opensky-network.org/data-samples/metadata/aircraftDatabase-2024-04.csv",
    bytes: 94_509_542,
    blake3: "2f96e8533dc7aa3d9ed06ad5d5b0c49fdb8682aa190c608f1477b56760587ebe",
    env: "KEEPSAKE_OPENSKY_CSV",
};

/// What is kept of it: `icao24 \t registration \t built` (`built` may be
/// empty), after [`HEADER`].
pub const TSV_FILE: &str = "aircraft-built.tsv";
/// The kept file's first line since it keeps every aircraft's code.
pub const HEADER: &str = "#keepsake-aircraft\t2";
/// Overrides where the kept file is, for tests and developers.
pub const TSV_ENV: &str = "KEEPSAKE_AIRCRAFT_BUILT";

/// A build date as kept: `YYYY` or `YYYY-MM-DD`. `jan_first_is_year`:
/// OpenSky writes a year alone as `YYYY-01-01` (97% of its dates), so
/// that is read as the year alone.
pub fn normalize(s: &str, jan_first_is_year: bool) -> Option<String> {
    let s = s.trim();
    let y = s.get(..4)?;
    if !y.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i32 = y.parse().ok()?;
    if !(1900..=2100).contains(&year) {
        return None;
    }
    let rest = &s[4..];
    if rest.is_empty() {
        return Some(y.to_owned());
    }
    // A time after the date (`2014-03-05 00:00:00`) is dropped.
    let date = s.get(..10).unwrap_or(s);
    match chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(_) if jan_first_is_year && date.ends_with("-01-01") => Some(y.to_owned()),
        Ok(d) => Some(d.format("%Y-%m-%d").to_string()),
        // `2014-03`: the year.
        Err(_) if rest.len() <= 3 && rest.starts_with('-') => Some(y.to_owned()),
        Err(_) => None,
    }
}

/// The year of a kept build date.
pub fn year(built: &str) -> &str {
    built.get(..4).unwrap_or(built)
}

/// The keyword for a build date: `Aircraft|Built|2014`.
pub fn keyword(built: &str) -> Result<TagPath, AiError> {
    Ok(TagPath::new([TAG_PARENT, KIND, year(built)])?)
}

/// Registrations are compared without case, spaces or hyphens.
fn reg_key(r: &str) -> String {
    r.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

/// A transponder code (`C04826`), as a number.
fn icao_number(icao: &str) -> Option<u32> {
    let icao = icao.trim();
    (icao.len() == 6)
        .then(|| u32::from_str_radix(icao, 16).ok())
        .flatten()
}

/// The build dates of OpenSky's database, by transponder code and by
/// registration, and the transponder code of each registration.
#[derive(Debug, Default)]
pub struct BuiltDb {
    by_icao: HashMap<String, String>,
    by_reg: HashMap<String, String>,
    code_by_reg: HashMap<String, u32>,
    codes: bool,
}

impl BuiltDb {
    /// From the kept file's text.
    pub fn from_tsv(text: &str) -> Self {
        let mut db = Self::default();
        for line in text.lines() {
            if line.starts_with('#') {
                db.codes |= line == HEADER;
                continue;
            }
            let mut f = line.split('\t');
            let (Some(icao), Some(reg), Some(built)) = (f.next(), f.next(), f.next()) else {
                continue;
            };
            let k = reg_key(reg);
            if let (Some(n), false) = (icao_number(icao), k.is_empty()) {
                db.code_by_reg.insert(k, n);
            }
            let Some(built) = normalize(built, false) else {
                continue;
            };
            if !icao.is_empty() {
                db.by_icao.insert(icao.to_ascii_lowercase(), built.clone());
            }
            let k = reg_key(reg);
            if !k.is_empty() {
                db.by_reg.insert(k, built);
            }
        }
        db
    }

    /// The kept file.
    pub fn load(path: &Path) -> Result<Self, AiError> {
        Ok(Self::from_tsv(&std::fs::read_to_string(path)?))
    }

    /// The kept file, if it is on this computer.
    pub fn installed() -> Option<Self> {
        let p = tsv_path().filter(|p| p.is_file())?;
        match Self::load(&p) {
            Ok(db) => Some(db),
            Err(e) => {
                tracing::warn!("aircraft build years not read: {e}");
                None
            }
        }
    }

    /// How many aircraft have a build date.
    pub fn len(&self) -> usize {
        self.by_icao.len().max(self.by_reg.len())
    }

    /// The file keeps every aircraft's transponder code ([`HEADER`]).
    pub fn has_codes(&self) -> bool {
        self.codes
    }

    /// The transponder code of `registration` (`C04826`), any case or
    /// hyphen.
    pub fn code(&self, registration: &str) -> Option<String> {
        self.code_by_reg
            .get(&reg_key(registration))
            .map(|n| format!("{n:06X}"))
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// When the aircraft was built: by its transponder code (`mode_s`,
    /// which stays with the airframe), else by its registration.
    pub fn find(&self, mode_s: Option<&str>, registration: &str) -> Option<&str> {
        mode_s
            .and_then(|m| self.by_icao.get(&m.trim().to_ascii_lowercase()))
            .or_else(|| self.by_reg.get(&reg_key(registration)))
            .map(String::as_str)
    }
}

/// Where the kept file is (or goes).
pub fn tsv_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(TSV_ENV) {
        return Some(PathBuf::from(p));
    }
    DATABASE.user_path().map(|p| p.with_file_name(TSV_FILE))
}

/// OpenSky's build dates are on this computer.
pub fn installed() -> bool {
    tsv_path().is_some_and(|p| p.is_file())
}

/// The file on this computer keeps every aircraft's transponder code (its
/// first line is [`HEADER`]); one kept before is to be downloaded again.
pub fn installed_with_codes() -> bool {
    let Some(f) = tsv_path().and_then(|p| std::fs::File::open(p).ok()) else {
        return false;
    };
    let mut first = String::new();
    BufReader::new(f).read_line(&mut first).is_ok() && first.trim_end() == HEADER
}

/// Download OpenSky's database (`progress`: bytes so far, total), keep its
/// codes, registrations and build dates, and delete the rest. Returns how
/// many aircraft have a build date.
pub fn install(cancel: &AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<u64, AiError> {
    let csv = DATABASE
        .user_path()
        .ok_or_else(|| AiError::Download("no cache folder on this computer".into()))?;
    let tsv =
        tsv_path().ok_or_else(|| AiError::Download("no cache folder on this computer".into()))?;
    download_checked(
        DATABASE.url,
        &csv,
        DATABASE.bytes,
        DATABASE.blake3,
        cancel,
        progress,
    )?;
    let n = convert_file(&csv, &tsv)?;
    let _ = std::fs::remove_file(&csv);
    Ok(n)
}

/// Make the kept file from the database's CSV (written beside, then put
/// in place).
pub fn convert_file(csv: &Path, tsv: &Path) -> Result<u64, AiError> {
    if let Some(d) = tsv.parent() {
        std::fs::create_dir_all(d)?;
    }
    let part = tsv.with_extension("part");
    let mut out = BufWriter::new(std::fs::File::create(&part)?);
    let n = convert(BufReader::new(std::fs::File::open(csv)?), &mut out)?;
    out.flush()?;
    drop(out);
    std::fs::rename(&part, tsv)?;
    Ok(n)
}

/// Remove the kept file (and a download left over). `true` if one was there.
pub fn remove() -> Result<bool, AiError> {
    let mut any = false;
    if let Some(p) = DATABASE.user_path() {
        any |= remove_model_at(&p)?;
    }
    if let Some(p) = tsv_path() {
        any |= remove_model_at(&p)?;
    }
    Ok(any)
}

/// The aircraft of OpenSky's CSV that have a build date (`built`, else
/// `firstflightdate`) or both a transponder code and a registration, as
/// `icao24 \t registration \t built` lines after [`HEADER`]. Returns how
/// many have a build date.
pub fn convert(input: impl BufRead, out: &mut impl Write) -> Result<u64, AiError> {
    let mut rows = CsvRows::new(input);
    let head = rows
        .next()
        .transpose()?
        .ok_or_else(|| AiError::Download("OpenSky's database is empty".into()))?;
    let col = |name: &str| {
        head.iter()
            .position(|h| h.trim().eq_ignore_ascii_case(name))
    };
    let bad = || AiError::Download("OpenSky's database has other columns than expected".into());
    let (icao, reg, built) = (
        col("icao24").ok_or_else(bad)?,
        col("registration").ok_or_else(bad)?,
        col("built").ok_or_else(bad)?,
    );
    let first = col("firstflightdate");
    writeln!(out, "{HEADER}")?;
    let mut n = 0;
    for row in rows {
        let row = row?;
        let get = |i: usize| row.get(i).map(|s| s.trim()).unwrap_or("");
        let date =
            normalize(get(built), true).or_else(|| first.and_then(|f| normalize(get(f), true)));
        let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
        let (i, r) = (clean(get(icao)).to_ascii_lowercase(), clean(get(reg)));
        match &date {
            Some(_) if i.is_empty() && r.is_empty() => continue,
            None if icao_number(&i).is_none() || reg_key(&r).is_empty() => continue,
            _ => {}
        }
        writeln!(out, "{i}\t{r}\t{}", date.as_deref().unwrap_or(""))?;
        n += u64::from(date.is_some());
    }
    Ok(n)
}

/// The records of a CSV file: fields in double quotes may hold commas,
/// line breaks and doubled quotes.
struct CsvRows<R> {
    input: R,
    buf: Vec<u8>,
}

impl<R: BufRead> CsvRows<R> {
    fn new(input: R) -> Self {
        Self {
            input,
            buf: Vec::new(),
        }
    }
}

impl<R: BufRead> Iterator for CsvRows<R> {
    type Item = Result<Vec<String>, AiError>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut fields = Vec::new();
        let mut field = Vec::new();
        let mut quoted = false;
        let mut any = false;
        loop {
            self.buf.clear();
            match self.input.read_until(b'\n', &mut self.buf) {
                Ok(0) => {
                    if !any {
                        return None;
                    }
                    fields.push(String::from_utf8_lossy(&field).into_owned());
                    return Some(Ok(fields));
                }
                Ok(_) => any = true,
                Err(e) => return Some(Err(e.into())),
            }
            let mut i = 0;
            let b = &self.buf;
            while i < b.len() {
                let c = b[i];
                if quoted {
                    if c == b'"' {
                        if b.get(i + 1) == Some(&b'"') {
                            field.push(b'"');
                            i += 1;
                        } else {
                            quoted = false;
                        }
                    } else {
                        field.push(c);
                    }
                } else {
                    match c {
                        b'"' => quoted = true,
                        b',' => fields.push(
                            String::from_utf8_lossy(&std::mem::take(&mut field)).into_owned(),
                        ),
                        b'\r' | b'\n' => {}
                        _ => field.push(c),
                    }
                }
                i += 1;
            }
            if !quoted {
                fields.push(String::from_utf8_lossy(&field).into_owned());
                return Some(Ok(fields));
            }
        }
    }
}

/// A registration's build date as kept, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub built: String,
    /// [`FROM_OPENSKY`] or [`FROM_TYPED`].
    pub source: String,
}

/// What is kept about when `registration` was built.
pub fn kept(cat: &Catalog, registration: &str) -> Result<Option<Kept>, AiError> {
    Ok(cat
        .read(|tx| tx.ai().aircraft_built(registration))?
        .map(|(built, source)| Kept { built, source }))
}

/// Keep the build date a lookup found (a typed one stays).
pub fn note_found(cat: &mut Catalog, found: &Found) -> Result<(), AiError> {
    let Some(built) = found.built.as_deref().and_then(|b| normalize(b, false)) else {
        return Ok(());
    };
    let now = crate::jobs::now_ms();
    cat.write(|tx| {
        tx.ai()
            .put_aircraft_built(&found.registration, &built, FROM_OPENSKY, now)
    })?;
    Ok(())
}

/// The user typed when `registration` was built (`None` or blank: forget
/// it). Its photos get `Aircraft|Built|<year>` in place of another year.
/// Returns how many photos changed.
pub fn set_typed(
    cat: &mut Catalog,
    lib: &Library,
    registration: &str,
    built: Option<&str>,
) -> Result<u64, AiError> {
    let registration = registration.trim();
    let built = match built.map(str::trim).filter(|b| !b.is_empty()) {
        Some(b) => Some(normalize(b, false).ok_or_else(|| {
            AiError::Image(format!("{b} is not a year (2014) or a date (2014-03-05)"))
        })?),
        None => None,
    };
    let before = kept(cat, registration)?;
    let now = crate::jobs::now_ms();
    match &built {
        Some(b) => cat.write(|tx| tx.ai().put_aircraft_built(registration, b, FROM_TYPED, now))?,
        None => {
            cat.write(|tx| tx.ai().forget_aircraft_built(registration))?;
            // What the lookup found comes back.
            if let Some(f) = super::lookup::kept(cat, registration)? {
                note_found(cat, &f)?;
            }
        }
    }
    let new = built.as_deref().map(keyword).transpose()?;
    let old = before.as_ref().map(|k| keyword(&k.built)).transpose()?;
    let reg = TagPath::new([TAG_PARENT, "Registration", registration])?;
    let ids = cat.read(|tx| tx.tags().assets_with(&reg))?;
    let mut changed = 0;
    for id in ids {
        let mut svc = MetadataService::new(lib, cat);
        let had = svc.user_tags(id).map_err(lib_err)?;
        let mut tags: Vec<TagPath> = had
            .iter()
            .filter(|t| {
                let s = t.segments();
                let built_kw = s.len() == 3 && s[0] == TAG_PARENT && s[1] == KIND;
                // Typing replaces any year; forgetting removes the one typed.
                !(built_kw && (new.is_some() || old.as_ref() == Some(t)))
            })
            .cloned()
            .collect();
        if let Some(n) = &new {
            tags.push(n.clone());
        }
        if tags != had {
            svc.set_user_tags(id, &tags).map_err(lib_err)?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// After OpenSky's database is downloaded: the registrations already looked
/// up get their build date, and their transponder code when adsbdb gave
/// none, from it (nothing is sent; when each was asked stays). Returns how
/// many changed.
pub fn fill_from_db(cat: &mut Catalog, db: &BuiltDb) -> Result<u64, AiError> {
    let rows = cat.read(|tx| tx.ai().aircraft_lookups())?;
    let mut n = 0;
    for (reg, json, at) in rows {
        let Ok(mut f) = serde_json::from_str::<Found>(&json) else {
            continue;
        };
        let before = f.clone();
        if f.mode_s.is_none() {
            f.mode_s = db.code(&reg);
        }
        if let Some(b) = db.find(f.mode_s.as_deref(), &reg) {
            f.built = Some(b.to_owned());
        }
        if f == before {
            continue;
        }
        let json = serde_json::to_string(&f)
            .map_err(|e| AiError::Image(format!("the lookup could not be kept: {e}")))?;
        cat.write(|tx| tx.ai().put_aircraft_lookup(&reg, &json, at))?;
        note_found(cat, &f)?;
        n += 1;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_years_are_read_and_a_first_of_january_is_a_year() {
        assert_eq!(normalize("2014", false).as_deref(), Some("2014"));
        assert_eq!(
            normalize(" 2014-03-05 ", false).as_deref(),
            Some("2014-03-05")
        );
        assert_eq!(
            normalize("2014-03-05 00:00:00", true).as_deref(),
            Some("2014-03-05")
        );
        assert_eq!(normalize("2009-01-01", true).as_deref(), Some("2009"));
        assert_eq!(
            normalize("2009-01-01", false).as_deref(),
            Some("2009-01-01")
        );
        assert_eq!(normalize("2014-03", false).as_deref(), Some("2014"));
        for bad in ["", "14", "abcd", "1850", "2014-13-40x", "20a4"] {
            assert_eq!(normalize(bad, false), None, "{bad}");
        }
        assert_eq!(year("2014-03-05"), "2014");
        assert_eq!(
            keyword("2014-03-05").unwrap().to_lightroom(),
            "Aircraft|Built|2014"
        );
    }

    const CSV: &str =
        "\"icao24\",\"registration\",\"model\",\"built\",\"firstflightdate\",\"notes\"\n\
\"4ca7b4\",\"EI-EFY\",\"737-8AS\",\"2009-01-01\",\"\",\"\"\n\
\"c0174f\",\"C-FIVM\",\"777\",\"\",\"2008-04-11\",\"a note, with a comma\nand a line break\"\n\
\"aa9f6f\",\"N784AN\",\"777 \"\"ER\"\"\",\"2000-05-02\",\"\",\"\"\n\
\"000001\",\"SP-FGR\",\"An-2\",\"\",\"\",\"\"\n\
\"484f6e\",\"PH-BKO\",\"787-10\",\"\",\"\",\"\"\n\
\"\",\"G-NONE\",\"A320\",\"\",\"\",\"\"\n";

    #[test]
    fn the_database_is_cut_down_to_codes_registrations_and_dates() {
        let mut out = Vec::new();
        let n = convert(CSV.as_bytes(), &mut out).unwrap();
        assert_eq!(n, 3);
        let text = String::from_utf8(out).unwrap();
        assert_eq!(
            text,
            "#keepsake-aircraft\t2\n4ca7b4\tEI-EFY\t2009\nc0174f\tC-FIVM\t2008-04-11\n\
             aa9f6f\tN784AN\t2000-05-02\n000001\tSP-FGR\t\n484f6e\tPH-BKO\t\n"
        );
        let db = BuiltDb::from_tsv(&text);
        assert_eq!(db.len(), 3);
        assert!(db.has_codes());
        // The code of every aircraft, by registration; none without one.
        assert_eq!(db.code("ph-bko").as_deref(), Some("484F6E"));
        assert_eq!(db.code("SP-FGR").as_deref(), Some("000001"));
        assert_eq!(db.code("G-NONE"), None);
        // A file kept before the codes were: dates only, offered again.
        let old = BuiltDb::from_tsv("4ca7b4\tEI-EFY\t2009\n");
        assert!(!old.has_codes());
        assert_eq!(old.code("EI-EFY").as_deref(), Some("4CA7B4"));
        // By transponder code first, else by registration (any case or hyphen).
        assert_eq!(db.find(Some("4CA7B4"), "X-XXXX"), Some("2009"));
        assert_eq!(db.find(None, "cfivm"), Some("2008-04-11"));
        assert_eq!(db.find(Some("ffffff"), "N784AN"), Some("2000-05-02"));
        assert_eq!(db.find(None, "SP-FGR"), None);
        assert!(convert("\"a\",\"b\"\n".as_bytes(), &mut Vec::new()).is_err());
    }

    #[test]
    fn the_pin_is_whole() {
        assert_eq!(DATABASE.blake3.len(), 64);
        assert!(DATABASE.url.ends_with(DATABASE.file));
        assert_ne!(DATABASE.env, TSV_ENV);
    }
}

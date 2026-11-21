//! National aircraft registers (ADR-045): Transport Canada's Canadian
//! Civil Aircraft Register and the FAA's Releasable Aircraft Database,
//! downloaded when the user asks (Settings → Read aircraft) and imported
//! into the catalog (`aircraft_register`). Both are open data (Open
//! Government Licence – Canada; US government work), refreshed daily.
//!
//! - **Update now** asks the server whether the file changed since the one
//!   imported (`If-Modified-Since`): unchanged, nothing is downloaded.
//! - Kept per registration: maker, model, the type as Keepsake names it
//!   ([`type_name`]), serial, build date, transponder code, whether it is
//!   registered, and the owner when it is a company (the airline it is,
//!   when it is one). **No owner who is a person, and no address, is
//!   kept.**
//! - Used for the aircraft's build year, its transponder code (Add route),
//!   and the type and airline offered in Read aircraft ([`with_register`]);
//!   a registration read one character off a real one is offered as the
//!   real one ([`near`]).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::lookup::{self, Found};
use super::marks::Airlines;
use crate::ai::AiError;
use crate::catalog::{Catalog, RegisterRow, RegisterSource};

/// A national register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Register {
    /// Transport Canada (`C-…`).
    Tc,
    /// The FAA (`N…`).
    Faa,
}

impl Register {
    pub const ALL: [Register; 2] = [Register::Tc, Register::Faa];

    /// As kept (`tc`, `faa`).
    pub fn id(self) -> &'static str {
        match self {
            Self::Tc => "tc",
            Self::Faa => "faa",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.id() == id)
    }

    /// For people.
    pub fn name(self) -> &'static str {
        match self {
            Self::Tc => "Transport Canada",
            Self::Faa => "FAA",
        }
    }

    pub fn url(self) -> &'static str {
        match self {
            Self::Tc => "https://wwwapps.tc.gc.ca/Saf-Sec-Sur/2/CCARCS-RIACC/download/ccarcsdb.zip",
            Self::Faa => "https://registry.faa.gov/database/ReleasableAircraft.zip",
        }
    }

    /// About how big the download is (for its progress).
    pub fn bytes(self) -> u64 {
        match self {
            Self::Tc => 4_600_000,
            Self::Faa => 73_000_000,
        }
    }

    /// The register a registration belongs to.
    pub fn of(registration: &str) -> Option<Self> {
        let r = registration.trim().to_ascii_uppercase();
        if r.starts_with("C-") || r.starts_with("CF-") {
            Some(Self::Tc)
        } else if r.len() >= 2
            && r.starts_with('N')
            && r.as_bytes()[1].is_ascii_digit()
            && !r.contains('-')
        {
            Some(Self::Faa)
        } else {
            None
        }
    }
}

fn bad(what: &str, e: impl std::fmt::Display) -> AiError {
    AiError::Download(format!("the {what} could not be read: {e}"))
}

/// One line of a comma-separated file, fields trimmed (quoted fields may
/// hold commas; `""` is a quote).
pub fn csv_fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => {
                out.push(field.trim().to_owned());
                field.clear();
            }
            (c, _) => field.push(c),
        }
    }
    out.push(field.trim().to_owned());
    out
}

/// The lines of `r`, read as Latin-1 (Transport Canada) or UTF-8 (the
/// FAA, without its byte-order mark).
fn lines(r: impl Read, latin1: bool) -> impl Iterator<Item = String> {
    let mut br = BufReader::new(r);
    std::iter::from_fn(move || {
        let mut buf = Vec::new();
        match br.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => None,
            Ok(_) => {
                while buf.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                    buf.pop();
                }
                let s = if latin1 {
                    buf.iter().map(|&b| char::from(b)).collect()
                } else {
                    String::from_utf8_lossy(&buf).into_owned()
                };
                Some(s.trim_start_matches('\u{feff}').to_owned())
            }
        }
    })
}

fn some(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

/// A transponder code as kept: six hex digits, upper case.
fn hex_code(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() == 24 && s.bytes().all(|b| b == b'0' || b == b'1') {
        return u32::from_str_radix(s, 2).ok().map(|n| format!("{n:06X}"));
    }
    (s.len() == 6 && s.bytes().all(|b| b.is_ascii_hexdigit())).then(|| s.to_ascii_uppercase())
}

/// A maker as people say it: `BOEING` → `Boeing`, `AIRBUS INDUSTRIE`
/// → `Airbus`, `BOMBARDIER INC` → `Bombardier`, `EMBRAER S A` → `Embraer`.
pub fn maker_name(raw: &str) -> String {
    let up = raw.trim().to_ascii_uppercase();
    for (key, name) in [
        ("AIRBUS HELICOPTERS", "Airbus Helicopters"),
        ("AIRBUS", "Airbus"),
        ("BOEING", "Boeing"),
        ("BOMBARDIER", "Bombardier"),
        ("CANADAIR", "Canadair"),
        ("EMBRAER", "Embraer"),
        ("MCDONNELL DOUGLAS", "McDonnell Douglas"),
        ("DE HAVILLAND", "De Havilland"),
        ("ATR", "ATR"),
        ("LOCKHEED", "Lockheed"),
        ("CESSNA", "Cessna"),
        ("PIPER", "Piper"),
        ("BEECH", "Beechcraft"),
        ("TEXTRON", "Textron"),
        ("GULFSTREAM", "Gulfstream"),
        ("DASSAULT", "Dassault"),
        ("PILATUS", "Pilatus"),
    ] {
        if up.starts_with(key) {
            return name.to_owned();
        }
    }
    // Title case, without company words.
    up.split_whitespace()
        .filter(|w| {
            !matches!(
                *w,
                "INC" | "INC." | "LTD" | "LTD." | "LLC" | "CO" | "CORP" | "S" | "A" | "SA" | "SAS"
            )
        })
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_string() + &c.as_str().to_ascii_lowercase())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The ICAO type designator of a register's model, when it is an airliner
/// Keepsake knows (`737-990ER` → `B739`).
fn designator(maker: &str, model: &str) -> Option<&'static str> {
    let maker = maker.to_ascii_uppercase();
    let m = model.trim().to_ascii_uppercase().replace(' ', "");
    // `CL-600-2D24(SERIES900)` → `CL-600-2D24`.
    let m = m.split('(').next().unwrap_or(&m).to_owned();
    let after = |p: &str| m.strip_prefix(p).map(str::to_owned);
    if maker.contains("BOEING") {
        // 737-8, 737-9: the MAX; 737-8CT, 737-824: a 737-800.
        if let Some(r) = after("737-") {
            let max = match r.as_str() {
                "7" => Some("B37M"),
                "8" | "8200" => Some("B38M"),
                "9" => Some("B39M"),
                "10" => Some("B3XM"),
                _ => None,
            };
            if max.is_some() {
                return max;
            }
            return match r.chars().next()? {
                '3' => Some("B733"),
                '4' => Some("B734"),
                '5' => Some("B735"),
                '6' => Some("B736"),
                '7' => Some("B737"),
                '8' => Some("B738"),
                '9' => Some("B739"),
                _ => None,
            };
        }
        if m == "717-200" || m.starts_with("717-2") {
            return Some("B712");
        }
        if let Some(r) = after("747-") {
            return match r.chars().next()? {
                '4' => Some("B744"),
                '8' => Some("B748"),
                _ => None,
            };
        }
        if let Some(r) = after("757-") {
            return match r.chars().next()? {
                '2' => Some("B752"),
                '3' => Some("B753"),
                _ => None,
            };
        }
        if let Some(r) = after("767-") {
            return match r.chars().next()? {
                '2' => Some("B762"),
                '3' => Some("B763"),
                '4' => Some("B764"),
                _ => None,
            };
        }
        if let Some(r) = after("777-") {
            return match (r.chars().next()?, r.ends_with("LR"), r.ends_with("ER")) {
                ('2', true, _) => Some("B77L"),
                ('2', false, _) => Some("B772"),
                ('3', _, true) => Some("B77W"),
                ('3', _, false) => Some("B773"),
                _ => None,
            };
        }
        if let Some(r) = after("787-") {
            return match r.as_str() {
                "8" => Some("B788"),
                "9" => Some("B789"),
                "10" => Some("B78X"),
                _ => None,
            };
        }
        return None;
    }
    if m.starts_with("BD-500-1A10") {
        return Some("BCS1");
    }
    if m.starts_with("BD-500-1A11") {
        return Some("BCS3");
    }
    if maker.contains("AIRBUS") {
        let neo = m.ends_with('N') || m.ends_with("NX") || m.ends_with("NY");
        for (fam, ceo, n) in [
            ("A319-", "A319", "A19N"),
            ("A320-", "A320", "A20N"),
            ("A321-", "A321", "A21N"),
        ] {
            if m.starts_with(fam) {
                return Some(if neo { n } else { ceo });
            }
        }
        if m.starts_with("A318-") {
            return Some("A318");
        }
        let digit = |p: &str| after(p).and_then(|r| r.chars().next());
        if let Some(d) = digit("A330-") {
            return match d {
                '2' => Some("A332"),
                '3' => Some("A333"),
                '8' => Some("A338"),
                '9' => Some("A339"),
                _ => None,
            };
        }
        if let Some(d) = digit("A340-") {
            return match d {
                '2' => Some("A342"),
                '3' => Some("A343"),
                '5' => Some("A345"),
                '6' => Some("A346"),
                _ => None,
            };
        }
        if let Some(r) = after("A350-") {
            return if r.starts_with("10") {
                Some("A35K")
            } else if r.starts_with('9') {
                Some("A359")
            } else {
                None
            };
        }
        if m.starts_with("A380-") {
            return Some("A388");
        }
        return None;
    }
    if maker.contains("BOMBARDIER") || maker.contains("CANADAIR") {
        let c = m.replace("CL600", "CL-600").replace("CL-6002", "CL-600-2");
        return match c.get(..11) {
            Some("CL-600-2B19") => Some("CRJ2"),
            Some("CL-600-2C10") => Some("CRJ7"),
            Some("CL-600-2D15") | Some("CL-600-2D24") => Some("CRJ9"),
            Some("CL-600-2E25") => Some("CRJX"),
            _ if m.starts_with("DHC-8-4") => Some("DH8D"),
            _ if m.starts_with("DHC-8-3") => Some("DH8C"),
            _ if m.starts_with("DHC-8-2") => Some("DH8B"),
            _ if m.starts_with("DHC-8-1") => Some("DH8A"),
            _ => None,
        };
    }
    if maker.contains("HAVILLAND") || maker.contains("DHC") {
        return match m.get(..7) {
            Some("DHC-8-4") => Some("DH8D"),
            Some("DHC-8-3") => Some("DH8C"),
            Some("DHC-8-2") => Some("DH8B"),
            Some("DHC-8-1") => Some("DH8A"),
            _ => None,
        };
    }
    if maker.contains("EMBRAER") {
        let e = m.replace("ERJ", "").replace("EMB-", "").replace('-', "");
        return match e.get(..6) {
            Some("170100") => Some("E170"),
            Some("170200") => Some("E75L"),
            Some("190100") => Some("E190"),
            Some("190200") => Some("E195"),
            Some("190300") => Some("E290"),
            Some("190400") => Some("E295"),
            _ => None,
        };
    }
    if maker.contains("MCDONNELL") && m.starts_with("MD-11") {
        return Some("MD11");
    }
    None
}

/// The type as Keepsake names it, from a register's maker and model:
/// `BOEING` `737-990ER` → `Boeing 737-900ER`, `Bombardier` `CL-600-2D24
/// (Series 900)` → `Bombardier CRJ900`, `AIRBUS` `A320-251N` → `Airbus
/// A320neo`. Others: the maker and the model (`Cessna 172S`).
pub fn type_name(maker: &str, model: &str) -> Option<String> {
    let model = model.trim();
    if model.is_empty() {
        return None;
    }
    if let Some(name) = designator(maker, model).and_then(lookup::icao_type_name) {
        let up = model.to_ascii_uppercase();
        // The ER/F the designator's name leaves out.
        if name == "Boeing 737-900" && up.ends_with("ER") {
            return Some("Boeing 737-900ER".into());
        }
        if up.starts_with("777F") || (up.starts_with("777-") && up.ends_with('F')) {
            return Some("Boeing 777F".into());
        }
        return Some(name.to_owned());
    }
    let m = model.to_ascii_uppercase();
    if m == "777F" {
        return Some("Boeing 777F".into());
    }
    // Embraer's regional jets: `EMB-145XR` → `Embraer ERJ-145XR`.
    for fam in ["135", "140", "145"] {
        if let Some(rest) = m.strip_prefix(&format!("EMB-{fam}")) {
            return Some(format!("Embraer ERJ-{fam}{}", rest.trim()));
        }
    }
    // `DC-9-83(MD-83)` → `MD-83`.
    let shown = match (m.find('('), m.find(')')) {
        (Some(a), Some(b)) if b > a && m[a + 1..b].starts_with("MD-") => model[a + 1..b].trim(),
        _ => model,
    };
    Some(format!("{} {}", maker_name(maker), shown).trim().to_owned())
}

/// The airline a company owner is, as the airline list names it.
pub fn airline_of_owner(owner: &str, airlines: &Airlines) -> Option<String> {
    airlines
        .owner(owner)
        .or_else(|| airlines.owner(&owner.to_ascii_uppercase()))
        .map(str::to_owned)
}

/// Transport Canada's register: `carscurr.txt` (an aircraft a line, 47
/// fields) and `carsownr.txt` (its owners, 20 fields).
pub fn parse_tc(current: impl Read, owners: impl Read) -> Result<Vec<RegisterRow>, AiError> {
    const MARK: usize = 46;
    let airlines = Airlines::builtin();
    // Mark → the first owner that is a company (never a person).
    let mut company: HashMap<String, String> = HashMap::new();
    for l in lines(owners, true) {
        let f = csv_fields(&l);
        if f.len() < 20 || f[11] != "Entity" {
            continue;
        }
        let mark = f[19].to_ascii_uppercase();
        if let Some(name) = some(&f[1]) {
            company.entry(mark).or_insert(name);
        }
    }
    let mut out = Vec::new();
    for l in lines(current, true) {
        let f = csv_fields(&l);
        if f.len() < 47 || f[MARK].is_empty() {
            continue;
        }
        let mark = f[MARK].to_ascii_uppercase();
        let owner = company.get(&mark).cloned();
        out.push(RegisterRow {
            registration: format!("C-{mark}"),
            source: Register::Tc.id().to_owned(),
            maker: some(&f[3]),
            model: some(&f[4]),
            aircraft_type: type_name(&f[3], &f[4]),
            serial: some(&f[5]),
            built: super::built::normalize(&f[31].replace('/', "-"), true),
            mode_s: hex_code(&f[42]),
            airline: owner
                .as_deref()
                .and_then(|o| airline_of_owner(o, &airlines)),
            owner,
            current: f[38] == "Registered",
        });
    }
    Ok(out)
}

/// The FAA's register: `MASTER.txt` (an aircraft a line) and
/// `ACFTREF.txt` (its maker and model by code). Owners that are people
/// (registrant types 1, 2, 4, 9) are not kept.
pub fn parse_faa(master: impl Read, acftref: impl Read) -> Result<Vec<RegisterRow>, AiError> {
    let airlines = Airlines::builtin();
    let mut refs: HashMap<String, (String, String)> = HashMap::new();
    let mut ref_lines = lines(acftref, false);
    ref_lines.next();
    for l in ref_lines {
        let f = csv_fields(&l);
        if f.len() > 2 {
            refs.insert(f[0].clone(), (f[1].clone(), f[2].clone()));
        }
    }
    let mut ml = lines(master, false);
    let head: Vec<String> = ml.next().map(|h| csv_fields(&h)).unwrap_or_default();
    let col = |name: &str| {
        head.iter()
            .position(|h| h == name)
            .ok_or_else(|| bad("FAA register", format!("no {name} column")))
    };
    let (n, serial, code, year, kind, name, status, hex) = (
        col("N-NUMBER")?,
        col("SERIAL NUMBER")?,
        col("MFR MDL CODE")?,
        col("YEAR MFR")?,
        col("TYPE REGISTRANT")?,
        col("NAME")?,
        col("STATUS CODE")?,
        col("MODE S CODE HEX")?,
    );
    let last = [n, serial, code, year, kind, name, status, hex]
        .into_iter()
        .max()
        .unwrap_or(0);
    let mut out = Vec::new();
    for l in ml {
        let f = csv_fields(&l);
        if f.len() <= last || f[n].is_empty() {
            continue;
        }
        let (maker, model) = refs.get(&f[code]).cloned().unwrap_or_default();
        let owner = matches!(f[kind].as_str(), "3" | "5" | "7" | "8")
            .then(|| some(&f[name]))
            .flatten();
        out.push(RegisterRow {
            registration: format!("N{}", f[n].to_ascii_uppercase()),
            source: Register::Faa.id().to_owned(),
            maker: some(&maker).map(|m| maker_name(&m)),
            model: some(&model),
            aircraft_type: type_name(&maker, &model),
            serial: some(&f[serial]),
            built: super::built::normalize(&f[year], false),
            mode_s: hex_code(&f[hex]),
            airline: owner
                .as_deref()
                .and_then(|o| airline_of_owner(o, &airlines)),
            owner,
            current: f[status] == "V",
        });
    }
    Ok(out)
}

/// Read a downloaded register file.
pub fn read_zip(which: Register, zip_path: &Path) -> Result<Vec<RegisterRow>, AiError> {
    let file = std::fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(file).map_err(|e| bad(which.name(), e))?;
    let names: Vec<String> = z
        .file_names()
        .filter_map(Result::ok)
        .map(|n| n.into_owned())
        .collect();
    let find = |want: &str| {
        names
            .iter()
            .find(|n| {
                Path::new(n)
                    .file_name()
                    .is_some_and(|f| f.to_string_lossy().eq_ignore_ascii_case(want))
            })
            .cloned()
            .ok_or_else(|| bad(which.name(), format!("no {want} in the file")))
    };
    let mut take = |want: &str| -> Result<Vec<u8>, AiError> {
        let name = find(want)?;
        let mut f = z.by_name(&name).map_err(|e| bad(which.name(), e))?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        Ok(buf)
    };
    match which {
        Register::Tc => {
            let (cur, own) = (take("carscurr.txt")?, take("carsownr.txt")?);
            parse_tc(cur.as_slice(), own.as_slice())
        }
        Register::Faa => {
            // The small reference file in memory; the master file (about
            // 200 MB) read as it is unpacked.
            let refs = take("ACFTREF.txt")?;
            let name = find("MASTER.txt")?;
            let master = z.by_name(&name).map_err(|e| bad(which.name(), e))?;
            parse_faa(master, refs.as_slice())
        }
    }
}

/// What **Update now** did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Updated {
    /// The file has not changed since the one imported.
    Unchanged,
    /// Imported: how many aircraft.
    Imported(u64),
}

/// Download `which` (if it changed since the one imported) and import it.
/// The downloaded file is deleted once read.
pub fn update(
    cat: &mut Catalog,
    which: Register,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Updated, AiError> {
    let since = sources(cat)?
        .into_iter()
        .find(|s| s.source == which.id())
        .and_then(|s| s.file_modified);
    let dir = crate::ai::install::user_models_dir()
        .ok_or_else(|| AiError::Download("no cache folder on this computer".into()))?
        .join("registers");
    let zip_path = dir.join(format!("{}.zip", which.id()));
    let got = crate::ai::install::download_if_newer(
        which.url(),
        &zip_path,
        since.as_deref(),
        which.bytes(),
        cancel,
        progress,
    )?;
    let Some(modified) = got else {
        return Ok(Updated::Unchanged);
    };
    let read = read_zip(which, &zip_path);
    let _ = std::fs::remove_file(&zip_path);
    let rows = read?;
    if rows.is_empty() {
        return Err(bad(which.name(), "it lists no aircraft"));
    }
    import(cat, which, &rows, Some(&modified))?;
    Ok(Updated::Imported(rows.len() as u64))
}

/// Put `rows` in place of `which`'s register.
pub fn import(
    cat: &mut Catalog,
    which: Register,
    rows: &[RegisterRow],
    file_modified: Option<&str>,
) -> Result<(), AiError> {
    let now = crate::jobs::now_ms();
    cat.write(|tx| {
        tx.ai().register_replace(
            which.id(),
            rows,
            file_modified.filter(|m| !m.is_empty()),
            now,
        )
    })?;
    Ok(())
}

/// Delete `which`'s register.
pub fn remove(cat: &mut Catalog, which: Register) -> Result<(), AiError> {
    cat.write(|tx| tx.ai().register_remove(which.id()))?;
    Ok(())
}

/// When each register was last downloaded.
pub fn sources(cat: &Catalog) -> Result<Vec<RegisterSource>, AiError> {
    Ok(cat.read(|tx| tx.ai().register_sources())?)
}

/// A registration as the registers keep it: upper case, `C-FJZS`, `N123AB`
/// (`CFJZS` and `N-123AB` too).
pub fn key(registration: &str) -> String {
    let r = registration.trim().to_ascii_uppercase();
    if let Some(rest) = r.strip_prefix("N-") {
        return format!("N{rest}");
    }
    if r.len() == 5
        && r.starts_with('C')
        && !r.contains('-')
        && r.as_bytes()[1].is_ascii_alphabetic()
    {
        return format!("C-{}", &r[1..]);
    }
    r
}

/// The register's entry for `registration`.
pub fn get(cat: &Catalog, registration: &str) -> Result<Option<RegisterRow>, AiError> {
    Register::of(&key(registration)).map_or(Ok(None), |_| {
        Ok(cat.read(|tx| tx.ai().register_get(&key(registration)))?)
    })
}

/// The registered aircraft of `which`, kept in memory until the register
/// is imported again (for [`near`], asked for every read shown).
fn current_marks(
    cat: &Catalog,
    which: Register,
    imported_ms: i64,
) -> Result<std::sync::Arc<std::collections::HashSet<String>>, AiError> {
    type Cached = HashMap<Register, (i64, std::sync::Arc<std::collections::HashSet<String>>)>;
    static CACHE: std::sync::Mutex<Option<Cached>> = std::sync::Mutex::new(None);
    if let Ok(c) = CACHE.lock() {
        if let Some((at, set)) = c.as_ref().and_then(|m| m.get(&which)) {
            if *at == imported_ms {
                return Ok(set.clone());
            }
        }
    }
    let set: std::collections::HashSet<String> = cat
        .read(|tx| tx.ai().register_current(which.id()))?
        .into_iter()
        .collect();
    let set = std::sync::Arc::new(set);
    if let Ok(mut c) = CACHE.lock() {
        c.get_or_insert_with(HashMap::new)
            .insert(which, (imported_ms, set.clone()));
    }
    Ok(set)
}

/// Characters text recognition takes for one another (`2` for `Z`).
fn look_alikes(c: u8) -> &'static [u8] {
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
        b'V' => b"U",
        _ => b"",
    }
}

/// A registration read that no register lists, but that one registered
/// aircraft matches with one character text recognition confuses (`C-FJ2S`
/// for `C-FJZS`): that one. `None` when it is listed, when its register is
/// not downloaded, or when several are as near.
pub fn near(cat: &Catalog, registration: &str) -> Result<Option<String>, AiError> {
    let r = key(registration);
    let Some(which) = Register::of(&r) else {
        return Ok(None);
    };
    let Some(src) = sources(cat)?.into_iter().find(|s| s.source == which.id()) else {
        return Ok(None);
    };
    if !r.is_ascii() || get(cat, &r)?.is_some() {
        return Ok(None);
    }
    let marks = current_marks(cat, which, src.imported_ms)?;
    // `C-` and `N` stay.
    let fixed = if which == Register::Tc { 2 } else { 1 };
    let mut found: Vec<String> = Vec::new();
    let mut b = r.clone().into_bytes();
    for i in fixed..b.len() {
        let was = b[i];
        for &c in look_alikes(was) {
            b[i] = c;
            if let Ok(s) = std::str::from_utf8(&b) {
                if marks.contains(s) && !found.iter().any(|f| f == s) {
                    found.push(s.to_owned());
                }
            }
        }
        b[i] = was;
        if found.len() > 1 {
            return Ok(None);
        }
    }
    Ok((found.len() == 1).then(|| found.remove(0)))
}

/// What the lookup knows of an aircraft, with what its national register
/// says filling the gaps: type, airline, build date, transponder code.
/// `None` when neither knows it.
pub fn with_register(
    cat: &Catalog,
    registration: &str,
    f: Option<Found>,
) -> Result<Option<Found>, AiError> {
    let Some(row) = get(cat, registration)? else {
        return Ok(f);
    };
    let mut out = f.unwrap_or_else(|| Found {
        registration: registration.to_owned(),
        version: lookup::LOOKUP_VERSION,
        ..Found::default()
    });
    out.known = true;
    if out.aircraft_type.is_none() {
        out.aircraft_type = row.aircraft_type;
    }
    if out.airline.is_none() {
        out.airline = row.airline;
    }
    if out.manufacturer.is_none() {
        out.manufacturer = row.maker;
    }
    if out.owner.is_none() {
        out.owner = row.owner;
    }
    if out.built.is_none() {
        out.built = row.built;
    }
    if out.mode_s.is_none() {
        out.mode_s = row.mode_s;
    }
    Ok(Some(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_models_get_the_names_keepsake_gives_types() {
        let t = |maker: &str, model: &str| type_name(maker, model).unwrap();
        assert_eq!(t("Boeing", "777-333ER"), "Boeing 777-300ER");
        assert_eq!(t("Boeing", "777-233LR"), "Boeing 777-200LR");
        assert_eq!(t("BOEING", "737-990ER"), "Boeing 737-900ER");
        assert_eq!(t("Boeing", "737-8"), "Boeing 737 MAX 8");
        assert_eq!(t("Boeing", "737-8CT"), "Boeing 737-800");
        assert_eq!(t("Boeing", "737-7CT"), "Boeing 737-700");
        assert_eq!(t("BOEING", "737-9"), "Boeing 737 MAX 9");
        assert_eq!(t("Boeing", "787-9"), "Boeing 787-9");
        assert_eq!(t("BOEING", "757-24APF"), "Boeing 757-200");
        assert_eq!(t("BOEING", "767-300F"), "Boeing 767-300");
        assert_eq!(t("BOEING", "777F"), "Boeing 777F");
        assert_eq!(t("Airbus", "A321-271NX"), "Airbus A321neo");
        assert_eq!(t("AIRBUS", "A320-251N"), "Airbus A320neo");
        assert_eq!(t("Airbus", "A319-114"), "Airbus A319");
        assert_eq!(t("Airbus", "A330-343"), "Airbus A330-300");
        assert_eq!(t("AIRBUS CANADA LP", "BD-500-1A11"), "Airbus A220-300");
        assert_eq!(
            t("Bombardier", "CL-600-2D24 (Series 900)"),
            "Bombardier CRJ900"
        );
        assert_eq!(
            t("Canadair", "CL-600-2B19 (Series 100)"),
            "Bombardier CRJ200"
        );
        assert_eq!(t("BOMBARDIER INC", "CL-600-2C10"), "Bombardier CRJ700");
        assert_eq!(t("Bombardier", "DHC-8-402"), "De Havilland Dash 8-400");
        assert_eq!(t("Embraer", "ERJ 190-400"), "Embraer E195-E2");
        assert_eq!(t("EMBRAER S A", "ERJ 170-200 LR"), "Embraer E175");
        assert_eq!(t("MCDONNELL DOUGLAS", "MD-11F"), "McDonnell Douglas MD-11");
        assert_eq!(t("EMBRAER", "EMB-145XR"), "Embraer ERJ-145XR");
        assert_eq!(
            t("MCDONNELL DOUGLAS", "DC-9-83(MD-83)"),
            "McDonnell Douglas MD-83"
        );
        // Not an airliner Keepsake knows: the maker and the model.
        assert_eq!(t("CESSNA", "172S"), "Cessna 172S");
        assert_eq!(t("BOMBARDIER INC", "BD-100-1A10"), "Bombardier BD-100-1A10");
        assert_eq!(type_name("Boeing", " "), None);
    }

    #[test]
    fn registrations_are_read_as_the_registers_keep_them() {
        assert_eq!(key(" c-fjzs"), "C-FJZS");
        assert_eq!(key("CFJZS"), "C-FJZS");
        assert_eq!(key("n492as"), "N492AS");
        assert_eq!(key("N-492AS"), "N492AS");
        assert_eq!(Register::of("C-FJZS"), Some(Register::Tc));
        assert_eq!(Register::of("N492AS"), Some(Register::Faa));
        assert_eq!(Register::of("PH-BKO"), None);
        assert_eq!(Register::of("NA-X"), None);
        assert_eq!(
            hex_code("110000000001101001100001").as_deref(),
            Some("C01A61")
        );
        assert_eq!(hex_code("a61797").as_deref(), Some("A61797"));
        assert_eq!(hex_code("50002263"), None);
        assert_eq!(
            csv_fields(r#""C-FJZS","Air ""Canada"", Inc", 2016/01/01 ,,"x""#),
            ["C-FJZS", "Air \"Canada\", Inc", "2016/01/01", "", "x"]
        );
    }

    /// A line of Transport Canada's aircraft file (47 fields).
    fn tc_line(
        mark: &str,
        common: &str,
        model: &str,
        built: &str,
        modes: &str,
        status: &str,
    ) -> String {
        let mut f = vec![String::new(); 47];
        f[0] = format!(" {mark}");
        f[3] = common.into();
        f[4] = model.into();
        f[5] = "62400".into();
        f[31] = built.into();
        f[38] = status.into();
        f[42] = modes.into();
        f[46] = mark.into();
        f.iter()
            .map(|x| format!("\"{x}\""))
            .collect::<Vec<_>>()
            .join(",")
    }

    fn tc_owner(mark: &str, name: &str, kind: &str) -> String {
        let mut f = vec![String::new(); 20];
        f[0] = mark.into();
        f[1] = name.into();
        f[3] = "1 Private Road".into();
        f[11] = kind.into();
        f[19] = mark.into();
        f.iter()
            .map(|x| format!("\"{x}\""))
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn transport_canada_is_read_without_people_or_addresses() {
        let cur = [
            tc_line(
                "FJZS",
                "Boeing",
                "777-333ER",
                "2016/01/01",
                "110000000001101001100001",
                "Registered",
            ),
            tc_line(
                "GABC",
                "Cessna",
                "172S",
                "2001/06/15",
                "110000000000000000000001",
                "Registration Cancelled",
            ),
        ]
        .join("\r\n");
        let own = [
            tc_owner("FJZS", "Air Canada", "Entity"),
            tc_owner("GABC", "Jane Doe", "Individual"),
        ]
        .join("\r\n");
        let rows = parse_tc(cur.as_bytes(), own.as_bytes()).unwrap();
        assert_eq!(rows.len(), 2);
        let a = &rows[0];
        assert_eq!(a.registration, "C-FJZS");
        assert_eq!(a.aircraft_type.as_deref(), Some("Boeing 777-300ER"));
        // 1 January: the year alone.
        assert_eq!(a.built.as_deref(), Some("2016"));
        assert_eq!(a.mode_s.as_deref(), Some("C01A61"));
        assert_eq!(a.owner.as_deref(), Some("Air Canada"));
        assert_eq!(a.airline.as_deref(), Some("Air Canada"));
        assert!(a.current);
        let b = &rows[1];
        // A person: not kept.
        assert_eq!(b.owner, None);
        assert_eq!(b.airline, None);
        assert_eq!(b.built.as_deref(), Some("2001-06-15"));
        assert!(!b.current);
    }

    #[test]
    fn the_faa_register_is_read_without_people_or_addresses() {
        let refs = "CODE,MFR,MODEL,TYPE-ACFT,\r\n1384432,BOEING                        ,737-990ER           ,5,\r\n2072704,CESSNA                        ,172S                ,4,\r\n";
        let head = "N-NUMBER,SERIAL NUMBER,MFR MDL CODE,ENG MFR MDL,YEAR MFR,TYPE REGISTRANT,NAME,STREET,STATUS CODE,MODE S CODE HEX,\r\n";
        let master = format!(
            "\u{feff}{head}492AS,36352 ,1384432,54556,2015,3,ALASKA AIRLINES INC   ,PO BOX 68900 ,V,A61797    ,\r\n12345,1720001,2072704,41514,2001,1,DOE JOHN ,1 MAIN ST,V,A00001,\r\n"
        );
        let rows = parse_faa(master.as_bytes(), refs.as_bytes()).unwrap();
        assert_eq!(rows.len(), 2);
        let a = &rows[0];
        assert_eq!(a.registration, "N492AS");
        assert_eq!(a.maker.as_deref(), Some("Boeing"));
        assert_eq!(a.aircraft_type.as_deref(), Some("Boeing 737-900ER"));
        assert_eq!(a.built.as_deref(), Some("2015"));
        assert_eq!(a.mode_s.as_deref(), Some("A61797"));
        assert_eq!(a.owner.as_deref(), Some("ALASKA AIRLINES INC"));
        assert_eq!(a.airline.as_deref(), Some("Alaska Airlines"));
        // A person (registrant type 1): not kept.
        assert_eq!(rows[1].owner, None);
        assert_eq!(rows[1].aircraft_type.as_deref(), Some("Cessna 172S"));
    }
}

//! Looking a registration up online (ADR-041, addition): what adsbdb.com
//! knows about the aircraft (its type, its registered owner and country),
//! to offer in the review window beside what was read.
//!
//! - Only the registration's text is sent (`GET
//!   https://api.adsbdb.com/v0/aircraft/<REGISTRATION>`), never a picture.
//!   No key is needed. adsbdb publishes no rate limit, so the aircraft
//!   worker asks at most one a second ([`MIN_GAP`]).
//! - Every answer is kept in the catalog (`aircraft_lookups`), found or
//!   not, so a registration is asked about once ([`lookup_cached`]). One
//!   not known is asked again after [`RETRY_UNKNOWN_MS`] (30 days), in the
//!   background ([`refresh_one`]): adsbdb learns of new aircraft.
//! - The type comes from the ICAO type designator (`B77W` → `Boeing
//!   777-300ER`, `icao-types.tsv`), named as a painted type would be;
//!   otherwise its manufacturer and model (`Boeing 777`). The registered
//!   owner counts as the airline only when it is one in `airlines.tsv` (an
//!   owner can be a leasing company).
//! - The answer keeps the transponder code (`mode_s`), and when the
//!   aircraft was built from OpenSky's database on this computer, if it is
//!   there ([`super::built`]; nothing more is sent). When adsbdb gives no
//!   transponder code, OpenSky's database gives it, by registration (for
//!   Add route). Answers kept before
//!   that ([`LOOKUP_VERSION`]) are asked again once, in the background, at
//!   the same pace ([`refresh_one`]).
//! - Off until the user turns it on (`ai.aircraft_lookup`).

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::built::{self, BuiltDb};
use super::marks::Airlines;
use crate::ai::cloud::Http;
use crate::ai::AiError;
use crate::catalog::Catalog;

/// The aircraft endpoint, up to the registration.
pub const API_BASE: &str = "https://api.adsbdb.com/v0/aircraft/";
/// Who answers, for people.
pub const SERVICE: &str = "adsbdb";
/// The least time between two requests.
pub const MIN_GAP: Duration = Duration::from_millis(1100);
/// A registration read at least this surely is looked up.
pub const LOOKUP_MIN: f32 = 0.6;
/// At most this many registrations of one photo are looked up.
pub const MAX_PER_PHOTO: usize = 3;
/// What a kept answer holds: 2 added the transponder code and the build
/// date. One kept before is asked again once.
pub const LOOKUP_VERSION: u32 = 2;
/// An aircraft adsbdb did not know is asked about again after this long.
pub const RETRY_UNKNOWN_MS: i64 = 30 * 24 * 3600 * 1000;

const ICAO_TSV: &str = include_str!("icao-types.tsv");

/// What the service knows about a registration.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Found {
    pub registration: String,
    /// The service knows the aircraft.
    pub known: bool,
    /// The type, as Keepsake names it (`Boeing 777-200LR`).
    pub aircraft_type: Option<String>,
    /// The registered owner when it is a known airline, as named in the
    /// airline list.
    pub airline: Option<String>,
    pub manufacturer: Option<String>,
    /// The registered owner as the service gives it.
    pub owner: Option<String>,
    /// The ICAO type designator (`B77L`).
    pub icao_type: Option<String>,
    pub country: Option<String>,
    /// The transponder (Mode S) address, hex (`C01732`).
    #[serde(default)]
    pub mode_s: Option<String>,
    /// When it was built (`2014` or `2014-03-05`), from OpenSky's database.
    #[serde(default)]
    pub built: Option<String>,
    /// What the answer holds ([`LOOKUP_VERSION`]); 0: kept before 2.
    #[serde(default)]
    pub version: u32,
    /// The transponder code the user typed (Add route), hex: used before
    /// [`Found::mode_s`], and kept when the registration is asked again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub typed_mode_s: Option<String>,
    /// What Flightradar24 said about the aircraft (Settings → Look up
    /// aircraft on Flightradar24); kept 30 days ([`super::fr24`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fr24: Option<Fr24Aircraft>,
}

/// An aircraft as one of its recent flights on Flightradar24 shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fr24Aircraft {
    /// When Flightradar24 was asked (Unix ms): deleted 30 days on.
    pub asked_ms: i64,
    /// A flight was found in the weeks asked about.
    pub found: bool,
    /// The ICAO type designator (`B78X`).
    #[serde(default)]
    pub icao_type: Option<String>,
    /// The ICAO code of the airline operating it (`KLM`), and of its livery.
    #[serde(default)]
    pub operator: Option<String>,
    #[serde(default)]
    pub painted_as: Option<String>,
    /// The transponder code, hex (`485E31`).
    #[serde(default)]
    pub hex: Option<String>,
    /// The day of the flight seen (`2026-11-17`).
    #[serde(default)]
    pub seen: Option<String>,
    /// Asked more than 30 days ago: what it said is deleted (its terms);
    /// only when it was asked is kept, so it is not asked again unasked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub expired: bool,
}

impl Fr24Aircraft {
    /// Still within the 30 days Flightradar24's terms allow it to be kept.
    pub fn fresh(&self, now_ms: i64) -> bool {
        !self.expired && now_ms - self.asked_ms < super::route::FR24_KEEP_MS
    }
}

impl Found {
    /// Kept before the transponder code and build date were: ask again.
    pub fn stale(&self) -> bool {
        self.version < LOOKUP_VERSION
    }

    /// To be asked again now (`now_ms`), as kept at `looked_ms`: kept
    /// before [`LOOKUP_VERSION`], or not known for [`RETRY_UNKNOWN_MS`].
    pub fn due(&self, looked_ms: i64, now_ms: i64) -> bool {
        self.stale() || (!self.known && now_ms.saturating_sub(looked_ms) >= RETRY_UNKNOWN_MS)
    }
}

/// Looks registrations up.
pub trait Lookup: Send + Sync {
    fn lookup(&self, registration: &str) -> Result<Found, AiError>;

    /// Where `callsign` usually flies (adsbdb's flight routes): its
    /// departure and arrival ICAO codes (`CYYZ`, `GMMN`). `None` when not
    /// known.
    fn callsign_route(&self, _callsign: &str) -> Result<Option<(String, String)>, AiError> {
        Ok(None)
    }
}

/// adsbdb's flight routes by callsign.
pub const CALLSIGN_BASE: &str = "https://api.adsbdb.com/v0/callsign/";

/// adsbdb's answer for a callsign: its origin and destination ICAO codes.
pub fn parse_callsign_route(status: u16, body: &str) -> Result<Option<(String, String)>, AiError> {
    match status {
        200 => {}
        404 => return Ok(None),
        429 => {
            return Err(AiError::Cloud {
                message: format!("{SERVICE} is busy: trying again later"),
                retry: true,
            })
        }
        s => {
            return Err(AiError::Cloud {
                message: format!("{SERVICE} answered {s}"),
                retry: s >= 500,
            })
        }
    }
    let v: Value = serde_json::from_str(body).map_err(|e| AiError::Cloud {
        message: format!("{SERVICE}'s answer could not be read: {e}"),
        retry: true,
    })?;
    let r = &v["response"]["flightroute"];
    let code = |end: &str| {
        r[end]["icao_code"]
            .as_str()
            .map(|c| c.trim().to_ascii_uppercase())
            .filter(|c| !c.is_empty())
    };
    Ok(code("origin").zip(code("destination")))
}

/// The type a designator names (`B77W` → `Boeing 777-300ER`).
pub fn icao_type_name(code: &str) -> Option<&'static str> {
    let code = code.trim();
    ICAO_TSV
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .find(|(c, _)| c.eq_ignore_ascii_case(code))
        .map(|(_, n)| n.trim())
}

/// The type to offer: by designator, else the manufacturer and the model
/// (the first word of the service's type: `777 233LR` → `Boeing 777`).
pub fn type_name(
    icao: Option<&str>,
    manufacturer: Option<&str>,
    raw: Option<&str>,
) -> Option<String> {
    if let Some(n) = icao.and_then(icao_type_name) {
        return Some(n.to_owned());
    }
    let raw = raw?.trim();
    let maker = manufacturer.map(str::trim).filter(|m| !m.is_empty());
    // The model is the first word after the manufacturer's name, if the
    // service repeats it (`Boeing 787` as well as `787 8`).
    let rest = match maker {
        Some(m) if raw.to_lowercase().starts_with(&m.to_lowercase()) => &raw[m.len()..],
        _ => raw,
    };
    let model = rest.split_whitespace().next()?;
    Some(match maker {
        Some(m) => format!("{m} {model}"),
        None => model.to_owned(),
    })
}

/// Read the service's answer for `registration`.
pub fn parse(
    registration: &str,
    status: u16,
    body: &str,
    airlines: &Airlines,
) -> Result<Found, AiError> {
    let unknown = Found {
        registration: registration.to_owned(),
        version: LOOKUP_VERSION,
        ..Found::default()
    };
    match status {
        200 => {}
        404 => return Ok(unknown),
        429 => {
            return Err(AiError::Cloud {
                message: format!("{SERVICE} is busy: trying again later"),
                retry: true,
            })
        }
        s => {
            return Err(AiError::Cloud {
                message: format!("{SERVICE} answered {s}"),
                retry: s >= 500,
            })
        }
    }
    let v: Value = serde_json::from_str(body).map_err(|e| AiError::Cloud {
        message: format!("{SERVICE}'s answer could not be read: {e}"),
        retry: true,
    })?;
    let a = &v["response"]["aircraft"];
    if !a.is_object() {
        return Ok(unknown);
    }
    let text = |k: &str| {
        a[k].as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let owner = text("registered_owner");
    Ok(Found {
        registration: registration.to_owned(),
        known: true,
        aircraft_type: type_name(
            text("icao_type").as_deref(),
            text("manufacturer").as_deref(),
            text("type").as_deref(),
        ),
        airline: owner
            .as_deref()
            .and_then(|o| airlines.owner(o).or_else(|| airlines.find(o)))
            .map(str::to_owned),
        manufacturer: text("manufacturer"),
        owner,
        icao_type: text("icao_type"),
        country: text("registered_owner_country_name"),
        mode_s: text("mode_s").map(|m| m.to_ascii_uppercase()),
        built: None,
        typed_mode_s: None,
        fr24: None,
        version: LOOKUP_VERSION,
    })
}

/// [`Lookup`] with adsbdb.com.
pub struct AdsbDb {
    http: Arc<dyn Http>,
    airlines: Airlines,
    built: Option<Arc<BuiltDb>>,
}

impl AdsbDb {
    pub fn new(http: Arc<dyn Http>) -> Self {
        Self {
            http,
            airlines: Airlines::builtin(),
            built: None,
        }
    }

    /// Also say when the aircraft was built, from OpenSky's database on
    /// this computer.
    pub fn with_built(mut self, db: Option<Arc<BuiltDb>>) -> Self {
        self.built = db;
        self
    }
}

impl Lookup for AdsbDb {
    fn callsign_route(&self, callsign: &str) -> Result<Option<(String, String)>, AiError> {
        // Callsigns are letters and digits only.
        let cs: String = callsign
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_uppercase();
        if cs.is_empty() {
            return Ok(None);
        }
        let (status, body) = self
            .http
            .get(
                &format!("{CALLSIGN_BASE}{cs}"),
                &[("accept", "application/json")],
            )
            .map_err(|e| AiError::Cloud {
                message: format!("could not reach {SERVICE}: {e}"),
                retry: true,
            })?;
        parse_callsign_route(status, &body)
    }

    fn lookup(&self, registration: &str) -> Result<Found, AiError> {
        // Registrations are letters, digits and hyphens only.
        let reg: String = registration
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        if reg.is_empty() {
            return Ok(Found::default());
        }
        let (status, body) = self
            .http
            .get(
                &format!("{API_BASE}{reg}"),
                &[("accept", "application/json")],
            )
            .map_err(|e| AiError::Cloud {
                message: format!("could not reach {SERVICE}: {e}"),
                retry: true,
            })?;
        let mut found = parse(registration, status, &body, &self.airlines)?;
        if let Some(db) = &self.built {
            if found.mode_s.is_none() {
                found.mode_s = db.code(registration);
            }
            found.built = db
                .find(found.mode_s.as_deref(), registration)
                .map(str::to_owned);
        }
        Ok(found)
    }
}

/// What is known about `registration`: kept from before, else asked now
/// (and kept). `Ok(None)`: not asked, as `ask` is false and nothing is kept.
pub fn lookup_cached(
    cat: &mut Catalog,
    lookup: Option<&dyn Lookup>,
    registration: &str,
) -> Result<Option<Found>, AiError> {
    let before = kept_at(cat, registration)?;
    let Some(lookup) = lookup else {
        return Ok(before.map(|(f, _)| f));
    };
    let now = crate::jobs::now_ms();
    if let Some((f, _)) = before.filter(|(f, at)| !f.due(*at, now)) {
        return Ok(Some(f));
    }
    ask(cat, lookup, registration).map(Some)
}

/// Ask about `registration` now, and keep the answer (and when the aircraft
/// was built, [`built::note_found`]).
fn ask(cat: &mut Catalog, lookup: &dyn Lookup, registration: &str) -> Result<Found, AiError> {
    let mut found = lookup.lookup(registration)?;
    // A code the user typed, and Flightradar24's details, stay.
    if let Some(before) = kept(cat, registration)? {
        found.typed_mode_s = before.typed_mode_s;
        found.fr24 = before.fr24;
    }
    let json = serde_json::to_string(&found)
        .map_err(|e| AiError::Image(format!("the lookup could not be kept: {e}")))?;
    let now = crate::jobs::now_ms();
    cat.write(|tx| tx.ai().put_aircraft_lookup(registration, &json, now))?;
    built::note_found(cat, &found)?;
    Ok(found)
}

/// Ask again about one registration whose answer is due ([`Found::due`]:
/// kept before [`LOOKUP_VERSION`], or not known for 30 days; the longest
/// ago first). Returns it, or `None` when none is due. The aircraft worker
/// calls this when it has nothing to read, at most one each [`MIN_GAP`].
pub fn refresh_one(cat: &mut Catalog, lookup: &dyn Lookup) -> Result<Option<String>, AiError> {
    let rows = cat.read(|tx| tx.ai().aircraft_lookups())?;
    let now = crate::jobs::now_ms();
    let stale = rows.into_iter().find(|(_, json, at)| {
        serde_json::from_str::<Found>(json)
            .map(|f| f.due(*at, now))
            .unwrap_or(true)
    });
    let Some((reg, _, _)) = stale else {
        return Ok(None);
    };
    ask(cat, lookup, &reg)?;
    Ok(Some(reg))
}

/// Keep the transponder code the user typed for `registration` (six hex
/// digits; `None`: forget it). Kept with what the lookup said, or as a
/// registration not known when nothing was asked yet.
pub fn set_typed_code(
    cat: &mut Catalog,
    registration: &str,
    code: Option<&str>,
) -> Result<(), AiError> {
    let code = match code.map(str::trim).filter(|c| !c.is_empty()) {
        Some(c) => {
            let hex = c.trim_start_matches("0x").trim_start_matches("0X");
            if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(AiError::Cloud {
                    message: format!(
                        "{c} is not a transponder code: it is six digits and letters A–F (484F6E)"
                    ),
                    retry: false,
                });
            }
            Some(hex.to_ascii_uppercase())
        }
        None => None,
    };
    let now = crate::jobs::now_ms();
    let (mut found, at) = kept_at(cat, registration)?.unwrap_or_else(|| {
        (
            Found {
                registration: registration.to_owned(),
                version: LOOKUP_VERSION,
                ..Found::default()
            },
            now,
        )
    });
    found.typed_mode_s = code;
    let json = serde_json::to_string(&found)
        .map_err(|e| AiError::Image(format!("the lookup could not be kept: {e}")))?;
    cat.write(|tx| tx.ai().put_aircraft_lookup(registration, &json, at))?;
    Ok(())
}

/// What is kept about `registration`, without asking.
pub fn kept(cat: &Catalog, registration: &str) -> Result<Option<Found>, AiError> {
    Ok(kept_at(cat, registration)?.map(|(f, _)| f))
}

/// What is kept about `registration`, and when it was asked (Unix ms).
pub fn kept_at(cat: &Catalog, registration: &str) -> Result<Option<(Found, i64)>, AiError> {
    Ok(cat
        .read(|tx| tx.ai().aircraft_lookup(registration))?
        .and_then(|(j, at)| serde_json::from_str(&j).ok().map(|f| (f, at))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_callsign_route_is_read_and_an_unknown_callsign_is_none() {
        let body = r#"{"response":{"flightroute":{"callsign":"RAM211","origin":{"iata_code":"YYZ","icao_code":"CYYZ"},"destination":{"iata_code":"CMN","icao_code":"gmmn"}}}}"#;
        assert_eq!(
            parse_callsign_route(200, body).unwrap(),
            Some(("CYYZ".into(), "GMMN".into()))
        );
        assert_eq!(
            parse_callsign_route(404, r#"{"response":"unknown callsign"}"#).unwrap(),
            None
        );
        let half = r#"{"response":{"flightroute":{"origin":{"icao_code":"CYYZ"}}}}"#;
        assert_eq!(parse_callsign_route(200, half).unwrap(), None);
        assert!(matches!(
            parse_callsign_route(429, ""),
            Err(AiError::Cloud { retry: true, .. })
        ));
        assert!(parse_callsign_route(200, "not json").is_err());
    }

    const C_FIUJ: &str = r#"{"response":{"aircraft":{"type":"777 233LR","icao_type":"B77L","manufacturer":"Boeing","mode_s":"C01732","registration":"C-FIUJ","registered_owner_country_iso_name":"CA","registered_owner_country_name":"Canada","registered_owner_operator_flag_code":"ACA","registered_owner":"Air Canada","url_photo":"https://image.airport-data.com/aircraft/001497074.jpg","url_photo_thumbnail":"https://airport-data.com/images/aircraft/thumbnails/001/497/001497074.jpg"}}}"#;

    #[test]
    fn a_known_aircraft_gives_its_type_airline_and_country() {
        let f = parse("C-FIUJ", 200, C_FIUJ, &Airlines::builtin()).unwrap();
        assert!(f.known);
        assert_eq!(f.aircraft_type.as_deref(), Some("Boeing 777-200LR"));
        assert_eq!(f.airline.as_deref(), Some("Air Canada"));
        assert_eq!(f.owner.as_deref(), Some("Air Canada"));
        assert_eq!(f.country.as_deref(), Some("Canada"));
        assert_eq!(f.icao_type.as_deref(), Some("B77L"));
        // adsbdb has no build date; the transponder code is kept.
        assert_eq!(f.mode_s.as_deref(), Some("C01732"));
        assert_eq!(f.built, None);
        assert!(!f.stale());
    }

    #[test]
    fn an_answer_kept_before_version_2_is_stale() {
        let old = r#"{"registration":"C-FIUJ","known":true,"aircraft_type":"Boeing 777-200LR","airline":"Air Canada","manufacturer":"Boeing","owner":"Air Canada","icao_type":"B77L","country":"Canada"}"#;
        let f: Found = serde_json::from_str(old).unwrap();
        assert!(f.stale());
        assert_eq!(f.mode_s, None);
        let a = Airlines::builtin();
        assert!(!parse("VT-JEX", 404, "", &a).unwrap().stale());
    }

    #[test]
    fn the_build_date_comes_from_the_database_on_this_computer() {
        struct One;
        impl Http for One {
            fn post(&self, _: &str, _: &[(&str, &str)], _: &[u8]) -> Result<(u16, String), String> {
                Err("no".into())
            }
            fn get(&self, _: &str, _: &[(&str, &str)]) -> Result<(u16, String), String> {
                Ok((200, C_FIUJ.to_owned()))
            }
        }
        let bare = AdsbDb::new(Arc::new(One));
        assert_eq!(bare.lookup("C-FIUJ").unwrap().built, None);
        let db = BuiltDb::from_tsv("c01732\tC-FIUJ\t2008-03-05\n");
        let with = AdsbDb::new(Arc::new(One)).with_built(Some(Arc::new(db)));
        assert_eq!(
            with.lookup("C-FIUJ").unwrap().built.as_deref(),
            Some("2008-03-05")
        );
        // By registration, a year alone.
        let db = BuiltDb::from_tsv("\tC-FIUJ\t2008\n");
        let with = AdsbDb::new(Arc::new(One)).with_built(Some(Arc::new(db)));
        assert_eq!(
            with.lookup("C-FIUJ").unwrap().built.as_deref(),
            Some("2008")
        );
    }

    #[test]
    fn an_unknown_one_is_kept_as_unknown_and_trouble_is_said() {
        let a = Airlines::builtin();
        let f = parse("VT-JEX", 404, r#"{"response":"unknown aircraft"}"#, &a).unwrap();
        assert!(!f.known);
        assert_eq!(f.registration, "VT-JEX");
        assert!(matches!(
            parse("X", 429, "", &a),
            Err(AiError::Cloud { retry: true, .. })
        ));
        assert!(matches!(
            parse("X", 503, "", &a),
            Err(AiError::Cloud { retry: true, .. })
        ));
        assert!(matches!(
            parse("X", 400, "", &a),
            Err(AiError::Cloud { retry: false, .. })
        ));
        assert!(matches!(
            parse("X", 200, "not json", &a),
            Err(AiError::Cloud { retry: true, .. })
        ));
    }

    #[test]
    fn a_leasing_owner_is_not_an_airline_and_unlisted_types_keep_their_model() {
        let body = r#"{"response":{"aircraft":{"type":"A320 214","icao_type":"ZZZZ","manufacturer":"Airbus","registered_owner":"Wells Fargo Trust Co"}}}"#;
        let f = parse("N123AB", 200, body, &Airlines::builtin()).unwrap();
        assert_eq!(f.airline, None);
        assert_eq!(f.aircraft_type.as_deref(), Some("Airbus A320"));
        assert_eq!(icao_type_name("b77w"), Some("Boeing 777-300ER"));
        assert_eq!(icao_type_name("DH8D"), Some("De Havilland Dash 8-400"));
        assert_eq!(
            type_name(None, Some("Boeing"), Some("Boeing 787")).as_deref(),
            Some("Boeing 787")
        );
        assert_eq!(type_name(None, None, None), None);
    }
}

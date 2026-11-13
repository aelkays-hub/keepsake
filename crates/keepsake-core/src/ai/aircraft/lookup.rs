//! Looking a registration up online (ADR-041, addition): what adsbdb.com
//! knows about the aircraft (its type, its registered owner and country),
//! to offer in the review window beside what was read.
//!
//! - Only the registration's text is sent (`GET
//!   https://api.adsbdb.com/v0/aircraft/<REGISTRATION>`), never a picture.
//!   No key is needed. adsbdb publishes no rate limit, so the aircraft
//!   worker asks at most one a second ([`MIN_GAP`]).
//! - Every answer is kept in the catalog (`aircraft_lookups`), found or
//!   not, so a registration is asked about once ([`lookup_cached`]).
//! - The type comes from the ICAO type designator (`B77W` → `Boeing
//!   777-300ER`, `icao-types.tsv`), named as a painted type would be;
//!   otherwise its manufacturer and model (`Boeing 777`). The registered
//!   owner counts as the airline only when it is one in `airlines.tsv` (an
//!   owner can be a leasing company).
//! - Off until the user turns it on (`ai.aircraft_lookup`).

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
}

/// Looks registrations up.
pub trait Lookup: Send + Sync {
    fn lookup(&self, registration: &str) -> Result<Found, AiError>;
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
    })
}

/// [`Lookup`] with adsbdb.com.
pub struct AdsbDb {
    http: Arc<dyn Http>,
    airlines: Airlines,
}

impl AdsbDb {
    pub fn new(http: Arc<dyn Http>) -> Self {
        Self {
            http,
            airlines: Airlines::builtin(),
        }
    }
}

impl Lookup for AdsbDb {
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
        parse(registration, status, &body, &self.airlines)
    }
}

/// What is known about `registration`: kept from before, else asked now
/// (and kept). `Ok(None)`: not asked, as `ask` is false and nothing is kept.
pub fn lookup_cached(
    cat: &mut Catalog,
    lookup: Option<&dyn Lookup>,
    registration: &str,
) -> Result<Option<Found>, AiError> {
    if let Some(json) = cat.read(|tx| tx.ai().aircraft_lookup(registration))? {
        if let Ok(f) = serde_json::from_str::<Found>(&json) {
            return Ok(Some(f));
        }
    }
    let Some(lookup) = lookup else {
        return Ok(None);
    };
    let found = lookup.lookup(registration)?;
    let json = serde_json::to_string(&found)
        .map_err(|e| AiError::Image(format!("the lookup could not be kept: {e}")))?;
    let now = crate::jobs::now_ms();
    cat.write(|tx| tx.ai().put_aircraft_lookup(registration, &json, now))?;
    Ok(Some(found))
}

/// What is kept about `registration`, without asking.
pub fn kept(cat: &Catalog, registration: &str) -> Result<Option<Found>, AiError> {
    Ok(cat
        .read(|tx| tx.ai().aircraft_lookup(registration))?
        .and_then(|j| serde_json::from_str(&j).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

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

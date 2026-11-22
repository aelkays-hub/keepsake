//! The flight an aircraft photo shows (ADR-044, part 2): right-click →
//! Add route. From the photo's registration (its transponder code, from the
//! lookup) and its capture time, OpenSky's flight history gives the flights
//! that aircraft flew that day; the one nearest the photo is offered as
//! `Aircraft|From|YYZ`, `Aircraft|To|LHR`, their countries and the flight.
//!
//! - Sent to opensky-network.org: the transponder code and a UTC day
//!   (`GET /api/flights/aircraft?icao24=…&begin=…&end=…`, under 24 hours:
//!   4 credits), with an OAuth2 token from the user's free API client,
//!   kept for its 30 minutes ([`OpenSky`]). No photo is sent.
//! - Every answer is kept per aircraft and UTC day (`aircraft_flights`), so
//!   the same aircraft and day is asked about once, and photos taken beside
//!   it use the same answer.
//! - One question per aircraft (ADR-049): a job asks, with its own photo's
//!   days, the days of every other queued photo of the same transponder
//!   code, one question per UTC day ([`MAX_WINDOW_DAYS`], [`windows`]): a
//!   day is 4 credits, where a two-day question (which OpenSky allows)
//!   would be 30, so many photos of a day share one question and a night
//!   across midnight is two.
//! - OpenSky's history goes back to about 2023 (checked on the owner's
//!   photos: September 2023 found, August 2022 and 2018 not), and only where
//!   its receivers are; an airport it did not see is "not known".
//! - Airports: `airports.tsv` (OurAirports, The Unlicense), the airports
//!   with an IATA code: ICAO → IATA and country.
//! - Nothing becomes a keyword until the user accepts it.
//! - Or from Flightradar24 ([`Flightradar`], the user's API subscription):
//!   asked by registration, so no transponder code is needed
//!   (`GET /api/flight-summary/light?registrations=…&flight_datetime_from=…`,
//!   a UTC day; 3 credits per flight returned, none for none). Its terms
//!   allow keeping its data 30 days: answers are kept per registration and
//!   day (`aircraft_flights_fr24`) and deleted after [`FR24_KEEP_MS`]
//!   ([`forget_expired`]). The keywords the user accepts are theirs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::lookup::{self, Lookup};
use super::scan::TAG_PARENT;
use crate::ai::cloud::Http;
use crate::ai::AiError;
use crate::catalog::{AssetId, Catalog, JobState, NewJob, Priority, TagPath, TagSource};

/// The job kind: find the flights of one photo's aircraft.
pub const JOB_AI_ROUTE: &str = "ai.route";
/// Who answers, for people.
pub const SERVICE: &str = "OpenSky";
pub const FR24_SERVICE: &str = "Flightradar24";
pub const FR24_API: &str = "https://fr24api.flightradar24.com/api/flight-summary/light";
/// Flightradar24's terms: its data is kept at most 30 days.
pub const FR24_KEEP_MS: i64 = 30 * 24 * 3600 * 1000;

/// Where Add route finds flights (Settings → Read aircraft).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    /// OpenSky's flight history, by transponder code (free).
    #[default]
    OpenSky,
    /// Flightradar24's API, by registration (the user's subscription).
    Flightradar,
}

impl Source {
    /// As kept in the settings (`opensky`, `flightradar`).
    pub fn id(self) -> &'static str {
        match self {
            Self::OpenSky => "opensky",
            Self::Flightradar => "flightradar",
        }
    }

    pub fn parse(s: &str) -> Self {
        if s == "flightradar" {
            Self::Flightradar
        } else {
            Self::OpenSky
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::OpenSky => SERVICE,
            Self::Flightradar => FR24_SERVICE,
        }
    }
}
/// How Add route chooses where to ask (Settings → Read aircraft → Flights
/// from, ADR-050).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// By the photo's date: Flightradar24 within the user's plan's history,
    /// OpenSky before it; the other when one cannot be asked.
    #[default]
    Auto,
    /// Only this one.
    Only(Source),
}

impl Mode {
    /// As kept in the settings (`auto`, `opensky`, `flightradar`).
    pub fn id(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Only(s) => s.id(),
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "opensky" => Self::Only(Source::OpenSky),
            "flightradar" => Self::Only(Source::Flightradar),
            _ => Self::Auto,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "OpenSky or Flightradar24",
            Self::Only(s) => s.name(),
        }
    }
}

/// How far back Flightradar24 is used until its answer says how far the
/// user's plan goes (days).
pub const FR24_DAYS: i64 = 30;

/// What Add route chooses a photo's source with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choose {
    pub mode: Mode,
    /// How many days back the user's Flightradar24 plan gives flights.
    pub fr24_days: i64,
    /// An OpenSky client and a Flightradar24 token are saved.
    pub opensky: bool,
    pub fr24: bool,
    /// Unix seconds.
    pub now_s: i64,
}

impl Choose {
    /// Whether Flightradar24's plan covers a photo taken at `taken_s`: every
    /// UTC day asked about for it ([`days_for`]) starts inside the plan's
    /// history, with a day to spare (it moves on while the job waits).
    pub fn fr24_covers(&self, taken_s: i64) -> bool {
        let first = days_for(taken_s).first().map_or(taken_s, |d| d.1);
        first >= self.now_s - self.fr24_days.max(0) * DAY_S + DAY_S
    }

    /// The source to ask about a photo taken at `taken_s`, or why none can
    /// be. Automatic: Flightradar24 while its plan covers the photo, OpenSky
    /// before; the other when one is not set up.
    pub fn source(&self, taken_s: i64) -> Result<Source, String> {
        let none = |s: Source| match s {
            Source::OpenSky => {
                "No OpenSky client ID and secret: add them in Settings → Read aircraft".to_owned()
            }
            Source::Flightradar => {
                "No Flightradar24 API token: add it in Settings → Read aircraft".to_owned()
            }
        };
        match self.mode {
            Mode::Only(s) => {
                let have = match s {
                    Source::OpenSky => self.opensky,
                    Source::Flightradar => self.fr24,
                };
                if have {
                    Ok(s)
                } else {
                    Err(none(s))
                }
            }
            Mode::Auto => {
                if self.fr24 && self.fr24_covers(taken_s) {
                    Ok(Source::Flightradar)
                } else if self.opensky {
                    Ok(Source::OpenSky)
                } else if self.fr24 {
                    Err(format!(
                        "this photo is older than your Flightradar24 plan's {} days: add an OpenSky client ID and secret in Settings → Read aircraft for older photos",
                        self.fr24_days
                    ))
                } else {
                    Err("Add an OpenSky client or a Flightradar24 token in Settings → Read aircraft".to_owned())
                }
            }
        }
    }
}

pub const API_BASE: &str = "https://opensky-network.org/api/flights/aircraft";
pub const TOKEN_URL: &str =
    "https://auth.opensky-network.org/auth/realms/opensky-network/protocol/openid-connect/token";
/// A flight is the photo's when it took off or landed this close to it.
pub const NEAR_S: i64 = 3 * 3600;
/// Two flights this close to the photo's time are equally close.
pub const TIE_S: i64 = 10 * 60;
/// One day, in seconds.
const DAY_S: i64 = 86_400;
/// A token is renewed this long before it runs out.
const TOKEN_MARGIN_MS: i64 = 60_000;
/// The keyword kinds a route gives.
pub const KINDS: [&str; 5] = ["From", "To", "From country", "To country", "Flight"];
/// What OpenSky says when the day's credits are used.
pub const LIMIT_REACHED: &str = "OpenSky's daily limit is reached; Add route goes on tomorrow";
/// How long to wait at OpenSky's limit when it does not say.
const LIMIT_WAIT_S: i64 = 3600;

/// At OpenSky's limit until `until_ms`: what the window says.
pub fn limit_reached(until_ms: i64) -> AiError {
    let at = chrono::DateTime::from_timestamp_millis(until_ms)
        .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_default();
    AiError::Later {
        message: format!(
            "OpenSky's limit is reached (it allows more at {at}); Add route goes on by itself then"
        ),
        until_ms,
    }
}

const AIRPORTS_TSV: &str = include_str!("airports.tsv");
const AIRLINE_CODES_TSV: &str = include_str!("airline-codes.tsv");

/// The flight number a callsign is sold as: `ACA856` → `AC856`, `DLH470`
/// → `LH470` (`airline-codes.tsv`). A callsign of another airline, or one
/// that is not the airline's code and a number (`BAW12AB`), is kept as it
/// is.
pub fn flight_number(callsign: &str) -> String {
    let cs = callsign.trim().to_ascii_uppercase();
    let (code, num) = cs.split_at(cs.len().min(3));
    let digits = num.trim_start_matches('0');
    let looks = code.len() == 3
        && code.bytes().all(|b| b.is_ascii_uppercase())
        && (1..=4).contains(&num.len())
        && num.bytes().all(|b| b.is_ascii_digit())
        && !digits.is_empty();
    if !looks {
        return cs;
    }
    AIRLINE_CODES_TSV
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .find(|(icao, _)| *icao == code)
        .map_or(cs.clone(), |(_, rest)| {
            format!("{}{digits}", rest.split('\t').next().unwrap_or("").trim())
        })
}

/// The airline of an ICAO airline code (`KLM` → `KLM`, `ACA` → `Air
/// Canada`), as `airlines.tsv` names it (`airline-codes.tsv`).
pub fn airline_of_code(icao: &str) -> Option<&'static str> {
    let code = icao.trim().to_ascii_uppercase();
    AIRLINE_CODES_TSV
        .lines()
        .filter(|l| !l.starts_with('#'))
        .find_map(|l| {
            let f: Vec<&'static str> = l.split('\t').collect();
            (f.len() >= 3 && f[0] == code).then(|| f[2].trim())
        })
        .filter(|n| !n.is_empty())
}

/// One flight, as OpenSky saw it (times in Unix seconds).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flight {
    pub first_seen: i64,
    pub last_seen: i64,
    /// ICAO airport codes (`CYYZ`), when OpenSky worked them out.
    pub departure: Option<String>,
    pub arrival: Option<String>,
    /// Trimmed (`ACA856`).
    pub callsign: Option<String>,
    /// The flight number, when the source gives it (Flightradar24: `AC856`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<String>,
    /// Not seen by the source, worked out ([`missing_leg`]): it landed at
    /// one airport and next took off from another, so it flew between them
    /// some time from `first_seen` (that landing) to `last_seen` (that
    /// take-off). Never kept.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inferred: bool,
    /// A worked-out flight of which one end only was seen
    /// ([`missing_end`]): `Arrival`, the flight in (it landed at `arrival`
    /// before the photo and was next seen taking off from there at
    /// `last_seen`); `Departure`, the flight out (it landed at `departure`
    /// at `first_seen`, and its take-off after the photo was not seen).
    /// `None`: between two airports ([`missing_leg`]). Never kept.
    #[serde(skip)]
    pub half: Option<At>,
    /// An end the source did not see was filled in from where the
    /// callsign usually flies ([`fill_ends`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_callsign: bool,
    /// The callsign's usual route was asked for ([`fill_ends`]): not again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub route_asked: bool,
}

impl Flight {
    /// The flight number to show: the source's, else worked out from the
    /// callsign ([`flight_number`]).
    pub fn shown_number(&self) -> Option<String> {
        self.number
            .clone()
            .filter(|n| !n.trim().is_empty())
            .or_else(|| self.callsign.as_deref().map(flight_number))
    }
}

/// Gives an aircraft's flights between two times (Unix seconds): by its
/// transponder code (OpenSky), or by its registration (Flightradar24).
pub trait Flights: Send + Sync {
    fn flights(&self, id: &str, begin: i64, end: i64) -> Result<Vec<Flight>, AiError>;

    /// Which source this is: what `id` is, and where answers are kept.
    fn source(&self) -> Source {
        Source::OpenSky
    }

    /// Since when (Unix seconds) the source last said it gives flights to
    /// this account (Flightradar24's plan), once: `None` when it has not
    /// said so since last asked.
    fn take_history_from(&self) -> Option<i64> {
        None
    }
}

/// When Flightradar24 says the user's plan starts, from its refusal
/// ("… Your plan provides data from '2026-10-20 13:51:38' onward."), in
/// Unix seconds.
pub fn plan_from(message: &str) -> Option<i64> {
    let at = message.split("data from '").nth(1)?.get(..19)?;
    chrono::NaiveDateTime::parse_from_str(at, "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|d| d.and_utc().timestamp())
}

/// The user's OpenSky API client: its id and secret.
pub type Credentials = Arc<dyn Fn() -> Option<(String, String)> + Send + Sync>;

/// [`Flights`] from OpenSky's REST API.
pub struct OpenSky {
    http: Arc<dyn Http>,
    credentials: Credentials,
    clock: Arc<dyn Fn() -> i64 + Send + Sync>,
    /// The token and when it runs out (Unix ms).
    token: Mutex<Option<(String, i64)>>,
    /// At OpenSky's limit until then (Unix ms): nothing is asked before.
    limited_until: Mutex<Option<i64>>,
}

fn cloud(message: impl Into<String>, retry: bool) -> AiError {
    AiError::Cloud {
        message: message.into(),
        retry,
    }
}

/// `application/x-www-form-urlencoded`.
fn form(pairs: &[(&str, &str)]) -> String {
    let enc = |s: &str| {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect::<String>()
    };
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

impl OpenSky {
    pub fn new(http: Arc<dyn Http>, credentials: Credentials) -> Self {
        Self {
            http,
            credentials,
            clock: Arc::new(crate::jobs::now_ms),
            token: Mutex::new(None),
            limited_until: Mutex::new(None),
        }
    }

    /// Tell the time with this (tests).
    pub fn with_clock(mut self, clock: Arc<dyn Fn() -> i64 + Send + Sync>) -> Self {
        self.clock = clock;
        self
    }

    /// A token: the one kept while it lasts, else a new one.
    fn token(&self) -> Result<String, AiError> {
        let now = (self.clock)();
        if let Ok(t) = self.token.lock() {
            if let Some((tok, until)) = t.as_ref() {
                if now + TOKEN_MARGIN_MS < *until {
                    return Ok(tok.clone());
                }
            }
        }
        let (id, secret) = (self.credentials)().ok_or_else(|| {
            cloud(
                "No OpenSky client ID and secret: add them in Settings → Read aircraft",
                false,
            )
        })?;
        let body = form(&[
            ("grant_type", "client_credentials"),
            ("client_id", &id),
            ("client_secret", &secret),
        ]);
        let (status, text) = self
            .http
            .post(
                TOKEN_URL,
                &[("content-type", "application/x-www-form-urlencoded")],
                body.as_bytes(),
            )
            .map_err(|e| cloud(format!("could not reach {SERVICE}: {e}"), true))?;
        match status {
            200 => {}
            400 | 401 | 403 => return Err(cloud(
                "OpenSky refused the client ID and secret: check them in Settings → Read aircraft",
                false,
            )),
            s => return Err(cloud(format!("{SERVICE} answered {s}"), s >= 500)),
        }
        let v: Value = serde_json::from_str(&text)
            .map_err(|e| cloud(format!("{SERVICE}'s token could not be read: {e}"), true))?;
        let tok = v["access_token"]
            .as_str()
            .filter(|t| !t.is_empty())
            .ok_or_else(|| cloud(format!("{SERVICE} gave no token"), true))?
            .to_owned();
        let secs = v["expires_in"].as_i64().unwrap_or(1800);
        if let Ok(mut t) = self.token.lock() {
            *t = Some((tok.clone(), now + secs * 1000));
        }
        Ok(tok)
    }

    fn forget_token(&self) {
        if let Ok(mut t) = self.token.lock() {
            *t = None;
        }
    }

    /// Whether the client ID and secret work (Settings → Test).
    pub fn test(&self) -> Result<(), AiError> {
        self.forget_token();
        self.token().map(|_| ())
    }
}

/// Read OpenSky's answer.
pub fn parse_flights(status: u16, body: &str) -> Result<Vec<Flight>, AiError> {
    match status {
        200 => {}
        // OpenSky says 404 for no flights.
        404 => return Ok(Vec::new()),
        403 => {
            return Err(cloud(
                "OpenSky does not give this account the flight history",
                false,
            ))
        }
        429 => return Err(cloud(LIMIT_REACHED, true)),
        s => return Err(cloud(format!("{SERVICE} answered {s}"), s >= 500)),
    }
    let v: Value = serde_json::from_str(body)
        .map_err(|e| cloud(format!("{SERVICE}'s answer could not be read: {e}"), true))?;
    let text = |f: &Value, k: &str| {
        f[k].as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    Ok(v.as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|f| {
            Some(Flight {
                first_seen: f["firstSeen"].as_i64()?,
                last_seen: f["lastSeen"].as_i64()?,
                departure: text(f, "estDepartureAirport").map(|s| s.to_ascii_uppercase()),
                arrival: text(f, "estArrivalAirport").map(|s| s.to_ascii_uppercase()),
                callsign: text(f, "callsign"),
                number: None,
                ..Flight::default()
            })
        })
        .collect())
}

impl Flights for OpenSky {
    fn flights(&self, icao24: &str, begin: i64, end: i64) -> Result<Vec<Flight>, AiError> {
        let icao: String = icao24
            .chars()
            .filter(char::is_ascii_hexdigit)
            .collect::<String>()
            .to_ascii_lowercase();
        if icao.len() != 6 {
            return Err(cloud(format!("{icao24} is not a transponder code"), false));
        }
        let url = format!("{API_BASE}?icao24={icao}&begin={begin}&end={end}");
        let now = (self.clock)();
        if let Some(until) = self.limited_until.lock().ok().and_then(|u| *u) {
            if now < until {
                return Err(limit_reached(until));
            }
        }
        for attempt in 0..2 {
            let tok = self.token()?;
            let auth = format!("Bearer {tok}");
            let (status, body, wait) = self
                .http
                .get_wait(
                    &url,
                    &[("authorization", &auth), ("accept", "application/json")],
                )
                .map_err(|e| cloud(format!("could not reach {SERVICE}: {e}"), true))?;
            if status == 429 {
                // Out of credits: until OpenSky says (about a day).
                let until = now + wait.unwrap_or(LIMIT_WAIT_S).max(60) * 1000;
                if let Ok(mut u) = self.limited_until.lock() {
                    *u = Some(until);
                }
                return Err(limit_reached(until));
            }
            if status == 401 && attempt == 0 {
                // The token ran out early: once more with a new one.
                self.forget_token();
                continue;
            }
            if status == 401 {
                return Err(cloud(
                    "OpenSky refused the client ID and secret: check them in Settings → Read aircraft",
                    false,
                ));
            }
            return parse_flights(status, &body);
        }
        Err(cloud(format!("{SERVICE} refused the token"), false))
    }
}

/// The user's Flightradar24 API token.
pub type Token = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// [`Flights`] from Flightradar24's API (Flight summary, light), by
/// registration.
pub struct Flightradar {
    http: Arc<dyn Http>,
    token: Token,
    /// Since when the user's plan gives flights, when it said so.
    plan_from: Mutex<Option<i64>>,
}

/// `2025-04-01T00:00:00` (UTC) of a Unix second.
fn fr24_time(s: i64) -> String {
    chrono::DateTime::from_timestamp(s, 0)
        .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// A Unix second of Flightradar24's `2025-04-01T20:56:18` (UTC).
fn fr24_parse_time(s: &str) -> Option<i64> {
    let s = s.trim().trim_end_matches('Z');
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .ok()
        .map(|d| d.and_utc().timestamp())
}

/// Flightradar24's answer: its `data` list, or why not.
fn fr24_data(status: u16, body: &str) -> Result<Vec<Value>, AiError> {
    // Its `details` say why (`message` is only "Validation failed").
    let said = || {
        serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|v| {
                v["details"]
                    .as_str()
                    .or(v["message"].as_str())
                    .or(v["error"].as_str())
                    .map(str::to_owned)
            })
            .unwrap_or_default()
    };
    match status {
        200 => {}
        404 => return Ok(Vec::new()),
        401 => {
            return Err(cloud(
                "Flightradar24 refused the API token: check it in Settings → Read aircraft",
                false,
            ))
        }
        402 => {
            return Err(cloud(
                "Flightradar24 says the subscription's credits are used up",
                false,
            ))
        }
        403 => {
            return Err(cloud(
                format!(
                    "Flightradar24 refused: check the API token and that the plan includes Flight summary {}",
                    said()
                )
                .trim_end()
                .to_owned(),
                false,
            ))
        }
        429 => {
            return Err(cloud(
                "Flightradar24 asks to slow down; Add route goes on later",
                true,
            ))
        }
        400 => {
            let why = said();
            // "…earlier than your subscription plan allows. Your plan
            // provides data from '2026-10-20 13:51:38' onward."
            if why.contains("earlier than your subscription plan allows") {
                let from = why
                    .split("data from '")
                    .nth(1)
                    .and_then(|r| r.get(..10))
                    .unwrap_or("a later date");
                return Err(cloud(
                    format!(
                        "Flightradar24: your API plan gives flights from {from} on; this photo is older (a higher plan goes further back)"
                    ),
                    false,
                ));
            }
            return Err(cloud(
                format!("Flightradar24 did not accept the question: {why}"),
                false,
            ));
        }
        s => return Err(cloud(format!("{FR24_SERVICE} answered {s}"), s >= 500)),
    }
    let v: Value = serde_json::from_str(body).map_err(|e| {
        cloud(
            format!("{FR24_SERVICE}'s answer could not be read: {e}"),
            true,
        )
    })?;
    Ok(v["data"].as_array().cloned().unwrap_or_default())
}

/// The first text of `keys` in `f`, trimmed.
fn fr24_text(f: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| {
        f[*k]
            .as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    })
}

/// Read Flightradar24's answer.
pub fn parse_fr24(status: u16, body: &str) -> Result<Vec<Flight>, AiError> {
    let text = fr24_text;
    Ok(fr24_data(status, body)?
        .iter()
        .filter_map(|f| {
            let time = |keys: &[&str]| text(f, keys).as_deref().and_then(fr24_parse_time);
            Some(Flight {
                first_seen: time(&["datetime_takeoff", "first_seen"])?,
                last_seen: time(&["datetime_landed", "last_seen"])?,
                departure: text(f, &["origin_icao", "orig_icao"]).map(|s| s.to_ascii_uppercase()),
                arrival: text(
                    f,
                    &[
                        "destination_icao_actual",
                        "dest_icao_actual",
                        "destination_icao",
                        "dest_icao",
                    ],
                )
                .map(|s| s.to_ascii_uppercase()),
                callsign: text(f, &["callsign"]),
                number: text(f, &["flight"]).map(|s| s.to_ascii_uppercase()),
                ..Flight::default()
            })
        })
        .collect())
}

/// Read Flightradar24's answer about an aircraft (one flight, the latest):
/// `None` when it flew none in the time asked about.
pub fn parse_fr24_aircraft(
    status: u16,
    body: &str,
    asked_ms: i64,
) -> Result<Option<lookup::Fr24Aircraft>, AiError> {
    let data = fr24_data(status, body)?;
    let Some(f) = data.first() else {
        return Ok(None);
    };
    let up = |k: &[&str]| fr24_text(f, k).map(|s| s.to_ascii_uppercase());
    Ok(Some(lookup::Fr24Aircraft {
        asked_ms,
        found: true,
        icao_type: up(&["type"]),
        operator: up(&["operated_as", "operating_as"]),
        painted_as: up(&["painted_as"]),
        hex: up(&["hex"]),
        seen: fr24_text(f, &["datetime_takeoff", "first_seen"])
            .and_then(|s| s.get(..10).map(str::to_owned)),
        expired: false,
    }))
}

/// Asks Flightradar24 about an aircraft, by registration.
pub trait Fr24Aircrafts: Send + Sync {
    /// Its latest flight between two times (Unix seconds; at most 14 days
    /// apart), as an aircraft; `None`: no flight then.
    fn aircraft(
        &self,
        registration: &str,
        begin: i64,
        end: i64,
    ) -> Result<Option<lookup::Fr24Aircraft>, AiError>;
}

impl Flightradar {
    pub fn new(http: Arc<dyn Http>, token: Token) -> Self {
        Self {
            http,
            token,
            plan_from: Mutex::new(None),
        }
    }

    /// GET the flight summary (light) with `query`.
    fn get(&self, query: &[(&str, &str)]) -> Result<(u16, String), AiError> {
        let token = (self.token)().ok_or_else(|| {
            cloud(
                "No Flightradar24 API token: add it in Settings → Read aircraft",
                false,
            )
        })?;
        let url = format!("{FR24_API}?{}", form(query));
        let auth = format!("Bearer {token}");
        self.http
            .get(
                &url,
                &[
                    ("authorization", &auth),
                    ("accept", "application/json"),
                    ("accept-version", "v1"),
                ],
            )
            .map_err(|e| cloud(format!("could not reach {FR24_SERVICE}: {e}"), true))
    }

    /// Whether the token works: a question with no answer (no credits):
    /// a registration no aircraft has, for a minute two hours ago (every
    /// plan covers the last days).
    pub fn test(&self, now_s: i64) -> Result<(), AiError> {
        let at = now_s - 2 * 3600;
        self.flights("ZZ-ZZZ", at, at + 60).map(|_| ())
    }
}

impl Flights for Flightradar {
    fn flights(&self, registration: &str, begin: i64, end: i64) -> Result<Vec<Flight>, AiError> {
        let reg = registration.trim().to_ascii_uppercase();
        if reg.is_empty() || reg.contains(',') {
            return Err(cloud(
                format!("{registration} is not a registration"),
                false,
            ));
        }
        let (status, body) = self.get(&[
            ("flight_datetime_from", &fr24_time(begin)),
            ("flight_datetime_to", &fr24_time(end)),
            ("registrations", &reg),
            ("limit", "100"),
        ])?;
        if status == 400 {
            // Too far back for the plan: it says from when it gives flights.
            if let Some(from) = plan_from(&body) {
                if let Ok(mut p) = self.plan_from.lock() {
                    *p = Some(from);
                }
            }
        }
        parse_fr24(status, &body)
    }

    fn source(&self) -> Source {
        Source::Flightradar
    }

    fn take_history_from(&self) -> Option<i64> {
        self.plan_from.lock().ok().and_then(|mut p| p.take())
    }
}

impl Fr24Aircrafts for Flightradar {
    fn aircraft(
        &self,
        registration: &str,
        begin: i64,
        end: i64,
    ) -> Result<Option<lookup::Fr24Aircraft>, AiError> {
        let reg = registration.trim().to_ascii_uppercase();
        if reg.is_empty() || reg.contains(',') {
            return Err(cloud(
                format!("{registration} is not a registration"),
                false,
            ));
        }
        let (status, body) = self.get(&[
            ("flight_datetime_from", &fr24_time(begin)),
            ("flight_datetime_to", &fr24_time(end)),
            ("registrations", &reg),
            ("limit", "1"),
            ("sort", "desc"),
        ])?;
        parse_fr24_aircraft(status, &body, crate::jobs::now_ms())
    }
}

/// Delete the Flightradar24 answers asked more than 30 days before `now_ms`
/// (its terms): the flights, and what it said about aircraft. Returns how
/// many were deleted.
pub fn forget_expired(cat: &mut Catalog, now_ms: i64) -> Result<u64, AiError> {
    let n = cat.write(|tx| tx.ai().forget_fr24_flights_before(now_ms - FR24_KEEP_MS))?;
    Ok(n + super::fr24::forget_expired(cat, now_ms)?)
}

/// Where a photo's flights are kept: by transponder code (OpenSky) or by
/// registration (Flightradar24).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Key {
    Icao(String),
    Reg(String),
}

impl Key {
    /// What the source is asked with.
    fn id(&self) -> &str {
        match self {
            Self::Icao(s) | Self::Reg(s) => s,
        }
    }
}

/// The flights kept under `key` on `day` (a Flightradar24 answer only for
/// 30 days); `None` when not asked yet.
fn kept_at_key(cat: &Catalog, key: &Key, day: &str) -> Result<Option<Vec<Flight>>, AiError> {
    match key {
        Key::Icao(i) => kept_day(cat, i, day),
        Key::Reg(r) => {
            let now = crate::jobs::now_ms();
            Ok(cat
                .read(|tx| tx.ai().fr24_flights(r, day))?
                .filter(|(_, at)| now - at < FR24_KEEP_MS)
                .and_then(|(j, _)| serde_json::from_str(&j).ok()))
        }
    }
}

fn keep_at_key(cat: &mut Catalog, key: &Key, day: &str, found: &[Flight]) -> Result<(), AiError> {
    let json = serde_json::to_string(found)
        .map_err(|e| AiError::Image(format!("the flights could not be kept: {e}")))?;
    let now = crate::jobs::now_ms();
    match key {
        Key::Icao(i) => cat.write(|tx| tx.ai().put_aircraft_flights(i, day, &json, now))?,
        Key::Reg(r) => cat.write(|tx| tx.ai().put_fr24_flights(r, day, &json, now))?,
    }
    Ok(())
}

fn forget_at_key(cat: &mut Catalog, key: &Key, day: &str) -> Result<(), AiError> {
    match key {
        Key::Icao(i) => cat.write(|tx| tx.ai().forget_aircraft_flights(i, day))?,
        Key::Reg(r) => cat.write(|tx| tx.ai().forget_fr24_flights(r, day))?,
    }
    Ok(())
}

/// The registration as Flightradar24's answers are kept under.
fn reg_key(registration: &str) -> Key {
    Key::Reg(registration.trim().to_ascii_uppercase())
}

/// An airport of the list.
#[derive(Debug, Clone, PartialEq)]
pub struct Airport {
    pub icao: String,
    pub iata: String,
    pub country: String,
    pub lat: f64,
    pub lon: f64,
    pub name: String,
}

/// The airports with an IATA code, by ICAO code.
pub struct Airports {
    by_icao: HashMap<String, Airport>,
}

impl Airports {
    pub fn from_tsv(text: &str) -> Self {
        let by_icao = text
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                let [icao, iata, country, lat, lon, name] = f.as_slice() else {
                    return None;
                };
                Some((
                    icao.to_string(),
                    Airport {
                        icao: icao.to_string(),
                        iata: iata.to_string(),
                        country: country.to_string(),
                        lat: lat.parse().ok()?,
                        lon: lon.parse().ok()?,
                        name: name.to_string(),
                    },
                ))
            })
            .collect();
        Self { by_icao }
    }

    /// `airports.tsv`, built in.
    pub fn builtin() -> Self {
        Self::from_tsv(AIRPORTS_TSV)
    }

    pub fn len(&self) -> usize {
        self.by_icao.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_icao.is_empty()
    }

    pub fn get(&self, icao: &str) -> Option<&Airport> {
        self.by_icao.get(&icao.trim().to_ascii_uppercase())
    }

    /// The ICAO code of an airport as a keyword names it: its IATA code
    /// (`YYZ`) or its ICAO code (`CYYZ`); `None` when not in the list.
    pub fn icao_of(&self, code: &str) -> Option<String> {
        let c = code.trim().to_ascii_uppercase();
        if self.by_icao.contains_key(&c) {
            return Some(c);
        }
        self.by_icao
            .values()
            .filter(|a| a.iata.eq_ignore_ascii_case(&c))
            .map(|a| a.icao.clone())
            .min()
    }

    /// The code to show (IATA, else the ICAO code kept) and the country,
    /// when known.
    pub fn place(&self, icao: &str) -> (String, Option<String>) {
        match self.get(icao) {
            Some(a) => (a.iata.clone(), Some(a.country.clone())),
            None => (icao.trim().to_ascii_uppercase(), None),
        }
    }
}

/// Where the photo was taken in the flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum At {
    /// Near when it took off (or climbing out).
    Departure,
    /// Near when it landed (or on approach).
    Arrival,
}

/// A flight offered for a photo.
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    pub flight: Flight,
    pub at: At,
    /// Seconds between the photo and the take-off or landing (0: in the air).
    pub gap_s: i64,
}

/// Which flight a photo shows.
#[derive(Debug, Clone, PartialEq)]
pub enum Pick {
    One(Choice),
    /// Equally close: the user chooses.
    Several(Vec<Choice>),
    /// Flights that day, but none within [`NEAR_S`] of the photo.
    NoneNear,
    /// No flights known for that aircraft and day.
    Nothing,
}

/// Distance in km between two points.
fn km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (la1, lo1, la2, lo2) = (
        a.0.to_radians(),
        a.1.to_radians(),
        b.0.to_radians(),
        b.1.to_radians(),
    );
    let h = ((la2 - la1) / 2.0).sin().powi(2)
        + la1.cos() * la2.cos() * ((lo2 - lo1) / 2.0).sin().powi(2);
    2.0 * 6371.0 * h.sqrt().asin()
}

/// An airliner's speed between airports, for [`missing_leg`].
const CRUISE_KMH: f64 = 850.0;
/// Time on the ground at both ends, for [`missing_leg`].
const GROUND_S: i64 = 3600;

/// The flight the source did not see, when the photo falls in it: the
/// aircraft landed at one airport (`a`) and next took off from another
/// (`b`), so it flew from the first to the second in between (OpenSky has
/// no receivers over oceans: SP-LSC landed at YYZ at 21:22 UTC and next
/// left WAW at 10:52; its YYZ → WAW flight is in neither day's answer).
/// It must have had time for it: the flight at [`CRUISE_KMH`] and an hour
/// on the ground ([`GROUND_S`]); a pair of airports not in the list, at
/// least two hours.
pub fn missing_leg(photo_s: i64, flights: &[Flight], airports: &Airports) -> Option<Flight> {
    let mut seen: Vec<&Flight> = flights.iter().filter(|f| !f.inferred).collect();
    seen.sort_by_key(|f| f.first_seen);
    seen.windows(2).find_map(|w| {
        let (a, b) = (w[0], w[1]);
        let (landed, left) = (a.arrival.as_deref()?, b.departure.as_deref()?);
        let needed = match (airports.get(landed), airports.get(left)) {
            (Some(x), Some(y)) => {
                let hours = km((x.lat, x.lon), (y.lat, y.lon)) / CRUISE_KMH;
                GROUND_S + (hours * 3600.0) as i64
            }
            _ => 2 * 3600,
        };
        (landed != left
            && a.last_seen < photo_s
            && photo_s < b.first_seen
            && b.first_seen - a.last_seen >= needed)
            .then(|| Flight {
                first_seen: a.last_seen,
                last_seen: b.first_seen,
                departure: Some(landed.to_owned()),
                arrival: Some(left.to_owned()),
                inferred: true,
                ..Flight::default()
            })
    })
}

/// Least time an aircraft is on the ground between landing and taking
/// off again, for [`missing_end`].
const TURN_S: i64 = 40 * 60;
/// Farthest from the photo the take-off or landing seen may be, for
/// [`missing_end`].
const END_S: i64 = 6 * 3600;

/// The flight in or out the source did not see, when the photo falls
/// before everything it saw or after it: CN-RGY was seen only taking off
/// from YYZ for CMN at 02:30 UTC, the photo taken 2½ hours before as it
/// landed from Casablanca, over the Atlantic and out of OpenSky's reach.
/// The flight in: nothing seen before the photo, and the first take-off
/// seen at least [`TURN_S`] (and at most [`END_S`]) after it; it landed
/// where that take-off left from, most likely from where that went (an
/// aircraft flies out and back). The flight out the same way round: nothing
/// seen after the photo, the last landing seen [`TURN_S`] to [`END_S`]
/// before it; most likely back to where that came from.
pub fn missing_end(photo_s: i64, flights: &[Flight]) -> Option<Flight> {
    let seen: Vec<&Flight> = flights.iter().filter(|f| !f.inferred).collect();
    let before = seen.iter().any(|f| f.first_seen <= photo_s);
    let after = seen.iter().any(|f| f.last_seen >= photo_s);
    let near = |gap: i64| (TURN_S..=END_S).contains(&gap);
    if !before {
        let next = seen.iter().min_by_key(|f| f.first_seen)?;
        let at = next.departure.clone()?;
        return near(next.first_seen - photo_s).then(|| Flight {
            first_seen: next.first_seen,
            last_seen: next.first_seen,
            departure: next.arrival.clone().filter(|a| *a != at),
            arrival: Some(at),
            inferred: true,
            half: Some(At::Arrival),
            ..Flight::default()
        });
    }
    if !after {
        let last = seen.iter().max_by_key(|f| f.last_seen)?;
        let at = last.arrival.clone()?;
        return near(photo_s - last.last_seen).then(|| Flight {
            first_seen: last.last_seen,
            last_seen: last.last_seen,
            arrival: last.departure.clone().filter(|d| *d != at),
            departure: Some(at),
            inferred: true,
            half: Some(At::Departure),
            ..Flight::default()
        });
    }
    None
}

/// Fill in the end of each flight the source did not see from where its
/// callsign usually flies (adsbdb's flight routes, by [`Lookup`]): OpenSky
/// saw RAM211B leave YYZ but not where it went (over the Atlantic); RAM211
/// flies YYZ → CMN. Only when the end seen is the route's (a callsign
/// flown elsewhere that day says nothing). Each callsign is asked once
/// (`routes` keeps the answers for the job); a flight is not asked about
/// again (`route_asked`). A route that could not be asked is left for next
/// time. Returns the requests sent and whether a flight changed.
pub fn fill_ends(
    flights: &mut [Flight],
    lookup: &dyn Lookup,
    routes: &mut HashMap<String, Option<(String, String)>>,
) -> (u32, bool) {
    let mut requests = 0;
    let mut changed = false;
    for f in flights.iter_mut() {
        if f.inferred || f.route_asked || f.departure.is_some() == f.arrival.is_some() {
            continue;
        }
        let Some(cs) = f.callsign.clone().filter(|c| !c.trim().is_empty()) else {
            continue;
        };
        let route = match routes.get(&cs) {
            Some(r) => r.clone(),
            None => {
                requests += 1;
                match lookup.callsign_route(&cs) {
                    Ok(r) => {
                        routes.insert(cs.clone(), r.clone());
                        r
                    }
                    Err(e) => {
                        tracing::warn!(callsign = %cs, error = %e, "callsign route not asked");
                        continue;
                    }
                }
            }
        };
        f.route_asked = true;
        changed = true;
        let Some((from, to)) = route else {
            continue;
        };
        if f.departure.as_deref() == Some(from.as_str()) && f.arrival.is_none() && to != from {
            f.arrival = Some(to);
            f.by_callsign = true;
        } else if f.arrival.as_deref() == Some(to.as_str()) && f.departure.is_none() && to != from {
            f.departure = Some(from);
            f.by_callsign = true;
        }
    }
    (requests, changed)
}

/// The flight the photo (taken at `photo_s`, Unix seconds, at `gps`)
/// shows: the one that took off or landed nearest it, within [`NEAR_S`]
/// (in the air at the time counts as nearest). Flights within [`TIE_S`] of
/// each other are equally close: the photo's position, when known, picks
/// the one whose airport is nearest; otherwise all are offered. When the
/// photo falls between a landing and a take-off from another airport, the
/// flight between them the source did not see ([`missing_leg`]) is offered
/// too, beside the flights near; so is the flight in or out it did not see
/// when the photo is before or after all it saw ([`missing_end`]): the
/// user chooses. Never a silent guess.
pub fn pick(
    photo_s: i64,
    gps: Option<(f64, f64)>,
    flights: &[Flight],
    airports: &Airports,
) -> Pick {
    if flights.is_empty() {
        return Pick::Nothing;
    }
    let mut near: Vec<Choice> = flights
        .iter()
        .filter_map(|f| {
            let (gap_s, at) = if photo_s < f.first_seen {
                (f.first_seen - photo_s, At::Departure)
            } else if photo_s > f.last_seen {
                (photo_s - f.last_seen, At::Arrival)
            } else if photo_s - f.first_seen <= f.last_seen - photo_s {
                (0, At::Departure)
            } else {
                (0, At::Arrival)
            };
            (gap_s <= NEAR_S).then(|| Choice {
                flight: f.clone(),
                at,
                gap_s,
            })
        })
        .collect();
    let leg = missing_leg(photo_s, flights, airports).or_else(|| missing_end(photo_s, flights));
    if let Some(leg) = leg {
        near.push(Choice {
            at: leg.half.unwrap_or(At::Departure),
            flight: leg,
            gap_s: 0,
        });
        // In time order; the flight in before the take-off it was seen
        // making next.
        near.sort_by_key(|c| (c.flight.first_seen, !c.flight.inferred));
        return if near.len() == 1 {
            Pick::One(near.swap_remove(0))
        } else {
            Pick::Several(near)
        };
    }
    if near.is_empty() {
        return Pick::NoneNear;
    }
    near.sort_by_key(|c| (c.gap_s, c.flight.first_seen));
    let best = near[0].gap_s;
    let mut close: Vec<Choice> = near
        .into_iter()
        .filter(|c| c.gap_s - best <= TIE_S)
        .collect();
    if close.len() > 1 {
        if let Some(p) = gps {
            let dist = |c: &Choice| {
                let code = match c.at {
                    At::Departure => c.flight.departure.as_deref(),
                    At::Arrival => c.flight.arrival.as_deref(),
                };
                code.and_then(|c| airports.get(c))
                    .map(|a| km(p, (a.lat, a.lon)))
            };
            let d: Vec<Option<f64>> = close.iter().map(dist).collect();
            if let Some(min) = d.iter().flatten().copied().reduce(f64::min) {
                // One clearly nearest (by more than 5 km) wins.
                let nearest: Vec<usize> = (0..close.len())
                    .filter(|&i| d[i].is_some_and(|x| x - min <= 5.0))
                    .collect();
                if nearest.len() == 1 {
                    return Pick::One(close.swap_remove(nearest[0]));
                }
            }
        }
        return Pick::Several(close);
    }
    Pick::One(close.swap_remove(0))
}

/// The keywords to offer for a flight: where from and to (IATA, else the
/// ICAO code; none when OpenSky did not see it), their countries, and the
/// flight number ([`flight_number`]: `AC856`, else the callsign).
pub fn keywords(f: &Flight, airports: &Airports) -> Result<Vec<TagPath>, AiError> {
    let mut out = Vec::new();
    let mut add = |kind: &str, name: &str| -> Result<(), AiError> {
        let name = name.replace(['/', '|'], "-");
        out.push(TagPath::new([TAG_PARENT, kind, name.trim()])?);
        Ok(())
    };
    for (end, kind, country) in [
        (&f.departure, "From", "From country"),
        (&f.arrival, "To", "To country"),
    ] {
        if let Some(code) = end {
            let (shown, c) = airports.place(code);
            add(kind, &shown)?;
            if let Some(c) = c {
                add(country, &c)?;
            }
        }
    }
    if let Some(n) = f.shown_number() {
        add("Flight", &n)?;
    }
    Ok(out)
}

/// Accept a photo's route: its route keywords become `paths` (those it had
/// from another route are taken off: a photo is of one flight), as the
/// aircraft reader's. Returns how many photos got something new.
pub fn accept(
    cat: &mut Catalog,
    lib: &crate::library::Library,
    asset: AssetId,
    paths: &[TagPath],
) -> Result<u64, AiError> {
    if let Some(p) = paths.iter().find(|p| !is_route_keyword(p)) {
        return Err(cloud(
            format!("{} is not a route keyword", p.to_lightroom()),
            false,
        ));
    }
    let lib_err = |e: crate::library::LibraryError| AiError::Image(e.to_string());
    let mut svc = crate::metadata::MetadataService::new(lib, cat);
    let had = svc.user_tags(asset).map_err(lib_err)?;
    let keep: Vec<TagPath> = had
        .iter()
        .filter(|t| !is_route_keyword(t) || paths.contains(t))
        .cloned()
        .collect();
    if keep.len() != had.len() {
        svc.set_user_tags(asset, &keep).map_err(lib_err)?;
    }
    super::scan::add_to(cat, lib, &[asset], paths)
}

/// Is `path` a keyword a route gives (`Aircraft|From|YYZ`, …)?
pub fn is_route_keyword(path: &TagPath) -> bool {
    let s = path.segments();
    s.len() == 3 && s[0] == TAG_PARENT && KINDS.contains(&s[1].as_str())
}

/// `2026-05-31` (UTC) and the day's start, of a Unix second.
fn utc_day(s: i64) -> (String, i64) {
    let start = s.div_euclid(DAY_S) * DAY_S;
    let day = chrono::DateTime::from_timestamp(start, 0)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    (day, start)
}

/// The UTC days a photo's flights are looked for in: those within
/// [`NEAR_S`] of it (one, or two near midnight).
pub fn days_for(photo_s: i64) -> Vec<(String, i64)> {
    let mut out = vec![utc_day(photo_s - NEAR_S)];
    let last = utc_day(photo_s + NEAR_S);
    if last.0 != out[0].0 {
        out.push(last);
    }
    out
}

/// The most UTC days one OpenSky question covers. OpenSky allows two ("a
/// time interval … not larger than 2 days") but charges a question under
/// 24 hours 4 credits and one of one to two days 30: one day at a time
/// saves credits (the owner's choice, ADR-049).
pub const MAX_WINDOW_DAYS: usize = 1;
/// How many queued photos one job looks through for others of its
/// aircraft.
const PEERS: u32 = 1000;

/// The questions to ask for `days` (UTC day starts, any order), at most
/// [`MAX_WINDOW_DAYS`] each ([`windows_up_to`]).
pub fn windows(days: Vec<i64>) -> Vec<Vec<i64>> {
    windows_up_to(days, MAX_WINDOW_DAYS)
}

/// The questions to ask for `days`: runs of consecutive days, each cut into
/// at most `max` days. Days far apart are separate questions; each is asked
/// from the first day's start to the last day's end.
pub fn windows_up_to(mut days: Vec<i64>, max: usize) -> Vec<Vec<i64>> {
    days.sort_unstable();
    days.dedup();
    let mut out: Vec<Vec<i64>> = Vec::new();
    for d in days {
        match out.last_mut() {
            Some(w) if w.len() < max && w.last() == Some(&(d - DAY_S)) => w.push(d),
            _ => out.push(vec![d]),
        }
    }
    out
}

/// The UTC days (starts) of the other photos waiting for Add route whose
/// aircraft has transponder code `icao24`: asked about with `asset`'s.
fn peer_days(
    cat: &Catalog,
    asset: AssetId,
    icao24: &str,
    clock: Clock,
) -> Result<Vec<i64>, AiError> {
    let queued = cat.read(|tx| tx.jobs().queued_assets(JOB_AI_ROUTE, PEERS))?;
    // A registration's code, looked up once.
    let mut codes: HashMap<String, Option<String>> = HashMap::new();
    let mut out = Vec::new();
    for id in queued.into_iter().filter(|id| *id != asset) {
        let p = photo(cat, id, clock)?;
        let (Some(reg), Some(t)) = (p.registration, p.taken_s) else {
            continue;
        };
        let code = match codes.get(&reg) {
            Some(c) => c.clone(),
            None => {
                let c = icao24_of(cat, &reg)?;
                codes.insert(reg, c.clone());
                c
            }
        };
        if code.as_deref() == Some(icao24) {
            out.extend(days_for(t).into_iter().map(|(_, start)| start));
        }
    }
    Ok(out)
}

/// Add `more` to `all`, once each: the same flight kept under two days (one
/// answer covering both) is one flight; the copy with its ends filled in
/// ([`fill_ends`]) wins.
fn merge_flights(all: &mut Vec<Flight>, more: Vec<Flight>) {
    let same = |a: &Flight, b: &Flight| {
        a.first_seen == b.first_seen
            && a.last_seen == b.last_seen
            && a.callsign == b.callsign
            && a.inferred == b.inferred
    };
    for f in more {
        match all.iter_mut().find(|g| same(g, &f)) {
            Some(g) => {
                if f.route_asked && !g.route_asked {
                    *g = f;
                }
            }
            None => all.push(f),
        }
    }
}

/// How Add route reads the camera's clock (Settings → Read aircraft).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Clock {
    /// The time and time zone as the camera recorded them.
    #[default]
    AsRecorded,
    /// The camera's clock showed Eastern time, changed for daylight
    /// saving, though it marked its photos UTC−05:00 (or with no time
    /// zone): the offset is worked out from the date instead (−04:00 in
    /// daylight saving, −05:00 otherwise). Other time zones are kept.
    Eastern,
}

/// Eastern standard time, in minutes from UTC.
const EST_MIN: i16 = -300;
/// Eastern daylight time.
const EDT_MIN: i16 = -240;

/// Eastern time's offset from UTC (minutes) for a wall-clock reading
/// (`wall_s`: local time as if it were UTC). Daylight saving as in Canada
/// and the US: from 2007, the second Sunday of March to the first Sunday
/// of November; 1987–2006, the first Sunday of April to the last Sunday
/// of October (earlier: the last Sunday of April); 02:00 local each.
pub fn eastern_offset(wall_s: i64) -> i16 {
    use chrono::{Datelike, NaiveDate, Weekday};
    let Some(wall) = chrono::DateTime::from_timestamp(wall_s, 0).map(|d| d.naive_utc()) else {
        return EST_MIN;
    };
    let y = wall.year();
    let nth = |m: u32, n: u8| NaiveDate::from_weekday_of_month_opt(y, m, Weekday::Sun, n);
    let last = |m: u32| nth(m, 5).or_else(|| nth(m, 4));
    let (start, end) = if y >= 2007 {
        (nth(3, 2), nth(11, 1))
    } else if y >= 1987 {
        (nth(4, 1), last(10))
    } else {
        (last(4), last(10))
    };
    let at2 = |d: Option<NaiveDate>| d.and_then(|d| d.and_hms_opt(2, 0, 0));
    match (at2(start), at2(end)) {
        (Some(s), Some(e)) if wall >= s && wall < e => EDT_MIN,
        _ => EST_MIN,
    }
}

/// The capture instant (Unix seconds, UTC) and the offset it is read with
/// (`None`: not known), under `clock`. A time taken from a sidecar
/// (`user_set`: a time zone set in Keepsake, or another program's date) is
/// kept as it is.
pub fn read_clock(
    utc_ms: i64,
    offset_min: Option<i16>,
    clock: Clock,
    user_set: bool,
) -> (i64, Option<i16>) {
    let s = utc_ms.div_euclid(1000);
    match (clock, offset_min) {
        (Clock::Eastern, None | Some(EST_MIN)) if !user_set => {
            let wall = s + i64::from(offset_min.unwrap_or(0)) * 60;
            let o = eastern_offset(wall);
            (wall - i64::from(o) * 60, Some(o))
        }
        _ => (s, offset_min),
    }
}

/// What a photo needs for its route: its registration, capture time and
/// position.
#[derive(Debug, Clone, PartialEq)]
pub struct Photo {
    pub registration: Option<String>,
    /// Unix seconds, UTC, as read under the [`Clock`].
    pub taken_s: Option<i64>,
    /// The offset from UTC it was read with (minutes), when known.
    pub offset_min: Option<i16>,
    pub gps: Option<(f64, f64)>,
}

/// The photo's accepted registration, capture time and position.
pub fn photo(cat: &Catalog, asset: AssetId, clock: Clock) -> Result<Photo, AiError> {
    let a = cat.read(|tx| tx.assets().get(asset))?;
    let registration = cat
        .read(|tx| tx.tags().for_asset(asset))?
        .into_iter()
        .filter(|t| t.source == TagSource::User)
        .find_map(|t| {
            let s = t.path.segments();
            (s.len() == 3 && s[0] == TAG_PARENT && s[1] == "Registration").then(|| s[2].clone())
        });
    let user_set = a
        .as_ref()
        .is_some_and(|a| a.capture_source == Some(crate::catalog::CaptureSource::Sidecar));
    let read = a
        .as_ref()
        .and_then(|a| a.capture)
        .map(|c| read_clock(c.utc_ms, c.utc_offset_min, clock, user_set));
    Ok(Photo {
        registration,
        taken_s: read.map(|r| r.0),
        offset_min: read.and_then(|r| r.1),
        gps: a.and_then(|a| a.gps).map(|g| (g.lat, g.lon)),
    })
}

/// The transponder code of `registration`, as the lookup keeps it: the
/// one the user typed, else the lookup's, else Flightradar24's (while it
/// may be kept).
pub fn icao24_of(cat: &Catalog, registration: &str) -> Result<Option<String>, AiError> {
    let now = crate::jobs::now_ms();
    let kept = lookup::kept(cat, registration)?;
    let typed = kept
        .as_ref()
        .and_then(|f| f.typed_mode_s.clone().or(f.mode_s.clone()));
    // Then a national register (ADR-045), then Flightradar24.
    let code = match typed {
        Some(c) => Some(c),
        None => match super::register::get(cat, registration)?.and_then(|r| r.mode_s) {
            Some(c) => Some(c),
            None => kept
                .and_then(|f| f.fr24)
                .filter(|a| a.fresh(now))
                .and_then(|a| a.hex),
        },
    };
    Ok(code.map(|m| m.to_ascii_lowercase()))
}

/// The flights of `registration`'s aircraft kept for the days around
/// `photo_s` (OpenSky's by its transponder code, and Flightradar24's while
/// kept); nothing is asked.
pub fn kept_around(
    cat: &Catalog,
    registration: &str,
    photo_s: i64,
) -> Result<Vec<Flight>, AiError> {
    let mut keys = vec![reg_key(registration)];
    if let Some(i) = icao24_of(cat, registration)? {
        keys.push(Key::Icao(i));
    }
    let mut all: Vec<Flight> = Vec::new();
    for key in &keys {
        for (day, _) in days_for(photo_s) {
            merge_flights(&mut all, kept_at_key(cat, key, &day)?.unwrap_or_default());
        }
    }
    Ok(all)
}

/// The kept flights of `icao24` on `day`; `None` when not asked yet.
pub fn kept_day(cat: &Catalog, icao24: &str, day: &str) -> Result<Option<Vec<Flight>>, AiError> {
    Ok(cat
        .read(|tx| tx.ai().aircraft_flights(icao24, day))?
        .and_then(|j| serde_json::from_str(&j).ok()))
}

/// What one `ai.route` job did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Asked {
    /// Requests sent (OpenSky and the registration lookup): the worker
    /// waits between each.
    pub requests: u32,
    /// Where the photo's flights were asked for ([`run_job_auto`]).
    pub source: Option<Source>,
    /// How many days back Flightradar24 said the user's plan goes, when it
    /// said so: kept for the next photos ([`Sources::learn`]).
    pub fr24_days: Option<i64>,
}

/// Where Add route asks (ADR-050): OpenSky's and Flightradar24's clients,
/// those set up, and how it chooses between them.
#[derive(Clone)]
pub struct Sources {
    pub mode: Mode,
    pub opensky: Option<Arc<dyn Flights>>,
    pub fr24: Option<Arc<dyn Flights>>,
    /// How many days back the user's Flightradar24 plan goes (learnt from
    /// its answers, [`Self::learn`]).
    fr24_days: Arc<std::sync::atomic::AtomicI64>,
    /// Told when Flightradar24 says how far back the plan goes (days), to
    /// keep it in the settings.
    on_learn: Option<Arc<dyn Fn(i64) + Send + Sync>>,
}

impl Sources {
    pub fn new(
        mode: Mode,
        opensky: Option<Arc<dyn Flights>>,
        fr24: Option<Arc<dyn Flights>>,
        fr24_days: i64,
    ) -> Self {
        Self {
            mode,
            opensky,
            fr24,
            fr24_days: Arc::new(std::sync::atomic::AtomicI64::new(fr24_days)),
            on_learn: None,
        }
    }

    /// One client, as before: only it is asked.
    pub fn only(flights: Arc<dyn Flights>) -> Self {
        let s = flights.source();
        let (opensky, fr24) = match s {
            Source::OpenSky => (Some(flights), None),
            Source::Flightradar => (None, Some(flights)),
        };
        Self::new(Mode::Only(s), opensky, fr24, FR24_DAYS)
    }

    /// Call `f` with the plan's days when Flightradar24 says them.
    pub fn on_learn(mut self, f: Arc<dyn Fn(i64) + Send + Sync>) -> Self {
        self.on_learn = Some(f);
        self
    }

    pub fn fr24_days(&self) -> i64 {
        self.fr24_days.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Flightradar24 said its plan gives flights from `from_s`: the plan
    /// goes back that many whole days from `now_s`. Returns the days.
    pub fn learn(&self, from_s: i64, now_s: i64) -> i64 {
        let days = ((now_s - from_s) / DAY_S).max(0);
        let was = self
            .fr24_days
            .swap(days, std::sync::atomic::Ordering::SeqCst);
        if was != days {
            tracing::info!(
                days,
                "Flightradar24's plan gives flights this many days back"
            );
            if let Some(f) = &self.on_learn {
                f(days);
            }
        }
        days
    }

    /// What a photo's source is chosen with, at `now_s`.
    pub fn choose(&self, now_s: i64) -> Choose {
        Choose {
            mode: self.mode,
            fr24_days: self.fr24_days(),
            opensky: self.opensky.is_some(),
            fr24: self.fr24.is_some(),
            now_s,
        }
    }

    fn client(&self, s: Source) -> Option<&dyn Flights> {
        match s {
            Source::OpenSky => self.opensky.as_deref(),
            Source::Flightradar => self.fr24.as_deref(),
        }
    }
}

/// One `ai.route` job, asking where [`Sources`] says for the photo's date
/// ([`Choose::source`]). Automatic: when Flightradar24 refuses the photo as
/// older than the user's plan, the plan's days are learnt and OpenSky is
/// asked instead (if set up).
pub fn run_job_auto(
    cat: &mut Catalog,
    sources: &Sources,
    lookup: Option<&dyn Lookup>,
    asset: AssetId,
    clock: Clock,
    now_s: i64,
) -> Result<Asked, AiError> {
    let p = photo(cat, asset, clock)?;
    // No capture time: any client says so.
    let Some(t) = p.taken_s else {
        let any = sources.opensky.as_deref().or(sources.fr24.as_deref());
        return match any {
            Some(f) => run_job(cat, f, lookup, asset, clock),
            None => Err(cloud("the photo has no capture time", false)),
        };
    };
    let src = sources
        .choose(now_s)
        .source(t)
        .map_err(|why| cloud(why, false))?;
    let Some(client) = sources.client(src) else {
        return Err(cloud(format!("{} is not set up", src.name()), false));
    };
    let first = run_job(cat, client, lookup, asset, clock);
    let learnt = (src == Source::Flightradar)
        .then(|| client.take_history_from())
        .flatten()
        .map(|from| sources.learn(from, now_s));
    let mut asked = match first {
        Ok(a) => a,
        Err(e) => {
            // Before the plan: OpenSky, when automatic and set up.
            let refused = learnt.is_some() && !sources.choose(now_s).fr24_covers(t);
            match (sources.mode, sources.opensky.as_deref()) {
                (Mode::Auto, Some(sky)) if refused => {
                    let mut a = run_job(cat, sky, lookup, asset, clock)?;
                    a.requests += 1;
                    a.source = Some(Source::OpenSky);
                    a.fr24_days = learnt;
                    return Ok(a);
                }
                _ => return Err(e),
            }
        }
    };
    asked.source = Some(src);
    asked.fr24_days = learnt;
    Ok(asked)
}

/// One `ai.route` job: make sure the flights of `asset`'s aircraft on the
/// days around it are kept, asking OpenSky for those not kept yet (and
/// the registration's transponder code first, if unknown and `lookup` is
/// given; with `lookup`, the ends OpenSky did not see are filled in from
/// the callsigns' routes, [`fill_ends`]). Choosing the flight is left to the window ([`review`]).
pub fn run_job(
    cat: &mut Catalog,
    flights: &dyn Flights,
    lookup: Option<&dyn Lookup>,
    asset: AssetId,
    clock: Clock,
) -> Result<Asked, AiError> {
    let mut asked = Asked::default();
    let p = photo(cat, asset, clock)?;
    let Some(reg) = p.registration else {
        return Err(cloud(
            "the photo has no registration: Read aircraft first",
            false,
        ));
    };
    let Some(t) = p.taken_s else {
        return Err(cloud("the photo has no capture time", false));
    };
    if flights.source() == Source::Flightradar {
        // By registration: no transponder code needed.
        forget_expired(cat, crate::jobs::now_ms())?;
        let key = reg_key(&reg);
        for (day, start) in days_for(t) {
            if kept_at_key(cat, &key, &day)?.is_some() {
                continue;
            }
            let found = flights.flights(key.id(), start, start + DAY_S - 1)?;
            asked.requests += 1;
            keep_at_key(cat, &key, &day, &found)?;
        }
        return Ok(asked);
    }
    let mut icao = icao24_of(cat, &reg)?;
    if icao.is_none() {
        if let Some(l) = lookup {
            if lookup::kept(cat, &reg)?.is_none_or(|f| f.stale()) {
                lookup::lookup_cached(cat, Some(l), &reg)?;
                asked.requests += 1;
                icao = icao24_of(cat, &reg)?;
            }
        }
    }
    let Some(icao) = icao else {
        return Err(cloud(
            if lookup.is_some() {
                format!(
                    "the transponder code of {reg} is not known: neither adsbdb nor OpenSky's aircraft database lists it. Type it in the Add route window"
                )
            } else {
                format!(
                    "the transponder code of {reg} is not known (turn on Look registrations up online, or type it in the Add route window)"
                )
            },
            false,
        ));
    };
    let key = Key::Icao(icao.clone());
    let mut routes = HashMap::new();
    // The days not kept yet: this photo's, and those of the other photos of
    // this aircraft waiting to be asked about, so that one question covers
    // them all (as few as OpenSky allows, [`windows`]).
    let mut need: Vec<i64> = Vec::new();
    for (day, start) in days_for(t) {
        if kept_at_key(cat, &key, &day)?.is_none() {
            need.push(start);
        }
    }
    if !need.is_empty() {
        for start in peer_days(cat, asset, &icao, clock)? {
            if !need.contains(&start) && kept_at_key(cat, &key, &utc_day(start).0)?.is_none() {
                need.push(start);
            }
        }
    }
    for w in windows(need) {
        let (Some(first), Some(last)) = (w.first(), w.last()) else {
            continue;
        };
        let mut found = flights.flights(key.id(), *first, *last + DAY_S - 1)?;
        asked.requests += 1;
        tracing::info!(
            icao24 = key.id(),
            from = %utc_day(*first).0,
            days = w.len(),
            flights = found.len(),
            "Add route asked OpenSky"
        );
        // The ends OpenSky did not see, once for the whole answer.
        if let Some(l) = lookup {
            asked.requests += fill_ends(&mut found, l, &mut routes).0;
        }
        // Kept under each day it covers: a photo reads its days' answers.
        for start in &w {
            keep_at_key(cat, &key, &utc_day(*start).0, &found)?;
        }
    }
    for (day, start) in days_for(t) {
        let (mut found, mut changed) = match kept_at_key(cat, &key, &day)? {
            Some(v) => (v, false),
            None => {
                let v = flights.flights(key.id(), start, start + DAY_S - 1)?;
                asked.requests += 1;
                (v, true)
            }
        };
        // The ends OpenSky did not see, from the callsigns' routes.
        if let Some(l) = lookup {
            let (n, c) = fill_ends(&mut found, l, &mut routes);
            asked.requests += n;
            changed |= c;
        }
        if changed {
            keep_at_key(cat, &key, &day, &found)?;
        }
    }
    Ok(asked)
}

/// Where a photo's route stands, for the window.
#[derive(Debug, Clone, PartialEq)]
pub enum State {
    /// No `Aircraft|Registration|…` keyword.
    NoRegistration,
    NoCaptureTime,
    /// The transponder code is not known (no lookup).
    NoTransponder,
    /// Not asked yet (queued, or failed: see the job).
    Waiting,
    Found(Pick),
}

/// A photo of the window.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteItem {
    pub asset: AssetId,
    pub registration: Option<String>,
    pub taken_s: Option<i64>,
    /// The offset from UTC the capture time was read with, when known.
    pub offset_min: Option<i16>,
    pub state: State,
    /// The route keywords it has already.
    pub has: Vec<TagPath>,
    /// Where its flights come from (or are asked for); `None` when it is
    /// not asked (no registration or capture time).
    pub source: Option<Source>,
}

/// Where `asset`'s route stands, from `source`'s answers kept (else the
/// other's): nothing is asked here.
pub fn review(
    cat: &Catalog,
    airports: &Airports,
    asset: AssetId,
    clock: Clock,
    source: Source,
) -> Result<RouteItem, AiError> {
    let choose = Choose {
        mode: Mode::Only(source),
        fr24_days: FR24_DAYS,
        opensky: true,
        fr24: true,
        now_s: crate::jobs::now_ms() / 1000,
    };
    review_with(cat, airports, asset, clock, &choose)
}

/// Where `asset`'s route stands: worked out from what is kept (nothing is
/// asked here). The answers of the source chosen for its date
/// ([`Choose::source`]) are read first, then the other's: a photo answered
/// before the choice changed (its date left Flightradar24's plan, or the
/// setting changed) keeps its answer.
pub fn review_with(
    cat: &Catalog,
    airports: &Airports,
    asset: AssetId,
    clock: Clock,
    choose: &Choose,
) -> Result<RouteItem, AiError> {
    let p = photo(cat, asset, clock)?;
    let has = cat
        .read(|tx| tx.tags().for_asset(asset))?
        .into_iter()
        .filter(|t| t.source == TagSource::User && is_route_keyword(&t.path))
        .map(|t| t.path)
        .collect();
    let item = |state, source| RouteItem {
        asset,
        registration: p.registration.clone(),
        taken_s: p.taken_s,
        offset_min: p.offset_min,
        state,
        has,
        source,
    };
    let Some(reg) = p.registration.as_deref() else {
        return Ok(item(State::NoRegistration, None));
    };
    let Some(t) = p.taken_s else {
        return Ok(item(State::NoCaptureTime, None));
    };
    let chosen = choose.source(t).unwrap_or(match choose.mode {
        Mode::Only(s) => s,
        Mode::Auto => Source::OpenSky,
    });
    let icao = icao24_of(cat, reg)?;
    let key_of = |s: Source| match s {
        Source::Flightradar => Some(reg_key(reg)),
        Source::OpenSky => icao.clone().map(Key::Icao),
    };
    let other = match chosen {
        Source::OpenSky => Source::Flightradar,
        Source::Flightradar => Source::OpenSky,
    };
    for s in [chosen, other] {
        let Some(key) = key_of(s) else {
            continue;
        };
        let mut all: Vec<Flight> = Vec::new();
        let mut kept = true;
        for (day, _) in days_for(t) {
            match kept_at_key(cat, &key, &day)? {
                Some(v) => merge_flights(&mut all, v),
                None => kept = false,
            }
        }
        if kept {
            return Ok(item(State::Found(pick(t, p.gps, &all, airports)), Some(s)));
        }
    }
    let waiting = cat
        .read(|tx| tx.jobs().find(JOB_AI_ROUTE, asset))?
        .is_some_and(|j| matches!(j.state, JobState::Queued | JobState::Running));
    let state = if chosen == Source::OpenSky && icao.is_none() && !waiting {
        State::NoTransponder
    } else {
        State::Waiting
    };
    Ok(item(state, Some(chosen)))
}

/// Queue Add route for `ids` (again for those done before).
pub fn enqueue(cat: &mut Catalog, ids: &[AssetId]) -> Result<u64, AiError> {
    let now = crate::jobs::now_ms();
    let size = crate::render::ENQUEUE_CHUNK;
    let mut n = 0;
    for (c, chunk) in ids.chunks(size).enumerate() {
        n += cat.write(|tx| {
            let jobs: Vec<NewJob<'_>> = chunk
                .iter()
                .enumerate()
                .map(|(i, id)| NewJob {
                    kind: JOB_AI_ROUTE,
                    asset_id: *id,
                    rank: 0,
                    sort_key: i64::try_from(c * size + i).unwrap_or(i64::MAX),
                })
                .collect();
            tx.jobs().enqueue(&jobs, Priority::Recent, now, true)
        })?;
    }
    super::scan::to_front(cat, JOB_AI_ROUTE, ids)?;
    Ok(n)
}

/// Ask again (the window's button): forget what OpenSky said about each
/// photo's aircraft on its days, and what the registration lookup said
/// when it did not know the aircraft, then queue them. Returns how many
/// photos are asked about (those with a registration).
pub fn again(
    cat: &mut Catalog,
    ids: &[AssetId],
    clock: Clock,
    source: Source,
) -> Result<u64, AiError> {
    let mut ask = Vec::new();
    for id in ids {
        let p = photo(cat, *id, clock)?;
        let Some(reg) = p.registration else {
            continue;
        };
        if source != Source::Flightradar
            && lookup::kept(cat, &reg)?.is_some_and(|f| !f.known && f.typed_mode_s.is_none())
        {
            cat.write(|tx| tx.ai().forget_aircraft_lookup(&reg))?;
        }
        // Both sources' answers: either may be asked now (ADR-050).
        let mut keys = vec![reg_key(&reg)];
        keys.extend(icao24_of(cat, &reg)?.map(Key::Icao));
        if let Some(t) = p.taken_s {
            for key in &keys {
                for (day, _) in days_for(t) {
                    forget_at_key(cat, key, &day)?;
                }
            }
        }
        ask.push(*id);
    }
    enqueue(cat, &ask)?;
    Ok(u64::try_from(ask.len()).unwrap_or(u64::MAX))
}

/// How far Add route has got for `ids`.
pub fn status(
    cat: &Catalog,
    ids: &[AssetId],
) -> Result<(crate::ai::cloud::CloudStatus, Option<String>), AiError> {
    crate::ai::cloud::job_status(cat, JOB_AI_ROUTE, ids)
}

/// Stop: drop the queued jobs of `ids` (flights kept stay).
pub fn stop(cat: &mut Catalog, ids: &[AssetId]) -> Result<u64, AiError> {
    Ok(cat.write(|tx| tx.jobs().remove_queued_for(JOB_AI_ROUTE, ids))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};

    fn f(first: i64, last: i64, dep: Option<&str>, arr: Option<&str>, cs: &str) -> Flight {
        Flight {
            first_seen: first,
            last_seen: last,
            departure: dep.map(str::to_owned),
            arrival: arr.map(str::to_owned),
            callsign: Some(cs.to_owned()),
            ..Flight::default()
        }
    }

    const H: i64 = 3600;

    fn airports() -> Airports {
        Airports::builtin()
    }

    #[test]
    fn days_are_asked_one_at_a_time_or_in_runs_of_up_to_two() {
        let d = |n: i64| n * DAY_S;
        assert_eq!(MAX_WINDOW_DAYS, 1);
        assert!(windows(Vec::new()).is_empty());
        // One day per question; repeated days once.
        assert_eq!(windows(vec![d(6), d(5), d(6)]), [vec![d(5)], vec![d(6)]]);
        // Up to two: out of order and repeated, one run of two.
        assert_eq!(windows_up_to(vec![d(5)], 2), [vec![d(5)]]);
        assert_eq!(windows_up_to(vec![d(6), d(5), d(6)], 2), [vec![d(5), d(6)]]);
        // Three in a row: two and one; far apart: apart.
        assert_eq!(
            windows_up_to(vec![d(1), d(2), d(3), d(10)], 2),
            [vec![d(1), d(2)], vec![d(3)], vec![d(10)]]
        );
    }

    #[test]
    fn the_same_flight_kept_under_two_days_is_one() {
        let a = f(10, 20, Some("CYYZ"), None, "ACA1");
        let filled = Flight {
            arrival: Some("EGLL".into()),
            route_asked: true,
            by_callsign: true,
            ..a.clone()
        };
        let mut all = vec![a.clone()];
        merge_flights(
            &mut all,
            vec![filled.clone(), f(30, 40, None, None, "ACA2")],
        );
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], filled);
        merge_flights(&mut all, vec![a]);
        assert_eq!(all[0], filled);
    }

    #[test]
    fn the_airport_list_gives_iata_codes_and_countries() {
        let a = airports();
        assert!(a.len() > 5000);
        assert_eq!(a.place("CYYZ"), ("YYZ".into(), Some("Canada".into())));
        assert_eq!(
            a.place("egll"),
            ("LHR".into(), Some("United Kingdom".into()))
        );
        // Not in the list: the ICAO code is kept, no country.
        assert_eq!(a.place("ZZZZ"), ("ZZZZ".into(), None));
    }

    #[test]
    fn near_take_off_is_the_departure_near_landing_the_arrival() {
        let a = airports();
        let fl = [
            f(10 * H, 17 * H, Some("CYYZ"), Some("EGLL"), "ACA856"),
            f(19 * H, 26 * H, Some("EGLL"), Some("CYYZ"), "ACA857"),
        ];
        // 20 minutes before take-off.
        match pick(10 * H - 1200, None, &fl, &a) {
            Pick::One(c) => {
                assert_eq!(c.at, At::Departure);
                assert_eq!(c.flight.callsign.as_deref(), Some("ACA856"));
                assert_eq!(c.gap_s, 1200);
            }
            p => panic!("{p:?}"),
        }
        // On approach, 10 minutes before landing (in the air).
        match pick(17 * H - 600, None, &fl, &a) {
            Pick::One(c) => assert_eq!((c.at, c.gap_s), (At::Arrival, 0)),
            p => panic!("{p:?}"),
        }
        // Nearest wins: 30 min after landing beats 90 min before the next.
        match pick(17 * H + 1800, None, &fl, &a) {
            Pick::One(c) => assert_eq!(c.flight.callsign.as_deref(), Some("ACA856")),
            p => panic!("{p:?}"),
        }
    }

    #[test]
    fn nothing_near_nothing_known_and_two_equally_close() {
        let a = airports();
        let fl = [
            f(10 * H, 11 * H, Some("CYYZ"), Some("CYOW"), "ACA443"),
            f(12 * H, 13 * H, Some("CYYZ"), Some("CYUL"), "ACA412"),
        ];
        assert_eq!(pick(20 * H, None, &fl, &a), Pick::NoneNear);
        assert_eq!(pick(20 * H, None, &[], &a), Pick::Nothing);
        // Between them, 30 minutes from each: both, the user chooses.
        match pick(11 * H + 1800, None, &fl, &a) {
            Pick::Several(v) => assert_eq!(v.len(), 2),
            p => panic!("{p:?}"),
        }
        // One lands at Toronto as the other leaves it: the photo's position
        // does not help.
        let fl = [
            f(10 * H, 11 * H, Some("CYOW"), Some("CYYZ"), "ACA444"),
            f(12 * H, 13 * H, Some("CYYZ"), Some("CYUL"), "ACA412"),
        ];
        match pick(11 * H + 1800, Some((43.68, -79.63)), &fl, &a) {
            Pick::Several(v) => assert_eq!(v.len(), 2),
            p => panic!("{p:?}"),
        }
    }

    #[test]
    fn a_flight_the_source_did_not_see_is_worked_out_and_offered() {
        let a = airports();
        // SP-LSC: WAW → YYZ landed 21:22; next seen leaving WAW at 10:52
        // the next day (for Miami). The photo: YYZ, 00:00, 158 min after
        // the landing, as it left for Warsaw.
        let fl = [
            f(
                13 * H,
                21 * H + 22 * 60,
                Some("EPWA"),
                Some("CYYZ"),
                "LOT8N",
            ),
            f(
                34 * H + 52 * 60,
                45 * H,
                Some("EPWA"),
                Some("KMIA"),
                "LOT29",
            ),
        ];
        let photo = 24 * H;
        let leg = missing_leg(photo, &fl, &a).unwrap();
        assert!(leg.inferred);
        assert_eq!(leg.departure.as_deref(), Some("CYYZ"));
        assert_eq!(leg.arrival.as_deref(), Some("EPWA"));
        assert_eq!(
            (leg.first_seen, leg.last_seen),
            (fl[0].last_seen, fl[1].first_seen)
        );
        let k: Vec<String> = keywords(&leg, &a)
            .unwrap()
            .iter()
            .map(TagPath::to_lightroom)
            .collect();
        assert_eq!(
            k,
            [
                "Aircraft|From|YYZ",
                "Aircraft|From country|Canada",
                "Aircraft|To|WAW",
                "Aircraft|To country|Poland"
            ]
        );
        // Offered beside the arrival (158 min after it): the user chooses.
        match pick(photo, None, &fl, &a) {
            Pick::Several(v) => {
                assert_eq!(v.len(), 2);
                assert_eq!(v[0].flight.callsign.as_deref(), Some("LOT8N"));
                assert!(v[1].flight.inferred);
            }
            p => panic!("{p:?}"),
        }
        // Long after the landing (nothing near): the missing flight alone.
        match pick(28 * H, None, &fl, &a) {
            Pick::One(c) => assert!(c.flight.inferred),
            p => panic!("{p:?}"),
        }
        // Next leaving from where it landed: nothing missing.
        let fl = [
            f(13 * H, 21 * H, Some("EPWA"), Some("CYYZ"), "LOT8N"),
            f(23 * H, 31 * H, Some("CYYZ"), Some("EPWA"), "LOT45"),
        ];
        assert_eq!(missing_leg(22 * H, &fl, &a), None);
        // No time for it (lands at Ottawa, leaves Toronto an hour later).
        let fl = [
            f(10 * H, 11 * H, Some("CYYZ"), Some("CYOW"), "ACA443"),
            f(12 * H, 13 * H, Some("CYYZ"), Some("CYUL"), "ACA412"),
        ];
        assert_eq!(missing_leg(11 * H + 1800, &fl, &a), None);
        // Not between the two: nothing worked out.
        let fl = [
            f(13 * H, 21 * H, Some("EPWA"), Some("CYYZ"), "LOT8N"),
            f(34 * H, 45 * H, Some("EPWA"), Some("KMIA"), "LOT29"),
        ];
        assert_eq!(missing_leg(20 * H, &fl, &a), None);
        // An answer kept before this (no `inferred`) still reads.
        let old: Flight = serde_json::from_str(
            r#"{"first_seen":1,"last_seen":2,"departure":null,"arrival":null,"callsign":null}"#,
        )
        .unwrap();
        assert!(!old.inferred);
    }

    #[test]
    fn the_flight_in_or_out_the_source_did_not_see_is_worked_out_and_offered() {
        let a = airports();
        // A7402578.ARW, CN-RGY: the photo at 00:00 UTC as it landed from
        // Casablanca; OpenSky saw only RAM211B leave YYZ for CMN at 02:30.
        let fl = [f(
            2 * H + 1800,
            9 * H,
            Some("CYYZ"),
            Some("GMMN"),
            "RAM211B",
        )];
        let photo = 0;
        let leg = missing_end(photo, &fl).unwrap();
        assert!(leg.inferred);
        assert_eq!(leg.half, Some(At::Arrival));
        assert_eq!(leg.departure.as_deref(), Some("GMMN"));
        assert_eq!(leg.arrival.as_deref(), Some("CYYZ"));
        assert_eq!(
            (leg.first_seen, leg.last_seen),
            (fl[0].first_seen, fl[0].first_seen)
        );
        let k: Vec<String> = keywords(&leg, &a)
            .unwrap()
            .iter()
            .map(TagPath::to_lightroom)
            .collect();
        assert_eq!(
            k,
            [
                "Aircraft|From|CMN",
                "Aircraft|From country|Morocco",
                "Aircraft|To|YYZ",
                "Aircraft|To country|Canada"
            ]
        );
        // Offered first, beside the take-off 150 min after: the user chooses.
        match pick(photo, None, &fl, &a) {
            Pick::Several(v) => {
                assert_eq!(v.len(), 2);
                assert!(v[0].flight.inferred);
                assert_eq!(v[0].at, At::Arrival);
                assert_eq!(v[1].flight.callsign.as_deref(), Some("RAM211B"));
            }
            p => panic!("{p:?}"),
        }
        // Where the take-off went not known: from where, not known.
        let unknown = [f(2 * H + 1800, 9 * H, Some("CYYZ"), None, "RAM211B")];
        let leg = missing_end(photo, &unknown).unwrap();
        assert_eq!(leg.departure, None);
        assert_eq!(leg.arrival.as_deref(), Some("CYYZ"));
        // Too soon before the take-off (taxiing out): only the take-off.
        assert_eq!(missing_end(2 * H, &fl), None);
        match pick(2 * H, None, &fl, &a) {
            Pick::One(c) => assert_eq!(c.flight.callsign.as_deref(), Some("RAM211B")),
            p => panic!("{p:?}"),
        }
        // Too long before it: nothing.
        assert_eq!(missing_end(-5 * H, &fl), None);
        // Something seen before the photo: not the flight in.
        let both = [
            f(-9 * H, -H, Some("GMMN"), Some("CYYZ"), "RAM210"),
            fl[0].clone(),
        ];
        assert_eq!(missing_end(photo, &both), None);

        // The flight out, the same way round: landed at YYZ from CMN at
        // 22:00, the photo at 00:30, nothing seen after.
        let fl = [f(-10 * H, -2 * H, Some("GMMN"), Some("CYYZ"), "RAM210")];
        let leg = missing_end(H / 2, &fl).unwrap();
        assert_eq!(leg.half, Some(At::Departure));
        assert_eq!(leg.departure.as_deref(), Some("CYYZ"));
        assert_eq!(leg.arrival.as_deref(), Some("GMMN"));
        assert_eq!((leg.first_seen, leg.last_seen), (-2 * H, -2 * H));
        match pick(H / 2, None, &fl, &a) {
            Pick::Several(v) => {
                assert_eq!(v[0].flight.callsign.as_deref(), Some("RAM210"));
                assert_eq!(v[1].at, At::Departure);
                assert!(v[1].flight.inferred);
            }
            p => panic!("{p:?}"),
        }
        // Back where it came from: not said.
        let round = [f(-10 * H, -2 * H, Some("CYYZ"), Some("CYYZ"), "ACA1")];
        assert_eq!(missing_end(H / 2, &round).unwrap().arrival, None);
    }

    /// Callsign routes as adsbdb gives them, counting what is asked.
    struct Routes(Mutex<Vec<String>>);

    impl Lookup for Routes {
        fn lookup(&self, _: &str) -> Result<lookup::Found, AiError> {
            Ok(lookup::Found::default())
        }
        fn callsign_route(&self, cs: &str) -> Result<Option<(String, String)>, AiError> {
            self.0.lock().unwrap().push(cs.to_owned());
            match cs {
                "RAM211B" => Ok(Some(("CYYZ".into(), "GMMN".into()))),
                "RAM210" => Ok(Some(("GMMN".into(), "CYYZ".into()))),
                "DOWN1" => Err(AiError::Cloud {
                    message: "could not reach adsbdb".into(),
                    retry: true,
                }),
                _ => Ok(None),
            }
        }
    }

    #[test]
    fn the_end_the_source_did_not_see_is_filled_from_the_callsign_route() {
        let routes = Routes(Mutex::new(Vec::new()));
        let mut fl = vec![
            // Left YYZ, not seen landing: CMN, from the route.
            f(10 * H, 12 * H, Some("CYYZ"), None, "RAM211B"),
            // Seen landing at YYZ, not leaving: CMN.
            f(H, 9 * H, None, Some("CYYZ"), "RAM210"),
            // Flown from elsewhere that day: the route says nothing.
            f(20 * H, 21 * H, Some("KJFK"), None, "RAM211B"),
            // Both ends seen: not asked.
            f(30 * H, 31 * H, Some("CYYZ"), Some("CYUL"), "ACA412"),
            // Not known to adsbdb: asked, left.
            f(40 * H, 41 * H, Some("CYYZ"), None, "XYZ1"),
            // adsbdb not reached: left for next time.
            f(50 * H, 51 * H, Some("CYYZ"), None, "DOWN1"),
        ];
        let mut cache = HashMap::new();
        let (n, changed) = fill_ends(&mut fl, &routes, &mut cache);
        assert!(changed);
        // RAM211B once for both flights.
        assert_eq!(n, 4);
        assert_eq!(
            *routes.0.lock().unwrap(),
            ["RAM211B", "RAM210", "XYZ1", "DOWN1"]
        );
        assert_eq!(fl[0].arrival.as_deref(), Some("GMMN"));
        assert!(fl[0].by_callsign && fl[0].route_asked);
        assert_eq!(fl[1].departure.as_deref(), Some("GMMN"));
        assert!(fl[1].by_callsign);
        assert_eq!(fl[2].arrival, None);
        assert!(!fl[2].by_callsign && fl[2].route_asked);
        assert!(!fl[3].route_asked);
        assert!(fl[4].route_asked && fl[4].arrival.is_none());
        assert!(!fl[5].route_asked);
        // Kept and read back; asked again: only the one not reached.
        let json = serde_json::to_string(&fl).unwrap();
        let mut back: Vec<Flight> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, fl);
        let (n, _) = fill_ends(&mut back, &routes, &mut HashMap::new());
        assert_eq!(n, 1);
    }

    #[test]
    fn the_photo_position_breaks_a_tie() {
        let a = airports();
        // One lands in Ottawa as the other leaves Toronto, 30 min from the photo.
        let fl = [
            f(10 * H, 11 * H, Some("CYYZ"), Some("CYOW"), "ACA443"),
            f(12 * H, 13 * H, Some("CYYZ"), Some("CYUL"), "ACA412"),
        ];
        match pick(11 * H + 1800, Some((45.32, -75.67)), &fl, &a) {
            Pick::One(c) => assert_eq!(c.flight.callsign.as_deref(), Some("ACA443")),
            p => panic!("{p:?}"),
        }
    }

    #[test]
    fn the_keywords_say_from_and_to_with_countries_and_unknown_ends_are_left_out() {
        let a = airports();
        let fl = f(0, H, Some("CYYZ"), Some("EGLL"), "ACA856");
        let k: Vec<String> = keywords(&fl, &a)
            .unwrap()
            .iter()
            .map(TagPath::to_lightroom)
            .collect();
        assert_eq!(
            k,
            [
                "Aircraft|From|YYZ",
                "Aircraft|From country|Canada",
                "Aircraft|To|LHR",
                "Aircraft|To country|United Kingdom",
                "Aircraft|Flight|AC856"
            ]
        );
        let fl = f(0, H, None, Some("ZZZZ"), "ROU2000");
        let k: Vec<String> = keywords(&fl, &a)
            .unwrap()
            .iter()
            .map(TagPath::to_lightroom)
            .collect();
        assert_eq!(k, ["Aircraft|To|ZZZZ", "Aircraft|Flight|RV2000"]);
        assert!(k
            .iter()
            .all(|s| is_route_keyword(&TagPath::parse_lightroom(s).unwrap())));
        assert!(!is_route_keyword(
            &TagPath::parse_lightroom("Aircraft|Type|X").unwrap()
        ));
    }

    #[test]
    fn an_aircraft_is_read_from_flightradar24_and_airline_codes_have_names() {
        let body = r#"{"data":[{"fr24_id":"1","flight":"KL692","callsign":"KLM692","operated_as":"KLM","painted_as":"KLM","type":"B78X","reg":"PH-BKO","hex":"485e31","datetime_takeoff":"2026-11-17T21:20:00","first_seen":"2026-11-17T21:05:00"}]}"#;
        let a = parse_fr24_aircraft(200, body, 7).unwrap().unwrap();
        assert_eq!(a.asked_ms, 7);
        assert!(a.found);
        assert_eq!(a.icao_type.as_deref(), Some("B78X"));
        assert_eq!(a.operator.as_deref(), Some("KLM"));
        assert_eq!(a.hex.as_deref(), Some("485E31"));
        assert_eq!(a.seen.as_deref(), Some("2026-11-17"));
        assert_eq!(parse_fr24_aircraft(200, r#"{"data":[]}"#, 7).unwrap(), None);
        assert!(parse_fr24_aircraft(401, "", 7).is_err());
        assert_eq!(airline_of_code("aca"), Some("Air Canada"));
        assert_eq!(airline_of_code("KLM"), Some("KLM"));
        assert_eq!(airline_of_code("ZZZ"), None);
    }

    #[test]
    fn a_callsign_becomes_the_flight_number_people_know() {
        assert_eq!(flight_number("ACA856"), "AC856");
        assert_eq!(flight_number("dlh470 "), "LH470");
        assert_eq!(flight_number("ROU2000"), "RV2000");
        assert_eq!(flight_number("WJA0012"), "WS12");
        // Not airline and number, or an airline not listed: as it is.
        assert_eq!(flight_number("BAW12AB"), "BAW12AB");
        assert_eq!(flight_number("JZA8011"), "QK8011");
        assert_eq!(flight_number("XQZ8011"), "XQZ8011");
        assert_eq!(flight_number("CGBIJ"), "CGBIJ");
        assert_eq!(flight_number("ACA"), "ACA");
    }

    #[test]
    fn the_days_looked_at_are_those_within_three_hours() {
        // 2026-05-31 12:31 UTC: that day only.
        let t = 1_780_230_671;
        let d = days_for(t);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].0, "2026-05-31");
        assert_eq!(d[0].1 % DAY_S, 0);
        // 01:00: the day before too.
        let d = days_for(d[0].1 + 3600);
        assert_eq!(
            d.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
            ["2026-05-30", "2026-05-31"]
        );
    }

    #[test]
    fn eastern_time_has_daylight_saving_by_the_date() {
        let wall = |y, m, d, h, mi| {
            chrono::NaiveDate::from_ymd_opt(y, m, d)
                .unwrap()
                .and_hms_opt(h, mi, 0)
                .unwrap()
                .and_utc()
                .timestamp()
        };
        assert_eq!(eastern_offset(wall(2025, 4, 1, 15, 56)), -240);
        assert_eq!(eastern_offset(wall(2025, 1, 15, 12, 0)), -300);
        // 2025: 9 March to 2 November, at 02:00.
        assert_eq!(eastern_offset(wall(2025, 3, 9, 1, 59)), -300);
        assert_eq!(eastern_offset(wall(2025, 3, 9, 2, 0)), -240);
        assert_eq!(eastern_offset(wall(2025, 11, 2, 1, 59)), -240);
        assert_eq!(eastern_offset(wall(2025, 11, 2, 2, 0)), -300);
        // Before 2007: April to October.
        assert_eq!(eastern_offset(wall(2006, 3, 20, 12, 0)), -300);
        assert_eq!(eastern_offset(wall(2006, 4, 2, 3, 0)), -240);
        assert_eq!(eastern_offset(wall(2006, 10, 29, 3, 0)), -300);

        // A7_05212.ARW: 15:56 on 1 April 2025, marked UTC−05:00.
        let utc_ms = 1_743_540_978_000;
        assert_eq!(
            read_clock(utc_ms, Some(-300), Clock::AsRecorded, false),
            (1_743_540_978, Some(-300))
        );
        assert_eq!(
            read_clock(utc_ms, Some(-300), Clock::Eastern, false),
            (1_743_540_978 - 3600, Some(-240))
        );
        // Another time zone is kept; none at all is read as Eastern.
        assert_eq!(
            read_clock(utc_ms, Some(60), Clock::Eastern, false),
            (1_743_540_978, Some(60))
        );
        let w = wall(2025, 4, 1, 15, 56);
        assert_eq!(
            read_clock(w * 1000, None, Clock::Eastern, false),
            (w + 4 * 3600, Some(-240))
        );
        assert_eq!(
            read_clock(w * 1000, None, Clock::AsRecorded, false),
            (w, None)
        );
        // A time zone the user set is kept.
        assert_eq!(
            read_clock(utc_ms, Some(-300), Clock::Eastern, true),
            (1_743_540_978, Some(-300))
        );
    }

    #[test]
    fn flightradar24_answers_are_read_and_trouble_is_said_clearly() {
        // Flight summary light (docs example), and a full one's names.
        let body = r#"{"data":[{"fr24_id":"1","flight":"KL692","callsign":"KLM692","reg":"PH-BKO","origin_icao":"CYYZ","datetime_takeoff":"2025-04-01T21:20:00","destination_icao":"EHAM","destination_icao_actual":null,"datetime_landed":"2025-04-02T05:55:00","hex":"485E31","first_seen":"2025-04-01T21:05:00","last_seen":"2025-04-02T06:02:00","flight_ended":"true"},
          {"fr24_id":"2","flight":null,"callsign":"KLM691","orig_icao":"EHAM","datetime_takeoff":null,"dest_icao":"CYYZ","dest_icao_actual":"CYOW","datetime_landed":null,"first_seen":"2025-04-01T11:00:00","last_seen":"2025-04-01T19:10:00"},
          {"fr24_id":"3","callsign":"X","first_seen":null,"last_seen":null}]}"#;
        let v = parse_fr24(200, body).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].departure.as_deref(), Some("CYYZ"));
        assert_eq!(v[0].arrival.as_deref(), Some("EHAM"));
        // Take-off and landing, not first and last seen.
        assert_eq!(
            v[0].first_seen,
            fr24_parse_time("2025-04-01T21:20:00").unwrap()
        );
        assert_eq!(v[0].shown_number().as_deref(), Some("KL692"));
        // Diverted: where it landed; no take-off time: first seen.
        assert_eq!(v[1].arrival.as_deref(), Some("CYOW"));
        assert_eq!(
            v[1].first_seen,
            fr24_parse_time("2025-04-01 11:00:00").unwrap()
        );
        assert_eq!(v[1].shown_number().as_deref(), Some("KL691"));
        assert_eq!(parse_fr24(404, "").unwrap(), []);
        assert_eq!(parse_fr24(200, r#"{"data":[]}"#).unwrap(), []);
        for (status, retry, says) in [
            (401, false, "API token"),
            (402, false, "credits"),
            (403, false, "Flight summary"),
            (429, true, "slow down"),
            (400, false, "bad date"),
            (402, false, "credits"),
            (503, true, "503"),
        ] {
            match parse_fr24(status, r#"{"message":"bad date"}"#) {
                Err(AiError::Cloud { retry: r, message }) => {
                    assert_eq!(r, retry, "{status}");
                    assert!(message.contains(says), "{status}: {message}");
                }
                x => panic!("{status}: {x:?}"),
            }
        }
    }

    /// A request: its URL and headers.
    type Request = (String, Vec<(String, String)>);

    /// Flightradar24 as the tests see it: keeps the request.
    struct Fr24Fake {
        gets: Mutex<Vec<Request>>,
    }

    impl Http for Fr24Fake {
        fn post(&self, _: &str, _: &[(&str, &str)], _: &[u8]) -> Result<(u16, String), String> {
            Err("no posts".into())
        }
        fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<(u16, String), String> {
            self.gets.lock().unwrap().push((
                url.to_owned(),
                headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
            ));
            Ok((200, r#"{"data":[]}"#.into()))
        }
    }

    #[test]
    fn flightradar24_says_when_the_plan_does_not_go_back_far_enough() {
        // As Flightradar24 answered the owner's Explorer plan.
        let body = r#"{"message":"Validation failed","details":"The requested date '2025-04-01T00:00:00' is earlier than your subscription plan allows. Your plan provides data from '2026-10-20 13:51:39' onward."}"#;
        match parse_fr24(400, body) {
            Err(AiError::Cloud {
                retry: false,
                message,
            }) => {
                assert!(message.contains("from 2026-10-20 on"), "{message}");
                assert!(message.contains("this photo is older"), "{message}");
            }
            x => panic!("{x:?}"),
        }
    }

    /// Flightradar24 refusing a date before the plan, as it answered.
    struct PlanRefuses;

    impl Http for PlanRefuses {
        fn post(&self, _: &str, _: &[(&str, &str)], _: &[u8]) -> Result<(u16, String), String> {
            Err("no posts".into())
        }
        fn get(&self, _: &str, _: &[(&str, &str)]) -> Result<(u16, String), String> {
            Ok((400, r#"{"message":"Validation failed","details":"The requested date '2025-04-01T00:00:00' is earlier than your subscription plan allows. Your plan provides data from '2026-10-20 13:51:39' onward."}"#.into()))
        }
    }

    #[test]
    fn flightradar24_s_refusal_says_from_when_the_plan_gives_flights_once() {
        let at = chrono::NaiveDate::from_ymd_opt(2026, 10, 20)
            .and_then(|d| d.and_hms_opt(13, 51, 39))
            .map(|d| d.and_utc().timestamp());
        assert_eq!(
            plan_from("… Your plan provides data from '2026-10-20 13:51:39' onward."),
            at
        );
        assert_eq!(plan_from("Validation failed"), None);
        let fr = Flightradar::new(Arc::new(PlanRefuses), Arc::new(|| Some("tok".into())));
        assert_eq!(fr.take_history_from(), None);
        assert!(fr.flights("PH-BKO", 0, 1).is_err());
        assert_eq!(fr.take_history_from(), at);
        // Said once: a later look does not see an old date.
        assert_eq!(fr.take_history_from(), None);
    }

    #[test]
    fn the_source_is_chosen_by_the_photo_s_date_and_what_is_set_up() {
        let now = 100 * DAY_S;
        let c = |mode, opensky, fr24| Choose {
            mode,
            fr24_days: 30,
            opensky,
            fr24,
            now_s: now,
        };
        let auto = c(Mode::Auto, true, true);
        // Within the plan (with a day to spare): Flightradar24.
        assert_eq!(auto.source(now - 3600), Ok(Source::Flightradar));
        assert_eq!(auto.source(now - 28 * DAY_S), Ok(Source::Flightradar));
        // Near or past its start: OpenSky.
        assert_eq!(auto.source(now - 29 * DAY_S - 3600), Ok(Source::OpenSky));
        assert_eq!(auto.source(now - 400 * DAY_S), Ok(Source::OpenSky));
        // Only one set up: that one where it can.
        assert_eq!(
            c(Mode::Auto, false, true).source(now - 3600),
            Ok(Source::Flightradar)
        );
        assert_eq!(
            c(Mode::Auto, true, false).source(now - 3600),
            Ok(Source::OpenSky)
        );
        assert!(c(Mode::Auto, false, true)
            .source(now - 60 * DAY_S)
            .unwrap_err()
            .contains("older than your Flightradar24 plan's 30 days"));
        assert!(c(Mode::Auto, false, false).source(now).is_err());
        // Forced: that one, whatever the date; none without it.
        let fr = c(Mode::Only(Source::Flightradar), true, true);
        assert_eq!(fr.source(now - 400 * DAY_S), Ok(Source::Flightradar));
        assert!(c(Mode::Only(Source::OpenSky), false, true)
            .source(now)
            .is_err());
        assert_eq!(Mode::parse("auto"), Mode::Auto);
        assert_eq!(Mode::parse(""), Mode::Auto);
        assert_eq!(Mode::parse("opensky"), Mode::Only(Source::OpenSky));
        assert_eq!(Mode::parse("flightradar").id(), "flightradar");
    }

    #[test]
    fn flightradar24_is_asked_by_registration_for_the_utc_day_with_the_token() {
        let http = Arc::new(Fr24Fake {
            gets: Mutex::new(Vec::new()),
        });
        let fr = Flightradar::new(http.clone(), Arc::new(|| Some("tok".into())));
        assert_eq!(fr.source(), Source::Flightradar);
        // 2025-04-01, the whole UTC day.
        fr.flights(" ph-bko", 1_743_465_600, 1_743_465_600 + DAY_S - 1)
            .unwrap();
        let gets = http.gets.lock().unwrap();
        assert_eq!(
            gets[0].0,
            format!("{FR24_API}?flight_datetime_from=2025-04-01T00%3A00%3A00&flight_datetime_to=2025-04-01T23%3A59%3A59&registrations=PH-BKO&limit=100")
        );
        let h: HashMap<_, _> = gets[0].1.iter().cloned().collect();
        assert_eq!(h["authorization"], "Bearer tok");
        assert_eq!(h["accept-version"], "v1");
        drop(gets);
        // No token: nothing is sent.
        let fr = Flightradar::new(http.clone(), Arc::new(|| None));
        assert!(fr.flights("PH-BKO", 0, 1).is_err());
        assert_eq!(http.gets.lock().unwrap().len(), 1);
        assert_eq!(Source::parse("flightradar"), Source::Flightradar);
        assert_eq!(Source::parse("anything"), Source::OpenSky);
    }

    #[test]
    fn opensky_answers_are_read() {
        let body = r#"[{"icao24":"c04826","firstSeen":1780229760,"estDepartureAirport":"CYYZ","lastSeen":1780235100,"estArrivalAirport":null,"callsign":"ROU2000 "}]"#;
        let v = parse_flights(200, body).unwrap();
        assert_eq!(
            v,
            [f(1780229760, 1780235100, Some("CYYZ"), None, "ROU2000")]
        );
        assert_eq!(parse_flights(404, "[]").unwrap(), []);
        assert!(matches!(
            parse_flights(429, ""),
            Err(AiError::Cloud { retry: true, message }) if message == LIMIT_REACHED
        ));
        assert!(matches!(
            parse_flights(403, ""),
            Err(AiError::Cloud { retry: false, .. })
        ));
        assert!(matches!(
            parse_flights(503, ""),
            Err(AiError::Cloud { retry: true, .. })
        ));
        assert_eq!(form(&[("a b", "x&y=z")]), "a%20b=x%26y%3Dz");
    }

    /// OpenSky as the tests see it: counts the tokens asked for and answers
    /// with `flight_status` (after a 401 when `expire_once`).
    struct Fake {
        tokens: AtomicI64,
        gets: Mutex<Vec<(String, String)>>,
        token_status: u16,
        flight_status: Mutex<Vec<u16>>,
    }

    impl Http for Fake {
        fn post(
            &self,
            url: &str,
            _: &[(&str, &str)],
            body: &[u8],
        ) -> Result<(u16, String), String> {
            assert_eq!(url, TOKEN_URL);
            assert!(String::from_utf8_lossy(body).contains("client_secret=s%26cret"));
            let n = self.tokens.fetch_add(1, Ordering::SeqCst) + 1;
            Ok((
                self.token_status,
                format!(r#"{{"access_token":"tok{n}","expires_in":1800}}"#),
            ))
        }
        fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<(u16, String), String> {
            let auth = headers
                .iter()
                .find(|(k, _)| *k == "authorization")
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();
            self.gets.lock().unwrap().push((url.to_owned(), auth));
            let s = {
                let mut v = self.flight_status.lock().unwrap();
                if v.len() > 1 {
                    v.remove(0)
                } else {
                    v[0]
                }
            };
            Ok((s, "[]".into()))
        }
        fn get_wait(
            &self,
            url: &str,
            headers: &[(&str, &str)],
        ) -> Result<(u16, String, Option<i64>), String> {
            // What OpenSky said on 2026-11-20 at its limit.
            self.get(url, headers)
                .map(|(s, b)| (s, b, (s == 429).then_some(61_899)))
        }
    }

    fn fake(token_status: u16, flight_status: &[u16]) -> Arc<Fake> {
        Arc::new(Fake {
            tokens: AtomicI64::new(0),
            gets: Mutex::new(Vec::new()),
            token_status,
            flight_status: Mutex::new(flight_status.to_vec()),
        })
    }

    fn creds() -> Credentials {
        Arc::new(|| Some(("me-api-client".into(), "s&cret".into())))
    }

    #[test]
    fn the_token_is_fetched_once_and_again_after_it_runs_out() {
        let http = fake(200, &[200]);
        let now = Arc::new(AtomicI64::new(1_000_000));
        let n2 = now.clone();
        let sky = OpenSky::new(http.clone(), creds())
            .with_clock(Arc::new(move || n2.load(Ordering::SeqCst)));
        sky.flights("C04826", 0, 86_399).unwrap();
        sky.flights("c04826", 86_400, 172_799).unwrap();
        assert_eq!(http.tokens.load(Ordering::SeqCst), 1);
        {
            let gets = http.gets.lock().unwrap();
            assert_eq!(
                gets[0].0,
                format!("{API_BASE}?icao24=c04826&begin=0&end=86399")
            );
            assert_eq!(gets[0].1, "Bearer tok1");
        }
        // 30 minutes on: a new token.
        now.fetch_add(1_800_000, Ordering::SeqCst);
        sky.flights("c04826", 0, 86_399).unwrap();
        assert_eq!(http.tokens.load(Ordering::SeqCst), 2);
        assert_eq!(http.gets.lock().unwrap()[2].1, "Bearer tok2");
    }

    #[test]
    fn a_401_asks_for_a_new_token_once_and_trouble_is_said_clearly() {
        // Refused once (the token ran out early), then fine.
        let http = fake(200, &[401, 200]);
        let sky = OpenSky::new(http.clone(), creds());
        sky.flights("c04826", 0, 1).unwrap();
        assert_eq!(http.tokens.load(Ordering::SeqCst), 2);
        // Always refused: the credentials.
        let sky = OpenSky::new(fake(200, &[401]), creds());
        assert!(matches!(
            sky.flights("c04826", 0, 1),
            Err(AiError::Cloud { retry: false, message }) if message.contains("client ID and secret")
        ));
        // The token itself refused.
        let sky = OpenSky::new(fake(401, &[200]), creds());
        assert!(matches!(
            sky.test(),
            Err(AiError::Cloud { retry: false, .. })
        ));
        // The daily limit: until when OpenSky says; nothing asked before.
        let http = fake(200, &[429]);
        let sky = OpenSky::new(http.clone(), creds()).with_clock(Arc::new(|| 1_000_000));
        match sky.flights("c04826", 0, 1) {
            Err(AiError::Later { message, until_ms }) => {
                assert_eq!(until_ms, 1_000_000 + 61_899_000);
                assert!(message.contains("1970-01-01 17:28 UTC"), "{message}");
            }
            r => panic!("{r:?}"),
        }
        assert!(matches!(
            sky.flights("c04826", 0, 1),
            Err(AiError::Later {
                until_ms: 62_899_000,
                ..
            })
        ));
        assert_eq!(http.gets.lock().unwrap().len(), 1);
        // No credentials.
        let sky = OpenSky::new(fake(200, &[200]), Arc::new(|| None));
        assert!(matches!(
            sky.flights("c04826", 0, 1),
            Err(AiError::Cloud { retry: false, .. })
        ));
        // Not a transponder code: nothing is sent.
        let http = fake(200, &[200]);
        let sky = OpenSky::new(http.clone(), creds());
        assert!(sky.flights("C-GBIJ", 0, 1).is_err());
        assert!(http.gets.lock().unwrap().is_empty());
    }
}

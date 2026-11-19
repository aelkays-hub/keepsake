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
//! - OpenSky's history goes back to about 2023 (checked on the owner's
//!   photos: September 2023 found, August 2022 and 2018 not), and only where
//!   its receivers are; an airport it did not see is "not known".
//! - Airports: `airports.tsv` (OurAirports, The Unlicense), the airports
//!   with an IATA code: ICAO → IATA and country.
//! - Nothing becomes a keyword until the user accepts it.

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
        .map_or(cs.clone(), |(_, iata)| format!("{}{digits}", iata.trim()))
}

/// One flight, as OpenSky saw it (times in Unix seconds).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flight {
    pub first_seen: i64,
    pub last_seen: i64,
    /// ICAO airport codes (`CYYZ`), when OpenSky worked them out.
    pub departure: Option<String>,
    pub arrival: Option<String>,
    /// Trimmed (`ACA856`).
    pub callsign: Option<String>,
}

/// Gives an aircraft's flights between two times (Unix seconds).
pub trait Flights: Send + Sync {
    fn flights(&self, icao24: &str, begin: i64, end: i64) -> Result<Vec<Flight>, AiError>;
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
        for attempt in 0..2 {
            let tok = self.token()?;
            let auth = format!("Bearer {tok}");
            let (status, body) = self
                .http
                .get(
                    &url,
                    &[("authorization", &auth), ("accept", "application/json")],
                )
                .map_err(|e| cloud(format!("could not reach {SERVICE}: {e}"), true))?;
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

/// The flight the photo (taken at `photo_s`, Unix seconds, at `gps`)
/// shows: the one that took off or landed nearest it, within [`NEAR_S`]
/// (in the air at the time counts as nearest). Flights within [`TIE_S`] of
/// each other are equally close: the photo's position, when known, picks
/// the one whose airport is nearest; otherwise all are offered. Never a
/// silent guess.
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
    if let Some(cs) = &f.callsign {
        add("Flight", &flight_number(cs))?;
    }
    Ok(out)
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

/// The transponder code of `registration`, as the lookup keeps it (the
/// one the user typed first).
pub fn icao24_of(cat: &Catalog, registration: &str) -> Result<Option<String>, AiError> {
    Ok(lookup::kept(cat, registration)?
        .and_then(|f| f.typed_mode_s.or(f.mode_s))
        .map(|m| m.to_ascii_lowercase()))
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
}

/// One `ai.route` job: make sure the flights of `asset`'s aircraft on the
/// days around it are kept, asking OpenSky for those not kept yet (and
/// the registration's transponder code first, if unknown and `lookup` is
/// given). Choosing the flight is left to the window ([`review`]).
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
    for (day, start) in days_for(t) {
        if kept_day(cat, &icao, &day)?.is_some() {
            continue;
        }
        let found = flights.flights(&icao, start, start + DAY_S - 1)?;
        asked.requests += 1;
        let json = serde_json::to_string(&found)
            .map_err(|e| AiError::Image(format!("the flights could not be kept: {e}")))?;
        let now = crate::jobs::now_ms();
        cat.write(|tx| tx.ai().put_aircraft_flights(&icao, &day, &json, now))?;
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
}

/// Where `asset`'s route stands: worked out from what is kept (nothing is
/// asked here).
pub fn review(
    cat: &Catalog,
    airports: &Airports,
    asset: AssetId,
    clock: Clock,
) -> Result<RouteItem, AiError> {
    let p = photo(cat, asset, clock)?;
    let has = cat
        .read(|tx| tx.tags().for_asset(asset))?
        .into_iter()
        .filter(|t| t.source == TagSource::User && is_route_keyword(&t.path))
        .map(|t| t.path)
        .collect();
    let item = |state| RouteItem {
        asset,
        registration: p.registration.clone(),
        taken_s: p.taken_s,
        offset_min: p.offset_min,
        state,
        has,
    };
    let Some(reg) = p.registration.as_deref() else {
        return Ok(item(State::NoRegistration));
    };
    let Some(t) = p.taken_s else {
        return Ok(item(State::NoCaptureTime));
    };
    let Some(icao) = icao24_of(cat, reg)? else {
        let waiting = cat
            .read(|tx| tx.jobs().find(JOB_AI_ROUTE, asset))?
            .is_some_and(|j| matches!(j.state, JobState::Queued | JobState::Running));
        return Ok(item(if waiting {
            State::Waiting
        } else {
            State::NoTransponder
        }));
    };
    let mut all: Vec<Flight> = Vec::new();
    for (day, _) in days_for(t) {
        match kept_day(cat, &icao, &day)? {
            Some(v) => {
                for f in v {
                    if !all.contains(&f) {
                        all.push(f);
                    }
                }
            }
            None => return Ok(item(State::Waiting)),
        }
    }
    Ok(item(State::Found(pick(t, p.gps, &all, airports))))
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
pub fn again(cat: &mut Catalog, ids: &[AssetId], clock: Clock) -> Result<u64, AiError> {
    let mut ask = Vec::new();
    for id in ids {
        let p = photo(cat, *id, clock)?;
        let Some(reg) = p.registration else {
            continue;
        };
        if lookup::kept(cat, &reg)?.is_some_and(|f| !f.known && f.typed_mode_s.is_none()) {
            cat.write(|tx| tx.ai().forget_aircraft_lookup(&reg))?;
        }
        if let (Some(icao), Some(t)) = (icao24_of(cat, &reg)?, p.taken_s) {
            for (day, _) in days_for(t) {
                cat.write(|tx| tx.ai().forget_aircraft_flights(&icao, &day))?;
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
        }
    }

    const H: i64 = 3600;

    fn airports() -> Airports {
        Airports::builtin()
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
    fn a_callsign_becomes_the_flight_number_people_know() {
        assert_eq!(flight_number("ACA856"), "AC856");
        assert_eq!(flight_number("dlh470 "), "LH470");
        assert_eq!(flight_number("ROU2000"), "RV2000");
        assert_eq!(flight_number("WJA0012"), "WS12");
        // Not airline and number, or an airline not listed: as it is.
        assert_eq!(flight_number("BAW12AB"), "BAW12AB");
        assert_eq!(flight_number("JZA8011"), "JZA8011");
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
        // The daily limit.
        let sky = OpenSky::new(fake(200, &[429]), creds());
        assert!(matches!(
            sky.flights("c04826", 0, 1),
            Err(AiError::Cloud { retry: true, message }) if message == LIMIT_REACHED
        ));
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

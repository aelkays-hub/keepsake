//! Fill in route keywords (ADR-044 addition): from the route keywords a
//! photo has (`Aircraft|Flight|AT210`, `Aircraft|From|…`, `Aircraft|To|…`),
//! the ones it is missing. A flight number gives its airports (adsbdb's
//! flight routes, [`Lookup::callsign_route`]); airports without a flight
//! number give it from the aircraft's flights kept that day (OpenSky or
//! Flightradar24, nothing asked), else from the flight the other way that
//! day: its return is most often numbered one away (RAM211 out of YYZ,
//! RAM210 back), kept only when adsbdb says that number flies the photo's
//! airports ([`paired`]); every airport gives its country (the airport
//! list). Only what is missing is added; what the photo has is
//! never changed, and a flight that does not fly what the photo says is
//! not used.

use super::lookup::Lookup;
use super::route::{self, Airports, Clock, Flight, NEAR_S, TIE_S};
use super::scan::TAG_PARENT;
use crate::ai::AiError;
use crate::catalog::{AssetId, Catalog, JobState, NewJob, Priority, TagPath, TagSource};

/// The job: fill in one photo's route keywords.
pub const JOB_AI_ROUTE_FILL: &str = "ai.route.fill";

/// What a photo's route keywords say.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Given {
    pub flight: Option<String>,
    /// As written (`YYZ`, or an ICAO code).
    pub from: Option<String>,
    pub to: Option<String>,
    pub from_country: bool,
    pub to_country: bool,
}

/// The user's route keywords of `asset`.
pub fn given(cat: &Catalog, asset: AssetId) -> Result<Given, AiError> {
    let mut g = Given::default();
    for t in cat.read(|tx| tx.tags().for_asset(asset))? {
        if t.source != TagSource::User {
            continue;
        }
        let s = t.path.segments();
        if s.len() != 3 || s[0] != TAG_PARENT {
            continue;
        }
        let v = Some(s[2].trim().to_owned()).filter(|v| !v.is_empty());
        match s[1].as_str() {
            "Flight" => g.flight = g.flight.or(v),
            "From" => g.from = g.from.or(v),
            "To" => g.to = g.to.or(v),
            "From country" => g.from_country = true,
            "To country" => g.to_country = true,
            _ => {}
        }
    }
    Ok(g)
}

/// What filling in did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filled {
    /// The keywords added.
    Added(Vec<TagPath>),
    /// Nothing to add, and why.
    Nothing(String),
    /// Several flight numbers fly the photo's airports: the user chooses
    /// (`why`, and the numbers).
    Choose(String, Vec<String>),
}

/// Between a message and the flight numbers to choose from, as a job's
/// error keeps them ([`choices_of`]).
const CHOICES: &str = " | choose: ";

/// A job's error with the flight numbers to choose from.
pub fn with_choices(why: &str, numbers: &[String]) -> String {
    format!("{why}{CHOICES}{}", numbers.join(","))
}

/// A job's error read back: the message and the flight numbers to choose
/// from (none when it has none).
pub fn choices_of(error: &str) -> (String, Vec<String>) {
    match error.split_once(CHOICES) {
        Some((why, list)) => (
            why.to_owned(),
            list.split(',')
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_owned)
                .collect(),
        ),
        None => (error.to_owned(), Vec::new()),
    }
}

/// Tag `asset` with the flight number the user chose (and nothing else).
pub fn choose(
    cat: &mut Catalog,
    lib: &crate::library::Library,
    asset: AssetId,
    number: &str,
) -> Result<TagPath, AiError> {
    let n = number.trim().replace(['/', '|'], "-");
    if n.is_empty() {
        return Err(AiError::Image("no flight number".into()));
    }
    let path = TagPath::new([TAG_PARENT, "Flight", n.as_str()])?;
    super::scan::add_to(cat, lib, &[asset], std::slice::from_ref(&path))?;
    Ok(path)
}

/// The ICAO code of an airport as a keyword says it (`YYZ` or `CYYZ`).
fn icao_of(code: &str, airports: &Airports) -> Option<String> {
    airports.icao_of(code)
}

/// Whether `code` (as a keyword says it) is the airport `icao`.
fn same(code: &str, icao: &str, airports: &Airports) -> bool {
    icao_of(code, airports).is_some_and(|c| c.eq_ignore_ascii_case(icao))
        || code.eq_ignore_ascii_case(icao)
}

/// The flight of the aircraft, kept for the photo's days, that flies what
/// the photo says (`from` and/or `to`), nearest the photo; `None` when
/// none, or several as near.
fn kept_flight(flights: &[Flight], photo_s: i64, g: &Given, airports: &Airports) -> Option<Flight> {
    let fits = |f: &&Flight| {
        g.from
            .as_deref()
            .is_none_or(|c| f.departure.as_deref().is_some_and(|d| same(c, d, airports)))
            && g.to
                .as_deref()
                .is_none_or(|c| f.arrival.as_deref().is_some_and(|a| same(c, a, airports)))
    };
    let gap = |f: &Flight| {
        if photo_s < f.first_seen {
            f.first_seen - photo_s
        } else if photo_s > f.last_seen {
            photo_s - f.last_seen
        } else {
            0
        }
    };
    let mut near: Vec<(&Flight, i64)> = flights
        .iter()
        .filter(|f| !f.inferred)
        .filter(fits)
        .map(|f| (f, gap(f)))
        .filter(|(_, g)| *g <= NEAR_S)
        .collect();
    near.sort_by_key(|(_, g)| *g);
    match near.as_slice() {
        [] => None,
        [(f, _)] => Some((*f).clone()),
        [(f, a), (_, b), ..] => (b - a > TIE_S).then(|| (*f).clone()),
    }
}

/// The flight numbers one away from a callsign (`RAM211B` → `RAM210`,
/// `RAM212`): an airline most often numbers a flight and its return so.
pub fn neighbours(callsign: &str) -> Vec<String> {
    let cs = callsign.trim().to_ascii_uppercase();
    let code: String = cs.chars().take_while(char::is_ascii_uppercase).collect();
    let digits: String = cs[code.len()..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let (Ok(n), 2..=3) = (digits.parse::<u32>(), code.len()) else {
        return Vec::new();
    };
    [n.checked_sub(1), n.checked_add(1)]
        .into_iter()
        .flatten()
        .filter(|m| *m > 0)
        .map(|m| format!("{code}{m}"))
        .collect()
}

/// The flight number of a leg the source did not see, from the flights
/// either side of it: the one landing where the photo's flight left from
/// (`dep`), and the next leaving where it landed (`arr`), with the same
/// callsign (DAL8885 landed at PHL, next left YYZ: PHL → YYZ was DAL8885
/// too, a flight of several legs).
fn through(flights: &[Flight], photo_s: i64, dep: &str, arr: &str) -> Option<String> {
    let seen: Vec<&Flight> = flights.iter().filter(|f| !f.inferred).collect();
    let before = seen
        .iter()
        .filter(|f| f.last_seen <= photo_s)
        .max_by_key(|f| f.last_seen)?;
    let after = seen
        .iter()
        .filter(|f| f.first_seen >= photo_s)
        .min_by_key(|f| f.first_seen)?;
    let (a, b) = (before.callsign.as_deref()?, after.callsign.as_deref()?);
    (a.eq_ignore_ascii_case(b)
        && before.arrival.as_deref() == Some(dep)
        && after.departure.as_deref() == Some(arr))
    .then(|| route::flight_number(a))
}

/// The flight numbers of the photo's flight from the flight the other way
/// that day (one leaving where the photo's flight lands, or landing where
/// it left): the numbers one away from its callsign that adsbdb says fly
/// `dep` → `arr` (shown as sold: `QK893`). Returns them and the requests
/// sent.
fn paired(
    flights: &[Flight],
    dep: Option<&str>,
    arr: Option<&str>,
    lookup: &dyn Lookup,
) -> Result<(Vec<String>, u32), AiError> {
    let mut asked: Vec<String> = Vec::new();
    let mut found: Vec<String> = Vec::new();
    for f in flights.iter().filter(|f| !f.inferred) {
        let back = (arr.is_some() && f.departure.as_deref() == arr)
            || (dep.is_some() && f.arrival.as_deref() == dep);
        let Some(cs) = f.callsign.as_deref().filter(|_| back) else {
            continue;
        };
        for n in neighbours(cs) {
            if asked.contains(&n) {
                continue;
            }
            asked.push(n.clone());
            if let Some((o, d)) = lookup.callsign_route(&n)? {
                let fits = dep.is_none_or(|x| x == o) && arr.is_none_or(|x| x == d);
                if fits && !found.contains(&n) {
                    found.push(n);
                }
            }
        }
    }
    let requests = u32::try_from(asked.len()).unwrap_or(u32::MAX);
    Ok((
        found.iter().map(|n| route::flight_number(n)).collect(),
        requests,
    ))
}

/// Fill in `asset`'s route keywords from those it has. `lookup` (adsbdb)
/// for a flight number's airports; `None`: only what is kept and the
/// airport list. Returns what was added (written as the user's, in the
/// catalog and the sidecar) and the requests sent.
pub fn fill(
    cat: &mut Catalog,
    lib: &crate::library::Library,
    airports: &Airports,
    lookup: Option<&dyn Lookup>,
    asset: AssetId,
    clock: Clock,
) -> Result<(Filled, u32), AiError> {
    let g = given(cat, asset)?;
    if g.flight.is_none() && g.from.is_none() && g.to.is_none() {
        return Ok((
            Filled::Nothing(
                "no route keyword: add Aircraft|Flight|… (AT210), Aircraft|From|… or Aircraft|To|… first"
                    .into(),
            ),
            0,
        ));
    }
    let mut requests = 0;
    let mut dep: Option<String> = g.from.as_deref().and_then(|c| icao_of(c, airports));
    let mut arr: Option<String> = g.to.as_deref().and_then(|c| icao_of(c, airports));
    let mut number: Option<String> = None;
    // A flight number: its airports.
    if let Some(flight) = g.flight.as_deref() {
        if g.from.is_none() || g.to.is_none() {
            let Some(l) = lookup else {
                return Err(AiError::Cloud {
                    message: "turn on Look registrations up online in Settings → Read aircraft: a flight's airports come from adsbdb".into(),
                    retry: false,
                });
            };
            requests += 1;
            match l.callsign_route(flight)? {
                None => {
                    if dep.is_none() && arr.is_none() {
                        return Ok((
                            Filled::Nothing(format!("adsbdb does not know where {flight} flies")),
                            requests,
                        ));
                    }
                }
                Some((o, d)) => {
                    let wrong = |given: &Option<String>, icao: &str| {
                        given.as_deref().is_some_and(|c| !same(c, icao, airports))
                    };
                    if wrong(&g.from, &o) || wrong(&g.to, &d) {
                        let (o, d) = (airports.place(&o).0, airports.place(&d).0);
                        return Ok((
                            Filled::Nothing(format!(
                                "{flight} flies {o} → {d}, not what the photo says: nothing added"
                            )),
                            requests,
                        ));
                    }
                    dep = dep.or(Some(o));
                    arr = arr.or(Some(d));
                }
            }
        }
    } else {
        // Airports without a flight number: the aircraft's kept flight.
        let p = route::photo(cat, asset, clock)?;
        if let (Some(reg), Some(t)) = (p.registration.as_deref(), p.taken_s) {
            let flights = route::kept_around(cat, reg, t)?;
            if let Some(f) = kept_flight(&flights, t, &g, airports) {
                number = f.shown_number();
                dep = dep.or(f.departure.clone());
                arr = arr.or(f.arrival.clone());
            } else if let Some(n) = dep
                .as_deref()
                .zip(arr.as_deref())
                .and_then(|(d, a)| through(&flights, t, d, a))
            {
                // Not seen, but the same flight number either side of it.
                number = Some(n);
            } else if let Some(l) = lookup {
                // Not seen: the number of the flight the other way, one away.
                let (found, asked) = paired(&flights, dep.as_deref(), arr.as_deref(), l)?;
                requests += asked;
                match found.as_slice() {
                    [] => {}
                    [one] => number = Some(one.clone()),
                    several => {
                        let (d, a) = (
                            dep.as_deref().map_or("?".into(), |c| airports.place(c).0),
                            arr.as_deref().map_or("?".into(), |c| airports.place(c).0),
                        );
                        return Ok((
                            Filled::Choose(
                                format!(
                                    "{} fly {d} → {a}: choose the one photographed",
                                    several.join(" and ")
                                ),
                                several.to_vec(),
                            ),
                            requests,
                        ));
                    }
                }
            }
        }
    }
    let f = Flight {
        departure: dep,
        arrival: arr,
        number,
        ..Flight::default()
    };
    let has = |kind: &str| match kind {
        "Flight" => g.flight.is_some(),
        "From" => g.from.is_some(),
        "To" => g.to.is_some(),
        "From country" => g.from_country,
        "To country" => g.to_country,
        _ => true,
    };
    let add: Vec<TagPath> = route::keywords(&f, airports)?
        .into_iter()
        .filter(|k| k.segments().get(1).is_some_and(|kind| !has(kind)))
        .collect();
    if add.is_empty() {
        return Ok((
            Filled::Nothing(if g.flight.is_none() {
                "the flight number is not known: no flight Add route found for this aircraft that day flies this route, nor the number next to the one it flew the other way; type it as Aircraft|Flight|… if you know it".into()
            } else {
                "nothing missing".into()
            }),
            requests,
        ));
    }
    super::scan::add_to(cat, lib, &[asset], &add)?;
    Ok((Filled::Added(add), requests))
}

/// A flight number as it is compared: letters and digits, upper case, as
/// the flight is sold (`aca 856` → `AC856`, [`route::flight_number`]).
pub fn flight_key(flight: &str) -> String {
    let clean: String = flight
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_uppercase();
    if clean.is_empty() {
        return clean;
    }
    route::flight_number(&clean)
}

/// A route that other photos tagged with the same flight number show
/// (ADR-049): offered in the Add route window when the user types a flight
/// number; nothing is added until they accept it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// As written on those photos (`YYZ`, or an ICAO code); `None`: not said.
    pub from: Option<String>,
    pub from_country: Option<String>,
    pub to: Option<String>,
    pub to_country: Option<String>,
    /// The flight number as those photos have it.
    pub flight: String,
    /// How many photos show it.
    pub photos: u64,
    /// The latest of their capture times (Unix ms, UTC), when known.
    pub latest_ms: Option<i64>,
    /// What accepting it adds: `Aircraft|From|YYZ`, its country, `To`, its
    /// country and `Aircraft|Flight|…`.
    pub keywords: Vec<TagPath>,
}

/// One photo's route keywords, with the countries' names.
#[derive(Default)]
struct Written {
    flight: Option<String>,
    from: Option<String>,
    to: Option<String>,
    from_country: Option<String>,
    to_country: Option<String>,
}

fn written(cat: &Catalog, asset: AssetId) -> Result<Written, AiError> {
    let mut w = Written::default();
    for t in cat.read(|tx| tx.tags().for_asset(asset))? {
        if t.source != TagSource::User {
            continue;
        }
        let s = t.path.segments();
        if s.len() != 3 || s[0] != TAG_PARENT {
            continue;
        }
        let v = Some(s[2].trim().to_owned()).filter(|v| !v.is_empty());
        let slot = match s[1].as_str() {
            "Flight" => &mut w.flight,
            "From" => &mut w.from,
            "To" => &mut w.to,
            "From country" => &mut w.from_country,
            "To country" => &mut w.to_country,
            _ => continue,
        };
        if slot.is_none() {
            *slot = v;
        }
    }
    Ok(w)
}

/// The routes the photos tagged `Aircraft|Flight|<flight>` (as the user's
/// keyword; `ac856`, `AC 856` and `ACA856` are the same flight) show, other
/// than `except`: their from and to airports and countries (a country not
/// written is the airport list's). Those with the most photos first, then
/// the most recent. Nothing is asked online and nothing is changed.
pub fn suggest(
    cat: &Catalog,
    airports: &Airports,
    flight: &str,
    except: &[AssetId],
) -> Result<Vec<Suggestion>, AiError> {
    let want = flight_key(flight);
    if want.is_empty() {
        return Ok(Vec::new());
    }
    let parent = TagPath::new([TAG_PARENT, "Flight"])?;
    let names = cat.read(|tx| tx.tags().user_children(&parent))?;
    let mut seen: Vec<AssetId> = Vec::new();
    let mut out: Vec<Suggestion> = Vec::new();
    for (name, _) in names.into_iter().filter(|(n, _)| flight_key(n) == want) {
        let path = TagPath::new([TAG_PARENT, "Flight", name.as_str()])?;
        for id in cat.read(|tx| tx.tags().assets_with(&path))? {
            if except.contains(&id) || seen.contains(&id) {
                continue;
            }
            seen.push(id);
            let w = written(cat, id)?;
            let Some(number) = w.flight.filter(|f| flight_key(f) == want) else {
                continue;
            };
            if w.from.is_none() && w.to.is_none() {
                continue;
            }
            let country = |code: &Option<String>, given: Option<String>| {
                given.or_else(|| {
                    code.as_deref()
                        .and_then(|c| airports.icao_of(c))
                        .and_then(|i| airports.place(&i).1)
                })
            };
            let from_country = country(&w.from, w.from_country);
            let to_country = country(&w.to, w.to_country);
            let at = cat
                .read(|tx| tx.assets().get(id))?
                .and_then(|a| a.capture)
                .map(|c| c.utc_ms);
            let up = |c: &Option<String>| c.as_deref().map(str::to_ascii_uppercase);
            match out
                .iter_mut()
                .find(|s| up(&s.from) == up(&w.from) && up(&s.to) == up(&w.to))
            {
                Some(s) => {
                    s.photos += 1;
                    s.latest_ms = s.latest_ms.max(at);
                    s.from_country = s.from_country.take().or(from_country);
                    s.to_country = s.to_country.take().or(to_country);
                }
                None => out.push(Suggestion {
                    from: w.from,
                    from_country,
                    to: w.to,
                    to_country,
                    flight: number,
                    photos: 1,
                    latest_ms: at,
                    keywords: Vec::new(),
                }),
            }
        }
    }
    for s in &mut out {
        let mut k = Vec::new();
        for (kind, v) in [
            ("From", &s.from),
            ("From country", &s.from_country),
            ("To", &s.to),
            ("To country", &s.to_country),
            ("Flight", &Some(s.flight.clone())),
        ] {
            if let Some(v) = v {
                let v = v.replace(['/', '|'], "-");
                k.push(TagPath::new([TAG_PARENT, kind, v.trim()])?);
            }
        }
        s.keywords = k;
    }
    out.sort_by(|a, b| {
        b.photos
            .cmp(&a.photos)
            .then(b.latest_ms.cmp(&a.latest_ms))
            .then(a.from.cmp(&b.from))
            .then(a.to.cmp(&b.to))
    });
    Ok(out)
}

/// Queue filling in for `ids` (again for those done before).
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
                    kind: JOB_AI_ROUTE_FILL,
                    asset_id: *id,
                    rank: 0,
                    sort_key: i64::try_from(c * size + i).unwrap_or(i64::MAX),
                })
                .collect();
            tx.jobs().enqueue(&jobs, Priority::Recent, now, true)
        })?;
    }
    Ok(n)
}

/// Where filling in stands for one photo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Queued or under way.
    Waiting,
    /// Done: its route keywords now.
    Done(Vec<TagPath>),
    /// Nothing added, and why; the flight numbers to choose from, if any.
    Nothing(String, Vec<String>),
}

/// Where filling in stands for `asset`.
pub fn state(cat: &Catalog, asset: AssetId) -> Result<State, AiError> {
    let job = cat.read(|tx| tx.jobs().find(JOB_AI_ROUTE_FILL, asset))?;
    Ok(match job {
        None => State::Waiting,
        Some(j) => match j.state {
            JobState::Queued | JobState::Running => State::Waiting,
            JobState::Failed => {
                let (why, choices) = choices_of(&j.error.unwrap_or_default());
                State::Nothing(why, choices)
            }
            _ => State::Done(
                cat.read(|tx| tx.tags().for_asset(asset))?
                    .into_iter()
                    .filter(|t| t.source == TagSource::User && route::is_route_keyword(&t.path))
                    .map(|t| t.path)
                    .collect(),
            ),
        },
    })
}

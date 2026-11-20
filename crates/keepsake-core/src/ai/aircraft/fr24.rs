//! Look up aircraft on Flightradar24 (Settings → Read aircraft; ADR-044):
//! one batch job asks the user's Flightradar24 API, by registration, for
//! each accepted registration's latest flight in the last four weeks, and
//! keeps what it shows of the aircraft: its type, the airline operating it
//! and its livery, its transponder code.
//!
//! - Sent to fr24api.flightradar24.com: the registration and the weeks
//!   asked about (`registrations=PH-BKO&limit=1&sort=desc`). No photo.
//! - It costs 2 credits per aircraft found (a flight under 30 days old),
//!   none for one that did not fly; a second question (the two weeks
//!   before) only for those not found in the first.
//! - Only aircraft that flew in the last four weeks are found: no build
//!   year, owner or serial (Flightradar24 has no aircraft endpoint).
//! - Its terms allow keeping its data 30 days: it is kept with the
//!   registration's lookup (`Found::fr24`) and deleted 30 days after it was
//!   asked ([`forget_expired`]); then the registration may be asked again.
//!   The keywords the user accepts from it (type, airline) are theirs.
//! - Its transponder code lets OpenSky find the aircraft's older routes
//!   ([`super::route::icao24_of`]) while it is kept.
//! - The job takes [`BATCH`] registrations at a time, [`GAP`] apart
//!   (Explorer allows 10 requests a minute), and queues itself again while
//!   some are left, so Stop (dropping the queued job) ends it after the
//!   registrations it has in hand, and nothing is lost on a restart.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::lookup::{self, Found, Fr24Aircraft, LOOKUP_VERSION};
use super::route::Fr24Aircrafts;
use super::scan::TAG_PARENT;
use crate::ai::AiError;
use crate::catalog::{AssetId, Catalog, JobState, NewJob, Priority, TagPath};

/// The job kind: look a batch of registrations up.
pub const JOB_FR24_AIRCRAFT: &str = "ai.fr24.aircraft";
/// Registrations asked about per job.
pub const BATCH: usize = 25;
/// The time between two questions (Explorer: 10 a minute).
pub const GAP: Duration = Duration::from_millis(6_500);
/// How long to wait when Flightradar24 asks to slow down.
pub const SLOW_DOWN: Duration = Duration::from_secs(60);
/// One question covers this much (Flightradar24 allows 14 days).
const WINDOW_S: i64 = 14 * 86_400;

/// Every registration the user accepted on a photo, each with one of its
/// photos.
pub fn registrations(cat: &Catalog) -> Result<Vec<(String, AssetId)>, AiError> {
    let parent = TagPath::new([TAG_PARENT, "Registration"])?;
    Ok(cat.read(|tx| tx.tags().user_children(&parent))?)
}

/// What Flightradar24 said about `registration`, while it may be kept.
pub fn kept(
    cat: &Catalog,
    registration: &str,
    now_ms: i64,
) -> Result<Option<Fr24Aircraft>, AiError> {
    Ok(lookup::kept(cat, registration)?
        .and_then(|f| f.fr24)
        .filter(|a| a.fresh(now_ms)))
}

/// What Flightradar24 said about every registration looked up, while it
/// may be kept: one read (a read per registration, polled while the job
/// runs, kept the catalog's log from ever being written back).
fn all_kept(
    cat: &Catalog,
    now_ms: i64,
) -> Result<std::collections::HashMap<String, Fr24Aircraft>, AiError> {
    Ok(cat
        .read(|tx| tx.ai().aircraft_lookups())?
        .into_iter()
        .filter_map(|(reg, json, _)| {
            let a = serde_json::from_str::<Found>(&json).ok()?.fr24?;
            a.fresh(now_ms).then_some((reg, a))
        })
        .collect())
}

/// The registrations not asked about in the last 30 days.
pub fn due(cat: &Catalog, now_ms: i64) -> Result<Vec<String>, AiError> {
    let kept = all_kept(cat, now_ms)?;
    Ok(registrations(cat)?
        .into_iter()
        .map(|(reg, _)| reg)
        .filter(|reg| !kept.contains_key(reg))
        .collect())
}

/// Keep what Flightradar24 said about `registration`, with what the lookup
/// said (or as a registration not looked up yet).
pub fn put(cat: &mut Catalog, registration: &str, a: Fr24Aircraft) -> Result<(), AiError> {
    let now = crate::jobs::now_ms();
    let (mut found, at) = lookup::kept_at(cat, registration)?.unwrap_or_else(|| {
        (
            Found {
                registration: registration.to_owned(),
                version: LOOKUP_VERSION,
                ..Found::default()
            },
            now,
        )
    });
    found.fr24 = Some(a);
    let json = serde_json::to_string(&found)
        .map_err(|e| AiError::Image(format!("the lookup could not be kept: {e}")))?;
    cat.write(|tx| tx.ai().put_aircraft_lookup(registration, &json, at))?;
    Ok(())
}

/// What the lookup knows of an aircraft, with what Flightradar24 showed of
/// it (while kept) filling the gaps: its airline and type; `None` when
/// neither knows it.
pub fn with_fr24(f: &Found, now_ms: i64) -> Option<Found> {
    let a = f.fr24.as_ref().filter(|a| a.found && a.fresh(now_ms));
    if !f.known && a.is_none() {
        return None;
    }
    let mut out = f.clone();
    if let Some(a) = a {
        if out.airline.is_none() {
            // Flightradar24 leaves the operator empty for most flights:
            // the livery's airline then.
            out.airline = a
                .operator
                .as_deref()
                .or(a.painted_as.as_deref())
                .and_then(super::route::airline_of_code)
                .map(str::to_owned);
        }
        if out.aircraft_type.is_none() {
            out.aircraft_type = a
                .icao_type
                .as_deref()
                .and_then(|c| lookup::type_name(Some(c), None, None));
        }
    }
    out.known = true;
    Some(out)
}

/// What Flightradar24 showed of an aircraft, for people.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    /// `Boeing 787-10`, else the designator (`B78X`).
    pub aircraft_type: Option<String>,
    pub icao_type: Option<String>,
    /// The airline (from the operator, else the livery), and the livery's
    /// code as Flightradar24 gives it (`KLM`).
    pub airline: Option<String>,
    pub livery: Option<String>,
    pub hex: Option<String>,
    /// The day of the flight seen.
    pub seen: Option<String>,
    /// When it was asked, and when it is deleted (Unix ms).
    pub asked_ms: i64,
    pub until_ms: i64,
    pub found: bool,
}

/// What Flightradar24 said about `registration`, while it may be kept.
pub fn shown(cat: &Catalog, registration: &str, now_ms: i64) -> Result<Option<Shown>, AiError> {
    Ok(kept(cat, registration, now_ms)?.map(|a| {
        let code = a.operator.clone().or_else(|| a.painted_as.clone());
        Shown {
            aircraft_type: a
                .icao_type
                .as_deref()
                .and_then(|c| lookup::type_name(Some(c), None, None))
                .or_else(|| a.icao_type.clone()),
            icao_type: a.icao_type.clone(),
            airline: code
                .as_deref()
                .and_then(super::route::airline_of_code)
                .map(str::to_owned),
            livery: a.painted_as.clone(),
            hex: a.hex.clone(),
            seen: a.seen.clone(),
            asked_ms: a.asked_ms,
            until_ms: a.asked_ms + super::route::FR24_KEEP_MS,
            found: a.found,
        }
    }))
}

/// Delete what Flightradar24 said about aircraft more than 30 days before
/// `now_ms` (its terms). Returns how many were deleted.
pub fn forget_expired(cat: &mut Catalog, now_ms: i64) -> Result<u64, AiError> {
    let rows = cat.read(|tx| tx.ai().aircraft_lookups())?;
    let mut n = 0;
    for (reg, json, at) in rows {
        let Ok(mut f) = serde_json::from_str::<Found>(&json) else {
            continue;
        };
        if f.fr24.as_ref().is_some_and(|a| !a.fresh(now_ms)) {
            f.fr24 = None;
            let json = serde_json::to_string(&f)
                .map_err(|e| AiError::Image(format!("the lookup could not be kept: {e}")))?;
            cat.write(|tx| tx.ai().put_aircraft_lookup(&reg, &json, at))?;
            n += 1;
        }
    }
    Ok(n)
}

/// Ask about one registration: the last two weeks, then the two before.
pub fn ask_one(
    client: &dyn Fr24Aircrafts,
    registration: &str,
    now_ms: i64,
    pause: &dyn Fn(),
) -> Result<(Fr24Aircraft, u32), AiError> {
    let now_s = now_ms / 1000;
    let mut asked = 0;
    for weeks in 0..2 {
        if weeks > 0 {
            pause();
        }
        let end = now_s - weeks * WINDOW_S;
        asked += 1;
        if let Some(a) = client.aircraft(registration, end - WINDOW_S + 1, end)? {
            return Ok((a, asked));
        }
    }
    Ok((
        Fr24Aircraft {
            asked_ms: now_ms,
            found: false,
            ..Fr24Aircraft::default()
        },
        asked,
    ))
}

/// What one batch did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Batch {
    pub asked: u32,
    pub found: u32,
    /// Registrations still to ask about.
    pub left: usize,
}

/// One job: ask about up to [`BATCH`] registrations due, [`GAP`] apart,
/// keeping each answer as it comes. Flightradar24 asking to slow down is
/// waited out (once per registration); stops early when `cancel` is set.
pub fn run_batch(
    cat: &mut Catalog,
    client: &dyn Fr24Aircrafts,
    cancel: &AtomicBool,
    gap: Duration,
) -> Result<Batch, AiError> {
    let now = crate::jobs::now_ms();
    let due = due(cat, now)?;
    let mut b = Batch::default();
    let pause = |d: Duration| {
        let until = std::time::Instant::now() + d;
        while std::time::Instant::now() < until && !cancel.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100).min(d));
        }
    };
    for reg in due.iter().take(BATCH) {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        if b.asked > 0 {
            pause(gap);
        }
        let r = match ask_one(client, reg, now, &|| pause(gap)) {
            Err(AiError::Cloud { retry: true, .. }) => {
                pause(SLOW_DOWN);
                ask_one(client, reg, now, &|| pause(gap))
            }
            r => r,
        };
        let (a, asked) = r?;
        b.asked += asked;
        b.found += u32::from(a.found);
        put(cat, reg, a)?;
    }
    b.left = self::due(cat, now)?.len();
    Ok(b)
}

/// Queue the batch job (on the first registered photo). `false`: no
/// registration to ask about.
pub fn enqueue(cat: &mut Catalog) -> Result<bool, AiError> {
    let Some((_, anchor)) = registrations(cat)?.into_iter().next() else {
        return Ok(false);
    };
    let now = crate::jobs::now_ms();
    cat.write(|tx| {
        tx.jobs().enqueue(
            &[NewJob {
                kind: JOB_FR24_AIRCRAFT,
                asset_id: anchor,
                rank: 0,
                sort_key: 0,
            }],
            Priority::Recent,
            now,
            true,
        )
    })?;
    Ok(true)
}

/// Stop: drop the queued job (a batch under way finishes its registrations).
pub fn stop(cat: &mut Catalog) -> Result<(), AiError> {
    if let Some((_, anchor)) = registrations(cat)?.into_iter().next() {
        cat.write(|tx| tx.jobs().remove_queued_for(JOB_FR24_AIRCRAFT, &[anchor]))?;
    }
    Ok(())
}

/// Where looking up stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Status {
    pub registrations: u64,
    /// Asked about in the last 30 days, and of them found.
    pub asked: u64,
    pub found: u64,
    /// The job is queued or running.
    pub running: bool,
    /// Why it stopped, when it failed.
    pub problem: Option<String>,
}

pub fn status(cat: &Catalog) -> Result<Status, AiError> {
    let now = crate::jobs::now_ms();
    let regs = registrations(cat)?;
    let mut st = Status {
        registrations: regs.len() as u64,
        ..Status::default()
    };
    let kept = all_kept(cat, now)?;
    for (reg, _) in &regs {
        if let Some(a) = kept.get(reg) {
            st.asked += 1;
            st.found += u64::from(a.found);
        }
    }
    if let Some((_, anchor)) = regs.first() {
        if let Some(j) = cat.read(|tx| tx.jobs().find(JOB_FR24_AIRCRAFT, *anchor))? {
            st.running = matches!(j.state, JobState::Queued | JobState::Running);
            if j.state == JobState::Failed {
                st.problem = j.error;
            }
        }
    }
    Ok(st)
}

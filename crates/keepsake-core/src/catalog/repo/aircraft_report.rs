//! The aircraft report (Tools → Report → Aircraft): the photos of aircraft
//! and the airlines, types and registrations named on them (ADR-043).
//!
//! An item counts when it has the `Airplane` tag (as a keyword or an
//! on-device AI tag, or a tag below it) or a keyword under `Aircraft`
//! (`Aircraft|Airline|KLM`, `Aircraft|Type|Boeing 747`,
//! `Aircraft|Registration|PH-BFA`): what the user typed and what Read
//! aircraft found and the user accepted. Reads still waiting for review are
//! counted apart, not as marks.
//!
//! Read-only: two scans of `asset_tags` and one of the items found; no
//! media file is read.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::define_repo;
use super::tags::PATHS_CTE;
use crate::catalog::error::CatalogError;
use crate::catalog::AssetId;

define_repo!(
    /// Aircraft report queries (read-only).
    AircraftReportRepo
);

/// The tag that marks an aircraft photo.
pub const AIRPLANE_TAG: &str = "Airplane";
/// The parent of the aircraft keywords.
pub const AIRCRAFT_KEYWORDS: &str = "Aircraft";
/// How many airlines or types a row lists.
pub const TOP: usize = 3;

/// The kinds of aircraft keyword, as in `Aircraft|<kind>|<name>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Kind {
    Airline,
    Type,
    Registration,
}

impl Kind {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "Airline" => Some(Self::Airline),
            "Type" => Some(Self::Type),
            "Registration" => Some(Self::Registration),
            _ => None,
        }
    }
}

/// The report's totals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AircraftTotals {
    /// Items with the Airplane tag or an aircraft keyword.
    pub photos: u64,
    /// The same, with copies of one picture in several folders counted once
    /// (same file name and capture time).
    pub shots: u64,
    /// Items with an airline, type or registration keyword.
    pub with_marks: u64,
    pub with_airline: u64,
    pub with_type: u64,
    pub with_registration: u64,
    /// Different airlines, types and registrations.
    pub airlines: u64,
    pub types: u64,
    pub registrations: u64,
    /// Read aircraft results not yet accepted or dismissed.
    pub waiting_review: u64,
}

/// One airline, type or registration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AircraftRow {
    /// As in the keyword (`KLM`, `Boeing 747`, `PH-BFA`).
    pub name: String,
    /// The keyword path, for showing its items (`Aircraft|Airline|KLM`).
    pub path: String,
    pub photos: u64,
    /// Different registrations on these photos (0 in the registrations
    /// table).
    pub registrations: u64,
    /// The airlines and types on these photos, most photos first, at most
    /// [`TOP`] each (empty for the row's own kind).
    pub airlines: Vec<String>,
    pub types: Vec<String>,
    /// The first and last capture day (`YYYY-MM-DD`, local), when dated.
    pub first: Option<String>,
    pub last: Option<String>,
}

/// One year.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AircraftYear {
    /// `None` for undated items.
    pub year: Option<String>,
    pub photos: u64,
    pub with_marks: u64,
}

/// The whole aircraft report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AircraftReport {
    pub totals: AircraftTotals,
    /// Most photos first, then by name.
    pub airlines: Vec<AircraftRow>,
    pub types: Vec<AircraftRow>,
    pub registrations: Vec<AircraftRow>,
    /// Oldest first; undated last.
    pub years: Vec<AircraftYear>,
}

#[derive(Default)]
struct Acc {
    path: String,
    photos: HashSet<AssetId>,
    regs: HashSet<String>,
    airlines: HashMap<String, u64>,
    types: HashMap<String, u64>,
    first: Option<String>,
    last: Option<String>,
}

fn top(counts: HashMap<String, u64>) -> Vec<String> {
    let mut v: Vec<(String, u64)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.into_iter().take(TOP).map(|(n, _)| n).collect()
}

fn rows(map: HashMap<String, Acc>, kind: Kind) -> Vec<AircraftRow> {
    let mut out: Vec<AircraftRow> = map
        .into_iter()
        .map(|(name, a)| AircraftRow {
            name,
            path: a.path,
            photos: a.photos.len() as u64,
            registrations: if kind == Kind::Registration {
                0
            } else {
                a.regs.len() as u64
            },
            airlines: if kind == Kind::Airline {
                Vec::new()
            } else {
                top(a.airlines)
            },
            types: if kind == Kind::Type {
                Vec::new()
            } else {
                top(a.types)
            },
            first: a.first,
            last: a.last,
        })
        .collect();
    out.sort_by(|a, b| b.photos.cmp(&a.photos).then_with(|| a.name.cmp(&b.name)));
    out
}

/// An item in the report.
struct Item {
    day: Option<String>,
    shot: (Option<String>, Option<i64>, Option<AssetId>),
}

impl<M> AircraftReportRepo<'_, M> {
    /// Build the aircraft report.
    pub fn aircraft(&self) -> Result<AircraftReport, CatalogError> {
        // Every aircraft keyword on every item.
        let mut st = self.conn.prepare(&format!(
            "{PATHS_CTE}
             SELECT DISTINCT at.asset_id, tp.path
               FROM asset_tags at JOIN tag_paths tp ON tp.id = at.tag_id
              WHERE tp.path LIKE '{AIRCRAFT_KEYWORDS}|%|%'"
        ))?;
        let mut marks: HashMap<AssetId, Vec<(Kind, String, String)>> = HashMap::new();
        let mut q = st.query([])?;
        while let Some(r) = q.next()? {
            let id: AssetId = r.get(0)?;
            let path: String = r.get(1)?;
            let mut parts = path.splitn(3, '|');
            let (_, Some(kind), Some(name)) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let Some(kind) = Kind::parse(kind) else {
                continue;
            };
            let e = marks.entry(id).or_default();
            if !e.iter().any(|(k, n, _)| *k == kind && n == name) {
                e.push((kind, name.to_owned(), path.clone()));
            }
        }
        drop(q);

        // The items: tagged Airplane (or below), or with a keyword above.
        let mut st = self.conn.prepare(&format!(
            "{PATHS_CTE},
             plane AS (SELECT DISTINCT at.asset_id AS id
                         FROM asset_tags at JOIN tag_paths tp ON tp.id = at.tag_id
                        WHERE tp.path = '{AIRPLANE_TAG}' OR tp.path LIKE '{AIRPLANE_TAG}|%'),
             marked AS (SELECT DISTINCT at.asset_id AS id
                          FROM asset_tags at JOIN tag_paths tp ON tp.id = at.tag_id
                         WHERE tp.path LIKE '{AIRCRAFT_KEYWORDS}|%|%')
             SELECT a.id, a.capture_local_date, a.primary_name, a.capture_ts_ms
               FROM assets a
              WHERE a.id IN (SELECT id FROM plane) OR a.id IN (SELECT id FROM marked)"
        ))?;
        let mut items: HashMap<AssetId, Item> = HashMap::new();
        let mut q = st.query([])?;
        while let Some(r) = q.next()? {
            let id: AssetId = r.get(0)?;
            let name: Option<String> = r.get(2)?;
            let ts: Option<i64> = r.get(3)?;
            // A copy is the same picture: same name without its extension,
            // same capture time. An undated or nameless item is its own.
            let stem = name.map(|n| match n.rsplit_once('.') {
                Some((s, _)) => s.to_uppercase(),
                None => n.to_uppercase(),
            });
            let shot = match (&stem, ts) {
                (Some(_), Some(_)) => (stem, ts, None),
                _ => (None, None, Some(id)),
            };
            items.insert(
                id,
                Item {
                    day: r.get(1)?,
                    shot,
                },
            );
        }
        drop(q);

        let waiting: i64 = self.conn.query_row(
            "SELECT count(*) FROM aircraft_reads WHERE review = 'waiting'",
            [],
            |r| r.get(0),
        )?;

        let mut by: HashMap<Kind, HashMap<String, Acc>> = HashMap::new();
        let mut with = HashMap::<Kind, u64>::new();
        for (id, ms) in &marks {
            let day = items.get(id).and_then(|i| i.day.clone());
            for k in [Kind::Airline, Kind::Type, Kind::Registration] {
                if ms.iter().any(|(kind, _, _)| *kind == k) {
                    *with.entry(k).or_default() += 1;
                }
            }
            for (kind, name, path) in ms {
                let a = by
                    .entry(*kind)
                    .or_default()
                    .entry(name.clone())
                    .or_default();
                a.path.clone_from(path);
                a.photos.insert(*id);
                for (k2, n2, _) in ms {
                    match k2 {
                        Kind::Registration => {
                            a.regs.insert(n2.clone());
                        }
                        Kind::Airline => *a.airlines.entry(n2.clone()).or_default() += 1,
                        Kind::Type => *a.types.entry(n2.clone()).or_default() += 1,
                    }
                }
                if let Some(d) = &day {
                    if a.first.as_ref().is_none_or(|f| d < f) {
                        a.first = Some(d.clone());
                    }
                    if a.last.as_ref().is_none_or(|l| d > l) {
                        a.last = Some(d.clone());
                    }
                }
            }
        }

        let mut years: BTreeMap<Option<String>, (u64, u64)> = BTreeMap::new();
        for (id, it) in &items {
            let y = it
                .day
                .as_ref()
                .map(|d| d.chars().take(4).collect::<String>());
            let e = years.entry(y).or_default();
            e.0 += 1;
            if marks.contains_key(id) {
                e.1 += 1;
            }
        }
        let mut years: Vec<AircraftYear> = years
            .into_iter()
            .map(|(year, (photos, with_marks))| AircraftYear {
                year,
                photos,
                with_marks,
            })
            .collect();
        // BTreeMap puts None first; undated goes last.
        if years.first().is_some_and(|y| y.year.is_none()) {
            years.rotate_left(1);
        }

        let mut take = |k: Kind| rows(by.remove(&k).unwrap_or_default(), k);
        let airlines = take(Kind::Airline);
        let types = take(Kind::Type);
        let registrations = take(Kind::Registration);
        let shots: HashSet<_> = items.values().map(|i| &i.shot).collect();
        Ok(AircraftReport {
            totals: AircraftTotals {
                photos: items.len() as u64,
                shots: shots.len() as u64,
                with_marks: marks.len() as u64,
                with_airline: with.get(&Kind::Airline).copied().unwrap_or(0),
                with_type: with.get(&Kind::Type).copied().unwrap_or(0),
                with_registration: with.get(&Kind::Registration).copied().unwrap_or(0),
                airlines: airlines.len() as u64,
                types: types.len() as u64,
                registrations: registrations.len() as u64,
                waiting_review: u64::try_from(waiting).unwrap_or(0),
            },
            airlines,
            types,
            registrations,
            years,
        })
    }
}

//! The sidebar's aircraft trees (ADR-053): the library by airline, type or
//! registration, then by year and month, with how many photos each holds.
//!
//! A photo is under each name of the kind it has as a keyword
//! (`Aircraft|Airline|KLM`, or one below it). The photos with an aircraft
//! keyword (`Aircraft` or one below it) but none of this kind are under
//! the unnamed node. Each node's count is exactly what the library's search
//! finds for it: `tags` with the name, `without_tags` with the kind for the
//! unnamed, and `from`/`to` for a year or month or `undated` for the
//! photos with no date.
//!
//! Read-only: one scan of the aircraft keywords; no media file is read.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::aircraft_report::{AircraftReportRepo, AIRCRAFT_KEYWORDS};
use super::tags::PATHS_CTE;
use crate::catalog::error::CatalogError;
use crate::catalog::AssetId;

/// Which aircraft keyword a tree is built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AircraftKind {
    Airline,
    Type,
    Registration,
}

impl AircraftKind {
    /// As in the keyword: `Aircraft|<this>|<name>`.
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Airline => "Airline",
            Self::Type => "Type",
            Self::Registration => "Registration",
        }
    }
}

/// One month of a year.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AircraftTreeMonth {
    /// 1–12.
    pub month: u32,
    pub photos: u64,
}

/// One year of a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AircraftTreeYear {
    /// `None`: the photos with no date.
    pub year: Option<i32>,
    pub photos: u64,
    /// Oldest first (none for the photos with no date).
    pub months: Vec<AircraftTreeMonth>,
}

/// One airline, type or registration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AircraftTreeNode {
    /// As in the keyword (`KLM`); `None`: the photos that name none of this
    /// kind.
    pub name: Option<String>,
    pub photos: u64,
    /// Oldest first; the photos with no date last.
    pub years: Vec<AircraftTreeYear>,
}

/// A photo's day: `(year, month)`, or `None` when it has no date.
type Day = Option<(i32, u32)>;

fn day_of(date: Option<&str>) -> Option<(i32, u32)> {
    let d = date?;
    let y = d.get(0..4)?.parse().ok()?;
    let m = d.get(5..7)?.parse().ok()?;
    Some((y, m))
}

fn node(name: Option<String>, photos: &HashMap<AssetId, Day>) -> AircraftTreeNode {
    // `None` sorts first in a BTreeMap: the undated go last below.
    let mut years: BTreeMap<Option<i32>, BTreeMap<u32, u64>> = BTreeMap::new();
    let mut per_year: HashMap<Option<i32>, u64> = HashMap::new();
    for day in photos.values() {
        let y = day.map(|(y, _)| y);
        *per_year.entry(y).or_default() += 1;
        let months = years.entry(y).or_default();
        if let Some((_, m)) = day {
            *months.entry(*m).or_default() += 1;
        }
    }
    let mut years: Vec<AircraftTreeYear> = years
        .into_iter()
        .map(|(year, months)| AircraftTreeYear {
            year,
            photos: per_year.get(&year).copied().unwrap_or(0),
            months: months
                .into_iter()
                .map(|(month, photos)| AircraftTreeMonth { month, photos })
                .collect(),
        })
        .collect();
    if years.first().is_some_and(|y| y.year.is_none()) {
        years.rotate_left(1);
    }
    AircraftTreeNode {
        name,
        photos: photos.len() as u64,
        years,
    }
}

impl<M> AircraftReportRepo<'_, M> {
    /// The library by `kind`, then year and month: most photos first, then
    /// by name; the unnamed last.
    pub fn tree(&self, kind: AircraftKind) -> Result<Vec<AircraftTreeNode>, CatalogError> {
        // Every aircraft keyword (or the bare parent) on every item, with
        // the item's day. LIKE ignores ASCII case, as the search does.
        let mut st = self.conn.prepare(&format!(
            "{PATHS_CTE}
             SELECT DISTINCT a.id, a.capture_ts_ms IS NULL, a.capture_local_date, tp.path
               FROM asset_tags at
               JOIN tag_paths tp ON tp.id = at.tag_id
               JOIN assets a ON a.id = at.asset_id
              WHERE tp.path = '{AIRCRAFT_KEYWORDS}' OR tp.path LIKE '{AIRCRAFT_KEYWORDS}|%'"
        ))?;
        let want = kind.keyword().to_ascii_lowercase();
        let mut day: HashMap<AssetId, Day> = HashMap::new();
        // Each name's photos; the photos with this kind of keyword at all.
        let mut named: HashMap<String, HashMap<AssetId, Day>> = HashMap::new();
        let mut has_kind: HashSet<AssetId> = HashSet::new();
        let mut q = st.query([])?;
        while let Some(r) = q.next()? {
            let id: AssetId = r.get(0)?;
            let undated: bool = r.get(1)?;
            let date: Option<String> = r.get(2)?;
            let path: String = r.get(3)?;
            let d = if undated {
                None
            } else {
                match day_of(date.as_deref()) {
                    Some(d) => Some(d),
                    // Dated, but no readable local day: in no year or
                    // month the search could find; left out.
                    None => continue,
                }
            };
            day.insert(id, d);
            let mut parts = path.split('|');
            let (_, k, name) = (parts.next(), parts.next(), parts.next());
            if k.is_none_or(|k| k.to_ascii_lowercase() != want) {
                continue;
            }
            has_kind.insert(id);
            if let Some(n) = name {
                named.entry(n.to_owned()).or_default().insert(id, d);
            }
        }
        drop(q);

        let mut out: Vec<AircraftTreeNode> = named
            .into_iter()
            .map(|(name, photos)| node(Some(name), &photos))
            .collect();
        out.sort_by(|a, b| b.photos.cmp(&a.photos).then_with(|| a.name.cmp(&b.name)));
        let none: HashMap<AssetId, Day> = day
            .into_iter()
            .filter(|(id, _)| !has_kind.contains(id))
            .collect();
        if !none.is_empty() {
            out.push(node(None, &none));
        }
        Ok(out)
    }
}

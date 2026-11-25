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

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::aircraft_report::{AircraftReportRepo, AIRCRAFT_KEYWORDS};
use super::name_tree::{self, Day, NameTreeNode};
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

impl<M> AircraftReportRepo<'_, M> {
    /// The library by `kind`, then year and month: most photos first, then
    /// by name; the unnamed last.
    pub fn tree(&self, kind: AircraftKind) -> Result<Vec<NameTreeNode>, CatalogError> {
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
            let Some(d) = name_tree::day_of(undated, date.as_deref()) else {
                continue;
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

        let none: HashMap<AssetId, Day> = day
            .into_iter()
            .filter(|(id, _)| !has_kind.contains(id))
            .collect();
        Ok(name_tree::tree(named, &none))
    }
}

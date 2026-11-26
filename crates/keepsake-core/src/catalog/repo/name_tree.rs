//! The sidebar's trees by name, then year and month (ADR-053): by person
//! (`face_tree`). The nodes, and building them from each name's photos and
//! their days.

use std::cmp::Ordering;

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::catalog::AssetId;

/// One month of a year.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameTreeMonth {
    /// 1–12.
    pub month: u32,
    pub photos: u64,
}

/// One year of a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameTreeYear {
    /// `None`: the photos with no date.
    pub year: Option<i32>,
    pub photos: u64,
    /// Oldest first (none for the photos with no date).
    pub months: Vec<NameTreeMonth>,
}

/// One name: a person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameTreeNode {
    /// As in the keyword (`Sara`); `None`: the unnamed node (the photos
    /// with a face that has no name).
    pub name: Option<String>,
    pub photos: u64,
    /// Oldest first; the photos with no date last.
    pub years: Vec<NameTreeYear>,
}

/// A photo's day: `(year, month)`, or `None` when it has no date.
pub(crate) type Day = Option<(i32, u32)>;

/// The day of an item with `undated` (no capture time) and its local date
/// (`YYYY-MM-DD`). `None`: dated, but no readable local day, so in no year
/// or month the search could find: left out of the tree.
pub(crate) fn day_of(undated: bool, date: Option<&str>) -> Option<Day> {
    if undated {
        return Some(None);
    }
    let d = date?;
    let y = d.get(0..4)?.parse().ok()?;
    let m = d.get(5..7)?.parse().ok()?;
    Some(Some((y, m)))
}

/// A name's node from its photos and their days.
pub(crate) fn node(name: Option<String>, photos: &HashMap<AssetId, Day>) -> NameTreeNode {
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
    let mut years: Vec<NameTreeYear> = years
        .into_iter()
        .map(|(year, months)| NameTreeYear {
            year,
            photos: per_year.get(&year).copied().unwrap_or(0),
            months: months
                .into_iter()
                .map(|(month, photos)| NameTreeMonth { month, photos })
                .collect(),
        })
        .collect();
    if years.first().is_some_and(|y| y.year.is_none()) {
        years.rotate_left(1);
    }
    NameTreeNode {
        name,
        photos: photos.len() as u64,
        years,
    }
}

/// A to Z, ignoring case (then as written, so the order is fixed).
fn by_name(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

/// The tree: the names A to Z; the unnamed node last (when it holds any
/// photo).
pub(crate) fn tree(
    named: HashMap<String, HashMap<AssetId, Day>>,
    unnamed: &HashMap<AssetId, Day>,
) -> Vec<NameTreeNode> {
    let mut out: Vec<NameTreeNode> = named
        .into_iter()
        .map(|(name, photos)| node(Some(name), &photos))
        .collect();
    out.sort_by(|a, b| {
        by_name(
            a.name.as_deref().unwrap_or_default(),
            b.name.as_deref().unwrap_or_default(),
        )
    });
    if !unnamed.is_empty() {
        out.push(node(None, unnamed));
    }
    out
}

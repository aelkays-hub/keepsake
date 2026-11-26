//! The sidebar's tree by face (ADR-053): the library by person, then by
//! year and month, with how many photos each holds.
//!
//! A photo is under each person it is named with: the `People|<name>`
//! keywords a confirmed face writes (ADR-040), or that another app wrote
//! (or one below them). The photos with a face that has no name (found,
//! or only suggested for someone, and not "not a face") are under the
//! unnamed node, whatever other faces they have. Each node's count is
//! exactly what the library's search finds for it: `tags` with the
//! person's keyword, `unnamed_faces` for the unnamed, and `from`/`to` for
//! a year or month or `undated` for the photos with no date.
//!
//! Read-only: no media file is read.

use std::collections::HashMap;

use super::faces::Faces;
use super::name_tree::{self, Day, NameTreeNode};
use super::tags::PATHS_CTE;
use crate::catalog::error::CatalogError;
use crate::catalog::AssetId;
use crate::metadata::PEOPLE_TAG;

impl<M> Faces<'_, M> {
    /// The library by person, then year and month: people A to Z; the
    /// photos with an unnamed face last.
    pub fn tree(&self) -> Result<Vec<NameTreeNode>, CatalogError> {
        // Every person keyword on every item, with the item's day. LIKE
        // ignores ASCII case, as the search does; the bare `People` (an AI
        // label) names no one.
        let mut st = self.conn.prepare(&format!(
            "{PATHS_CTE}
             SELECT DISTINCT a.id, a.capture_ts_ms IS NULL, a.capture_local_date, tp.path
               FROM asset_tags at
               JOIN tag_paths tp ON tp.id = at.tag_id
               JOIN assets a ON a.id = at.asset_id
              WHERE tp.path LIKE '{PEOPLE_TAG}|%'"
        ))?;
        let mut named: HashMap<String, HashMap<AssetId, Day>> = HashMap::new();
        let mut q = st.query([])?;
        while let Some(r) = q.next()? {
            let id: AssetId = r.get(0)?;
            let date: Option<String> = r.get(2)?;
            let path: String = r.get(3)?;
            let Some(d) = name_tree::day_of(r.get(1)?, date.as_deref()) else {
                continue;
            };
            if let Some(name) = path.split('|').nth(1).filter(|n| !n.is_empty()) {
                named.entry(name.to_owned()).or_default().insert(id, d);
            }
        }
        drop(q);

        let mut st = self.conn.prepare(
            "SELECT DISTINCT a.id, a.capture_ts_ms IS NULL, a.capture_local_date
               FROM faces f JOIN assets a ON a.id = f.asset_id
              WHERE f.state IN ('unknown', 'suggested')",
        )?;
        let mut unnamed: HashMap<AssetId, Day> = HashMap::new();
        let mut q = st.query([])?;
        while let Some(r) = q.next()? {
            let id: AssetId = r.get(0)?;
            let date: Option<String> = r.get(2)?;
            if let Some(d) = name_tree::day_of(r.get(1)?, date.as_deref()) {
                unnamed.insert(id, d);
            }
        }
        drop(q);

        Ok(name_tree::tree(named, &unnamed))
    }
}

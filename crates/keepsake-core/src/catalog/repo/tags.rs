use rusqlite::{params, OptionalExtension};

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::{fts, AssetId, AssetTag, Rw, TagPath, TagSource};
use crate::relpath::{path_key, CaseSensitivity};

define_repo!(
    /// Hierarchical tags and their assignment to assets.
    Tags
);

/// Recursive CTE yielding `(tag_id, 'A|B|C')` for every tag.
pub(crate) const PATHS_CTE: &str = "WITH RECURSIVE tag_paths(id, path) AS (
       SELECT id, name FROM tags WHERE parent_id IS NULL
       UNION ALL
       SELECT t.id, tp.path || '|' || t.name FROM tags t JOIN tag_paths tp ON t.parent_id = tp.id
     )";

impl<M> Tags<'_, M> {
    /// Tags on an asset, sorted by source then path.
    pub fn for_asset(&self, asset: AssetId) -> Result<Vec<AssetTag>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "{PATHS_CTE}
             SELECT tp.path, at.source, at.confidence
               FROM asset_tags at JOIN tag_paths tp ON tp.id = at.tag_id
              WHERE at.asset_id = ?1"
        ))?;
        let raw: Vec<(String, TagSource, Option<f64>)> = st
            .query_map([asset], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<_, _>>()?;
        let mut out = raw
            .into_iter()
            .map(|(p, source, confidence)| {
                Ok(AssetTag {
                    path: TagPath::parse_lightroom(&p)?,
                    source,
                    confidence,
                })
            })
            .collect::<Result<Vec<_>, CatalogError>>()?;
        out.sort_by(|a, b| (a.source, &a.path).cmp(&(b.source, &b.path)));
        Ok(out)
    }

    /// Every tag path in the catalog, sorted.
    pub fn all_paths(&self) -> Result<Vec<TagPath>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "{PATHS_CTE} SELECT path FROM tag_paths ORDER BY path"
        ))?;
        let raw: Vec<String> = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        raw.iter().map(|p| TagPath::parse_lightroom(p)).collect()
    }

    /// Each user keyword on any of `ids`, with how many of them carry it,
    /// sorted by path. Duplicate ids must be removed first.
    pub fn user_counts(&self, ids: &[AssetId]) -> Result<Vec<(TagPath, u64)>, CatalogError> {
        let mut by_tag: std::collections::HashMap<i64, u64> = std::collections::HashMap::new();
        for chunk in ids.chunks(500) {
            let sql = format!(
                "SELECT tag_id, count(*) FROM asset_tags
                  WHERE source = 'user' AND asset_id IN ({}) GROUP BY tag_id",
                vec!["?"; chunk.len()].join(",")
            );
            let mut st = self.conn.prepare(&sql)?;
            let rows = st.query_map(rusqlite::params_from_iter(chunk.iter()), |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (tag, n) = row?;
                *by_tag.entry(tag).or_default() += u64::try_from(n).unwrap_or(0);
            }
        }
        if by_tag.is_empty() {
            return Ok(Vec::new());
        }
        let mut st = self
            .conn
            .prepare(&format!("{PATHS_CTE} SELECT id, path FROM tag_paths"))?;
        let paths: Vec<(i64, String)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let mut out = Vec::with_capacity(by_tag.len());
        for (id, p) in paths {
            if let Some(n) = by_tag.get(&id) {
                out.push((TagPath::parse_lightroom(&p)?, *n));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }

    /// Every tag on at least one asset (keywords and AI tags), with how
    /// many assets carry it or a tag below it, most used first.
    pub fn counts(&self) -> Result<Vec<(TagPath, u64)>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "{PATHS_CTE}
             SELECT p.path, count(DISTINCT at.asset_id)
               FROM tag_paths p
               JOIN tag_paths d ON d.path = p.path OR d.path LIKE p.path || '|%'
               JOIN asset_tags at ON at.tag_id = d.id
              GROUP BY p.path
              ORDER BY count(DISTINCT at.asset_id) DESC, p.path
              LIMIT 1000"
        ))?;
        let raw: Vec<(String, i64)> = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        raw.into_iter()
            .map(|(p, n)| Ok((TagPath::parse_lightroom(&p)?, u64::try_from(n).unwrap_or(0))))
            .collect()
    }

    /// Ids of assets carrying `path` (exactly, not descendants).
    pub fn assets_with(&self, path: &TagPath) -> Result<Vec<AssetId>, CatalogError> {
        let Some(tag) = find_path(self.conn, path)? else {
            return Ok(Vec::new());
        };
        let mut st = self.conn.prepare(
            "SELECT DISTINCT asset_id FROM asset_tags WHERE tag_id = ?1 ORDER BY asset_id",
        )?;
        let rows = st
            .query_map([tag], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

impl Tags<'_, Rw> {
    /// Return the id of the leaf tag of `path`, creating any missing ancestors.
    pub fn ensure_path(&self, path: &TagPath) -> Result<i64, CatalogError> {
        let mut parent: Option<i64> = None;
        for seg in path.segments() {
            let key = path_key(seg, CaseSensitivity::Insensitive);
            let existing: Option<i64> = self
                .conn
                .query_row(
                    "SELECT id FROM tags WHERE ifnull(parent_id, 0) = ifnull(?1, 0) AND name_key = ?2",
                    params![parent, key],
                    |r| r.get(0),
                )
                .optional()?;
            let id = match existing {
                Some(id) => id,
                None => {
                    self.conn.execute(
                        "INSERT INTO tags (parent_id, name, name_key) VALUES (?1, ?2, ?3)",
                        params![parent, seg, key],
                    )?;
                    self.conn.last_insert_rowid()
                }
            };
            parent = Some(id);
        }
        parent.ok_or_else(|| CatalogError::Invalid("empty tag path".into()))
    }

    /// Replace all of an asset's tags from one source.
    pub fn set_for_asset(
        &self,
        asset: AssetId,
        source: TagSource,
        tags: &[AssetTag],
    ) -> Result<(), CatalogError> {
        self.conn.execute(
            "DELETE FROM asset_tags WHERE asset_id = ?1 AND source = ?2",
            params![asset, source],
        )?;
        for t in tags {
            if t.source != source {
                return Err(CatalogError::Invalid(format!(
                    "tag {} has source {} but {} was requested",
                    t.path, t.source, source
                )));
            }
            if let Some(c) = t.confidence {
                if !(0.0..=1.0).contains(&c) {
                    return Err(CatalogError::Invalid(format!(
                        "confidence {c} out of range"
                    )));
                }
            }
            let tag = self.ensure_path(&t.path)?;
            self.conn.execute(
                "INSERT INTO asset_tags (asset_id, tag_id, source, confidence) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(asset_id, tag_id, source) DO UPDATE SET confidence = excluded.confidence",
                params![asset, tag, source, t.confidence],
            )?;
        }
        fts::reindex(self.conn, asset)
    }

    /// Delete tags that no asset uses and that have no children. Repeats until
    /// stable so whole unused branches disappear. Returns how many were removed.
    pub fn prune_unused(&self) -> Result<u64, CatalogError> {
        let mut total = 0u64;
        loop {
            let n = self.conn.execute(
                "DELETE FROM tags WHERE
                   NOT EXISTS (SELECT 1 FROM asset_tags at WHERE at.tag_id = tags.id) AND
                   NOT EXISTS (SELECT 1 FROM tags c WHERE c.parent_id = tags.id)",
                [],
            )?;
            if n == 0 {
                return Ok(total);
            }
            total += n as u64;
        }
    }
}

fn find_path(conn: &rusqlite::Connection, path: &TagPath) -> Result<Option<i64>, CatalogError> {
    let mut parent: Option<i64> = None;
    for seg in path.segments() {
        let key = path_key(seg, CaseSensitivity::Insensitive);
        let id: Option<i64> = conn
            .query_row(
                "SELECT id FROM tags WHERE ifnull(parent_id, 0) = ifnull(?1, 0) AND name_key = ?2",
                params![parent, key],
                |r| r.get(0),
            )
            .optional()?;
        match id {
            Some(id) => parent = Some(id),
            None => return Ok(None),
        }
    }
    Ok(parent)
}

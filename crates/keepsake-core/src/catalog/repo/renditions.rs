use rusqlite::{params, OptionalExtension, Row};

use super::{define_repo, rel_path_col, to_i64, u64_col};
use crate::catalog::error::CatalogError;
use crate::catalog::{Blake3Hash, Rendition, RenditionId, RenditionKind, Rw};
use crate::relpath::RelPath;

define_repo!(
    /// Rendition cache index.
    Renditions
);

const COLS: &str =
    "id, source_hash, recipe_hash, kind, rel_path, size_bytes, created_ms, last_used_ms";

fn map(r: &Row<'_>) -> rusqlite::Result<Rendition> {
    Ok(Rendition {
        id: r.get(0)?,
        source_hash: r.get(1)?,
        recipe_hash: r.get(2)?,
        kind: r.get(3)?,
        rel_path: rel_path_col(r, 4)?,
        size_bytes: u64_col(r, 5)?,
        created_ms: r.get(6)?,
        last_used_ms: r.get(7)?,
    })
}

impl<M> Renditions<'_, M> {
    pub fn find(
        &self,
        source: &Blake3Hash,
        recipe: &Blake3Hash,
        kind: RenditionKind,
    ) -> Result<Option<Rendition>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {COLS} FROM renditions WHERE source_hash = ?1 AND recipe_hash = ?2 AND kind = ?3"
                ),
                params![source, recipe, kind],
                map,
            )
            .optional()?)
    }

    /// Every rendition of one source.
    pub fn for_source(&self, source: &Blake3Hash) -> Result<Vec<Rendition>, CatalogError> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {COLS} FROM renditions WHERE source_hash = ?1 ORDER BY id"
        ))?;
        let rows = st.query_map([source], map)?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Up to `limit` renditions (after `after`, by id) whose file is not at
    /// its content-addressed name `<kind>/<aa>/<source>-<recipe>.<ext>`:
    /// left behind when their source was re-keyed (ADR-029, ADR-034).
    pub fn misnamed(
        &self,
        after: Option<RenditionId>,
        limit: u32,
    ) -> Result<Vec<Rendition>, CatalogError> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {COLS} FROM renditions
             WHERE (?1 IS NULL OR id > ?1)
               AND rel_path NOT LIKE kind || '/' || lower(substr(hex(source_hash), 1, 2)) || '/'
                   || lower(hex(source_hash)) || '-' || lower(hex(recipe_hash)) || '.%'
             ORDER BY id LIMIT ?2"
        ))?;
        let rows = st
            .query_map(params![after, limit], map)?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Number of renditions and their total size.
    pub fn totals(&self) -> Result<(u64, u64), CatalogError> {
        let (n, b): (i64, i64) = self.conn.query_row(
            "SELECT count(*), ifnull(sum(size_bytes), 0) FROM renditions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok((u64::try_from(n).unwrap_or(0), u64::try_from(b).unwrap_or(0)))
    }

    /// Bytes of renditions whose source has no original on these volumes.
    pub fn bytes_not_on(&self, volumes: &[crate::catalog::VolumeId]) -> Result<u64, CatalogError> {
        let list = vec!["?"; volumes.len()].join(",");
        let on = if volumes.is_empty() {
            "0".to_owned()
        } else {
            format!(
                "EXISTS (SELECT 1 FROM files f WHERE f.blake3 = r.source_hash AND f.role = 'original'
                         AND f.volume_id IN ({list}))"
            )
        };
        let b: i64 = self.conn.query_row(
            &format!("SELECT ifnull(sum(r.size_bytes), 0) FROM renditions r WHERE NOT {on}"),
            rusqlite::params_from_iter(volumes.iter()),
            |r| r.get(0),
        )?;
        Ok(u64::try_from(b).unwrap_or(0))
    }

    /// Does `source` have an original on one of these volumes?
    pub fn source_on(
        &self,
        source: &Blake3Hash,
        volumes: &[crate::catalog::VolumeId],
    ) -> Result<bool, CatalogError> {
        if volumes.is_empty() {
            return Ok(false);
        }
        let sql = format!(
            "SELECT EXISTS (SELECT 1 FROM files WHERE blake3 = ? AND role = 'original' AND volume_id IN ({}))",
            vec!["?"; volumes.len()].join(",")
        );
        let mut args: Vec<&dyn rusqlite::ToSql> = vec![source];
        for v in volumes {
            args.push(v);
        }
        Ok(self.conn.query_row(&sql, args.as_slice(), |r| r.get(0))?)
    }

    /// The order cache eviction removes renditions in: previews and proxies
    /// first, then thumbnails, each least recently used first. Thumbnails
    /// are small and the grid needs every one; the others are large and
    /// made again when a picture or video is opened.
    pub fn list_lru(&self) -> Result<Vec<Rendition>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLS} FROM renditions ORDER BY kind = 'thumbnail', last_used_ms, id"
        ))?;
        let rows = st.query_map([], map)?.collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

impl Renditions<'_, Rw> {
    /// Insert, replacing any rendition with the same `(source, recipe, kind)`.
    pub fn upsert(&self, r: &Rendition) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO renditions (id, source_hash, recipe_hash, kind, rel_path, size_bytes, created_ms, last_used_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(source_hash, recipe_hash, kind) DO UPDATE SET id = excluded.id,
               rel_path = excluded.rel_path, size_bytes = excluded.size_bytes,
               created_ms = excluded.created_ms, last_used_ms = excluded.last_used_ms",
            params![
                r.id,
                r.source_hash,
                r.recipe_hash,
                r.kind,
                r.rel_path.as_str(),
                to_i64(r.size_bytes, "size")?,
                r.created_ms,
                r.last_used_ms
            ],
        )?;
        Ok(())
    }

    pub fn touch(&self, id: RenditionId, now_ms: i64) -> Result<(), CatalogError> {
        self.conn.execute(
            "UPDATE renditions SET last_used_ms = ?2 WHERE id = ?1",
            params![id, now_ms],
        )?;
        Ok(())
    }

    /// Point a rendition at its file's new name, unless the row changed
    /// since it was read (another process fixed it first).
    pub fn set_rel_path(
        &self,
        id: RenditionId,
        from: &RelPath,
        to: &RelPath,
        size_bytes: u64,
    ) -> Result<bool, CatalogError> {
        let n = self.conn.execute(
            "UPDATE renditions SET rel_path = ?3, size_bytes = ?4 WHERE id = ?1 AND rel_path = ?2",
            params![id, from.as_str(), to.as_str(), to_i64(size_bytes, "size")?],
        )?;
        Ok(n > 0)
    }

    pub fn delete(&self, id: RenditionId) -> Result<(), CatalogError> {
        self.conn
            .execute("DELETE FROM renditions WHERE id = ?1", [id])?;
        Ok(())
    }
}

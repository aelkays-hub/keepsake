use rusqlite::{params, OptionalExtension, Row};

use super::{define_repo, rel_path_col, to_i64, u64_col};
use crate::catalog::error::CatalogError;
use crate::catalog::{Blake3Hash, Rendition, RenditionId, RenditionKind, Rw};

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

    /// Least recently used first (for cache eviction).
    pub fn list_lru(&self) -> Result<Vec<Rendition>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLS} FROM renditions ORDER BY last_used_ms, id"
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

    pub fn delete(&self, id: RenditionId) -> Result<(), CatalogError> {
        self.conn
            .execute("DELETE FROM renditions WHERE id = ?1", [id])?;
        Ok(())
    }
}

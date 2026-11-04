use rusqlite::{params, OptionalExtension, Row};

use super::{define_repo, rel_path_col};
use crate::catalog::error::CatalogError;
use crate::catalog::{AssetId, EditRecipe, RecipeId, Rw};

define_repo!(
    /// Edit recipe references.
    Recipes
);

const COLS: &str =
    "id, asset_id, kind, schema_version, rel_path, content_hash, created_ms, updated_ms";

fn map(r: &Row<'_>) -> rusqlite::Result<EditRecipe> {
    Ok(EditRecipe {
        id: r.get(0)?,
        asset_id: r.get(1)?,
        kind: r.get(2)?,
        schema_version: r.get(3)?,
        rel_path: rel_path_col(r, 4)?,
        content_hash: r.get(5)?,
        created_ms: r.get(6)?,
        updated_ms: r.get(7)?,
    })
}

impl<M> Recipes<'_, M> {
    pub fn get(&self, id: RecipeId) -> Result<Option<EditRecipe>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLS} FROM edit_recipes WHERE id = ?1"),
                [id],
                map,
            )
            .optional()?)
    }

    pub fn for_asset(&self, asset: AssetId) -> Result<Vec<EditRecipe>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLS} FROM edit_recipes WHERE asset_id = ?1 ORDER BY created_ms, id"
        ))?;
        let rows = st.query_map([asset], map)?.collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

impl Recipes<'_, Rw> {
    pub fn upsert(&self, r: &EditRecipe) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO edit_recipes (id, asset_id, kind, schema_version, rel_path, content_hash, created_ms, updated_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET asset_id = excluded.asset_id, kind = excluded.kind,
               schema_version = excluded.schema_version, rel_path = excluded.rel_path,
               content_hash = excluded.content_hash, created_ms = excluded.created_ms,
               updated_ms = excluded.updated_ms",
            params![
                r.id,
                r.asset_id,
                r.kind,
                r.schema_version,
                r.rel_path.as_str(),
                r.content_hash,
                r.created_ms,
                r.updated_ms
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: RecipeId) -> Result<(), CatalogError> {
        self.conn
            .execute("DELETE FROM edit_recipes WHERE id = ?1", [id])?;
        Ok(())
    }
}

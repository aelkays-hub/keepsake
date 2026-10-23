use rusqlite::{params, OptionalExtension};

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::{Label, Rw};
use crate::relpath::nfc;

define_repo!(
    /// Colour label definitions.
    Labels
);

impl<M> Labels<'_, M> {
    pub fn list(&self) -> Result<Vec<Label>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT id, name, color_hex, sort_order FROM labels ORDER BY sort_order, id",
        )?;
        let rows = st
            .query_map([], |r| {
                Ok(Label {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    color_hex: r.get(2)?,
                    sort_order: r.get(3)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Case-insensitive lookup by name.
    pub fn id_of(&self, name: &str) -> Result<Option<i64>, CatalogError> {
        Ok(self
            .conn
            .query_row("SELECT id FROM labels WHERE name = ?1", [nfc(name)], |r| {
                r.get(0)
            })
            .optional()?)
    }
}

impl Labels<'_, Rw> {
    /// Return the id of `name`, creating a label without a colour if unknown.
    pub fn ensure(&self, name: &str) -> Result<i64, CatalogError> {
        ensure_label(self.conn, name)
    }
}

pub(crate) fn ensure_label(conn: &rusqlite::Connection, name: &str) -> Result<i64, CatalogError> {
    let name = nfc(name.trim());
    if name.is_empty() {
        return Err(CatalogError::Invalid("empty label name".into()));
    }
    if let Some(id) = conn
        .query_row("SELECT id FROM labels WHERE name = ?1", [&name], |r| {
            r.get(0)
        })
        .optional()?
    {
        return Ok(id);
    }
    conn.execute(
        "INSERT INTO labels (name, color_hex, sort_order)
         VALUES (?1, NULL, (SELECT ifnull(max(sort_order), 0) + 1 FROM labels))",
        params![name],
    )?;
    Ok(conn.last_insert_rowid())
}

use rusqlite::{params, OptionalExtension};

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::Rw;

define_repo!(
    /// Key/value library metadata.
    Meta
);

impl<M> Meta<'_, M> {
    pub fn get(&self, key: &str) -> Result<Option<String>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM library_meta WHERE key = ?1",
                [key],
                |r| r.get(0),
            )
            .optional()?)
    }
}

impl Meta<'_, Rw> {
    pub fn set(&self, key: &str, value: &str) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO library_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

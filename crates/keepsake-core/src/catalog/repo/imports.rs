use rusqlite::{params, OptionalExtension, Row};

use super::{define_repo, rel_path_col};
use crate::catalog::error::CatalogError;
use crate::catalog::{
    ImportDestination, ImportItem, ImportSession, ImportSessionId, ImportStatus, Rw,
};

define_repo!(
    /// Import session history.
    Imports
);

const SESSION_COLS: &str = "id, source_volume_id, started_ms, finished_ms, status, verified";

fn map_session(r: &Row<'_>) -> rusqlite::Result<ImportSession> {
    Ok(ImportSession {
        id: r.get(0)?,
        source_volume_id: r.get(1)?,
        started_ms: r.get(2)?,
        finished_ms: r.get(3)?,
        status: r.get(4)?,
        verified: r.get(5)?,
    })
}

impl<M> Imports<'_, M> {
    pub fn get(&self, id: ImportSessionId) -> Result<Option<ImportSession>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {SESSION_COLS} FROM import_sessions WHERE id = ?1"),
                [id],
                map_session,
            )
            .optional()?)
    }

    /// Most recent first.
    pub fn list(&self) -> Result<Vec<ImportSession>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {SESSION_COLS} FROM import_sessions ORDER BY started_ms DESC, id"
        ))?;
        let rows = st.query_map([], map_session)?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn destinations(
        &self,
        session: ImportSessionId,
    ) -> Result<Vec<ImportDestination>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT session_id, volume_id, verified FROM import_destinations
             WHERE session_id = ?1 ORDER BY volume_id",
        )?;
        let rows = st
            .query_map([session], |r| {
                Ok(ImportDestination {
                    session_id: r.get(0)?,
                    volume_id: r.get(1)?,
                    verified: r.get(2)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn items(&self, session: ImportSessionId) -> Result<Vec<ImportItem>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT session_id, source_rel_path, dest_volume_id, file_id, blake3, status, error
               FROM import_items WHERE session_id = ?1 ORDER BY source_rel_path, dest_volume_id",
        )?;
        let rows = st
            .query_map([session], |r| {
                Ok(ImportItem {
                    session_id: r.get(0)?,
                    source_rel_path: rel_path_col(r, 1)?,
                    dest_volume_id: r.get(2)?,
                    file_id: r.get(3)?,
                    blake3: r.get(4)?,
                    status: r.get(5)?,
                    error: r.get(6)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

impl Imports<'_, Rw> {
    pub fn upsert_session(&self, s: &ImportSession) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO import_sessions (id, source_volume_id, started_ms, finished_ms, status, verified)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET source_volume_id = excluded.source_volume_id,
               started_ms = excluded.started_ms, finished_ms = excluded.finished_ms,
               status = excluded.status, verified = excluded.verified",
            params![s.id, s.source_volume_id, s.started_ms, s.finished_ms, s.status, s.verified],
        )?;
        Ok(())
    }

    pub fn finish(
        &self,
        id: ImportSessionId,
        status: ImportStatus,
        verified: bool,
        now_ms: i64,
    ) -> Result<(), CatalogError> {
        let n = self.conn.execute(
            "UPDATE import_sessions SET status = ?2, verified = ?3, finished_ms = ?4 WHERE id = ?1",
            params![id, status, verified, now_ms],
        )?;
        if n == 0 {
            return Err(CatalogError::not_found("import session", id));
        }
        Ok(())
    }

    pub fn upsert_destination(&self, d: &ImportDestination) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO import_destinations (session_id, volume_id, verified) VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id, volume_id) DO UPDATE SET verified = excluded.verified",
            params![d.session_id, d.volume_id, d.verified],
        )?;
        Ok(())
    }

    pub fn upsert_item(&self, i: &ImportItem) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO import_items (session_id, source_rel_path, dest_volume_id, file_id, blake3, status, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(session_id, source_rel_path, dest_volume_id) DO UPDATE SET
               file_id = excluded.file_id, blake3 = excluded.blake3,
               status = excluded.status, error = excluded.error",
            params![
                i.session_id,
                i.source_rel_path.as_str(),
                i.dest_volume_id,
                i.file_id,
                i.blake3,
                i.status,
                i.error
            ],
        )?;
        Ok(())
    }
}

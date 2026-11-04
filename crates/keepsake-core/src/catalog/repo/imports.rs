use rusqlite::{params, OptionalExtension, Row};

use super::{define_repo, rel_path_col, to_i64, u64_col};
use crate::catalog::error::CatalogError;
use crate::catalog::{
    ImportAsset, ImportAssetState, ImportDestination, ImportItem, ImportItemStatus, ImportSession,
    ImportSessionId, ImportStatus, Rw, VolumeId,
};

define_repo!(
    /// Import sessions, their assets (the ingest state machine) and file items.
    Imports
);

const SESSION_COLS: &str =
    "id, source_volume_id, started_ms, finished_ms, status, verified, template, safe_to_format";

fn map_session(r: &Row<'_>) -> rusqlite::Result<ImportSession> {
    Ok(ImportSession {
        id: r.get(0)?,
        source_volume_id: r.get(1)?,
        started_ms: r.get(2)?,
        finished_ms: r.get(3)?,
        status: r.get(4)?,
        verified: r.get(5)?,
        template: r.get(6)?,
        safe_to_format: r.get(7)?,
    })
}

const ASSET_COLS: &str =
    "session_id, source_key, asset_id, diff, state, error, attempts, updated_ms";

fn map_asset(r: &Row<'_>) -> rusqlite::Result<ImportAsset> {
    Ok(ImportAsset {
        session_id: r.get(0)?,
        source_key: r.get(1)?,
        asset_id: r.get(2)?,
        diff: r.get(3)?,
        state: r.get(4)?,
        error: r.get(5)?,
        attempts: r.get(6)?,
        updated_ms: r.get(7)?,
    })
}

const ITEM_COLS: &str = "session_id, source_rel_path, dest_volume_id, source_key, role,
    segment_index, size_bytes, quick_hash, dest_rel_path, file_id, blake3, verified_blake3,
    status, error";

fn map_item(r: &Row<'_>) -> rusqlite::Result<ImportItem> {
    let quick: i64 = r.get(7)?;
    Ok(ImportItem {
        session_id: r.get(0)?,
        source_rel_path: rel_path_col(r, 1)?,
        dest_volume_id: r.get(2)?,
        source_key: r.get(3)?,
        role: r.get(4)?,
        segment_index: r.get(5)?,
        size_bytes: u64_col(r, 6)?,
        // The 64-bit hash is stored bit-for-bit in an INTEGER column.
        quick_hash: quick as u64,
        dest_rel_path: rel_path_col(r, 8)?,
        file_id: r.get(9)?,
        blake3: r.get(10)?,
        verified_blake3: r.get(11)?,
        status: r.get(12)?,
        error: r.get(13)?,
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

    /// Assets of a session in source order.
    pub fn assets(&self, session: ImportSessionId) -> Result<Vec<ImportAsset>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {ASSET_COLS} FROM import_assets WHERE session_id = ?1 ORDER BY source_key"
        ))?;
        let rows = st
            .query_map([session], map_asset)?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn asset(
        &self,
        session: ImportSessionId,
        source_key: &str,
    ) -> Result<Option<ImportAsset>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT {ASSET_COLS} FROM import_assets WHERE session_id = ?1 AND source_key = ?2"
                ),
                params![session, source_key],
                map_asset,
            )
            .optional()?)
    }

    pub fn items(&self, session: ImportSessionId) -> Result<Vec<ImportItem>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {ITEM_COLS} FROM import_items WHERE session_id = ?1
             ORDER BY source_key, source_rel_path, dest_volume_id"
        ))?;
        let rows = st
            .query_map([session], map_item)?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Items of one asset.
    pub fn asset_items(
        &self,
        session: ImportSessionId,
        source_key: &str,
    ) -> Result<Vec<ImportItem>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {ITEM_COLS} FROM import_items WHERE session_id = ?1 AND source_key = ?2
             ORDER BY source_rel_path, dest_volume_id"
        ))?;
        let rows = st
            .query_map(params![session, source_key], map_item)?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Items from earlier sessions that copied `source_rel_path` from `volume`.
    pub fn earlier_items(
        &self,
        source_volume: VolumeId,
        source_rel_path: &str,
        except: ImportSessionId,
    ) -> Result<Vec<ImportItem>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {ITEM_COLS} FROM import_items i
              WHERE i.source_rel_path = ?2 AND i.session_id <> ?3
                AND i.session_id IN (SELECT id FROM import_sessions WHERE source_volume_id = ?1)"
        ))?;
        let rows = st
            .query_map(params![source_volume, source_rel_path, except], map_item)?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

impl Imports<'_, Rw> {
    pub fn upsert_session(&self, s: &ImportSession) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO import_sessions (id, source_volume_id, started_ms, finished_ms, status,
                                          verified, template, safe_to_format)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET source_volume_id = excluded.source_volume_id,
               started_ms = excluded.started_ms, finished_ms = excluded.finished_ms,
               status = excluded.status, verified = excluded.verified,
               template = excluded.template, safe_to_format = excluded.safe_to_format",
            params![
                s.id,
                s.source_volume_id,
                s.started_ms,
                s.finished_ms,
                s.status,
                s.verified,
                s.template,
                s.safe_to_format
            ],
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

    pub fn set_session_status(
        &self,
        id: ImportSessionId,
        status: ImportStatus,
        safe_to_format: bool,
        finished_ms: Option<i64>,
    ) -> Result<(), CatalogError> {
        self.conn.execute(
            "UPDATE import_sessions SET status = ?2, safe_to_format = ?3, verified = ?3,
                    finished_ms = ?4 WHERE id = ?1",
            params![id, status, safe_to_format, finished_ms],
        )?;
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

    pub fn upsert_asset(&self, a: &ImportAsset) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO import_assets (session_id, source_key, asset_id, diff, state, error,
                                        attempts, updated_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(session_id, source_key) DO UPDATE SET asset_id = excluded.asset_id,
               diff = excluded.diff, state = excluded.state, error = excluded.error,
               attempts = excluded.attempts, updated_ms = excluded.updated_ms",
            params![
                a.session_id,
                a.source_key,
                a.asset_id,
                a.diff,
                a.state,
                a.error,
                a.attempts,
                a.updated_ms
            ],
        )?;
        Ok(())
    }

    /// Move an asset to `state` (clearing or setting its error).
    pub fn set_asset_state(
        &self,
        session: ImportSessionId,
        source_key: &str,
        state: ImportAssetState,
        error: Option<&str>,
        now_ms: i64,
    ) -> Result<(), CatalogError> {
        let n = self.conn.execute(
            "UPDATE import_assets SET state = ?3, error = ?4, updated_ms = ?5,
                    attempts = attempts + CASE WHEN ?3 = 'copying' THEN 1 ELSE 0 END
              WHERE session_id = ?1 AND source_key = ?2",
            params![session, source_key, state, error, now_ms],
        )?;
        if n == 0 {
            return Err(CatalogError::not_found("import asset", source_key));
        }
        Ok(())
    }

    pub fn upsert_item(&self, i: &ImportItem) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO import_items (session_id, source_rel_path, dest_volume_id, source_key,
                role, segment_index, size_bytes, quick_hash, dest_rel_path, file_id, blake3,
                verified_blake3, status, error)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
             ON CONFLICT(session_id, source_rel_path, dest_volume_id) DO UPDATE SET
               source_key = excluded.source_key, role = excluded.role,
               segment_index = excluded.segment_index, size_bytes = excluded.size_bytes,
               quick_hash = excluded.quick_hash, dest_rel_path = excluded.dest_rel_path,
               file_id = excluded.file_id, blake3 = excluded.blake3,
               verified_blake3 = excluded.verified_blake3, status = excluded.status,
               error = excluded.error",
            params![
                i.session_id,
                i.source_rel_path.as_str(),
                i.dest_volume_id,
                i.source_key,
                i.role,
                i.segment_index,
                to_i64(i.size_bytes, "size")?,
                i.quick_hash as i64,
                i.dest_rel_path.as_str(),
                i.file_id,
                i.blake3,
                i.verified_blake3,
                i.status,
                i.error
            ],
        )?;
        Ok(())
    }

    /// Update the copy outcome of one item.
    #[allow(clippy::too_many_arguments)]
    pub fn set_item(
        &self,
        session: ImportSessionId,
        source_rel_path: &str,
        dest: VolumeId,
        status: ImportItemStatus,
        blake3: Option<&crate::catalog::Blake3Hash>,
        verified: Option<&crate::catalog::Blake3Hash>,
        error: Option<&str>,
    ) -> Result<(), CatalogError> {
        let n = self.conn.execute(
            "UPDATE import_items SET status = ?4,
                    blake3 = coalesce(?5, blake3), verified_blake3 = ?6, error = ?7
              WHERE session_id = ?1 AND source_rel_path = ?2 AND dest_volume_id = ?3",
            params![
                session,
                source_rel_path,
                dest,
                status,
                blake3,
                verified,
                error
            ],
        )?;
        if n == 0 {
            return Err(CatalogError::not_found("import item", source_rel_path));
        }
        Ok(())
    }

    pub fn set_item_file(
        &self,
        session: ImportSessionId,
        source_rel_path: &str,
        dest: VolumeId,
        file: crate::catalog::FileId,
    ) -> Result<(), CatalogError> {
        self.conn.execute(
            "UPDATE import_items SET file_id = ?4
              WHERE session_id = ?1 AND source_rel_path = ?2 AND dest_volume_id = ?3",
            params![session, source_rel_path, dest, file],
        )?;
        Ok(())
    }
}

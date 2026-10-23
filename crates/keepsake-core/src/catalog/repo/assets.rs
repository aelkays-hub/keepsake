use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::repo::labels::ensure_label;
use crate::catalog::{fts, Asset, AssetId, CaptureTime, EventId, Rating, Rw};

define_repo!(
    /// Logical assets.
    Assets
);

/// Number of assets captured on one local calendar day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayCount {
    /// `YYYY-MM-DD` in the capture's local time.
    pub date: String,
    pub count: u64,
}

const SELECT: &str = "SELECT a.id, a.media_kind, a.capture_ts_ms, a.capture_utc_offset_min,
       a.capture_shift_s, a.duration_ms, a.camera_make, a.camera_model, a.lens_model,
       a.event_id, a.rating, l.name, a.comment, a.created_ms, a.metadata_ms
  FROM assets a LEFT JOIN labels l ON l.id = a.label_id";

fn map(r: &Row<'_>) -> rusqlite::Result<Asset> {
    let ts: Option<i64> = r.get(2)?;
    let off: Option<i16> = r.get(3)?;
    let rating: u8 = r.get(10)?;
    Ok(Asset {
        id: r.get(0)?,
        media_kind: r.get(1)?,
        capture: ts.map(|utc_ms| CaptureTime {
            utc_ms,
            utc_offset_min: off,
        }),
        capture_shift_s: r.get(4)?,
        duration_ms: r.get(5)?,
        camera_make: r.get(6)?,
        camera_model: r.get(7)?,
        lens_model: r.get(8)?,
        event_id: r.get(9)?,
        rating: Rating::from_xmp(i64::from(rating)),
        label: r.get(11)?,
        comment: r.get(12)?,
        created_ms: r.get(13)?,
        metadata_ms: r.get(14)?,
    })
}

impl<M> Assets<'_, M> {
    pub fn get(&self, id: AssetId) -> Result<Option<Asset>, CatalogError> {
        Ok(self
            .conn
            .query_row(&format!("{SELECT} WHERE a.id = ?1"), [id], map)
            .optional()?)
    }

    pub fn require(&self, id: AssetId) -> Result<Asset, CatalogError> {
        self.get(id)?
            .ok_or_else(|| CatalogError::not_found("asset", id))
    }

    pub fn count(&self) -> Result<u64, CatalogError> {
        let n: i64 = self
            .conn
            .query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?;
        super::to_u64(n, "count")
    }

    /// All asset ids, oldest capture first (unknown dates last).
    pub fn list_ids(&self) -> Result<Vec<AssetId>, CatalogError> {
        let mut st = self
            .conn
            .prepare("SELECT id FROM assets ORDER BY capture_ts_ms IS NULL, capture_ts_ms, id")?;
        let rows = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Assets captured between two local dates (inclusive, `YYYY-MM-DD`).
    pub fn by_local_date(&self, from: &str, to: &str) -> Result<Vec<Asset>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "{SELECT} WHERE a.capture_local_date BETWEEN ?1 AND ?2
             ORDER BY a.capture_ts_ms, a.id"
        ))?;
        let rows = st
            .query_map(params![from, to], map)?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Asset counts per local calendar day (drives the calendar view).
    pub fn days(&self) -> Result<Vec<DayCount>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT capture_local_date, count(*) FROM assets
             WHERE capture_local_date IS NOT NULL
             GROUP BY capture_local_date ORDER BY capture_local_date",
        )?;
        let rows = st
            .query_map([], |r| {
                Ok(DayCount {
                    date: r.get(0)?,
                    count: super::u64_col(r, 1)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn in_event(&self, event: EventId) -> Result<Vec<AssetId>, CatalogError> {
        let mut st = self
            .conn
            .prepare("SELECT id FROM assets WHERE event_id = ?1 ORDER BY capture_ts_ms, id")?;
        let rows = st
            .query_map([event], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

impl Assets<'_, Rw> {
    pub fn insert(&self, a: &Asset) -> Result<(), CatalogError> {
        let label_id = a
            .label
            .as_deref()
            .map(|l| ensure_label(self.conn, l))
            .transpose()?;
        self.conn.execute(
            "INSERT INTO assets (id, media_kind, capture_ts_ms, capture_utc_offset_min,
                capture_shift_s, duration_ms, camera_make, camera_model, lens_model,
                event_id, rating, label_id, comment, created_ms, metadata_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                a.id,
                a.media_kind,
                a.capture.map(|c| c.utc_ms),
                a.capture.and_then(|c| c.utc_offset_min),
                a.capture_shift_s,
                a.duration_ms,
                a.camera_make,
                a.camera_model,
                a.lens_model,
                a.event_id,
                a.rating.get(),
                label_id,
                a.comment,
                a.created_ms,
                a.metadata_ms
            ],
        )?;
        fts::reindex(self.conn, a.id)
    }

    /// Replace every column of an existing asset.
    pub fn update(&self, a: &Asset) -> Result<(), CatalogError> {
        let label_id = a
            .label
            .as_deref()
            .map(|l| ensure_label(self.conn, l))
            .transpose()?;
        let n = self.conn.execute(
            "UPDATE assets SET media_kind = ?2, capture_ts_ms = ?3, capture_utc_offset_min = ?4,
                capture_shift_s = ?5, duration_ms = ?6, camera_make = ?7, camera_model = ?8,
                lens_model = ?9, event_id = ?10, rating = ?11, label_id = ?12, comment = ?13,
                created_ms = ?14, metadata_ms = ?15
             WHERE id = ?1",
            params![
                a.id,
                a.media_kind,
                a.capture.map(|c| c.utc_ms),
                a.capture.and_then(|c| c.utc_offset_min),
                a.capture_shift_s,
                a.duration_ms,
                a.camera_make,
                a.camera_model,
                a.lens_model,
                a.event_id,
                a.rating.get(),
                label_id,
                a.comment,
                a.created_ms,
                a.metadata_ms
            ],
        )?;
        if n == 0 {
            return Err(CatalogError::not_found("asset", a.id));
        }
        fts::reindex(self.conn, a.id)
    }

    /// Remove an asset and (by cascade) its files, tags, recipes and sidecar
    /// records from the catalog. Never touches anything on disk.
    pub fn delete(&self, id: AssetId) -> Result<(), CatalogError> {
        fts::remove(self.conn, id)?;
        self.conn
            .execute("DELETE FROM assets WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Delete every asset that no longer has any file. Returns how many.
    pub fn delete_orphans(&self) -> Result<u64, CatalogError> {
        self.conn.execute(
            "DELETE FROM asset_fts WHERE rowid IN (
               SELECT seq FROM assets a WHERE NOT EXISTS (SELECT 1 FROM files f WHERE f.asset_id = a.id))",
            [],
        )?;
        let n = self.conn.execute(
            "DELETE FROM assets WHERE NOT EXISTS (SELECT 1 FROM files f WHERE f.asset_id = assets.id)",
            [],
        )?;
        Ok(n as u64)
    }
}

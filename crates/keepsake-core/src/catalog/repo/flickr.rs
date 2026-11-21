use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::{AssetId, Rw};

define_repo!(
    /// Flickr upload (ADR-046): the photos uploaded and those waiting.
    FlickrRepo
);

/// One photo uploaded to Flickr.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlickrUpload {
    pub asset_id: AssetId,
    pub user_nsid: String,
    pub photo_id: String,
    pub url: String,
    /// `public`, `friends_family`, `friends`, `family` or `private`.
    pub privacy: String,
    pub tags: Vec<String>,
    pub uploaded_ms: i64,
}

/// What the user chose for an upload still to be done.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FlickrQueued {
    pub asset_id: AssetId,
    pub privacy: String,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub queued_ms: i64,
}

fn tags_col(r: &Row<'_>, i: usize) -> rusqlite::Result<Vec<String>> {
    let s: String = r.get(i)?;
    serde_json::from_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn tags_json(tags: &[String]) -> Result<String, CatalogError> {
    serde_json::to_string(tags).map_err(|e| CatalogError::Invalid(e.to_string()))
}

fn upload_row(r: &Row<'_>) -> rusqlite::Result<FlickrUpload> {
    Ok(FlickrUpload {
        asset_id: r.get(0)?,
        user_nsid: r.get(1)?,
        photo_id: r.get(2)?,
        url: r.get(3)?,
        privacy: r.get(4)?,
        tags: tags_col(r, 5)?,
        uploaded_ms: r.get(6)?,
    })
}

const UPLOAD_COLS: &str = "asset_id, user_nsid, photo_id, url, privacy, tags, uploaded_ms";

impl<M> FlickrRepo<'_, M> {
    /// Every upload of `asset`, newest first.
    pub fn uploads(&self, asset: AssetId) -> Result<Vec<FlickrUpload>, CatalogError> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {UPLOAD_COLS} FROM flickr_uploads WHERE asset_id = ?1
              ORDER BY uploaded_ms DESC, id DESC"
        ))?;
        let rows = st.query_map([asset], upload_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The upload waiting for `asset`, if any.
    pub fn queued(&self, asset: AssetId) -> Result<Option<FlickrQueued>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                "SELECT asset_id, privacy, title, description, tags, queued_ms
                   FROM flickr_queue WHERE asset_id = ?1",
                [asset],
                |r| {
                    Ok(FlickrQueued {
                        asset_id: r.get(0)?,
                        privacy: r.get(1)?,
                        title: r.get(2)?,
                        description: r.get(3)?,
                        tags: tags_col(r, 4)?,
                        queued_ms: r.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    /// Every upload, newest first (Report → Flickr).
    pub fn all_uploads(&self) -> Result<Vec<FlickrUpload>, CatalogError> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {UPLOAD_COLS} FROM flickr_uploads ORDER BY uploaded_ms DESC, id DESC"
        ))?;
        let rows = st.query_map([], upload_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// How many photos have been uploaded (at least once).
    pub fn uploaded_count(&self) -> Result<u64, CatalogError> {
        let n: i64 = self.conn.query_row(
            "SELECT count(DISTINCT asset_id) FROM flickr_uploads",
            [],
            |r| r.get(0),
        )?;
        Ok(u64::try_from(n).unwrap_or(0))
    }
}

impl FlickrRepo<'_, Rw> {
    /// Keep what the user chose for `q.asset_id`'s upload (replacing an
    /// earlier choice).
    pub fn put_queued(&self, q: &FlickrQueued) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO flickr_queue (asset_id, privacy, title, description, tags, queued_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(asset_id) DO UPDATE SET privacy = excluded.privacy,
               title = excluded.title, description = excluded.description,
               tags = excluded.tags, queued_ms = excluded.queued_ms",
            params![
                q.asset_id,
                q.privacy,
                q.title,
                q.description,
                tags_json(&q.tags)?,
                q.queued_ms
            ],
        )?;
        Ok(())
    }

    pub fn remove_queued(&self, asset: AssetId) -> Result<(), CatalogError> {
        self.conn
            .execute("DELETE FROM flickr_queue WHERE asset_id = ?1", [asset])?;
        Ok(())
    }

    /// Record an upload and forget what was waiting for it.
    pub fn record(&self, u: &FlickrUpload) -> Result<(), CatalogError> {
        self.conn.execute(
            &format!(
                "INSERT INTO flickr_uploads ({UPLOAD_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(user_nsid, photo_id) DO NOTHING"
            ),
            params![
                u.asset_id,
                u.user_nsid,
                u.photo_id,
                u.url,
                u.privacy,
                tags_json(&u.tags)?,
                u.uploaded_ms
            ],
        )?;
        self.remove_queued(u.asset_id)
    }
}

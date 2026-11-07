use rusqlite::{params, OptionalExtension};

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::{AiAnalysis, AssetId, DuplicateChoice, MediaKind, Rw, SequenceId, TagPath};

define_repo!(
    /// On-device AI results and the user's decisions about them (ADR-035).
    Ai
);

/// One analysed asset of a day, for finding duplicates.
#[derive(Debug, Clone, PartialEq)]
pub struct DayItem {
    pub analysis: AiAnalysis,
    pub media_kind: MediaKind,
    /// Capture time (UTC ms), if known.
    pub capture_ms: Option<i64>,
    pub rating: i64,
    /// The burst, bracket or interval run it belongs to.
    pub sequence: Option<SequenceId>,
    pub choice: Option<DuplicateChoice>,
}

/// One asset of a date range, for "Find duplicates" and "Tag photos".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeItem {
    pub id: AssetId,
    /// It has a RAW file (only those are compared for duplicates).
    pub raw: bool,
    /// Analysed with the current model.
    pub analysed: bool,
    /// The tag list version its AI tags came from (0: not tagged).
    pub tag_list: u32,
}

/// How far the analysis has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct AiCounts {
    /// Assets analysed with the current model.
    pub analysed: u64,
    /// All assets.
    pub assets: u64,
    /// `ai.analyze` jobs still to run (queued or running).
    pub pending: u64,
    /// `ai.analyze` jobs that failed for good.
    pub failed: u64,
}

fn i8_blob(v: &[i8]) -> Vec<u8> {
    v.iter().map(|b| b.to_ne_bytes()[0]).collect()
}

fn blob_i8(v: Vec<u8>) -> Vec<i8> {
    v.into_iter().map(|b| i8::from_ne_bytes([b])).collect()
}

fn analysis_row(r: &rusqlite::Row<'_>, at: usize) -> rusqlite::Result<AiAnalysis> {
    let dhash: i64 = r.get(at + 4)?;
    let scale: f64 = r.get(at + 3)?;
    Ok(AiAnalysis {
        asset_id: r.get(at)?,
        model: r.get(at + 1)?,
        embedding: blob_i8(r.get(at + 2)?),
        emb_scale: scale as f32,
        dhash: u64::from_ne_bytes(dhash.to_ne_bytes()),
        made_ms: r.get(at + 5)?,
        tag_list: r.get(at + 6)?,
    })
}

const ANALYSIS_COLS: &str =
    "x.asset_id, x.model, x.embedding, x.emb_scale, x.dhash, x.made_ms, x.tag_list";

impl<M> Ai<'_, M> {
    pub fn analysis(&self, asset: AssetId) -> Result<Option<AiAnalysis>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {ANALYSIS_COLS} FROM ai_analysis x WHERE x.asset_id = ?1"),
                [asset],
                |r| analysis_row(r, 0),
            )
            .optional()?)
    }

    /// Analysed assets captured on `date` (`YYYY-MM-DD`, local time) with
    /// `model`, in capture order; with `raw_only`, only those with a RAW file.
    pub fn day_items(
        &self,
        date: &str,
        model: &str,
        raw_only: bool,
    ) -> Result<Vec<DayItem>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {ANALYSIS_COLS}, a.media_kind, a.capture_ts_ms, a.rating,
                    s.sequence_id, c.choice
               FROM assets a
               JOIN ai_analysis x ON x.asset_id = a.id
               LEFT JOIN asset_sequences s ON s.asset_id = a.id
               LEFT JOIN duplicate_choices c ON c.asset_id = a.id
              WHERE a.capture_local_date = ?1 AND x.model = ?2 AND (?3 = 0 OR a.is_raw = 1)
              ORDER BY a.capture_ts_ms, a.id"
        ))?;
        let rows = st
            .query_map(params![date, model, raw_only], |r| {
                Ok(DayItem {
                    analysis: analysis_row(r, 0)?,
                    media_kind: r.get(7)?,
                    capture_ms: r.get(8)?,
                    rating: r.get(9)?,
                    sequence: r.get(10)?,
                    choice: r.get(11)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Days (`YYYY-MM-DD`) from `from` to `to` (inclusive; `None`: no
    /// bound) with at least two assets analysed with `model` (with a RAW
    /// file, if `raw_only`), newest first.
    pub fn days_with_pairs(
        &self,
        model: &str,
        from: Option<&str>,
        to: Option<&str>,
        raw_only: bool,
    ) -> Result<Vec<String>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT a.capture_local_date
               FROM assets a JOIN ai_analysis x ON x.asset_id = a.id
              WHERE a.capture_local_date IS NOT NULL AND x.model = ?1
                AND a.capture_local_date >= ifnull(?2, '')
                AND a.capture_local_date <= ifnull(?3, '9999')
                AND (?4 = 0 OR a.is_raw = 1)
              GROUP BY a.capture_local_date HAVING count(*) >= 2
              ORDER BY a.capture_local_date DESC",
        )?;
        let rows = st
            .query_map(params![model, from, to, raw_only], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Assets captured from `from` to `to` (`YYYY-MM-DD`, local,
    /// inclusive), newest first.
    pub fn in_dates(
        &self,
        from: &str,
        to: &str,
        model: &str,
    ) -> Result<Vec<RangeItem>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT a.id, a.is_raw, x.tag_list
               FROM assets a
               LEFT JOIN ai_analysis x ON x.asset_id = a.id AND x.model = ?3
              WHERE a.capture_local_date BETWEEN ?1 AND ?2
              ORDER BY a.capture_ts_ms DESC, a.id",
        )?;
        let rows = st
            .query_map(params![from, to, model], |r| {
                let list: Option<u32> = r.get(2)?;
                Ok(RangeItem {
                    id: r.get(0)?,
                    raw: r.get(1)?,
                    analysed: list.is_some(),
                    tag_list: list.unwrap_or(0),
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Assets not yet analysed with `model`.
    pub fn without_analysis(&self, model: &str) -> Result<Vec<AssetId>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT a.id FROM assets a
              WHERE NOT EXISTS (SELECT 1 FROM ai_analysis x
                                 WHERE x.asset_id = a.id AND x.model = ?1)
              ORDER BY a.capture_ts_ms DESC, a.id",
        )?;
        let rows = st
            .query_map([model], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Assets analysed with `model` and tagged from it.
    pub fn analysed(&self, model: &str) -> Result<Vec<AssetId>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT asset_id FROM ai_analysis WHERE model = ?1 AND tag_list > 0 ORDER BY asset_id",
        )?;
        let rows = st
            .query_map([model], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn counts(&self, model: &str, job_kind: &str) -> Result<AiCounts, CatalogError> {
        let analysed: i64 = self.conn.query_row(
            "SELECT count(*) FROM ai_analysis WHERE model = ?1",
            [model],
            |r| r.get(0),
        )?;
        let assets: i64 = self
            .conn
            .query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?;
        let (pending, failed): (i64, i64) = self.conn.query_row(
            "SELECT coalesce(sum(state IN ('queued','running')), 0),
                    coalesce(sum(state = 'failed'), 0)
               FROM jobs WHERE kind = ?1",
            [job_kind],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        Ok(AiCounts {
            analysed: super::to_u64(analysed, "count")?,
            assets: super::to_u64(assets, "count")?,
            pending: super::to_u64(pending, "count")?,
            failed: super::to_u64(failed, "count")?,
        })
    }

    /// AI tags the user removed from an asset, sorted.
    pub fn rejections(&self, asset: AssetId) -> Result<Vec<TagPath>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT tag_path FROM ai_tag_rejections WHERE asset_id = ?1 ORDER BY tag_path",
        )?;
        let raw: Vec<String> = st
            .query_map([asset], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        raw.iter().map(|p| TagPath::parse_lightroom(p)).collect()
    }

    pub fn choice(&self, asset: AssetId) -> Result<Option<DuplicateChoice>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                "SELECT choice FROM duplicate_choices WHERE asset_id = ?1",
                [asset],
                |r| r.get(0),
            )
            .optional()?)
    }
}

impl Ai<'_, Rw> {
    /// Store (or replace) an asset's analysis.
    pub fn put_analysis(&self, a: &AiAnalysis) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO ai_analysis
               (asset_id, model, embedding, emb_scale, dhash, made_ms, tagged, tag_list)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7 > 0, ?7)
             ON CONFLICT(asset_id) DO UPDATE SET model = excluded.model,
               embedding = excluded.embedding, emb_scale = excluded.emb_scale,
               dhash = excluded.dhash, made_ms = excluded.made_ms,
               tagged = excluded.tagged, tag_list = excluded.tag_list",
            params![
                a.asset_id,
                a.model,
                i8_blob(&a.embedding),
                f64::from(a.emb_scale),
                i64::from_ne_bytes(a.dhash.to_ne_bytes()),
                a.made_ms,
                a.tag_list
            ],
        )?;
        Ok(())
    }

    /// Record that an asset's AI tags now come from tag list `list`.
    pub fn set_tag_list(&self, asset: AssetId, list: u32) -> Result<(), CatalogError> {
        self.conn.execute(
            "UPDATE ai_analysis SET tagged = ?2 > 0, tag_list = ?2 WHERE asset_id = ?1",
            params![asset, list],
        )?;
        Ok(())
    }

    /// Replace the AI tags the user removed from an asset.
    pub fn set_rejections(&self, asset: AssetId, paths: &[TagPath]) -> Result<(), CatalogError> {
        self.conn
            .execute("DELETE FROM ai_tag_rejections WHERE asset_id = ?1", [asset])?;
        for p in paths {
            self.conn.execute(
                "INSERT OR IGNORE INTO ai_tag_rejections (asset_id, tag_path) VALUES (?1, ?2)",
                params![asset, p.to_lightroom()],
            )?;
        }
        Ok(())
    }

    /// Record (or clear, with `None`) the user's duplicate decision.
    pub fn set_choice(
        &self,
        asset: AssetId,
        choice: Option<DuplicateChoice>,
        now_ms: i64,
    ) -> Result<(), CatalogError> {
        match choice {
            Some(c) => self.conn.execute(
                "INSERT INTO duplicate_choices (asset_id, choice, made_ms) VALUES (?1, ?2, ?3)
                 ON CONFLICT(asset_id) DO UPDATE SET choice = excluded.choice,
                   made_ms = excluded.made_ms",
                params![asset, c, now_ms],
            )?,
            None => self
                .conn
                .execute("DELETE FROM duplicate_choices WHERE asset_id = ?1", [asset])?,
        };
        Ok(())
    }
}

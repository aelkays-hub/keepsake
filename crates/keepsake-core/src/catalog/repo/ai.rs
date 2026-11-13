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

/// What Read aircraft found in a photo, kept for review (ADR-041).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AircraftReadRow {
    pub asset_id: AssetId,
    /// JSON written by `ai::aircraft::marks::Candidates`.
    pub candidates: String,
    pub looks_like: Option<String>,
    pub full_size: bool,
    pub made_ms: i64,
    /// `waiting`, `accepted` or `dismissed`.
    pub review: String,
    pub reviewed_ms: Option<i64>,
}

fn aircraft_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AircraftReadRow> {
    Ok(AircraftReadRow {
        asset_id: r.get(0)?,
        candidates: r.get(1)?,
        looks_like: r.get(2)?,
        full_size: r.get::<_, i64>(3)? != 0,
        made_ms: r.get(4)?,
        review: r.get(5)?,
        reviewed_ms: r.get(6)?,
    })
}

const AIRCRAFT_COLS: &str =
    "asset_id, candidates, looks_like, full_size, made_ms, review, reviewed_ms";

/// A photo's stored edit suggestions (ADR-042): JSON kept by
/// `ai::suggest`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditAdviceRow {
    /// What was measured.
    pub measures: String,
    pub made_ms: i64,
    /// What an AI advised, once asked.
    pub ai: Option<String>,
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
    /// What an online lookup said about `registration` (JSON), if asked.
    pub fn aircraft_lookup(&self, registration: &str) -> Result<Option<String>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                "SELECT found FROM aircraft_lookups WHERE registration = ?1",
                [registration],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The other items taken by the same camera (make and model) within
    /// `window_ms` of `asset`, as `(id, milliseconds after it)`; nearest
    /// first. None when `asset` has no capture time.
    pub fn taken_near(
        &self,
        asset: AssetId,
        window_ms: i64,
    ) -> Result<Vec<(AssetId, i64)>, CatalogError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT o.id, o.capture_ts_ms - a.capture_ts_ms FROM assets a
               JOIN assets o ON o.capture_ts_ms BETWEEN a.capture_ts_ms - ?2
                                                    AND a.capture_ts_ms + ?2
                AND o.id <> a.id
                AND o.camera_make IS a.camera_make AND o.camera_model IS a.camera_model
              WHERE a.id = ?1 AND a.capture_ts_ms IS NOT NULL
              ORDER BY abs(o.capture_ts_ms - a.capture_ts_ms), o.capture_ts_ms",
        )?;
        let rows = stmt.query_map(params![asset, window_ms], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// What Read aircraft found in `asset`, if it has been read with this
    /// version.
    pub fn aircraft_read(&self, asset: AssetId) -> Result<Option<AircraftReadRow>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {AIRCRAFT_COLS} FROM aircraft_reads WHERE asset_id = ?1"),
                [asset],
                aircraft_row,
            )
            .optional()?)
    }

    /// The asset's stored edit suggestions.
    pub fn edit_advice(&self, asset: AssetId) -> Result<Option<EditAdviceRow>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                "SELECT measures, made_ms, ai FROM edit_suggestions WHERE asset_id = ?1",
                [asset],
                |r| {
                    Ok(EditAdviceRow {
                        measures: r.get(0)?,
                        made_ms: r.get(1)?,
                        ai: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

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

    /// Assets matching `q` (a date range, a folder, …), newest first.
    pub fn matching(
        &self,
        q: &crate::catalog::SearchQuery,
        model: &str,
    ) -> Result<Vec<RangeItem>, CatalogError> {
        let mut f = super::search::filter_sql_in(q)?;
        if f.empty {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT a.id, a.is_raw, x.tag_list
               FROM assets a
               LEFT JOIN ai_analysis x ON x.asset_id = a.id AND x.model = ?{}{}
              ORDER BY a.capture_ts_ms DESC, a.id",
            f.joins,
            f.where_sql()
        );
        f.args.insert(0, Box::new(model.to_owned()));
        let mut st = self.conn.prepare(&sql)?;
        let rows = st
            .query_map(rusqlite::params_from_iter(f.args.iter()), |r| {
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

    /// The keywords each AI gave an asset with Tag with AI, as (provider
    /// id, keyword), sorted by provider then keyword (ADR-039).
    pub fn tag_sources(&self, asset: AssetId) -> Result<Vec<(String, TagPath)>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT provider, tag_path FROM ai_tag_sources WHERE asset_id = ?1
              ORDER BY provider, tag_path",
        )?;
        let raw: Vec<(String, String)> = st
            .query_map([asset], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        raw.into_iter()
            .map(|(p, t)| TagPath::parse_lightroom(&t).map(|t| (p, t)))
            .collect()
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
    /// Keep what an online lookup said about `registration`.
    pub fn put_aircraft_lookup(
        &self,
        registration: &str,
        found: &str,
        at_ms: i64,
    ) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO aircraft_lookups (registration, found, looked_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(registration) DO UPDATE SET found = excluded.found,
               looked_ms = excluded.looked_ms",
            params![registration, found, at_ms],
        )?;
        Ok(())
    }

    /// Keep what Read aircraft found in `asset`, waiting for review (a
    /// read before is replaced).
    pub fn put_aircraft_read(
        &self,
        asset: AssetId,
        candidates: &str,
        looks_like: Option<&str>,
        full_size: bool,
        made_ms: i64,
    ) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO aircraft_reads (asset_id, candidates, looks_like, full_size, made_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(asset_id) DO UPDATE SET candidates = excluded.candidates,
               looks_like = excluded.looks_like, full_size = excluded.full_size,
               made_ms = excluded.made_ms, review = 'waiting', reviewed_ms = NULL",
            params![asset, candidates, looks_like, i64::from(full_size), made_ms],
        )?;
        Ok(())
    }

    /// Mark a read accepted or dismissed. Returns whether there was one.
    pub fn set_aircraft_review(
        &self,
        asset: AssetId,
        review: &str,
        at_ms: i64,
    ) -> Result<bool, CatalogError> {
        Ok(self.conn.execute(
            "UPDATE aircraft_reads SET review = ?2, reviewed_ms = ?3 WHERE asset_id = ?1",
            params![asset, review, at_ms],
        )? > 0)
    }

    /// Store an asset's measurements; with `keep_ai` false, the AI's advice
    /// for the old ones is dropped.
    pub fn put_edit_measures(
        &self,
        asset: AssetId,
        measures: &str,
        made_ms: i64,
        keep_ai: bool,
    ) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO edit_suggestions (asset_id, measures, made_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT(asset_id) DO UPDATE SET measures = excluded.measures,
               made_ms = excluded.made_ms, ai = CASE WHEN ?4 THEN ai ELSE NULL END",
            params![asset, measures, made_ms, keep_ai],
        )?;
        Ok(())
    }

    /// Store what an AI advised for an asset that has measurements.
    /// Returns whether there was a row to put it in.
    pub fn put_edit_ai(&self, asset: AssetId, ai: &str) -> Result<bool, CatalogError> {
        Ok(self.conn.execute(
            "UPDATE edit_suggestions SET ai = ?2 WHERE asset_id = ?1",
            params![asset, ai],
        )? > 0)
    }

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

    /// Record that `provider` gave an asset `paths` (those recorded stay).
    pub fn add_tag_sources(
        &self,
        asset: AssetId,
        provider: &str,
        paths: &[TagPath],
    ) -> Result<(), CatalogError> {
        for p in paths {
            self.conn.execute(
                "INSERT OR IGNORE INTO ai_tag_sources (asset_id, provider, tag_path)
                 VALUES (?1, ?2, ?3)",
                params![asset, provider, p.to_lightroom()],
            )?;
        }
        Ok(())
    }

    /// Replace which AI gave which keyword of an asset.
    pub fn set_tag_sources(
        &self,
        asset: AssetId,
        sources: &[(String, TagPath)],
    ) -> Result<(), CatalogError> {
        self.conn
            .execute("DELETE FROM ai_tag_sources WHERE asset_id = ?1", [asset])?;
        for (provider, p) in sources {
            self.add_tag_sources(asset, provider, std::slice::from_ref(p))?;
        }
        Ok(())
    }

    /// Forget the sources of keywords the asset no longer has (`keywords`:
    /// those it has).
    pub fn prune_tag_sources(
        &self,
        asset: AssetId,
        keywords: &[TagPath],
    ) -> Result<(), CatalogError> {
        let kept: Vec<(String, TagPath)> = self
            .tag_sources(asset)?
            .into_iter()
            .filter(|(_, p)| keywords.contains(p))
            .collect();
        self.set_tag_sources(asset, &kept)
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

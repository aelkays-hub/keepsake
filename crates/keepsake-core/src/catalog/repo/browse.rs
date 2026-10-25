//! Browsing: counts, pages, ranges and calendar buckets over a filtered,
//! sorted view of the library. Built for 300,000 assets: every query is
//! one indexed SQL statement plus, for a page, one lookup of its files.

use std::collections::HashMap;

use rusqlite::{OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::define_repo;
use super::search::{filter_sql, FilterSql, SearchQuery};
use crate::catalog::error::CatalogError;
use crate::catalog::{AssetId, Blake3Hash, MediaKind, VolumeId};

define_repo!(
    /// Browsing views over the library.
    Browse
);

/// What to sort by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortField {
    #[default]
    Date,
    Name,
    Camera,
    Lens,
    Type,
    Size,
    Duration,
    Rating,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sort {
    pub field: SortField,
    #[serde(default)]
    pub descending: bool,
}

/// One row of a grid or details view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BrowseRow {
    pub id: AssetId,
    /// The original's file name.
    pub name: Option<String>,
    /// UTC capture instant (ms) and the camera's offset (minutes).
    pub capture_ms: Option<i64>,
    pub utc_offset_min: Option<i16>,
    pub camera: Option<String>,
    pub lens: Option<String>,
    pub kind: MediaKind,
    pub raw: bool,
    pub size: Option<u64>,
    pub duration_ms: Option<i64>,
    pub rating: u8,
    pub label: Option<String>,
    /// BLAKE3 of the original (the rendition cache key).
    pub source_hash: Option<Blake3Hash>,
    /// Volumes holding the original.
    pub volumes: Vec<VolumeId>,
}

/// A calendar cell: a year (`2024`), month (`2024-06`) or day
/// (`2024-06-15`), or `undated`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalendarBucket {
    pub key: String,
    pub count: u64,
    /// The best-rated, earliest asset of the bucket.
    pub cover: Option<AssetId>,
    pub cover_hash: Option<Blake3Hash>,
}

/// Calendar level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "level", rename_all = "snake_case")]
pub enum CalendarLevel {
    Years,
    Months { year: i32 },
    Days { year: i32, month: u32 },
}

/// A value and how many assets have it (filter menus).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Facet {
    pub value: String,
    pub count: u64,
}

const COLS: &str =
    "a.id, a.primary_name, a.capture_ts_ms, a.capture_utc_offset_min, a.camera_model,
     a.lens_model, a.media_kind, a.is_raw, a.size_bytes, a.duration_ms, a.rating, l.name";

fn map(r: &Row<'_>) -> rusqlite::Result<BrowseRow> {
    let raw: i64 = r.get(7)?;
    let size: Option<i64> = r.get(8)?;
    let rating: i64 = r.get(10)?;
    Ok(BrowseRow {
        id: r.get(0)?,
        name: r.get(1)?,
        capture_ms: r.get(2)?,
        utc_offset_min: r.get(3)?,
        camera: r.get(4)?,
        lens: r.get(5)?,
        kind: r.get(6)?,
        raw: raw != 0,
        size: size.and_then(|s| u64::try_from(s).ok()),
        duration_ms: r.get(9)?,
        rating: u8::try_from(rating).unwrap_or(0),
        label: r.get(11)?,
        source_hash: None,
        volumes: Vec::new(),
    })
}

/// ORDER BY for a sort: missing values last, ties by capture time then
/// insertion order, so the order is total and stable.
fn order_by(s: Sort) -> String {
    let d = if s.descending { "DESC" } else { "ASC" };
    match s.field {
        SortField::Date => format!("a.capture_ts_ms IS NULL, a.capture_ts_ms {d}, a.seq {d}"),
        SortField::Name => format!(
            "a.primary_name IS NULL, a.primary_name COLLATE NOCASE {d}, a.capture_ts_ms, a.seq"
        ),
        SortField::Camera => format!(
            "a.camera_model IS NULL, a.camera_model COLLATE NOCASE {d}, a.capture_ts_ms, a.seq"
        ),
        SortField::Lens => {
            format!("a.lens_model IS NULL, a.lens_model COLLATE NOCASE {d}, a.capture_ts_ms, a.seq")
        }
        SortField::Type => format!("a.media_kind {d}, a.is_raw {d}, a.capture_ts_ms, a.seq"),
        SortField::Size => format!("a.size_bytes IS NULL, a.size_bytes {d}, a.seq"),
        SortField::Duration => format!("a.duration_ms IS NULL, a.duration_ms {d}, a.seq"),
        SortField::Rating => format!("a.rating {d}, a.capture_ts_ms, a.seq"),
    }
}

fn params(f: &FilterSql) -> rusqlite::ParamsFromIter<impl Iterator<Item = &dyn rusqlite::ToSql>> {
    rusqlite::params_from_iter(f.args.iter().map(|b| b.as_ref()))
}

impl<M> Browse<'_, M> {
    /// How many assets match.
    pub fn count(&self, q: &SearchQuery) -> Result<u64, CatalogError> {
        let f = filter_sql(q)?;
        if f.empty {
            return Ok(0);
        }
        let sql = format!("SELECT count(*) FROM assets a{}{}", f.joins, f.where_sql());
        let n: i64 = self.conn.query_row(&sql, params(&f), |r| r.get(0))?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    /// Rows `offset..offset+limit` of the sorted view.
    pub fn page(
        &self,
        q: &SearchQuery,
        sort: Sort,
        offset: u64,
        limit: u32,
    ) -> Result<Vec<BrowseRow>, CatalogError> {
        let mut f = filter_sql(q)?;
        if f.empty || limit == 0 {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT {COLS} FROM assets a LEFT JOIN labels l ON l.id = a.label_id{}{} ORDER BY {} LIMIT ? OFFSET ?",
            f.joins,
            f.where_sql(),
            order_by(sort)
        );
        f.args.push(Box::new(i64::from(limit)));
        f.args
            .push(Box::new(i64::try_from(offset).unwrap_or(i64::MAX)));
        let mut st = self.conn.prepare(&sql)?;
        let mut rows: Vec<BrowseRow> = st.query_map(params(&f), map)?.collect::<Result<_, _>>()?;
        self.attach_files(&mut rows)?;
        Ok(rows)
    }

    /// Fill `source_hash` and `volumes` from the originals.
    fn attach_files(&self, rows: &mut [BrowseRow]) -> Result<(), CatalogError> {
        if rows.is_empty() {
            return Ok(());
        }
        let sql = format!(
            "SELECT asset_id, volume_id, blake3 FROM files WHERE role = 'original' AND asset_id IN ({})",
            vec!["?"; rows.len()].join(",")
        );
        let mut st = self.conn.prepare(&sql)?;
        let ids: Vec<AssetId> = rows.iter().map(|r| r.id).collect();
        let mut by: HashMap<AssetId, (Option<Blake3Hash>, Vec<VolumeId>)> = HashMap::new();
        let found = st.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
            Ok((
                r.get::<_, AssetId>(0)?,
                r.get::<_, VolumeId>(1)?,
                r.get::<_, Blake3Hash>(2)?,
            ))
        })?;
        for row in found {
            let (a, v, h) = row?;
            let e = by.entry(a).or_default();
            e.0.get_or_insert(h);
            e.1.push(v);
        }
        for r in rows {
            if let Some((h, v)) = by.remove(&r.id) {
                r.source_hash = h;
                r.volumes = v;
            }
        }
        Ok(())
    }

    /// Every matching id in view order: one sorted scan. The app keeps
    /// this as the view's snapshot and pages through it with [`Self::rows`],
    /// so deep pages cost the same as the first.
    pub fn ordered_ids(&self, q: &SearchQuery, sort: Sort) -> Result<Vec<AssetId>, CatalogError> {
        let f = filter_sql(q)?;
        if f.empty {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT a.id FROM assets a{}{} ORDER BY {}",
            f.joins,
            f.where_sql(),
            order_by(sort)
        );
        let mut st = self.conn.prepare(&sql)?;
        let ids = st
            .query_map(params(&f), |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    /// Rows for these ids, in the order given (missing ids are skipped).
    pub fn rows(&self, ids: &[AssetId]) -> Result<Vec<BrowseRow>, CatalogError> {
        let mut out = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(500) {
            let sql = format!(
                "SELECT {COLS} FROM assets a LEFT JOIN labels l ON l.id = a.label_id WHERE a.id IN ({})",
                vec!["?"; chunk.len()].join(",")
            );
            let mut st = self.conn.prepare(&sql)?;
            let mut by: HashMap<AssetId, BrowseRow> = st
                .query_map(rusqlite::params_from_iter(chunk.iter()), map)?
                .map(|r| r.map(|row| (row.id, row)))
                .collect::<Result<_, _>>()?;
            for id in chunk {
                if let Some(r) = by.remove(id) {
                    out.push(r);
                }
            }
        }
        self.attach_files(&mut out)?;
        Ok(out)
    }

    /// The ids of rows `from..to` (exclusive) of the sorted view: what a
    /// multi-selection of index ranges stands for.
    pub fn ids_range(
        &self,
        q: &SearchQuery,
        sort: Sort,
        from: u64,
        to: u64,
    ) -> Result<Vec<AssetId>, CatalogError> {
        let mut f = filter_sql(q)?;
        if f.empty || to <= from {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT a.id FROM assets a{}{} ORDER BY {} LIMIT ? OFFSET ?",
            f.joins,
            f.where_sql(),
            order_by(sort)
        );
        f.args
            .push(Box::new(i64::try_from(to - from).unwrap_or(i64::MAX)));
        f.args
            .push(Box::new(i64::try_from(from).unwrap_or(i64::MAX)));
        let mut st = self.conn.prepare(&sql)?;
        let ids = st
            .query_map(params(&f), |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    /// Where an asset sits in the sorted view (`None`: not in it).
    pub fn index_of(
        &self,
        q: &SearchQuery,
        sort: Sort,
        asset: AssetId,
    ) -> Result<Option<u64>, CatalogError> {
        let f = filter_sql(q)?;
        if f.empty {
            return Ok(None);
        }
        let sql = format!(
            "SELECT n - 1 FROM (SELECT a.id, row_number() OVER (ORDER BY {}) AS n
               FROM assets a{}{}) WHERE id = ?",
            order_by(sort),
            f.joins,
            f.where_sql()
        );
        let mut args: Vec<&dyn rusqlite::ToSql> = f.args.iter().map(|b| b.as_ref()).collect();
        args.push(&asset);
        let n: Option<i64> = self
            .conn
            .query_row(&sql, args.as_slice(), |r| r.get(0))
            .optional()?;
        Ok(n.and_then(|n| u64::try_from(n).ok()))
    }

    /// Calendar cells at a level, with counts and covers.
    pub fn calendar(
        &self,
        q: &SearchQuery,
        level: CalendarLevel,
    ) -> Result<Vec<CalendarBucket>, CatalogError> {
        let f = filter_sql(q)?;
        if f.empty {
            return Ok(Vec::new());
        }
        let (len, prefix) = match level {
            CalendarLevel::Years => (4, None),
            CalendarLevel::Months { year } => (7, Some(format!("{year:04}-"))),
            CalendarLevel::Days { year, month } => (10, Some(format!("{year:04}-{month:02}-"))),
        };
        let mut wh = f.wh.clone();
        wh.push("a.capture_local_date IS NOT NULL".into());
        // Ranges, not LIKE, so the date index is used.
        let upper = prefix.as_ref().map(|p| format!("{p}~"));
        if prefix.is_some() {
            wh.push("a.capture_local_date >= ? AND a.capture_local_date < ?".into());
        }
        let where_sql = format!(" WHERE {}", wh.join(" AND "));
        let mut args: Vec<&dyn rusqlite::ToSql> = f.args.iter().map(|b| b.as_ref()).collect();
        if let (Some(p), Some(u)) = (&prefix, &upper) {
            args.push(p);
            args.push(u);
        }
        let sql = format!(
            "SELECT substr(a.capture_local_date, 1, {len}) AS k, count(*) FROM assets a{}{where_sql}
             GROUP BY k ORDER BY k",
            f.joins
        );
        let mut st = self.conn.prepare(&sql)?;
        let counts: Vec<(String, i64)> = st
            .query_map(args.as_slice(), |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<_, _>>()?;
        let cover_sql = format!(
            "SELECT a.id, (SELECT blake3 FROM files WHERE asset_id = a.id AND role = 'original' LIMIT 1)
               FROM assets a{} WHERE {} AND a.capture_local_date >= ? AND a.capture_local_date < ?
              ORDER BY a.rating DESC, a.capture_ts_ms, a.seq LIMIT 1",
            f.joins,
            {
                let mut w = f.wh.clone();
                w.push("a.capture_local_date IS NOT NULL".into());
                w.join(" AND ")
            }
        );
        let mut cover = self.conn.prepare(&cover_sql)?;
        let mut out = Vec::with_capacity(counts.len() + 1);
        for (key, n) in counts {
            let mut a: Vec<&dyn rusqlite::ToSql> = f.args.iter().map(|b| b.as_ref()).collect();
            // `~` sorts after digits and `-`, so `key~` bounds every date in `key`.
            let hi = format!("{key}~");
            a.push(&key);
            a.push(&hi);
            let c: Option<(AssetId, Option<Blake3Hash>)> = cover
                .query_row(a.as_slice(), |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            out.push(CalendarBucket {
                key,
                count: u64::try_from(n).unwrap_or(0),
                cover: c.map(|c| c.0),
                cover_hash: c.and_then(|c| c.1),
            });
        }
        if level == CalendarLevel::Years {
            let undated = self.count(&SearchQuery {
                undated: true,
                ..q.clone()
            })?;
            if undated > 0 {
                out.push(CalendarBucket {
                    key: "undated".into(),
                    count: undated,
                    cover: None,
                    cover_hash: None,
                });
            }
        }
        Ok(out)
    }

    /// Assets per volume (counting originals).
    pub fn volume_counts(&self) -> Result<Vec<(VolumeId, u64)>, CatalogError> {
        self.counts(
            "SELECT volume_id, count(DISTINCT asset_id) FROM files WHERE role = 'original' GROUP BY volume_id",
        )
    }

    /// Assets per event.
    pub fn event_counts(&self) -> Result<Vec<(crate::catalog::EventId, u64)>, CatalogError> {
        self.counts(
            "SELECT event_id, count(*) FROM assets WHERE event_id IS NOT NULL GROUP BY event_id",
        )
    }

    /// Catalogued assets per import session.
    pub fn session_counts(
        &self,
    ) -> Result<Vec<(crate::catalog::ImportSessionId, u64)>, CatalogError> {
        self.counts(
            "SELECT session_id, count(*) FROM import_assets WHERE state = 'catalogued' GROUP BY session_id",
        )
    }

    fn counts<K: rusqlite::types::FromSql>(
        &self,
        sql: &str,
    ) -> Result<Vec<(K, u64)>, CatalogError> {
        let mut st = self.conn.prepare(sql)?;
        let rows = st
            .query_map([], |r| {
                Ok((
                    r.get::<_, K>(0)?,
                    u64::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                ))
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Cameras or lenses in the library, most used first.
    pub fn facets(&self, column: FacetColumn) -> Result<Vec<Facet>, CatalogError> {
        let col = match column {
            FacetColumn::Camera => "camera_model",
            FacetColumn::Lens => "lens_model",
        };
        let mut st = self.conn.prepare(&format!(
            "SELECT {col}, count(*) FROM assets WHERE {col} IS NOT NULL AND {col} != ''
             GROUP BY {col} ORDER BY count(*) DESC, {col} LIMIT 500"
        ))?;
        let rows = st
            .query_map([], |r| {
                Ok(Facet {
                    value: r.get(0)?,
                    count: u64::try_from(r.get::<_, i64>(1)?).unwrap_or(0),
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacetColumn {
    Camera,
    Lens,
}

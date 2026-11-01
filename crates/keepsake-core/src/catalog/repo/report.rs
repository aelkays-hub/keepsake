//! The library report (Tools → Report): how many photos and videos the
//! library holds and how much disk they take, by date, camera, lens, file
//! type and drive.
//!
//! Everything is GROUP BY over the catalog; no media file is read. The
//! counting rules:
//! - An item is one asset, so a RAW+JPEG pair is one photo.
//! - Files and bytes count every file of the item (both halves of a pair,
//!   sidecars, segments), but only one copy of each: copies of the same file
//!   on several drives (backups, a second import destination) share a BLAKE3
//!   key and count once. Files not fingerprinted yet have a stand-in key
//!   unique to their place, so they always count.
//! - Files on unplugged drives count like any other.
//! - The drives table is the exception: it counts every copy on each drive.
//!
//! Speed (300,000 items): the date, camera and lens breakdowns are one scan
//! of `assets`, whose `file_count` and `file_bytes` the catalog keeps up to
//! date (schema v8). File types and drives are one scan of `files` each,
//! with no join; so they count items, without the photo/video split.

use rusqlite::Row;
use serde::{Deserialize, Serialize};

use super::{define_repo, u64_col};
use crate::catalog::error::CatalogError;
use crate::catalog::VolumeId;

define_repo!(
    /// Library report queries (read-only).
    Report
);

/// How a missing group (no date, camera, lens or extension) is shown.
pub const UNKNOWN: &str = "Unknown";

/// One group of a breakdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportRow {
    /// The group (`2024`, `2024-06`, `2024-06-01`, `ILCE-7M4`, `ARW`, a
    /// drive's name); `None` when unknown.
    pub group: Option<String>,
    /// Photos plus videos.
    pub items: u64,
    /// `None` in the file type and drive breakdowns (not split there).
    pub photos: Option<u64>,
    pub videos: Option<u64>,
    pub files: u64,
    pub bytes: u64,
}

impl ReportRow {
    /// The group as shown to people (`Unknown` when missing).
    pub fn label(&self) -> &str {
        self.group.as_deref().unwrap_or(UNKNOWN)
    }
}

/// One drive: every copy stored on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriveRow {
    pub volume_id: VolumeId,
    pub online: bool,
    /// `group` is the drive's name.
    pub row: ReportRow,
}

/// The whole report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryReport {
    /// Grand totals (`group` is `None`).
    pub totals: ReportRow,
    /// Newest first; undated last.
    pub years: Vec<ReportRow>,
    pub months: Vec<ReportRow>,
    pub days: Vec<ReportRow>,
    /// Most items first; unknown last.
    pub cameras: Vec<ReportRow>,
    pub lenses: Vec<ReportRow>,
    /// Upper-case extension without the dot (`JPG`). An item counts under
    /// each extension it has (a RAW+JPEG pair under both `ARW` and `JPG`).
    pub file_types: Vec<ReportRow>,
    /// By name.
    pub drives: Vec<DriveRow>,
}

/// The text after a file name's last dot, upper case (`''` with no dot).
/// `rtrim(name, <name without dots>)` strips the extension, leaving `stem.`.
const EXT: &str = "CASE WHEN instr(name, '.') = 0 THEN ''
      ELSE upper(substr(name, length(rtrim(name, replace(name, '.', ''))) + 1)) END";

const DAY: i64 = 0;
const MONTH: i64 = 1;
const YEAR: i64 = 2;
const CAMERA: i64 = 3;
const LENS: i64 = 4;

/// Sums over `cube`.
const SUMS: &str = "ifnull(sum(photos), 0) AS p, ifnull(sum(videos), 0) AS v,
                    ifnull(sum(files), 0) AS n, ifnull(sum(bytes), 0) AS b";

/// `group, photos, videos, files, bytes` from column `first` on.
fn split_row(r: &Row<'_>, first: usize) -> rusqlite::Result<ReportRow> {
    let photos = u64_col(r, first + 1)?;
    let videos = u64_col(r, first + 2)?;
    Ok(ReportRow {
        group: r.get(first)?,
        items: photos + videos,
        photos: Some(photos),
        videos: Some(videos),
        files: u64_col(r, first + 3)?,
        bytes: u64_col(r, first + 4)?,
    })
}

/// `group, items, files, bytes` from column `first` on.
fn items_row(r: &Row<'_>, first: usize) -> rusqlite::Result<ReportRow> {
    Ok(ReportRow {
        group: r.get(first)?,
        items: u64_col(r, first + 1)?,
        photos: None,
        videos: None,
        files: u64_col(r, first + 2)?,
        bytes: u64_col(r, first + 3)?,
    })
}

impl<M> Report<'_, M> {
    /// Build the whole report.
    pub fn library(&self) -> Result<LibraryReport, CatalogError> {
        let mut out = LibraryReport {
            totals: self.totals()?,
            years: Vec::new(),
            months: Vec::new(),
            days: Vec::new(),
            cameras: Vec::new(),
            lenses: Vec::new(),
            file_types: self.file_types()?,
            drives: self.drives()?,
        };
        self.asset_breakdowns(&mut out)?;
        Ok(out)
    }

    fn totals(&self) -> Result<ReportRow, CatalogError> {
        Ok(self.conn.query_row(
            "SELECT NULL, ifnull(sum(media_kind = 'photo'), 0), ifnull(sum(media_kind = 'video'), 0),
                    ifnull(sum(file_count), 0), ifnull(sum(file_bytes), 0)
               FROM assets NOT INDEXED",
            [],
            |r| split_row(r, 0),
        )?)
    }

    /// Dates, cameras and lenses in one statement: assets are first summed
    /// per (day, camera, lens), and every breakdown is a GROUP BY over that.
    fn asset_breakdowns(&self, out: &mut LibraryReport) -> Result<(), CatalogError> {
        let sql = format!(
            "WITH cube AS MATERIALIZED (
               SELECT capture_local_date AS d,
                      nullif(trim(camera_model), '') AS cam,
                      nullif(trim(lens_model), '') AS lens,
                      sum(media_kind = 'photo') AS photos,
                      sum(media_kind = 'video') AS videos,
                      sum(file_count) AS files,
                      sum(file_bytes) AS bytes
                 FROM assets NOT INDEXED
                GROUP BY 1, 2, 3)
             SELECT * FROM (
                       SELECT {DAY} AS dim, d AS g, {SUMS} FROM cube GROUP BY d
             UNION ALL SELECT {MONTH}, substr(d, 1, 7), {SUMS} FROM cube GROUP BY 2
             UNION ALL SELECT {YEAR}, substr(d, 1, 4), {SUMS} FROM cube GROUP BY 2
             UNION ALL SELECT {CAMERA}, cam, {SUMS} FROM cube GROUP BY cam
             UNION ALL SELECT {LENS}, lens, {SUMS} FROM cube GROUP BY lens)
             ORDER BY dim, g IS NULL, CASE WHEN dim <= {YEAR} THEN g END DESC,
                      p + v DESC, g"
        );
        let mut st = self.conn.prepare(&sql)?;
        let mut rows = st.query([])?;
        while let Some(r) = rows.next()? {
            let dim: i64 = r.get(0)?;
            let v = split_row(r, 1)?;
            match dim {
                DAY => out.days.push(v),
                MONTH => out.months.push(v),
                YEAR => out.years.push(v),
                CAMERA => out.cameras.push(v),
                _ => out.lenses.push(v),
            }
        }
        Ok(())
    }

    /// By extension, one copy of each file (as in `assets.file_count`).
    fn file_types(&self) -> Result<Vec<ReportRow>, CatalogError> {
        let sql = format!(
            "WITH one AS (
               SELECT asset_id, max(size_bytes) AS sz, min(file_name) AS name
                 FROM files NOT INDEXED GROUP BY asset_id, blake3)
             SELECT nullif({EXT}, '') AS ext, count(DISTINCT asset_id) AS items,
                    count(*), sum(sz)
               FROM one
              GROUP BY ext
              ORDER BY ext IS NULL, items DESC, ext"
        );
        let mut st = self.conn.prepare(&sql)?;
        let rows = st.query_map([], |r| items_row(r, 0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every copy on each drive, plugged in or not.
    fn drives(&self) -> Result<Vec<DriveRow>, CatalogError> {
        let mut st = self.conn.prepare(
            "WITH n AS (SELECT volume_id, count(DISTINCT asset_id) AS items
                          FROM files GROUP BY volume_id),
                  s AS (SELECT volume_id, count(*) AS files, sum(size_bytes) AS bytes
                          FROM files NOT INDEXED GROUP BY volume_id)
             SELECT v.id, v.online, v.label, n.items, s.files, s.bytes
               FROM volumes v
               JOIN n ON n.volume_id = v.id
               JOIN s ON s.volume_id = v.id
              ORDER BY v.label COLLATE NOCASE, v.id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(DriveRow {
                volume_id: r.get(0)?,
                online: r.get(1)?,
                row: items_row(r, 2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

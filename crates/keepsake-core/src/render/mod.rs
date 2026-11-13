//! Renditions: thumbnails, previews and video proxies (ADR-021).
//!
//! - [`Variant`]: what is made (256/1024 px thumbnails, a 2560 px preview,
//!   a 720p H.264 proxy, or the camera's own H.264 proxy copied as-is).
//! - [`cache::Cache`]: content-addressed files keyed by source hash +
//!   recipe hash, with LRU eviction that keeps renditions of offline media.
//! - [`source`]: which file of an asset is rendered from.
//! - [`still`] and [`video`]: the renderers. Originals are only read.
//! - [`service::RenderService`]: workers on the job queue, with pauses and
//!   progress events.
//! - [`backfill::Backfill`]: a headless walk that makes every missing
//!   thumbnail in batches, resuming where the last run stopped.
//!
//! Job kinds: [`JOB_STILLS`] makes the preview and both thumbnails in one
//! pass (the source is decoded once); [`JOB_PROXY`] makes the proxy.

pub mod backfill;
pub mod cache;
pub mod card;
pub mod full;
pub mod recipe;
pub mod service;
pub mod source;
pub mod still;
pub mod video;

use std::path::PathBuf;

use thiserror::Error;

pub use cache::{Cache, CacheStats, EvictReport, RepairReport};
pub use recipe::Variant;
pub use service::{RenderEvent, RenderOptions, RenderService, RenderStatus};

use crate::catalog::{AssetId, Catalog, CatalogError, MediaKind, NewJob, Priority};
use crate::library::LibraryError;
use crate::probe::ProbeError;

/// Preview + thumbnails for one asset.
pub const JOB_STILLS: &str = "render.stills";
/// Proxy for one video asset.
pub const JOB_PROXY: &str = "render.proxy";

/// Errors from rendering one asset.
#[derive(Debug, Error)]
pub enum RenderError {
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Library(#[from] LibraryError),
    #[error(transparent)]
    Tool(#[from] ProbeError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("image: {0}")]
    Image(String),
    /// No copy of the asset is on a mounted volume (retried later).
    #[error("no copy of the asset is online")]
    Offline,
    /// The asset's format cannot be rendered (not retried).
    #[error("cannot render {0}")]
    Unsupported(String),
    /// Not ready yet (e.g. AI waits for a thumbnail): tried again later,
    /// not counted as an attempt.
    #[error("waiting: {0}")]
    Waiting(String),
    #[error("{0}")]
    Failed(String),
    /// Refused for a reason that trying again will not fix (e.g. the AI
    /// provider did not accept the API key; not retried).
    #[error("{0}")]
    Refused(String),
}

impl RenderError {
    /// The catalog was locked by another writer for too long: try the job
    /// again soon, without counting it as a failed attempt.
    pub fn is_busy(&self) -> bool {
        match self {
            RenderError::Catalog(e) | RenderError::Library(LibraryError::Catalog(e)) => e.is_busy(),
            _ => false,
        }
    }

    /// Retrying will not help.
    pub fn is_permanent(&self) -> bool {
        matches!(self, RenderError::Unsupported(_) | RenderError::Refused(_))
    }
}

impl From<image::ImageError> for RenderError {
    fn from(e: image::ImageError) -> Self {
        RenderError::Image(e.to_string())
    }
}

/// External tools the renderers run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderTools {
    pub exiftool: PathBuf,
    pub ffprobe: PathBuf,
    pub ffmpeg: PathBuf,
    /// LibRaw's `dcraw_emu` (full RAW decoding, ADR-023).
    pub dcraw: PathBuf,
}

impl RenderTools {
    /// From probe tool paths: ffmpeg sits next to ffprobe unless
    /// `KEEPSAKE_FFMPEG` says otherwise.
    pub fn from_probe(t: &crate::probe::ToolPaths) -> Self {
        let ffmpeg = std::env::var_os("KEEPSAKE_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                t.ffprobe.with_file_name(if cfg!(windows) {
                    "ffmpeg.exe"
                } else {
                    "ffmpeg"
                })
            });
        // vendor/libraw/dcraw_emu, next to vendor/ffprobe.
        let dcraw = std::env::var_os("KEEPSAKE_DCRAW")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                t.ffprobe.with_file_name("libraw").join(if cfg!(windows) {
                    "dcraw_emu.exe"
                } else {
                    "dcraw_emu"
                })
            });
        Self {
            exiftool: t.exiftool.clone(),
            ffprobe: t.ffprobe.clone(),
            ffmpeg,
            dcraw,
        }
    }

    pub fn discover() -> Self {
        Self::from_probe(&crate::probe::ToolPaths::discover())
    }
}

/// Newest capture first; undated last.
fn sort_key(capture_utc_ms: Option<i64>) -> i64 {
    capture_utc_ms.map_or(i64::MAX, |t| -t)
}

/// Queue the render jobs of these assets. Already finished jobs are queued
/// again only when `again` (e.g. a rendition was evicted or is missing).
/// Returns how many jobs were added.
pub fn enqueue(
    cat: &mut Catalog,
    assets: &[AssetId],
    priority: Priority,
    again: bool,
) -> Result<u64, CatalogError> {
    // Short transactions, so a big backlog never blocks other writers
    // (claims, edits) for long.
    let mut n = 0;
    for chunk in assets.chunks(ENQUEUE_CHUNK) {
        n += enqueue_chunk(cat, chunk, priority, again)?;
    }
    Ok(n)
}

fn enqueue_chunk(
    cat: &mut Catalog,
    assets: &[AssetId],
    priority: Priority,
    again: bool,
) -> Result<u64, CatalogError> {
    let now = crate::jobs::now_ms();
    cat.write(|tx| {
        let mut jobs = Vec::with_capacity(assets.len() * 2);
        for id in assets {
            let Some(a) = tx.assets().get(*id)? else {
                continue;
            };
            let key = sort_key(a.capture.map(|c| c.utc_ms));
            jobs.push(NewJob {
                kind: JOB_STILLS,
                asset_id: *id,
                rank: 0,
                sort_key: key,
            });
            if a.media_kind == MediaKind::Video {
                jobs.push(NewJob {
                    kind: JOB_PROXY,
                    asset_id: *id,
                    rank: 1,
                    sort_key: key,
                });
            }
        }
        tx.jobs().enqueue(&jobs, priority, now, again)
    })
}

/// Jobs queued per write. Each holds the catalog while it runs, so a large
/// backlog is queued in small writes that let the workers and the window in
/// between (2,000 per write held it for seconds on a USB hard disk).
pub(crate) const ENQUEUE_CHUNK: usize = 200;

/// Queue every asset in the library that has no render jobs yet (backlog),
/// and the fingerprint jobs of files not yet read in full.
pub fn enqueue_backlog(cat: &mut Catalog) -> Result<u64, CatalogError> {
    let ids = cat.read(|tx| tx.assets().without_jobs())?;
    let n = enqueue(cat, &ids, Priority::Backlog, false)?;
    Ok(n + crate::fingerprint::enqueue_pending(cat)?)
}

/// Does an asset lack any of its renditions in `cache`?
pub fn needs_render(cat: &mut Catalog, cache: &Cache, asset: AssetId) -> Result<bool, RenderError> {
    let Some(hash) = source::source_hash(cat, asset)? else {
        return Ok(false);
    };
    for v in Variant::STILLS {
        if cache.find(cat, &hash, v)?.is_none() {
            return Ok(true);
        }
    }
    let video =
        cat.read(|tx| tx.assets().get(asset))?.map(|a| a.media_kind) == Some(MediaKind::Video);
    Ok(video
        && cache.find(cat, &hash, Variant::CameraProxy)?.is_none()
        && cache.find(cat, &hash, Variant::Proxy720)?.is_none())
}

/// The assets on screen: their jobs run first, and are queued again if a
/// rendition is missing (evicted, or deleted from the cache). Replaces the
/// previous visible set.
pub fn set_visible(
    cat: &mut Catalog,
    cache: &Cache,
    assets: &[AssetId],
) -> Result<u64, RenderError> {
    let mut stale = Vec::new();
    let mut fine = Vec::new();
    for a in assets {
        // A photo that could not be rendered (a RAW without a preview, a
        // file that is not a picture) is not tried again each time it is
        // on screen; "Retry failed" or a rescan does that.
        let failed = cat
            .read(|tx| tx.jobs().find(JOB_STILLS, *a))?
            .is_some_and(|j| j.state == crate::catalog::JobState::Failed);
        if failed {
            continue;
        }
        if needs_render(cat, cache, *a)? {
            stale.push(*a);
        } else {
            fine.push(*a);
        }
    }
    let mut n = enqueue(cat, &stale, Priority::Visible, true)?;
    n += enqueue(cat, &fine, Priority::Visible, false)?;
    cat.write(|tx| tx.jobs().set_visible(assets))?;
    Ok(n)
}

//! Which files of an asset a rendition is made from.

use std::path::PathBuf;

use super::RenderError;
use crate::catalog::{AssetId, Blake3Hash, Catalog, FileRole, MediaKind};
use crate::library::Library;

/// The files to render an asset from, all on one mounted volume.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub asset: AssetId,
    pub media_kind: MediaKind,
    /// BLAKE3 of the original: the cache key.
    pub hash: Blake3Hash,
    /// The original file.
    pub original: PathBuf,
    /// The cheapest member to make stills from (the JPEG of RAW + JPEG).
    pub still: PathBuf,
    /// Further AVCHD segments, in order.
    pub segments: Vec<PathBuf>,
    /// The camera's proxy, if present.
    pub camera_proxy: Option<PathBuf>,
    pub duration_ms: Option<i64>,
}

/// Lower is cheaper to decode.
fn still_cost(path: &std::path::Path) -> u8 {
    match lower_ext(path).as_str() {
        "jpg" | "jpeg" => 0,
        "png" | "tif" | "tiff" => 1,
        "hif" | "heif" | "heic" => 2,
        _ => 3, // raw: embedded preview
    }
}

pub fn lower_ext(path: &std::path::Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// The cache key of an asset (its original's BLAKE3), from the catalog
/// alone: works while the media is offline.
pub fn source_hash(cat: &Catalog, asset: AssetId) -> Result<Option<Blake3Hash>, RenderError> {
    let files = cat.read(|tx| tx.files().for_asset(asset))?;
    Ok(files
        .iter()
        .find(|f| f.role == FileRole::Original)
        .map(|f| f.blake3))
}

/// Find the asset's files on a mounted volume.
pub fn locate(cat: &Catalog, lib: &Library, asset: AssetId) -> Result<Source, RenderError> {
    let a = cat.read(|tx| tx.assets().require(asset))?;
    let files = cat.read(|tx| tx.files().for_asset(asset))?;
    let hash = files
        .iter()
        .find(|f| f.role == FileRole::Original)
        .map(|f| f.blake3)
        .ok_or_else(|| RenderError::Failed("the asset has no original file".into()))?;
    let mut volumes: Vec<_> = files.iter().map(|f| f.volume_id).collect();
    volumes.dedup();
    for v in volumes {
        let Some(mount) = crate::verify::mount_of(cat, lib, v) else {
            continue;
        };
        let here: Vec<_> = files.iter().filter(|f| f.volume_id == v).collect();
        let path_of = |f: &crate::catalog::File| f.rel_path.to_fs_path(&mount);
        let Some(orig) = here
            .iter()
            .find(|f| f.role == FileRole::Original)
            .map(|f| path_of(f))
            .filter(|p| p.is_file())
        else {
            continue;
        };
        let mut stills: Vec<PathBuf> = here
            .iter()
            .filter(|f| matches!(f.role, FileRole::Original | FileRole::RawPair))
            .map(|f| path_of(f))
            .filter(|p| p.is_file())
            .collect();
        stills.sort_by_key(|p| still_cost(p));
        let mut segs: Vec<_> = here
            .iter()
            .filter(|f| f.role == FileRole::Segment)
            .collect();
        segs.sort_by_key(|f| f.segment_index);
        let segments: Vec<PathBuf> = segs.iter().map(|f| path_of(f)).collect();
        if segments.iter().any(|p| !p.is_file()) {
            continue;
        }
        let camera_proxy = here
            .iter()
            .find(|f| f.role == FileRole::CameraProxy)
            .map(|f| path_of(f))
            .filter(|p| p.is_file());
        return Ok(Source {
            asset,
            media_kind: a.media_kind,
            hash,
            still: stills.first().cloned().unwrap_or_else(|| orig.clone()),
            original: orig,
            segments,
            camera_proxy,
            duration_ms: a.duration_ms,
        });
    }
    Err(RenderError::Offline)
}

//! Thumbnails of items still on a card, for the import window.
//!
//! These are not library renditions: there is no catalog row (the items are
//! not catalogued yet), so they go to `<cache>/card/` and are swept after a
//! week. The card is only read. Sources, cheapest first:
//! - a camera thumbnail (`PRIVATE/M4ROOT/THMBNL/*T01.JPG`);
//! - a JPEG member (RAW + JPEG);
//! - a RAW's embedded preview (ExifTool offsets);
//! - HEIF, or a video frame at 10 %, via FFmpeg (the camera's SUB proxy is
//!   used for video when present: it is small and fast to decode).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::source::{lower_ext, Source};
use super::{still, Cache, RenderError, RenderTools};
use crate::catalog::{AssetId, Blake3Hash, MediaKind};
use crate::probe::ExifTool;

/// Long edge of card thumbnails.
pub const CARD_THUMB: u32 = 320;
/// Card thumbnails older than this are removed by [`sweep`].
pub const KEEP_FOR: Duration = Duration::from_secs(7 * 24 * 3600);

fn card_dir(cache: &Cache) -> PathBuf {
    cache.root().join("card")
}

/// A key for the item: where it is and what it is (path, size, time), so a
/// changed or different card gets a new thumbnail.
fn key(mount: &Path, files: &[String]) -> Result<Blake3Hash, RenderError> {
    let mut h = blake3::Hasher::new();
    h.update(b"keepsake.card-thumb.v1\0");
    h.update(mount.to_string_lossy().as_bytes());
    for f in files {
        let md = std::fs::metadata(mount.join(f))?;
        h.update(b"\0");
        h.update(f.as_bytes());
        h.update(&md.len().to_le_bytes());
        let t = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis());
        h.update(&t.to_le_bytes());
    }
    Ok(Blake3Hash(*h.finalize().as_bytes()))
}

/// A thumbnail for one asset on a card. `files` are its paths relative to
/// `mount`, the original first (as `ingest::preview` lists them).
pub fn card_thumbnail(
    cache: &Cache,
    tools: &RenderTools,
    exiftool: Option<&ExifTool>,
    mount: &Path,
    files: &[String],
) -> Result<PathBuf, RenderError> {
    let original = files
        .first()
        .ok_or_else(|| RenderError::Failed("no files".into()))?;
    let k = key(mount, &files[..1])?;
    let hex = k.to_hex();
    let out = card_dir(cache).join(&hex[..2]).join(format!("{hex}.jpg"));
    if out.is_file() {
        return Ok(out);
    }
    let kind = crate::rebuild::media_kind_for(original);
    let path_of = |f: &String| mount.join(f);
    let camera_thumb = files
        .iter()
        .find(|f| f.to_ascii_uppercase().contains("THMBNL") && lower_ext(Path::new(f)) == "jpg");
    let img = if let Some(t) = camera_thumb {
        still::decode(&std::fs::read(path_of(t))?, None)?
    } else {
        let stills: Vec<PathBuf> = files
            .iter()
            .filter(|f| crate::rebuild::media_kind_for(f) == MediaKind::Photo)
            .filter(|f| !f.to_ascii_uppercase().ends_with(".XML"))
            .map(path_of)
            .collect();
        let jpeg = stills
            .iter()
            .find(|p| matches!(lower_ext(p).as_str(), "jpg" | "jpeg"))
            .cloned();
        let proxy = files
            .iter()
            .find(|f| f.to_ascii_uppercase().contains("/SUB/"))
            .map(path_of);
        let src = Source {
            asset: AssetId::new_random(),
            media_kind: kind,
            hash: k,
            original: match (kind, &proxy) {
                (MediaKind::Video, Some(p)) => p.clone(),
                _ => path_of(original),
            },
            still: jpeg.unwrap_or_else(|| path_of(original)),
            raw: None,
            segments: Vec::new(),
            camera_proxy: None,
            duration_ms: None,
        };
        let scratch = cache.temp_path("frame")?;
        still::picture(&src, tools, exiftool, &scratch)?
    };
    let thumb = still::fit(&img, CARD_THUMB)?;
    let bytes = still::encode_jpeg(&thumb)?;
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = cache.temp_path("jpg")?;
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, &out) {
        let _ = std::fs::remove_file(&tmp);
        if !out.is_file() {
            return Err(e.into());
        }
    }
    Ok(out)
}

/// Remove card thumbnails older than [`KEEP_FOR`]. Returns how many.
pub fn sweep(cache: &Cache) -> u64 {
    let mut n = 0;
    let cutoff = SystemTime::now()
        .checked_sub(KEEP_FOR)
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let Ok(dirs) = std::fs::read_dir(card_dir(cache)) else {
        return 0;
    };
    for d in dirs.flatten() {
        for f in std::fs::read_dir(d.path()).into_iter().flatten().flatten() {
            let old = f
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| t < cutoff);
            if old && std::fs::remove_file(f.path()).is_ok() {
                n += 1;
            }
        }
    }
    n
}

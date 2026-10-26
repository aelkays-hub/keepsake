//! Full-size pictures for the viewer's 100% zoom, made on demand.
//!
//! - JPEG, PNG, TIFF: decoded in-process (orientation applied);
//! - HEIF: decoded by the FFmpeg sidecar at full resolution;
//! - RAW: decoded by LibRaw's `dcraw_emu` sidecar (ADR-023), camera white
//!   balance, sRGB, AHD demosaic; LibRaw applies the camera orientation.
//!
//! The result is a JPEG in the rendition cache ([`Variant::Full`] or
//! [`Variant::FullRaw`]), evicted like any other rendition. Originals are
//! only read.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use image::DynamicImage;
use serde::Serialize;

use super::recipe::{Variant, FULL_QUALITY};
use super::source::{is_raw, locate, lower_ext};
use super::{still, Cache, RenderError, RenderTools};
use crate::catalog::{AssetId, Catalog, MediaKind};
use crate::library::Library;
use crate::probe::exiftool::path_arg;

/// Which member of an asset to show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Member {
    /// The JPEG (or HEIF/PNG/TIFF) of the asset; a RAW-only asset's RAW.
    Primary,
    /// The RAW of a RAW + JPEG pair (decoded by LibRaw).
    Raw,
}

/// What the viewer can toggle between for an asset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Members {
    /// The asset has both a RAW and a JPEG/HEIF to choose from.
    pub has_pair: bool,
    /// The primary member is itself a RAW (RAW-only asset).
    pub raw_only: bool,
}

pub fn members(cat: &Catalog, lib: &Library, asset: AssetId) -> Result<Members, RenderError> {
    let src = locate(cat, lib, asset)?;
    let raw_only = is_raw(&src.still);
    Ok(Members {
        has_pair: src.raw.is_some() && !raw_only,
        raw_only,
    })
}

fn encode(img: &DynamicImage) -> Result<Vec<u8>, RenderError> {
    let mut out = Vec::new();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, FULL_QUALITY);
    img.to_rgb8().write_with_encoder(enc)?;
    Ok(out)
}

/// Decode a RAW file at full size with LibRaw (`dcraw_emu ... -Z -` writes
/// a PPM to stdout).
pub fn decode_raw(dcraw: &Path, path: &Path) -> Result<DynamicImage, RenderError> {
    let ok = std::fs::metadata(dcraw).is_ok_and(|m| m.is_file());
    if !ok {
        return Err(RenderError::Failed(format!(
            "LibRaw (dcraw_emu) not found at {}; run scripts/setup",
            dcraw.display()
        )));
    }
    let mut cmd = Command::new(dcraw);
    cmd.args(["-w", "-o", "1", "-q", "3", "-Z", "-"])
        .arg(path_arg(path)?);
    let out = crate::probe::process::run("dcraw_emu", cmd, Duration::from_secs(120))?;
    if !out.success || out.stdout.is_empty() {
        return Err(RenderError::Failed(format!(
            "LibRaw could not decode {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(DynamicImage::ImageRgb8(still::parse_ppm(&out.stdout)?))
}

/// The full-size picture of `asset` (making it if needed).
pub fn full_image(
    cat: &mut Catalog,
    lib: &Library,
    cache: &Cache,
    tools: &RenderTools,
    asset: AssetId,
    member: Member,
) -> Result<PathBuf, RenderError> {
    let src = locate(cat, lib, asset)?;
    if src.media_kind == MediaKind::Video {
        return Err(RenderError::Unsupported("full size of a video".into()));
    }
    let (path, variant) = match member {
        Member::Raw => match &src.raw {
            Some(r) => (r.clone(), Variant::FullRaw),
            None => return Err(RenderError::Unsupported("the asset has no RAW file".into())),
        },
        Member::Primary if is_raw(&src.still) => (src.still.clone(), Variant::FullRaw),
        Member::Primary => (src.still.clone(), Variant::Full),
    };
    if let Some((_, p)) = cache.find(cat, &src.hash, variant)? {
        return Ok(p);
    }
    let ext = lower_ext(&path);
    let img = if variant == Variant::FullRaw {
        decode_raw(&tools.dcraw, &path)?
    } else if matches!(ext.as_str(), "hif" | "heif" | "heic") {
        DynamicImage::ImageRgb8(still::ffmpeg_frame(
            &tools.ffmpeg,
            &["-i".into(), path_arg(&path)?],
            None,
        )?)
    } else {
        still::decode(&std::fs::read(&path)?, None)?
    };
    let bytes = encode(&img)?;
    cache.put_bytes(cat, &src.hash, variant, &bytes)
}

/// Frame rate, start timecode and duration of a video, for the viewer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VideoInfo {
    pub fps_num: u32,
    pub fps_den: u32,
    /// `HH:MM:SS:FF` (`;` before the frames when drop-frame).
    pub start_timecode: Option<String>,
    pub duration_ms: Option<i64>,
    pub width: u32,
    pub height: u32,
}

pub fn video_info(
    cat: &Catalog,
    lib: &Library,
    tools: &RenderTools,
    asset: AssetId,
) -> Result<VideoInfo, RenderError> {
    let src = locate(cat, lib, asset)?;
    let m = crate::probe::ffprobe::probe(&tools.ffprobe, &src.original, Duration::from_secs(30))?;
    let v = m
        .video
        .ok_or_else(|| RenderError::Unsupported("no video stream".into()))?;
    let (num, den) = v.fps.map(|f| (f.num, f.den)).unwrap_or((30, 1));
    Ok(VideoInfo {
        fps_num: num,
        fps_den: den.max(1),
        start_timecode: m.timecode,
        duration_ms: m.duration_ms.or(src.duration_ms),
        width: v.width,
        height: v.height,
    })
}

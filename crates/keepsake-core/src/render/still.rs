//! Previews and thumbnails.
//!
//! How the full-size picture is obtained:
//! - JPEG, PNG, TIFF: decoded in-process (`image`), EXIF orientation applied;
//! - RAW (ARW, DNG, …): the largest embedded JPEG. ExifTool (stay-open)
//!   reports where it is (`PreviewImage`, `JpgFromRaw`, `OtherImage`); the
//!   bytes are read directly from the file. Orientation comes from the RAW;
//! - HEIF/HIF: decoded by the FFmpeg sidecar (ADR-021);
//! - video: the frame at 10 % of the duration, via FFmpeg (rotation applied
//!   by FFmpeg; AVCHD segments joined first).
//!
//! The preview is fitted to 2560 px (never enlarged); the 1024 px
//! thumbnail is made from the preview, the 256 px one from that. All are
//! JPEG at [`JPEG_QUALITY`].

use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use fast_image_resize::{images::Image as FirImage, PixelType, ResizeOptions, Resizer};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageReader, RgbImage};

use super::recipe::{Variant, JPEG_QUALITY};
use super::source::{lower_ext, Source};
use super::{Cache, RenderError, RenderTools};
use crate::catalog::{Catalog, MediaKind};
use crate::probe::exiftool::path_arg;
use crate::probe::ExifTool;

const FFMPEG: &str = "ffmpeg";

fn is_raster(ext: &str) -> bool {
    matches!(ext, "jpg" | "jpeg" | "png" | "tif" | "tiff")
}

fn is_heif(ext: &str) -> bool {
    matches!(ext, "hif" | "heif" | "heic")
}

/// Decode an encoded image, applying its EXIF orientation (or `orientation`
/// when given, e.g. the RAW's for an embedded preview).
pub fn decode(bytes: &[u8], orientation: Option<Orientation>) -> Result<DynamicImage, RenderError> {
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut decoder = reader.into_decoder()?;
    let own = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation.unwrap_or(own));
    Ok(img)
}

/// Fit within `long_edge` pixels (never enlarging), as 8-bit RGB.
pub fn fit(img: &DynamicImage, long_edge: u32) -> Result<DynamicImage, RenderError> {
    let rgb = match img {
        DynamicImage::ImageRgb8(_) => img.clone(),
        other => DynamicImage::ImageRgb8(other.to_rgb8()),
    };
    let (w, h) = (rgb.width(), rgb.height());
    if w == 0 || h == 0 {
        return Err(RenderError::Image("empty image".into()));
    }
    let long = w.max(h);
    if long <= long_edge {
        return Ok(rgb);
    }
    let scale = f64::from(long_edge) / f64::from(long);
    let nw = ((f64::from(w) * scale).round() as u32).max(1);
    let nh = ((f64::from(h) * scale).round() as u32).max(1);
    let src = FirImage::from_vec_u8(w, h, rgb.into_rgb8().into_raw(), PixelType::U8x3)
        .map_err(|e| RenderError::Image(e.to_string()))?;
    let mut dst = FirImage::new(nw, nh, PixelType::U8x3);
    Resizer::new()
        .resize(&src, &mut dst, &ResizeOptions::new())
        .map_err(|e| RenderError::Image(e.to_string()))?;
    let out = RgbImage::from_raw(nw, nh, dst.into_vec())
        .ok_or_else(|| RenderError::Image("resize produced a bad buffer".into()))?;
    Ok(DynamicImage::ImageRgb8(out))
}

pub fn encode_jpeg(img: &DynamicImage) -> Result<Vec<u8>, RenderError> {
    let mut out = Vec::new();
    let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
    img.to_rgb8().write_with_encoder(enc)?;
    Ok(out)
}

/// The largest embedded JPEG of a RAW file and the RAW's orientation.
pub fn embedded_preview(
    exiftool: &ExifTool,
    path: &Path,
) -> Result<(Vec<u8>, Option<Orientation>), RenderError> {
    let args: Vec<String> = [
        "-j",
        "-n",
        "-q",
        "-q",
        "-PreviewImageStart",
        "-PreviewImageLength",
        "-JpgFromRawStart",
        "-JpgFromRawLength",
        "-OtherImageStart",
        "-OtherImageLength",
        "-Orientation",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain(std::iter::once(path_arg(path)?))
    .collect();
    let out = exiftool.execute(&args)?;
    let v: serde_json::Value = serde_json::from_str(out.trim())
        .map_err(|e| RenderError::Failed(format!("exiftool output: {e}")))?;
    let tags = v.get(0).cloned().unwrap_or_default();
    let num = |k: &str| tags.get(k).and_then(serde_json::Value::as_u64);
    let file_len = std::fs::metadata(path)?.len();
    let best = ["PreviewImage", "JpgFromRaw", "OtherImage"]
        .iter()
        .filter_map(|t| {
            let start = num(&format!("{t}Start"))?;
            let len = num(&format!("{t}Length"))?;
            (len > 0 && start.checked_add(len)? <= file_len).then_some((start, len))
        })
        .max_by_key(|(_, len)| *len)
        .ok_or_else(|| {
            RenderError::Unsupported(format!("{} (no embedded preview)", path.display()))
        })?;
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(best.0))?;
    let mut buf = vec![0u8; usize::try_from(best.1).unwrap_or(0)];
    f.read_exact(&mut buf)?;
    if !buf.starts_with(&[0xFF, 0xD8]) {
        return Err(RenderError::Failed("embedded preview is not a JPEG".into()));
    }
    let orientation = num("Orientation")
        .and_then(|o| u8::try_from(o).ok())
        .and_then(Orientation::from_exif);
    Ok((buf, orientation))
}

/// FFmpeg inputs for a source: one file, or AVCHD segments joined with the
/// concat demuxer (the list goes in `list`).
pub(crate) fn ffmpeg_inputs(src: &Source, list: &Path) -> Result<Vec<String>, RenderError> {
    if src.segments.is_empty() {
        return Ok(vec!["-i".into(), path_arg(&src.original)?]);
    }
    let mut text = String::new();
    for p in std::iter::once(&src.original).chain(src.segments.iter()) {
        // Forward slashes work on Windows and need no escaping; quotes are
        // escaped the way the concat demuxer expects.
        let s = path_arg(p)?.replace('\\', "/").replace('\'', r"'\''");
        text.push_str(&format!("file '{s}'\n"));
    }
    std::fs::write(list, text)?;
    Ok(vec![
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        path_arg(list)?,
    ])
}

/// Parse a binary PPM (P6, 8-bit), as written by FFmpeg.
pub(crate) fn parse_ppm(b: &[u8]) -> Result<RgbImage, RenderError> {
    let bad = || RenderError::Failed("ffmpeg returned no frame".into());
    let mut fields = Vec::new();
    let mut i = 0;
    while fields.len() < 4 {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if b.get(i) == Some(&b'#') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        if start == i {
            return Err(bad());
        }
        fields.push(std::str::from_utf8(&b[start..i]).map_err(|_| bad())?);
    }
    i += 1; // the single whitespace after maxval
    if fields[0] != "P6" || fields[3] != "255" {
        return Err(bad());
    }
    let w: u32 = fields[1].parse().map_err(|_| bad())?;
    let h: u32 = fields[2].parse().map_err(|_| bad())?;
    let need = (w as usize) * (h as usize) * 3;
    let data = b.get(i..i + need).ok_or_else(bad)?;
    RgbImage::from_raw(w, h, data.to_vec()).ok_or_else(bad)
}

/// One frame decoded by FFmpeg: `input` args, seeking to `at_s` first.
pub(crate) fn ffmpeg_frame(
    ffmpeg: &Path,
    input: &[String],
    at_s: Option<f64>,
) -> Result<RgbImage, RenderError> {
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-hide_banner", "-v", "error", "-nostdin"]);
    if let Some(t) = at_s {
        cmd.args(["-ss", &format!("{t:.3}")]);
    }
    cmd.args(input).args([
        "-frames:v",
        "1",
        "-pix_fmt",
        "rgb24",
        "-f",
        "image2pipe",
        "-c:v",
        "ppm",
        "-",
    ]);
    let out = crate::probe::process::run(FFMPEG, cmd, Duration::from_secs(120))?;
    if !out.success || out.stdout.is_empty() {
        return Err(RenderError::Failed(format!(
            "ffmpeg: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    parse_ppm(&out.stdout)
}

/// Duration of a source in seconds (catalog, else ffprobe).
pub fn duration_s(src: &Source, tools: &RenderTools) -> Option<f64> {
    if let Some(ms) = src.duration_ms.filter(|ms| *ms > 0) {
        return Some(ms as f64 / 1000.0);
    }
    crate::probe::ffprobe::probe(&tools.ffprobe, &src.original, Duration::from_secs(30))
        .ok()?
        .duration_ms
        .map(|ms| ms as f64 / 1000.0)
}

/// The full-size picture for an asset's stills.
pub fn picture(
    src: &Source,
    tools: &RenderTools,
    exiftool: Option<&ExifTool>,
    scratch: &Path,
) -> Result<DynamicImage, RenderError> {
    match src.media_kind {
        MediaKind::Video => {
            let list = scratch.with_extension("txt");
            let input = ffmpeg_inputs(src, &list);
            let at = duration_s(src, tools).map(|d| d * 0.10);
            let r = input.and_then(|i| ffmpeg_frame(&tools.ffmpeg, &i, at));
            let _ = std::fs::remove_file(&list);
            Ok(DynamicImage::ImageRgb8(r?))
        }
        MediaKind::Photo => {
            let path = &src.still;
            let ext = lower_ext(path);
            if is_raster(&ext) {
                decode(&std::fs::read(path)?, None)
            } else if is_heif(&ext) {
                let img = ffmpeg_frame(&tools.ffmpeg, &["-i".into(), path_arg(path)?], None)?;
                Ok(DynamicImage::ImageRgb8(img))
            } else {
                let et = exiftool.ok_or_else(|| {
                    RenderError::Failed("ExifTool is needed for RAW previews".into())
                })?;
                let (bytes, orientation) = embedded_preview(et, path)?;
                decode(&bytes, orientation)
            }
        }
    }
}

/// Make whichever of the preview and thumbnails are missing.
pub fn render_stills(
    cat: &mut Catalog,
    cache: &Cache,
    src: &Source,
    tools: &RenderTools,
    exiftool: Option<&ExifTool>,
) -> Result<Vec<(Variant, PathBuf)>, RenderError> {
    let mut missing = Vec::new();
    for v in Variant::STILLS {
        if cache.find(cat, &src.hash, v)?.is_none() {
            missing.push(v);
        }
    }
    if missing.is_empty() {
        return Ok(Vec::new());
    }
    let scratch = cache.temp_path("frame")?;
    let full = picture(src, tools, exiftool, &scratch)?;
    let preview = fit(&full, 2560)?;
    drop(full);
    let t1024 = fit(&preview, 1024)?;
    let t256 = fit(&t1024, 256)?;
    let mut made = Vec::new();
    for (v, img) in [
        (Variant::Preview2560, &preview),
        (Variant::Thumb1024, &t1024),
        (Variant::Thumb256, &t256),
    ] {
        if missing.contains(&v) {
            let bytes = encode_jpeg(img)?;
            made.push((v, cache.put_bytes(cat, &src.hash, v, &bytes)?));
        }
    }
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppm_parses_with_comments() {
        let mut b = b"P6\n# by ffmpeg\n2 1\n255\n".to_vec();
        b.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        let img = parse_ppm(&b).unwrap();
        assert_eq!(img.dimensions(), (2, 1));
        assert_eq!(img.get_pixel(1, 0).0, [4, 5, 6]);
        assert!(parse_ppm(b"P6\n2 1\n255\n\x01").is_err());
        assert!(parse_ppm(b"").is_err());
    }

    #[test]
    fn fit_never_enlarges_and_keeps_aspect() {
        let img = DynamicImage::ImageRgb8(RgbImage::new(4000, 3000));
        let p = fit(&img, 2560).unwrap();
        assert_eq!((p.width(), p.height()), (2560, 1920));
        let t = fit(&p, 256).unwrap();
        assert_eq!((t.width(), t.height()), (256, 192));
        let small = DynamicImage::ImageRgb8(RgbImage::new(100, 50));
        assert_eq!(fit(&small, 256).unwrap().width(), 100);
        let tall = DynamicImage::ImageRgb8(RgbImage::new(1080, 1920));
        let t = fit(&tall, 1024).unwrap();
        assert_eq!((t.width(), t.height()), (576, 1024));
    }

    #[test]
    fn orientation_is_applied() {
        let mut img = RgbImage::new(4, 2);
        img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        let jpeg = encode_jpeg(&DynamicImage::ImageRgb8(img)).unwrap();
        let turned = decode(&jpeg, Some(Orientation::Rotate90)).unwrap();
        assert_eq!((turned.width(), turned.height()), (2, 4));
        let same = decode(&jpeg, None).unwrap();
        assert_eq!((same.width(), same.height()), (4, 2));
    }
}

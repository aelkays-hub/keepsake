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
//!
//! When only the thumbnails are wanted (a full cache), a camera JPEG's
//! built-in MPF preview (Sony: 1616×1080, at the end of the file) is used
//! instead of the whole picture, if it is at least 1024 px: a few hundred
//! KB read instead of the whole file.

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
use crate::catalog::{Catalog, CatalogError, MediaKind, Rendition};
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

/// A JPEG that needs no turning: no EXIF orientation, or "upright". Reads
/// the headers only. Anything else (another format, a broken file) is `false`.
pub fn is_upright_jpeg(bytes: &[u8]) -> bool {
    let Ok(reader) = ImageReader::new(Cursor::new(bytes)).with_guessed_format() else {
        return false;
    };
    if reader.format() != Some(image::ImageFormat::Jpeg) {
        return false;
    }
    match reader.into_decoder() {
        Ok(mut d) => matches!(d.orientation(), Ok(Orientation::NoTransforms)),
        Err(_) => false,
    }
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

/// How much of a JPEG's start is read to find its built-in preview.
const JPEG_HEAD: u64 = 256 * 1024;

fn be_or_le(le: bool, b: &[u8]) -> Option<u32> {
    let b: [u8; 4] = b.get(..4)?.try_into().ok()?;
    Some(if le {
        u32::from_le_bytes(b)
    } else {
        u32::from_be_bytes(b)
    })
}

fn u16_of(le: bool, b: &[u8]) -> Option<u16> {
    let b: [u8; 2] = b.get(..2)?.try_into().ok()?;
    Some(if le {
        u16::from_le_bytes(b)
    } else {
        u16::from_be_bytes(b)
    })
}

/// Where a JPEG's largest built-in preview is (file offset, length), from
/// the start of the file (`head`): the MPF segment (CIPA DC-007) that
/// cameras write, e.g. Sony's 1616×1080 "large thumbnail", stored after
/// the main picture. `None` without one.
pub fn mpf_preview_at(head: &[u8]) -> Option<(u64, u64)> {
    if !head.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2usize;
    // The markers before the picture data: APPn segments with lengths.
    let (tiff_at, tiff) = loop {
        if *head.get(i)? != 0xFF {
            return None;
        }
        let marker = *head.get(i + 1)?;
        if marker == 0xFF {
            i += 1;
            continue;
        }
        if marker == 0xDA || marker == 0xD9 {
            return None;
        }
        let len = usize::from(u16_of(false, head.get(i + 2..)?)?);
        let data = head.get(i + 4..i + 2 + len)?;
        if marker == 0xE2 && data.starts_with(b"MPF\0") {
            break (i + 8, &data[4..]);
        }
        i += 2 + len;
    };
    let le = match tiff.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let ifd = usize::try_from(be_or_le(le, tiff.get(4..)?)?).ok()?;
    let count = usize::from(u16_of(le, tiff.get(ifd..)?)?);
    // MPEntry (0xB002): 16 bytes per picture; the first is the main one.
    let entries = (0..count).find_map(|k| {
        let e = tiff.get(ifd + 2 + 12 * k..ifd + 14 + 12 * k)?;
        (u16_of(le, e)? == 0xB002).then(|| {
            let n = be_or_le(le, &e[4..])?;
            let at = be_or_le(le, &e[8..])?;
            Some((usize::try_from(at).ok()?, usize::try_from(n).ok()?))
        })?
    })?;
    let (at, n) = entries;
    let table = tiff.get(at..at.checked_add(n)?)?;
    table
        .as_chunks::<16>()
        .0
        .iter()
        .skip(1)
        .filter_map(|e| {
            let size = u64::from(be_or_le(le, &e[4..])?);
            let offset = u64::from(be_or_le(le, &e[8..])?);
            (size > 0 && offset > 0).then(|| (tiff_at as u64 + offset, size))
        })
        .max_by_key(|(_, size)| *size)
}

/// A JPEG's built-in preview, turned like the main picture, when it is at
/// least `min_long_edge` pixels: reads the file's start and the preview
/// only (about 200 KB of an 18 MB Sony JPEG). `None` when there is none
/// or it cannot be used; the caller then decodes the whole file.
pub fn jpeg_builtin_preview(
    path: &Path,
    min_long_edge: u32,
) -> Result<Option<DynamicImage>, RenderError> {
    let mut f = std::fs::File::open(path)?;
    let file_len = f.metadata()?.len();
    let mut head = Vec::new();
    (&mut f).take(JPEG_HEAD).read_to_end(&mut head)?;
    let Some((at, len)) = mpf_preview_at(&head) else {
        return Ok(None);
    };
    if at.checked_add(len).is_none_or(|end| end > file_len) || len > 16 * 1024 * 1024 {
        return Ok(None);
    }
    // The main picture's orientation, from its headers.
    let orientation = ImageReader::new(Cursor::new(&head))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_decoder().ok())
        .and_then(|mut d| d.orientation().ok());
    let Some(orientation) = orientation else {
        return Ok(None);
    };
    f.seek(SeekFrom::Start(at))?;
    let mut buf = vec![0u8; usize::try_from(len).unwrap_or(0)];
    f.read_exact(&mut buf)?;
    if !buf.starts_with(&[0xFF, 0xD8]) {
        return Ok(None);
    }
    let Ok(img) = decode(&buf, Some(orientation)) else {
        return Ok(None);
    };
    Ok((img.width().max(img.height()) >= min_long_edge).then_some(img))
}

/// A smaller picture to make the thumbnails from, when one is at hand
/// without reading the whole original (a camera JPEG's built-in preview).
fn thumbnail_picture(src: &Source) -> Result<Option<DynamicImage>, RenderError> {
    if src.media_kind != MediaKind::Photo {
        return Ok(None);
    }
    match lower_ext(&src.still).as_str() {
        "jpg" | "jpeg" => jpeg_builtin_preview(&src.still, 1024),
        _ => Ok(None),
    }
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
    render_stills_of(cat, cache, src, tools, exiftool, &Variant::STILLS)
}

/// Make whichever of `wanted` (some of [`Variant::STILLS`]) are missing:
/// the thumbnails alone while the cache is nearly full, say.
pub fn render_stills_of(
    cat: &mut Catalog,
    cache: &Cache,
    src: &Source,
    tools: &RenderTools,
    exiftool: Option<&ExifTool>,
    wanted: &[Variant],
) -> Result<Vec<(Variant, PathBuf)>, RenderError> {
    let mut missing = Vec::new();
    for v in Variant::STILLS {
        if wanted.contains(&v) && cache.find(cat, &src.hash, v)?.is_none() {
            missing.push(v);
        }
    }
    let made = make_stills(cache, src, tools, exiftool, &missing)?;
    if !made.is_empty() {
        cat.write(|tx| {
            for (_, _, row) in &made {
                tx.renditions().upsert(row)?;
            }
            Ok::<_, CatalogError>(())
        })?;
    }
    Ok(made.into_iter().map(|(v, p, _)| (v, p)).collect())
}

/// Make the renditions in `missing` (some of [`Variant::STILLS`]) and put
/// their files in the cache, without recording them: returns the rows for
/// the caller to write (the thumbnail backfill writes a whole batch in one
/// transaction). Existing renditions are not looked at.
pub fn make_stills(
    cache: &Cache,
    src: &Source,
    tools: &RenderTools,
    exiftool: Option<&ExifTool>,
    missing: &[Variant],
) -> Result<Vec<(Variant, PathBuf, Rendition)>, RenderError> {
    if missing.is_empty() {
        return Ok(Vec::new());
    }
    // Thumbnails alone: from a camera JPEG's built-in preview when it has
    // one, so the whole file is not read (much faster on a network drive).
    let quick = if missing.contains(&Variant::Preview2560) {
        None
    } else {
        thumbnail_picture(src)?
    };
    let preview = match quick {
        Some(img) => img,
        None => {
            let scratch = cache.temp_path("frame")?;
            let full = picture(src, tools, exiftool, &scratch)?;
            fit(&full, 2560)?
        }
    };
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
            let (path, row) = cache.store_bytes(&src.hash, v, &bytes)?;
            made.push((v, path, row));
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

    #[test]
    fn upright_jpegs_are_told_apart() {
        let jpeg = encode_jpeg(&DynamicImage::ImageRgb8(RgbImage::new(4, 2))).unwrap();
        assert!(is_upright_jpeg(&jpeg));
        let mut png = Vec::new();
        DynamicImage::ImageRgb8(RgbImage::new(4, 2))
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert!(!is_upright_jpeg(&png));
        assert!(!is_upright_jpeg(b"not a picture"));
    }
}

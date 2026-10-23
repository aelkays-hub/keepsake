//! Sony XAVC `NonRealTimeMeta` sidecars (`C0001M01.XML`).
//!
//! Elements are matched by local name because the namespace URI carries a
//! schema version that changes between camera generations.

use serde::{Deserialize, Serialize};

use super::error::ProbeError;
use super::ffprobe::Fps;
use crate::catalog::CaptureTime;
use crate::xmp::dom::{self, Element};
use crate::xmp::parse_date;

/// SMPTE timecode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timecode {
    pub hours: u8,
    pub minutes: u8,
    pub seconds: u8,
    pub frames: u8,
    pub drop_frame: bool,
}

impl std::fmt::Display for Timecode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let sep = if self.drop_frame { ';' } else { ':' };
        write!(
            f,
            "{:02}:{:02}:{:02}{sep}{:02}",
            self.hours, self.minutes, self.seconds, self.frames
        )
    }
}

fn bcd(b: u8) -> Option<u8> {
    let (hi, lo) = (b >> 4, b & 0x0F);
    (hi <= 9 && lo <= 9).then_some(hi * 10 + lo)
}

impl Timecode {
    /// Decode Sony's packed LTC value: four bytes `FF SS MM HH` written as
    /// eight hex digits, each byte BCD with flag bits (frames bit 6 = drop
    /// frame; the top bits of the other bytes are SMPTE flags).
    pub fn from_sony_ltc(value: &str) -> Option<Self> {
        let v = value.trim();
        if v.len() != 8 {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&v[i * 2..i * 2 + 2], 16).ok();
        let (ff, ss, mm, hh) = (byte(0)?, byte(1)?, byte(2)?, byte(3)?);
        let tc = Timecode {
            frames: bcd(ff & 0x3F)?,
            drop_frame: ff & 0x40 != 0,
            seconds: bcd(ss & 0x7F)?,
            minutes: bcd(mm & 0x7F)?,
            hours: bcd(hh & 0x3F)?,
        };
        (tc.seconds < 60 && tc.minutes < 60 && tc.hours < 24).then_some(tc)
    }
}

/// GPS position (decimal degrees).
pub type Gps = crate::catalog::GpsPoint;

/// Facts from a Sony XML sidecar.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SonyXml {
    /// `CreationDate`, with the camera's UTC offset.
    pub creation: Option<CaptureTime>,
    pub duration_frames: Option<u64>,
    pub capture_fps: Option<Fps>,
    pub format_fps: Option<Fps>,
    /// Duration in milliseconds (frames at the format frame rate), truncated.
    pub duration_ms: Option<i64>,
    pub timecode: Option<Timecode>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub lens: Option<String>,
    pub recording_mode: Option<String>,
    pub video_codec: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub gps: Option<Gps>,
}

/// Parse a Sony frame-rate string such as `59.94p`, `25p`, `50i` or `23.98p`.
pub fn parse_sony_fps(s: &str) -> Option<Fps> {
    let t = s.trim().trim_end_matches(['p', 'i', 'P', 'I']);
    let ntsc = |n: u32| Fps {
        num: n * 1000,
        den: 1001,
    };
    Some(match t {
        "23.98" | "23.976" => ntsc(24),
        "29.97" => ntsc(30),
        "47.95" => ntsc(48),
        "59.94" => ntsc(60),
        "119.88" => ntsc(120),
        "239.76" => ntsc(240),
        _ => {
            let n: u32 = t.parse().ok().filter(|n| *n > 0)?;
            Fps { num: n, den: 1 }
        }
    })
}

fn find<'a>(e: &'a Element, local: &str) -> Option<&'a Element> {
    e.elements().find(|c| c.local == local)
}

fn find_deep<'a>(e: &'a Element, local: &str) -> Option<&'a Element> {
    for c in e.elements() {
        if c.local == local {
            return Some(c);
        }
        if let Some(x) = find_deep(c, local) {
            return Some(x);
        }
    }
    None
}

fn attr<'a>(e: &'a Element, name: &str) -> Option<&'a str> {
    e.attrs
        .iter()
        .find(|a| a.local == name && !a.is_ns_decl())
        .map(|a| a.value.as_str())
        .filter(|v| !v.trim().is_empty())
}

fn owned(s: Option<&str>) -> Option<String> {
    s.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty())
}

/// `35:39:57.123`, `35 39 57.123`, `35,39.95` or `35.6659` → decimal degrees.
fn parse_dms(s: &str) -> Option<f64> {
    let parts: Vec<f64> = s
        .split([':', ' ', ','])
        .filter(|p| !p.is_empty())
        .map(parse_number)
        .collect::<Option<Vec<_>>>()?;
    match parts.as_slice() {
        [d] => Some(*d),
        [d, m] => Some(d + m / 60.0),
        [d, m, s] => Some(d + m / 60.0 + s / 3600.0),
        _ => None,
    }
}

/// Decimal or rational (`41/1`).
fn parse_number(s: &str) -> Option<f64> {
    let s = s.trim();
    match s.split_once('/') {
        Some((n, d)) => {
            let d: f64 = d.trim().parse().ok()?;
            (d != 0.0).then(|| n.trim().parse::<f64>().ok().map(|n| n / d))?
        }
        None => s.parse().ok(),
    }
}

fn read_gps(root: &Element) -> Option<Gps> {
    let acq = find_deep(root, "AcquisitionRecord")?;
    let group = acq
        .elements()
        .find(|g| g.local == "Group" && attr(g, "name") == Some("ExifGPS"))?;
    let item = |name: &str| {
        group
            .elements()
            .find(|i| i.local == "Item" && attr(i, "name") == Some(name))
            .and_then(|i| attr(i, "value"))
    };
    if item("Status").is_some_and(|s| s.trim().eq_ignore_ascii_case("V")) {
        return None; // receiver reported "void" (no fix)
    }
    let mut lat = parse_dms(item("Latitude")?)?;
    let mut lon = parse_dms(item("Longitude")?)?;
    if item("LatitudeRef").is_some_and(|r| r.trim().eq_ignore_ascii_case("S")) {
        lat = -lat.abs();
    }
    if item("LongitudeRef").is_some_and(|r| r.trim().eq_ignore_ascii_case("W")) {
        lon = -lon.abs();
    }
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    let alt_m = item("Altitude").and_then(parse_number).map(|a| {
        if item("AltitudeRef").is_some_and(|r| r.trim() == "1") {
            -a
        } else {
            a
        }
    });
    Some(Gps { lat, lon, alt_m })
}

/// Parse a Sony `NonRealTimeMeta` document.
pub fn parse(xml: &str) -> Result<SonyXml, ProbeError> {
    let doc = dom::parse(xml).map_err(|e| ProbeError::parse("Sony XML", e))?;
    let root = &doc.root;
    if root.local != "NonRealTimeMeta" {
        return Err(ProbeError::parse(
            "Sony XML",
            format!("root element is <{}>, not NonRealTimeMeta", root.local),
        ));
    }
    let mut x = SonyXml {
        creation: find(root, "CreationDate")
            .and_then(|e| attr(e, "value"))
            .and_then(parse_date),
        ..Default::default()
    };

    if let Some(frame) = find_deep(root, "VideoFrame") {
        x.capture_fps = attr(frame, "captureFps").and_then(parse_sony_fps);
        x.format_fps = attr(frame, "formatFps").and_then(parse_sony_fps);
        x.video_codec = owned(attr(frame, "videoCodec"));
    }
    if let Some(layout) = find_deep(root, "VideoLayout") {
        x.width = attr(layout, "pixel").and_then(|v| v.trim().parse().ok());
        x.height = attr(layout, "numOfVerticalLine").and_then(|v| v.trim().parse().ok());
    }
    x.duration_frames = find(root, "Duration")
        .and_then(|e| attr(e, "value"))
        .and_then(|v| v.trim().parse().ok());
    // Duration counts frames at the playback (format) rate; S&Q clips record faster.
    if let (Some(frames), Some(fps)) = (x.duration_frames, x.format_fps.or(x.capture_fps)) {
        let ms = u128::from(frames) * u128::from(fps.den) * 1000 / u128::from(fps.num);
        x.duration_ms = i64::try_from(ms).ok();
    }
    if let Some(table) = find(root, "LtcChangeTable") {
        x.timecode = table
            .elements()
            .filter(|c| c.local == "LtcChange")
            .find(|c| attr(c, "frameCount").map(str::trim) == Some("0"))
            .and_then(|c| attr(c, "value"))
            .and_then(Timecode::from_sony_ltc);
    }
    if let Some(dev) = find(root, "Device") {
        x.make = owned(attr(dev, "manufacturer"));
        x.model = owned(attr(dev, "modelName"));
        x.serial = owned(attr(dev, "serialNo"));
    }
    x.lens = find(root, "Lens").and_then(|e| owned(attr(e, "modelName")));
    x.recording_mode = find(root, "RecordingMode").and_then(|e| owned(attr(e, "type")));
    x.gps = read_gps(root);
    Ok(x)
}

/// Read and parse a sidecar file.
pub fn read(path: &std::path::Path) -> Result<SonyXml, ProbeError> {
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    parse(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ltc_decoding() {
        let tc = Timecode::from_sony_ltc("40000000").unwrap();
        assert_eq!(tc.to_string(), "00:00:00;00");
        let tc = Timecode::from_sony_ltc("54890000").unwrap();
        assert_eq!((tc.seconds, tc.frames, tc.drop_frame), (9, 14, true));
        let tc = Timecode::from_sony_ltc("12345601").unwrap();
        assert_eq!(tc.to_string(), "01:56:34:12");
        assert!(Timecode::from_sony_ltc("0000006A").is_none()); // bad BCD
        assert!(Timecode::from_sony_ltc("123").is_none());
    }

    #[test]
    fn fps_strings() {
        assert_eq!(
            parse_sony_fps("59.94p"),
            Some(Fps {
                num: 60000,
                den: 1001
            })
        );
        assert_eq!(parse_sony_fps("25p"), Some(Fps { num: 25, den: 1 }));
        assert_eq!(parse_sony_fps("50i"), Some(Fps { num: 50, den: 1 }));
        assert_eq!(
            parse_sony_fps("23.98p"),
            Some(Fps {
                num: 24000,
                den: 1001
            })
        );
        assert_eq!(parse_sony_fps("x"), None);
    }

    #[test]
    fn dms() {
        assert!((parse_dms("35:39:57.0").unwrap() - 35.665_833).abs() < 1e-5);
        assert!((parse_dms("35.5").unwrap() - 35.5).abs() < 1e-9);
        assert!((parse_number("41/2").unwrap() - 20.5).abs() < 1e-9);
    }
}

//! `ffprobe` for container and stream facts.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::error::ProbeError;
use super::process;

const TOOL: &str = "ffprobe";

/// Chroma subsampling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Chroma {
    Yuv420,
    Yuv422,
    Yuv444,
    Gray,
    Rgb,
}

/// A rational frame rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fps {
    pub num: u32,
    pub den: u32,
}

impl Fps {
    pub fn as_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den.max(1))
    }
}

/// The main video stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoStream {
    pub codec: String,
    pub profile: Option<String>,
    pub width: u32,
    pub height: u32,
    pub fps: Option<Fps>,
    pub pix_fmt: Option<String>,
    pub bit_depth: Option<u8>,
    pub chroma: Option<Chroma>,
    /// Clockwise rotation to apply for display: 0, 90, 180 or 270.
    pub rotation_cw: u16,
    pub color_transfer: Option<String>,
    pub field_order: Option<String>,
}

/// One audio stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioStream {
    pub codec: String,
    pub channels: Option<u32>,
    pub sample_rate: Option<u32>,
}

/// What ffprobe reports about a file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaStreams {
    pub format: Option<String>,
    /// Truncated to whole milliseconds.
    pub duration_ms: Option<i64>,
    /// Container creation time (UTC; the camera's local offset is not stored).
    pub creation_utc_ms: Option<i64>,
    pub timecode: Option<String>,
    pub video: Option<VideoStream>,
    pub audio: Vec<AudioStream>,
}

/// Bit depth and chroma from an FFmpeg pixel format name.
pub fn pix_fmt_info(pix_fmt: &str) -> (Option<u8>, Option<Chroma>) {
    let f = pix_fmt.to_ascii_lowercase();
    let depth = |s: &str| -> Option<u8> {
        let digits: String = s
            .trim_end_matches("le")
            .trim_end_matches("be")
            .chars()
            .rev()
            .take_while(char::is_ascii_digit)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        digits.parse().ok()
    };
    // Semi-planar formats: nv12/nv21 (8-bit 4:2:0), p010/p016 (4:2:0), p210/p216 (4:2:2), p410 (4:4:4).
    match f.as_str() {
        "nv12" | "nv21" => return (Some(8), Some(Chroma::Yuv420)),
        "nv16" => return (Some(8), Some(Chroma::Yuv422)),
        "nv24" | "nv42" => return (Some(8), Some(Chroma::Yuv444)),
        _ => {}
    }
    if let Some(rest) = f.strip_prefix('p') {
        let d: String = rest.chars().take(3).collect();
        if d.len() == 3 && d.chars().all(|c| c.is_ascii_digit()) {
            let chroma = match &d[..1] {
                "0" => Chroma::Yuv420,
                "2" => Chroma::Yuv422,
                "4" => Chroma::Yuv444,
                _ => return (None, None),
            };
            let bits: u8 = d[1..].parse().unwrap_or(0);
            return (Some(if bits == 16 { 16 } else { bits }), Some(chroma));
        }
    }
    let chroma = if f.starts_with("yuv420") || f.starts_with("yuvj420") || f.starts_with("yuva420")
    {
        Some(Chroma::Yuv420)
    } else if f.starts_with("yuv422") || f.starts_with("yuvj422") || f.starts_with("yuva422") {
        Some(Chroma::Yuv422)
    } else if f.starts_with("yuv444") || f.starts_with("yuvj444") || f.starts_with("yuva444") {
        Some(Chroma::Yuv444)
    } else if f.starts_with("gray") {
        Some(Chroma::Gray)
    } else if f.starts_with("rgb") || f.starts_with("bgr") || f.starts_with("gbr") {
        Some(Chroma::Rgb)
    } else {
        None
    };
    let bits = match chroma {
        Some(Chroma::Rgb) if f.starts_with("rgb24") || f.starts_with("bgr24") => Some(8),
        Some(_) => depth(&f).filter(|d| (9..=16).contains(d)).or(Some(8)),
        None => None,
    };
    (bits, chroma)
}

fn parse_rational(s: &str) -> Option<Fps> {
    let (n, d) = s.split_once('/')?;
    let (num, den): (u32, u32) = (n.parse().ok()?, d.parse().ok()?);
    (num > 0 && den > 0).then_some(Fps { num, den })
}

fn str_field(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_owned)
}

fn tag(v: &Value, k: &str) -> Option<String> {
    v.get("tags")?.as_object()?.iter().find_map(|(key, val)| {
        key.eq_ignore_ascii_case(k)
            .then(|| val.as_str().map(str::to_owned))
            .flatten()
    })
}

fn rotation_cw(stream: &Value) -> u16 {
    // Display matrix side data: `rotation` is counter-clockwise degrees.
    let from_side = stream
        .get("side_data_list")
        .and_then(Value::as_array)
        .and_then(|l| {
            l.iter()
                .find_map(|sd| sd.get("rotation").and_then(Value::as_f64))
        })
        .map(|ccw| -ccw);
    // Older files: `rotate` tag, clockwise degrees.
    let from_tag = || tag(stream, "rotate").and_then(|s| s.parse::<f64>().ok());
    let deg = from_side.or_else(from_tag).unwrap_or(0.0);
    let norm = ((deg.round() as i64 % 360) + 360) % 360;
    // Snap to the nearest quarter turn.
    (((norm + 45) / 90 % 4) * 90) as u16
}

/// Parse `ffprobe -show_format -show_streams -of json` output.
pub fn parse(json: &str) -> Result<MediaStreams, ProbeError> {
    let v: Value = serde_json::from_str(json).map_err(|e| ProbeError::parse("ffprobe JSON", e))?;
    let streams = v
        .get("streams")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let format = v.get("format").cloned().unwrap_or(Value::Null);

    let duration_ms = str_field(&format, "duration")
        .and_then(|d| d.parse::<f64>().ok())
        .map(|s| (s * 1000.0).floor() as i64);
    let creation_utc_ms = tag(&format, "creation_time")
        .or_else(|| streams.iter().find_map(|s| tag(s, "creation_time")))
        .and_then(|t| DateTime::parse_from_rfc3339(&t).ok())
        .map(|t| t.timestamp_millis());
    let timecode =
        tag(&format, "timecode").or_else(|| streams.iter().find_map(|s| tag(s, "timecode")));

    let is_cover = |s: &Value| {
        s.get("disposition")
            .and_then(|d| d.get("attached_pic"))
            .and_then(Value::as_i64)
            == Some(1)
    };
    let video = streams
        .iter()
        .find(|s| str_field(s, "codec_type").as_deref() == Some("video") && !is_cover(s))
        .map(|s| {
            let pix_fmt = str_field(s, "pix_fmt");
            let (mut bit_depth, chroma) =
                pix_fmt.as_deref().map(pix_fmt_info).unwrap_or((None, None));
            if let Some(b) = str_field(s, "bits_per_raw_sample").and_then(|b| b.parse::<u8>().ok())
            {
                if b > 0 && bit_depth.is_none() {
                    bit_depth = Some(b);
                }
            }
            VideoStream {
                codec: str_field(s, "codec_name").unwrap_or_default(),
                profile: str_field(s, "profile"),
                width: s.get("width").and_then(Value::as_u64).unwrap_or(0) as u32,
                height: s.get("height").and_then(Value::as_u64).unwrap_or(0) as u32,
                fps: str_field(s, "r_frame_rate")
                    .and_then(|r| parse_rational(&r))
                    .or_else(|| str_field(s, "avg_frame_rate").and_then(|r| parse_rational(&r))),
                pix_fmt,
                bit_depth,
                chroma,
                rotation_cw: rotation_cw(s),
                color_transfer: str_field(s, "color_transfer"),
                field_order: str_field(s, "field_order"),
            }
        });
    let audio = streams
        .iter()
        .filter(|s| str_field(s, "codec_type").as_deref() == Some("audio"))
        .map(|s| AudioStream {
            codec: str_field(s, "codec_name").unwrap_or_default(),
            channels: s.get("channels").and_then(Value::as_u64).map(|c| c as u32),
            sample_rate: str_field(s, "sample_rate").and_then(|r| r.parse().ok()),
        })
        .collect();

    Ok(MediaStreams {
        format: str_field(&format, "format_name"),
        duration_ms,
        creation_utc_ms,
        timecode,
        video,
        audio,
    })
}

/// Run ffprobe on one file.
pub fn probe(ffprobe: &Path, file: &Path, timeout: Duration) -> Result<MediaStreams, ProbeError> {
    let ok = std::fs::metadata(ffprobe).is_ok_and(|m| m.is_file() && m.len() > 0);
    if !ok {
        return Err(ProbeError::ToolMissing {
            tool: TOOL,
            path: ffprobe.to_path_buf(),
        });
    }
    let mut cmd = Command::new(ffprobe);
    cmd.args([
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ])
    .arg(file);
    let out = process::run(TOOL, cmd, timeout)?;
    if !out.success {
        return Err(ProbeError::Tool {
            tool: TOOL,
            detail: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        });
    }
    parse(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_formats() {
        assert_eq!(pix_fmt_info("yuv420p"), (Some(8), Some(Chroma::Yuv420)));
        assert_eq!(pix_fmt_info("yuvj420p"), (Some(8), Some(Chroma::Yuv420)));
        assert_eq!(
            pix_fmt_info("yuv422p10le"),
            (Some(10), Some(Chroma::Yuv422))
        );
        assert_eq!(
            pix_fmt_info("yuv420p10le"),
            (Some(10), Some(Chroma::Yuv420))
        );
        assert_eq!(
            pix_fmt_info("yuv444p12be"),
            (Some(12), Some(Chroma::Yuv444))
        );
        assert_eq!(pix_fmt_info("p010le"), (Some(10), Some(Chroma::Yuv420)));
        assert_eq!(pix_fmt_info("p210le"), (Some(10), Some(Chroma::Yuv422)));
        assert_eq!(pix_fmt_info("nv12"), (Some(8), Some(Chroma::Yuv420)));
        assert_eq!(pix_fmt_info("gray10le"), (Some(10), Some(Chroma::Gray)));
        assert_eq!(pix_fmt_info("rgb24"), (Some(8), Some(Chroma::Rgb)));
        assert_eq!(pix_fmt_info("weird"), (None, None));
    }

    #[test]
    fn parses_sony_like_output() {
        let json = r#"{"streams":[
          {"codec_type":"video","codec_name":"hevc","profile":"Main 4:2:2 10","width":3840,"height":2160,
           "pix_fmt":"yuv422p10le","r_frame_rate":"60000/1001","color_transfer":"arib-std-b67",
           "side_data_list":[{"side_data_type":"Display Matrix","rotation":-90}],
           "tags":{"creation_time":"2026-10-14T23:00:22.000000Z"}},
          {"codec_type":"audio","codec_name":"pcm_s16be","channels":2,"sample_rate":"48000"},
          {"codec_type":"data","tags":{"timecode":"01:02:03;04"}}],
          "format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"9.509500",
           "tags":{"creation_time":"2026-10-14T23:00:22.000000Z"}}}"#;
        let m = parse(json).unwrap();
        assert_eq!(m.duration_ms, Some(9509));
        assert_eq!(m.creation_utc_ms, Some(1_792_018_822_000));
        assert_eq!(m.timecode.as_deref(), Some("01:02:03;04"));
        let v = m.video.unwrap();
        assert_eq!((v.bit_depth, v.chroma), (Some(10), Some(Chroma::Yuv422)));
        assert_eq!(v.rotation_cw, 90);
        assert_eq!(
            v.fps,
            Some(Fps {
                num: 60000,
                den: 1001
            })
        );
        assert_eq!(m.audio.len(), 1);
    }

    #[test]
    fn rotation_variants() {
        let r = |side: Option<f64>, tag: Option<&str>| {
            let mut s = serde_json::json!({});
            if let Some(x) = side {
                s["side_data_list"] = serde_json::json!([{"rotation": x}]);
            }
            if let Some(t) = tag {
                s["tags"] = serde_json::json!({"rotate": t});
            }
            rotation_cw(&s)
        };
        assert_eq!(r(None, None), 0);
        assert_eq!(r(Some(90.0), None), 270);
        assert_eq!(r(Some(-90.0), None), 90);
        assert_eq!(r(Some(180.0), None), 180);
        assert_eq!(r(Some(-180.0), None), 180);
        assert_eq!(r(None, Some("90")), 90);
        assert_eq!(r(None, Some("270")), 270);
    }
}

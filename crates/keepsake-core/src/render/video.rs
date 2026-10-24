//! Video proxies: the camera's H.264 SUB proxy copied into the cache, or a
//! 720p H.264 + AAC MP4 made with the settings proven in the playback
//! spike (ADR-009): `scale=-2:720`, AAC 128 kb/s, `+faststart`, hardware
//! encoders first. The GOP is set explicitly to one second (`-g` = frame
//! rate) so seeking stays instant whatever the encoder's default.
//!
//! The webview must never get HEVC or MTS (ADR-009): a camera proxy is used
//! only if it is H.264; anything else is transcoded.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;

use super::recipe::{Variant, PROXY_AUDIO_KBPS, PROXY_HEIGHT};
use super::source::Source;
use super::still::ffmpeg_inputs;
use super::{Cache, RenderError, RenderTools};
use crate::catalog::Catalog;
use crate::probe::exiftool::path_arg;

const FFMPEG: &str = "ffmpeg";

/// An H.264 encoder and its settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Encoder {
    pub name: String,
    pub args: Vec<String>,
    /// Pixel format the encoder is fed.
    pub pix_fmt: String,
}

fn enc(name: &str, args: &[&str], pix_fmt: &str) -> Encoder {
    Encoder {
        name: name.into(),
        args: args.iter().map(|s| s.to_string()).collect(),
        pix_fmt: pix_fmt.into(),
    }
}

/// Encoders to try, best first (spike settings where the spike measured
/// them; ADR-021 for the others).
pub fn candidates() -> Vec<Encoder> {
    let mut v = Vec::new();
    if cfg!(target_os = "macos") {
        v.push(enc("h264_videotoolbox", &["-b:v", "4M"], "yuv420p"));
    } else {
        v.push(enc(
            "h264_nvenc",
            &["-preset", "p4", "-b:v", "4M"],
            "yuv420p",
        ));
        v.push(enc("h264_qsv", &["-global_quality", "25"], "nv12"));
        v.push(enc(
            "h264_amf",
            &["-quality", "balanced", "-b:v", "4M"],
            "nv12",
        ));
        if cfg!(windows) {
            v.push(enc("h264_mf", &["-q:v", "20"], "nv12"));
        }
    }
    // Software, in the LGPL build everywhere.
    v.push(enc("libopenh264", &["-b:v", "4M"], "yuv420p"));
    v
}

fn works(ffmpeg: &Path, e: &Encoder) -> bool {
    let mut cmd = Command::new(ffmpeg);
    cmd.args([
        "-hide_banner",
        "-v",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
    ])
    .arg("testsrc2=size=320x240:rate=25:duration=1")
    .args(["-frames:v", "10", "-vf", &format!("format={}", e.pix_fmt)])
    .args(["-c:v", &e.name])
    .args(&e.args)
    .args(["-g", "25", "-f", "null", "-"]);
    crate::probe::process::run(FFMPEG, cmd, Duration::from_secs(20)).is_ok_and(|o| o.success)
}

/// (ffmpeg path, forced name, encoder found).
type Detected = Vec<(PathBuf, Option<String>, Encoder)>;
static DETECTED: OnceLock<Mutex<Detected>> = OnceLock::new();

/// The first encoder that works on this machine (tested once per process
/// with a tiny encode). `forced` picks one by name.
pub fn detect(ffmpeg: &Path, forced: Option<&str>) -> Result<Encoder, RenderError> {
    let memo = DETECTED.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(m) = memo.lock() {
        if let Some((_, _, e)) = m
            .iter()
            .find(|(p, f, _)| p == ffmpeg && f.as_deref() == forced)
        {
            return Ok(e.clone());
        }
    }
    let list: Vec<Encoder> = match forced {
        Some(name) => {
            let c = candidates()
                .into_iter()
                .find(|e| e.name == name)
                .unwrap_or_else(|| enc(name, &["-b:v", "4M"], "yuv420p"));
            vec![c]
        }
        None => candidates(),
    };
    let found = list
        .into_iter()
        .find(|e| works(ffmpeg, e))
        .ok_or_else(|| RenderError::Failed("no working H.264 encoder in ffmpeg".into()))?;
    tracing::info!(encoder = %found.name, "proxy encoder");
    if let Ok(mut m) = memo.lock() {
        m.push((
            ffmpeg.to_path_buf(),
            forced.map(str::to_owned),
            found.clone(),
        ));
    }
    Ok(found)
}

/// Is this camera proxy H.264 (safe for the webview)?
pub fn camera_proxy_is_h264(tools: &RenderTools, path: &Path) -> bool {
    crate::probe::ffprobe::probe(&tools.ffprobe, path, Duration::from_secs(30))
        .ok()
        .and_then(|m| m.video)
        .is_some_and(|v| v.codec == "h264")
}

/// Copy the camera's H.264 proxy into the cache. `None` if there is none or
/// it is not H.264.
pub fn copy_camera_proxy(
    cat: &mut Catalog,
    cache: &Cache,
    src: &Source,
    tools: &RenderTools,
) -> Result<Option<PathBuf>, RenderError> {
    let Some(p) = &src.camera_proxy else {
        return Ok(None);
    };
    if !camera_proxy_is_h264(tools, p) {
        return Ok(None);
    }
    let tmp = cache.temp_path("mp4")?;
    if let Err(e) = std::fs::copy(p, &tmp) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(Some(cache.put_file(
        cat,
        &src.hash,
        Variant::CameraProxy,
        &tmp,
    )?))
}

/// The ffmpeg arguments after the inputs, for a source with frame rate
/// `fps`, interlaced or not.
pub fn proxy_args(e: &Encoder, fps: f64, interlaced: bool, out: &str) -> Vec<String> {
    let gop = (fps.round() as i64).max(1);
    let mut vf = String::new();
    if interlaced {
        vf.push_str("yadif=mode=0,");
    }
    vf.push_str(&format!(
        "scale=-2:'min({PROXY_HEIGHT},ih)',format={}",
        e.pix_fmt
    ));
    let mut a: Vec<String> = [
        "-map", "0:v:0", "-map", "0:a:0?", "-sn", "-dn", "-vf", &vf, "-c:v", &e.name,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    a.extend(e.args.iter().cloned());
    a.extend([
        "-g".to_string(),
        gop.to_string(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        format!("{PROXY_AUDIO_KBPS}k"),
        "-ac".into(),
        "2".into(),
        "-movflags".into(),
        "+faststart".into(),
        "-y".into(),
        out.to_owned(),
    ]);
    a
}

/// Encode a 720p H.264 proxy into the cache.
pub fn make_proxy(
    cat: &mut Catalog,
    cache: &Cache,
    src: &Source,
    tools: &RenderTools,
    encoder: &Encoder,
) -> Result<PathBuf, RenderError> {
    let streams =
        crate::probe::ffprobe::probe(&tools.ffprobe, &src.original, Duration::from_secs(60))?;
    let video = streams.video.as_ref().ok_or_else(|| {
        RenderError::Unsupported(format!("{} (no video stream)", src.original.display()))
    })?;
    let fps = video
        .fps
        .map(|f| f.as_f64())
        .filter(|f| *f > 0.0)
        .unwrap_or(30.0);
    let interlaced = video
        .field_order
        .as_deref()
        .is_some_and(|f| matches!(f, "tt" | "bb" | "tb" | "bt"));
    let secs = super::still::duration_s(src, tools).unwrap_or(600.0);
    let tmp = cache.temp_path("mp4")?;
    let list = tmp.with_extension("txt");
    let result = (|| -> Result<(), RenderError> {
        let mut cmd = Command::new(&tools.ffmpeg);
        cmd.args(["-hide_banner", "-v", "error", "-nostdin"])
            .args(ffmpeg_inputs(src, &list)?)
            .args(proxy_args(encoder, fps, interlaced, &path_arg(&tmp)?));
        // Generous: software encoding runs at about 5x real time.
        let timeout = Duration::from_secs(120 + (secs * 4.0) as u64);
        let out = crate::probe::process::run(FFMPEG, cmd, timeout)?;
        if !out.success {
            return Err(RenderError::Failed(format!(
                "ffmpeg ({}): {}",
                encoder.name,
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        let check = crate::probe::ffprobe::probe(&tools.ffprobe, &tmp, Duration::from_secs(30))?;
        if check.video.as_ref().map(|v| v.codec.as_str()) != Some("h264") {
            return Err(RenderError::Failed("the proxy is not H.264".into()));
        }
        Ok(())
    })();
    let _ = std::fs::remove_file(&list);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    cache.put_file(cat, &src.hash, Variant::Proxy720, &tmp)
}

/// Make the proxy of a video asset unless it exists: the camera's H.264
/// proxy if there is one, else an encode. Returns what was made.
pub fn render_proxy(
    cat: &mut Catalog,
    cache: &Cache,
    src: &Source,
    tools: &RenderTools,
    forced_encoder: Option<&str>,
) -> Result<Option<(Variant, PathBuf, Option<String>)>, RenderError> {
    for v in [Variant::CameraProxy, Variant::Proxy720] {
        if cache.find(cat, &src.hash, v)?.is_some() {
            return Ok(None);
        }
    }
    if let Some(p) = copy_camera_proxy(cat, cache, src, tools)? {
        return Ok(Some((Variant::CameraProxy, p, None)));
    }
    let e = detect(&tools.ffmpeg, forced_encoder)?;
    let p = make_proxy(cat, cache, src, tools, &e)?;
    Ok(Some((Variant::Proxy720, p, Some(e.name))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_args_set_gop_scale_and_audio() {
        let e = enc("h264_mf", &["-q:v", "20"], "nv12");
        let a = proxy_args(&e, 29.97, false, "out.mp4");
        let s = a.join(" ");
        assert!(s.contains("-g 30"), "{s}");
        assert!(s.contains("scale=-2:'min(720,ih)',format=nv12"), "{s}");
        assert!(s.contains("-c:v h264_mf -q:v 20"), "{s}");
        assert!(s.contains("-c:a aac -b:a 128k -ac 2"), "{s}");
        assert!(s.contains("-movflags +faststart"), "{s}");
        assert!(s.ends_with("-y out.mp4"));
        let a = proxy_args(&e, 59.94, true, "o.mp4").join(" ");
        assert!(
            a.contains("yadif=mode=0,scale") && a.contains("-g 60"),
            "{a}"
        );
    }

    #[test]
    fn software_fallback_is_always_a_candidate() {
        let c = candidates();
        assert_eq!(c.last().map(|e| e.name.as_str()), Some("libopenh264"));
    }
}

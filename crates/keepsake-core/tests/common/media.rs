//! Generate small test media with the vendored FFmpeg and ExifTool.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use keepsake_core::probe::{ExifTool, ToolPaths};

/// Tools plus ffmpeg, or `None` when they are not installed. On CI a missing
/// tool is a failure, not a skip.
pub fn tools() -> Option<(ToolPaths, PathBuf)> {
    let vendor = super::workspace_root().join("vendor");
    let t = ToolPaths::in_vendor(&vendor);
    let ffmpeg = vendor.join(if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    });
    let works = |p: &Path, arg: &str| {
        std::fs::metadata(p).is_ok_and(|m| m.len() > 0)
            && Command::new(p)
                .arg(arg)
                .output()
                .is_ok_and(|o| o.status.success())
    };
    let ok =
        works(&t.exiftool, "-ver") && works(&t.ffprobe, "-version") && works(&ffmpeg, "-version");
    if !ok {
        if std::env::var_os("CI").is_some() {
            panic!("CI must provide ExifTool, ffprobe and ffmpeg in vendor/ (scripts/setup.ps1 -VendorOnly)");
        }
        eprintln!("skipping: ExifTool/ffprobe/ffmpeg not found in vendor/");
        return None;
    }
    Some((t, ffmpeg))
}

fn run(ffmpeg: &Path, args: &[&str]) {
    let out = Command::new(ffmpeg)
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ffmpeg {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn mkparent(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
}

/// 1-second H.264 MP4 (320x240, 25 fps) with container creation time and timecode.
pub fn h264_mp4(ffmpeg: &Path, out: &Path, creation_utc: &str, timecode: Option<&str>) {
    mkparent(out);
    let creation = format!("creation_time={creation_utc}");
    let mut args = vec![
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=320x240:rate=25",
        "-t",
        "1",
        "-c:v",
        "libopenh264",
        "-metadata",
        &creation,
    ];
    if let Some(tc) = timecode {
        args.extend(["-timecode", tc]);
    }
    let o = out.to_str().unwrap();
    args.push(o);
    run(ffmpeg, &args);
}

/// Copy `src` to `out` with a display-matrix rotation (counter-clockwise degrees).
pub fn rotate_copy(ffmpeg: &Path, src: &Path, out: &Path, ccw: i32) {
    mkparent(out);
    let r = ccw.to_string();
    run(
        ffmpeg,
        &[
            "-display_rotation",
            &r,
            "-i",
            src.to_str().unwrap(),
            "-c",
            "copy",
            out.to_str().unwrap(),
        ],
    );
}

/// 1-second 10-bit 4:2:2 ProRes MOV.
pub fn prores_10bit(ffmpeg: &Path, out: &Path) {
    mkparent(out);
    run(
        ffmpeg,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=25",
            "-t",
            "1",
            "-c:v",
            "prores_ks",
            "-profile:v",
            "3",
            "-pix_fmt",
            "yuv422p10le",
            out.to_str().unwrap(),
        ],
    );
}

/// An MPEG-TS segment of `secs` seconds.
pub fn ts_segment(ffmpeg: &Path, out: &Path, secs: u32) {
    mkparent(out);
    let t = secs.to_string();
    run(
        ffmpeg,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=25",
            "-t",
            &t,
            "-c:v",
            "mpeg2video",
            "-f",
            "mpegts",
            out.to_str().unwrap(),
        ],
    );
}

/// A small JPEG.
pub fn jpeg(ffmpeg: &Path, out: &Path) {
    mkparent(out);
    run(
        ffmpeg,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=64x48",
            "-frames:v",
            "1",
            out.to_str().unwrap(),
        ],
    );
}

/// Write EXIF tags into a test file (never used on originals).
pub fn set_tags(et: &ExifTool, file: &Path, tags: &[&str]) {
    let mut args: Vec<String> = vec!["-overwrite_original".into(), "-q".into()];
    args.extend(tags.iter().map(|t| t.to_string()));
    args.push(file.to_str().unwrap().to_owned());
    et.execute(&args).unwrap();
}

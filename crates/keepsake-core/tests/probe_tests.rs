//! Probing: Sony XML fixtures (always), and ExifTool / ffprobe / end-to-end
//! tests on media generated at test time (need the vendored tools; required on CI).

mod common;

use std::path::Path;
use std::time::Duration;

use common::media;
use keepsake_core::catalog::{CaptureSource, CaptureTime, Confidence, FileRole, SequenceKind};
use keepsake_core::grouping::{self, GroupKind};
use keepsake_core::probe::avchd::synth;
use keepsake_core::probe::{sony_xml, Chroma, ExifTool, ProbeError, ProbeOptions, Prober};
use keepsake_core::rebuild::{group_folder, list_media_files, rebuild_catalog_with};
use keepsake_core::relpath::{CaseSensitivity, RelPath};
use keepsake_core::xmp::{format_date, parse_date};

fn fixture_xml(name: &str) -> sony_xml::SonyXml {
    sony_xml::read(&common::fixture(&format!("sony_xml/{name}"))).unwrap()
}

// ── Sony XML (no tools needed) ───────────────────────────────────────────────

#[test]
fn sony_xml_full_with_gps_and_slow_motion() {
    let x = fixture_xml("xavc_hs_sq_gps.xml");
    assert_eq!(
        x.creation.map(format_date).as_deref(),
        Some("2024-06-15T13:30:00.000+09:00")
    );
    // 240 frames at the 23.98p playback rate (recorded at 119.88p).
    assert_eq!(x.duration_frames, Some(240));
    assert_eq!(x.duration_ms, Some(10_010));
    assert_eq!(x.capture_fps.map(|f| (f.num, f.den)), Some((120_000, 1001)));
    assert_eq!(x.timecode.unwrap().to_string(), "01:56:34:12");
    assert_eq!(x.make.as_deref(), Some("Sony"));
    assert_eq!(x.model.as_deref(), Some("ILCE-7SM3"));
    assert_eq!(x.serial.as_deref(), Some("00000001"));
    assert_eq!(x.lens.as_deref(), Some("FE 24-70mm F2.8 GM II"));
    assert_eq!(x.recording_mode.as_deref(), Some("normal"));
    assert_eq!((x.width, x.height), (Some(3840), Some(2160)));
    let g = x.gps.unwrap();
    assert!((g.lat - 35.66).abs() < 1e-6, "{}", g.lat);
    assert!((g.lon - 139.7005).abs() < 1e-6, "{}", g.lon);
    assert_eq!(g.alt_m, Some(41.0));
}

#[test]
fn sony_xml_void_gps_and_interlaced() {
    let x = fixture_xml("gps_void_southwest.xml");
    assert_eq!(x.gps, None, "Status=V means no fix");
    assert_eq!(x.duration_ms, Some(60_000)); // 3000 frames at 50i
    assert_eq!(x.creation.unwrap().utc_offset_min, Some(-180));
}

#[test]
fn sony_xml_missing_fields_and_corpus_style_stub() {
    let x = fixture_xml("no_date.xml");
    assert_eq!(x.creation, None);
    assert_eq!(x.duration_ms, None);
    assert_eq!(x.model, None);
    let stub = sony_xml::read(&common::fixture("xavc/PRIVATE/M4ROOT/CLIP/C0001M01.XML")).unwrap();
    assert_eq!(stub.duration_ms, Some(1000));
    assert_eq!(stub.model.as_deref(), Some("ILCE-7M4"));
    assert!(sony_xml::parse("<NotSony/>").is_err());
    assert!(sony_xml::parse("not xml").is_err());
}

// ── ExifTool process ─────────────────────────────────────────────────────────

#[test]
fn exiftool_reads_and_writes_on_unicode_paths_with_spaces() {
    let Some((t, ffmpeg)) = media::tools() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("Été 2024").join("Fête du lac 01.jpg");
    media::jpeg(&ffmpeg, &f);
    let et = ExifTool::start(&t.exiftool, Duration::from_secs(30)).unwrap();
    media::set_tags(
        &et,
        &f,
        &[
            "-DateTimeOriginal=2024:06:15 13:30:00",
            "-SubSecTimeOriginal=05",
            "-OffsetTimeOriginal=+09:00",
            "-Make=SONY",
            "-Model=ILCE-7M4",
            "-GPSLatitude=35.66",
            "-GPSLatitudeRef=N",
            "-GPSLongitude=139.70",
            "-GPSLongitudeRef=W",
        ],
    );
    let tags = et.read_tags(&[f.as_path()]).unwrap();
    let m = &tags[0];
    assert_eq!(m["Model"], "ILCE-7M4");
    assert_eq!(m["DateTimeOriginal"], "2024:06:15 13:30:00");
    // Leading zero kept: "05" is 50 ms, not 5.
    assert_eq!(m["SubSecTimeOriginal"], "05");
    assert_eq!(m["OffsetTimeOriginal"], "+09:00");
    // Composite GPS is signed.
    assert!((m["GPSLongitude"].as_f64().unwrap() + 139.70).abs() < 1e-6);
}

#[test]
fn exiftool_serves_concurrent_requests_in_one_process() {
    let Some((t, ffmpeg)) = media::tools() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("a.jpg");
    media::jpeg(&ffmpeg, &f);
    let et = ExifTool::start(&t.exiftool, Duration::from_secs(30)).unwrap();
    std::thread::scope(|s| {
        for _ in 0..6 {
            s.spawn(|| {
                for _ in 0..5 {
                    let v = et.read_tags(&[f.as_path()]).unwrap();
                    assert_eq!(v.len(), 1);
                    assert_eq!(v[0]["MIMEType"], "image/jpeg");
                }
            });
        }
    });
    assert_eq!(et.restarts(), 0);
}

#[test]
fn exiftool_reports_missing_files() {
    let Some((t, ffmpeg)) = media::tools() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ok = dir.path().join("a.jpg");
    media::jpeg(&ffmpeg, &ok);
    let et = ExifTool::start(&t.exiftool, Duration::from_secs(30)).unwrap();
    let missing = dir.path().join("nope.jpg");
    assert!(et.read_tags(&[missing.as_path()]).is_err());
    // A batch with one missing file still returns the good one.
    let v = et.read_tags(&[ok.as_path(), missing.as_path()]).unwrap();
    assert_eq!(v[0]["MIMEType"], "image/jpeg");
    assert!(v[1].is_empty());
    assert!(matches!(
        et.execute(&["bad\narg".to_string()]),
        Err(ProbeError::BadPath { .. })
    ));
    assert!(ExifTool::start(&dir.path().join("no-exiftool"), Duration::from_secs(1)).is_err());
}

/// A stand-in for ExifTool that hangs on its first start and behaves
/// normally afterwards, to exercise timeout + restart.
fn hanging_once_wrapper(dir: &Path, real: &Path) -> std::path::PathBuf {
    let marker = dir.join("started-once");
    if cfg!(windows) {
        let p = dir.join("fake-exiftool.cmd");
        std::fs::write(
            &p,
            format!(
                "@echo off\r\nif exist \"{m}\" (\r\n  \"{r}\" %*\r\n) else (\r\n  echo x> \"{m}\"\r\n  ping -n 8 127.0.0.1 >nul\r\n)\r\n",
                m = marker.display(),
                r = real.display()
            ),
        )
        .unwrap();
        p
    } else {
        let p = dir.join("fake-exiftool.sh");
        std::fs::write(
            &p,
            format!(
                "#!/bin/sh\nif [ -e '{m}' ]; then exec '{r}' \"$@\"; else touch '{m}'; sleep 8; fi\n",
                m = marker.display(),
                r = real.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        p
    }
}

#[test]
fn exiftool_times_out_then_restarts() {
    let Some((t, ffmpeg)) = media::tools() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("a.jpg");
    media::jpeg(&ffmpeg, &f);
    let fake = hanging_once_wrapper(dir.path(), &t.exiftool);
    let et = ExifTool::start(&fake, Duration::from_secs(2)).unwrap();
    let started = std::time::Instant::now();
    let first = et.read_tags(&[f.as_path()]);
    assert!(
        matches!(first, Err(ProbeError::Timeout { .. })),
        "{first:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "timeout not enforced"
    );
    assert_eq!(et.restarts(), 1);
    // The next request starts a fresh, working process.
    let second = et
        .execute_with_timeout(
            &["-j".into(), "-MIMEType".into(), f.to_str().unwrap().into()],
            Duration::from_secs(30),
        )
        .unwrap();
    assert!(second.contains("image/jpeg"), "{second}");
}

// ── ffprobe ──────────────────────────────────────────────────────────────────

#[test]
fn ffprobe_reads_codec_duration_rotation_timecode_and_creation() {
    let Some((t, ffmpeg)) = media::tools() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.mp4");
    let rotated = dir.path().join("rot.mp4");
    media::h264_mp4(&ffmpeg, &a, "2024-06-15T04:30:00Z", Some("01:02:03:04"));
    media::rotate_copy(&ffmpeg, &a, &rotated, 90);
    let orig =
        keepsake_core::probe::ffprobe::probe(&t.ffprobe, &a, Duration::from_secs(30)).unwrap();
    assert_eq!(
        orig.creation_utc_ms,
        parse_date("2024-06-15T04:30:00Z").map(|c| c.utc_ms)
    );
    assert_eq!(orig.video.as_ref().unwrap().rotation_cw, 0);
    let s = keepsake_core::probe::ffprobe::probe(&t.ffprobe, &rotated, Duration::from_secs(30))
        .unwrap();
    assert_eq!(s.duration_ms, Some(1000));
    assert_eq!(s.timecode.as_deref(), Some("01:02:03:04"));
    let v = s.video.unwrap();
    assert_eq!(v.codec, "h264");
    assert_eq!((v.width, v.height), (320, 240));
    assert_eq!((v.bit_depth, v.chroma), (Some(8), Some(Chroma::Yuv420)));
    assert_eq!(v.fps.map(|f| (f.num, f.den)), Some((25, 1)));
    // 90° counter-clockwise display matrix = 270° clockwise.
    assert_eq!(v.rotation_cw, 270);
}

#[test]
fn ffprobe_reads_10_bit_422() {
    let Some((t, ffmpeg)) = media::tools() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("b.mov");
    media::prores_10bit(&ffmpeg, &f);
    let s = keepsake_core::probe::ffprobe::probe(&t.ffprobe, &f, Duration::from_secs(30)).unwrap();
    let v = s.video.unwrap();
    assert_eq!(v.codec, "prores");
    assert_eq!((v.bit_depth, v.chroma), (Some(10), Some(Chroma::Yuv422)));
}

#[test]
fn ffprobe_rejects_non_media() {
    let Some((t, _)) = media::tools() else { return };
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("x.mp4");
    std::fs::write(&f, b"STUB_MP4").unwrap();
    assert!(keepsake_core::probe::ffprobe::probe(&t.ffprobe, &f, Duration::from_secs(30)).is_err());
}

// ── End to end: grouping + probing a generated card ──────────────────────────

fn prober(t: &keepsake_core::probe::ToolPaths) -> Prober {
    Prober::new(t, ProbeOptions::default()).unwrap()
}

fn rel(s: &str) -> RelPath {
    RelPath::parse(s).unwrap()
}

/// A card with two XAVC clips, stills (burst, bracket via EXIF, offset-less
/// shot) and a split AVCHD recording.
fn build_card(card: &Path, t: &keepsake_core::probe::ToolPaths, ffmpeg: &Path) {
    // XAVC 1: XML agrees with the container. XAVC 2: container is 1 h off.
    let xml = std::fs::read_to_string(common::fixture("sony_xml/xavc_hs_sq_gps.xml")).unwrap();
    media::h264_mp4(
        ffmpeg,
        &card.join("PRIVATE/M4ROOT/CLIP/C0001.MP4"),
        "2024-06-15T04:30:00Z",
        None,
    );
    std::fs::write(card.join("PRIVATE/M4ROOT/CLIP/C0001M01.XML"), &xml).unwrap();
    media::h264_mp4(
        ffmpeg,
        &card.join("PRIVATE/M4ROOT/CLIP/C0002.MP4"),
        "2024-06-15T05:30:00Z",
        None,
    );
    std::fs::write(card.join("PRIVATE/M4ROOT/CLIP/C0002M01.XML"), &xml).unwrap();
    std::fs::create_dir_all(card.join("PRIVATE/M4ROOT/THMBNL")).unwrap();
    std::fs::write(card.join("PRIVATE/M4ROOT/THMBNL/C0001T01.JPG"), b"thumb").unwrap();
    std::fs::write(card.join("PRIVATE/M4ROOT/MEDIAPRO.XML"), b"<x/>").unwrap();

    // Stills.
    let et = ExifTool::start(&t.exiftool, Duration::from_secs(30)).unwrap();
    let dcim = card.join("DCIM/100MSDCF");
    let still = |name: &str, tags: &[&str]| {
        let p = dcim.join(name);
        media::jpeg(ffmpeg, &p);
        media::set_tags(&et, &p, tags);
    };
    // Burst by timing (no ReleaseMode2 in generic JPEGs): 100 ms apart.
    for (i, ss) in ["100", "200", "300"].iter().enumerate() {
        still(
            &format!("DSC0000{}.JPG", i + 1),
            &[
                "-DateTimeOriginal=2024:06:15 13:31:00",
                &format!("-SubSecTimeOriginal={ss}"),
                "-OffsetTimeOriginal=+09:00",
                "-Make=SONY",
                "-Model=ILCE-7M4",
            ],
        );
    }
    // Five minutes later, no offset recorded: borrows +09:00 from the burst.
    still(
        "DSC00004.JPG",
        &[
            "-DateTimeOriginal=2024:06:15 13:36:00",
            "-Make=SONY",
            "-Model=ILCE-7M4",
        ],
    );
    // A RAW companion of DSC00004 (copied bytes; only grouping matters).
    std::fs::copy(dcim.join("DSC00004.JPG"), dcim.join("DSC00004.ARW")).unwrap();

    // AVCHD: two seamless segments + MDPM date + clip info + playlist.
    let bdmv = card.join("PRIVATE/AVCHD/BDMV");
    media::ts_segment(ffmpeg, &bdmv.join("STREAM/00001.MTS"), 1);
    media::ts_segment(ffmpeg, &bdmv.join("STREAM/00002.MTS"), 2);
    let mut seg1 = std::fs::read(bdmv.join("STREAM/00001.MTS")).unwrap();
    seg1.extend(synth::mdpm(2013, 5, 3, 10, 12, 13));
    std::fs::write(bdmv.join("STREAM/00001.MTS"), seg1).unwrap();
    std::fs::create_dir_all(bdmv.join("CLIPINF")).unwrap();
    std::fs::create_dir_all(bdmv.join("PLAYLIST")).unwrap();
    std::fs::write(bdmv.join("CLIPINF/00001.CPI"), synth::cpi(1)).unwrap();
    std::fs::write(bdmv.join("CLIPINF/00002.CPI"), synth::cpi(1)).unwrap();
    std::fs::write(
        bdmv.join("PLAYLIST/00000.MPL"),
        synth::mpl(&[("00001", 1, 0, 45_000), ("00002", 5, 45_000, 135_000)]),
    )
    .unwrap();
    std::fs::write(bdmv.join("INDEX.BDM"), b"INDX0100").unwrap();
}

#[test]
fn probe_generated_card_end_to_end() {
    let Some((t, ffmpeg)) = media::tools() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let card = dir.path().join("card");
    build_card(&card, &t, &ffmpeg);

    let (files, _) = list_media_files(&card);
    let g = group_folder(&card, &files);
    assert!(g.problems.is_empty(), "{:?}", g.problems);
    let card_files: Vec<&str> = g.card_files.iter().map(|i| files[*i].as_str()).collect();
    assert_eq!(
        card_files,
        vec![
            "PRIVATE/AVCHD/BDMV/INDEX.BDM",
            "PRIVATE/AVCHD/BDMV/PLAYLIST/00000.MPL",
            "PRIVATE/M4ROOT/MEDIAPRO.XML"
        ]
    );
    let p = prober(&t);
    let report = p.probe(&card, &files, &g);
    let by_orig = |path: &str| {
        let i = g
            .groups
            .iter()
            .position(|x| files[x.original()].as_str() == path)
            .unwrap_or_else(|| panic!("{path} is not an original"));
        (&g.groups[i], &report.assets[i], i)
    };

    // XAVC 1: XML date, high; GPS, lens, timecode, XML duration; ffprobe stream info.
    let (grp, a, _) = by_orig("PRIVATE/M4ROOT/CLIP/C0001.MP4");
    assert_eq!(grp.kind, GroupKind::Xavc);
    assert_eq!(grp.members.len(), 3, "MP4 + XML + thumbnail");
    let c = a.capture.as_ref().unwrap();
    assert_eq!(
        (c.source, c.confidence),
        (CaptureSource::SonyXml, Confidence::High)
    );
    assert_eq!(format_date(c.time), "2024-06-15T13:30:00.000+09:00");
    assert_eq!(a.model.as_deref(), Some("ILCE-7SM3"));
    assert_eq!(a.lens.as_deref(), Some("FE 24-70mm F2.8 GM II"));
    assert_eq!(a.duration_ms, Some(10_010));
    assert_eq!(a.timecode.as_deref(), Some("01:56:34:12"));
    assert!(a.gps.is_some());
    assert_eq!(a.video.as_ref().unwrap().codec, "h264");

    // XAVC 2: container disagrees by an hour → medium, with a note.
    let (_, a, _) = by_orig("PRIVATE/M4ROOT/CLIP/C0002.MP4");
    let c = a.capture.as_ref().unwrap();
    assert_eq!(
        (c.source, c.confidence),
        (CaptureSource::SonyXml, Confidence::Medium)
    );
    assert!(
        c.notes.iter().any(|n| n.contains("container differs")),
        "{:?}",
        c.notes
    );

    // Stills: EXIF with sub-seconds and offset; burst; borrowed offset.
    let (_, a1, i1) = by_orig("DCIM/100MSDCF/DSC00001.JPG");
    let c = a1.capture.as_ref().unwrap();
    assert_eq!(format_date(c.time), "2024-06-15T13:31:00.100+09:00");
    assert_eq!(
        (c.source, c.confidence),
        (CaptureSource::Exif, Confidence::High)
    );
    let (_, _, i2) = by_orig("DCIM/100MSDCF/DSC00002.JPG");
    let (_, _, i3) = by_orig("DCIM/100MSDCF/DSC00003.JPG");
    assert_eq!(report.sequences.len(), 1);
    assert_eq!(report.sequences[0].kind, SequenceKind::Burst);
    assert_eq!(report.sequences[0].members, vec![i1, i2, i3]);

    let (grp4, a4, _) = by_orig("DCIM/100MSDCF/DSC00004.JPG");
    assert_eq!(grp4.members[1].role, FileRole::RawPair);
    let c = a4.capture.as_ref().unwrap();
    assert_eq!(format_date(c.time), "2024-06-15T13:36:00.000+09:00");
    assert_eq!(c.confidence, Confidence::Medium);
    assert!(
        c.notes.iter().any(|n| n.contains("borrowed")),
        "{:?}",
        c.notes
    );

    // AVCHD: one recording, two segments, durations summed, MDPM date (offset unknown).
    let (grp, a, _) = by_orig("PRIVATE/AVCHD/BDMV/STREAM/00001.MTS");
    assert_eq!(grp.kind, GroupKind::Avchd);
    assert_eq!(
        grp.segments()
            .iter()
            .map(|i| files[*i].file_name())
            .collect::<Vec<_>>(),
        vec!["00001.MTS", "00002.MTS"]
    );
    let d = a.duration_ms.unwrap();
    assert!((2900..=3100).contains(&d), "summed duration {d}");
    let c = a.capture.as_ref().unwrap();
    assert_eq!(c.source, CaptureSource::AvchdMdpm);
    assert_eq!(c.confidence, Confidence::Low);
    assert_eq!(
        c.time,
        CaptureTime {
            utc_ms: parse_date("2013-05-03T10:12:13").unwrap().utc_ms,
            utc_offset_min: None
        }
    );
    assert_eq!(a.video.as_ref().unwrap().codec, "mpeg2video");
    let _ = grouping::sequences::BURST_GAP_MS;
}

#[test]
fn rebuild_with_probe_fills_catalog_and_is_stable() {
    let Some((t, ffmpeg)) = media::tools() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let card = dir.path().join("card");
    build_card(&card, &t, &ffmpeg);
    let lib = keepsake_core::library::Library::create(&dir.path().join("lib")).unwrap();
    let rec = common::volume_record("Card", CaseSensitivity::Insensitive, true);
    lib.register_volume(&rec).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let p = prober(&t);
    let r = rebuild_catalog_with(&mut cat, &lib, rec.id, &card, Some(&p)).unwrap();
    // 2 XAVC + 4 stills + 1 AVCHD.
    assert_eq!(r.assets, 7);
    assert_eq!(r.probed, 7);
    assert_eq!(r.sequences, 1);

    let find = |path: &str| {
        cat.read(|tx| tx.files().by_location(rec.id, &rel(path)))
            .unwrap()
            .unwrap()
    };
    let clip_id = find("PRIVATE/M4ROOT/CLIP/C0001.MP4").asset_id;
    let clip = cat.read(|tx| tx.assets().require(clip_id)).unwrap();
    assert_eq!(clip.capture_source, Some(CaptureSource::SonyXml));
    assert_eq!(clip.capture_confidence, Some(Confidence::High));
    assert_eq!(clip.camera_model.as_deref(), Some("ILCE-7SM3"));
    assert_eq!(clip.duration_ms, Some(10_010));
    assert!(clip.gps.is_some());
    let seg2 = find("PRIVATE/AVCHD/BDMV/STREAM/00002.MTS");
    assert_eq!(
        (seg2.role, seg2.segment_index),
        (FileRole::Segment, Some(1))
    );
    let second = find("DCIM/100MSDCF/DSC00002.JPG").asset_id;
    let burst = cat
        .read(|tx| tx.sequences().for_asset(second))
        .unwrap()
        .unwrap();
    assert_eq!(
        (burst.0.kind, burst.0.members.len(), burst.1),
        (SequenceKind::Burst, 3, 1)
    );
    // Calendar works from probed local dates.
    let days = cat.read(|tx| tx.assets().days()).unwrap();
    assert_eq!(
        days.iter().map(|d| d.date.as_str()).collect::<Vec<_>>(),
        vec!["2013-05-03", "2024-06-15"]
    );

    // Same input, same catalog.
    let before = cat.read(|tx| tx.snapshot()).unwrap();
    rebuild_catalog_with(&mut cat, &lib, rec.id, &card, Some(&p)).unwrap();
    assert_eq!(cat.read(|tx| tx.snapshot()).unwrap(), before);
}

#[test]
fn every_tag_of_a_photo_is_read_without_changing_it() {
    let Some((t, ffmpeg)) = media::tools() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("Été 2026").join("A7404962.jpg");
    media::jpeg(&ffmpeg, &f);
    let et = ExifTool::start(&t.exiftool, Duration::from_secs(30)).unwrap();
    media::set_tags(
        &et,
        &f,
        &[
            "-Make=SONY",
            "-Model=ILCE-7M4",
            "-ExposureTime=1/250",
            "-FNumber=2.8",
            "-ISO=400",
            "-XMP-dc:Subject=Lake",
        ],
    );
    let before = std::fs::read(&f).unwrap();
    let groups = et.read_all(&f).unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), before, "only read");
    let get = |g: &str, n: &str| {
        groups
            .iter()
            .find(|x| x.group == g)
            .and_then(|x| x.tags.iter().find(|t| t.name == n))
            .map(|t| t.value.clone())
    };
    assert_eq!(groups[0].group, "IFD0", "{groups:#?}");
    assert_eq!(get("IFD0", "Make").as_deref(), Some("SONY"));
    assert_eq!(get("IFD0", "Model").as_deref(), Some("ILCE-7M4"));
    assert_eq!(get("ExifIFD", "ExposureTime").as_deref(), Some("1/250"));
    assert_eq!(get("ExifIFD", "ISO").as_deref(), Some("400"));
    assert_eq!(get("XMP-dc", "Subject").as_deref(), Some("Lake"));
    assert!(get("File", "ImageWidth").is_some());
    assert_eq!(groups.last().map(|g| g.group.as_str()), Some("System"));
}

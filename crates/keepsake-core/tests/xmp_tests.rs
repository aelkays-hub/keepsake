//! XMP read/write tests, including Lightroom- and digiKam-authored sidecars.

mod common;

use keepsake_core::catalog::{
    AssetId, Blake3Hash, CaptureSource, CaptureTime, Confidence, Event, EventId, FileRole,
    GpsPoint, MediaKind, Rating, RecipeId, SequenceId, SequenceKind, TagPath,
};
use keepsake_core::relpath::RelPath;
use keepsake_core::xmp::{ns, sidecar, AssetXmp, FileRef, RecipeRef, SequenceRef, XmpPacket};

fn tag(s: &str) -> TagPath {
    TagPath::parse_lightroom(s).unwrap()
}

fn full_sample() -> AssetXmp {
    AssetXmp {
        asset_id: Some(AssetId::new_random()),
        media_kind: Some(MediaKind::Video),
        added_ms: Some(1_700_000_000_123),
        metadata_ms: Some(1_700_000_100_456),
        rating: Some(Rating::new(5).unwrap()),
        label: Some("Purple".into()),
        comment: Some("Line one\nLine <two> & \"three\"".into()),
        user_tags: vec![tag("Family"), tag("Travel|Japan|Tokyo")],
        ai_tags: vec![(tag("Outdoors|Beach"), 0.875), (tag("Sky"), 0.1)],
        capture: Some(CaptureTime {
            utc_ms: 1_718_425_800_120,
            utc_offset_min: Some(540),
        }),
        capture_shift_s: -3600,
        duration_ms: Some(12_345),
        camera_make: Some("SONY".into()),
        camera_model: Some("ILCE-7M4".into()),
        lens_model: Some("FE 24-70mm F2.8 GM II".into()),
        event: Some(Event {
            id: EventId::new_random(),
            title: "Summer trip".into(),
            place_name: Some("Tōkyō".into()),
            lat: Some(35.6762),
            lon: Some(139.6503),
            created_ms: 1_700_000_000_000,
        }),
        recipes: vec![RecipeRef {
            id: RecipeId::new_random(),
            kind: "trim".into(),
            schema_version: 1,
            rel_path: RelPath::parse("abc/def.json").unwrap(),
            content_hash: Blake3Hash::of(b"{}"),
            created_ms: 1_700_000_000_001,
            updated_ms: 1_700_000_000_002,
        }],
        files: vec![
            FileRef {
                rel_path: RelPath::parse("PRIVATE/M4ROOT/CLIP/C0001.MP4").unwrap(),
                role: FileRole::Original,
                segment_index: None,
            },
            FileRef {
                rel_path: RelPath::parse("PRIVATE/M4ROOT/SUB/C0001S01.MP4").unwrap(),
                role: FileRole::CameraProxy,
                segment_index: None,
            },
            FileRef {
                rel_path: RelPath::parse("PRIVATE/AVCHD/BDMV/STREAM/00002.MTS").unwrap(),
                role: FileRole::Segment,
                segment_index: Some(1),
            },
        ],
        capture_source: Some(CaptureSource::SonyXml),
        capture_confidence: Some(Confidence::Medium),
        gps: Some(GpsPoint {
            lat: -33.856_784,
            lon: 151.215_297,
            alt_m: Some(-12.5),
        }),
        sequence: Some(SequenceRef {
            id: SequenceId::new_random(),
            kind: SequenceKind::Burst,
            position: 3,
        }),
    }
}

#[test]
fn full_round_trip_through_text() {
    let x = full_sample();
    let mut p = XmpPacket::new();
    x.write(&mut p);
    let text = p.to_xml();
    let back = AssetXmp::read(&XmpPacket::parse(&text).unwrap());
    assert_eq!(back, x, "\n{text}");
    // Writing twice gives the same text (stable output).
    let mut p2 = XmpPacket::parse(&text).unwrap();
    back.write(&mut p2);
    assert_eq!(p2.to_xml(), text);
}

#[test]
fn clearing_fields_removes_them() {
    let mut p = XmpPacket::new();
    full_sample().write(&mut p);
    AssetXmp::default().write(&mut p);
    let text = p.to_xml();
    assert!(!text.contains("keepsake:"), "{text}");
    assert!(!text.contains("dc:subject"), "{text}");
    assert_eq!(AssetXmp::read(&p), AssetXmp::default());
}

#[test]
fn reads_lightroom_sidecar() {
    let text = std::fs::read_to_string(common::fixture("xmp/lightroom.xmp")).unwrap();
    let p = XmpPacket::parse(&text).unwrap();
    let x = AssetXmp::read(&p);
    assert_eq!(x.rating, Some(Rating::new(4).unwrap()));
    assert_eq!(x.label.as_deref(), Some("Red"));
    assert_eq!(x.comment.as_deref(), Some("Sunset at Shibuya & friends"));
    assert_eq!(x.user_tags, vec![tag("Travel|Japan|Tokyo"), tag("sunset")]);
    assert_eq!(x.camera_make.as_deref(), Some("SONY"));
    assert_eq!(x.camera_model.as_deref(), Some("ILCE-7M4"));
    let c = x.capture.unwrap();
    assert_eq!(c.utc_offset_min, Some(540));
    assert_eq!(c.utc_ms, 1_718_429_400_120); // 2024-06-15T05:30:00.12Z
    assert_eq!(x.asset_id, None);
}

#[test]
fn writing_into_lightroom_sidecar_preserves_foreign_fields() {
    let text = std::fs::read_to_string(common::fixture("xmp/lightroom.xmp")).unwrap();
    let mut p = XmpPacket::parse(&text).unwrap();
    let mut x = AssetXmp::read(&p);
    x.rating = Some(Rating::new(2).unwrap());
    x.user_tags.push(tag("Food"));
    x.user_tags.sort();
    x.asset_id = Some(AssetId::new_random());
    x.write(&mut p);
    let out = p.to_xml();
    let p2 = XmpPacket::parse(&out).unwrap();
    // Develop settings and unknown namespaces survive.
    assert_eq!(
        p2.simple(
            "http://ns.adobe.com/camera-raw-settings/1.0/",
            "Exposure2012"
        )
        .as_deref(),
        Some("+0.35")
    );
    assert_eq!(
        p2.array(
            "http://ns.adobe.com/camera-raw-settings/1.0/",
            "ToneCurvePV2012"
        ),
        vec!["0, 0".to_owned(), "255, 255".to_owned()]
    );
    assert_eq!(
        p2.simple("http://ns.adobe.com/exif/1.0/aux/", "Lens")
            .as_deref(),
        Some("FE 24-70mm F2.8 GM II")
    );
    // Lightroom reads these fields.
    assert_eq!(p2.simple(ns::XMP, "Rating").as_deref(), Some("2"));
    let mut subjects = p2.array(ns::DC, "subject");
    subjects.sort();
    assert_eq!(subjects, vec!["Food", "Japan", "Tokyo", "Travel", "sunset"]);
    assert!(p2
        .array(ns::LR, "hierarchicalSubject")
        .contains(&"Travel|Japan|Tokyo".to_owned()));
    assert_eq!(AssetXmp::read(&p2), x);
    // The attribute form was replaced, not duplicated.
    assert!(!out.contains("xmp:Rating=\""), "{out}");
    assert_eq!(out.matches("<xmp:Rating>").count(), 1, "{out}");
}

#[test]
fn reads_digikam_sidecar() {
    let text = std::fs::read_to_string(common::fixture("xmp/digikam.xmp")).unwrap();
    let p = XmpPacket::parse(&text).unwrap();
    let x = AssetXmp::read(&p);
    assert_eq!(x.rating, Some(Rating::new(3).unwrap()));
    assert_eq!(
        x.label.as_deref(),
        Some("Green"),
        "from digiKam:ColorLabel=4"
    );
    assert_eq!(
        x.user_tags,
        vec![tag("Family|Christmas"), tag("People|Grandma")]
    );
    assert_eq!(x.comment.as_deref(), Some("Christmas Eve dinner"));
    let c = x.capture.unwrap();
    assert_eq!(c.utc_offset_min, None);
}

#[test]
fn writing_into_digikam_sidecar_keeps_other_languages() {
    let text = std::fs::read_to_string(common::fixture("xmp/digikam.xmp")).unwrap();
    let mut p = XmpPacket::parse(&text).unwrap();
    let mut x = AssetXmp::read(&p);
    x.comment = Some("Dinner".into());
    x.label = Some("Blue".into());
    x.write(&mut p);
    let p2 = XmpPacket::parse(&p.to_xml()).unwrap();
    assert_eq!(
        p2.lang_alt_default(ns::DC, "description").as_deref(),
        Some("Dinner")
    );
    assert!(p.to_xml().contains("Heiligabend"));
    assert_eq!(p2.simple(ns::DIGIKAM, "ColorLabel").as_deref(), Some("5"));
    assert_eq!(
        p2.array(ns::DIGIKAM, "TagsList"),
        vec!["Family/Christmas".to_owned(), "People/Grandma".to_owned()]
    );
    assert_eq!(
        p2.simple("http://ns.microsoft.com/photo/1.0/", "Rating")
            .as_deref(),
        Some("50")
    );
}

#[test]
fn prefix_clash_gets_a_new_prefix() {
    // `keepsake` bound to some other namespace must not capture our fields.
    let src = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
      <rdf:Description rdf:about="" xmlns:keepsake="urn:other" keepsake:AssetID="not-ours"/>
    </rdf:RDF></x:xmpmeta>"#;
    let mut p = XmpPacket::parse(src).unwrap();
    assert_eq!(AssetXmp::read(&p).asset_id, None);
    let x = AssetXmp {
        asset_id: Some(AssetId::new_random()),
        ..Default::default()
    };
    x.write(&mut p);
    let p2 = XmpPacket::parse(&p.to_xml()).unwrap();
    assert_eq!(AssetXmp::read(&p2).asset_id, x.asset_id);
    assert_eq!(
        p2.simple("urn:other", "AssetID").as_deref(),
        Some("not-ours")
    );
}

#[test]
fn rejects_malformed_packets() {
    assert!(XmpPacket::parse("<notxmp/>").is_err());
    assert!(XmpPacket::parse("<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">").is_err());
    assert!(XmpPacket::parse("").is_err());
}

#[test]
fn atomic_write_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deep/folder/a.jpg.xmp");
    let x = full_sample();
    let mut p = XmpPacket::new();
    x.write(&mut p);
    let (hash, _) = sidecar::write_atomic(&path, &p).unwrap();
    let loaded = sidecar::read(&path).unwrap().unwrap();
    assert_eq!(loaded.blake3, hash);
    assert_eq!(AssetXmp::read(&loaded.packet), x);
    // No temporary files left behind.
    let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1);
    assert!(sidecar::read(&dir.path().join("missing.xmp"))
        .unwrap()
        .is_none());
}

/// Cross-check with ExifTool when it is available in `vendor/`.
#[test]
fn exiftool_reads_our_sidecar() {
    let root = common::workspace_root().join("vendor");
    let exe = [
        "exiftool/exiftool.exe",
        "exiftool/exiftool",
        "exiftool.exe",
        "exiftool",
    ]
    .iter()
    .map(|p| root.join(p))
    .find(|p| p.is_file());
    // CI has an empty placeholder in vendor/, so only use a binary that runs.
    let exe = exe.filter(|p| {
        std::fs::metadata(p).is_ok_and(|m| m.len() > 0)
            && std::process::Command::new(p)
                .arg("-ver")
                .output()
                .is_ok_and(|o| o.status.success())
    });
    let Some(exe) = exe else {
        eprintln!("skipping: no working ExifTool in vendor/");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("DSC00001.ARW.xmp");
    let mut p = XmpPacket::new();
    full_sample().write(&mut p);
    sidecar::write_atomic(&path, &p).unwrap();
    let out = std::process::Command::new(exe)
        .args(["-j", "-n", "-XMP:all"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let o = &v[0];
    assert_eq!(o["Rating"], 5);
    assert_eq!(o["Label"], "Purple");
    assert_eq!(o["Make"], "SONY");
    let subj = o["Subject"].as_array().unwrap();
    assert!(subj.iter().any(|s| s == "Tokyo"));
    assert!(o["HierarchicalSubject"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s == "Travel|Japan|Tokyo"));
    assert_eq!(o["Description"], "Line one\nLine <two> & \"three\"");
}

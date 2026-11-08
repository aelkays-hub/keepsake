//! XMP read/write tests, including Lightroom- and digiKam-authored sidecars.

mod common;

use keepsake_core::catalog::{
    AssetId, Blake3Hash, CaptureSource, CaptureTime, Confidence, DuplicateChoice, Event, EventId,
    FileRole, GpsPoint, MediaKind, Rating, RecipeId, SequenceId, SequenceKind, TagPath,
};
use keepsake_core::relpath::RelPath;
use keepsake_core::xmp::{
    ns, sidecar, ArrayKind, AssetXmp, FaceRegion, FaceRejection, FileRef, RecipeRef, SequenceRef,
    XmpPacket,
};

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
        ai_tags_rejected: vec![tag("Dog"), tag("Food|Pizza")],
        duplicate_choice: Some(DuplicateChoice::NotKept),
        ai_tag_sources: vec![
            ("claude".into(), tag("Beach")),
            ("gemini".into(), tag("AI|Sand Castle")),
            ("gemini".into(), tag("Beach")),
        ],
        face_regions: vec![
            FaceRegion {
                name: "Sara".into(),
                cx: 0.25,
                cy: 0.375,
                w: 0.125,
                h: 0.25,
            },
            FaceRegion {
                name: "Ali Ünal".into(),
                cx: 0.75,
                cy: 0.5,
                w: 0.0625,
                h: 0.125,
            },
        ],
        face_size: Some((1024, 768)),
        face_rejections: vec![FaceRejection {
            person: "Sara".into(),
            cx: 0.5,
            cy: 0.5,
            w: 0.125,
            h: 0.25,
        }],
        // Written only (read back empty).
        face_names: vec![],
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

#[test]
fn ai_tag_sources_of_an_unknown_ai_are_ignored() {
    // A sidecar written by a later version may name an AI this one does not
    // know: those rows are left out when read.
    let x = AssetXmp {
        ai_tag_sources: vec![
            ("openai".into(), tag("Dog")),
            ("someone".into(), tag("Cat")),
        ],
        ..Default::default()
    };
    let mut p = XmpPacket::new();
    x.write(&mut p);
    let back = AssetXmp::read(&XmpPacket::parse(&p.to_xml()).unwrap());
    assert_eq!(back.ai_tag_sources, vec![("openai".to_owned(), tag("Dog"))]);
}

const LIGHTROOM_FACES: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:mwg-rs="http://www.metadataworkinggroup.com/schemas/regions/"
    xmlns:stDim="http://ns.adobe.com/xap/1.0/sType/Dimensions#"
    xmlns:stArea="http://ns.adobe.com/xmp/sType/Area#">
   <mwg-rs:Regions rdf:parseType="Resource">
    <mwg-rs:AppliedToDimensions stDim:w="6000" stDim:h="4000" stDim:unit="pixel"/>
    <mwg-rs:RegionList>
     <rdf:Bag>
      <rdf:li>
       <rdf:Description mwg-rs:Name="Bob" mwg-rs:Type="Face">
        <mwg-rs:Area stArea:x="0.5" stArea:y="0.25" stArea:w="0.1" stArea:h="0.15" stArea:unit="normalized"/>
       </rdf:Description>
      </rdf:li>
      <rdf:li>
       <rdf:Description mwg-rs:Name="Rex" mwg-rs:Type="Pet">
        <mwg-rs:Area stArea:x="0.2" stArea:y="0.2" stArea:w="0.1" stArea:h="0.1" stArea:unit="normalized"/>
       </rdf:Description>
      </rdf:li>
      <rdf:li rdf:parseType="Resource">
       <mwg-rs:Type>Face</mwg-rs:Type>
       <mwg-rs:Area stArea:x="0.8" stArea:y="0.3" stArea:w="0.05" stArea:h="0.08" stArea:unit="normalized"/>
      </rdf:li>
     </rdf:Bag>
    </mwg-rs:RegionList>
   </mwg-rs:Regions>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>"#;

const MWG: &str = "http://www.metadataworkinggroup.com/schemas/regions/";

fn region_names(p: &XmpPacket) -> Vec<(String, String)> {
    let r = p.value(MWG, "Regions").unwrap();
    let mut v: Vec<(String, String)> = r
        .field(MWG, "RegionList")
        .unwrap()
        .items()
        .iter()
        .map(|i| {
            let t = |k: &str| {
                i.field(MWG, k)
                    .and_then(|v| v.as_text())
                    .unwrap_or("")
                    .to_owned()
            };
            (t("Type"), t("Name"))
        })
        .collect();
    v.sort();
    v
}

#[test]
fn named_faces_are_read_from_lightroom_style_regions() {
    let x = AssetXmp::read(&XmpPacket::parse(LIGHTROOM_FACES).unwrap());
    // Bob only: Rex is a pet and the other face has no name.
    assert_eq!(
        x.face_regions,
        vec![FaceRegion {
            name: "Bob".into(),
            cx: 0.5,
            cy: 0.25,
            w: 0.1,
            h: 0.15,
        }]
    );
    assert_eq!(x.face_size, Some((6000, 4000)));
    // MWG areas are centred: the box starts half its size before.
    let (bx, by, bw, bh) = x.face_regions[0].to_box();
    assert!((bx - 0.45).abs() < 1e-6 && (by - 0.175).abs() < 1e-6);
    assert!((bw - 0.1).abs() < 1e-6 && (bh - 0.15).abs() < 1e-6);
}

#[test]
fn writing_faces_keeps_other_apps_regions() {
    let mut p = XmpPacket::parse(LIGHTROOM_FACES).unwrap();
    // Keepsake names Sara; Bob is not one of its people: he stays.
    let x = AssetXmp {
        face_regions: vec![FaceRegion::from_box("Sara", 0.1, 0.1, 0.2, 0.3)],
        face_size: Some((1024, 683)),
        face_names: vec!["Sara".into()],
        ..Default::default()
    };
    x.write(&mut p);
    let p = XmpPacket::parse(&p.to_xml()).unwrap();
    assert_eq!(
        region_names(&p),
        [
            ("Face".to_owned(), String::new()),
            ("Face".to_owned(), "Bob".to_owned()),
            ("Face".to_owned(), "Sara".to_owned()),
            ("Pet".to_owned(), "Rex".to_owned()),
        ]
    );
    // Keepsake took the names off (Bob is one of its people now, and it
    // wrote Sara): their regions go; the pet and the unnamed face stay.
    let mut p2 = p.clone();
    AssetXmp {
        face_names: vec!["bob".into()],
        ..Default::default()
    }
    .write(&mut p2);
    let p2 = XmpPacket::parse(&p2.to_xml()).unwrap();
    assert_eq!(
        region_names(&p2),
        [
            ("Face".to_owned(), String::new()),
            ("Pet".to_owned(), "Rex".to_owned()),
        ],
        "Sara's region was Keepsake's (it wrote her last time): it goes too"
    );
    // The picture's size of the other app stays when Keepsake has none.
    let d = p2.value(MWG, "Regions").unwrap();
    let dims = d.field(MWG, "AppliedToDimensions").unwrap();
    assert_eq!(
        dims.field("http://ns.adobe.com/xap/1.0/sType/Dimensions#", "w")
            .and_then(|v| v.as_text()),
        Some("1024")
    );
}

#[test]
fn nested_values_round_trip() {
    use keepsake_core::xmp::Value;
    let v = Value::fields([
        ("urn:a", "Name", Value::text("x & <y>")),
        (
            "urn:a",
            "List",
            Value::Array(
                ArrayKind::Seq,
                vec![
                    Value::fields([("urn:a", "N", Value::text("1"))]),
                    Value::text("two"),
                ],
            ),
        ),
    ]);
    let mut p = XmpPacket::new();
    p.set_value("urn:a", "Thing", Some(&v));
    let back = XmpPacket::parse(&p.to_xml()).unwrap();
    assert_eq!(back.value("urn:a", "Thing"), Some(v));
    let mut p = back;
    p.set_value("urn:a", "Thing", None);
    assert_eq!(p.value("urn:a", "Thing"), None);
}

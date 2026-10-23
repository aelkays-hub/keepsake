//! Build a catalog, delete it, rebuild it from files + sidecars, compare.

mod common;

use std::path::{Path, PathBuf};

use common::{copy_dir, new_file, volume_record, write_file, xmp_files};
use keepsake_core::catalog::{
    Asset, AssetId, CaptureTime, Catalog, CatalogSnapshot, Event, EventId, FileRole, MediaKind,
    Rating, SidecarLocation, TagPath, TagSource,
};
use keepsake_core::library::{Library, VolumeRecord};
use keepsake_core::metadata::MetadataService;
use keepsake_core::rebuild::rebuild_catalog;
use keepsake_core::relpath::{CaseSensitivity, RelPath};

struct World {
    _tmp: tempfile::TempDir,
    lib: Library,
    card: (VolumeRecord, PathBuf),
    backup: (VolumeRecord, PathBuf),
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::create(&tmp.path().join("library")).unwrap();
    let card_dir = tmp.path().join("card");
    let backup_dir = tmp.path().join("backup");
    std::fs::create_dir_all(&card_dir).unwrap();
    std::fs::create_dir_all(&backup_dir).unwrap();
    let card = volume_record("Card copy", CaseSensitivity::Insensitive, false);
    // The backup volume is treated as read-only: sidecars go to the library mirror.
    let backup = volume_record("Backup", CaseSensitivity::Insensitive, true);
    World {
        _tmp: tmp,
        lib,
        card: (card, card_dir),
        backup: (backup, backup_dir),
    }
}

fn snapshot(cat: &Catalog) -> CatalogSnapshot {
    cat.read(|tx| tx.snapshot()).unwrap()
}

fn asset(kind: MediaKind, ts: i64, offset: Option<i16>) -> Asset {
    let mut a = Asset::new(AssetId::new_random(), kind, 1_700_000_000_000);
    a.capture = Some(CaptureTime {
        utc_ms: ts,
        utc_offset_min: offset,
    });
    a.camera_make = Some("SONY".into());
    a.camera_model = Some("ILCE-7M4".into());
    a
}

fn tag(s: &str) -> TagPath {
    TagPath::parse_lightroom(s).unwrap()
}

/// Populate both volumes and the catalog through the public API.
fn build(w: &World, cat: &mut Catalog) -> Vec<AssetId> {
    let (card, card_dir) = (&w.card.0, w.card.1.as_path());
    let (backup, backup_dir) = (&w.backup.0, w.backup.1.as_path());
    let mut svc = MetadataService::new(&w.lib, cat);
    svc.register_volume(card, Some(card_dir)).unwrap();
    svc.register_volume(backup, Some(backup_dir)).unwrap();

    // XAVC clip with Sony XML and camera proxy.
    let clip = write_file(card_dir, "PRIVATE/M4ROOT/CLIP/C0001.MP4", b"clip-1");
    let xml = write_file(card_dir, "PRIVATE/M4ROOT/CLIP/C0001M01.XML", b"<xml/>");
    let proxy = write_file(card_dir, "PRIVATE/M4ROOT/SUB/C0001S01.MP4", b"proxy-1");
    let mut video = asset(MediaKind::Video, 1_718_425_800_000, Some(540));
    video.duration_ms = Some(12_345);
    svc.add_asset(
        &video,
        &[
            new_file(card_dir, card.id, &clip, FileRole::Original),
            new_file(card_dir, card.id, &xml, FileRole::Sidecar),
            new_file(card_dir, card.id, &proxy, FileRole::CameraProxy),
        ],
    )
    .unwrap();

    // RAW + JPEG pair.
    let jpg = write_file(card_dir, "DCIM/100MSDCF/DSC00001.JPG", b"jpeg-1");
    let arw = write_file(card_dir, "DCIM/100MSDCF/DSC00001.ARW", b"raw-1");
    let mut pair = asset(MediaKind::Photo, 1_718_430_000_000, Some(540));
    pair.lens_model = Some("FE 24-70mm F2.8 GM II".into());
    svc.add_asset(
        &pair,
        &[
            new_file(card_dir, card.id, &jpg, FileRole::Original),
            new_file(card_dir, card.id, &arw, FileRole::RawPair),
        ],
    )
    .unwrap();

    // A photo stored on both volumes (card + read-only backup).
    let both_card = write_file(card_dir, "DCIM/100MSDCF/DSC00002.JPG", b"jpeg-2");
    let both_backup = write_file(backup_dir, "2024/2024-06-15/DSC00002.JPG", b"jpeg-2");
    let both = asset(MediaKind::Photo, 1_718_431_000_000, None);
    svc.add_asset(
        &both,
        &[
            new_file(card_dir, card.id, &both_card, FileRole::Original),
            new_file(backup_dir, backup.id, &both_backup, FileRole::Original),
        ],
    )
    .unwrap();

    // A file whose name was written in NFD (as macOS HFS+ does).
    let nfd = write_file(card_dir, "Cafe\u{301}/Cre\u{300}me.JPG", b"jpeg-3");
    let unicode = asset(MediaKind::Photo, 1_718_432_000_000, Some(-300));
    svc.add_asset(
        &unicode,
        &[new_file(card_dir, card.id, &nfd, FileRole::Original)],
    )
    .unwrap();

    // A video only on the backup.
    let mov = write_file(backup_dir, "2024/2024-06-16/clip.mov", b"mov-1");
    let backup_only = asset(MediaKind::Video, 1_718_500_000_000, Some(60));
    svc.add_asset(
        &backup_only,
        &[new_file(backup_dir, backup.id, &mov, FileRole::Original)],
    )
    .unwrap();

    // User edits.
    svc.set_rating(video.id, Rating::new(5).unwrap()).unwrap();
    svc.set_label(video.id, Some("Red")).unwrap();
    svc.set_comment(video.id, Some("First dive & a turtle\nsecond line"))
        .unwrap();
    svc.set_user_tags(video.id, &[tag("Travel|Japan|Okinawa"), tag("Diving")])
        .unwrap();
    svc.set_ai_tags(
        video.id,
        &[(tag("Underwater"), 0.93), (tag("Animal|Turtle"), 0.71)],
    )
    .unwrap();
    svc.set_rating(pair.id, Rating::new(3).unwrap()).unwrap();
    svc.set_label(pair.id, Some("To Do")).unwrap(); // custom label
    svc.shift_capture_time(pair.id, -3600).unwrap();
    svc.set_rating(both.id, Rating::new(4).unwrap()).unwrap();
    svc.set_user_tags(both.id, &[tag("Family")]).unwrap();
    svc.set_comment(unicode.id, Some("Crème brûlée")).unwrap();
    let trip = Event {
        id: EventId::new_random(),
        title: "Okinawa trip".into(),
        place_name: Some("Naha".into()),
        lat: Some(26.2124),
        lon: Some(127.6809),
        created_ms: 1_700_000_000_500,
    };
    svc.set_event(&trip, &[video.id, pair.id, both.id]).unwrap();
    svc.add_recipe(
        video.id,
        "trim",
        1,
        &serde_json::json!({"in_ms": 1000, "out_ms": 9000, "mode": "keyframe"}),
    )
    .unwrap();
    let crop = svc
        .add_recipe(pair.id, "crop", 1, &serde_json::json!({"x": 0.1}))
        .unwrap();
    svc.update_recipe(crop.id, &serde_json::json!({"x": 0.2, "y": 0.1}))
        .unwrap();

    vec![video.id, pair.id, both.id, unicode.id, backup_only.id]
}

fn rebuild_all(w: &World, cat: &mut Catalog) {
    for (rec, dir) in [&w.card, &w.backup] {
        let report = rebuild_catalog(cat, &w.lib, rec.id, dir).unwrap();
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    }
}

#[test]
fn rebuild_reproduces_deleted_catalog() {
    let w = world();
    let mut cat = w.lib.open_catalog().unwrap();
    let ids = build(&w, &mut cat);
    let before = snapshot(&cat);
    assert_eq!(before.assets.len(), ids.len());
    // Make sure the comparison covers real content, not empty tables.
    let count = |f: fn(&keepsake_core::catalog::AssetSnapshot) -> usize| -> usize {
        before.assets.iter().map(f).sum()
    };
    assert_eq!(count(|a| a.files.len()), 9);
    assert_eq!(count(|a| a.tags.len()), 5);
    assert_eq!(count(|a| a.recipes.len()), 2);
    assert_eq!(count(|a| a.sidecars.len()), 6);
    assert_eq!(before.events.len(), 1);
    assert_eq!(before.volumes.len(), 2);
    assert_eq!(before.fts.len(), ids.len());
    assert!(cat.read(|tx| tx.fts_mismatches()).unwrap().is_empty());
    drop(cat);

    Catalog::delete_files(&w.lib.catalog_path()).unwrap();
    assert!(!w.lib.catalog_path().exists());

    let mut cat = w.lib.open_catalog().unwrap();
    assert_eq!(cat.read(|tx| tx.assets().count()).unwrap(), 0);
    rebuild_all(&w, &mut cat);
    let after = snapshot(&cat);
    assert_eq!(after, before);
    assert!(cat.read(|tx| tx.fts_mismatches()).unwrap().is_empty());

    // Spot-check the parts that matter most to users.
    let video = cat.read(|tx| tx.assets().require(ids[0])).unwrap();
    assert_eq!(video.rating.get(), 5);
    assert_eq!(video.label.as_deref(), Some("Red"));
    let tags = cat.read(|tx| tx.tags().for_asset(ids[0])).unwrap();
    assert_eq!(tags.iter().filter(|t| t.source == TagSource::Ai).count(), 2);
    let pair = cat.read(|tx| tx.assets().require(ids[1])).unwrap();
    assert_eq!(pair.capture_shift_s, -3600);
    assert_eq!(pair.label.as_deref(), Some("To Do"));
    assert_eq!(
        cat.read(|tx| tx.search().text("creme", 10)).unwrap(),
        vec![ids[3]]
    );
}

#[test]
fn rebuild_is_idempotent() {
    let w = world();
    let mut cat = w.lib.open_catalog().unwrap();
    build(&w, &mut cat);
    let before = snapshot(&cat);
    rebuild_all(&w, &mut cat);
    rebuild_all(&w, &mut cat);
    assert_eq!(snapshot(&cat), before);
}

#[test]
fn read_only_volume_is_never_written() {
    let w = world();
    let mut cat = w.lib.open_catalog().unwrap();
    build(&w, &mut cat);
    let backup_dir = &w.backup.1;
    assert!(
        xmp_files(backup_dir).is_empty(),
        "sidecar written on read-only volume"
    );
    let mirror = w.lib.sidecars_dir().join(w.backup.0.id.to_string());
    let mirrored = xmp_files(&mirror);
    assert_eq!(mirrored.len(), 2);
    assert!(mirror.join("2024/2024-06-15/DSC00002.JPG.xmp").is_file());
    // Writable card gets sidecars beside originals.
    assert!(w.card.1.join("DCIM/100MSDCF/DSC00001.JPG.xmp").is_file());
    let recs = cat
        .read(|tx| {
            let id = tx
                .files()
                .by_location(
                    w.backup.0.id,
                    &RelPath::parse("2024/2024-06-15/DSC00002.JPG").unwrap(),
                )?
                .unwrap()
                .asset_id;
            tx.sidecars().for_asset(id)
        })
        .unwrap();
    let locs: Vec<_> = recs.iter().map(|r| r.location).collect();
    assert!(locs.contains(&SidecarLocation::Beside));
    assert!(locs.contains(&SidecarLocation::Mirror));
}

#[test]
fn originals_are_never_modified() {
    let w = world();
    let mut cat = w.lib.open_catalog().unwrap();
    // Hash every non-sidecar file before and after edits + rebuild.
    fn hashes(dir: &Path) -> Vec<(PathBuf, blake3::Hash)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else if !p.to_string_lossy().ends_with(".xmp") {
                    out.push((p.clone(), blake3::hash(&std::fs::read(&p).unwrap())));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
    build(&w, &mut cat);
    let before = (hashes(&w.card.1), hashes(&w.backup.1));
    rebuild_all(&w, &mut cat);
    let id = cat.read(|tx| tx.assets().list_ids()).unwrap()[0];
    MetadataService::new(&w.lib, &mut cat)
        .set_rating(id, Rating::new(1).unwrap())
        .unwrap();
    assert_eq!((hashes(&w.card.1), hashes(&w.backup.1)), before);
}

#[test]
fn newest_sidecar_wins_between_beside_and_mirror() {
    let w = world();
    let mut cat = w.lib.open_catalog().unwrap();
    let ids = build(&w, &mut cat);
    let video = ids[0];
    // Edit while the card is offline: the change goes to the mirror only.
    cat.write(|tx| tx.volumes().set_mount(w.card.0.id, None, 0))
        .unwrap();
    MetadataService::new(&w.lib, &mut cat)
        .set_rating(video, Rating::new(1).unwrap())
        .unwrap();
    let mirror = w
        .lib
        .sidecars_dir()
        .join(w.card.0.id.to_string())
        .join("PRIVATE/M4ROOT/CLIP/C0001.MP4.xmp");
    assert!(mirror.is_file());
    drop(cat);
    Catalog::delete_files(&w.lib.catalog_path()).unwrap();
    let mut cat = w.lib.open_catalog().unwrap();
    rebuild_all(&w, &mut cat);
    let a = cat.read(|tx| tx.assets().require(video)).unwrap();
    assert_eq!(a.rating.get(), 1, "the newer mirror sidecar must win");
}

#[test]
fn rebuild_infers_assets_from_sony_layout_and_foreign_sidecars() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let vol = tmp.path().join("vol");
    copy_dir(&common::fixture("xavc"), &vol);
    copy_dir(&common::fixture("dcim"), &vol);
    copy_dir(&common::fixture("avchd"), &vol);
    // A Lightroom `<stem>.xmp` for the DCIM raw pair, and a digiKam sidecar.
    std::fs::copy(
        common::fixture("xmp/lightroom.xmp"),
        vol.join("DCIM/100MSDCF/DSC00001.xmp"),
    )
    .unwrap();
    std::fs::copy(
        common::fixture("xmp/digikam.xmp"),
        vol.join("DCIM/100MSDCF/DSC00002.JPG.xmp"),
    )
    .unwrap();
    // Hidden files are ignored.
    std::fs::write(vol.join(".DS_Store"), b"junk").unwrap();

    let rec = volume_record("Card", CaseSensitivity::Insensitive, false);
    lib.register_volume(&rec).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let report = rebuild_catalog(&mut cat, &lib, rec.id, &vol).unwrap();
    assert!(report.problems.is_empty(), "{:?}", report.problems);
    assert_eq!(report.hidden_skipped, 1);
    // INDEX.BDM and MOVIEOBJ.BDM belong to no asset.
    assert_eq!(report.skipped.len(), 2, "{:?}", report.skipped);

    // xavc: 2 clips, dcim: 2 stills, avchd: 2 segments.
    assert_eq!(report.assets, 6);
    let find = |rel: &str| {
        cat.read(|tx| {
            tx.files()
                .by_location(rec.id, &RelPath::parse(rel).unwrap())
        })
        .unwrap()
        .unwrap_or_else(|| panic!("{rel} not catalogued"))
    };
    let clip = find("PRIVATE/M4ROOT/CLIP/C0001.MP4");
    assert_eq!(clip.role, FileRole::Original);
    let files = cat.read(|tx| tx.files().for_asset(clip.asset_id)).unwrap();
    let mut roles: Vec<_> = files
        .iter()
        .map(|f| (f.rel_path.file_name().to_owned(), f.role))
        .collect();
    roles.sort();
    assert_eq!(
        roles,
        vec![
            ("C0001.MP4".into(), FileRole::Original),
            ("C0001M01.XML".into(), FileRole::Sidecar),
            ("C0001S01.MP4".into(), FileRole::CameraProxy),
        ]
    );
    let v = cat.read(|tx| tx.assets().require(clip.asset_id)).unwrap();
    assert_eq!(v.media_kind, MediaKind::Video);

    // DSC00001: JPG original + ARW/HIF companions, metadata from Lightroom's DSC00001.xmp.
    let jpg = find("DCIM/100MSDCF/DSC00001.JPG");
    assert_eq!(jpg.role, FileRole::Original);
    assert_eq!(find("DCIM/100MSDCF/DSC00001.ARW").role, FileRole::RawPair);
    let a = cat.read(|tx| tx.assets().require(jpg.asset_id)).unwrap();
    assert_eq!(a.rating.get(), 4);
    assert_eq!(a.label.as_deref(), Some("Red"));
    // digiKam sidecar for DSC00002.
    let d2 = find("DCIM/100MSDCF/DSC00002.JPG");
    let a2 = cat.read(|tx| tx.assets().require(d2.asset_id)).unwrap();
    assert_eq!(a2.label.as_deref(), Some("Green"));
    let t2 = cat.read(|tx| tx.tags().for_asset(d2.asset_id)).unwrap();
    assert_eq!(t2.len(), 2);

    // Deterministic ids: rebuilding again yields the same catalog.
    let before = snapshot(&cat);
    rebuild_catalog(&mut cat, &lib, rec.id, &vol).unwrap();
    assert_eq!(snapshot(&cat), before);

    // Rebuild did not write anything to the volume.
    let xmps = xmp_files(&vol);
    assert_eq!(xmps.len(), 2);
}

#[test]
fn missing_recipe_is_reported() {
    let w = world();
    let mut cat = w.lib.open_catalog().unwrap();
    build(&w, &mut cat);
    // Delete every recipe JSON.
    std::fs::remove_dir_all(w.lib.recipes_dir()).unwrap();
    drop(cat);
    Catalog::delete_files(&w.lib.catalog_path()).unwrap();
    let mut cat = w.lib.open_catalog().unwrap();
    let report = rebuild_catalog(&mut cat, &w.lib, w.card.0.id, &w.card.1).unwrap();
    assert_eq!(
        report
            .problems
            .iter()
            .filter(|p| p.contains("is missing"))
            .count(),
        2,
        "{:?}",
        report.problems
    );
    let n: usize = cat
        .read(|tx| {
            let mut n = 0;
            for id in tx.assets().list_ids()? {
                n += tx.recipes().for_asset(id)?.len();
            }
            Ok::<_, keepsake_core::catalog::CatalogError>(n)
        })
        .unwrap();
    assert_eq!(n, 0);
}

#[test]
fn unknown_volume_is_an_error() {
    let w = world();
    let mut cat = w.lib.open_catalog().unwrap();
    let r = rebuild_catalog(
        &mut cat,
        &w.lib,
        keepsake_core::catalog::VolumeId::new_random(),
        &w.card.1,
    );
    assert!(r.is_err());
}

mod random_layouts {
    use super::*;
    use proptest::prelude::*;

    fn layout() -> impl Strategy<Value = Vec<String>> {
        let dirs = prop_oneof![
            Just("DCIM/100MSDCF"),
            Just("PRIVATE/M4ROOT/CLIP"),
            Just("PRIVATE/M4ROOT/SUB"),
            Just("MP_ROOT/100ANV01"),
            Just("Exports/Ünïcödé"),
        ];
        let stems = prop_oneof![
            Just("DSC00001"),
            Just("DSC00002"),
            Just("C0001"),
            Just("C0001M01"),
            Just("C0001S01"),
            Just("Cafe\u{301}"),
        ];
        let exts = prop_oneof![
            Just("JPG"),
            Just("ARW"),
            Just("HIF"),
            Just("MP4"),
            Just("XML"),
            Just("txt"),
        ];
        prop::collection::btree_set((dirs, stems, exts), 1..12).prop_map(|set| {
            set.into_iter()
                .map(|(d, s, e)| format!("{d}/{s}.{e}"))
                .collect()
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(24))]

        /// Every file is either indexed or reported, and a second rebuild
        /// produces exactly the same catalog.
        #[test]
        fn rebuild_is_stable(files in layout()) {
            let tmp = tempfile::tempdir().unwrap();
            let lib = Library::create(&tmp.path().join("lib")).unwrap();
            let vol = tmp.path().join("vol");
            for (i, f) in files.iter().enumerate() {
                write_file(&vol, f, format!("content-{i}").as_bytes());
            }
            let rec = volume_record("v", CaseSensitivity::Insensitive, false);
            lib.register_volume(&rec).unwrap();
            let mut cat = lib.open_catalog().unwrap();
            let r1 = rebuild_catalog(&mut cat, &lib, rec.id, &vol).unwrap();
            prop_assert!(r1.problems.is_empty(), "{:?}", r1.problems);
            prop_assert_eq!(r1.files_indexed + r1.skipped.len() as u64, files.len() as u64);
            let s1 = snapshot(&cat);
            let r2 = rebuild_catalog(&mut cat, &lib, rec.id, &vol).unwrap();
            prop_assert_eq!(r2.assets, r1.assets);
            prop_assert_eq!(snapshot(&cat), s1);
            prop_assert!(cat.read(|tx| tx.fts_mismatches()).unwrap().is_empty());
        }
    }
}

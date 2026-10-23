//! Build a catalog, delete it, rebuild it from files + sidecars, compare.

mod common;

use std::path::{Path, PathBuf};

use common::{copy_dir, new_file, volume_record, write_file, xmp_files};
use keepsake_core::catalog::{
    Asset, AssetId, Blake3Hash, CaptureTime, Catalog, CatalogSnapshot, Event, EventId, FileRole,
    MediaKind, Rating, SidecarLocation, TagPath, TagSource, VolumeId,
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

/// Copy `tests/fixtures/<src>` to `<root>/<dst>` and return `dst` as a RelPath.
fn copy_fixture(src: &str, root: &Path, dst: &str) -> RelPath {
    let bytes = std::fs::read(common::fixture(src)).unwrap();
    write_file(root, dst, &bytes)
}

/// What the test set through the API, written down independently of the
/// catalog so a value the API silently dropped cannot "match itself".
struct Expect {
    id: AssetId,
    media_kind: MediaKind,
    /// Final capture time: the original plus any shift, with its UTC offset.
    capture: CaptureTime,
    shift_s: i64,
    duration_ms: Option<i64>,
    rating: u8,
    label: Option<&'static str>,
    comment: Option<&'static str>,
    user_tags: Vec<&'static str>,
    ai_tags: Vec<(&'static str, f64)>,
    /// `(volume, path, role)` for every file of the asset.
    files: Vec<(VolumeId, &'static str, FileRole)>,
}

const T_DIVE: i64 = 1_718_425_800_000; // 2024-06-15T04:30:00Z
const T_KIDS: i64 = 1_718_470_000_000;
const T_PAIR: i64 = 1_718_430_000_000;
const T_BOTH: i64 = 1_718_431_000_000;
const T_FETE: i64 = 1_718_440_000_123;
const T_NFD: i64 = 1_718_432_000_000;
const T_MAH: i64 = 1_718_500_000_000;

/// The NFD spelling of `Café/Crème.JPG`, as macOS HFS+ writes it.
const NFD_PATH: &str = "Cafe\u{301}/Cre\u{300}me.JPG";

/// Copy the fixture card layouts onto two volumes, catalogue every file
/// through the public API, and apply user edits. Returns what was set.
fn build(w: &World, cat: &mut Catalog) -> Vec<Expect> {
    let (card, card_dir) = (&w.card.0, w.card.1.as_path());
    let (backup, backup_dir) = (&w.backup.0, w.backup.1.as_path());

    // Card: the XAVC and DCIM fixture layouts plus a path with spaces and accents.
    copy_dir(&common::fixture("xavc"), card_dir);
    copy_dir(&common::fixture("dcim"), card_dir);
    copy_dir(&common::fixture("names"), card_dir);
    // NFD name, written at run time (git would store the NFC form).
    let nfd = copy_fixture("dcim/DCIM/100MSDCF/DSC00002.JPG", card_dir, NFD_PATH);
    // Backup (read-only): fixture copies under folder names with spaces and accents.
    let both_backup = copy_fixture(
        "dcim/DCIM/100MSDCF/DSC00002.JPG",
        backup_dir,
        "Photos 2024/Été/DSC00002.JPG",
    );
    let mah = copy_fixture(
        "mp_root/MP_ROOT/100ANV01/MAH00001.MP4",
        backup_dir,
        "Clips/Été 2024/MAH00001.MP4",
    );

    let mut svc = MetadataService::new(&w.lib, cat);
    svc.register_volume(card, Some(card_dir)).unwrap();
    svc.register_volume(backup, Some(backup_dir)).unwrap();

    let rel = |s: &str| RelPath::parse(s).unwrap();
    let on_card = |s: &str, role| new_file(card_dir, card.id, &rel(s), role);

    // XAVC clip 1: MP4 + Sony XML + camera proxy.
    let mut dive = asset(MediaKind::Video, T_DIVE, Some(540));
    dive.duration_ms = Some(12_345);
    svc.add_asset(
        &dive,
        &[
            on_card("PRIVATE/M4ROOT/CLIP/C0001.MP4", FileRole::Original),
            on_card("PRIVATE/M4ROOT/CLIP/C0001M01.XML", FileRole::Sidecar),
            on_card("PRIVATE/M4ROOT/SUB/C0001S01.MP4", FileRole::CameraProxy),
        ],
    )
    .unwrap();

    // XAVC clip 2, recorded in a negative UTC offset.
    let mut kids = asset(MediaKind::Video, T_KIDS, Some(-420));
    kids.duration_ms = Some(61_000);
    svc.add_asset(
        &kids,
        &[
            on_card("PRIVATE/M4ROOT/CLIP/C0002.MP4", FileRole::Original),
            on_card("PRIVATE/M4ROOT/CLIP/C0002M01.XML", FileRole::Sidecar),
            on_card("PRIVATE/M4ROOT/SUB/C0002S01.MP4", FileRole::CameraProxy),
        ],
    )
    .unwrap();

    // JPEG + ARW + HIF of one shot.
    let mut pair = asset(MediaKind::Photo, T_PAIR, Some(540));
    pair.lens_model = Some("FE 24-70mm F2.8 GM II".into());
    svc.add_asset(
        &pair,
        &[
            on_card("DCIM/100MSDCF/DSC00001.JPG", FileRole::Original),
            on_card("DCIM/100MSDCF/DSC00001.ARW", FileRole::RawPair),
            on_card("DCIM/100MSDCF/DSC00001.HIF", FileRole::RawPair),
        ],
    )
    .unwrap();

    // One photo stored on both volumes; no known UTC offset.
    let both = asset(MediaKind::Photo, T_BOTH, None);
    svc.add_asset(
        &both,
        &[
            on_card("DCIM/100MSDCF/DSC00002.JPG", FileRole::Original),
            new_file(backup_dir, backup.id, &both_backup, FileRole::Original),
        ],
    )
    .unwrap();

    // Path with spaces and non-ASCII characters (committed fixture).
    let fete = asset(MediaKind::Photo, T_FETE, Some(120));
    svc.add_asset(
        &fete,
        &[on_card(
            "Exports/Été 2024/Fête du lac 01.JPG",
            FileRole::Original,
        )],
    )
    .unwrap();

    // NFD file name.
    let unicode = asset(MediaKind::Photo, T_NFD, Some(-300));
    svc.add_asset(
        &unicode,
        &[new_file(card_dir, card.id, &nfd, FileRole::Original)],
    )
    .unwrap();

    // A video only on the read-only backup.
    let backup_only = asset(MediaKind::Video, T_MAH, Some(60));
    svc.add_asset(
        &backup_only,
        &[new_file(backup_dir, backup.id, &mah, FileRole::Original)],
    )
    .unwrap();

    // User edits through the API.
    svc.set_rating(dive.id, Rating::new(5).unwrap()).unwrap();
    svc.set_label(dive.id, Some("Red")).unwrap();
    svc.set_comment(dive.id, Some("First dive & a turtle\nsecond line"))
        .unwrap();
    svc.set_user_tags(dive.id, &[tag("Travel|Japan|Okinawa"), tag("Diving")])
        .unwrap();
    svc.set_ai_tags(
        dive.id,
        &[(tag("Underwater"), 0.93), (tag("Animal|Turtle"), 0.71)],
    )
    .unwrap();

    svc.set_rating(kids.id, Rating::new(2).unwrap()).unwrap();
    svc.set_label(kids.id, Some("Blue")).unwrap();
    svc.set_user_tags(kids.id, &[tag("Family|Kids")]).unwrap();

    svc.set_rating(pair.id, Rating::new(3).unwrap()).unwrap();
    svc.set_label(pair.id, Some("To Do")).unwrap(); // custom label
    svc.set_user_tags(pair.id, &[tag("sunset")]).unwrap(); // flat keyword
    svc.shift_capture_time(pair.id, -3600).unwrap();

    svc.set_rating(both.id, Rating::new(4).unwrap()).unwrap();
    svc.set_user_tags(both.id, &[tag("Family")]).unwrap();

    svc.set_rating(fete.id, Rating::new(4).unwrap()).unwrap();
    svc.set_label(fete.id, Some("Green")).unwrap();
    svc.set_comment(fete.id, Some("Fête du lac — été")).unwrap();
    svc.set_user_tags(fete.id, &[tag("Lieux|Genève"), tag("Fête")])
        .unwrap();
    svc.shift_capture_time(fete.id, 30).unwrap();
    svc.shift_capture_time(fete.id, 60).unwrap(); // shifts add up

    svc.set_comment(unicode.id, Some("Crème brûlée")).unwrap();

    svc.set_rating(backup_only.id, Rating::new(1).unwrap())
        .unwrap();
    svc.set_label(backup_only.id, Some("Purple")).unwrap();

    let trip = Event {
        id: EventId::new_random(),
        title: "Okinawa trip".into(),
        place_name: Some("Naha".into()),
        lat: Some(26.2124),
        lon: Some(127.6809),
        created_ms: 1_700_000_000_500,
    };
    svc.set_event(&trip, &[dive.id, pair.id, both.id]).unwrap();
    svc.add_recipe(
        dive.id,
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

    let ct = |utc_ms, off| CaptureTime {
        utc_ms,
        utc_offset_min: off,
    };
    let (c, b) = (card.id, backup.id);
    vec![
        Expect {
            id: dive.id,
            media_kind: MediaKind::Video,
            capture: ct(T_DIVE, Some(540)),
            shift_s: 0,
            duration_ms: Some(12_345),
            rating: 5,
            label: Some("Red"),
            comment: Some("First dive & a turtle\nsecond line"),
            user_tags: vec!["Diving", "Travel|Japan|Okinawa"],
            ai_tags: vec![("Animal|Turtle", 0.71), ("Underwater", 0.93)],
            files: vec![
                (c, "PRIVATE/M4ROOT/CLIP/C0001.MP4", FileRole::Original),
                (c, "PRIVATE/M4ROOT/CLIP/C0001M01.XML", FileRole::Sidecar),
                (c, "PRIVATE/M4ROOT/SUB/C0001S01.MP4", FileRole::CameraProxy),
            ],
        },
        Expect {
            id: kids.id,
            media_kind: MediaKind::Video,
            capture: ct(T_KIDS, Some(-420)),
            shift_s: 0,
            duration_ms: Some(61_000),
            rating: 2,
            label: Some("Blue"),
            comment: None,
            user_tags: vec!["Family|Kids"],
            ai_tags: vec![],
            files: vec![
                (c, "PRIVATE/M4ROOT/CLIP/C0002.MP4", FileRole::Original),
                (c, "PRIVATE/M4ROOT/CLIP/C0002M01.XML", FileRole::Sidecar),
                (c, "PRIVATE/M4ROOT/SUB/C0002S01.MP4", FileRole::CameraProxy),
            ],
        },
        Expect {
            id: pair.id,
            media_kind: MediaKind::Photo,
            capture: ct(T_PAIR - 3_600_000, Some(540)),
            shift_s: -3600,
            duration_ms: None,
            rating: 3,
            label: Some("To Do"),
            comment: None,
            user_tags: vec!["sunset"],
            ai_tags: vec![],
            files: vec![
                (c, "DCIM/100MSDCF/DSC00001.ARW", FileRole::RawPair),
                (c, "DCIM/100MSDCF/DSC00001.HIF", FileRole::RawPair),
                (c, "DCIM/100MSDCF/DSC00001.JPG", FileRole::Original),
            ],
        },
        Expect {
            id: both.id,
            media_kind: MediaKind::Photo,
            capture: ct(T_BOTH, None),
            shift_s: 0,
            duration_ms: None,
            rating: 4,
            label: None,
            comment: None,
            user_tags: vec!["Family"],
            ai_tags: vec![],
            files: vec![
                (c, "DCIM/100MSDCF/DSC00002.JPG", FileRole::Original),
                (b, "Photos 2024/Été/DSC00002.JPG", FileRole::Original),
            ],
        },
        Expect {
            id: fete.id,
            media_kind: MediaKind::Photo,
            capture: ct(T_FETE + 90_000, Some(120)),
            shift_s: 90,
            duration_ms: None,
            rating: 4,
            label: Some("Green"),
            comment: Some("Fête du lac — été"),
            user_tags: vec!["Fête", "Lieux|Genève"],
            ai_tags: vec![],
            files: vec![(c, "Exports/Été 2024/Fête du lac 01.JPG", FileRole::Original)],
        },
        Expect {
            id: unicode.id,
            media_kind: MediaKind::Photo,
            capture: ct(T_NFD, Some(-300)),
            shift_s: 0,
            duration_ms: None,
            rating: 0,
            label: None,
            comment: Some("Crème brûlée"),
            user_tags: vec![],
            ai_tags: vec![],
            files: vec![(c, NFD_PATH, FileRole::Original)],
        },
        Expect {
            id: backup_only.id,
            media_kind: MediaKind::Video,
            capture: ct(T_MAH, Some(60)),
            shift_s: 0,
            duration_ms: None,
            rating: 1,
            label: Some("Purple"),
            comment: None,
            user_tags: vec![],
            ai_tags: vec![],
            files: vec![(b, "Clips/Été 2024/MAH00001.MP4", FileRole::Original)],
        },
    ]
}

fn rebuild_all(w: &World, cat: &mut Catalog) {
    for (rec, dir) in [&w.card, &w.backup] {
        let report = rebuild_catalog(cat, &w.lib, rec.id, dir).unwrap();
        assert!(report.problems.is_empty(), "{:?}", report.problems);
        assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    }
}

/// Check the catalog against the expectations, field by field, and every
/// file's hash and size against the bytes on disk.
fn check_against_expectations(w: &World, cat: &Catalog, expects: &[Expect], when: &str) {
    let mounts = [(w.card.0.id, &w.card.1), (w.backup.0.id, &w.backup.1)];
    assert_eq!(
        cat.read(|tx| tx.assets().count()).unwrap(),
        expects.len() as u64,
        "{when}: number of assets"
    );
    for e in expects {
        let ctx = format!("{when}: asset {:?}", e.files[0].1);
        let a = cat
            .read(|tx| tx.assets().get(e.id))
            .unwrap()
            .unwrap_or_else(|| panic!("{ctx}: missing (asset id not preserved?)"));

        // Capture time, including the UTC offset and the user's shift.
        assert_eq!(a.capture, Some(e.capture), "{ctx}: capture time/offset");
        assert_eq!(a.capture_shift_s, e.shift_s, "{ctx}: time shift");
        assert_eq!(a.media_kind, e.media_kind, "{ctx}: media kind");
        assert_eq!(a.duration_ms, e.duration_ms, "{ctx}: duration");
        assert_eq!(a.camera_model.as_deref(), Some("ILCE-7M4"), "{ctx}: camera");

        // User metadata.
        assert_eq!(a.rating.get(), e.rating, "{ctx}: rating");
        assert_eq!(a.label.as_deref(), e.label, "{ctx}: label");
        assert_eq!(a.comment.as_deref(), e.comment, "{ctx}: comment");
        let tags = cat.read(|tx| tx.tags().for_asset(e.id)).unwrap();
        let user: Vec<String> = tags
            .iter()
            .filter(|t| t.source == TagSource::User)
            .map(|t| t.path.to_string())
            .collect();
        assert_eq!(user, e.user_tags, "{ctx}: user keywords");
        let ai: Vec<(String, f64)> = tags
            .iter()
            .filter(|t| t.source == TagSource::Ai)
            .map(|t| (t.path.to_string(), t.confidence.unwrap()))
            .collect();
        let want_ai: Vec<(String, f64)> =
            e.ai_tags.iter().map(|(p, c)| (p.to_string(), *c)).collect();
        assert_eq!(ai, want_ai, "{ctx}: AI tags");

        // File-to-asset grouping, roles, and content hashes.
        let mut files: Vec<(VolumeId, String, FileRole)> = cat
            .read(|tx| tx.files().for_asset(e.id))
            .unwrap()
            .into_iter()
            .map(|f| (f.volume_id, f.rel_path.to_string(), f.role))
            .collect();
        files.sort();
        let mut want: Vec<(VolumeId, String, FileRole)> = e
            .files
            .iter()
            .map(|(v, p, r)| (*v, p.to_string(), *r))
            .collect();
        want.sort();
        assert_eq!(files, want, "{ctx}: files and roles");
        for (vol, path, _) in &e.files {
            let f = cat
                .read(|tx| tx.files().by_location(*vol, &RelPath::parse(path).unwrap()))
                .unwrap()
                .unwrap();
            assert_eq!(f.asset_id, e.id, "{ctx}: {path} grouped into wrong asset");
            let mount = mounts.iter().find(|(id, _)| id == vol).unwrap().1;
            let bytes = std::fs::read(f.rel_path.to_fs_path(mount)).unwrap();
            assert_eq!(f.blake3, Blake3Hash::of(&bytes), "{ctx}: {path} hash");
            assert_eq!(f.size_bytes, bytes.len() as u64, "{ctx}: {path} size");
        }
    }
}

#[test]
fn rebuild_reproduces_deleted_catalog() {
    let w = world();
    let mut cat = w.lib.open_catalog().unwrap();
    let expects = build(&w, &mut cat);

    // 1. The API stored what we set (so equality below is meaningful).
    check_against_expectations(&w, &cat, &expects, "before delete");
    let before = snapshot(&cat);
    let count = |f: fn(&keepsake_core::catalog::AssetSnapshot) -> usize| -> usize {
        before.assets.iter().map(f).sum()
    };
    assert_eq!(count(|a| a.files.len()), 14);
    assert_eq!(count(|a| a.recipes.len()), 2);
    assert_eq!(count(|a| a.sidecars.len()), 8);
    assert_eq!(before.events.len(), 1);
    assert_eq!(before.volumes.len(), 2);
    assert!(cat.read(|tx| tx.fts_mismatches()).unwrap().is_empty());
    // The space/accent path got its sidecar beside it on disk.
    assert!(w
        .card
        .1
        .join("Exports/Été 2024/Fête du lac 01.JPG.xmp")
        .is_file());
    drop(cat);

    // 2. Delete the database (and its WAL/SHM files).
    Catalog::delete_files(&w.lib.catalog_path()).unwrap();
    assert!(!w.lib.catalog_path().exists());

    // 3. Rebuild from disk plus XMP sidecars.
    let mut cat = w.lib.open_catalog().unwrap();
    assert_eq!(cat.read(|tx| tx.assets().count()).unwrap(), 0);
    rebuild_all(&w, &mut cat);

    // 4. Same values as set through the API ...
    check_against_expectations(&w, &cat, &expects, "after rebuild");
    // ... and every other stored field (ids, events, recipes, sidecar
    // records, search index) is identical to the catalog before deletion.
    assert_eq!(snapshot(&cat), before);
    assert!(cat.read(|tx| tx.fts_mismatches()).unwrap().is_empty());
    let search = |q: &str| cat.read(|tx| tx.search().text(q, 10)).unwrap();
    assert_eq!(search("creme"), vec![expects[5].id]);
    assert_eq!(search("geneve"), vec![expects[4].id]);
    assert_eq!(search("lac"), vec![expects[4].id]);
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
    assert!(mirror.join("Photos 2024/Été/DSC00002.JPG.xmp").is_file());
    // Writable card gets sidecars beside originals.
    assert!(w.card.1.join("DCIM/100MSDCF/DSC00001.JPG.xmp").is_file());
    let recs = cat
        .read(|tx| {
            let id = tx
                .files()
                .by_location(
                    w.backup.0.id,
                    &RelPath::parse("Photos 2024/Été/DSC00002.JPG").unwrap(),
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
    let ids: Vec<AssetId> = build(&w, &mut cat).iter().map(|e| e.id).collect();
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

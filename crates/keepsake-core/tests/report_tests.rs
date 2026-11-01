//! Tools → Report: counts and sizes by date, camera, lens, file type and
//! drive, straight from the catalog.

use std::path::PathBuf;
use std::time::Instant;

use keepsake_core::catalog::{
    Asset, AssetId, Blake3Hash, CaptureTime, Catalog, FileHash, FileRole, LibraryReport, MediaKind,
    NewFile, ReportRow, SearchQuery, Volume, VolumeId, VolumeKind,
};
use keepsake_core::relpath::{CaseSensitivity, RelPath};
use keepsake_core::report;

fn volume(label: &str, online: bool) -> Volume {
    Volume {
        id: VolumeId::new_random(),
        label: label.into(),
        fs_id: format!("fs-{}", uuid::Uuid::new_v4()),
        kind: VolumeKind::External,
        case: CaseSensitivity::Insensitive,
        read_only: false,
        online,
        mount_path: online.then(|| PathBuf::from("/mnt/x")),
        last_seen_ms: None,
    }
}

/// A file whose content (hash) is `content`: copies share it.
fn file(vol: &Volume, rel: &str, role: FileRole, size: u64, content: &str) -> NewFile {
    NewFile {
        volume_id: vol.id,
        rel_path: RelPath::parse(rel).unwrap(),
        role,
        size_bytes: size,
        hash: FileHash::Full(Blake3Hash::of(content.as_bytes())),
        xxh3: Some(1),
        mtime_ms: Some(1),
        segment_index: None,
    }
}

/// `day` is `YYYY-MM-DD` (UTC, offset 0).
fn asset(kind: MediaKind, day: Option<&str>, camera: Option<&str>, lens: Option<&str>) -> Asset {
    let mut a = Asset::new(AssetId::new_random(), kind, 1);
    a.capture = day.map(|d| {
        let dt = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc();
        CaptureTime {
            utc_ms: dt.timestamp_millis(),
            utc_offset_min: Some(0),
        }
    });
    a.camera_model = camera.map(str::to_owned);
    a.lens_model = lens.map(str::to_owned);
    a
}

fn add(cat: &mut Catalog, a: &Asset, files: &[NewFile]) {
    cat.write(|tx| {
        tx.assets().insert(a)?;
        for f in files {
            tx.files().insert(a.id, f)?;
        }
        Ok::<_, keepsake_core::catalog::CatalogError>(())
    })
    .unwrap();
}

fn setup(vols: &[&Volume]) -> Catalog {
    let mut cat = Catalog::open_in_memory().unwrap();
    for v in vols {
        cat.write(|tx| tx.volumes().upsert(v)).unwrap();
    }
    cat
}

fn report(cat: &Catalog) -> LibraryReport {
    cat.read(|tx| tx.report().library()).unwrap()
}

fn find<'a>(rows: &'a [ReportRow], group: Option<&str>) -> &'a ReportRow {
    rows.iter()
        .find(|r| r.group.as_deref() == group)
        .unwrap_or_else(|| panic!("no group {group:?} in {rows:?}"))
}

/// Photos, videos, files, bytes (date, camera and lens breakdowns).
fn counts(r: &ReportRow) -> (u64, u64, u64, u64) {
    let (p, v) = (r.photos.unwrap(), r.videos.unwrap());
    assert_eq!(r.items, p + v);
    (p, v, r.files, r.bytes)
}

/// Items, files, bytes (file types and drives: no photo/video split).
fn items(r: &ReportRow) -> (u64, u64, u64) {
    assert_eq!((r.photos, r.videos), (None, None));
    (r.items, r.files, r.bytes)
}

#[test]
fn empty_library_reports_zeros() {
    let cat = setup(&[]);
    let r = report(&cat);
    assert_eq!(counts(&r.totals), (0, 0, 0, 0));
    assert!(r.years.is_empty() && r.cameras.is_empty() && r.file_types.is_empty());
    assert!(r.drives.is_empty());
}

#[test]
fn raw_jpeg_pair_is_one_photo_but_both_files_count() {
    let v = volume("Photos", true);
    let mut cat = setup(&[&v]);
    let pair = asset(
        MediaKind::Photo,
        Some("2024-06-01"),
        Some("ILCE-7M4"),
        Some("FE 24-70mm F2.8 GM II"),
    );
    add(
        &mut cat,
        &pair,
        &[
            file(&v, "2024/DSC00001.JPG", FileRole::Original, 100, "jpg1"),
            file(&v, "2024/DSC00001.ARW", FileRole::RawPair, 1_000, "arw1"),
        ],
    );
    let r = report(&cat);
    assert_eq!(counts(&r.totals), (1, 0, 2, 1_100));
    assert_eq!(r.totals.items, 1);
    assert_eq!(counts(find(&r.cameras, Some("ILCE-7M4"))), (1, 0, 2, 1_100));
    assert_eq!(
        counts(find(&r.lenses, Some("FE 24-70mm F2.8 GM II"))),
        (1, 0, 2, 1_100)
    );
    // By type the pair shows under both extensions, each with its own bytes.
    assert_eq!(items(find(&r.file_types, Some("ARW"))), (1, 1, 1_000));
    assert_eq!(items(find(&r.file_types, Some("JPG"))), (1, 1, 100));
    // Bigger groups first.
    assert_eq!(r.file_types.len(), 2);
}

#[test]
fn photos_and_videos_are_counted_apart_by_day_month_and_year() {
    let v = volume("Photos", true);
    let mut cat = setup(&[&v]);
    let cam = Some("ILCE-7M4");
    for (i, (kind, day)) in [
        (MediaKind::Photo, "2023-12-31"),
        (MediaKind::Photo, "2024-06-01"),
        (MediaKind::Video, "2024-06-01"),
        (MediaKind::Photo, "2024-06-15"),
        (MediaKind::Video, "2024-07-02"),
    ]
    .into_iter()
    .enumerate()
    {
        let a = asset(kind, Some(day), cam, None);
        let name = format!(
            "f/{i}.{}",
            if kind == MediaKind::Photo {
                "jpg"
            } else {
                "mp4"
            }
        );
        add(
            &mut cat,
            &a,
            &[file(&v, &name, FileRole::Original, 10, &name)],
        );
    }
    let r = report(&cat);
    assert_eq!(counts(&r.totals), (3, 2, 5, 50));
    let years: Vec<_> = r.years.iter().map(|y| y.label().to_owned()).collect();
    assert_eq!(years, ["2024", "2023"], "newest first");
    assert_eq!(counts(find(&r.years, Some("2024"))), (2, 2, 4, 40));
    assert_eq!(counts(find(&r.months, Some("2024-06"))), (2, 1, 3, 30));
    assert_eq!(counts(find(&r.months, Some("2024-07"))), (0, 1, 1, 10));
    assert_eq!(counts(find(&r.days, Some("2024-06-01"))), (1, 1, 2, 20));
    assert_eq!(r.days.first().unwrap().label(), "2024-07-02");
    assert_eq!(r.days.len(), 4);
    // Lower-case extensions are grouped upper-case.
    assert_eq!(items(find(&r.file_types, Some("MP4"))), (2, 2, 20));
}

#[test]
fn missing_date_camera_lens_and_extension_are_unknown_and_last() {
    let v = volume("Photos", true);
    let mut cat = setup(&[&v]);
    let known = asset(
        MediaKind::Photo,
        Some("2024-01-01"),
        Some("ILCE-7M4"),
        Some("FE 35mm"),
    );
    let unknown = asset(MediaKind::Photo, None, None, Some("  "));
    let other = asset(MediaKind::Photo, None, None, None);
    add(
        &mut cat,
        &known,
        &[file(&v, "a.jpg", FileRole::Original, 5, "a")],
    );
    add(
        &mut cat,
        &unknown,
        &[file(&v, "README", FileRole::Original, 7, "b")],
    );
    add(
        &mut cat,
        &other,
        &[file(&v, "c.jpg", FileRole::Original, 9, "c")],
    );
    let r = report(&cat);
    for rows in [&r.years, &r.months, &r.days, &r.cameras, &r.lenses] {
        let last = rows.last().unwrap();
        assert_eq!(last.group, None, "unknown last in {rows:?}");
        assert_eq!(last.label(), "Unknown");
        assert_eq!(counts(last), (2, 0, 2, 16));
    }
    assert_eq!(r.cameras.len(), 2);
    // A blank lens is unknown too.
    assert_eq!(r.lenses.len(), 2);
    assert_eq!(items(find(&r.file_types, None)), (1, 1, 7));
    assert_eq!(r.file_types.last().unwrap().label(), "Unknown");
}

#[test]
fn unplugged_drives_count_and_backup_copies_count_once() {
    let main = volume("Network photos", true);
    let archive = volume("Archive (unplugged)", false);
    let backup = volume("Backup", false);
    let mut cat = setup(&[&main, &archive, &backup]);
    // Only on the unplugged archive.
    let old = asset(
        MediaKind::Video,
        Some("2019-05-05"),
        Some("FDR-AX700"),
        None,
    );
    add(
        &mut cat,
        &old,
        &[file(&archive, "v.MP4", FileRole::Original, 5_000, "old")],
    );
    // On the main drive and backed up (same content, same hash).
    let pic = asset(MediaKind::Photo, Some("2024-01-01"), Some("ILCE-7M4"), None);
    add(
        &mut cat,
        &pic,
        &[
            file(&main, "p.JPG", FileRole::Original, 300, "pic"),
            file(&backup, "2024/p.JPG", FileRole::Original, 300, "pic"),
        ],
    );
    // Not fingerprinted yet: stand-in keys differ, so both copies count.
    let pending = asset(MediaKind::Photo, Some("2024-01-02"), Some("ILCE-7M4"), None);
    let mut f1 = file(&main, "q.JPG", FileRole::Original, 40, "");
    f1.hash = FileHash::pending_for(main.id, "q.jpg", 40, Some(1));
    add(&mut cat, &pending, &[f1]);

    let r = report(&cat);
    assert_eq!(counts(&r.totals), (2, 1, 3, 5_340));
    assert_eq!(counts(find(&r.years, Some("2019"))), (0, 1, 1, 5_000));
    assert_eq!(counts(find(&r.cameras, Some("ILCE-7M4"))), (2, 0, 2, 340));

    // Drives count every copy, plugged in or not, by name.
    let names: Vec<_> = r.drives.iter().map(|d| d.row.label().to_owned()).collect();
    assert_eq!(names, ["Archive (unplugged)", "Backup", "Network photos"]);
    let d = |name: &str| r.drives.iter().find(|d| d.row.label() == name).unwrap();
    assert!(!d("Archive (unplugged)").online);
    assert_eq!(items(&d("Archive (unplugged)").row), (1, 1, 5_000));
    assert_eq!(items(&d("Backup").row), (1, 1, 300));
    assert_eq!(items(&d("Network photos").row), (2, 2, 340));
}

#[test]
fn csv_has_every_breakdown_with_unknown_groups() {
    let v = volume("Photos, 2024", false);
    let mut cat = setup(&[&v]);
    let a = asset(
        MediaKind::Photo,
        Some("2024-06-01"),
        None,
        Some("FE 24-70mm, F2.8"),
    );
    add(
        &mut cat,
        &a,
        &[file(&v, "x.jpg", FileRole::Original, 2_500_000, "x")],
    );
    let csv = report::to_csv(&report(&cat));
    let lines: Vec<&str> = csv.split("\r\n").filter(|l| !l.is_empty()).collect();
    assert_eq!(lines[0], report::CSV_HEADER);
    assert_eq!(lines[1], "Total,All,1,0,1,1,2500000,2.5 MB");
    for want in [
        "Year,2024,1,0,1,1,2500000,2.5 MB",
        "Month,2024-06,",
        "Day,2024-06-01,",
        "Camera,Unknown,1,0,1,1,",
        "Lens,\"FE 24-70mm, F2.8\",1,",
        "File type,JPG,,,1,1,2500000,2.5 MB",
        "Drive,\"Photos, 2024 (unplugged)\",,,1,1,2500000,2.5 MB",
    ] {
        assert!(
            lines.iter().any(|l| l.starts_with(want)),
            "{want} in\n{csv}"
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("report.csv");
    report::write_csv(&report(&cat), &out).unwrap();
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF]);
}

#[test]
fn report_on_a_seeded_library_matches_the_catalog_and_is_quick() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = keepsake_core::library::Library::create(&tmp.path().join("lib")).unwrap();
    let cache = keepsake_core::render::Cache::new(tmp.path().join("cache"));
    keepsake_core::seed::seed(
        &lib,
        &cache,
        &keepsake_core::seed::SeedOptions {
            assets: 5_000,
            distinct: 8,
            offline_share: 0.25,
        },
    )
    .unwrap();
    let cat = lib.open_catalog().unwrap();
    let t = Instant::now();
    let r = report(&cat);
    let took = t.elapsed();
    let assets = cat.read(|tx| tx.assets().count()).unwrap();
    assert_eq!(r.totals.items, assets);
    // Every breakdown adds up to the totals.
    for rows in [&r.years, &r.months, &r.days, &r.cameras, &r.lenses] {
        let sum = |f: fn(&ReportRow) -> u64| rows.iter().map(f).sum::<u64>();
        assert_eq!(sum(|x| x.items), r.totals.items);
        assert_eq!(sum(|x| x.files), r.totals.files);
        assert_eq!(sum(|x| x.bytes), r.totals.bytes);
    }
    assert_eq!(
        r.file_types.iter().map(|x| x.bytes).sum::<u64>(),
        r.totals.bytes
    );
    assert!(
        r.drives.iter().any(|d| !d.online),
        "seed has an unplugged drive"
    );
    assert!(took.as_secs() < 5, "report took {took:?}");
}

#[test]
fn item_totals_follow_fingerprints_and_deletes() {
    let main = volume("Photos", true);
    let backup = volume("Backup", true);
    let mut cat = setup(&[&main, &backup]);
    let a = asset(MediaKind::Photo, Some("2024-01-01"), None, None);
    // Added in place, not read in full yet.
    let mut pending = file(&main, "a.JPG", FileRole::Original, 70, "");
    pending.hash = FileHash::pending_for(main.id, "a.jpg", 70, Some(1));
    add(&mut cat, &a, &[pending]);
    let copy = file(&backup, "2024/a.JPG", FileRole::Original, 70, "real");
    let copy_id = cat.write(|tx| tx.files().insert(a.id, &copy)).unwrap().id;
    // Until the original's fingerprint is known, the two keys differ.
    assert_eq!(counts(&report(&cat).totals), (1, 0, 2, 140));

    let original = cat
        .read(|tx| tx.files().for_volume(main.id))
        .unwrap()
        .remove(0);
    let real = Blake3Hash::of(b"real");
    assert!(cat
        .write(|tx| tx.files().set_fingerprint(original.id, &real))
        .unwrap());
    assert_eq!(counts(&report(&cat).totals), (1, 0, 1, 70));

    cat.write(|tx| tx.files().delete(copy_id)).unwrap();
    assert_eq!(counts(&report(&cat).totals), (1, 0, 1, 70));
    cat.write(|tx| tx.files().delete(original.id)).unwrap();
    assert_eq!(counts(&report(&cat).totals), (1, 0, 0, 0));
}

/// How many items a search finds.
fn found(cat: &Catalog, q: &SearchQuery) -> u64 {
    cat.read(|tx| tx.browse().count(q)).unwrap()
}

/// The search a report row opens (as the UI's `lineQuery` builds it).
fn date_query(group: Option<&str>) -> SearchQuery {
    let Some(g) = group else {
        return SearchQuery {
            undated: true,
            ..Default::default()
        };
    };
    let (from, to) = match g.len() {
        4 => (format!("{g}-01-01"), format!("{g}-12-31")),
        7 => {
            let first = chrono::NaiveDate::parse_from_str(&format!("{g}-01"), "%Y-%m-%d").unwrap();
            let last = first
                .checked_add_months(chrono::Months::new(1))
                .unwrap()
                .pred_opt()
                .unwrap();
            (first.to_string(), last.to_string())
        }
        _ => (g.to_owned(), g.to_owned()),
    };
    SearchQuery {
        from: Some(from),
        to: Some(to),
        ..Default::default()
    }
}

#[test]
fn every_report_row_opens_exactly_its_items() {
    let main = volume("Photos", true);
    let archive = volume("Archive", false);
    let mut cat = setup(&[&main, &archive]);
    // `ILCE-7` is part of `ILCE-7M4`: a row must not show the other's items.
    let a7 = asset(
        MediaKind::Photo,
        Some("2024-02-29"),
        Some("ILCE-7"),
        Some("FE 35mm"),
    );
    add(
        &mut cat,
        &a7,
        &[file(&main, "a.JPG", FileRole::Original, 5, "a")],
    );
    let m4 = asset(
        MediaKind::Photo,
        Some("2024-03-01"),
        Some(" ILCE-7M4 "),
        Some("FE 35mm F1.8"),
    );
    add(
        &mut cat,
        &m4,
        &[
            file(&main, "b.ARW", FileRole::RawPair, 50, "b"),
            file(&main, "b.jpg", FileRole::Original, 5, "b2"),
        ],
    );
    let clip = asset(MediaKind::Video, Some("2023-12-31"), Some("ILCE-7M4"), None);
    add(
        &mut cat,
        &clip,
        &[file(&archive, "c.MP4", FileRole::Original, 500, "c")],
    );
    let odd = asset(MediaKind::Photo, None, None, Some(" "));
    add(
        &mut cat,
        &odd,
        &[file(&main, "README", FileRole::Original, 1, "d")],
    );
    let backed = asset(MediaKind::Photo, Some("2024-02-01"), Some("ILCE-7"), None);
    add(
        &mut cat,
        &backed,
        &[
            file(&main, "e.jpg", FileRole::Original, 5, "e"),
            file(&archive, "2024/e.jpg", FileRole::Original, 5, "e"),
        ],
    );

    let r = report(&cat);
    for rows in [&r.years, &r.months, &r.days] {
        for row in rows {
            let q = date_query(row.group.as_deref());
            assert_eq!(found(&cat, &q), row.items, "{q:?}");
        }
    }
    let exact = |g: &Option<String>| Some(g.clone().unwrap_or_default());
    for row in &r.cameras {
        let q = SearchQuery {
            camera_is: exact(&row.group),
            ..Default::default()
        };
        assert_eq!(found(&cat, &q), row.items, "{q:?}");
    }
    for row in &r.lenses {
        let q = SearchQuery {
            lens_is: exact(&row.group),
            ..Default::default()
        };
        assert_eq!(found(&cat, &q), row.items, "{q:?}");
    }
    for row in &r.file_types {
        let q = SearchQuery {
            file_type: exact(&row.group),
            ..Default::default()
        };
        assert_eq!(found(&cat, &q), row.items, "{q:?}");
    }
    for d in &r.drives {
        let q = SearchQuery {
            volume: Some(d.volume_id),
            ..Default::default()
        };
        assert_eq!(found(&cat, &q), d.row.items, "{q:?}");
    }
    // The rows checked above are the ones that matter here.
    assert_eq!(find(&r.cameras, Some("ILCE-7")).items, 2);
    assert_eq!(find(&r.cameras, Some("ILCE-7M4")).items, 2);
    assert_eq!(find(&r.cameras, None).items, 1);
    assert_eq!(find(&r.months, Some("2024-02")).items, 2);
    assert_eq!(items(find(&r.file_types, Some("JPG"))).0, 3);
    // The filter takes any case and a leading dot, like people type it.
    let q = SearchQuery {
        file_type: Some(".arw".into()),
        ..Default::default()
    };
    assert_eq!(found(&cat, &q), 1);
}

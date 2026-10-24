//! Repository-level catalog tests (no sidecars).

mod common;

use keepsake_core::catalog::{
    Asset, AssetId, AssetTag, Blake3Hash, CaptureTime, Catalog, CatalogError, Event, EventId,
    FileRole, ImportItem, ImportItemStatus, ImportSession, ImportSessionId, ImportStatus,
    MediaKind, NewFile, Rating, Rendition, RenditionId, RenditionKind, TagPath, TagSource, Volume,
    VolumeId, VolumeKind, LATEST_VERSION,
};
use keepsake_core::relpath::{CaseSensitivity, RelPath};

fn volume(case: CaseSensitivity) -> Volume {
    Volume {
        id: VolumeId::new_random(),
        label: "Test".into(),
        fs_id: format!("fs-{}", uuid::Uuid::new_v4()),
        kind: VolumeKind::External,
        case,
        read_only: false,
        online: false,
        mount_path: None,
        last_seen_ms: None,
    }
}

fn file(vol: VolumeId, rel: &str, role: FileRole) -> NewFile {
    NewFile {
        volume_id: vol,
        rel_path: RelPath::parse(rel).unwrap(),
        role,
        size_bytes: 10,
        blake3: Blake3Hash::of(rel.as_bytes()),
        xxh3: Some(u64::MAX),
        mtime_ms: Some(1),
        segment_index: None,
    }
}

fn photo(ts: i64, offset: Option<i16>) -> Asset {
    let mut a = Asset::new(AssetId::new_random(), MediaKind::Photo, 1_000);
    a.capture = Some(CaptureTime {
        utc_ms: ts,
        utc_offset_min: offset,
    });
    a
}

fn setup() -> (Catalog, Volume) {
    let mut cat = Catalog::open_in_memory().unwrap();
    let v = volume(CaseSensitivity::Insensitive);
    cat.write(|tx| tx.volumes().upsert(&v)).unwrap();
    (cat, v)
}

#[test]
fn opens_file_catalog_in_wal_mode_and_migrates_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.db");
    {
        let cat = Catalog::open(&path).unwrap();
        assert_eq!(cat.journal_mode().unwrap(), "wal");
        assert_eq!(cat.schema_version().unwrap(), LATEST_VERSION);
    }
    // Reopen: migrations are not re-applied (they would fail on existing tables).
    let cat = Catalog::open(&path).unwrap();
    assert_eq!(cat.schema_version().unwrap(), LATEST_VERSION);
    drop(cat);
    Catalog::delete_files(&path).unwrap();
    assert!(!path.exists());
}

#[test]
fn refuses_newer_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog.db");
    {
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch("PRAGMA user_version = 99").unwrap();
    }
    match Catalog::open(&path) {
        Err(CatalogError::SchemaTooNew { found: 99, .. }) => {}
        Err(other) => panic!("unexpected error {other}"),
        Ok(_) => panic!("opened a newer schema"),
    }
}

#[test]
fn asset_round_trip_and_calendar_uses_local_date() {
    let (mut cat, _) = setup();
    // 2024-06-15 23:30 in UTC is 2024-06-16 08:30 in Tokyo (+09:00).
    let mut a = photo(1_718_494_200_000, Some(540));
    a.rating = Rating::new(4).unwrap();
    a.label = Some("Red".into());
    a.comment = Some("hello".into());
    a.camera_model = Some("ILCE-7M4".into());
    let b = photo(1_718_494_200_000, None);
    cat.write(|tx| {
        tx.assets().insert(&a)?;
        tx.assets().insert(&b)
    })
    .unwrap();

    let got = cat.read(|tx| tx.assets().require(a.id)).unwrap();
    assert_eq!(got, a);
    let days = cat.read(|tx| tx.assets().days()).unwrap();
    let dates: Vec<_> = days.iter().map(|d| d.date.as_str()).collect();
    assert_eq!(dates, vec!["2024-06-15", "2024-06-16"]);
    let on_16 = cat
        .read(|tx| tx.assets().by_local_date("2024-06-16", "2024-06-16"))
        .unwrap();
    assert_eq!(on_16.len(), 1);
    assert_eq!(on_16[0].id, a.id);
}

#[test]
fn unknown_labels_are_created() {
    let (mut cat, _) = setup();
    let mut a = photo(0, None);
    a.label = Some("To Do".into());
    cat.write(|tx| tx.assets().insert(&a)).unwrap();
    let labels = cat.read(|tx| tx.labels().list()).unwrap();
    assert_eq!(labels.len(), 6);
    let todo = labels.iter().find(|l| l.name == "To Do").unwrap();
    assert_eq!(todo.color_hex, None);
    // Case-insensitive match reuses the seeded label.
    let mut b = photo(0, None);
    b.label = Some("red".into());
    cat.write(|tx| tx.assets().insert(&b)).unwrap();
    assert_eq!(cat.read(|tx| tx.labels().list()).unwrap().len(), 6);
    assert_eq!(
        cat.read(|tx| tx.assets().require(b.id))
            .unwrap()
            .label
            .as_deref(),
        Some("Red")
    );
}

#[test]
fn rating_is_checked() {
    assert!(Rating::new(6).is_err());
    assert_eq!(Rating::from_xmp(-1).get(), 0);
    assert_eq!(Rating::from_xmp(9).get(), 5);
}

#[test]
fn write_rolls_back_on_error() {
    let (mut cat, v) = setup();
    let a = photo(0, None);
    let res: Result<(), CatalogError> = cat.write(|tx| {
        tx.assets().insert(&a)?;
        tx.files()
            .insert(a.id, &file(v.id, "DCIM/A.JPG", FileRole::Original))?;
        Err(CatalogError::Invalid("boom".into()))
    });
    assert!(res.is_err());
    assert_eq!(cat.read(|tx| tx.assets().count()).unwrap(), 0);
    assert!(cat
        .read(|tx| tx.files().for_volume(v.id))
        .unwrap()
        .is_empty());
}

#[test]
fn files_dedupe_by_case_and_unicode_on_insensitive_volume() {
    let (mut cat, v) = setup();
    let a = photo(0, None);
    cat.write(|tx| {
        tx.assets().insert(&a)?;
        tx.files()
            .insert(a.id, &file(v.id, "Caf\u{e9}/A.JPG", FileRole::Original))
            .map(|_| ())
    })
    .unwrap();
    for dup in ["cafe\u{301}/a.jpg", "CAF\u{c9}\\A.JPG"] {
        let r = cat.write(|tx| tx.files().insert(a.id, &file(v.id, dup, FileRole::Sidecar)));
        assert!(
            matches!(r, Err(CatalogError::Duplicate(_))),
            "{dup:?} should collide: {r:?}"
        );
    }
    // Lookups find the stored spelling.
    let found = cat
        .read(|tx| {
            tx.files()
                .by_location(v.id, &RelPath::parse("CAFE\u{301}/a.jpg").unwrap())
        })
        .unwrap()
        .unwrap();
    assert_eq!(found.rel_path.as_str(), "Caf\u{e9}/A.JPG");
}

#[test]
fn case_sensitive_volume_allows_case_variants() {
    let mut cat = Catalog::open_in_memory().unwrap();
    let v = volume(CaseSensitivity::Sensitive);
    let a = photo(0, None);
    cat.write(|tx| {
        tx.volumes().upsert(&v)?;
        tx.assets().insert(&a)?;
        tx.files()
            .insert(a.id, &file(v.id, "a.jpg", FileRole::Original))?;
        tx.files()
            .insert(a.id, &file(v.id, "A.JPG", FileRole::RawPair))?;
        // NFC/NFD still collide on a case-sensitive volume.
        tx.files()
            .insert(a.id, &file(v.id, "\u{e9}.jpg", FileRole::Sidecar))?;
        Ok::<_, CatalogError>(())
    })
    .unwrap();
    let r = cat.write(|tx| {
        tx.files()
            .insert(a.id, &file(v.id, "e\u{301}.jpg", FileRole::Sidecar))
    });
    assert!(matches!(r, Err(CatalogError::Duplicate(_))));
}

#[test]
fn one_original_per_asset_per_volume() {
    let (mut cat, v) = setup();
    let v2 = volume(CaseSensitivity::Insensitive);
    let a = photo(0, None);
    cat.write(|tx| {
        tx.volumes().upsert(&v2)?;
        tx.assets().insert(&a)?;
        tx.files()
            .insert(a.id, &file(v.id, "A.JPG", FileRole::Original))?;
        // A backup copy on another volume is also an original.
        tx.files()
            .insert(a.id, &file(v2.id, "A.JPG", FileRole::Original))?;
        Ok::<_, CatalogError>(())
    })
    .unwrap();
    let r = cat.write(|tx| {
        tx.files()
            .insert(a.id, &file(v.id, "B.JPG", FileRole::Original))
    });
    assert!(r.is_err());
    assert_eq!(cat.read(|tx| tx.files().originals(a.id)).unwrap().len(), 2);
}

#[test]
fn file_ids_are_derived_from_location() {
    let (mut cat, v) = setup();
    let a = photo(0, None);
    let f1 = cat
        .write(|tx| {
            tx.assets().insert(&a)?;
            tx.files()
                .insert(a.id, &file(v.id, "X/Y.JPG", FileRole::Original))
        })
        .unwrap();
    assert_eq!(
        f1.id,
        keepsake_core::catalog::FileId::for_location(
            v.id,
            &RelPath::parse("x/y.jpg")
                .unwrap()
                .key(CaseSensitivity::Insensitive)
        )
    );
    let back = cat.read(|tx| tx.files().get(f1.id)).unwrap().unwrap();
    assert_eq!(back, f1);
    assert_eq!(back.xxh3, Some(u64::MAX));
}

#[test]
fn volume_with_files_cannot_be_deleted() {
    let (mut cat, v) = setup();
    let a = photo(0, None);
    cat.write(|tx| {
        tx.assets().insert(&a)?;
        tx.files()
            .insert(a.id, &file(v.id, "A.JPG", FileRole::Original))
    })
    .unwrap();
    assert!(cat.write(|tx| tx.volumes().delete(v.id)).is_err());
}

fn search(cat: &Catalog, q: &str) -> Vec<AssetId> {
    cat.read(|tx| tx.search().text(q, 10)).unwrap()
}

#[test]
fn hierarchical_tags_and_search() {
    let (mut cat, v) = setup();
    let mut a = photo(0, None);
    a.comment = Some("Dinner with Grandma, crème brûlée".into());
    let b = photo(0, None);
    let tokyo = TagPath::parse_lightroom("Travel|Japan|Tokyo").unwrap();
    cat.write(|tx| {
        tx.assets().insert(&a)?;
        tx.assets().insert(&b)?;
        tx.files()
            .insert(a.id, &file(v.id, "DCIM/DSC00001.JPG", FileRole::Original))?;
        tx.files().insert(
            b.id,
            &file(v.id, "PRIVATE/M4ROOT/CLIP/C0001.MP4", FileRole::Original),
        )?;
        tx.tags()
            .set_for_asset(a.id, TagSource::User, &[AssetTag::user(tokyo.clone())])?;
        tx.tags().set_for_asset(
            b.id,
            TagSource::Ai,
            &[AssetTag::ai(
                TagPath::parse_lightroom("Beach").unwrap(),
                0.9,
            )],
        )
    })
    .unwrap();

    let tags = cat.read(|tx| tx.tags().for_asset(a.id)).unwrap();
    assert_eq!(tags, vec![AssetTag::user(tokyo.clone())]);
    let paths = cat.read(|tx| tx.tags().all_paths()).unwrap();
    assert_eq!(paths.len(), 4); // Beach, Travel, Travel|Japan, Travel|Japan|Tokyo

    assert_eq!(search(&cat, "tokyo"), vec![a.id]);
    assert_eq!(search(&cat, "japan"), vec![a.id], "ancestors are indexed");
    assert_eq!(
        search(&cat, "creme brulee"),
        vec![a.id],
        "diacritics folded"
    );
    assert_eq!(
        search(&cat, "DSC0000"),
        vec![a.id],
        "prefix match on file name"
    );
    assert_eq!(search(&cat, "C0001"), vec![b.id]);
    assert_eq!(search(&cat, "beach"), vec![b.id], "AI tags are searchable");
    assert!(search(&cat, "\"NEAR(").is_empty());

    // Case-insensitive tag identity.
    cat.write(|tx| {
        tx.tags().set_for_asset(
            b.id,
            TagSource::User,
            &[AssetTag::user(
                TagPath::parse_lightroom("travel|JAPAN").unwrap(),
            )],
        )
    })
    .unwrap();
    assert_eq!(cat.read(|tx| tx.tags().all_paths()).unwrap().len(), 4);

    // Removing tags and pruning empties the branch.
    cat.write(|tx| {
        tx.tags().set_for_asset(a.id, TagSource::User, &[])?;
        tx.tags().set_for_asset(b.id, TagSource::User, &[])?;
        tx.tags().prune_unused()
    })
    .unwrap();
    assert_eq!(cat.read(|tx| tx.tags().all_paths()).unwrap().len(), 1);
    assert!(search(&cat, "tokyo").is_empty());
    assert!(cat.read(|tx| tx.fts_mismatches()).unwrap().is_empty());
}

#[test]
fn ai_tags_need_confidence() {
    let (mut cat, _) = setup();
    let a = photo(0, None);
    let bad = AssetTag {
        path: TagPath::parse_lightroom("x").unwrap(),
        source: TagSource::Ai,
        confidence: None,
    };
    cat.write(|tx| tx.assets().insert(&a)).unwrap();
    assert!(cat
        .write(|tx| tx
            .tags()
            .set_for_asset(a.id, TagSource::Ai, std::slice::from_ref(&bad)))
        .is_err());
    assert!(TagPath::parse_lightroom("a|b/c").is_err());
    assert!(TagPath::parse_lightroom("a||b").is_err());
}

#[test]
fn events_and_spans() {
    let (mut cat, _) = setup();
    let a = photo(1_000_000, None);
    let b = photo(5_000_000, None);
    let e = Event {
        id: EventId::new_random(),
        title: "Wedding".into(),
        place_name: Some("Tokyo".into()),
        lat: Some(35.6762),
        lon: Some(139.6503),
        created_ms: 7,
    };
    cat.write(|tx| {
        tx.assets().insert(&a)?;
        tx.assets().insert(&b)?;
        tx.events().upsert(&e)?;
        tx.events().assign(&[a.id, b.id], Some(e.id))
    })
    .unwrap();
    let span = cat.read(|tx| tx.events().span(e.id)).unwrap().unwrap();
    assert_eq!(
        (span.start_ms, span.end_ms, span.assets),
        (Some(1_000_000), Some(5_000_000), 2)
    );
    assert_eq!(
        cat.read(|tx| tx.events().get(e.id)).unwrap(),
        Some(e.clone())
    );
    // Deleting the event detaches members.
    cat.write(|tx| tx.events().delete(e.id)).unwrap();
    assert_eq!(
        cat.read(|tx| tx.assets().require(a.id)).unwrap().event_id,
        None
    );
}

#[test]
fn renditions_and_imports() {
    let (mut cat, v) = setup();
    let r = Rendition {
        id: RenditionId::new_random(),
        source_hash: Blake3Hash::of(b"src"),
        recipe_hash: Blake3Hash::of(b"thumb-256"),
        kind: RenditionKind::Thumbnail,
        rel_path: RelPath::parse("ab/cd.jpg").unwrap(),
        size_bytes: 1234,
        created_ms: 1,
        last_used_ms: 1,
    };
    cat.write(|tx| {
        tx.renditions().upsert(&r)?;
        tx.renditions().touch(r.id, 50)
    })
    .unwrap();
    let got = cat
        .read(|tx| {
            tx.renditions()
                .find(&r.source_hash, &r.recipe_hash, RenditionKind::Thumbnail)
        })
        .unwrap()
        .unwrap();
    assert_eq!(got.last_used_ms, 50);

    let s = ImportSession {
        id: ImportSessionId::new_random(),
        source_volume_id: v.id,
        started_ms: 10,
        finished_ms: None,
        status: ImportStatus::Running,
        verified: false,
        template: Some("{yyyy}/{orig}".into()),
        safe_to_format: false,
    };
    let asset = keepsake_core::catalog::ImportAsset {
        session_id: s.id,
        source_key: "dcim/a.jpg".into(),
        asset_id: AssetId::new_random(),
        diff: keepsake_core::catalog::ImportDiff::New,
        state: keepsake_core::catalog::ImportAssetState::Selected,
        error: None,
        attempts: 0,
        updated_ms: 10,
    };
    let item = ImportItem {
        session_id: s.id,
        source_rel_path: RelPath::parse("DCIM/A.JPG").unwrap(),
        dest_volume_id: v.id,
        source_key: "dcim/a.jpg".into(),
        role: FileRole::Original,
        segment_index: None,
        size_bytes: 3,
        quick_hash: u64::MAX,
        dest_rel_path: RelPath::parse("2024/A.JPG").unwrap(),
        file_id: None,
        blake3: Some(Blake3Hash::of(b"a")),
        verified_blake3: Some(Blake3Hash::of(b"a")),
        status: ImportItemStatus::Verified,
        error: None,
    };
    cat.write(|tx| {
        tx.imports().upsert_session(&s)?;
        tx.imports().upsert_asset(&asset)?;
        tx.imports().upsert_item(&item)?;
        tx.imports().set_asset_state(
            s.id,
            "dcim/a.jpg",
            keepsake_core::catalog::ImportAssetState::Copying,
            None,
            11,
        )?;
        tx.imports().finish(s.id, ImportStatus::Completed, true, 20)
    })
    .unwrap();
    let got = cat.read(|tx| tx.imports().get(s.id)).unwrap().unwrap();
    assert_eq!(got.status, ImportStatus::Completed);
    assert!(got.verified);
    assert_eq!(got.template.as_deref(), Some("{yyyy}/{orig}"));
    let a = cat
        .read(|tx| tx.imports().asset(s.id, "dcim/a.jpg"))
        .unwrap()
        .unwrap();
    assert_eq!(
        (a.state, a.attempts),
        (keepsake_core::catalog::ImportAssetState::Copying, 1)
    );
    // An item must belong to an asset of its session.
    let mut orphan = item.clone();
    orphan.source_key = "nope".into();
    orphan.source_rel_path = RelPath::parse("DCIM/B.JPG").unwrap();
    assert!(cat.write(|tx| tx.imports().upsert_item(&orphan)).is_err());
    assert_eq!(cat.read(|tx| tx.imports().items(s.id)).unwrap(), vec![item]);
}

#[test]
fn search_query_filters_combine() {
    use keepsake_core::catalog::SearchQuery;
    let (mut cat, v) = setup();
    // 2024-06-15 13:30 Tokyo, 2024-06-16 08:30 Tokyo, undated.
    let mut a = photo(1_718_425_800_000, Some(540));
    a.camera_model = Some("ILCE-7M4".into());
    let mut b = photo(1_718_494_200_000, Some(540));
    b.camera_model = Some("FX3".into());
    b.media_kind = MediaKind::Video;
    let c = photo(0, None);
    let c = Asset { capture: None, ..c };
    cat.write(|tx| {
        for (x, n) in [(&a, "A.JPG"), (&b, "B.MP4"), (&c, "C.JPG")] {
            tx.assets().insert(x)?;
            tx.files()
                .insert(x.id, &file(v.id, n, FileRole::Original))?;
        }
        Ok::<_, CatalogError>(())
    })
    .unwrap();
    let q = |q: SearchQuery| cat.read(|tx| tx.search().query(&q)).unwrap();
    let day = |d: &str| Some(d.to_owned());
    assert_eq!(
        q(SearchQuery {
            from: day("2024-06-15"),
            to: day("2024-06-15"),
            ..Default::default()
        }),
        vec![a.id]
    );
    assert_eq!(
        q(SearchQuery {
            from: day("2024-06-16"),
            ..Default::default()
        }),
        vec![b.id]
    );
    assert_eq!(
        q(SearchQuery {
            camera: Some("7m4".into()),
            ..Default::default()
        }),
        vec![a.id]
    );
    assert_eq!(
        q(SearchQuery {
            kind: Some(MediaKind::Video),
            ..Default::default()
        }),
        vec![b.id]
    );
    assert_eq!(
        q(SearchQuery {
            text: Some("C".into()),
            ..Default::default()
        }),
        vec![c.id]
    );
    // No filter: every asset, by capture time, undated last.
    assert_eq!(q(SearchQuery::default()), vec![a.id, b.id, c.id]);
    assert_eq!(
        q(SearchQuery {
            limit: Some(1),
            ..Default::default()
        }),
        vec![a.id]
    );
    assert!(cat
        .read(|tx| tx.search().query(&SearchQuery {
            from: day("15/06/2024"),
            ..Default::default()
        }))
        .is_err());
}

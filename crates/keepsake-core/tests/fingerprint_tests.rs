//! Full fingerprints made in the background (ADR-029): a folder added in
//! batches is catalogued from the start and end of each file, and the
//! fingerprint job reads each file in full later.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use keepsake_core::catalog::{
    Blake3Hash, Catalog, File, FileHash, Rendition, RenditionId, RenditionKind,
};
use keepsake_core::fingerprint::{
    enqueue_pending, fingerprint_asset, FingerprintError, JOB_FINGERPRINT,
};
use keepsake_core::ingest::destination_volume;
use keepsake_core::ingest::fault::FaultPlan;
use keepsake_core::library::Library;
use keepsake_core::rebuild::{add_folder, RebuildOptions, BATCH_FILES};
use keepsake_core::relpath::RelPath;
use keepsake_core::render::{Cache, RepairReport, Variant};
use keepsake_core::verify::{verify, Problem};

/// A folder of `n` photos, each 3 MiB so the quick hash leaves the middle unread.
fn photos(root: &Path, n: usize) -> PathBuf {
    let dir = root.join("2019");
    for i in 0..n {
        let mut bytes = vec![i as u8; 3 << 20];
        bytes[0] = 0xFF;
        common::write_file(&dir, &format!("May/DSC{i:05}.JPG"), &bytes);
    }
    dir
}

fn add(cat: &mut Catalog, lib: &Library, dir: &Path, batched: bool) {
    add_folder(
        cat,
        lib,
        dir,
        &mut RebuildOptions {
            batch_files: batched.then_some(BATCH_FILES),
            ..Default::default()
        },
    )
    .unwrap();
}

fn all_files(cat: &Catalog) -> Vec<File> {
    let mut out = Vec::new();
    for id in cat.read(|tx| tx.assets().list_ids()).unwrap() {
        out.extend(cat.read(|tx| tx.files().for_asset(id)).unwrap());
    }
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    out
}

fn fingerprint_all(cat: &mut Catalog, lib: &Library) -> u64 {
    let mut n = 0;
    for id in cat.read(|tx| tx.files().assets_unfingerprinted()).unwrap() {
        n += fingerprint_asset(cat, lib, &cache_of(lib), id).unwrap();
    }
    n
}

#[test]
fn a_batched_add_leaves_full_fingerprints_for_the_background() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 5);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add(&mut cat, &lib, &dir, true);

    let files = all_files(&cat);
    assert_eq!(files.len(), 5);
    assert!(files.iter().all(|f| !f.hash.is_full() && f.xxh3.is_some()));
    // Stand-in keys are unique and are never taken for content.
    let mut keys: Vec<_> = files.iter().map(|f| f.hash.key()).collect();
    keys.dedup();
    assert_eq!(keys.len(), 5);
    let real = Blake3Hash::of(&std::fs::read(dir.join("May/DSC00000.JPG")).unwrap());
    assert!(cat.read(|tx| tx.files().by_hash(&real)).unwrap().is_empty());
    let pending = cat
        .read(|tx| tx.files().unfingerprinted_by_volume())
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, 5);

    // One job per asset; queued again only while files are left.
    assert_eq!(enqueue_pending(&mut cat).unwrap(), 5);
    let counts = cat.read(|tx| tx.jobs().counts()).unwrap();
    assert_eq!(counts.by_kind[JOB_FINGERPRINT]["queued"], 5);

    assert_eq!(fingerprint_all(&mut cat, &lib), 5);
    for f in all_files(&cat) {
        let bytes = std::fs::read(f.rel_path.to_fs_path(&dir)).unwrap();
        assert_eq!(
            f.hash,
            FileHash::Full(Blake3Hash::of(&bytes)),
            "{}",
            f.rel_path
        );
    }
    assert_eq!(cat.read(|tx| tx.files().by_hash(&real)).unwrap().len(), 1);
    assert!(cat
        .read(|tx| tx.files().assets_unfingerprinted())
        .unwrap()
        .is_empty());
    // Doing it again finds nothing to do.
    assert_eq!(fingerprint_all(&mut cat, &lib), 0);
}

#[test]
fn adding_without_batches_still_reads_every_file_in_full() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 2);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add(&mut cat, &lib, &dir, false);
    assert!(all_files(&cat).iter().all(|f| f.hash.is_full()));
}

#[test]
fn a_file_changed_after_adding_is_not_fingerprinted() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 2);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add(&mut cat, &lib, &dir, true);
    let f = all_files(&cat).remove(0);
    // Same size, same first and last MiB, new middle; and a new date.
    let path = f.rel_path.to_fs_path(&dir);
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[(3 << 20) / 2] ^= 1;
    std::fs::write(&path, &bytes).unwrap();
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(120);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    match fingerprint_asset(&mut cat, &lib, &cache_of(&lib), f.asset_id) {
        Err(FingerprintError::Changed(p)) => assert_eq!(p, f.rel_path.to_string()),
        other => panic!("expected a change, got {other:?}"),
    }
    let now = cat.read(|tx| tx.files().get(f.id)).unwrap().unwrap();
    assert!(!now.hash.is_full());

    // A scan of the folder catalogues it again, and then it is read.
    add(&mut cat, &lib, &dir, true);
    let again = cat
        .read(|tx| tx.files().by_location(f.volume_id, &f.rel_path))
        .unwrap()
        .unwrap();
    assert_eq!(
        fingerprint_asset(&mut cat, &lib, &cache_of(&lib), again.asset_id).unwrap(),
        1
    );
    let done = cat.read(|tx| tx.files().get(again.id)).unwrap().unwrap();
    assert_eq!(done.hash, FileHash::Full(Blake3Hash::of(&bytes)));
}

#[test]
fn a_disconnected_drive_is_left_for_later() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 1);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add(&mut cat, &lib, &dir, true);
    let f = all_files(&cat).remove(0);
    let moved = tmp.path().join("unplugged");
    std::fs::rename(&dir, &moved).unwrap();
    assert!(matches!(
        fingerprint_asset(&mut cat, &lib, &cache_of(&lib), f.asset_id),
        Err(FingerprintError::Offline)
    ));
    std::fs::rename(&moved, &dir).unwrap();
    assert_eq!(
        fingerprint_asset(&mut cat, &lib, &cache_of(&lib), f.asset_id).unwrap(),
        1
    );
}

/// The rendition cache the tests use: next to the library.
fn cache_of(lib: &Library) -> Cache {
    Cache::new(lib.root().join("cache"))
}

/// The grid's check (`convert::cached` in the app): the file at the name
/// computed from the hash, with no catalog.
fn grid_thumb(cache: &Cache, hash: &Blake3Hash, v: Variant) -> Option<PathBuf> {
    let p = Cache::rel_path(hash, v).unwrap().to_fs_path(cache.root());
    p.is_file().then_some(p)
}

/// One photo added in batches (a stand-in key), and its real BLAKE3.
fn one_pending(tmp: &Path) -> (Library, Catalog, File, Blake3Hash) {
    let dir = photos(tmp, 1);
    let lib = Library::create(&tmp.join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add(&mut cat, &lib, &dir, true);
    let f = all_files(&cat).remove(0);
    assert!(!f.hash.is_full());
    let real = Blake3Hash::of(&std::fs::read(dir.join("May/DSC00000.JPG")).unwrap());
    (lib, cat, f, real)
}

fn bytes_at(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap()
}

#[test]
fn thumbnails_made_before_the_fingerprint_follow_it_to_the_new_name() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, mut cat, f, real) = one_pending(tmp.path());
    let cache = cache_of(&lib);
    let old = f.hash.key();
    let thumbs = [Variant::Thumb256, Variant::Thumb1024];
    for v in thumbs {
        cache
            .put_bytes(&mut cat, &old, v, v.name().as_bytes())
            .unwrap();
        assert!(grid_thumb(&cache, &old, v).is_some());
    }

    fingerprint_asset(&mut cat, &lib, &cache, f.asset_id).unwrap();

    for v in thumbs {
        // The grid finds the thumbnail under the real fingerprint...
        let p = grid_thumb(&cache, &real, v).expect("the grid finds the thumbnail");
        assert_eq!(bytes_at(&p), v.name().as_bytes());
        // ...the row names that file...
        let r = cat
            .read(|tx| tx.renditions().find(&real, &v.recipe_hash(), v.kind()))
            .unwrap()
            .expect("the row moved to the real fingerprint");
        assert_eq!(r.rel_path, Cache::rel_path(&real, v).unwrap());
        // ...and nothing is left under the stand-in key.
        assert!(grid_thumb(&cache, &old, v).is_none());
        assert!(cat
            .read(|tx| tx.renditions().find(&old, &v.recipe_hash(), v.kind()))
            .unwrap()
            .is_none());
        assert_eq!(
            cache.find(&mut cat, &real, v).unwrap().unwrap().1,
            p,
            "the cache agrees"
        );
    }
}

#[test]
fn a_thumbnail_already_made_under_the_real_key_wins() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, mut cat, f, real) = one_pending(tmp.path());
    let cache = cache_of(&lib);
    let old = f.hash.key();
    let v = Variant::Thumb256;
    cache.put_bytes(&mut cat, &old, v, b"stand-in").unwrap();
    cache.put_bytes(&mut cat, &real, v, b"real").unwrap();

    fingerprint_asset(&mut cat, &lib, &cache, f.asset_id).unwrap();

    let p = grid_thumb(&cache, &real, v).unwrap();
    assert_eq!(bytes_at(&p), b"real");
    assert!(
        grid_thumb(&cache, &old, v).is_none(),
        "the leftover is removed"
    );
    assert!(cat
        .read(|tx| tx.renditions().find(&old, &v.recipe_hash(), v.kind()))
        .unwrap()
        .is_none());
    assert_eq!(cat.read(|tx| tx.renditions().totals()).unwrap().0, 1);
}

#[test]
fn a_crash_between_the_row_and_the_file_is_mended_on_the_next_look() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, mut cat, f, real) = one_pending(tmp.path());
    let cache = cache_of(&lib);
    let v = Variant::Thumb256;
    cache
        .put_bytes(&mut cat, &f.hash.key(), v, b"thumb")
        .unwrap();
    // The catalog step only (as a version without the file move did, or a
    // crash right after it): the row has the new key, the file the old name.
    assert!(cat
        .write(|tx| tx.files().set_fingerprint(f.id, &real))
        .unwrap());
    assert!(grid_thumb(&cache, &real, v).is_none());

    // The cache does not report a misnamed file as present: it moves it.
    let (r, p) = cache.find(&mut cat, &real, v).unwrap().unwrap();
    assert_eq!(r.rel_path, Cache::rel_path(&real, v).unwrap());
    assert_eq!(grid_thumb(&cache, &real, v), Some(p));
    let again = cat
        .read(|tx| tx.renditions().find(&real, &v.recipe_hash(), v.kind()))
        .unwrap()
        .unwrap();
    assert_eq!(again.rel_path, r.rel_path);
}

#[test]
fn a_misnamed_row_whose_file_is_gone_counts_as_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let (lib, mut cat, f, real) = one_pending(tmp.path());
    let cache = cache_of(&lib);
    let v = Variant::Thumb256;
    let made = cache
        .put_bytes(&mut cat, &f.hash.key(), v, b"thumb")
        .unwrap();
    cat.write(|tx| tx.files().set_fingerprint(f.id, &real))
        .unwrap();
    std::fs::remove_file(made).unwrap();

    assert!(cache.find(&mut cat, &real, v).unwrap().is_none());
    assert_eq!(cat.read(|tx| tx.renditions().totals()).unwrap().0, 0);
}

/// A row as an older version left it: `source` in the catalog, the file
/// named after `named`.
fn misnamed_row(
    cat: &mut Catalog,
    cache: &Cache,
    source: Blake3Hash,
    named: Blake3Hash,
    file: Option<&[u8]>,
) -> RenditionId {
    let v = Variant::Thumb256;
    let rel = Cache::rel_path(&named, v).unwrap();
    if let Some(b) = file {
        let p = rel.to_fs_path(cache.root());
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b).unwrap();
    }
    let id = RenditionId::new_random();
    cat.write(|tx| {
        tx.renditions().upsert(&Rendition {
            id,
            source_hash: source,
            recipe_hash: v.recipe_hash(),
            kind: RenditionKind::Thumbnail,
            rel_path: rel.clone(),
            size_bytes: 1,
            created_ms: 1,
            last_used_ms: 1,
        })
    })
    .unwrap();
    id
}

#[test]
fn repair_names_mends_a_library_with_misnamed_thumbnails() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let cache = cache_of(&lib);
    let v = Variant::Thumb256;
    let h = |s: &str| Blake3Hash::of(s.as_bytes());

    // More than one batch of plain misnamed rows with their files.
    let many = 1_203;
    for i in 0..many {
        let s = format!("{i}");
        misnamed_row(
            &mut cat,
            &cache,
            h(&format!("real {s}")),
            h(&format!("old {s}")),
            Some(s.as_bytes()),
        );
    }
    // Its file is gone.
    misnamed_row(&mut cat, &cache, h("gone"), h("old gone"), None);
    // The proper name is already taken by the same rendition.
    let p = Cache::rel_path(&h("taken"), v)
        .unwrap()
        .to_fs_path(cache.root());
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, b"kept").unwrap();
    misnamed_row(&mut cat, &cache, h("taken"), h("old taken"), Some(b"old"));
    // A correct row, and a row wrongly pointing at its file.
    cache.put_bytes(&mut cat, &h("right"), v, b"right").unwrap();
    misnamed_row(&mut cat, &cache, h("stray"), h("right"), None);

    let rep = cache.repair_names(&mut cat).unwrap();
    assert_eq!(rep.renamed, many + 1, "{rep:?}");
    assert_eq!(rep.dropped, 2, "{rep:?}");

    for i in [0, 600, many - 1] {
        let s = format!("{i}");
        let p = grid_thumb(&cache, &h(&format!("real {s}")), v).unwrap();
        assert_eq!(bytes_at(&p), s.as_bytes());
        assert!(grid_thumb(&cache, &h(&format!("old {s}")), v).is_none());
    }
    assert!(cat
        .read(|tx| tx.renditions().find(&h("gone"), &v.recipe_hash(), v.kind()))
        .unwrap()
        .is_none());
    assert_eq!(
        bytes_at(&grid_thumb(&cache, &h("taken"), v).unwrap()),
        b"kept"
    );
    assert!(grid_thumb(&cache, &h("old taken"), v).is_none());
    assert_eq!(
        bytes_at(&grid_thumb(&cache, &h("right"), v).unwrap()),
        b"right",
        "another row's file is never moved or removed"
    );
    assert!(cat
        .read(|tx| tx
            .renditions()
            .find(&h("stray"), &v.recipe_hash(), v.kind()))
        .unwrap()
        .is_none());
    // Every row now names its own file, and a second pass has nothing to do.
    for r in cat.read(|tx| tx.renditions().list_lru()).unwrap() {
        assert_eq!(r.rel_path, Cache::expected_rel_path(&r).unwrap());
        assert!(r.rel_path.to_fs_path(cache.root()).is_file());
    }
    assert_eq!(
        cache.repair_names(&mut cat).unwrap(),
        RepairReport::default()
    );
}

#[test]
fn verify_checks_unfingerprinted_files_by_size_and_quick_hash() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 3);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add(&mut cat, &lib, &dir, true);
    let no = AtomicBool::new(false);
    let r = verify(&cat, &lib, None, &mut |_| {}, &no).unwrap();
    assert!(r.issues.is_empty(), "{r:?}");
    assert_eq!(r.volumes[0].not_fingerprinted, 3);

    // The last MiB of one file changes, another goes.
    let files = all_files(&cat);
    let p0 = files[0].rel_path.to_fs_path(&dir);
    let mut bytes = std::fs::read(&p0).unwrap();
    let n = bytes.len();
    bytes[n - 10] ^= 1;
    std::fs::write(&p0, &bytes).unwrap();
    std::fs::remove_file(files[1].rel_path.to_fs_path(&dir)).unwrap();
    let r = verify(&cat, &lib, None, &mut |_| {}, &no).unwrap();
    assert_eq!(r.missing(), 1);
    assert_eq!(r.changed(), 1);
    assert!(r
        .issues
        .iter()
        .any(|i| i.problem == Problem::ChangedSinceAdded));
}

#[test]
fn a_backup_fingerprints_what_it_copies() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 2);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add(&mut cat, &lib, &dir, true);
    let dest = destination_volume(
        &lib,
        &tmp.path().join("backup"),
        "B",
        keepsake_core::catalog::VolumeKind::External,
    )
    .unwrap();
    let run = |cat: &mut Catalog| {
        keepsake_core::backup::run(
            cat,
            &lib,
            &dest,
            &FaultPlan::none(),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap()
    };
    let rep = run(&mut cat);
    assert_eq!(rep.files_copied, 2, "{rep:?}");
    assert!(rep.failed.is_empty(), "{rep:?}");
    // Both the originals and the backup copies now carry the real fingerprint.
    let files = all_files(&cat);
    assert_eq!(files.len(), 4);
    assert!(files.iter().all(|f| f.hash.is_full()));
    // A second run finds the copies already there.
    let rep = run(&mut cat);
    assert_eq!(rep.files_copied, 0, "{rep:?}");
    assert_eq!(rep.files_already_there, 2, "{rep:?}");
}

#[test]
fn a_backup_refuses_a_file_changed_since_it_was_added() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 1);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add(&mut cat, &lib, &dir, true);
    let f = all_files(&cat).remove(0);
    let p = f.rel_path.to_fs_path(&dir);
    let mut bytes = std::fs::read(&p).unwrap();
    bytes[5] ^= 1;
    std::fs::write(&p, &bytes).unwrap();
    let dest = destination_volume(
        &lib,
        &tmp.path().join("backup"),
        "B",
        keepsake_core::catalog::VolumeKind::External,
    )
    .unwrap();
    let rep = keepsake_core::backup::run(
        &mut cat,
        &lib,
        &dest,
        &FaultPlan::none(),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(rep.files_copied, 0, "{rep:?}");
    assert_eq!(rep.failed.len(), 1, "{rep:?}");
    assert!(
        rep.failed[0].1.contains("changed since it was added"),
        "{rep:?}"
    );
    assert!(!cat
        .read(|tx| tx.files().get(f.id))
        .unwrap()
        .unwrap()
        .hash
        .is_full());
}

#[test]
fn a_fingerprint_job_does_not_hide_an_asset_from_the_render_backlog() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 3);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add(&mut cat, &lib, &dir, true);
    assert_eq!(enqueue_pending(&mut cat).unwrap(), 3);
    // Render jobs for all three; the fingerprint jobs are already queued.
    assert_eq!(keepsake_core::render::enqueue_backlog(&mut cat).unwrap(), 3);
    let counts = cat.read(|tx| tx.jobs().counts()).unwrap();
    assert_eq!(
        counts.by_kind[keepsake_core::render::JOB_STILLS]["queued"],
        3
    );
    assert_eq!(counts.by_kind[JOB_FINGERPRINT]["queued"], 3);
}

/// A large backlog is queued in many small writes (so the catalog is never
/// held for long) and still runs newest first across all of them.
#[test]
fn a_large_backlog_is_queued_in_small_writes_and_stays_newest_first() {
    use keepsake_core::catalog::{
        Asset, AssetId, CaptureTime, FileRole, MediaKind, NewFile, Volume, VolumeId, VolumeKind,
    };
    use keepsake_core::relpath::CaseSensitivity;

    let mut cat = Catalog::open_in_memory().unwrap();
    let vol = Volume {
        id: VolumeId::new_random(),
        label: "NAS".into(),
        fs_id: "fs-nas".into(),
        kind: VolumeKind::External,
        case: CaseSensitivity::Insensitive,
        read_only: true,
        online: true,
        mount_path: Some(PathBuf::from("/mnt/nas")),
        last_seen_ms: None,
    };
    cat.write(|tx| tx.volumes().upsert(&vol)).unwrap();
    // More than two writes' worth, in a shuffled order of capture times.
    let n = 450u64;
    for i in 0..n {
        let mut a = Asset::new(AssetId::new_random(), MediaKind::Photo, 1);
        a.capture = Some(CaptureTime {
            utc_ms: ((i * 7919) % n) as i64 * 1000,
            utc_offset_min: Some(0),
        });
        let rel = RelPath::parse(&format!("p/{i}.jpg")).unwrap();
        let f = NewFile {
            volume_id: vol.id,
            hash: FileHash::pending_for(vol.id, rel.as_str(), 10, Some(i)),
            rel_path: rel,
            role: FileRole::Original,
            size_bytes: 10,
            xxh3: Some(i),
            mtime_ms: Some(1),
            segment_index: None,
        };
        cat.write(|tx| {
            tx.assets().insert(&a)?;
            tx.files().insert(a.id, &f)
        })
        .unwrap();
    }
    let newest_first = cat.read(|tx| tx.files().assets_unfingerprinted()).unwrap();
    assert_eq!(newest_first.len() as u64, n);

    assert_eq!(enqueue_pending(&mut cat).unwrap(), n);
    let mut order = Vec::new();
    while let Some(job) = cat
        .write(|tx| tx.jobs().claim(&[JOB_FINGERPRINT], i64::MAX))
        .unwrap()
    {
        order.push(job.asset_id);
    }
    assert_eq!(order, newest_first);
}

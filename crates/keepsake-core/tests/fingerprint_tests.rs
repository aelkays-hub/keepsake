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
        n += fingerprint_asset(cat, lib, id).unwrap();
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
    match fingerprint_asset(&mut cat, &lib, f.asset_id) {
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
        fingerprint_asset(&mut cat, &lib, again.asset_id).unwrap(),
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
        fingerprint_asset(&mut cat, &lib, f.asset_id),
        Err(FingerprintError::Offline)
    ));
    std::fs::rename(&moved, &dir).unwrap();
    assert_eq!(fingerprint_asset(&mut cat, &lib, f.asset_id).unwrap(), 1);
}

#[test]
fn thumbnails_made_before_the_fingerprint_are_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 1);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add(&mut cat, &lib, &dir, true);
    let f = all_files(&cat).remove(0);
    let recipe = Blake3Hash::of(b"thumb-256");
    cat.write(|tx| {
        tx.renditions().upsert(&Rendition {
            id: RenditionId::new_random(),
            source_hash: f.hash.key(),
            recipe_hash: recipe,
            kind: RenditionKind::Thumbnail,
            rel_path: RelPath::parse("thumbnail/ab/x.jpg").unwrap(),
            size_bytes: 10,
            created_ms: 1,
            last_used_ms: 1,
        })
    })
    .unwrap();
    fingerprint_asset(&mut cat, &lib, f.asset_id).unwrap();
    let real = cat
        .read(|tx| tx.files().get(f.id))
        .unwrap()
        .unwrap()
        .hash
        .full()
        .unwrap();
    let r = cat
        .read(|tx| {
            tx.renditions()
                .find(&real, &recipe, RenditionKind::Thumbnail)
        })
        .unwrap()
        .expect("the thumbnail moved to the real fingerprint");
    assert_eq!(r.rel_path.as_str(), "thumbnail/ab/x.jpg");
    assert!(cat
        .read(|tx| {
            tx.renditions()
                .find(&f.hash.key(), &recipe, RenditionKind::Thumbnail)
        })
        .unwrap()
        .is_none());
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

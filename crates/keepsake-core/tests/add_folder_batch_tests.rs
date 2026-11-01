//! Adding a folder a batch at a time (`RebuildOptions::batch_files`): the
//! folder is in the library at once, items are saved as they are read, a
//! scan that stops keeps them, and the next scan carries on from there.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use keepsake_core::catalog::Catalog;
use keepsake_core::library::{Library, LibraryError};
use keepsake_core::rebuild::{add_folder, RebuildOptions, RebuildProgress, RebuildReport};

/// `n` photos in two month folders, and a note.
fn photos(root: &Path, n: usize) -> PathBuf {
    let dir = root.join("2010");
    for i in 0..n {
        let month = if i < n / 2 { "January" } else { "February" };
        common::write_file(
            &dir,
            &format!("{month}/DSC{i:05}.JPG"),
            format!("jpeg {i}").as_bytes(),
        );
    }
    common::write_file(&dir, "notes.txt", b"not media");
    dir
}

fn assets(cat: &Catalog) -> u64 {
    cat.read(|tx| tx.assets().count()).unwrap()
}

/// Add `dir` in batches of 20, stopping once `stop_after` items are saved.
fn add_batched(
    cat: &mut Catalog,
    lib: &Library,
    dir: &Path,
    stop_after: Option<u64>,
) -> (Result<RebuildReport, LibraryError>, Vec<u64>, u64) {
    let stop = AtomicBool::new(false);
    let mut saved = Vec::new();
    let mut bytes = 0u64;
    let mut f = |p: &RebuildProgress<'_>| match *p {
        RebuildProgress::Saved { assets, .. } => {
            // In the library from the first report on.
            assert!(
                lib.volumes().unwrap().iter().any(|v| v.label == "2010"),
                "not registered before saving"
            );
            saved.push(assets);
            if stop_after.is_some_and(|n| assets >= n) {
                stop.store(true, Ordering::SeqCst);
            }
        }
        RebuildProgress::File { bytes: b, .. } => bytes += b,
        _ => {}
    };
    let r = add_folder(
        cat,
        lib,
        dir,
        &mut RebuildOptions {
            cancel: Some(&stop),
            progress: Some(&mut f),
            batch_files: Some(20),
            ..Default::default()
        },
    )
    .map(|(_, r)| r);
    (r, saved, bytes)
}

#[test]
fn items_are_saved_twenty_files_at_a_time() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 50);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let (r, saved, bytes) = add_batched(&mut cat, &lib, &dir, None);
    let r = r.unwrap();
    // Registered with nothing yet, then 20, 40 and the last 10.
    assert_eq!(saved, [0, 20, 40, 50]);
    assert_eq!((r.assets, r.files_indexed, r.resumed), (50, 50, 0));
    assert_eq!(assets(&cat), 50);
    // Every file counted once for the progress, the note too.
    let total: u64 = walk_bytes(&dir);
    assert_eq!(bytes, total);
}

#[test]
fn a_scan_that_stops_keeps_what_it_saved_and_the_next_carries_on() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 50);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let before = common::tree_hashes(&dir);

    let (r, saved, _) = add_batched(&mut cat, &lib, &dir, Some(20));
    assert!(matches!(r, Err(LibraryError::Cancelled)), "{r:?}");
    // The batch in hand when it stopped was saved too.
    let kept = *saved.last().unwrap();
    assert!((20..50).contains(&kept), "{saved:?}");
    assert_eq!(assets(&cat), kept);
    assert_eq!(lib.volumes().unwrap().len(), 1, "the folder stays");

    // Again: what was saved is not read again.
    let (r, saved, _) = add_batched(&mut cat, &lib, &dir, None);
    let r = r.unwrap();
    assert_eq!(saved.first(), Some(&kept), "starts from what was saved");
    assert_eq!(r.resumed, kept);
    assert_eq!(r.assets, 50);
    assert_eq!(assets(&cat), 50);
    assert_eq!(lib.volumes().unwrap().len(), 1);
    // Nothing in the folder was changed.
    assert_eq!(common::tree_hashes(&dir), before);
}

#[test]
fn scanning_again_reads_changed_files_and_drops_removed_ones() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 30);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add_batched(&mut cat, &lib, &dir, None).0.unwrap();
    let hash_of = |cat: &Catalog, name: &str| {
        let vol = lib.volumes().unwrap()[0].id;
        cat.read(|tx| tx.files().for_volume(vol))
            .unwrap()
            .into_iter()
            .find(|f| f.rel_path.as_str().ends_with(name))
            .map(|f| f.hash)
    };
    let old = hash_of(&cat, "DSC00003.JPG").unwrap();

    std::fs::remove_file(dir.join("January/DSC00001.JPG")).unwrap();
    common::write_file(&dir, "January/DSC00003.JPG", b"edited elsewhere, longer");
    common::write_file(&dir, "February/DSC00100.JPG", b"new one");

    let r = add_batched(&mut cat, &lib, &dir, None).0.unwrap();
    assert_eq!(r.assets, 30, "{r:?}");
    assert_eq!(r.resumed, 28, "the unchanged ones are not read again");
    assert_eq!(assets(&cat), 30);
    assert_eq!(hash_of(&cat, "DSC00001.JPG"), None, "removed");
    assert_ne!(hash_of(&cat, "DSC00003.JPG"), Some(old), "read again");
    assert!(hash_of(&cat, "DSC00100.JPG").is_some(), "added");
}

#[test]
fn a_scan_that_stops_before_saving_anything_still_lists_the_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = photos(tmp.path(), 5);
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let (r, saved, _) = add_batched(&mut cat, &lib, &dir, Some(0));
    assert!(matches!(r, Err(LibraryError::Cancelled)), "{r:?}");
    assert_eq!(saved, [0]);
    assert_eq!(lib.volumes().unwrap().len(), 1);
    let (r, _, _) = add_batched(&mut cat, &lib, &dir, None);
    assert_eq!(r.unwrap().assets, 5);
}

fn walk_bytes(dir: &Path) -> u64 {
    let mut n = 0;
    for e in std::fs::read_dir(dir).unwrap() {
        let e = e.unwrap();
        let m = e.metadata().unwrap();
        n += if m.is_dir() {
            walk_bytes(&e.path())
        } else {
            m.len()
        };
    }
    n
}

#[test]
fn each_lightroom_sidecar_goes_with_its_own_photo() {
    // Enough sidecars to be read several at a time, each with its own rating.
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("2010");
    let lr =
        String::from_utf8(std::fs::read(common::fixture("xmp/lightroom.xmp")).unwrap()).unwrap();
    assert!(lr.contains(r#"xmp:Rating="4""#));
    for i in 0..60usize {
        let folder = if i % 2 == 0 { "January" } else { "February" };
        common::write_file(&dir, &format!("{folder}/DSC{i:05}.ARW"), b"raw");
        let rating = format!(r#"xmp:Rating="{}""#, i % 6);
        common::write_file(
            &dir,
            &format!("{folder}/DSC{i:05}.xmp"),
            lr.replace(r#"xmp:Rating="4""#, &rating).as_bytes(),
        );
    }
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let (r, _, _) = add_batched(&mut cat, &lib, &dir, None);
    let r = r.unwrap();
    assert_eq!(r.assets, 60);
    assert_eq!(r.sidecars_used, 60);
    for id in cat.read(|tx| tx.assets().list_ids()).unwrap() {
        let rating = cat.read(|tx| tx.assets().require(id)).unwrap().rating.get();
        let files = cat.read(|tx| tx.files().for_asset(id)).unwrap();
        assert_eq!(files.len(), 1);
        let name = files[0].rel_path.file_name().to_owned();
        let i: usize = name[3..8].parse().unwrap();
        assert_eq!(usize::from(rating), i % 6, "{name}");
    }
}

//! Adding a folder of existing photos in place (`rebuild::add_folder`):
//! nothing is copied or written into the folder, Lightroom sidecars are
//! read, a library or cache inside the folder is left out, and overlapping
//! folders are refused.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use keepsake_core::catalog::{Catalog, Rating};
use keepsake_core::library::{Library, LibraryError};
use keepsake_core::metadata::MetadataService;
use keepsake_core::rebuild::{add_folder, RebuildOptions, RebuildProgress};

/// A folder laid out like a photo archive: months, a RAW with a Lightroom
/// sidecar, a video and a note.
fn archive(root: &Path) -> PathBuf {
    let dir = root.join("2025");
    common::write_file(&dir, "January/DSC00001.JPG", b"jpeg one");
    common::write_file(&dir, "January/DSC00002.ARW", b"raw two");
    let lr = std::fs::read(common::fixture("xmp/lightroom.xmp")).unwrap();
    common::write_file(&dir, "January/DSC00002.xmp", &lr);
    common::write_file(&dir, "February/C0001.MP4", b"video");
    common::write_file(&dir, "notes.txt", b"not media");
    dir
}

fn add(cat: &mut Catalog, lib: &Library, dir: &Path) -> keepsake_core::rebuild::RebuildReport {
    add_folder(cat, lib, dir, &mut RebuildOptions::default())
        .unwrap()
        .1
}

/// Every catalogued file's path on its volume.
fn catalogued(cat: &Catalog) -> Vec<String> {
    let mut out = Vec::new();
    for id in cat.read(|tx| tx.assets().list_ids()).unwrap() {
        for f in cat.read(|tx| tx.files().for_asset(id)).unwrap() {
            out.push(f.rel_path.to_string());
        }
    }
    out.sort();
    out
}

fn ratings(cat: &Catalog) -> Vec<u8> {
    let mut out: Vec<u8> = cat
        .read(|tx| tx.assets().list_ids())
        .unwrap()
        .into_iter()
        .map(|id| cat.read(|tx| tx.assets().require(id)).unwrap().rating.get())
        .collect();
    out.sort();
    out
}

#[test]
fn a_folder_is_catalogued_in_place_and_never_written_to() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = archive(tmp.path());
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let before = common::tree_hashes(&dir);

    let (rec, report) = add_folder(&mut cat, &lib, &dir, &mut RebuildOptions::default()).unwrap();
    assert!(
        rec.read_only,
        "edits must go to the library, not the folder"
    );
    assert_eq!(rec.label, "2025");
    assert_eq!(report.assets, 3, "{report:?}");
    assert_eq!(
        catalogued(&cat),
        [
            "February/C0001.MP4",
            "January/DSC00001.JPG",
            "January/DSC00002.ARW"
        ]
    );
    // The Lightroom rating came with the RAW.
    assert_eq!(ratings(&cat), [0, 0, 4]);
    assert_eq!(lib.volumes().unwrap().len(), 1);

    // A rating made in Keepsake goes to the library's sidecars.
    let id = cat.read(|tx| tx.assets().list_ids()).unwrap()[0];
    MetadataService::new(&lib, &mut cat)
        .set_rating(id, Rating::new(5).unwrap())
        .unwrap();
    assert!(common::tree_hashes(&lib.sidecars_dir())
        .keys()
        .any(|p| p.to_string_lossy().ends_with(".xmp")));

    // Nothing in the folder was added, changed or removed.
    assert_eq!(common::tree_hashes(&dir), before);
}

#[test]
fn a_library_and_the_cache_inside_the_folder_are_left_out() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = archive(tmp.path());
    // The library was made in the photo folder itself, and the cache too.
    let lib = Library::create(&dir).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let cache = dir.join("cache");
    common::write_file(&cache, "thumbnail/ab/abcd-ef01.jpg", b"thumb");
    // Another library (an empty one made by mistake) in a sub-folder.
    let other = Library::create(&dir.join("February")).unwrap();
    other.open_catalog().unwrap();

    let mut opts = RebuildOptions {
        cache_dirs: vec![cache.clone()],
        ..Default::default()
    };
    let (_, report) = add_folder(&mut cat, &lib, &dir, &mut opts).unwrap();
    assert_eq!(report.assets, 3, "{report:?}");
    assert!(report.keepsake_skipped > 0);
    assert_eq!(
        catalogued(&cat),
        [
            "February/C0001.MP4",
            "January/DSC00001.JPG",
            "January/DSC00002.ARW"
        ]
    );

    // A rating lands in the library's sidecars (inside the folder here);
    // scanning again does not take them for photos, and keeps the rating.
    let id = cat
        .read(|tx| tx.assets().list_ids())
        .unwrap()
        .into_iter()
        .find(|id| {
            cat.read(|tx| tx.assets().require(*id))
                .unwrap()
                .rating
                .get()
                == 0
        })
        .unwrap();
    MetadataService::new(&lib, &mut cat)
        .set_rating(id, Rating::new(5).unwrap())
        .unwrap();
    let mut opts = RebuildOptions {
        cache_dirs: vec![cache],
        ..Default::default()
    };
    let (_, again) = add_folder(&mut cat, &lib, &dir, &mut opts).unwrap();
    assert_eq!(again.assets, 3, "{again:?}");
    assert!(again.problems.is_empty(), "{:?}", again.problems);
    assert_eq!(ratings(&cat), [0, 4, 5]);
}

#[test]
fn scanning_again_finds_new_files_and_keeps_the_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = archive(tmp.path());
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let (first, _) = add_folder(&mut cat, &lib, &dir, &mut RebuildOptions::default()).unwrap();
    common::write_file(&dir, "March/DSC00003.JPG", b"new");
    std::fs::remove_file(dir.join("February/C0001.MP4")).unwrap();
    let (second, report) =
        add_folder(&mut cat, &lib, &dir, &mut RebuildOptions::default()).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(report.assets, 3);
    assert!(catalogued(&cat).contains(&"March/DSC00003.JPG".to_owned()));
    assert!(!catalogued(&cat).contains(&"February/C0001.MP4".to_owned()));
    assert_eq!(lib.volumes().unwrap().len(), 1);
}

#[test]
fn overlapping_folders_are_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = archive(tmp.path());
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add(&mut cat, &lib, &dir);

    let inside = add_folder(
        &mut cat,
        &lib,
        &dir.join("January"),
        &mut RebuildOptions::default(),
    );
    assert!(
        matches!(&inside, Err(LibraryError::Invalid(m)) if m.contains("already in the library")),
        "{inside:?}"
    );
    std::fs::create_dir_all(tmp.path().join("other")).unwrap();
    let parent = add_folder(&mut cat, &lib, tmp.path(), &mut RebuildOptions::default());
    assert!(
        matches!(&parent, Err(LibraryError::Invalid(m)) if m.contains("holds")),
        "{parent:?}"
    );
    // A folder inside the library itself is not a photo folder.
    let in_lib = add_folder(
        &mut cat,
        &lib,
        &lib.sidecars_dir(),
        &mut RebuildOptions::default(),
    );
    assert!(
        matches!(&in_lib, Err(LibraryError::Invalid(m)) if m.contains("inside the library")),
        "{in_lib:?}"
    );
    assert_eq!(lib.volumes().unwrap().len(), 1);
    assert_eq!(cat.read(|tx| tx.assets().count()).unwrap(), 3);
}

#[test]
fn a_stopped_scan_leaves_the_library_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = archive(tmp.path());
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let stop = AtomicBool::new(true);
    let r = add_folder(
        &mut cat,
        &lib,
        &dir,
        &mut RebuildOptions {
            cancel: Some(&stop),
            ..Default::default()
        },
    );
    assert!(matches!(r, Err(LibraryError::Cancelled)), "{r:?}");
    assert!(lib.volumes().unwrap().is_empty());
    assert_eq!(cat.read(|tx| tx.assets().count()).unwrap(), 0);
}

#[test]
fn progress_counts_every_file_read() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = archive(tmp.path());
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let (mut listed, mut read, mut bytes, mut phases) = (None, 0u64, 0u64, Vec::new());
    let mut f = |p: &RebuildProgress<'_>| match *p {
        RebuildProgress::Listed { files, bytes } => listed = Some((files, bytes)),
        RebuildProgress::File { bytes: b, .. } => {
            read += 1;
            bytes += b;
        }
        RebuildProgress::Phase(name) => phases.push(name),
    };
    add_folder(
        &mut cat,
        &lib,
        &dir,
        &mut RebuildOptions {
            progress: Some(&mut f),
            ..Default::default()
        },
    )
    .unwrap();
    // Four files to read (the note too); the Lightroom sidecar is not one.
    let total: u64 = [8u64, 7, 5, 9].iter().sum();
    assert_eq!(listed, Some((4, total)));
    assert_eq!((read, bytes), (4, total));
    assert_eq!(phases, ["Saving"]);
}

#[test]
fn a_write_locked_folder_is_added_rated_and_scanned_again() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = archive(tmp.path());
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let before = common::tree_hashes(&dir);
    // The operating system refuses every write: an attempt fails loudly.
    let guard = common::readonly::deny_writes(&dir);
    let (rec, report) = add_folder(&mut cat, &lib, &dir, &mut RebuildOptions::default()).unwrap();
    assert_eq!(report.assets, 3, "{report:?}");
    let ids = cat.read(|tx| tx.assets().list_ids()).unwrap();
    let mut svc = MetadataService::new(&lib, &mut cat);
    for id in &ids {
        svc.set_rating(*id, Rating::new(3).unwrap()).unwrap();
        svc.set_comment(*id, Some("written without touching the folder"))
            .unwrap();
    }
    let (again, _) = add_folder(&mut cat, &lib, &dir, &mut RebuildOptions::default()).unwrap();
    assert_eq!(again.id, rec.id);
    assert_eq!(ratings(&cat), [3, 3, 3]);
    drop(guard);
    assert_eq!(common::tree_hashes(&dir), before);
}

#[test]
fn photos_in_the_cache_folder_are_added_but_its_thumbnails_are_not() {
    // As reported: the cache folder is set to a photo folder (2026) that
    // also holds the library, and the parent (Pictures) is added.
    let tmp = tempfile::tempdir().unwrap();
    let pictures = tmp.path().join("Pictures");
    archive(&pictures);
    let y2026 = pictures.join("2026");
    common::write_file(&y2026, "September/A7404962.jpg", b"a 2026 photo");
    common::write_file(&y2026, "thumbnail/0f/0f0a-9af6.jpg", b"thumb");
    common::write_file(&y2026, "preview/0f/0f0a-9af6.jpg", b"preview");
    common::write_file(&y2026, "proxy/0f/0f0a-9af6.mp4", b"proxy");
    let lib = Library::create(&y2026).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let mut opts = RebuildOptions {
        cache_dirs: vec![y2026.clone()],
        ..Default::default()
    };
    let (_, report) = add_folder(&mut cat, &lib, &pictures, &mut opts).unwrap();
    assert_eq!(
        catalogued(&cat),
        [
            "2025/February/C0001.MP4",
            "2025/January/DSC00001.JPG",
            "2025/January/DSC00002.ARW",
            "2026/September/A7404962.jpg",
        ],
        "{report:?}"
    );
}

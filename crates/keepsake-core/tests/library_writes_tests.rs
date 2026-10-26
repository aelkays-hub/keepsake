//! Writes that had no test of their own (docs/SAFETY.md): edit recipes, and
//! the clean-up of a backup's own unfinished files.

mod common;

use std::sync::atomic::AtomicBool;

use keepsake_core::catalog::VolumeKind;
use keepsake_core::ingest::destination_volume;
use keepsake_core::ingest::fault::FaultPlan;
use keepsake_core::library::Library;
use keepsake_core::metadata::MetadataService;
use keepsake_core::rebuild::add_volume;

fn files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn recipes_are_written_and_removed_in_the_library_only() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let media = tmp.path().join("media");
    common::copy_dir(&common::fixture("dcim"), &media);
    let mut cat = lib.open_catalog().unwrap();
    add_volume(
        &mut cat,
        &lib,
        "M",
        VolumeKind::External,
        &media,
        false,
        None,
    )
    .unwrap();
    let id = cat.read(|tx| tx.assets().list_ids()).unwrap()[0];
    let before = common::tree_hashes(&media);

    let mut svc = MetadataService::new(&lib, &mut cat);
    let r = svc
        .add_recipe(id, "crop", 1, &serde_json::json!({ "x": 0.1, "w": 0.5 }))
        .unwrap();
    let path = r.rel_path.to_fs_path(&lib.recipes_dir());
    assert!(path.is_file());
    assert!(std::fs::read_to_string(&path).unwrap().contains("0.5"));
    let r = svc
        .update_recipe(r.id, &serde_json::json!({ "x": 0.2, "w": 0.4 }))
        .unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("0.4"));
    svc.remove_recipe(r.id).unwrap();
    assert!(!path.exists());
    svc.remove_recipe(r.id).unwrap(); // already gone: fine
    drop(svc);
    // The recipe lives in the library; the media folder only gains the
    // XMP sidecar that records it (beside the original, never inside it).
    let after = common::tree_hashes(&media);
    for (p, h) in &before {
        assert_eq!(after.get(p), Some(h), "{} changed", p.display());
    }
    assert!(after
        .keys()
        .filter(|p| !before.contains_key(*p))
        .all(|p| p.to_string_lossy().ends_with(".xmp")));
}

#[test]
fn a_backup_removes_only_its_own_unfinished_files() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let media = tmp.path().join("media");
    common::copy_dir(&common::fixture("dcim"), &media);
    let mut cat = lib.open_catalog().unwrap();
    add_volume(
        &mut cat,
        &lib,
        "M",
        VolumeKind::External,
        &media,
        false,
        None,
    )
    .unwrap();
    let dest =
        destination_volume(&lib, &tmp.path().join("backup"), "B", VolumeKind::External).unwrap();
    let dir = dest.mount.join("old");
    std::fs::create_dir_all(&dir).unwrap();
    // A backup that was stopped hard left this behind…
    std::fs::write(dir.join(".DSC00001.JPG.keepsake-partial-backup"), b"half").unwrap();
    // …and these are not the backup's: an import's temporary and a user file.
    std::fs::write(dir.join(".DSC00002.JPG.keepsake-partial-0123abcd"), b"half").unwrap();
    std::fs::write(dir.join("notes.txt"), b"mine").unwrap();
    let rep = keepsake_core::backup::run(
        &mut cat,
        &lib,
        &dest,
        &FaultPlan::none(),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(rep.stale_temporaries_removed, 1, "{rep:?}");
    assert!(!dir.join(".DSC00001.JPG.keepsake-partial-backup").exists());
    assert!(dir.join(".DSC00002.JPG.keepsake-partial-0123abcd").exists());
    assert!(dir.join("notes.txt").exists());
    assert!(files_under(&dest.mount)
        .iter()
        .any(|p| p.ends_with("DSC00001.JPG")));
}

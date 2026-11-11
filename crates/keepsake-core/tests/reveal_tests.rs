//! Where an item's original is, for "Show in Explorer / Finder".

mod common;

use std::path::{Path, PathBuf};

use keepsake_core::catalog::{AssetId, Catalog, VolumeKind};
use keepsake_core::library::Library;
use keepsake_core::rebuild::add_volume;
use keepsake_core::reveal::{original_location, OriginalLocation};

struct World {
    _tmp: tempfile::TempDir,
    lib: Library,
    cat: Catalog,
    media: PathBuf,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let media = tmp.path().join("media");
    common::copy_dir(&common::fixture("dcim"), &media);
    let mut cat = lib.open_catalog().unwrap();
    add_volume(
        &mut cat,
        &lib,
        "Card A",
        VolumeKind::External,
        &media,
        false,
        None,
    )
    .unwrap();
    World {
        _tmp: tmp,
        lib,
        cat,
        media: std::fs::canonicalize(media).unwrap(),
    }
}

fn first_asset(cat: &Catalog) -> AssetId {
    cat.read(|tx| tx.assets().list_ids()).unwrap()[0]
}

fn bytes(p: &Path) -> Vec<u8> {
    std::fs::read(p).unwrap()
}

#[test]
fn online_original_resolves_to_an_absolute_path_on_its_volume() {
    let w = world();
    let a = first_asset(&w.cat);
    let OriginalLocation::Online(p) = original_location(&w.cat, &w.lib, a).unwrap() else {
        panic!("expected the original to be online");
    };
    assert!(p.is_absolute(), "{}", p.display());
    assert!(p.is_file(), "{}", p.display());
    assert!(
        std::fs::canonicalize(&p).unwrap().starts_with(&w.media),
        "{} is not under {}",
        p.display(),
        w.media.display()
    );
    // It is the file the catalog holds for the item.
    let files = w.cat.read(|tx| tx.files().for_asset(a)).unwrap();
    assert!(files
        .iter()
        .any(|f| p.ends_with(f.rel_path.to_fs_path(Path::new("")))));
}

#[test]
fn offline_volume_names_the_drive() {
    let mut w = world();
    let a = first_asset(&w.cat);
    let OriginalLocation::Online(p) = original_location(&w.cat, &w.lib, a).unwrap() else {
        panic!("expected online first");
    };
    let before = bytes(&p);
    // Unplug: the folder is gone and the catalog says the drive is offline.
    let away = w.media.with_file_name("unplugged");
    std::fs::rename(&w.media, &away).unwrap();
    let vols = w.cat.read(|tx| tx.volumes().list()).unwrap();
    w.cat
        .write(|tx| tx.volumes().set_mount(vols[0].id, None, 0))
        .unwrap();
    assert_eq!(
        original_location(&w.cat, &w.lib, a).unwrap(),
        OriginalLocation::Offline {
            volume: "Card A".into()
        }
    );
    // Plugged back in at the same place: online again, the file untouched.
    std::fs::rename(&away, &w.media).unwrap();
    let OriginalLocation::Online(p2) = original_location(&w.cat, &w.lib, a).unwrap() else {
        panic!("expected online again");
    };
    assert_eq!(bytes(&p2), before);
}

#[test]
fn file_gone_from_a_connected_drive_is_missing() {
    let w = world();
    let a = first_asset(&w.cat);
    let OriginalLocation::Online(p) = original_location(&w.cat, &w.lib, a).unwrap() else {
        panic!("expected online first");
    };
    std::fs::remove_file(&p).unwrap();
    assert_eq!(
        original_location(&w.cat, &w.lib, a).unwrap(),
        OriginalLocation::Missing(p)
    );
}

#[test]
fn unknown_item_is_an_error() {
    let w = world();
    assert!(original_location(&w.cat, &w.lib, AssetId::new_random()).is_err());
}

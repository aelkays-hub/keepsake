//! The folder tree (`Browse::folders`) and the folder filter
//! (`SearchQuery::folder`), on a folder added in place.

mod common;

use keepsake_core::catalog::{MediaKind, SearchQuery, VolumeId};
use keepsake_core::library::Library;
use keepsake_core::rebuild::{add_folder, RebuildOptions};

struct World {
    _tmp: tempfile::TempDir,
    lib: Library,
    cat: keepsake_core::catalog::Catalog,
    vol: VolumeId,
}

/// Pictures/
///   2025/January/  2 photos (one in a sub-folder "Edits")
///   2025/May/      1 photo, 1 video
///   2026/          1 photo directly in it
///   Edited JPEG/   1 photo
///   top.jpg        directly in the folder
fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let pics = tmp.path().join("Pictures");
    for (rel, bytes) in [
        ("2025/January/DSC00001.JPG", &b"a"[..]),
        ("2025/January/Edits/DSC00001-edit.JPG", b"b"),
        ("2025/May/DSC00002.JPG", b"c"),
        ("2025/May/C0001.MP4", b"d"),
        ("2026/A7400387.jpg", b"e"),
        ("Edited JPEG/A7404962.jpg", b"f"),
        ("top.jpg", b"g"),
    ] {
        common::write_file(&pics, rel, bytes);
    }
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let (rec, _) = add_folder(&mut cat, &lib, &pics, &mut RebuildOptions::default()).unwrap();
    World {
        _tmp: tmp,
        lib,
        cat,
        vol: rec.id,
    }
}

fn tree(w: &World, parent: Option<&str>, q: &SearchQuery) -> Vec<(String, String, u64, bool)> {
    w.cat
        .read(|tx| tx.browse().folders(w.vol, parent, q))
        .unwrap()
        .into_iter()
        .map(|b| (b.name, b.path, b.count, b.has_children))
        .collect()
}

fn count_in(w: &World, folder: &str) -> u64 {
    w.cat
        .read(|tx| {
            tx.browse().count(&SearchQuery {
                volume: Some(w.vol),
                folder: Some(folder.into()),
                ..Default::default()
            })
        })
        .unwrap()
}

#[test]
fn the_tree_lists_sub_folders_with_counts_of_everything_below() {
    let w = world();
    let all = SearchQuery::default();
    let s = |a: &str| a.to_owned();
    assert_eq!(
        tree(&w, None, &all),
        [
            (s("2025"), s("2025"), 4, true),
            (s("2026"), s("2026"), 1, false),
            (s("Edited JPEG"), s("Edited JPEG"), 1, false),
        ],
        "top.jpg is in no sub-folder"
    );
    assert_eq!(
        tree(&w, Some("2025"), &all),
        [
            (s("January"), s("2025/January"), 2, true),
            (s("May"), s("2025/May"), 2, false),
        ]
    );
    assert_eq!(
        tree(&w, Some("2025/January/"), &all),
        [(s("Edits"), s("2025/January/Edits"), 1, false)],
        "a trailing slash is fine"
    );
    assert!(tree(&w, Some("2026"), &all).is_empty());
    assert!(tree(&w, Some("nowhere"), &all).is_empty());
    // Filters apply to the counts; empty folders drop out.
    let videos = SearchQuery {
        kind: Some(MediaKind::Video),
        ..Default::default()
    };
    assert_eq!(tree(&w, None, &videos), [(s("2025"), s("2025"), 1, true)]);
    assert_eq!(
        tree(&w, Some("2025"), &videos),
        [(s("May"), s("2025/May"), 1, false)]
    );
    let _ = &w.lib;
}

#[test]
fn the_folder_filter_takes_the_folder_and_everything_below() {
    let w = world();
    assert_eq!(count_in(&w, "2025"), 4);
    assert_eq!(count_in(&w, "2025/January"), 2);
    assert_eq!(count_in(&w, "2025/January/Edits"), 1);
    assert_eq!(count_in(&w, "/2025/May/"), 2, "slashes trimmed");
    assert_eq!(count_in(&w, "Edited JPEG"), 1);
    // `2025` must not match a sibling such as `2025-old`.
    assert_eq!(count_in(&w, "202"), 0);
    let no_volume = w.cat.read(|tx| {
        tx.browse().count(&SearchQuery {
            folder: Some("2025".into()),
            ..Default::default()
        })
    });
    assert!(no_volume.is_err());
}

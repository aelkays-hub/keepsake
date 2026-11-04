//! Importing a card into a folder added in place, by year / month name /
//! day: the folders are made as needed, the folder keeps its identity and
//! stays read-only for sidecars, and what was there is left alone.

mod common;

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use common::media;
use keepsake_core::catalog::{SearchQuery, VolumeKind};
use keepsake_core::ingest::template::{Template, YEAR_MONTH_DAY_TEMPLATE};
use keepsake_core::ingest::{
    self, destination_volume, IngestError, PlanOptions, RunContext, Selection,
};
use keepsake_core::library::Library;
use keepsake_core::probe::{ExifTool, Prober};
use keepsake_core::rebuild::{add_folder, RebuildOptions};

#[test]
fn a_card_lands_in_year_month_day_folders_of_the_added_folder() {
    let Some((tools, ffmpeg)) = media::tools() else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    // As on the user's drive: Pictures/<year>/<Month>/<day>, with the
    // library (and its cache) inside Pictures/2026.
    let pics = tmp.path().join("Pictures");
    let old = common::write_file(&pics, "2026/October/2026-10-01/OLD.JPG", b"already here");
    let y2026 = pics.join("2026");
    let lib = Library::create(&y2026).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let (folder, _) = add_folder(
        &mut cat,
        &lib,
        &pics,
        &mut RebuildOptions {
            cache_dirs: vec![y2026.clone()],
            ..Default::default()
        },
    )
    .unwrap();
    let old_bytes = std::fs::read(old.to_fs_path(&pics)).unwrap();

    // A card with two photos from different days.
    let card = tmp.path().join("card");
    let et = ExifTool::start(&tools.exiftool, Duration::from_secs(30)).unwrap();
    for (name, when) in [
        ("DSC00001.JPG", "2026:10:30 14:00:00"),
        ("DSC00002.JPG", "2025:12:31 23:30:00"),
    ] {
        let f = card.join("DCIM/100MSDCF").join(name);
        media::jpeg(&ffmpeg, &f);
        media::set_tags(&et, &f, &[&format!("-DateTimeOriginal={when}")]);
    }

    // The destination is the added folder itself: same volume, still
    // read-only for sidecars.
    let dest = destination_volume(&lib, &pics, "Card import", VolumeKind::External).unwrap();
    assert_eq!(dest.volume.id, folder.id);
    assert_eq!(dest.volume.label, "Pictures");
    assert!(dest.volume.read_only);
    assert_eq!(lib.volumes().unwrap().len(), 1);

    let prober = Prober::new(&tools, Default::default()).unwrap();
    let opts = PlanOptions {
        template: Template::parse(YEAR_MONTH_DAY_TEMPLATE).unwrap(),
        selection: Selection::NewOnly,
        event: None,
        allow_same_filesystem: true,
        source_id: None,
    };
    let s = ingest::plan(&mut cat, &lib, Some(&prober), &card, "Card", &[dest], &opts).unwrap();
    let cancel = AtomicBool::new(false);
    let mut progress = |_: &ingest::Progress| {};
    let fault = ingest::fault::FaultPlan::none();
    let mut ctx = RunContext {
        prober: Some(&prober),
        fault: &fault,
        cancel: &cancel,
        progress: &mut progress,
        retry_failed: true,
    };
    ingest::run(&mut cat, &lib, s, &mut ctx).unwrap();

    // The folders were made, and the photos are where the layout says.
    let a = pics.join("2026/October/2026-10-30/DSC00001.JPG");
    let b = pics.join("2025/December/2025-12-31/DSC00002.JPG");
    assert!(a.is_file(), "{a:?}");
    assert!(b.is_file(), "{b:?}");
    // No sidecar next to them: the folder stays read-only for sidecars.
    for d in [a.parent().unwrap(), b.parent().unwrap()] {
        let names: Vec<String> = std::fs::read_dir(d)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().all(|n| !n.ends_with(".xmp")), "{names:?}");
    }
    // What was there is untouched.
    assert_eq!(std::fs::read(old.to_fs_path(&pics)).unwrap(), old_bytes);

    // Catalogued in the folder: the tree and the calendar see them.
    let all = SearchQuery::default();
    let tree: Vec<(String, u64)> = cat
        .read(|tx| tx.browse().folders(folder.id, Some("2026"), &all))
        .unwrap()
        .into_iter()
        .map(|b| (b.path, b.count))
        .collect();
    assert_eq!(tree, [("2026/October".to_owned(), 2)]);
    let in_volume = cat
        .read(|tx| {
            tx.browse().count(&SearchQuery {
                volume: Some(folder.id),
                ..Default::default()
            })
        })
        .unwrap();
    assert_eq!(in_volume, 3);

    // The same card again: nothing new to copy.
    let dest = destination_volume(&lib, &pics, "Card import", VolumeKind::External).unwrap();
    let p = ingest::preview(&cat, &lib, Some(&prober), &card, &[dest], &opts).unwrap();
    assert_eq!(p.assets.len(), 2);
    assert!(
        p.assets
            .iter()
            .all(|x| x.diff == keepsake_core::catalog::ImportDiff::AlreadyImported),
        "{p:?}"
    );
}

#[test]
fn a_destination_inside_an_added_folder_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let pics = tmp.path().join("Pictures");
    common::write_file(&pics, "2025/May/a.jpg", b"a");
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add_folder(&mut cat, &lib, &pics, &mut RebuildOptions::default()).unwrap();
    for d in [pics.join("2025"), tmp.path().to_path_buf()] {
        let r = destination_volume(&lib, &d, "X", VolumeKind::External);
        assert!(
            matches!(&r, Err(IngestError::Unsafe(m)) if m.contains("overlaps")),
            "{d:?}: {:?}",
            r.as_ref().err()
        );
    }
    assert_eq!(lib.volumes().unwrap().len(), 1);
}

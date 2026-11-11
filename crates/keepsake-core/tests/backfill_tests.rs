//! Thumbnail backfill: batches, the saved position, and passing bad files.
//!
//! The media are small JPEGs made in-process, so no external tool is needed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use image::{DynamicImage, RgbImage};
use keepsake_core::catalog::{AssetId, Catalog, CatalogError, VolumeKind};
use keepsake_core::library::Library;
use keepsake_core::rebuild::add_volume;
use keepsake_core::render::backfill::{
    self, render_item, Backfill, BackfillEvent, BackfillOptions, BackfillReport, ItemRenderer,
    THUMBS,
};
use keepsake_core::render::source::source_hash;
use keepsake_core::render::{Cache, RenderError, RenderTools};

struct World {
    _tmp: tempfile::TempDir,
    lib: Library,
    cat: Catalog,
    media: PathBuf,
    cache: Cache,
}

fn jpeg(i: usize) -> Vec<u8> {
    // Noise seeded by `i`, so every file (and its hash) is different.
    let mut img = RgbImage::new(64, 48);
    let mut x = (i as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    for p in img.pixels_mut() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let b = x.to_le_bytes();
        *p = image::Rgb([b[0], b[1], b[2]]);
    }
    let mut out = Vec::new();
    DynamicImage::ImageRgb8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut out),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    out
}

impl World {
    /// A library of `n` photos; the names in `bad` hold no picture.
    fn new(n: usize, bad: &[usize]) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let lib = Library::create(&root.join("lib")).unwrap();
        let mut cat = lib.open_catalog().unwrap();
        let media = root.join("media");
        std::fs::create_dir_all(&media).unwrap();
        for i in 0..n {
            let bytes = if bad.contains(&i) {
                format!("not a picture {i}").into_bytes()
            } else {
                jpeg(i)
            };
            std::fs::write(media.join(format!("IMG_{i:04}.JPG")), bytes).unwrap();
        }
        add_volume(
            &mut cat,
            &lib,
            "Media",
            VolumeKind::External,
            &media,
            false,
            None,
        )
        .unwrap();
        assert_eq!(cat.read(|t| t.assets().count()).unwrap(), n as u64);
        let cache = Cache::new(root.join("cache"));
        Self {
            _tmp: tmp,
            lib,
            cat,
            media,
            cache,
        }
    }

    /// Assets in walk order (`seq`).
    fn walk(&self) -> Vec<(i64, AssetId)> {
        self.cat
            .read(|t| t.assets().page_after(0, usize::MAX))
            .unwrap()
    }

    fn backfill(&self, opts: BackfillOptions) -> Backfill {
        Backfill::new(&self.lib, self.cache.clone(), no_tools(), opts)
    }

    fn has_thumbs(&self, asset: AssetId) -> bool {
        let hash = source_hash(&self.cat, asset).unwrap().unwrap();
        THUMBS
            .iter()
            .all(|v| self.cache.has(&self.cat, &hash, *v).unwrap())
    }

    fn cursor(&self) -> i64 {
        backfill::cursor(&self.cat).unwrap()
    }

    /// The real renderer, counting the calls per asset.
    fn counting(&self, calls: &Arc<Mutex<HashMap<AssetId, u32>>>) -> ItemRenderer {
        let (lib, cache, calls) = (self.lib.clone(), self.cache.clone(), calls.clone());
        Arc::new(move |cat, asset| {
            *calls.lock().unwrap().entry(asset).or_default() += 1;
            render_item(cat, &lib, &cache, &no_tools(), None, asset)
        })
    }
}

/// JPEGs need no tool; anything that would is reported as a failure.
fn no_tools() -> RenderTools {
    let none = Path::new("keepsake-test-no-tool");
    RenderTools {
        exiftool: none.into(),
        ffprobe: none.into(),
        ffmpeg: none.into(),
        dcraw: none.into(),
    }
}

fn opts(workers: usize) -> BackfillOptions {
    BackfillOptions {
        workers,
        ..BackfillOptions::default()
    }
}

fn run(b: &Backfill) -> (BackfillReport, Vec<BackfillEvent>) {
    let cancel = AtomicBool::new(false);
    let mut evs = Vec::new();
    let rep = b.run(&cancel, &mut |e| evs.push(e.clone())).unwrap();
    (rep, evs)
}

fn committed(evs: &[BackfillEvent]) -> Vec<u64> {
    evs.iter()
        .filter_map(|e| match e {
            BackfillEvent::BatchCommitted { stats, .. } => Some(stats.items),
            _ => None,
        })
        .collect()
}

#[test]
fn makes_thumbnails_in_batches_of_100_saving_the_position_after_each() {
    let w = World::new(250, &[]);
    let walk = w.walk();
    let lib = w.lib.clone();
    let cancel = AtomicBool::new(false);
    let mut saved = Vec::new();
    let rep = w
        .backfill(opts(3))
        .run(&cancel, &mut |e| {
            if let BackfillEvent::BatchCommitted { stats, .. } = e {
                // Seen from another connection: the batch is committed.
                let other = lib.open_catalog().unwrap();
                saved.push((stats.items, backfill::cursor(&other).unwrap()));
            }
        })
        .unwrap();
    assert_eq!(
        saved,
        vec![(100, walk[99].0), (100, walk[199].0), (50, walk[249].0)]
    );
    assert!(rep.complete && !rep.cancelled);
    assert_eq!(
        (rep.batches, rep.totals.items, rep.totals.made),
        (3, 250, 250)
    );
    assert!(walk.iter().all(|(_, a)| w.has_thumbs(*a)));
    // Only the thumbnails, not the preview.
    let (files, _) = w.cat.read(|t| t.renditions().totals()).unwrap();
    assert_eq!(files, 500);

    // Nothing left: a second run finds the end straight away.
    let (rep, evs) = run(&w.backfill(opts(1)));
    assert!(rep.complete);
    assert_eq!(rep.batches, 0);
    assert_eq!(
        evs[0],
        BackfillEvent::Started {
            from_seq: walk[249].0,
            remaining: 0
        }
    );
}

#[test]
fn a_stopped_run_resumes_from_the_saved_position() {
    let w = World::new(230, &[]);
    let walk = w.walk();
    let calls = Arc::new(Mutex::new(HashMap::new()));
    let first = w
        .backfill(BackfillOptions {
            max_batches: Some(1),
            ..opts(2)
        })
        .with_renderer(w.counting(&calls));
    let (rep, _) = run(&first);
    assert!(!rep.complete);
    assert_eq!(rep.batches, 1);
    assert_eq!(w.cursor(), walk[99].0);
    assert!(w.has_thumbs(walk[99].1));
    assert!(!w.has_thumbs(walk[100].1));

    let second = w.backfill(opts(2)).with_renderer(w.counting(&calls));
    let (rep, evs) = run(&second);
    assert_eq!(
        evs[0],
        BackfillEvent::Started {
            from_seq: walk[99].0,
            remaining: 130
        }
    );
    assert_eq!(committed(&evs), vec![100, 30]);
    assert!(rep.complete);
    assert_eq!(w.cursor(), walk[229].0);
    // Every asset was rendered exactly once across the two runs.
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 230);
    assert!(calls.values().all(|n| *n == 1));
    assert!(walk.iter().all(|(_, a)| w.has_thumbs(*a)));
}

#[test]
fn a_crash_mid_batch_loses_only_that_batch() {
    let w = World::new(180, &[]);
    let walk = w.walk();
    let (lib, cache) = (w.lib.clone(), w.cache.clone());
    let n = Arc::new(AtomicUsize::new(0));
    let crashing: ItemRenderer = Arc::new(move |cat, asset| {
        // The 150th asset: the catalog fails (as if the process died).
        if n.fetch_add(1, Ordering::SeqCst) == 149 {
            return Err(RenderError::Catalog(CatalogError::Invalid(
                "simulated crash".into(),
            )));
        }
        render_item(cat, &lib, &cache, &no_tools(), None, asset)
    });
    let b = w.backfill(opts(1)).with_renderer(crashing);
    let cancel = AtomicBool::new(false);
    assert!(b.run(&cancel, &mut |_| {}).is_err());
    // The first batch stays; the second was never recorded.
    assert_eq!(w.cursor(), walk[99].0);
    assert!(w.has_thumbs(walk[99].1));
    assert!(!w.has_thumbs(walk[100].1));
    assert!(!w.has_thumbs(walk[148].1));

    let (rep, evs) = run(&w.backfill(opts(2)));
    assert_eq!(committed(&evs), vec![80]);
    assert!(rep.complete);
    assert_eq!(rep.totals.made, 80);
    assert!(walk.iter().all(|(_, a)| w.has_thumbs(*a)));
}

#[test]
fn a_bad_file_is_reported_and_passed() {
    let w = World::new(120, &[5, 110]);
    let walk = w.walk();
    let bad: Vec<AssetId> = ["IMG_0005.JPG", "IMG_0110.JPG"]
        .iter()
        .map(|n| {
            let p = std::fs::canonicalize(w.media.join(n)).unwrap();
            w.lib.locate(&w.cat, &p).unwrap().unwrap().asset_id
        })
        .collect();
    let (rep, evs) = run(&w.backfill(opts(2)));
    assert!(rep.complete);
    assert_eq!(rep.totals.failed, 2);
    assert_eq!(rep.totals.made, 118);
    assert_eq!(w.cursor(), walk[119].0);
    let mut failed: Vec<AssetId> = evs
        .iter()
        .filter_map(|e| match e {
            BackfillEvent::ItemFailed { asset, error, .. } => {
                assert!(!error.is_empty());
                Some(*asset)
            }
            _ => None,
        })
        .collect();
    failed.sort();
    let mut want = bad.clone();
    want.sort();
    assert_eq!(failed, want);
    for (_, a) in &walk {
        assert_eq!(w.has_thumbs(*a), !bad.contains(a));
    }

    // A restart walks everything again: the bad files fail again, the rest
    // are passed without rendering.
    let (rep, _) = run(&w.backfill(BackfillOptions {
        restart: true,
        ..opts(2)
    }));
    assert_eq!(
        (rep.totals.items, rep.totals.already, rep.totals.failed),
        (120, 118, 2)
    );
}

#[test]
fn a_renderer_crash_on_one_file_does_not_stop_the_run() {
    let w = World::new(10, &[]);
    let walk = w.walk();
    let victim = walk[3].1;
    let (lib, cache) = (w.lib.clone(), w.cache.clone());
    let r: ItemRenderer = Arc::new(move |cat, asset| {
        assert!(asset != victim, "decoder\nblew up");
        render_item(cat, &lib, &cache, &no_tools(), None, asset)
    });
    let (rep, evs) = run(&w.backfill(opts(1)).with_renderer(r));
    assert!(rep.complete);
    assert_eq!((rep.totals.made, rep.totals.failed), (9, 1));
    assert!(evs.iter().any(|e| matches!(
        e,
        BackfillEvent::ItemFailed { asset, error, .. }
            if *asset == victim && error.contains("decoder | blew up") && !error.contains('\n')
    )));
}

#[test]
fn a_batch_that_is_all_offline_stops_without_moving_past_it() {
    let w = World::new(150, &[]);
    let walk = w.walk();
    run(&w.backfill(BackfillOptions {
        max_batches: Some(1),
        ..opts(1)
    }));
    assert_eq!(w.cursor(), walk[99].0);
    // The share goes away.
    let away = w.media.with_file_name("media-away");
    std::fs::rename(&w.media, &away).unwrap();
    let (rep, evs) = run(&w.backfill(opts(2)));
    assert!(rep.stopped_offline && !rep.complete);
    assert_eq!(rep.batches, 0);
    assert!(evs.contains(&BackfillEvent::StoppedOffline {
        cursor: walk[99].0,
        items: 50
    }));
    assert_eq!(w.cursor(), walk[99].0);

    // It is back: the run carries on from there.
    std::fs::rename(&away, &w.media).unwrap();
    let (rep, _) = run(&w.backfill(opts(2)));
    assert!(rep.complete);
    assert_eq!(rep.totals.made, 50);
}

#[test]
fn skip_offline_moves_past_offline_media() {
    let w = World::new(30, &[]);
    let away = w.media.with_file_name("media-away");
    std::fs::rename(&w.media, &away).unwrap();
    let (rep, _) = run(&w.backfill(BackfillOptions {
        skip_offline: true,
        ..opts(1)
    }));
    assert!(rep.complete);
    assert_eq!(rep.totals.offline, 30);
    assert_eq!(w.cursor(), w.walk()[29].0);
}

#[test]
fn cancelling_saves_the_finished_part_of_the_batch() {
    let w = World::new(100, &[]);
    let walk = w.walk();
    let cancel = Arc::new(AtomicBool::new(false));
    let (lib, cache, c) = (w.lib.clone(), w.cache.clone(), cancel.clone());
    let n = Arc::new(AtomicUsize::new(0));
    let r: ItemRenderer = Arc::new(move |cat, asset| {
        if n.fetch_add(1, Ordering::SeqCst) == 29 {
            c.store(true, Ordering::SeqCst); // Ctrl-C during the 30th
        }
        render_item(cat, &lib, &cache, &no_tools(), None, asset)
    });
    let b = w.backfill(opts(1)).with_renderer(r);
    let rep = b.run(&cancel, &mut |_| {}).unwrap();
    assert!(rep.cancelled && !rep.complete);
    assert_eq!(rep.totals.items, 30);
    assert_eq!(w.cursor(), walk[29].0);
    assert!(w.has_thumbs(walk[29].1));
    assert!(!w.has_thumbs(walk[30].1));
}

#[test]
fn assets_added_after_a_finished_walk_are_picked_up_next_time() {
    let mut w = World::new(5, &[]);
    let (rep, _) = run(&w.backfill(opts(1)));
    assert!(rep.complete);
    // A new folder is added to the library.
    let more = w.media.with_file_name("more");
    std::fs::create_dir_all(&more).unwrap();
    for i in 5..8 {
        std::fs::write(more.join(format!("IMG_{i:04}.JPG")), jpeg(i)).unwrap();
    }
    add_volume(
        &mut w.cat,
        &w.lib,
        "More",
        VolumeKind::External,
        &more,
        false,
        None,
    )
    .unwrap();
    assert_eq!(w.cat.read(|t| t.assets().count()).unwrap(), 8);
    let (rep, _) = run(&w.backfill(opts(1)));
    assert_eq!((rep.totals.items, rep.totals.made), (3, 3));
}

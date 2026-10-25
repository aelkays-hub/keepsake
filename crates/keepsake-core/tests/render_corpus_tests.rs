//! Renditions of real Sony media from `$KEEPSAKE_CORPUS/library-sample`.
//! Ignored by default; run with `cargo test -- --include-ignored corpus`.
//! A few files are copied to a temp folder: the corpus is only read.

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use keepsake_core::catalog::{JobState, VolumeKind};
use keepsake_core::library::Library;
use keepsake_core::rebuild::add_volume;
use keepsake_core::render::{
    self, Cache, RenderEvent, RenderOptions, RenderService, RenderTools, Variant,
};

fn sample() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("KEEPSAKE_CORPUS")?).join("library-sample");
    p.is_dir().then_some(p)
}

/// The `n` smallest files with extension `ext` (case-insensitive) of at
/// least `min_bytes` (skips thumbnails and exports).
fn smallest(root: &Path, ext: &str, n: usize, min_bytes: u64) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .is_some_and(|x| x.to_string_lossy().eq_ignore_ascii_case(ext))
            {
                let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                if len >= min_bytes {
                    found.push((len, p));
                }
            }
        }
    }
    found.sort();
    found.into_iter().take(n).map(|(_, p)| p).collect()
}

#[test]
#[ignore = "needs KEEPSAKE_CORPUS"]
fn corpus_arw_and_avchd_render() {
    let Some(sample) = sample() else {
        eprintln!("skipping: KEEPSAKE_CORPUS/library-sample not found");
        return;
    };
    let Some((t, ffmpeg)) = common::media::tools() else {
        return;
    };
    let tools = RenderTools {
        exiftool: t.exiftool,
        ffprobe: t.ffprobe,
        ffmpeg,
        dcraw: common::workspace_root()
            .join("vendor")
            .join("libraw")
            .join(if cfg!(windows) {
                "dcraw_emu.exe"
            } else {
                "dcraw_emu"
            }),
    };
    let tmp = tempfile::tempdir().unwrap();
    let media = tmp.path().join("media");
    let picks: Vec<PathBuf> = smallest(&sample, "ARW", 3, 1 << 20)
        .into_iter()
        .chain(smallest(&sample, "M2TS", 1, 1 << 20))
        .chain(smallest(&sample, "JPG", 2, 2 << 20))
        .collect();
    assert!(
        picks.len() >= 4,
        "library-sample has ARW, M2TS and JPG files"
    );
    for (i, p) in picks.iter().enumerate() {
        let name = format!("{i:02}_{}", p.file_name().unwrap().to_string_lossy());
        std::fs::create_dir_all(&media).unwrap();
        std::fs::copy(p, media.join(name)).unwrap();
    }
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add_volume(
        &mut cat,
        &lib,
        "Sample",
        VolumeKind::External,
        &media,
        false,
        None,
    )
    .unwrap();
    render::enqueue_backlog(&mut cat).unwrap();
    let cache = Cache::new(tmp.path().join("cache"));
    let svc = RenderService::new(
        &lib,
        tools.clone(),
        cache.clone(),
        RenderOptions {
            workers: 2,
            proxy_workers: 1,
            max_bytes: u64::MAX,
            pause_on_battery: false,
            until_empty: true,
            forced_encoder: None,
            evict_every: 100,
        },
    );
    let mut evs = Vec::new();
    svc.run(&AtomicBool::new(false), &mut |e| evs.push(e.clone()))
        .unwrap();
    let failed: Vec<_> = evs
        .iter()
        .filter(|e| matches!(e, RenderEvent::Failed { .. }))
        .collect();
    assert!(failed.is_empty(), "{failed:#?}");
    let c = cat.read(|tx| tx.jobs().counts()).unwrap();
    assert_eq!(
        c.total(JobState::Queued) + c.total(JobState::Failed),
        0,
        "{c:?}"
    );
    for id in cat.read(|tx| tx.assets().list_ids()).unwrap() {
        let p = cache
            .lookup(&mut cat, id, Variant::Preview2560)
            .unwrap()
            .unwrap();
        let (w, h) = image::image_dimensions(&p).unwrap();
        assert!(w.max(h) >= 1000 && w.max(h) <= 2560, "{p:?}: {w}x{h}");
        let t = cache
            .lookup(&mut cat, id, Variant::Thumb256)
            .unwrap()
            .unwrap();
        assert_eq!(
            image::image_dimensions(&t)
                .unwrap()
                .0
                .max(image::image_dimensions(&t).unwrap().1),
            256
        );
    }
    let video = cat
        .read(|tx| tx.assets().list_ids())
        .unwrap()
        .into_iter()
        .find(|id| cache.lookup_proxy(&mut cat, *id).unwrap().is_some())
        .expect("the M2TS got a proxy");
    let proxy = cache.lookup_proxy(&mut cat, video).unwrap().unwrap();
    let m = keepsake_core::probe::ffprobe::probe(
        &tools.ffprobe,
        &proxy,
        std::time::Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(m.video.unwrap().codec, "h264");
    assert_eq!(
        m.audio.first().map(|a| a.codec.as_str()),
        Some("aac"),
        "AC-3 is transcoded"
    );
}

#[test]
#[ignore = "needs KEEPSAKE_CORPUS"]
fn corpus_arw_full_size_via_libraw() {
    use keepsake_core::render::full::{full_image, Member};
    let Some(sample) = sample() else {
        eprintln!("skipping: KEEPSAKE_CORPUS/library-sample not found");
        return;
    };
    let Some((t, ffmpeg)) = common::media::tools() else {
        return;
    };
    let dcraw = common::workspace_root()
        .join("vendor")
        .join("libraw")
        .join(if cfg!(windows) {
            "dcraw_emu.exe"
        } else {
            "dcraw_emu"
        });
    if !dcraw.is_file() {
        eprintln!("skipping: run scripts/setup for LibRaw");
        return;
    }
    let tools = RenderTools {
        exiftool: t.exiftool,
        ffprobe: t.ffprobe,
        ffmpeg,
        dcraw,
    };
    let arw = smallest(&sample, "ARW", 1, 1 << 20);
    let arw = arw.first().expect("library-sample has an ARW");
    let tmp = tempfile::tempdir().unwrap();
    let media = tmp.path().join("media");
    std::fs::create_dir_all(&media).unwrap();
    std::fs::copy(arw, media.join("DSC1.ARW")).unwrap();
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    add_volume(
        &mut cat,
        &lib,
        "S",
        VolumeKind::External,
        &media,
        false,
        None,
    )
    .unwrap();
    let id = cat.read(|tx| tx.assets().list_ids()).unwrap()[0];
    let cache = Cache::new(tmp.path().join("cache"));
    let p = full_image(&mut cat, &lib, &cache, &tools, id, Member::Primary).unwrap();
    let (w, h) = image::image_dimensions(&p).unwrap();
    assert!(
        w.max(h) > 2560,
        "full size, not the embedded preview: {w}x{h}"
    );
}

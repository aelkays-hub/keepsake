//! No code path writes to a source: a camera card being imported, or a
//! read-only library volume (an archive disc, a locked drive).
//!
//! The operating system is told to refuse every write to the source
//! (`common::readonly::deny_writes`: a deny ACL on Windows, `chmod a-w`
//! elsewhere), so an attempted write fails instead of slipping through.
//! Then everything that reads a source runs, and afterwards every file's
//! bytes, size and modification time, and the folder listing, are compared
//! with before. docs/SAFETY.md lists the paths covered.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use keepsake_core::catalog::{Blake3Hash, Rating, TagPath, VolumeKind};
use keepsake_core::ingest::fault::FaultPlan;
use keepsake_core::ingest::template::{Template, DEFAULT_TEMPLATE};
use keepsake_core::ingest::{self, destination_volume, PlanOptions, RunContext, Selection};
use keepsake_core::library::Library;
use keepsake_core::metadata::MetadataService;
use keepsake_core::probe::{ProbeOptions, Prober, ToolPaths};
use keepsake_core::render::{self, Cache, RenderOptions, RenderService, RenderTools};

/// Every file (bytes, size, time) and every folder under `root`.
fn tree(root: &Path) -> BTreeMap<PathBuf, Option<(Blake3Hash, u64, std::time::SystemTime)>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        out.insert(d.strip_prefix(root).unwrap().to_path_buf(), None);
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let md = std::fs::metadata(&p).unwrap();
                let rel = p.strip_prefix(root).unwrap().to_path_buf();
                let hash = Blake3Hash::of(&std::fs::read(&p).unwrap());
                out.insert(rel, Some((hash, md.len(), md.modified().unwrap())));
            }
        }
    }
    out
}

fn tools() -> Option<(RenderTools, ToolPaths)> {
    let (t, ffmpeg) = common::media::tools()?;
    let dcraw = common::workspace_root()
        .join("vendor")
        .join("libraw")
        .join(if cfg!(windows) {
            "dcraw_emu.exe"
        } else {
            "dcraw_emu"
        });
    Some((
        RenderTools {
            exiftool: t.exiftool.clone(),
            ffprobe: t.ffprobe.clone(),
            ffmpeg,
            dcraw,
        },
        t,
    ))
}

/// A Sony-like card with real media: JPEGs, a RAW + JPEG pair, a lone
/// "RAW", an XAVC clip with its XML and SUB proxy, and a camera thumbnail.
fn card(root: &Path, t: &RenderTools) -> PathBuf {
    let c = root.join("card");
    let dcim = c.join("DCIM").join("100MSDCF");
    for i in 1..=3 {
        common::media::jpeg_n(&t.ffmpeg, &dcim.join(format!("DSC0000{i}.JPG")), i);
    }
    std::fs::write(dcim.join("DSC00001.ARW"), b"not really a raw").unwrap();
    std::fs::write(dcim.join("DSC00009.ARW"), b"not really a raw either").unwrap();
    let m4 = c.join("PRIVATE").join("M4ROOT");
    common::media::h264_mp4(
        &t.ffmpeg,
        &m4.join("CLIP").join("C0001.MP4"),
        "2024-06-15T11:02:11Z",
        None,
    );
    std::fs::write(
        m4.join("CLIP").join("C0001M01.XML"),
        r#"<?xml version="1.0"?><NonRealTimeMeta><CreationDate value="2024-06-15T13:02:11+02:00"/></NonRealTimeMeta>"#,
    )
    .unwrap();
    common::media::h264_mp4(
        &t.ffmpeg,
        &m4.join("SUB").join("C0001S03.MP4"),
        "2024-06-15T11:02:11Z",
        None,
    );
    std::fs::create_dir_all(m4.join("THMBNL")).unwrap();
    common::media::jpeg(&t.ffmpeg, &m4.join("THMBNL").join("C0001T01.JPG"));
    c
}

fn opts() -> PlanOptions {
    PlanOptions {
        template: Template::parse(DEFAULT_TEMPLATE).unwrap(),
        selection: Selection::NewOnly,
        event: None,
        allow_same_filesystem: true,
        source_id: None,
    }
}

#[test]
fn a_card_is_only_read_by_everything_that_reads_a_card() {
    let Some((t, probe_tools)) = tools() else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let lib = Library::create(&root.join("lib")).unwrap();
    let card = card(root, &t);
    let before = tree(&card);
    let guard = common::readonly::deny_writes(&card);
    assert!(
        std::fs::write(card.join("x"), b"x").is_err(),
        "the OS refuses writes"
    );

    let mut cat = lib.open_catalog().unwrap();
    let prober = Prober::new(&probe_tools, ProbeOptions::default()).unwrap();
    let dests = |a: &str, b: &str| {
        vec![
            destination_volume(&lib, &root.join(a), a, VolumeKind::External).unwrap(),
            destination_volume(&lib, &root.join(b), b, VolumeKind::External).unwrap(),
        ]
    };
    let d1 = dests("A", "B");

    // The import window: list the card, thumbnails, the pre-check.
    let items = ingest::scan_card(&cat, &lib, &card, None).unwrap();
    assert!(items.len() >= 5, "{}", items.len());
    let cache = Cache::new(root.join("cache"));
    let et = keepsake_core::probe::ExifTool::start(&t.exiftool, std::time::Duration::from_secs(30))
        .unwrap();
    // The lone fake RAW has no embedded preview: it fails, but only reads.
    let made = items
        .iter()
        .filter(|it| render::card::card_thumbnail(&cache, &t, Some(&et), &card, &it.files).is_ok())
        .count();
    assert!(made >= items.len() - 1, "{made} of {}", items.len());
    drop(et);
    ingest::preview(&cat, &lib, Some(&prober), &card, &d1, &opts()).unwrap();

    // Import with probing, to two drives.
    let cancel = AtomicBool::new(false);
    let run = |cat: &mut keepsake_core::catalog::Catalog, s, cancel: &AtomicBool| {
        let mut progress = |_: &ingest::Progress| {};
        let mut ctx = RunContext {
            prober: Some(&prober),
            fault: &FaultPlan::none(),
            cancel,
            progress: &mut progress,
            retry_failed: true,
        };
        ingest::run(cat, &lib, s, &mut ctx)
    };
    let s = ingest::plan(&mut cat, &lib, Some(&prober), &card, "Card", &d1, &opts()).unwrap();
    assert!(run(&mut cat, s, &cancel).unwrap().safe_to_format);
    // Again: nothing new, but everything is read and confirmed.
    let s = ingest::plan(&mut cat, &lib, Some(&prober), &card, "Card", &d1, &opts()).unwrap();
    assert!(run(&mut cat, s, &cancel).unwrap().safe_to_format);
    // Stopped at once, then resumed, to two other drives.
    let d2 = dests("C", "D");
    let all = PlanOptions {
        selection: Selection::All,
        ..opts()
    };
    let s = ingest::plan(&mut cat, &lib, Some(&prober), &card, "Card", &d2, &all).unwrap();
    let stop = AtomicBool::new(true);
    // Stopped before the first file: nothing copied, nothing safe.
    assert!(!run(&mut cat, s, &stop).is_ok_and(|r| r.safe_to_format));
    let r = run(&mut cat, s, &cancel).unwrap();
    assert!(r.safe_to_format, "{:?} {:?}", r.not_safe, r.failed);
    // The device watcher's card recognition reads the layout and items.
    assert!(!keepsake_core::cards::recognize(&card).is_empty());
    assert!(!keepsake_core::cards::items(&card).is_empty());

    drop(guard);
    assert_eq!(tree(&card), before, "the card is exactly as it was");
}

#[test]
fn a_read_only_library_volume_is_only_read() {
    let Some((t, probe_tools)) = tools() else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let lib = Library::create(&root.join("lib")).unwrap();
    let media = card(root, &t);
    let before = tree(&media);
    let guard = common::readonly::deny_writes(&media);
    let prober = Prober::new(&probe_tools, ProbeOptions::default()).unwrap();
    let mut cat = lib.open_catalog().unwrap();

    // Catalogue it (a rebuild with probing), marked read-only.
    let (vol, _) = keepsake_core::rebuild::add_volume(
        &mut cat,
        &lib,
        "Archive",
        VolumeKind::External,
        &media,
        true,
        Some(&prober),
    )
    .unwrap();
    let ids = cat.read(|tx| tx.assets().list_ids()).unwrap();
    assert!(ids.len() >= 5);

    // Thumbnails, previews and proxies.
    let cache = Cache::new(root.join("cache"));
    render::enqueue_backlog(&mut cat).unwrap();
    let svc = RenderService::new(
        &lib,
        t.clone(),
        cache.clone(),
        RenderOptions {
            workers: 2,
            proxy_workers: 1,
            max_bytes: u64::MAX,
            pause_on_battery: false,
            until_empty: true,
            forced_encoder: None,
            evict_every: 25,
        },
    );
    svc.run(&AtomicBool::new(false), &mut |_| {}).unwrap();

    // The viewer: full size (and the RAW, which LibRaw rejects here), video.
    for id in &ids {
        let _ = render::full::full_image(
            &mut cat,
            &lib,
            &cache,
            &t,
            *id,
            render::full::Member::Primary,
        );
        let _ =
            render::full::full_image(&mut cat, &lib, &cache, &t, *id, render::full::Member::Raw);
        let _ = render::full::video_info(&cat, &lib, &t, *id);
    }

    // Edits: the sidecars go to the library's mirror, not beside originals.
    {
        let mut svc = MetadataService::new(&lib, &mut cat);
        for id in &ids {
            svc.set_rating(*id, Rating::new(4).unwrap()).unwrap();
            svc.set_label(*id, Some("Red")).unwrap();
            svc.set_comment(*id, Some("archived")).unwrap();
            svc.set_user_tags(*id, &[TagPath::parse_lightroom("Places|Lake").unwrap()])
                .unwrap();
            let _ = svc.shift_capture_time(*id, 3600);
        }
    }
    let mirror = keepsake_core::xmp::sidecar::mirror_root(&lib.sidecars_dir(), vol.id);
    assert!(
        common::xmp_files(&mirror).len() >= ids.len(),
        "sidecars mirrored in the library"
    );

    // Verify it, back it up, and rebuild the catalog from it.
    let rep = keepsake_core::verify::verify(
        &cat,
        &lib,
        Some(vol.id),
        &mut |_| {},
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(rep.issues.is_empty(), "{:?}", rep.issues);
    let dest =
        destination_volume(&lib, &root.join("backup"), "Backup", VolumeKind::External).unwrap();
    let b = keepsake_core::backup::run(
        &mut cat,
        &lib,
        &dest,
        &FaultPlan::none(),
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .unwrap();
    assert!(b.failed.is_empty() && b.files_copied > 0, "{b:?}");
    keepsake_core::rebuild::rebuild_catalog_with(&mut cat, &lib, vol.id, &media, Some(&prober))
        .unwrap();

    drop(guard);
    assert_eq!(
        tree(&media),
        before,
        "the read-only volume is exactly as it was"
    );
}

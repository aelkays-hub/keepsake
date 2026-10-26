//! Renditions and the job queue.
//!
//! Queue and cache tests need no tools. Render tests generate real media
//! with the vendored FFmpeg (and ExifTool for orientation and RAW
//! previews); they skip without the tools, except on CI.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use keepsake_core::catalog::{
    Asset, AssetId, Blake3Hash, Catalog, FileRole, JobState, MediaKind, NewJob, Priority,
    VolumeKind,
};
use keepsake_core::jobs;
use keepsake_core::library::Library;
use keepsake_core::rebuild::add_volume;
use keepsake_core::render::source::{locate, Source};
use keepsake_core::render::{
    self, still, video, Cache, RenderEvent, RenderOptions, RenderService, RenderTools, Variant,
    JOB_PROXY, JOB_STILLS,
};

// ── Helpers ─────────────────────────────────────────────────────────────────

fn tools() -> Option<RenderTools> {
    let (t, ffmpeg) = common::media::tools()?;
    Some(RenderTools {
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
    })
}

fn ff(tools: &RenderTools, args: &[&str]) {
    let out = Command::new(&tools.ffmpeg)
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ffmpeg {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn mk(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
}

fn still_file(t: &RenderTools, out: &Path, w: u32, h: u32, extra: &[&str]) {
    mk(out);
    let size = format!("testsrc2=size={w}x{h}");
    let mut a = vec!["-f", "lavfi", "-i", &size, "-frames:v", "1"];
    a.extend_from_slice(extra);
    a.push(s(out));
    ff(t, &a);
}

fn clip(t: &RenderTools, out: &Path, w: u32, h: u32, secs: u32, codec: &str) {
    mk(out);
    let v = format!("testsrc2=size={w}x{h}:rate=30");
    let d = secs.to_string();
    ff(
        t,
        &[
            "-f",
            "lavfi",
            "-i",
            &v,
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            &d,
            "-c:v",
            codec,
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
            s(out),
        ],
    );
}

fn set_orientation(t: &RenderTools, file: &Path, o: u8) {
    let out = Command::new(&t.exiftool)
        .args([
            "-q",
            "-n",
            "-overwrite_original",
            &format!("-Orientation={o}"),
        ])
        .arg(file)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn dims(p: &Path) -> (u32, u32) {
    image::image_dimensions(p).unwrap()
}

fn probe(t: &RenderTools, p: &Path) -> keepsake_core::probe::MediaStreams {
    keepsake_core::probe::ffprobe::probe(&t.ffprobe, p, std::time::Duration::from_secs(30)).unwrap()
}

/// Largest gap between keyframes, including the stretch to the end.
fn max_gop_s(t: &RenderTools, p: &Path) -> f64 {
    let out = Command::new(&t.ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-skip_frame",
            "nokey",
            "-show_entries",
            "frame=pts_time",
            "-of",
            "csv=p=0",
        ])
        .arg(p)
        .output()
        .unwrap();
    let mut keys: Vec<f64> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| l.trim().trim_end_matches(',').parse().ok())
        .collect();
    keys.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let dur = probe(t, p).duration_ms.unwrap() as f64 / 1000.0;
    keys.push(dur);
    keys.windows(2).map(|w| w[1] - w[0]).fold(0.0, f64::max)
}

struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    lib: Library,
    cat: Catalog,
    media: PathBuf,
    cache: Cache,
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let lib = Library::create(&root.join("lib")).unwrap();
        let cat = lib.open_catalog().unwrap();
        let media = root.join("media");
        std::fs::create_dir_all(&media).unwrap();
        let cache = Cache::new(root.join("cache"));
        Self {
            _tmp: tmp,
            root,
            lib,
            cat,
            media,
            cache,
        }
    }

    /// Catalogue `media/` as a library volume.
    fn index(&mut self) {
        add_volume(
            &mut self.cat,
            &self.lib,
            "Media",
            VolumeKind::External,
            &self.media,
            false,
            None,
        )
        .unwrap();
    }

    fn asset_of(&self, rel: &str) -> AssetId {
        let path = std::fs::canonicalize(self.media.join(rel)).unwrap();
        self.lib
            .locate(&self.cat, &path)
            .unwrap()
            .unwrap_or_else(|| panic!("{rel} not catalogued"))
            .asset_id
    }

    fn source(&self, rel: &str) -> Source {
        locate(&self.cat, &self.lib, self.asset_of(rel)).unwrap()
    }

    fn service(&self, t: &RenderTools, workers: usize) -> RenderService {
        let opts = RenderOptions {
            workers,
            proxy_workers: 1,
            max_bytes: u64::MAX,
            pause_on_battery: true,
            until_empty: true,
            forced_encoder: None,
            evict_every: 25,
        };
        RenderService::new(&self.lib, t.clone(), self.cache.clone(), opts)
    }

    fn run(&self, svc: &RenderService) -> Vec<RenderEvent> {
        let cancel = AtomicBool::new(false);
        let mut evs = Vec::new();
        svc.run(&cancel, &mut |e| evs.push(e.clone())).unwrap();
        evs
    }
}

/// Insert bare assets (no files) for queue tests.
fn assets(cat: &mut Catalog, kinds: &[(MediaKind, Option<i64>)]) -> Vec<AssetId> {
    cat.write(|tx| {
        let mut ids = Vec::new();
        for (k, t) in kinds {
            let id = AssetId::new_random();
            let mut a = Asset::new(id, *k, 0);
            a.capture = t.map(|ms| keepsake_core::catalog::CaptureTime {
                utc_ms: ms,
                utc_offset_min: Some(0),
            });
            tx.assets().insert(&a)?;
            ids.push(id);
        }
        Ok::<_, keepsake_core::catalog::CatalogError>(ids)
    })
    .unwrap()
}

fn claim(cat: &mut Catalog, kinds: &[&str]) -> Option<keepsake_core::catalog::Job> {
    cat.write(|tx| tx.jobs().claim(kinds, jobs::now_ms()))
        .unwrap()
}

fn tree(root: &Path) -> BTreeMap<PathBuf, Blake3Hash> {
    common::tree_hashes(root)
}

// ── Queue ───────────────────────────────────────────────────────────────────

#[test]
fn queue_runs_visible_then_recent_then_backlog_newest_first() {
    let mut cat = Catalog::open_in_memory().unwrap();
    let ids = assets(
        &mut cat,
        &[
            (MediaKind::Photo, Some(1_000)),
            (MediaKind::Photo, Some(3_000)),
            (MediaKind::Video, Some(2_000)),
            (MediaKind::Photo, None),
        ],
    );
    render::enqueue(&mut cat, &ids, Priority::Backlog, false).unwrap();
    render::enqueue(&mut cat, &ids[..1], Priority::Recent, false).unwrap();
    let cache = Cache::new(std::env::temp_dir().join("keepsake-no-cache"));
    render::set_visible(&mut cat, &cache, &ids[3..]).unwrap();
    let mut order = Vec::new();
    while let Some(j) = claim(&mut cat, &[]) {
        order.push((j.asset_id, j.kind.clone()));
        cat.write(|tx| tx.jobs().complete(j.id, 0)).unwrap();
    }
    let want = vec![
        (ids[3], JOB_STILLS.to_owned()), // visible
        (ids[0], JOB_STILLS.to_owned()), // recent
        (ids[1], JOB_STILLS.to_owned()), // backlog stills, newest first
        (ids[2], JOB_STILLS.to_owned()),
        (ids[2], JOB_PROXY.to_owned()), // proxies after stills
    ];
    assert_eq!(order, want);
}

#[test]
fn visibility_moves_with_the_screen_and_enqueue_never_duplicates() {
    let mut cat = Catalog::open_in_memory().unwrap();
    let ids = assets(
        &mut cat,
        &[(MediaKind::Photo, Some(1)), (MediaKind::Photo, Some(2))],
    );
    assert_eq!(
        render::enqueue(&mut cat, &ids, Priority::Backlog, false).unwrap(),
        2
    );
    assert_eq!(
        render::enqueue(&mut cat, &ids, Priority::Backlog, false).unwrap(),
        0
    );
    let cache = Cache::new(std::env::temp_dir().join("keepsake-no-cache"));
    render::set_visible(&mut cat, &cache, &ids[..1]).unwrap();
    render::set_visible(&mut cat, &cache, &ids[1..]).unwrap();
    let counts = cat.read(|tx| tx.jobs().counts()).unwrap();
    assert_eq!(counts.queued_by_priority.get("visible"), Some(&1));
    assert_eq!(counts.queued_by_priority.get("backlog"), Some(&1));
    assert_eq!(claim(&mut cat, &[]).unwrap().asset_id, ids[1]);
    // A recent import of a backlog asset improves its class.
    render::enqueue(&mut cat, &ids[..1], Priority::Recent, false).unwrap();
    let j = cat
        .read(|tx| tx.jobs().find(JOB_STILLS, ids[0]))
        .unwrap()
        .unwrap();
    assert_eq!(j.priority, Priority::Recent);
}

#[test]
fn failures_back_off_then_stop_and_offline_does_not_count() {
    let mut cat = Catalog::open_in_memory().unwrap();
    let ids = assets(&mut cat, &[(MediaKind::Photo, Some(1))]);
    render::enqueue(&mut cat, &ids, Priority::Backlog, false).unwrap();
    let mut now = 1_000_000;
    for attempt in 1..=jobs::MAX_ATTEMPTS {
        let j = cat.write(|tx| tx.jobs().claim(&[], now)).unwrap().unwrap();
        assert_eq!(j.attempts, attempt);
        let retry = jobs::retry_at(j.attempts, now);
        cat.write(|tx| tx.jobs().fail(j.id, "boom", retry, now))
            .unwrap();
        // Not claimable before the backoff ends.
        assert!(cat.write(|tx| tx.jobs().claim(&[], now)).unwrap().is_none());
        now += 10 * 60 * 1000;
    }
    let j = cat
        .read(|tx| tx.jobs().find(JOB_STILLS, ids[0]))
        .unwrap()
        .unwrap();
    assert_eq!(j.state, JobState::Failed);
    assert_eq!(j.error.as_deref(), Some("boom"));
    assert_eq!(cat.write(|tx| tx.jobs().retry_failed(&[])).unwrap(), 1);
    let j = cat.write(|tx| tx.jobs().claim(&[], now)).unwrap().unwrap();
    assert_eq!(j.attempts, 1);
    cat.write(|tx| tx.jobs().defer(j.id, "offline", now + 5))
        .unwrap();
    let j = cat
        .write(|tx| tx.jobs().claim(&[], now + 5))
        .unwrap()
        .unwrap();
    assert_eq!(j.attempts, 1, "a deferral is not an attempt");
}

#[test]
fn the_queue_survives_a_restart() {
    let t = tempfile::tempdir().unwrap();
    let lib = Library::create(t.path()).unwrap();
    let ids = {
        let mut cat = lib.open_catalog().unwrap();
        let ids = assets(
            &mut cat,
            &[(MediaKind::Photo, Some(1)), (MediaKind::Photo, Some(2))],
        );
        render::enqueue(&mut cat, &ids, Priority::Backlog, false).unwrap();
        let j = claim(&mut cat, &[]).unwrap();
        assert_eq!(j.state, JobState::Running);
        ids
        // "crash": the catalog is closed with a job running
    };
    let mut cat = lib.open_catalog().unwrap();
    assert_eq!(jobs::recover(&mut cat).unwrap(), 1);
    let c = cat.read(|tx| tx.jobs().counts()).unwrap();
    assert_eq!(c.total(JobState::Queued), 2);
    assert_eq!(c.total(JobState::Running), 0);
    assert!(ids.iter().all(|a| cat
        .read(|tx| tx.jobs().find(JOB_STILLS, *a))
        .unwrap()
        .is_some()));
}

#[test]
fn enqueue_again_requeues_finished_jobs() {
    let mut cat = Catalog::open_in_memory().unwrap();
    let ids = assets(&mut cat, &[(MediaKind::Video, Some(1))]);
    render::enqueue(&mut cat, &ids, Priority::Backlog, false).unwrap();
    while let Some(j) = claim(&mut cat, &[]) {
        cat.write(|tx| tx.jobs().complete(j.id, 0)).unwrap();
    }
    assert_eq!(
        render::enqueue(&mut cat, &ids, Priority::Backlog, false).unwrap(),
        0
    );
    assert_eq!(
        render::enqueue(&mut cat, &ids, Priority::Visible, true).unwrap(),
        2
    );
    cat.write(|tx| tx.jobs().set_visible(&ids)).unwrap();
    let j = claim(&mut cat, &[JOB_PROXY]).unwrap();
    assert_eq!(j.priority, Priority::Visible);
    // The generic queue takes other kinds too.
    cat.write(|tx| {
        tx.jobs().enqueue(
            &[NewJob {
                kind: "export.flickr",
                asset_id: ids[0],
                rank: 5,
                sort_key: 0,
            }],
            Priority::Backlog,
            0,
            false,
        )
    })
    .unwrap();
    assert!(claim(&mut cat, &["export.flickr"]).is_some());
}

#[test]
fn ingest_heartbeat_is_seen_and_cleared() {
    let t = tempfile::tempdir().unwrap();
    let lib = Library::create(t.path()).unwrap();
    let cat = lib.open_catalog().unwrap();
    assert!(!jobs::ingest_running(&cat, jobs::now_ms()));
    let beat = jobs::IngestHeartbeat::start(&lib);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !jobs::ingest_running(&cat, jobs::now_ms()) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(jobs::ingest_running(&cat, jobs::now_ms()));
    // A stale heartbeat (a crashed ingest) does not count.
    assert!(!jobs::ingest_running(
        &cat,
        jobs::now_ms() + jobs::HEARTBEAT_STALE_MS + 1
    ));
    drop(beat);
    assert!(!jobs::ingest_running(&cat, jobs::now_ms()));
}

// ── Cache ───────────────────────────────────────────────────────────────────

#[test]
fn cache_paths_are_content_addressed() {
    let src = Blake3Hash::of(b"original");
    let a = Cache::rel_path(&src, Variant::Thumb256)
        .unwrap()
        .to_string();
    let b = Cache::rel_path(&src, Variant::Thumb1024)
        .unwrap()
        .to_string();
    let hex = src.to_hex();
    assert!(
        a.starts_with(&format!("thumbnail/{}/{hex}-", &hex[..2])),
        "{a}"
    );
    assert!(a.ends_with(".jpg") && a != b);
    let p = Cache::rel_path(&src, Variant::Proxy720)
        .unwrap()
        .to_string();
    assert!(p.starts_with("proxy/") && p.ends_with(".mp4"), "{p}");
    assert!(
        Cache::rel_path(&Blake3Hash::of(b"other"), Variant::Thumb256)
            .unwrap()
            .to_string()
            != a
    );
}

#[test]
fn eviction_is_lru_and_keeps_offline_media() {
    let mut w = World::new();
    let online = w.root.join("online");
    let offline = w.root.join("offline");
    common::write_file(&online, "a.jpg", b"aaaa");
    common::write_file(&online, "b.jpg", b"bbbb");
    common::write_file(&offline, "c.jpg", b"cccc");
    for (label, m) in [("On", &online), ("Off", &offline)] {
        add_volume(
            &mut w.cat,
            &w.lib,
            label,
            VolumeKind::External,
            m,
            false,
            None,
        )
        .unwrap();
    }
    let hash = |name: &[u8]| Blake3Hash::of(name);
    // Oldest first: c (offline), a, b.
    for h in [hash(b"cccc"), hash(b"aaaa"), hash(b"bbbb")] {
        w.cache
            .put_bytes(&mut w.cat, &h, Variant::Thumb256, &[0u8; 100])
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    // The offline drive is unplugged.
    std::fs::rename(&offline, w.root.join("unplugged")).unwrap();
    let rep = w.cache.evict(&mut w.cat, &w.lib, 200).unwrap();
    assert_eq!(rep.removed, 1, "{rep:?}");
    assert!(w
        .cache
        .find(&mut w.cat, &hash(b"cccc"), Variant::Thumb256)
        .unwrap()
        .is_some());
    assert!(w
        .cache
        .find(&mut w.cat, &hash(b"aaaa"), Variant::Thumb256)
        .unwrap()
        .is_none());
    assert!(w
        .cache
        .find(&mut w.cat, &hash(b"bbbb"), Variant::Thumb256)
        .unwrap()
        .is_some());
    // Over the limit but nothing evictable left except b; limit 0 keeps c.
    let rep = w.cache.evict(&mut w.cat, &w.lib, 0).unwrap();
    assert_eq!(rep.removed, 1);
    assert_eq!(rep.kept_offline_bytes, 100);
    let st = w.cache.stats(&w.cat, &w.lib, 0).unwrap();
    assert_eq!((st.files, st.bytes, st.kept_offline_bytes), (1, 100, 100));
    // A file deleted behind the cache's back is noticed.
    let (_, p) = w
        .cache
        .find(&mut w.cat, &hash(b"cccc"), Variant::Thumb256)
        .unwrap()
        .unwrap();
    std::fs::remove_file(p).unwrap();
    assert!(w
        .cache
        .find(&mut w.cat, &hash(b"cccc"), Variant::Thumb256)
        .unwrap()
        .is_none());
    assert_eq!(w.cache.stats(&w.cat, &w.lib, 0).unwrap().files, 0);
}

// ── Stills ──────────────────────────────────────────────────────────────────

#[test]
fn stills_from_jpeg_png_and_16_bit_tiff_are_upright_and_sized() {
    let Some(t) = tools() else { return };
    let mut w = World::new();
    still_file(&t, &w.media.join("wide.jpg"), 4000, 3000, &["-q:v", "3"]);
    still_file(&t, &w.media.join("turned.jpg"), 3000, 2000, &["-q:v", "3"]);
    set_orientation(&t, &w.media.join("turned.jpg"), 6); // rotate 90° CW to view
    still_file(&t, &w.media.join("small.png"), 800, 600, &[]);
    still_file(
        &t,
        &w.media.join("deep.tif"),
        3000,
        3000,
        &["-pix_fmt", "rgb48le"],
    );
    w.index();
    let before = tree(&w.media);
    for (file, want) in [
        ("wide.jpg", [(2560, 1920), (1024, 768), (256, 192)]),
        ("turned.jpg", [(1707, 2560), (683, 1024), (171, 256)]),
        ("small.png", [(800, 600), (800, 600), (256, 192)]),
        ("deep.tif", [(2560, 2560), (1024, 1024), (256, 256)]),
    ] {
        let src = w.source(file);
        let made = still::render_stills(&mut w.cat, &w.cache, &src, &t, None).unwrap();
        let got: Vec<(u32, u32)> = made.iter().map(|(_, p)| dims(p)).collect();
        assert_eq!(got, want, "{file}");
        assert!(still::render_stills(&mut w.cat, &w.cache, &src, &t, None)
            .unwrap()
            .is_empty());
    }
    assert_eq!(tree(&w.media), before, "originals untouched");
}

#[test]
fn raw_jpeg_pair_renders_from_the_jpeg_and_bad_raw_fails_for_good() {
    let Some(t) = tools() else { return };
    let mut w = World::new();
    still_file(
        &t,
        &w.media.join("DCIM/100MSDCF/DSC00001.JPG"),
        1200,
        800,
        &[],
    );
    std::fs::write(
        w.media.join("DCIM/100MSDCF/DSC00001.ARW"),
        b"not really a raw",
    )
    .unwrap();
    std::fs::write(
        w.media.join("DCIM/100MSDCF/DSC00002.ARW"),
        b"not really a raw",
    )
    .unwrap();
    w.index();
    let pair = w.source("DCIM/100MSDCF/DSC00001.JPG");
    assert!(pair.still.to_string_lossy().ends_with(".JPG"));
    assert_eq!(
        still::render_stills(&mut w.cat, &w.cache, &pair, &t, None)
            .unwrap()
            .len(),
        3
    );
    let et = keepsake_core::probe::ExifTool::start(&t.exiftool, std::time::Duration::from_secs(30))
        .unwrap();
    let raw = w.source("DCIM/100MSDCF/DSC00002.ARW");
    let err = still::render_stills(&mut w.cat, &w.cache, &raw, &t, Some(&et)).unwrap_err();
    assert!(err.is_permanent(), "{err}");
}

#[test]
fn video_poster_is_the_frame_at_ten_percent_and_follows_rotation() {
    let Some(t) = tools() else { return };
    let mut w = World::new();
    clip(&t, &w.media.join("clip.mp4"), 1920, 1080, 5, "libopenh264");
    common::media::rotate_copy(
        &t.ffmpeg,
        &w.media.join("clip.mp4"),
        &w.media.join("portrait.mp4"),
        90,
    );
    w.index();
    let src = w.source("clip.mp4");
    let made = still::render_stills(&mut w.cat, &w.cache, &src, &t, None).unwrap();
    assert_eq!(
        dims(&made[0].1),
        (1920, 1080),
        "4K would be fitted; HD is kept"
    );
    let src = w.source("portrait.mp4");
    let made = still::render_stills(&mut w.cat, &w.cache, &src, &t, None).unwrap();
    let (pw, ph) = dims(&made[0].1);
    assert!(ph > pw, "rotated poster is portrait: {pw}x{ph}");
}

// ── Proxies ─────────────────────────────────────────────────────────────────

fn check_proxy(t: &RenderTools, p: &Path, src_secs: f64) {
    let m = probe(t, p);
    let v = m.video.as_ref().unwrap();
    assert_eq!(v.codec, "h264", "{p:?}");
    assert!(v.height <= 720, "{}x{}", v.width, v.height);
    assert_eq!(m.audio.first().map(|a| a.codec.as_str()), Some("aac"));
    let d = m.duration_ms.unwrap() as f64 / 1000.0;
    assert!((d - src_secs).abs() < 0.5, "duration {d} vs {src_secs}");
    let gop = max_gop_s(t, p);
    assert!(gop <= 1.05, "GOP {gop:.2}s");
}

#[test]
fn proxies_are_720p_h264_with_one_second_gops() {
    let Some(t) = tools() else { return };
    let mut w = World::new();
    clip(&t, &w.media.join("hd.mp4"), 1920, 1080, 4, "libopenh264");
    clip(&t, &w.media.join("hevc.mp4"), 1280, 720, 3, "libkvazaar");
    clip(&t, &w.media.join("small.mp4"), 640, 360, 2, "libopenh264");
    w.index();
    let before = tree(&w.media);
    // The detected encoder (hardware here, maybe) and the software fallback.
    for forced in [None, Some("libopenh264")] {
        for (file, secs, h) in [
            ("hd.mp4", 4.0, 720),
            ("hevc.mp4", 3.0, 720),
            ("small.mp4", 2.0, 360),
        ] {
            let src = w.source(file);
            let e = video::detect(&t.ffmpeg, forced).unwrap();
            let p = video::make_proxy(&mut w.cat, &w.cache, &src, &t, &e).unwrap();
            check_proxy(&t, &p, secs);
            assert_eq!(
                probe(&t, &p).video.unwrap().height,
                h,
                "{file} via {}",
                e.name
            );
        }
    }
    assert_eq!(tree(&w.media), before);
}

#[test]
fn camera_sub_proxy_is_copied_when_h264_and_replaced_when_hevc() {
    let Some(t) = tools() else { return };
    let mut w = World::new();
    let m4 = w.media.join("PRIVATE/M4ROOT");
    clip(&t, &m4.join("CLIP/C0001.MP4"), 1280, 720, 2, "libkvazaar");
    clip(&t, &m4.join("SUB/C0001S01.MP4"), 640, 360, 2, "libopenh264");
    clip(&t, &m4.join("CLIP/C0002.MP4"), 1280, 720, 3, "libkvazaar");
    clip(&t, &m4.join("SUB/C0002S01.MP4"), 640, 360, 3, "libkvazaar"); // an HEVC proxy
    w.index();
    let one = w.source("PRIVATE/M4ROOT/CLIP/C0001.MP4");
    assert!(one.camera_proxy.is_some(), "SUB proxy found");
    let (v, p, enc) = video::render_proxy(&mut w.cat, &w.cache, &one, &t, None)
        .unwrap()
        .unwrap();
    assert_eq!(v, Variant::CameraProxy);
    assert!(enc.is_none(), "no encode");
    assert_eq!(
        std::fs::read(&p).unwrap(),
        std::fs::read(one.camera_proxy.as_ref().unwrap()).unwrap()
    );
    assert!(video::render_proxy(&mut w.cat, &w.cache, &one, &t, None)
        .unwrap()
        .is_none());
    assert_eq!(
        w.cache.lookup_proxy(&mut w.cat, one.asset).unwrap(),
        Some(p)
    );

    let two = w.source("PRIVATE/M4ROOT/CLIP/C0002.MP4");
    let (v, p, enc) = video::render_proxy(&mut w.cat, &w.cache, &two, &t, None)
        .unwrap()
        .unwrap();
    assert_eq!(v, Variant::Proxy720, "HEVC must never reach the webview");
    assert!(enc.is_some());
    check_proxy(&t, &p, 3.0);
}

#[test]
fn avchd_segments_are_joined_before_encoding() {
    let Some(t) = tools() else { return };
    let mut w = World::new();
    let dir = w.media.join("STREAM");
    for (name, secs) in [("00000.MTS", "2"), ("00001.MTS", "1")] {
        let out = dir.join(name);
        mk(&out);
        ff(
            &t,
            &[
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=1440x1080:rate=25",
                "-f",
                "lavfi",
                "-i",
                "sine=sample_rate=48000",
                "-t",
                secs,
                "-c:v",
                "libopenh264",
                "-c:a",
                "ac3",
                "-f",
                "mpegts",
                s(&out),
            ],
        );
    }
    w.index();
    let mut src = w.source("STREAM/00000.MTS");
    src.segments = vec![std::fs::canonicalize(dir.join("00001.MTS")).unwrap()];
    src.duration_ms = Some(3000);
    let e = video::detect(&t.ffmpeg, Some("libopenh264")).unwrap();
    let p = video::make_proxy(&mut w.cat, &w.cache, &src, &t, &e).unwrap();
    check_proxy(&t, &p, 3.0);
    let poster = still::picture(&src, &t, None, &w.cache.temp_path("x").unwrap()).unwrap();
    assert_eq!((poster.width(), poster.height()), (1440, 1080));
}

// ── Service ─────────────────────────────────────────────────────────────────

#[test]
fn the_service_renders_everything_once_and_reports_progress() {
    let Some(t) = tools() else { return };
    let mut w = World::new();
    // Different sizes: identical files would share renditions.
    for i in 0..6 {
        still_file(
            &t,
            &w.media.join(format!("p{i}.jpg")),
            1600 + i * 8,
            1200,
            &[],
        );
    }
    clip(&t, &w.media.join("v.mp4"), 1280, 720, 2, "libopenh264");
    w.index();
    assert_eq!(
        render::enqueue_backlog(&mut w.cat).unwrap(),
        8,
        "6 stills + poster + proxy"
    );
    let svc = w.service(&t, 3);
    let evs = w.run(&svc);
    let done = evs
        .iter()
        .filter(|e| matches!(e, RenderEvent::Done { .. }))
        .count();
    assert_eq!(done, 8, "{evs:#?}");
    assert!(matches!(
        evs.last(),
        Some(RenderEvent::Finished {
            done: 8,
            failed: 0,
            cancelled: false
        })
    ));
    let v = w.asset_of("v.mp4");
    assert!(w
        .cache
        .lookup(&mut w.cat, v, Variant::Thumb256)
        .unwrap()
        .is_some());
    assert!(w.cache.lookup_proxy(&mut w.cat, v).unwrap().is_some());
    let st = w.cat.read(|tx| tx.jobs().counts()).unwrap();
    assert_eq!(st.total(JobState::Done), 8);
    // Nothing left to do.
    let evs = w.run(&svc);
    assert!(matches!(
        evs.last(),
        Some(RenderEvent::Finished { done: 0, .. })
    ));
    // A rendition removed from the cache is made again when it is visible.
    let p = w
        .cache
        .lookup(&mut w.cat, v, Variant::Thumb256)
        .unwrap()
        .unwrap();
    std::fs::remove_file(p).unwrap();
    render::set_visible(&mut w.cat, &w.cache, &[v]).unwrap();
    assert!(w
        .cache
        .lookup(&mut w.cat, v, Variant::Thumb256)
        .unwrap()
        .is_none());
    let evs = w.run(&svc);
    assert!(
        evs.iter().any(
            |e| matches!(e, RenderEvent::Done { made, .. } if made == &["thumb-256".to_owned()])
        ),
        "{evs:#?}"
    );
}

#[test]
fn the_service_pauses_on_battery_and_during_ingest() {
    let Some(t) = tools() else { return };
    let mut w = World::new();
    still_file(&t, &w.media.join("a.jpg"), 800, 600, &[]);
    w.index();
    render::enqueue_backlog(&mut w.cat).unwrap();
    let calls = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&calls);
    // On battery for the first few checks.
    let svc = w
        .service(&t, 1)
        .with_power(Arc::new(move || c.fetch_add(1, Ordering::SeqCst) < 3));
    let evs = w.run(&svc);
    let kinds: Vec<&str> = evs
        .iter()
        .map(|e| match e {
            RenderEvent::Paused { .. } => "paused",
            RenderEvent::Resumed => "resumed",
            RenderEvent::Done { .. } => "done",
            RenderEvent::Started { .. } => "started",
            _ => "other",
        })
        .collect();
    assert_eq!(
        &kinds[..4],
        ["paused", "resumed", "started", "done"],
        "{evs:#?}"
    );

    // An ingest elsewhere: workers wait for its heartbeat to stop.
    still_file(&t, &w.media.join("b.jpg"), 800, 600, &[]);
    w.index();
    render::enqueue_backlog(&mut w.cat).unwrap();
    let beat = jobs::IngestHeartbeat::start(&w.lib);
    std::thread::sleep(std::time::Duration::from_millis(300));
    let stop = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1500));
        drop(beat);
    });
    let svc = w.service(&t, 1);
    let evs = w.run(&svc);
    stop.join().unwrap();
    assert!(
        matches!(
            evs[0],
            RenderEvent::Paused {
                reason: jobs::PauseReason::Ingesting
            }
        ),
        "{evs:#?}"
    );
    assert!(evs.iter().any(|e| matches!(e, RenderEvent::Done { .. })));
}

#[test]
fn offline_media_is_deferred_and_cancel_stops_after_the_current_job() {
    let Some(t) = tools() else { return };
    let mut w = World::new();
    for i in 0..4 {
        still_file(
            &t,
            &w.media.join(format!("p{i}.jpg")),
            800 + i * 8,
            600,
            &[],
        );
    }
    w.index();
    render::enqueue_backlog(&mut w.cat).unwrap();
    // Cancel after the first job.
    let svc = w.service(&t, 1);
    let cancel = AtomicBool::new(false);
    let mut evs = Vec::new();
    svc.run(&cancel, &mut |e| {
        if matches!(e, RenderEvent::Done { .. }) {
            cancel.store(true, Ordering::SeqCst);
        }
        evs.push(e.clone());
    })
    .unwrap();
    // The worker may already have claimed the next job when the flag is
    // seen; it finishes that one and stops.
    let Some(RenderEvent::Finished {
        done,
        cancelled: true,
        ..
    }) = evs.last().cloned()
    else {
        panic!("{evs:#?}");
    };
    assert!((1..=2).contains(&done), "{evs:#?}");
    let c = w.cat.read(|tx| tx.jobs().counts()).unwrap();
    assert_eq!(c.total(JobState::Done), done);
    assert_eq!(c.total(JobState::Queued), 4 - done);
    assert_eq!(c.total(JobState::Running), 0);
    let left = 4 - done as usize;

    // The drive goes away: the rest is deferred, not failed.
    std::fs::rename(&w.media, w.root.join("gone")).unwrap();
    let evs = w.run(&w.service(&t, 2));
    let deferred = evs
        .iter()
        .filter(|e| matches!(e, RenderEvent::Deferred { .. }))
        .count();
    assert_eq!(deferred, left, "{evs:#?}");
    let c = w.cat.read(|tx| tx.jobs().counts()).unwrap();
    assert_eq!(c.total(JobState::Failed), 0);
    assert_eq!(c.total(JobState::Queued), left as u64);
}

#[test]
fn ingest_queues_its_assets_as_recent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let lib = Library::create(&root.join("lib")).unwrap();
    let card = root.join("card");
    common::copy_dir(&common::fixture("xavc"), &card);
    common::copy_dir(&common::fixture("dcim"), &card);
    let dests: Vec<_> = ["a", "b"]
        .iter()
        .map(|d| {
            keepsake_core::ingest::destination_volume(&lib, &root.join(d), d, VolumeKind::External)
                .unwrap()
        })
        .collect();
    let mut cat = lib.open_catalog().unwrap();
    let opts = keepsake_core::ingest::PlanOptions {
        template: keepsake_core::ingest::template::Template::parse("{orig}").unwrap(),
        selection: keepsake_core::ingest::Selection::NewOnly,
        event: None,
        allow_same_filesystem: true,
        source_id: None,
    };
    let s =
        keepsake_core::ingest::plan(&mut cat, &lib, None, &card, "Card", &dests, &opts).unwrap();
    let cancel = AtomicBool::new(false);
    let mut progress = |_: &keepsake_core::ingest::Progress| {};
    let fault = keepsake_core::ingest::fault::FaultPlan::none();
    let mut ctx = keepsake_core::ingest::RunContext {
        prober: None,
        fault: &fault,
        cancel: &cancel,
        progress: &mut progress,
        retry_failed: false,
    };
    keepsake_core::ingest::run(&mut cat, &lib, s, &mut ctx).unwrap();
    let c = cat.read(|tx| tx.jobs().counts()).unwrap();
    // 2 photos (one is JPG+ARW+HIF) + 2 clips: 4 stills jobs + 2 proxy jobs.
    assert_eq!(c.by_kind[JOB_STILLS]["queued"], 4, "{c:?}");
    assert_eq!(c.by_kind[JOB_PROXY]["queued"], 2);
    assert_eq!(c.queued_by_priority.get("recent"), Some(&6));
    assert!(
        !jobs::ingest_running(&cat, jobs::now_ms()),
        "heartbeat cleared"
    );
    let _ = FileRole::Original;
}

// ── Card thumbnails (import window) ─────────────────────────────────────────

#[test]
fn card_items_get_thumbnails_without_touching_the_card() {
    let Some(t) = tools() else { return };
    let w = World::new();
    let card = w.root.join("card");
    let m4 = card.join("PRIVATE/M4ROOT");
    clip(&t, &m4.join("CLIP/C0001.MP4"), 1280, 720, 2, "libopenh264");
    still_file(&t, &m4.join("THMBNL/C0001T01.JPG"), 160, 90, &[]);
    clip(&t, &m4.join("CLIP/C0002.MP4"), 1280, 720, 3, "libkvazaar");
    clip(&t, &m4.join("SUB/C0002S01.MP4"), 640, 360, 3, "libopenh264");
    clip(&t, &m4.join("CLIP/C0003.MP4"), 1280, 720, 2, "libopenh264");
    still_file(
        &t,
        &card.join("DCIM/100MSDCF/DSC00001.JPG"),
        3000,
        2000,
        &[],
    );
    std::fs::write(card.join("DCIM/100MSDCF/DSC00001.ARW"), b"raw").unwrap();
    let before = tree(&card);
    let cases: [(&[&str], (u32, u32)); 4] = [
        (
            &[
                "PRIVATE/M4ROOT/CLIP/C0001.MP4",
                "PRIVATE/M4ROOT/THMBNL/C0001T01.JPG",
            ],
            (160, 90),
        ),
        (
            &[
                "PRIVATE/M4ROOT/CLIP/C0002.MP4",
                "PRIVATE/M4ROOT/SUB/C0002S01.MP4",
            ],
            (320, 180),
        ),
        (&["PRIVATE/M4ROOT/CLIP/C0003.MP4"], (320, 180)),
        (
            &["DCIM/100MSDCF/DSC00001.JPG", "DCIM/100MSDCF/DSC00001.ARW"],
            (320, 213),
        ),
    ];
    for (files, want) in cases {
        let files: Vec<String> = files.iter().map(|s| s.to_string()).collect();
        let p = render::card::card_thumbnail(&w.cache, &t, None, &card, &files).unwrap();
        assert_eq!(dims(&p), want, "{files:?}");
        let again = render::card::card_thumbnail(&w.cache, &t, None, &card, &files).unwrap();
        assert_eq!(p, again, "served from the cache");
        assert!(p.starts_with(w.cache.root().join("card")));
    }
    assert_eq!(tree(&card), before, "the card is only read");
    // No catalog rows: these never count as renditions of offline media.
    let rows = w.cat.read(|tx| tx.renditions().list_lru()).unwrap();
    assert!(rows.is_empty());
    assert_eq!(
        render::card::sweep(&w.cache),
        0,
        "fresh thumbnails are kept"
    );
}

// ── Viewer: full size and video info ────────────────────────────────────────

#[test]
fn full_size_is_the_whole_picture_upright_and_made_once() {
    use keepsake_core::render::full::{full_image, Member};
    let Some(t) = tools() else { return };
    let mut w = World::new();
    still_file(&t, &w.media.join("wide.jpg"), 4000, 3000, &["-q:v", "3"]);
    still_file(&t, &w.media.join("turned.jpg"), 3000, 2000, &["-q:v", "3"]);
    set_orientation(&t, &w.media.join("turned.jpg"), 6);
    still_file(&t, &w.media.join("small.png"), 800, 600, &[]);
    w.index();
    let before = tree(&w.media);
    for (file, want) in [
        ("wide.jpg", (4000, 3000)),
        ("turned.jpg", (2000, 3000)),
        ("small.png", (800, 600)),
    ] {
        let id = w.asset_of(file);
        let p = full_image(&mut w.cat, &w.lib, &w.cache, &t, id, Member::Primary).unwrap();
        assert_eq!(dims(&p), want, "{file}");
        let made = std::fs::metadata(&p).unwrap().modified().unwrap();
        let again = full_image(&mut w.cat, &w.lib, &w.cache, &t, id, Member::Primary).unwrap();
        assert_eq!(again, p);
        assert_eq!(std::fs::metadata(&again).unwrap().modified().unwrap(), made);
        assert!(p.starts_with(w.cache.root()));
    }
    assert_eq!(tree(&w.media), before, "originals untouched");
}

#[test]
fn raw_pairs_offer_a_toggle_and_the_raw_goes_to_libraw() {
    use keepsake_core::render::full::{full_image, members, Member};
    let Some(t) = tools() else { return };
    let mut w = World::new();
    let dir = w.media.join("DCIM/100MSDCF");
    still_file(&t, &dir.join("DSC00001.JPG"), 1200, 800, &[]);
    std::fs::write(dir.join("DSC00001.ARW"), b"not really a raw").unwrap();
    std::fs::write(dir.join("DSC00002.ARW"), b"not really a raw").unwrap();
    w.index();
    let pair = w.asset_of("DCIM/100MSDCF/DSC00001.JPG");
    let solo = w.asset_of("DCIM/100MSDCF/DSC00002.ARW");
    let m = members(&w.cat, &w.lib, pair).unwrap();
    assert!(m.has_pair && !m.raw_only, "{m:?}");
    let m = members(&w.cat, &w.lib, solo).unwrap();
    assert!(!m.has_pair && m.raw_only, "{m:?}");
    let jpeg = full_image(&mut w.cat, &w.lib, &w.cache, &t, pair, Member::Primary).unwrap();
    assert_eq!(dims(&jpeg), (1200, 800));
    // A broken RAW fails with a message (LibRaw rejects it, or is missing).
    let err = full_image(&mut w.cat, &w.lib, &w.cache, &t, pair, Member::Raw).unwrap_err();
    assert!(err.to_string().contains("LibRaw"), "{err}");
    assert!(full_image(&mut w.cat, &w.lib, &w.cache, &t, solo, Member::Primary).is_err());
}

#[test]
fn video_info_reads_rate_and_start_timecode() {
    use keepsake_core::render::full::video_info;
    let Some(t) = tools() else { return };
    let mut w = World::new();
    let out = w.media.join("PRIVATE/M4ROOT/CLIP/C0001.MP4");
    mk(&out);
    ff(
        &t,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=30000/1001",
            "-t",
            "2",
            "-c:v",
            "libopenh264",
            "-pix_fmt",
            "yuv420p",
            "-timecode",
            "01:00:00;00",
            s(&out),
        ],
    );
    w.index();
    let id = w.asset_of("PRIVATE/M4ROOT/CLIP/C0001.MP4");
    let v = video_info(&w.cat, &w.lib, &t, id).unwrap();
    assert_eq!((v.fps_num, v.fps_den), (30000, 1001), "{v:?}");
    assert_eq!((v.width, v.height), (320, 240));
    assert_eq!(v.start_timecode.as_deref(), Some("01:00:00;00"));
    assert!((1900..=2100).contains(&v.duration_ms.unwrap()), "{v:?}");
}

#[test]
fn removing_an_old_cache_keeps_everything_that_is_not_ours() {
    let mut w = World::new();
    let src = Blake3Hash::of(b"original");
    let p = w
        .cache
        .put_bytes(&mut w.cat, &src, Variant::Thumb256, &[7u8; 100])
        .unwrap();
    std::fs::create_dir_all(w.cache.root().join("card/ab")).unwrap();
    std::fs::write(w.cache.root().join("card/ab/x.jpg"), [1u8; 20]).unwrap();
    std::fs::write(w.cache.root().join("notes.txt"), b"mine").unwrap();
    std::fs::create_dir_all(w.cache.root().join("photos")).unwrap();
    std::fs::write(w.cache.root().join("photos/a.jpg"), b"mine").unwrap();
    assert_eq!(w.cache.remove_files().unwrap(), 120);
    assert!(!p.exists());
    assert!(!w.cache.root().join("card").exists());
    assert!(w.cache.root().join("notes.txt").is_file());
    assert!(w.cache.root().join("photos/a.jpg").is_file());
    // The catalog row of the removed file goes on the next lookup.
    assert!(w
        .cache
        .find(&mut w.cat, &src, Variant::Thumb256)
        .unwrap()
        .is_none());
}

//! Reading aircraft (ADR-041) through the render service, with a stand-in
//! for the text models: always run, also in CI.
//!
//! One more test reads real pictures with the real models. It is ignored
//! by default and needs `KEEPSAKE_AIRCRAFT_SAMPLES` (a folder of aircraft
//! photos, never committed) and the models in `vendor/models`:
//!
//! ```text
//! cargo test -p keepsake-core --test aircraft_tests -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use image::{DynamicImage, Rgb, RgbImage};
use keepsake_core::ai::aircraft::lookup;
use keepsake_core::ai::aircraft::marks::Confidence;
use keepsake_core::ai::aircraft::ocr::TextLine;
use keepsake_core::ai::aircraft::scan::{self, Reader, ReaderLoader, JOB_AI_AIRCRAFT, SOURCE};
use keepsake_core::ai::{Accel, AiError, Embedding};
use keepsake_core::catalog::{AssetId, Catalog, JobState, TagPath, TagSource, VolumeKind};
use keepsake_core::library::Library;
use keepsake_core::metadata::MetadataService;
use keepsake_core::rebuild::add_volume;
use keepsake_core::render::{Cache, RenderEvent, RenderOptions, RenderService, RenderTools};

struct World {
    _tmp: tempfile::TempDir,
    lib: Library,
    cat: Catalog,
    media: PathBuf,
    cache: Cache,
}

fn jpeg(img: &DynamicImage) -> Vec<u8> {
    keepsake_core::render::still::encode_jpeg(img).unwrap()
}

fn photo(seed: u8, w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
        Rgb([seed.wrapping_add((x / 7) as u8), (y / 5) as u8, 90])
    }))
}

impl World {
    fn new(names: &[&str]) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let media = tmp.path().join("media");
        std::fs::create_dir_all(&media).unwrap();
        for (i, name) in names.iter().enumerate() {
            let file = media.join(name);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, jpeg(&photo((i as u8).wrapping_mul(40), 800, 600))).unwrap();
        }
        Self::of(tmp, media)
    }

    /// A library in `tmp` with the folder `media` added where it is.
    fn of(tmp: tempfile::TempDir, media: PathBuf) -> Self {
        let root = tmp.path().to_path_buf();
        let lib = Library::create(&root.join("lib")).unwrap();
        let mut cat = lib.open_catalog().unwrap();
        // Read-only: the media folder must stay as it was.
        add_volume(
            &mut cat,
            &lib,
            "Media",
            VolumeKind::External,
            &media,
            true,
            None,
        )
        .unwrap();
        let cache = Cache::new(root.join("cache"));
        Self {
            _tmp: tmp,
            lib,
            cat,
            media,
            cache,
        }
    }

    fn asset(&self, name: &str) -> AssetId {
        let path = std::fs::canonicalize(self.media.join(name)).unwrap();
        self.lib.locate(&self.cat, &path).unwrap().unwrap().asset_id
    }

    /// Give `id` a 1024 × 768 thumbnail.
    fn thumb(&mut self, id: AssetId) {
        let hash = keepsake_core::render::source::source_hash(&self.cat, id)
            .unwrap()
            .unwrap();
        self.cache
            .put_bytes(
                &mut self.cat,
                &hash,
                keepsake_core::render::Variant::Thumb1024,
                &jpeg(&photo(7, 1024, 768)),
            )
            .unwrap();
    }

    fn service(&self, loader: ReaderLoader) -> RenderService {
        let tools = RenderTools {
            exiftool: PathBuf::from("none"),
            ffprobe: PathBuf::from("none"),
            ffmpeg: PathBuf::from("none"),
            dcraw: PathBuf::from("none"),
        };
        self.service_with(tools, loader, false)
    }

    fn service_with(&self, tools: RenderTools, loader: ReaderLoader, gpu: bool) -> RenderService {
        let opts = RenderOptions {
            workers: 1,
            proxy_workers: 0,
            max_bytes: u64::MAX,
            pause_on_battery: false,
            until_empty: true,
            forced_encoder: None,
            evict_every: 25,
            ai: false,
            ai_gpu: gpu,
            faces: false,
            aircraft: true,
        };
        RenderService::new(&self.lib, tools, self.cache.clone(), opts).with_aircraft(loader)
    }

    /// The item's keywords, as `A|B|C`.
    fn keywords(&self, id: AssetId) -> Vec<String> {
        self.cat
            .read(|tx| tx.tags().for_asset(id))
            .unwrap()
            .into_iter()
            .filter(|t| t.source == TagSource::User)
            .map(|t| t.path.to_lightroom())
            .collect()
    }

    /// What the read of `id` offers for review at the default confidence,
    /// sorted (empty when it has not been read).
    fn offered(&self, id: AssetId) -> Vec<String> {
        let items = scan::review_items(&self.cat, &[id]).unwrap();
        let mut out: Vec<String> = items
            .first()
            .map(|it| {
                scan::offered(it, &Confidence::default())
                    .unwrap()
                    .iter()
                    .map(TagPath::to_lightroom)
                    .collect()
            })
            .unwrap_or_default();
        out.sort();
        out
    }

    /// Accept everything offered for `id` at the default confidence.
    fn accept_offered(&mut self, id: AssetId) -> Vec<String> {
        let paths: Vec<TagPath> = self
            .offered(id)
            .iter()
            .map(|p| TagPath::parse_lightroom(p).unwrap())
            .collect();
        scan::accept(&mut self.cat, &self.lib, id, &paths).unwrap()
    }

    fn review(&self, id: AssetId) -> Option<scan::Review> {
        scan::review_items(&self.cat, &[id])
            .unwrap()
            .first()
            .map(|it| it.review)
    }

    fn job_state(&self, id: AssetId) -> JobState {
        self.cat
            .read(|tx| tx.jobs().find(JOB_AI_AIRCRAFT, id))
            .unwrap()
            .unwrap()
            .state
    }
}

/// The media folder's files and their hashes.
fn hashes(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| {
            let bytes = std::fs::read(e.path()).unwrap();
            (
                e.file_name().to_string_lossy().into_owned(),
                blake3::hash(&bytes).to_hex().to_string(),
            )
        })
        .collect();
    out.sort();
    out
}

/// Stands in for the text models: every picture shows the same lines; the
/// sizes of the pictures it was given are kept.
struct FakeReader {
    lines: Vec<&'static str>,
    seen: Arc<Mutex<Vec<(u32, u32)>>>,
}

impl Reader for FakeReader {
    fn read(&mut self, img: &DynamicImage) -> Result<Vec<TextLine>, AiError> {
        self.seen.lock().unwrap().push((img.width(), img.height()));
        Ok(self
            .lines
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let (x, h) = (i as f32 * 200.0, 60.0 - i as f32 * 10.0);
                TextLine {
                    text: (*t).to_owned(),
                    score: 0.97,
                    quad: [[x, 0.0], [x + 150.0, 0.0], [x + 150.0, h], [x, h]],
                }
            })
            .collect())
    }

    fn embed(&mut self, _: &DynamicImage) -> Result<Option<Embedding>, AiError> {
        Ok(None)
    }

    fn accel(&self) -> Accel {
        Accel::Cpu
    }
}

fn fake(lines: &[&'static str], seen: &Arc<Mutex<Vec<(u32, u32)>>>) -> ReaderLoader {
    let (lines, seen) = (lines.to_vec(), Arc::clone(seen));
    Arc::new(move |_gpu| {
        Ok(Box::new(FakeReader {
            lines: lines.clone(),
            seen: Arc::clone(&seen),
        }) as Box<dyn Reader + Send>)
    })
}

const AIR_CANADA: [&str; 4] = ["AIR CANADA", "748", "*C-FJZS", "BOEING 777-30DER"];
const FOUND: [&str; 3] = [
    "Aircraft|Airline|Air Canada",
    "Aircraft|Registration|C-FJZS",
    "Aircraft|Type|Boeing 777-300ER",
];

#[test]
fn the_aircraft_worker_reads_the_full_size_picture_and_keeps_it_for_review() {
    let mut w = World::new(&["a.jpg", "b.jpg"]);
    let before = hashes(&w.media);
    let (a, b) = (w.asset("a.jpg"), w.asset("b.jpg"));
    // The thumbnail is there, but the photo itself is read: registrations
    // are too small in a thumbnail.
    w.thumb(a);
    // A keyword of the user's own stays.
    let mine = TagPath::new(["Trips", "Toronto"]).unwrap();
    MetadataService::new(&w.lib, &mut w.cat)
        .set_user_tags(a, std::slice::from_ref(&mine))
        .unwrap();
    assert_eq!(scan::enqueue(&mut w.cat, &[a]).unwrap(), 1);
    let (st, problem) = scan::status(&w.cat, &[a, b]).unwrap();
    assert_eq!((st.items, st.done, st.pending, st.failed), (2, 0, 1, 0));
    assert_eq!(problem, None);

    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut events = Vec::new();
    w.service(fake(&AIR_CANADA, &seen))
        .run(&AtomicBool::new(false), &mut |e| events.push(e.clone()))
        .unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, RenderEvent::AircraftReady { accel: Accel::Cpu })));
    assert_eq!(*seen.lock().unwrap(), [(800, 600)], "the photo, not b");
    // Reading adds no keyword: the read waits for review.
    assert_eq!(w.keywords(a), ["Trips|Toronto"]);
    assert!(w.cat.read(|tx| tx.ai().tag_sources(a)).unwrap().is_empty());
    assert_eq!(w.review(a), Some(scan::Review::Waiting));
    assert_eq!(w.review(b), None, "b was not read");
    assert_eq!(w.offered(a), FOUND);
    // The job says what it found.
    let made = events.iter().find_map(|e| match e {
        RenderEvent::Done { kind, made, .. } if kind == JOB_AI_AIRCRAFT => Some(made.clone()),
        _ => None,
    });
    let mut made = made.unwrap();
    made.sort();
    assert_eq!(made, FOUND);
    // Accepted: the keywords are added, and who gave each is recorded.
    let mut added = w.accept_offered(a);
    added.sort();
    assert_eq!(added, FOUND);
    let mut want: Vec<String> = FOUND.iter().map(|s| (*s).to_owned()).collect();
    want.push("Trips|Toronto".to_owned());
    assert_eq!(w.keywords(a), want);
    assert_eq!(w.review(a), Some(scan::Review::Accepted));
    let sources: Vec<(String, String)> = w
        .cat
        .read(|tx| tx.ai().tag_sources(a))
        .unwrap()
        .into_iter()
        .map(|(by, p)| (by, p.to_lightroom()))
        .collect();
    assert_eq!(sources.len(), 3);
    assert!(sources.iter().all(|(by, _)| by == SOURCE));
    assert_eq!(w.job_state(a), JobState::Done);
    let (st, _) = scan::status(&w.cat, &[a]).unwrap();
    assert_eq!((st.done, st.pending), (1, 0));

    // Read again: it waits for review again; accepting adds nothing twice.
    scan::enqueue(&mut w.cat, &[a]).unwrap();
    w.service(fake(&AIR_CANADA, &seen))
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(w.review(a), Some(scan::Review::Waiting));
    assert_eq!(w.keywords(a), want);
    assert!(w.accept_offered(a).is_empty());
    assert_eq!(w.keywords(a), want);
    // Dismissed: nothing is added; accepting nothing dismisses too.
    scan::dismiss(&mut w.cat, a).unwrap();
    assert_eq!(w.review(a), Some(scan::Review::Dismissed));
    // Only aircraft keywords can be accepted.
    let other = TagPath::new(["People", "Someone"]).unwrap();
    assert!(scan::accept(&mut w.cat, &w.lib, a, &[other]).is_err());
    assert_eq!(w.keywords(a), want);
    assert_eq!(hashes(&w.media), before, "originals unchanged");
}

#[test]
fn a_photo_whose_file_is_away_is_read_from_its_thumbnail_or_waits() {
    let mut w = World::new(&["a.jpg", "b.jpg"]);
    let (a, b) = (w.asset("a.jpg"), w.asset("b.jpg"));
    w.thumb(a);
    // The drive is not connected (the test's stand-in: the files are gone).
    std::fs::remove_file(w.media.join("a.jpg")).unwrap();
    std::fs::remove_file(w.media.join("b.jpg")).unwrap();
    scan::enqueue(&mut w.cat, &[a, b]).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    w.service(fake(&["KLM", "PH-BQB"], &seen))
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(*seen.lock().unwrap(), [(1024, 768)], "a's thumbnail");
    assert_eq!(
        w.offered(a),
        ["Aircraft|Airline|KLM", "Aircraft|Registration|PH-BQB"]
    );
    assert!(w.keywords(a).is_empty(), "nothing until accepted");
    // b has no thumbnail either: it waits, it has not failed.
    assert_eq!(w.job_state(a), JobState::Done);
    assert_eq!(w.job_state(b), JobState::Queued);
    let (st, _) = scan::status(&w.cat, &[a, b]).unwrap();
    assert_eq!((st.done, st.pending, st.failed), (1, 1, 0));
    // Stop drops what is still waiting.
    assert_eq!(scan::stop(&mut w.cat, &[a, b]).unwrap(), 1);
    let (st, _) = scan::status(&w.cat, &[a, b]).unwrap();
    assert_eq!((st.done, st.pending), (1, 0));
}

#[test]
fn a_photo_whose_thumbnail_cannot_be_made_fails_once_and_is_not_tried_again() {
    use keepsake_core::catalog::Priority;
    use keepsake_core::render::JOB_STILLS;
    let mut w = World::new(&["a.jpg"]);
    let a = w.asset("a.jpg");
    // Its thumbnail failed for good (a raw file without a built-in preview).
    keepsake_core::render::enqueue(&mut w.cat, &[a], Priority::Recent, false).unwrap();
    w.cat
        .write(|tx| {
            let job = tx.jobs().find(JOB_STILLS, a)?.unwrap();
            tx.jobs()
                .fail(job.id, "cannot render a.dng (no embedded preview)", None, 0)
        })
        .unwrap();
    let stills = |w: &World| {
        w.cat
            .read(|tx| tx.jobs().find(JOB_STILLS, a))
            .unwrap()
            .unwrap()
    };
    let before = stills(&w);
    assert_eq!(before.state, JobState::Failed);
    // The drive is away, so the reader wants the thumbnail.
    std::fs::remove_file(w.media.join("a.jpg")).unwrap();
    scan::enqueue(&mut w.cat, &[a]).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut events = Vec::new();
    w.service(fake(&["KLM"], &seen))
        .run(&AtomicBool::new(false), &mut |e| events.push(e.clone()))
        .unwrap();
    // The read fails, saying why; the thumbnail job is left as it was
    // (queued again, it would fail the same way every few seconds).
    assert_eq!(w.job_state(a), JobState::Failed);
    assert!(events.iter().any(|e| matches!(
        e,
        RenderEvent::Failed { error, retry: false, .. }
            if error.contains("no thumbnail can be made") && error.contains("no embedded preview")
    )));
    assert_eq!(stills(&w), before);
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn a_picture_with_nothing_to_read_gets_no_keywords_and_missing_models_are_said() {
    let mut w = World::new(&["a.jpg"]);
    let a = w.asset("a.jpg");
    scan::enqueue(&mut w.cat, &[a]).unwrap();
    // The models are not installed: the worker says so, the job stays.
    let missing: ReaderLoader = Arc::new(|_| Err(AiError::ModelMissing(PathBuf::from("det.onnx"))));
    let mut events = Vec::new();
    w.service(missing)
        .run(&AtomicBool::new(false), &mut |e| events.push(e.clone()))
        .unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        RenderEvent::AircraftUnavailable { reason } if reason.contains("det.onnx")
    )));
    assert_eq!(w.job_state(a), JobState::Queued);
    // With them, a picture of something else is read and gets nothing.
    let seen = Arc::new(Mutex::new(Vec::new()));
    w.service(fake(&["NO STEP", "ahmad el-kays"], &seen))
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(w.job_state(a), JobState::Done);
    assert!(w.offered(a).is_empty());
    assert!(w.keywords(a).is_empty());
    assert!(w.cat.read(|tx| tx.ai().tag_sources(a)).unwrap().is_empty());
    // Nothing found: nothing to review, so it is set aside.
    let read = w.cat.read(|tx| tx.ai().aircraft_read(a)).unwrap().unwrap();
    assert_eq!(read.review, "dismissed");
}

#[test]
fn reads_that_found_nothing_are_set_aside_and_not_counted_as_waiting() {
    let mut w = World::new(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
    let [a, b, c, d] = ["a.jpg", "b.jpg", "c.jpg", "d.jpg"].map(|n| w.asset(n));
    let empty = r#"{"registrations":[],"airlines":[],"types":[]}"#;
    let airline = r#"{"registrations":[],"airlines":[["Interjet",0.9]],"types":[]}"#;
    w.cat
        .write(|tx| {
            // Read before reads that found nothing were set aside.
            tx.ai().put_aircraft_read(a, empty, None, true, 10)?;
            tx.ai().put_aircraft_read(b, airline, None, true, 10)?;
            // Only the type it looks like: there is something to offer.
            tx.ai()
                .put_aircraft_read(c, empty, Some("Boeing 777"), true, 10)?;
            tx.ai().put_aircraft_read(d, empty, None, true, 10)?;
            tx.ai().set_aircraft_review(d, "accepted", 11)?;
            Ok::<_, keepsake_core::catalog::CatalogError>(())
        })
        .unwrap();
    let waiting = |w: &World| {
        w.cat
            .read(|tx| tx.aircraft_report().aircraft())
            .unwrap()
            .totals
            .waiting_review
    };
    // The report leaves a read that found nothing out already.
    assert_eq!(waiting(&w), 2);
    assert_eq!(scan::dismiss_empty(&mut w.cat, &[a, b, c, d]).unwrap(), 1);
    let review = |w: &World, id: AssetId| {
        w.cat
            .read(|tx| tx.ai().aircraft_read(id))
            .unwrap()
            .unwrap()
            .review
    };
    assert_eq!(review(&w, a), "dismissed");
    assert_eq!(review(&w, b), "waiting");
    assert_eq!(review(&w, c), "waiting");
    // One already reviewed keeps its review.
    assert_eq!(review(&w, d), "accepted");
    assert_eq!(waiting(&w), 2);
    assert_eq!(scan::dismiss_empty(&mut w.cat, &[a, b, c, d]).unwrap(), 0);
}

#[test]
fn a_folder_is_read_for_its_aircraft_photos_only_and_each_once() {
    use keepsake_core::ai::Target;
    let mut w = World::new(&["air/a.jpg", "air/deep/b.jpg", "air/c.jpg", "other/d.jpg"]);
    let (a, b, c, d) = (
        w.asset("air/a.jpg"),
        w.asset("air/deep/b.jpg"),
        w.asset("air/c.jpg"),
        w.asset("other/d.jpg"),
    );
    let volume = {
        let path = std::fs::canonicalize(w.media.join("air/a.jpg")).unwrap();
        w.lib.locate(&w.cat, &path).unwrap().unwrap().volume_id
    };
    // a and d: tagged Airplane by the on-device AI; b: the user's own
    // keyword, in a folder below; c: a photo of something else.
    let tag = |t: &str| TagPath::new([t]).unwrap();
    let mut svc = MetadataService::new(&w.lib, &mut w.cat);
    svc.set_ai_tags(a, &[(tag("Airplane"), 0.4)]).unwrap();
    svc.set_ai_tags(d, &[(tag("Airplane"), 0.4)]).unwrap();
    svc.set_user_tags(b, &[tag("Helicopter")]).unwrap();
    svc.set_ai_tags(c, &[(tag("Dog"), 0.9)]).unwrap();

    let air = Target::Folder {
        volume,
        folder: "air".into(),
    };
    let mut items = scan::target_items(&w.cat, &air).unwrap();
    items.sort();
    let mut want = vec![a, b];
    want.sort();
    assert_eq!(items, want, "only the tagged ones, the folder below too");
    let st = scan::enqueue_target(&mut w.cat, &air).unwrap();
    assert_eq!((st.items, st.analysed, st.pending), (2, 0, 2));

    let seen = Arc::new(Mutex::new(Vec::new()));
    w.service(fake(&["KLM", "PH-BQB"], &seen))
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(seen.lock().unwrap().len(), 2);
    for id in [a, b] {
        assert!(w
            .offered(id)
            .contains(&"Aircraft|Registration|PH-BQB".to_owned()));
    }
    assert!(w.offered(c).is_empty() && w.offered(d).is_empty());
    let st = scan::target_status(&w.cat, &air).unwrap();
    assert_eq!((st.items, st.analysed, st.pending), (2, 2, 0));

    // Asked again: the photos already read are not read again.
    let st = scan::enqueue_target(&mut w.cat, &air).unwrap();
    assert_eq!((st.analysed, st.pending), (2, 0));
    w.service(fake(&["KLM"], &seen))
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(seen.lock().unwrap().len(), 2);

    // A photo read before reads were kept (its job done, no read) is read
    // again when the folder is asked for.
    rusqlite::Connection::open(w.lib.catalog_path())
        .unwrap()
        .execute("DELETE FROM aircraft_reads WHERE asset_id = ?1", [b])
        .unwrap();
    let st = scan::enqueue_target(&mut w.cat, &air).unwrap();
    assert_eq!((st.analysed, st.pending), (1, 1));
    w.service(fake(&["KLM"], &seen))
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(seen.lock().unwrap().len(), 3);
    assert_eq!(w.offered(b), ["Aircraft|Airline|KLM"]);

    // The whole drive: d is the one left; Stop drops it.
    let all = Target::Folder {
        volume,
        folder: String::new(),
    };
    let st = scan::enqueue_target(&mut w.cat, &all).unwrap();
    assert_eq!((st.items, st.analysed, st.pending), (3, 2, 1));
    assert_eq!(scan::stop_target(&mut w.cat, &all).unwrap(), 1);
    let st = scan::target_status(&w.cat, &all).unwrap();
    assert_eq!((st.items, st.analysed, st.pending), (3, 2, 0));
    assert!(w.keywords(d).is_empty());
}

#[test]
fn a_folder_read_again_keeps_its_reviews_and_learns_its_fins() {
    use keepsake_core::ai::Target;
    let mut w = World::new(&["air/a.jpg", "air/b.jpg", "air/c.jpg"]);
    let [a, b, c] = ["air/a.jpg", "air/b.jpg", "air/c.jpg"].map(|n| w.asset(n));
    let volume = {
        let path = std::fs::canonicalize(w.media.join("air/a.jpg")).unwrap();
        w.lib.locate(&w.cat, &path).unwrap().unwrap().volume_id
    };
    let tag = |t: &str| TagPath::new([t]).unwrap();
    let mut svc = MetadataService::new(&w.lib, &mut w.cat);
    for id in [a, b, c] {
        svc.set_ai_tags(id, &[(tag("Airplane"), 0.4)]).unwrap();
    }
    let air = Target::Folder {
        volume,
        folder: "air".into(),
    };
    // Read before fins were read: the airline only.
    scan::enqueue_target(&mut w.cat, &air).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    w.service(fake(&["AIR CANADA"], &seen))
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    // a accepted with its registration (typed), c dismissed, b waits.
    let p = |s: &str| TagPath::parse_lightroom(s).unwrap();
    let keep = [
        p("Aircraft|Registration|C-GJYI"),
        p("Aircraft|Airline|Air Canada"),
    ];
    scan::accept(&mut w.cat, &w.lib, a, &keep).unwrap();
    scan::dismiss(&mut w.cat, c).unwrap();
    let review = |w: &World, id: AssetId| {
        w.cat
            .read(|tx| tx.ai().aircraft_read(id))
            .unwrap()
            .map(|r| r.review)
            .unwrap_or_default()
    };
    // The folder's Read aircraft skips them; read again, all are read.
    let st = scan::enqueue_target(&mut w.cat, &air).unwrap();
    assert_eq!(st.pending, 0);
    let st = scan::reread_target(&mut w.cat, &air).unwrap();
    assert_eq!((st.items, st.pending), (3, 3));
    w.service(fake(&["AIR CANADA", "147"], &seen))
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(seen.lock().unwrap().len(), 6);
    // Reviews kept: a accepted, c dismissed, b still waiting.
    assert_eq!(review(&w, a), "accepted");
    assert_eq!(review(&w, c), "dismissed");
    assert_eq!(review(&w, b), "waiting");
    // a's new read has its fin, and a is tagged: the table learned it.
    let rows = w.cat.read(|tx| tx.ai().fins_all()).unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        (rows[0].fin.as_str(), rows[0].registration.as_str()),
        ("147", "C-GJYI")
    );
    // b now offers C-GJYI from the fin.
    let it = scan::review_items(&w.cat, &[b]).unwrap().remove(0);
    let (o, _) = scan::offer_full(&w.cat, &it, &Confidence::default()).unwrap();
    assert_eq!(
        o.from_fin.map(|f| f.registration).as_deref(),
        Some("C-GJYI")
    );
    // Picked and read again (not the folder's): a waits for review again.
    scan::reread_target(&mut w.cat, &air).unwrap();
    scan::enqueue(&mut w.cat, &[a]).unwrap();
    w.service(fake(&["AIR CANADA", "147"], &seen))
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(review(&w, a), "waiting");
    assert_eq!(review(&w, c), "dismissed");
}

/// The real models on real pictures (see the top of this file). Prints
/// what each picture got; checks every picture was read, at full size,
/// and that the folder is as it was.
#[test]
#[ignore = "needs KEEPSAKE_AIRCRAFT_SAMPLES and the models in vendor/models"]
fn the_real_models_read_real_photos() {
    let Some(dir) = std::env::var_os("KEEPSAKE_AIRCRAFT_SAMPLES").map(PathBuf::from) else {
        eprintln!("KEEPSAKE_AIRCRAFT_SAMPLES is not set: skipped");
        return;
    };
    let before = hashes(&dir);
    let mut w = World::of(tempfile::tempdir().unwrap(), dir.clone());
    let ids: Vec<AssetId> = w
        .cat
        .read(|tx| {
            tx.ai()
                .matching(&Default::default(), keepsake_core::ai::MODEL_ID)
        })
        .unwrap()
        .into_iter()
        .map(|it| it.id)
        .collect();
    assert!(!ids.is_empty(), "no pictures in {}", dir.display());
    scan::enqueue(&mut w.cat, &ids).unwrap();
    let mut problem = None;
    w.service_with(RenderTools::discover(), scan::installed_loader(), true)
        .run(&AtomicBool::new(false), &mut |e| match e {
            RenderEvent::AircraftReady { accel } => eprintln!("on the {}", accel.describe()),
            RenderEvent::AircraftUnavailable { reason } => problem = Some(reason.clone()),
            RenderEvent::Done { kind, ms, .. } if kind == JOB_AI_AIRCRAFT => {
                eprintln!("read in {ms} ms")
            }
            RenderEvent::Failed { error, .. } => eprintln!("failed: {error}"),
            _ => {}
        })
        .unwrap();
    assert_eq!(problem, None, "the models could not be loaded");
    for id in &ids {
        eprintln!("{id}: {:?}", w.keywords(*id));
    }
    let (st, why) = scan::status(&w.cat, &ids).unwrap();
    assert_eq!(
        (st.done, st.pending, st.failed),
        (st.items, 0, 0),
        "{why:?}"
    );
    assert_eq!(hashes(&dir), before, "originals unchanged");
}

/// Stands in for adsbdb: knows C-FJZS (an Air Canada 777-200LR); counts
/// what it is asked.
struct FakeLookup {
    asked: Mutex<Vec<String>>,
}

impl lookup::Lookup for FakeLookup {
    fn lookup(&self, registration: &str) -> Result<lookup::Found, AiError> {
        self.asked.lock().unwrap().push(registration.to_owned());
        let body = r#"{"response":{"aircraft":{"type":"777 233LR","icao_type":"B77L","manufacturer":"Boeing","registered_owner":"Air Canada","registered_owner_country_name":"Canada"}}}"#;
        let status = if registration == "C-FJZS" { 200 } else { 404 };
        lookup::parse(
            registration,
            status,
            body,
            &keepsake_core::ai::aircraft::marks::Airlines::builtin(),
        )
    }
}

#[test]
fn registrations_are_looked_up_once_and_offered_beside_what_was_read() {
    let mut w = World::new(&["a.jpg"]);
    let a = w.asset("a.jpg");
    scan::enqueue(&mut w.cat, &[a]).unwrap();
    let registry = Arc::new(FakeLookup {
        asked: Mutex::new(Vec::new()),
    });
    let seen = Arc::new(Mutex::new(Vec::new()));
    // Painted: a 777-300ER; the registry says 777-200LR.
    w.service(fake(&AIR_CANADA, &seen))
        .with_lookup(registry.clone())
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(*registry.asked.lock().unwrap(), ["C-FJZS"]);
    let found = lookup::kept(&w.cat, "C-FJZS").unwrap().unwrap();
    assert_eq!(found.aircraft_type.as_deref(), Some("Boeing 777-200LR"));

    let item = &scan::review_items(&w.cat, &[a]).unwrap()[0];
    let conf = Confidence::default();
    let found = scan::found_for(&w.cat, item, &conf).unwrap();
    let offer = scan::offer(item, &conf, &found).unwrap();
    let names = |v: &[TagPath]| -> Vec<String> { v.iter().map(TagPath::to_lightroom).collect() };
    assert_eq!(
        names(&offer.keywords),
        [
            "Aircraft|Registration|C-FJZS",
            "Aircraft|Airline|Air Canada",
            "Aircraft|Type|Boeing 777-300ER",
            "Aircraft|Type|Boeing 777-200LR",
        ]
    );
    // The lookup gave the airline (also read) and its type; the painted
    // type it disagrees with starts unticked.
    assert_eq!(
        names(&offer.from_lookup),
        [
            "Aircraft|Airline|Air Canada",
            "Aircraft|Type|Boeing 777-200LR"
        ]
    );
    assert_eq!(names(&offer.unticked), ["Aircraft|Type|Boeing 777-300ER"]);

    // Read again: kept, not asked again.
    scan::enqueue(&mut w.cat, &[a]).unwrap();
    w.service(fake(&AIR_CANADA, &seen))
        .with_lookup(registry.clone())
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(registry.asked.lock().unwrap().len(), 1);
}

#[test]
fn photos_moments_apart_from_the_same_camera_are_neighbours_when_alike() {
    use keepsake_core::ai::Embedding;
    use keepsake_core::catalog::AiAnalysis;
    let mut w = World::new(&["a.jpg", "b.jpg", "c.jpg", "d.jpg", "e.jpg"]);
    let [a, b, c, d, e] = ["a.jpg", "b.jpg", "c.jpg", "d.jpg", "e.jpg"].map(|n| w.asset(n));
    // a at t; b 3 s after and alike; c 6 s before, a different plane;
    // d 30 s after; e 2 s after, another camera.
    let db = rusqlite::Connection::open(w.lib.catalog_path()).unwrap();
    for (id, t, cam) in [
        (a, 1_000_000i64, "ILCE-7M3"),
        (b, 1_003_000, "ILCE-7M3"),
        (c, 994_000, "ILCE-7M3"),
        (d, 1_030_000, "ILCE-7M3"),
        (e, 1_002_000, "ILCE-6400"),
    ] {
        db.execute(
            "UPDATE assets SET capture_ts_ms = ?2, camera_make = 'SONY', camera_model = ?3 WHERE id = ?1",
            rusqlite::params![id, t, cam],
        )
        .unwrap();
    }
    drop(db);
    let vec = |k: f32| Embedding::new((0..64).map(|i| ((i as f32) * k).sin()).collect()).unwrap();
    for (id, v) in [(a, vec(0.3)), (b, vec(0.301)), (c, vec(1.7))] {
        let (q, scale) = v.quantize();
        w.cat
            .write(|tx| {
                tx.ai().put_analysis(&AiAnalysis {
                    asset_id: id,
                    model: keepsake_core::ai::MODEL_ID.to_owned(),
                    embedding: q.clone(),
                    emb_scale: scale,
                    dhash: 0,
                    made_ms: 0,
                    tag_list: 0,
                })
            })
            .unwrap();
    }
    let n = scan::neighbours(&w.cat, a, scan::NEIGHBOUR_MS).unwrap();
    let ids: Vec<AssetId> = n.iter().map(|x| x.asset).collect();
    assert_eq!(
        ids,
        [b, c],
        "nearest first; not d (30 s) nor e (another camera)"
    );
    assert_eq!(n[0].ms, 3_000);
    assert_eq!(n[1].ms, -6_000);
    assert!(n[0].alike && n[0].similarity.unwrap() > 0.99);
    assert!(!n[1].alike, "{:?}", n[1].similarity);
    // A wider window reaches d (30 s after); another lens is not the same shot.
    let wide = scan::neighbours(&w.cat, a, 40_000).unwrap();
    assert!(wide.iter().any(|x| x.asset == d));
    rusqlite::Connection::open(w.lib.catalog_path())
        .unwrap()
        .execute(
            "UPDATE assets SET lens_model = 'FE 100-400mm' WHERE id = ?1",
            [b],
        )
        .unwrap();
    let other_lens = scan::neighbours(&w.cat, a, scan::NEIGHBOUR_MS).unwrap();
    assert!(other_lens.iter().all(|x| x.asset != b));
    // A photo with no capture time has none.
    assert!(scan::neighbours(&w.cat, d, scan::NEIGHBOUR_MS)
        .unwrap()
        .iter()
        .all(|x| x.asset != e));
}

#[test]
fn other_photos_of_the_same_registration_get_what_they_lack() {
    let mut w = World::new(&["a.jpg", "b.jpg", "c.jpg"]);
    let [a, b, c] = ["a.jpg", "b.jpg", "c.jpg"].map(|n| w.asset(n));
    let p = |s: &str| TagPath::parse_lightroom(s).unwrap();
    let reg = p("Aircraft|Registration|C-FJZS");
    let airline = p("Aircraft|Airline|Air Canada");
    let kind = p("Aircraft|Type|Boeing 777-300ER");
    let mut svc = MetadataService::new(&w.lib, &mut w.cat);
    svc.set_user_tags(a, &[reg.clone(), airline.clone(), kind.clone()])
        .unwrap();
    svc.set_user_tags(b, std::slice::from_ref(&reg)).unwrap();
    svc.set_user_tags(c, &[reg.clone(), airline.clone()])
        .unwrap();
    let wanted = [airline.clone(), kind.clone()];
    let mut others = scan::same_registration(&w.cat, "C-FJZS", &wanted, &[a]).unwrap();
    others.sort_by_key(|(id, _)| *id);
    let mut want = vec![
        (b, vec![airline.clone(), kind.clone()]),
        (c, vec![kind.clone()]),
    ];
    want.sort_by_key(|(id, _)| *id);
    assert_eq!(others, want);
    assert_eq!(
        scan::add_to(&mut w.cat, &w.lib, &[b, c], &wanted).unwrap(),
        2
    );
    assert!(scan::same_registration(&w.cat, "C-FJZS", &wanted, &[a])
        .unwrap()
        .is_empty());
    assert!(w
        .keywords(b)
        .contains(&"Aircraft|Type|Boeing 777-300ER".to_owned()));
    // Only aircraft keywords.
    assert!(scan::add_to(&mut w.cat, &w.lib, &[b], &[p("People|Someone")]).is_err());
}

#[test]
fn neighbours_accepted_with_a_photo_are_not_asked_about_again() {
    let mut w = World::new(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
    let [a, b, c, d] = ["a.jpg", "b.jpg", "c.jpg", "d.jpg"].map(|n| w.asset(n));
    let read = |w: &mut World, id: AssetId, made: i64| {
        w.cat
            .write(|tx| tx.ai().put_aircraft_read(id, "{}", None, true, made))
            .unwrap();
    };
    // a and b are read and wait for review; c, beside a, is not read yet.
    read(&mut w, a, 10);
    read(&mut w, b, 10);
    read(&mut w, d, 10);
    let review = |w: &World, id: AssetId| {
        w.cat
            .read(|tx| tx.ai().aircraft_read(id))
            .unwrap()
            .map(|r| r.review)
    };
    let p = |s: &str| TagPath::parse_lightroom(s).unwrap();
    let keep = [p("Aircraft|Registration|C-FJZS")];
    scan::accept(&mut w.cat, &w.lib, a, &keep).unwrap();
    assert_eq!(
        scan::accept_neighbours(&mut w.cat, &w.lib, &[b, c], &keep).unwrap(),
        [b, c]
    );
    for id in [a, b, c] {
        assert_eq!(review(&w, id).as_deref(), Some("accepted"));
    }
    assert!(w
        .keywords(c)
        .contains(&"Aircraft|Registration|C-FJZS".to_owned()));
    // d was not ticked: it still waits.
    assert_eq!(review(&w, d).as_deref(), Some("waiting"));
    // c is read later (its job was queued): it stays accepted.
    read(&mut w, c, 20);
    assert_eq!(review(&w, c).as_deref(), Some("accepted"));
    // Reading again a photo read before asks for its review again.
    read(&mut w, b, 30);
    assert_eq!(review(&w, b).as_deref(), Some("waiting"));
    read(&mut w, c, 40);
    assert_eq!(review(&w, c).as_deref(), Some("waiting"));
    // A neighbour already dismissed keeps that (it still gets the keywords).
    scan::dismiss(&mut w.cat, d).unwrap();
    assert!(scan::accept_neighbours(&mut w.cat, &w.lib, &[d], &keep)
        .unwrap()
        .is_empty());
    assert_eq!(review(&w, d).as_deref(), Some("dismissed"));
    assert!(w
        .keywords(d)
        .contains(&"Aircraft|Registration|C-FJZS".to_owned()));
    read(&mut w, d, 50);
    // A dismissed photo gives its neighbours nothing.
    assert!(scan::accept_neighbours(&mut w.cat, &w.lib, &[d], &[])
        .unwrap()
        .is_empty());
    assert_eq!(review(&w, d).as_deref(), Some("waiting"));
}

#[test]
fn a_photo_accepted_with_its_fin_read_teaches_the_fin_table() {
    use keepsake_core::ai::aircraft::fins;
    let names = ["a.jpg", "b.jpg", "c.jpg", "d.jpg"];
    let mut w = World::new(&names);
    let [a, b, c, d] = names.map(|n| w.asset(n));
    let db = rusqlite::Connection::open(w.lib.catalog_path()).unwrap();
    for (id, day) in [(a, 0), (b, 0), (c, 3), (d, 9)] {
        db.execute(
            "UPDATE assets SET capture_ts_ms = ?2 WHERE id = ?1",
            rusqlite::params![id, (PHOTO_S + day * 86_400) * 1000],
        )
        .unwrap();
    }
    drop(db);
    let read = |w: &mut World, id: AssetId, fins: &str| {
        let json = format!(
            r#"{{"registrations":[],"airlines":[["Air Canada",0.98]],"types":[],"fins":{fins}}}"#
        );
        w.cat
            .write(|tx| tx.ai().put_aircraft_read(id, &json, None, true, 10))
            .unwrap();
    };
    // a: the tail's 147 read surely; b (its burst): none; c, days later:
    // 147 again; d: read unsurely.
    read(&mut w, a, r#"[["147",0.97]]"#);
    read(&mut w, b, "[]");
    read(&mut w, c, r#"[["147",0.95]]"#);
    read(&mut w, d, r#"[["147",0.6]]"#);
    let p = |s: &str| TagPath::parse_lightroom(s).unwrap();
    let keep = [
        p("Aircraft|Registration|C-GJYI"),
        p("Aircraft|Airline|Air Canada"),
        p("Aircraft|Type|Airbus A220-300"),
    ];
    scan::accept(&mut w.cat, &w.lib, a, &keep).unwrap();
    scan::accept_neighbours(&mut w.cat, &w.lib, &[b], &keep).unwrap();
    let all = |w: &World| w.cat.read(|tx| tx.ai().fins_all()).unwrap();
    let rows = all(&w);
    assert_eq!(rows.len(), 1, "{rows:?}");
    let r = &rows[0];
    assert_eq!(
        (r.airline.as_str(), r.fin.as_str(), r.registration.as_str()),
        ("Air Canada", "147", "C-GJYI")
    );
    assert_eq!((r.source.as_str(), r.seen), ("learned", 1));
    assert_eq!(
        (r.first_ms, r.last_ms),
        (Some(PHOTO_S * 1000), Some(PHOTO_S * 1000))
    );
    // Days later: the same pair, its dates stretched.
    scan::accept(&mut w.cat, &w.lib, c, &keep).unwrap();
    let r = &all(&w)[0];
    assert_eq!(r.seen, 2);
    assert_eq!(r.last_ms, Some((PHOTO_S + 3 * 86_400) * 1000));
    // An unsure fin, or no airline accepted, teaches nothing.
    scan::accept(&mut w.cat, &w.lib, d, &keep).unwrap();
    assert_eq!(all(&w)[0].seen, 2);
    read(&mut w, d, r#"[["401",0.99]]"#);
    scan::accept(&mut w.cat, &w.lib, d, &keep[..1]).unwrap();
    assert_eq!(all(&w).len(), 1);
    // An imported file adds beside what was learned; again, it updates.
    let parsed = fins::parse_csv(
        "Air Canada,147,C-GJYI,2019\nAir Canada,401,C-FPWD\n",
        &keepsake_core::ai::aircraft::marks::Airlines::builtin(),
    );
    assert!(parsed.problems.is_empty());
    assert_eq!(fins::import(&mut w.cat, &parsed.rows).unwrap(), (2, 0));
    assert_eq!(fins::import(&mut w.cat, &parsed.rows).unwrap(), (0, 2));
    let rows = all(&w);
    assert_eq!(rows.len(), 3);
    assert_eq!(
        w.cat
            .read(|tx| tx.ai().fins_for("Air Canada", "147"))
            .unwrap()
            .len(),
        2
    );
    assert!(fins::conflicts(&rows).iter().all(|c| !c));
    // Forgotten one by one.
    assert!(w
        .cat
        .write(|tx| tx
            .ai()
            .forget_fin("Air Canada", "401", "C-FPWD", "imported"))
        .unwrap());
    assert_eq!(all(&w).len(), 2);
}

#[test]
fn the_fin_table_is_filled_from_photos_already_tagged() {
    use keepsake_core::ai::aircraft::fins;
    let names = ["a.jpg", "b.jpg", "c.jpg", "d.jpg"];
    let mut w = World::new(&names);
    let [a, b, c, d] = names.map(|n| w.asset(n));
    let p = |s: &str| TagPath::parse_lightroom(s).unwrap();
    let tag = |w: &mut World, id: AssetId, paths: &[TagPath]| {
        MetadataService::new(&w.lib, &mut w.cat)
            .set_user_tags(id, paths)
            .unwrap();
    };
    let read = |w: &mut World, id: AssetId, fins: &str| {
        let json = format!(r#"{{"registrations":[],"airlines":[],"types":[],"fins":{fins}}}"#);
        w.cat
            .write(|tx| tx.ai().put_aircraft_read(id, &json, None, true, 10))
            .unwrap();
    };
    let ac = p("Aircraft|Airline|Air Canada");
    // a: tagged C-GJYI and Air Canada, its read kept fin 147.
    tag(&mut w, a, &[p("Aircraft|Registration|C-GJYI"), ac.clone()]);
    read(&mut w, a, r#"[["147",0.97]]"#);
    // b: tagged too, but read before fins were kept.
    tag(&mut w, b, &[p("Aircraft|Registration|C-FPWD"), ac.clone()]);
    w.cat
        .write(|tx| {
            tx.ai().put_aircraft_read(
                b,
                r#"{"registrations":[],"airlines":[],"types":[]}"#,
                None,
                true,
                10,
            )
        })
        .unwrap();
    // c: a fin read, but no airline tagged.
    tag(&mut w, c, &[p("Aircraft|Registration|C-GHKR")]);
    read(&mut w, c, r#"[["951",0.99]]"#);
    // d: a fin read, nothing tagged.
    read(&mut w, d, r#"[["401",0.99]]"#);
    assert_eq!(fins::learn_from_library(&mut w.cat).unwrap(), 1);
    let rows = w.cat.read(|tx| tx.ai().fins_all()).unwrap();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(
        (
            rows[0].fin.as_str(),
            rows[0].registration.as_str(),
            rows[0].seen
        ),
        ("147", "C-GJYI", 1)
    );
    // Gone over again: nothing counted twice.
    fins::learn_from_library(&mut w.cat).unwrap();
    let rows = w.cat.read(|tx| tx.ai().fins_all()).unwrap();
    assert_eq!((rows.len(), rows[0].seen), (1, 1));
    // Exported where the user chose: it reads back.
    let out = w.media.join("fins.csv");
    fins::write_csv(&rows, &out).unwrap();
    let text = std::fs::read_to_string(&out).unwrap();
    let back = fins::parse_csv(
        &text,
        &keepsake_core::ai::aircraft::marks::Airlines::builtin(),
    );
    assert!(back.problems.is_empty(), "{:?}", back.problems);
    assert_eq!(back.rows[0].registration, "C-GJYI");
}

#[test]
fn a_photo_with_its_airline_and_fin_read_is_offered_the_registration() {
    use keepsake_core::ai::aircraft::fins;
    let names = ["a.jpg", "b.jpg", "c.jpg"];
    let mut w = World::new(&names);
    let [a, b, c] = names.map(|n| w.asset(n));
    let db = rusqlite::Connection::open(w.lib.catalog_path()).unwrap();
    for id in [a, b, c] {
        db.execute(
            "UPDATE assets SET capture_ts_ms = ?2 WHERE id = ?1",
            rusqlite::params![id, PHOTO_S * 1000],
        )
        .unwrap();
    }
    drop(db);
    let read = |w: &mut World, id: AssetId, regs: &str, looks: Option<&str>| {
        let json = format!(
            r#"{{"registrations":{regs},"airlines":[["Air Canada",0.98]],"types":[],"fins":[["147",0.97]]}}"#
        );
        w.cat
            .write(|tx| tx.ai().put_aircraft_read(id, &json, looks, true, 10))
            .unwrap();
    };
    let offer = |w: &World, id: AssetId| {
        let it = scan::review_items(&w.cat, &[id]).unwrap().remove(0);
        scan::offer_full(&w.cat, &it, &Confidence::default())
            .unwrap()
            .0
    };
    let reg = TagPath::parse_lightroom("Aircraft|Registration|C-GJYI").unwrap();
    // a: the airline and its fin read, no registration; the table knows 147.
    read(&mut w, a, "[]", Some("Airbus A220"));
    // With an empty table nothing is offered from the fin.
    assert_eq!(offer(&w, a).from_fin, None);
    let parsed = fins::parse_csv(
        "Air Canada,147,C-GJYI,2019\n",
        &keepsake_core::ai::aircraft::marks::Airlines::builtin(),
    );
    fins::import(&mut w.cat, &parsed.rows).unwrap();
    let o = offer(&w, a);
    let f = o.from_fin.clone().unwrap();
    assert_eq!(
        (
            f.registration.as_str(),
            f.airline.as_str(),
            f.fin.as_str(),
            f.sure
        ),
        ("C-GJYI", "Air Canada", "147", true)
    );
    assert!(o.keywords.contains(&reg));
    assert!(!o.unticked.contains(&reg), "{o:?}");
    // Its lookup says a 777: not what the photo looks like, so unticked.
    let found = lookup::Found {
        registration: "C-GJYI".into(),
        known: true,
        aircraft_type: Some("Boeing 777-300ER".into()),
        airline: Some("Air Canada".into()),
        version: lookup::LOOKUP_VERSION,
        ..lookup::Found::default()
    };
    let json = serde_json::to_string(&found).unwrap();
    w.cat
        .write(|tx| tx.ai().put_aircraft_lookup("C-GJYI", &json, 1))
        .unwrap();
    let o = offer(&w, a);
    assert!(
        o.keywords.contains(&reg) && o.unticked.contains(&reg),
        "{o:?}"
    );
    // An A220 as looked up: ticked again, its type offered from the lookup.
    let found = lookup::Found {
        aircraft_type: Some("Airbus A220-300".into()),
        ..found
    };
    let json = serde_json::to_string(&found).unwrap();
    w.cat
        .write(|tx| tx.ai().put_aircraft_lookup("C-GJYI", &json, 2))
        .unwrap();
    let o = offer(&w, a);
    assert!(
        o.keywords.contains(&reg) && !o.unticked.contains(&reg),
        "{o:?}"
    );
    // b: a registration read: the fin offers nothing.
    read(&mut w, b, r#"[["C-GJYK",0.97]]"#, None);
    assert_eq!(offer(&w, b).from_fin, None);
    // c: two registrations for 147 at once: none offered.
    let both = fins::parse_csv(
        "Air Canada,147,C-FAAA,2020\n",
        &keepsake_core::ai::aircraft::marks::Airlines::builtin(),
    );
    fins::import(&mut w.cat, &both.rows).unwrap();
    read(&mut w, c, "[]", None);
    assert_eq!(offer(&w, c).from_fin, None);
}

#[test]
fn a_picked_photo_accepted_beside_another_is_shown_for_review_when_read() {
    let mut w = World::new(&["a.jpg", "b.jpg"]);
    let (a, b) = (w.asset("a.jpg"), w.asset("b.jpg"));
    let review = |w: &World, id: AssetId| {
        w.cat
            .read(|tx| tx.ai().aircraft_read(id))
            .unwrap()
            .map(|r| r.review)
    };
    // b got a's keywords as its neighbour, before being read itself.
    let keep = [TagPath::parse_lightroom("Aircraft|Airline|KLM").unwrap()];
    assert_eq!(
        scan::accept_neighbours(&mut w.cat, &w.lib, &[b], &keep).unwrap(),
        [b]
    );
    assert_eq!(review(&w, b).as_deref(), Some("accepted"));
    // The user picks b and a to be read: what is read waits for review, so
    // the window shows it (it showed nothing, as if b had not been read).
    scan::enqueue(&mut w.cat, &[a, b]).unwrap();
    assert_eq!(review(&w, b), None, "not read yet");
    let seen = Arc::new(Mutex::new(Vec::new()));
    w.service(fake(&["KLM", "PH-BQB"], &seen))
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    for id in [a, b] {
        assert_eq!(review(&w, id).as_deref(), Some("waiting"));
    }
    // The keywords b was given stay.
    assert_eq!(w.keywords(b), ["Aircraft|Airline|KLM"]);
}

/// Stands in for Claude: every photo is a beach.
struct BeachTagger;

impl keepsake_core::ai::cloud::Tagger for BeachTagger {
    fn tag(
        &self,
        _: keepsake_core::ai::Provider,
        _: &[u8],
        _: &[String],
    ) -> Result<keepsake_core::ai::cloud::CloudTags, AiError> {
        Ok(keepsake_core::ai::cloud::CloudTags {
            tags: vec!["Beach".into()],
            new_tags: vec!["Sand Castle".into()],
            model: "stand-in".into(),
        })
    }
}

#[test]
fn tag_with_ai_gives_its_tags_to_the_same_shot() {
    use keepsake_core::ai::Embedding;
    use keepsake_core::catalog::AiAnalysis;
    let mut w = World::new(&["a.jpg", "b.jpg", "c.jpg", "d.jpg"]);
    let [a, b, c, d] = ["a.jpg", "b.jpg", "c.jpg", "d.jpg"].map(|n| w.asset(n));
    w.thumb(a);
    // b: 4 s later, alike, same camera and lens. c: 3 s later, alike, but
    // another lens. d: 25 s later, alike.
    let db = rusqlite::Connection::open(w.lib.catalog_path()).unwrap();
    for (id, t, lens) in [
        (a, 1_000_000i64, "FE 24-70mm"),
        (b, 1_004_000, "FE 24-70mm"),
        (c, 1_003_000, "FE 100-400mm"),
        (d, 1_025_000, "FE 24-70mm"),
    ] {
        db.execute(
            "UPDATE assets SET capture_ts_ms = ?2, camera_make = 'SONY', camera_model = 'ILCE-7M3', lens_model = ?3 WHERE id = ?1",
            rusqlite::params![id, t, lens],
        )
        .unwrap();
    }
    drop(db);
    let v = Embedding::new((0..64).map(|i| ((i as f32) * 0.3).sin()).collect()).unwrap();
    for id in [a, b, c, d] {
        let (q, scale) = v.quantize();
        w.cat
            .write(|tx| {
                tx.ai().put_analysis(&AiAnalysis {
                    asset_id: id,
                    model: keepsake_core::ai::MODEL_ID.to_owned(),
                    embedding: q.clone(),
                    emb_scale: scale,
                    dhash: 0,
                    made_ms: 0,
                    tag_list: 0,
                })
            })
            .unwrap();
    }
    let added = keepsake_core::ai::cloud::tag_asset(
        &mut w.cat,
        &w.lib,
        &w.cache,
        &BeachTagger,
        keepsake_core::ai::Provider::Claude,
        a,
        10_000,
    )
    .unwrap();
    assert_eq!(added, ["Beach", "AI|Sand Castle"]);
    assert_eq!(w.keywords(b), ["AI|Sand Castle", "Beach"]);
    assert!(w.keywords(c).is_empty(), "another lens");
    assert!(w.keywords(d).is_empty(), "outside the window");
    // Recorded as given by Claude.
    let by: Vec<String> = w
        .cat
        .read(|tx| tx.ai().tag_sources(b))
        .unwrap()
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert!(by.iter().all(|p| p == "claude") && !by.is_empty());
}

// ---- When the aircraft was built (ADR-044) ----

use keepsake_core::ai::aircraft::built::{self, BuiltDb};

/// adsbdb's answer for C-FJZS, with its transponder code, through the real
/// client over a stand-in for the network; counts what it is asked.
struct Adsbdb {
    asked: Mutex<Vec<String>>,
}

impl keepsake_core::ai::cloud::Http for Adsbdb {
    fn post(&self, _: &str, _: &[(&str, &str)], _: &[u8]) -> Result<(u16, String), String> {
        Err("no posts".into())
    }
    fn get(&self, url: &str, _: &[(&str, &str)]) -> Result<(u16, String), String> {
        self.asked.lock().unwrap().push(url.to_owned());
        Ok((
            200,
            r#"{"response":{"aircraft":{"type":"777 333ER","icao_type":"B77W","manufacturer":"Boeing","mode_s":"C0174F","registered_owner":"Air Canada","registered_owner_country_name":"Canada"}}}"#.to_owned(),
        ))
    }
}

/// The client, with OpenSky's build year of C0174F (2008).
fn adsbdb() -> (Arc<Adsbdb>, Arc<lookup::AdsbDb>) {
    let http = Arc::new(Adsbdb {
        asked: Mutex::new(Vec::new()),
    });
    let db = BuiltDb::from_tsv("c0174f\tC-FJZS\t2008\n");
    let client = lookup::AdsbDb::new(http.clone()).with_built(Some(Arc::new(db)));
    (http, Arc::new(client))
}

#[test]
fn lookups_kept_before_the_build_year_are_asked_again_once() {
    let mut w = World::new(&["a.jpg"]);
    // Kept by 0.5: no transponder code, no build year, no version.
    let old = r#"{"registration":"C-FJZS","known":true,"aircraft_type":"Boeing 777-300ER","airline":"Air Canada","manufacturer":"Boeing","owner":"Air Canada","icao_type":"B77W","country":"Canada"}"#;
    w.cat
        .write(|tx| tx.ai().put_aircraft_lookup("C-FJZS", old, 1))
        .unwrap();
    assert!(lookup::kept(&w.cat, "C-FJZS").unwrap().unwrap().stale());
    let (http, client) = adsbdb();

    assert_eq!(
        lookup::refresh_one(&mut w.cat, client.as_ref())
            .unwrap()
            .as_deref(),
        Some("C-FJZS")
    );
    let f = lookup::kept(&w.cat, "C-FJZS").unwrap().unwrap();
    assert!(!f.stale());
    assert_eq!(f.mode_s.as_deref(), Some("C0174F"));
    assert_eq!(f.built.as_deref(), Some("2008"));
    let k = built::kept(&w.cat, "C-FJZS").unwrap().unwrap();
    assert_eq!(
        (k.built.as_str(), k.source.as_str()),
        ("2008", built::FROM_OPENSKY)
    );

    // Once: nothing is left to ask again, and a cached lookup is not asked.
    assert_eq!(
        lookup::refresh_one(&mut w.cat, client.as_ref()).unwrap(),
        None
    );
    lookup::lookup_cached(&mut w.cat, Some(client.as_ref()), "C-FJZS").unwrap();
    assert_eq!(http.asked.lock().unwrap().len(), 1);
}

#[test]
fn the_build_year_is_offered_from_opensky_and_accepted_as_a_keyword() {
    let mut w = World::new(&["a.jpg"]);
    let before = hashes(&w.media);
    let a = w.asset("a.jpg");
    scan::enqueue(&mut w.cat, &[a]).unwrap();
    let (http, client) = adsbdb();
    let seen = Arc::new(Mutex::new(Vec::new()));
    w.service(fake(&AIR_CANADA, &seen))
        .with_lookup(client)
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(http.asked.lock().unwrap().len(), 1);

    let item = &scan::review_items(&w.cat, &[a]).unwrap()[0];
    let conf = Confidence::default();
    let found = scan::found_for(&w.cat, item, &conf).unwrap();
    let offer = scan::offer(item, &conf, &found).unwrap();
    let built_kw = TagPath::parse_lightroom("Aircraft|Built|2008").unwrap();
    assert!(offer.keywords.contains(&built_kw));
    assert!(offer.from_lookup.contains(&built_kw));
    assert!(scan::is_aircraft_keyword(&built_kw));

    scan::accept(&mut w.cat, &w.lib, a, &offer.keywords).unwrap();
    assert!(w.keywords(a).contains(&"Aircraft|Built|2008".to_owned()));
    // The report's registration row says when it was built.
    let r = w.cat.read(|tx| tx.aircraft_report().aircraft()).unwrap();
    let row = r.registrations.iter().find(|r| r.name == "C-FJZS").unwrap();
    assert_eq!(row.built.as_deref(), Some("2008"));
    // A Built keyword is not an airline, type or registration of its own.
    assert!(r.types.iter().all(|t| t.name != "2008"));
    assert_eq!(hashes(&w.media), before, "originals unchanged");
}

/// Every sidecar's text, joined.
fn sidecars(w: &World) -> String {
    let mut out = String::new();
    let mut stack = vec![w.lib.sidecars_dir()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "xmp") {
                out.push_str(&std::fs::read_to_string(&p).unwrap());
            }
        }
    }
    out
}

#[test]
fn a_typed_build_date_wins_and_goes_on_every_photo_of_the_registration() {
    let mut w = World::new(&["a.jpg", "b.jpg", "c.jpg"]);
    let before = hashes(&w.media);
    let [a, b, c] = ["a.jpg", "b.jpg", "c.jpg"].map(|n| w.asset(n));
    let reg = TagPath::parse_lightroom("Aircraft|Registration|C-FJZS").unwrap();
    let old = TagPath::parse_lightroom("Aircraft|Built|2007").unwrap();
    {
        let mut svc = MetadataService::new(&w.lib, &mut w.cat);
        svc.set_user_tags(a, &[reg.clone(), old]).unwrap();
        svc.set_user_tags(b, std::slice::from_ref(&reg)).unwrap();
    }
    // OpenSky said 2008 (kept with the lookup).
    let found = lookup::Found {
        registration: "C-FJZS".into(),
        known: true,
        built: Some("2008".into()),
        version: lookup::LOOKUP_VERSION,
        ..lookup::Found::default()
    };
    let json = serde_json::to_string(&found).unwrap();
    w.cat
        .write(|tx| tx.ai().put_aircraft_lookup("C-FJZS", &json, 1))
        .unwrap();
    built::note_found(&mut w.cat, &found).unwrap();

    assert_eq!(
        built::set_typed(&mut w.cat, &w.lib, "C-FJZS", Some("2010-06-15")).unwrap(),
        2
    );
    let k = built::kept(&w.cat, "C-FJZS").unwrap().unwrap();
    assert_eq!(
        (k.built.as_str(), k.source.as_str()),
        ("2010-06-15", built::FROM_TYPED)
    );
    for id in [a, b] {
        let kw = w.keywords(id);
        assert!(kw.contains(&"Aircraft|Built|2010".to_owned()), "{kw:?}");
        assert!(!kw.contains(&"Aircraft|Built|2007".to_owned()), "{kw:?}");
    }
    assert!(w.keywords(c).is_empty());
    assert!(sidecars(&w).contains("Aircraft|Built|2010"));
    // OpenSky does not replace what was typed.
    built::note_found(&mut w.cat, &found).unwrap();
    assert_eq!(
        built::kept(&w.cat, "C-FJZS").unwrap().unwrap().source,
        built::FROM_TYPED
    );
    let r = w.cat.read(|tx| tx.aircraft_report().aircraft()).unwrap();
    assert_eq!(r.registrations[0].built.as_deref(), Some("2010-06-15"));

    // Not a year: refused, nothing changes.
    assert!(built::set_typed(&mut w.cat, &w.lib, "C-FJZS", Some("soon")).is_err());
    // Forgotten: the typed year goes, and OpenSky's comes back.
    assert_eq!(
        built::set_typed(&mut w.cat, &w.lib, "C-FJZS", None).unwrap(),
        2
    );
    let k = built::kept(&w.cat, "C-FJZS").unwrap().unwrap();
    assert_eq!(
        (k.built.as_str(), k.source.as_str()),
        ("2008", built::FROM_OPENSKY)
    );
    assert!(!w
        .keywords(a)
        .iter()
        .any(|k| k.starts_with("Aircraft|Built|")));
    assert_eq!(hashes(&w.media), before, "originals unchanged");
}

#[test]
fn the_downloaded_database_gives_lookups_already_kept_their_build_year() {
    let mut w = World::new(&["a.jpg"]);
    let found = lookup::Found {
        registration: "C-FJZS".into(),
        known: true,
        mode_s: Some("C0174F".into()),
        version: lookup::LOOKUP_VERSION,
        ..lookup::Found::default()
    };
    let json = serde_json::to_string(&found).unwrap();
    w.cat
        .write(|tx| tx.ai().put_aircraft_lookup("C-FJZS", &json, 1))
        .unwrap();
    let db = BuiltDb::from_tsv("c0174f\tC-FJZS\t2008-04-11\n");
    assert_eq!(built::fill_from_db(&mut w.cat, &db).unwrap(), 1);
    assert_eq!(
        lookup::kept(&w.cat, "C-FJZS")
            .unwrap()
            .unwrap()
            .built
            .as_deref(),
        Some("2008-04-11")
    );
    assert_eq!(
        built::kept(&w.cat, "C-FJZS").unwrap().unwrap().built,
        "2008-04-11"
    );
    // Again: nothing new.
    assert_eq!(built::fill_from_db(&mut w.cat, &db).unwrap(), 0);
}

// ---- Add route (ADR-044, part 2) ----

use keepsake_core::ai::aircraft::route::{self, Airports, Clock, Flight, Pick, Source};

/// OpenSky as the tests see it: C-GBIJ's day (2026-05-31), counting what
/// it is asked.
struct FakeSky {
    asked: Mutex<Vec<(String, i64, i64)>>,
}

impl route::Flights for FakeSky {
    fn flights(&self, icao24: &str, begin: i64, end: i64) -> Result<Vec<Flight>, AiError> {
        self.asked
            .lock()
            .unwrap()
            .push((icao24.to_owned(), begin, end));
        let fl = |first: i64, last: i64, dep: Option<&str>, arr: Option<&str>, cs: &str| Flight {
            first_seen: first,
            last_seen: last,
            departure: dep.map(str::to_owned),
            arrival: arr.map(str::to_owned),
            callsign: Some(cs.to_owned()),
            ..Flight::default()
        };
        // ROU2000 left Toronto at 12:16 UTC; ROU1917 landed there at 10:43.
        Ok(vec![
            fl(1_780_229_760, 1_780_235_100, Some("CYYZ"), None, "ROU2000"),
            fl(
                1_780_215_000,
                1_780_224_180,
                Some("CYQY"),
                Some("CYYZ"),
                "ROU1917",
            ),
        ])
    }
}

/// `w`'s photos given a registration keyword, a capture time and the
/// lookup's transponder code.
fn route_world(names: &[&str], times_s: &[i64]) -> (World, Vec<AssetId>) {
    let mut w = World::new(names);
    let ids: Vec<AssetId> = names.iter().map(|n| w.asset(n)).collect();
    let db = rusqlite::Connection::open(w.lib.catalog_path()).unwrap();
    for (id, t) in ids.iter().zip(times_s) {
        db.execute(
            "UPDATE assets SET capture_ts_ms = ?2 WHERE id = ?1",
            rusqlite::params![id, t * 1000],
        )
        .unwrap();
    }
    drop(db);
    let reg = TagPath::parse_lightroom("Aircraft|Registration|C-GBIJ").unwrap();
    for id in &ids {
        MetadataService::new(&w.lib, &mut w.cat)
            .set_user_tags(*id, std::slice::from_ref(&reg))
            .unwrap();
    }
    let found = lookup::Found {
        registration: "C-GBIJ".into(),
        known: true,
        mode_s: Some("C04826".into()),
        version: lookup::LOOKUP_VERSION,
        ..lookup::Found::default()
    };
    let json = serde_json::to_string(&found).unwrap();
    w.cat
        .write(|tx| tx.ai().put_aircraft_lookup("C-GBIJ", &json, 1))
        .unwrap();
    (w, ids)
}

/// 2026-05-31 12:31:11 UTC.
const PHOTO_S: i64 = 1_780_230_671;

#[test]
fn the_same_aircraft_and_day_is_asked_once_and_photos_beside_it_reuse_it() {
    let (mut w, ids) = route_world(&["a.jpg", "b.jpg"], &[PHOTO_S, PHOTO_S + 4]);
    let sky = FakeSky {
        asked: Mutex::new(Vec::new()),
    };
    let asked = route::run_job(&mut w.cat, &sky, None, ids[0], Clock::AsRecorded).unwrap();
    assert_eq!(asked.requests, 1);
    {
        let a = sky.asked.lock().unwrap();
        // The whole UTC day, under 24 hours.
        assert_eq!(a[0].0, "c04826");
        assert_eq!(a[0].2 - a[0].1, 86_399);
        assert_eq!(a[0].1 % 86_400, 0);
    }
    // The photo taken beside it: nothing is asked.
    assert_eq!(
        route::run_job(&mut w.cat, &sky, None, ids[1], Clock::AsRecorded)
            .unwrap()
            .requests,
        0
    );
    assert_eq!(sky.asked.lock().unwrap().len(), 1);

    let airports = Airports::builtin();
    let it = route::review(
        &w.cat,
        &airports,
        ids[0],
        Clock::AsRecorded,
        Source::OpenSky,
    )
    .unwrap();
    match it.state {
        route::State::Found(Pick::One(c)) => {
            assert_eq!(c.flight.callsign.as_deref(), Some("ROU2000"));
            assert_eq!(c.at, route::At::Departure);
            let k: Vec<String> = route::keywords(&c.flight, &airports)
                .unwrap()
                .iter()
                .map(TagPath::to_lightroom)
                .collect();
            assert_eq!(
                k,
                [
                    "Aircraft|From|YYZ",
                    "Aircraft|From country|Canada",
                    "Aircraft|Flight|RV2000"
                ]
            );
        }
        s => panic!("{s:?}"),
    }
}

#[test]
fn a_photo_near_midnight_looks_at_both_days_and_one_without_a_registration_is_not_asked() {
    // 01:00 UTC on 2026-05-31: the day before too.
    let night = PHOTO_S - (PHOTO_S % 86_400) + 3_600;
    let (mut w, ids) = route_world(&["a.jpg"], &[night]);
    let sky = FakeSky {
        asked: Mutex::new(Vec::new()),
    };
    // One question per day (4 credits each; two days in one would be 30).
    assert_eq!(
        route::run_job(&mut w.cat, &sky, None, ids[0], Clock::AsRecorded)
            .unwrap()
            .requests,
        2
    );
    {
        let a = sky.asked.lock().unwrap();
        assert!(a.iter().all(|q| q.2 - q.1 == 86_399));
        assert_eq!(a[0].1, night - (night % 86_400) - 86_400);
    }

    let mut w2 = World::new(&["c.jpg"]);
    let c = w2.asset("c.jpg");
    assert!(matches!(
        route::run_job(&mut w2.cat, &sky, None, c, Clock::AsRecorded),
        Err(AiError::Cloud { retry: false, .. })
    ));
    let airports = Airports::builtin();
    assert_eq!(
        route::review(&w2.cat, &airports, c, Clock::AsRecorded, Source::OpenSky)
            .unwrap()
            .state,
        route::State::NoRegistration
    );
}

/// Give photo `id` the registration `reg`, and `reg` the transponder code
/// `code` (as the lookup keeps it).
fn set_aircraft(w: &mut World, id: AssetId, reg: &str, code: &str) {
    let path = TagPath::parse_lightroom(&format!("Aircraft|Registration|{reg}")).unwrap();
    MetadataService::new(&w.lib, &mut w.cat)
        .set_user_tags(id, std::slice::from_ref(&path))
        .unwrap();
    let found = lookup::Found {
        registration: reg.into(),
        known: true,
        mode_s: Some(code.into()),
        version: lookup::LOOKUP_VERSION,
        ..lookup::Found::default()
    };
    let json = serde_json::to_string(&found).unwrap();
    w.cat
        .write(|tx| tx.ai().put_aircraft_lookup(reg, &json, 1))
        .unwrap();
}

/// Queue Add route for `ids` and run each job as the worker does, counting
/// OpenSky's questions.
fn run_all(w: &mut World, sky: &FakeSky, ids: &[AssetId]) -> u32 {
    route::enqueue(&mut w.cat, ids).unwrap();
    let mut requests = 0;
    for id in ids {
        requests += route::run_job(&mut w.cat, sky, None, *id, Clock::AsRecorded)
            .unwrap()
            .requests;
        w.cat
            .write(|tx| tx.jobs().remove_queued_for(route::JOB_AI_ROUTE, &[*id]))
            .unwrap();
    }
    requests
}

fn names(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("p{i:02}.jpg")).collect()
}

#[test]
fn thirty_photos_of_one_aircraft_make_one_opensky_request_per_day() {
    // 13:00 to 16:50 UTC, every 8 minutes: one question for all 30.
    let day = PHOTO_S - (PHOTO_S % 86_400);
    let n = names(30);
    let refs: Vec<&str> = n.iter().map(String::as_str).collect();
    let times: Vec<i64> = (0..30).map(|i| day + 46_800 + i * 480).collect();
    let (mut w, ids) = route_world(&refs, &times);
    let sky = FakeSky {
        asked: Mutex::new(Vec::new()),
    };
    assert_eq!(run_all(&mut w, &sky, &ids), 1);
    assert_eq!(sky.asked.lock().unwrap()[0].1, day);

    // 22:00 to 01:50 UTC: two UTC days, one question each, not 30.
    let midnight = day + 86_400;
    let times: Vec<i64> = (0..30).map(|i| midnight - 7_200 + i * 480).collect();
    let (mut w, ids) = route_world(&refs, &times);
    let sky = FakeSky {
        asked: Mutex::new(Vec::new()),
    };
    assert_eq!(run_all(&mut w, &sky, &ids), 2);
    let a = sky.asked.lock().unwrap();
    let mut starts: Vec<i64> = a.iter().map(|q| q.1).collect();
    starts.sort_unstable();
    assert_eq!(starts, [midnight - 86_400, midnight]);
    assert!(a.iter().all(|q| q.2 - q.1 == 86_399));
    drop(a);
    // Every photo reads the one answer.
    let airports = Airports::builtin();
    for id in &ids {
        let it = route::review(&w.cat, &airports, *id, Clock::AsRecorded, Source::OpenSky).unwrap();
        assert!(matches!(it.state, route::State::Found(_)), "{it:?}");
    }
}

#[test]
fn two_aircraft_make_two_requests_and_days_far_apart_are_asked_apart() {
    let n = names(20);
    let refs: Vec<&str> = n.iter().map(String::as_str).collect();
    let times: Vec<i64> = (0..20).map(|i| PHOTO_S + i * 300).collect();
    let (mut w, ids) = route_world(&refs, &times);
    for id in &ids[10..] {
        set_aircraft(&mut w, *id, "D-ABVY", "3C4AD9");
    }
    let sky = FakeSky {
        asked: Mutex::new(Vec::new()),
    };
    assert_eq!(run_all(&mut w, &sky, &ids), 2);
    let mut codes: Vec<String> = sky
        .asked
        .lock()
        .unwrap()
        .iter()
        .map(|a| a.0.clone())
        .collect();
    codes.sort();
    assert_eq!(codes, ["3c4ad9", "c04826"]);

    // One aircraft on three days in a row and a week later: one question
    // per day.
    let day = PHOTO_S - (PHOTO_S % 86_400);
    let noon = |d: i64| day + d * 86_400 + 43_200;
    let (mut w, ids) = route_world(
        &["a.jpg", "b.jpg", "c.jpg", "d.jpg"],
        &[noon(0), noon(1), noon(2), noon(9)],
    );
    let sky = FakeSky {
        asked: Mutex::new(Vec::new()),
    };
    assert_eq!(run_all(&mut w, &sky, &ids), 4);
    let a = sky.asked.lock().unwrap();
    let mut spans: Vec<(i64, i64)> = a.iter().map(|q| (q.1 - day, q.2 - day)).collect();
    spans.sort_unstable();
    let one = |d: i64| (d * 86_400, (d + 1) * 86_400 - 1);
    assert_eq!(spans, [one(0), one(1), one(2), one(9)]);
}

/// OpenSky at its limit for the first question, then answering.
struct LimitThenSky {
    inner: FakeSky,
    limited: Mutex<bool>,
}

impl route::Flights for LimitThenSky {
    fn flights(&self, icao24: &str, begin: i64, end: i64) -> Result<Vec<Flight>, AiError> {
        let mut l = self.limited.lock().unwrap();
        if *l {
            *l = false;
            return Err(route::limit_reached(1_000));
        }
        drop(l);
        self.inner.flights(icao24, begin, end)
    }
}

#[test]
fn at_opensky_s_limit_the_batch_waits_and_then_asks_once() {
    let n = names(5);
    let refs: Vec<&str> = n.iter().map(String::as_str).collect();
    let times: Vec<i64> = (0..5).map(|i| PHOTO_S + i * 600).collect();
    let (mut w, ids) = route_world(&refs, &times);
    route::enqueue(&mut w.cat, &ids).unwrap();
    let sky = LimitThenSky {
        inner: FakeSky {
            asked: Mutex::new(Vec::new()),
        },
        limited: Mutex::new(true),
    };
    // At the limit: the job is to wait (not failed), nothing is kept.
    assert!(matches!(
        route::run_job(&mut w.cat, &sky, None, ids[0], Clock::AsRecorded),
        Err(AiError::Later { .. })
    ));
    let airports = Airports::builtin();
    assert_eq!(
        route::review(
            &w.cat,
            &airports,
            ids[1],
            Clock::AsRecorded,
            Source::OpenSky
        )
        .unwrap()
        .state,
        route::State::Waiting
    );
    // Later: one question for all five.
    let mut requests = 0;
    for id in &ids {
        requests += route::run_job(&mut w.cat, &sky, None, *id, Clock::AsRecorded)
            .unwrap()
            .requests;
    }
    assert_eq!(requests, 1);
    assert_eq!(sky.inner.asked.lock().unwrap().len(), 1);
}

#[test]
fn the_route_worker_asks_in_the_background_and_accepting_writes_only_aircraft_keywords() {
    let (mut w, ids) = route_world(&["a.jpg", "b.jpg"], &[PHOTO_S, PHOTO_S + 4]);
    let before = hashes(&w.media);
    assert_eq!(route::enqueue(&mut w.cat, &ids).unwrap(), 2);
    let sky = Arc::new(FakeSky {
        asked: Mutex::new(Vec::new()),
    });
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut opts_service = w.service(fake(&AIR_CANADA, &seen));
    opts_service = opts_service.with_flights(sky.clone());
    opts_service
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    // One day, asked once for both photos.
    assert_eq!(sky.asked.lock().unwrap().len(), 1);
    let (st, _) = route::status(&w.cat, &ids).unwrap();
    assert_eq!((st.done, st.pending, st.failed), (2, 0, 0));

    // Accepting adds the route keywords, in the catalog and the sidecar.
    let keep: Vec<TagPath> = ["Aircraft|From|YYZ", "Aircraft|Flight|RV2000"]
        .iter()
        .map(|k| TagPath::parse_lightroom(k).unwrap())
        .collect();
    assert_eq!(
        scan::add_to(&mut w.cat, &w.lib, &ids[..1], &keep).unwrap(),
        1
    );
    let kw = w.keywords(ids[0]);
    assert!(kw.contains(&"Aircraft|From|YYZ".to_owned()), "{kw:?}");
    assert!(kw.contains(&"Aircraft|Flight|RV2000".to_owned()), "{kw:?}");
    assert!(sidecars(&w).contains("Aircraft|Flight|RV2000"));
    // What is not an aircraft keyword is refused.
    let other = [TagPath::parse_lightroom("Places|Toronto").unwrap()];
    assert!(scan::add_to(&mut w.cat, &w.lib, &ids[..1], &other).is_err());
    let airports = Airports::builtin();
    assert_eq!(
        route::review(
            &w.cat,
            &airports,
            ids[0],
            Clock::AsRecorded,
            Source::OpenSky
        )
        .unwrap()
        .has
        .len(),
        2
    );
    assert_eq!(hashes(&w.media), before, "originals unchanged");
}

#[test]
fn a_photo_asked_about_now_is_read_before_a_backlog_even_one_already_queued() {
    let names = ["a.jpg", "b.jpg", "c.jpg", "d.jpg"];
    let mut w = World::new(&names);
    let ids: Vec<AssetId> = names.iter().map(|n| w.asset(n)).collect();
    let next = |cat: &Catalog| {
        cat.read(|tx| tx.jobs().next(&[JOB_AI_AIRCRAFT], i64::MAX))
            .unwrap()
            .map(|j| j.asset_id)
    };
    // A folder's worth queued first, in its order.
    scan::enqueue(&mut w.cat, &ids[..3]).unwrap();
    assert_eq!(next(&w.cat), Some(ids[0]));
    // A photo not queued yet, then one queued at the back: each goes first.
    scan::enqueue(&mut w.cat, &ids[3..]).unwrap();
    assert_eq!(next(&w.cat), Some(ids[3]));
    scan::enqueue(&mut w.cat, &ids[2..3]).unwrap();
    assert_eq!(next(&w.cat), Some(ids[2]));
    // Add route's queue does the same.
    use keepsake_core::ai::aircraft::route::{enqueue, JOB_AI_ROUTE};
    enqueue(&mut w.cat, &ids[..3]).unwrap();
    enqueue(&mut w.cat, &ids[1..2]).unwrap();
    let r = w
        .cat
        .read(|tx| tx.jobs().next(&[JOB_AI_ROUTE], i64::MAX))
        .unwrap()
        .map(|j| j.asset_id);
    assert_eq!(r, Some(ids[1]));
}

/// adsbdb as the tests see it, not knowing any aircraft (404).
struct Unknown {
    asked: Mutex<Vec<String>>,
}

impl keepsake_core::ai::cloud::Http for Unknown {
    fn post(&self, _: &str, _: &[(&str, &str)], _: &[u8]) -> Result<(u16, String), String> {
        Err("no posts".into())
    }
    fn get(&self, url: &str, _: &[(&str, &str)]) -> Result<(u16, String), String> {
        self.asked.lock().unwrap().push(url.to_owned());
        Ok((404, r#"{"response":"unknown aircraft"}"#.to_owned()))
    }
}

#[test]
fn an_aircraft_adsbdb_does_not_know_gets_its_code_from_opensky_and_is_asked_again_after_30_days() {
    let mut w = World::new(&["a.jpg"]);
    let http = Arc::new(Unknown {
        asked: Mutex::new(Vec::new()),
    });
    let db = BuiltDb::from_tsv("#keepsake-aircraft\t2\n484f6e\tPH-BKO\t\n");
    let client = lookup::AdsbDb::new(http.clone()).with_built(Some(Arc::new(db)));

    // adsbdb does not know it; OpenSky's database gives its code.
    let f = lookup::lookup_cached(&mut w.cat, Some(&client), "PH-BKO")
        .unwrap()
        .unwrap();
    assert!(!f.known);
    assert_eq!(f.mode_s.as_deref(), Some("484F6E"));
    assert_eq!(
        route::icao24_of(&w.cat, "PH-BKO").unwrap().as_deref(),
        Some("484f6e")
    );
    assert_eq!(http.asked.lock().unwrap().len(), 1);

    // Not asked again soon: neither when wanted nor in the background.
    lookup::lookup_cached(&mut w.cat, Some(&client), "PH-BKO").unwrap();
    assert_eq!(lookup::refresh_one(&mut w.cat, &client).unwrap(), None);
    assert_eq!(http.asked.lock().unwrap().len(), 1);

    // Kept 31 days ago: asked again, in the background.
    let json = serde_json::to_string(&f).unwrap();
    let long_ago = keepsake_core::jobs::now_ms() - lookup::RETRY_UNKNOWN_MS - 86_400_000;
    w.cat
        .write(|tx| tx.ai().put_aircraft_lookup("PH-BKO", &json, long_ago))
        .unwrap();
    assert_eq!(
        lookup::refresh_one(&mut w.cat, &client).unwrap().as_deref(),
        Some("PH-BKO")
    );
    assert_eq!(http.asked.lock().unwrap().len(), 2);
    assert_eq!(lookup::refresh_one(&mut w.cat, &client).unwrap(), None);

    // A known one is never asked again for its age.
    let known = lookup::Found {
        registration: "C-GBIJ".into(),
        known: true,
        version: lookup::LOOKUP_VERSION,
        ..lookup::Found::default()
    };
    assert!(!known.due(0, keepsake_core::jobs::now_ms()));
}

#[test]
fn downloading_the_database_again_gives_codes_to_registrations_looked_up_before() {
    let mut w = World::new(&["a.jpg"]);
    let unknown = lookup::Found {
        registration: "PH-BKO".into(),
        version: lookup::LOOKUP_VERSION,
        ..lookup::Found::default()
    };
    let json = serde_json::to_string(&unknown).unwrap();
    w.cat
        .write(|tx| tx.ai().put_aircraft_lookup("PH-BKO", &json, 5))
        .unwrap();
    let db = BuiltDb::from_tsv("#keepsake-aircraft\t2\n484f6e\tPH-BKO\t\n");
    assert_eq!(built::fill_from_db(&mut w.cat, &db).unwrap(), 1);
    let (f, at) = lookup::kept_at(&w.cat, "PH-BKO").unwrap().unwrap();
    assert_eq!(f.mode_s.as_deref(), Some("484F6E"));
    // When it was asked stays, so the 30 days are not restarted.
    assert_eq!(at, 5);
    assert_eq!(built::fill_from_db(&mut w.cat, &db).unwrap(), 0);
}

#[test]
fn ask_again_forgets_the_flights_kept_and_asks_opensky_once_more() {
    let (mut w, ids) = route_world(&["a.jpg"], &[PHOTO_S]);
    let sky = FakeSky {
        asked: Mutex::new(Vec::new()),
    };
    let run = |w: &mut World, sky: &FakeSky| {
        route::run_job(&mut w.cat, sky, None, ids[0], Clock::AsRecorded)
            .unwrap()
            .requests
    };
    assert_eq!(run(&mut w, &sky), 1);
    // Queued again without Ask again: the answer kept is used.
    route::enqueue(&mut w.cat, &ids).unwrap();
    assert_eq!(run(&mut w, &sky), 0);
    // Ask again: queued, and OpenSky is asked again.
    assert_eq!(
        route::again(&mut w.cat, &ids, Clock::AsRecorded, Source::OpenSky).unwrap(),
        1
    );
    assert!(route::kept_day(&w.cat, "c04826", "2026-05-31")
        .unwrap()
        .is_none());
    assert_eq!(run(&mut w, &sky), 1);
    assert_eq!(sky.asked.lock().unwrap().len(), 2);
    // A registration adsbdb did not know is asked about again too; one it
    // knew is kept.
    assert!(lookup::kept(&w.cat, "C-GBIJ").unwrap().is_some());
    let unknown = lookup::Found {
        registration: "C-GBIJ".into(),
        version: lookup::LOOKUP_VERSION,
        ..lookup::Found::default()
    };
    let json = serde_json::to_string(&unknown).unwrap();
    w.cat
        .write(|tx| tx.ai().put_aircraft_lookup("C-GBIJ", &json, 1))
        .unwrap();
    route::again(&mut w.cat, &ids, Clock::AsRecorded, Source::OpenSky).unwrap();
    assert!(lookup::kept(&w.cat, "C-GBIJ").unwrap().is_none());
}

#[test]
fn an_eastern_camera_clock_is_read_with_daylight_saving_by_the_date() {
    let (mut w, ids) = route_world(&["a.jpg"], &[PHOTO_S]);
    // The camera said UTC−05:00 (on 31 May, daylight saving time).
    let db = rusqlite::Connection::open(w.lib.catalog_path()).unwrap();
    db.execute(
        "UPDATE assets SET capture_utc_offset_min = -300 WHERE id = ?1",
        [ids[0]],
    )
    .unwrap();
    drop(db);
    let sky = FakeSky {
        asked: Mutex::new(Vec::new()),
    };
    let airports = Airports::builtin();
    route::run_job(&mut w.cat, &sky, None, ids[0], Clock::Eastern).unwrap();
    let as_recorded = route::review(
        &w.cat,
        &airports,
        ids[0],
        Clock::AsRecorded,
        Source::OpenSky,
    )
    .unwrap();
    assert_eq!(as_recorded.taken_s, Some(PHOTO_S));
    assert_eq!(as_recorded.offset_min, Some(-300));
    // Read as Eastern daylight time: an hour earlier, between ROU1917's
    // landing (48 min before) and ROU2000's take-off (45 min after).
    let eastern =
        route::review(&w.cat, &airports, ids[0], Clock::Eastern, Source::OpenSky).unwrap();
    assert_eq!(eastern.taken_s, Some(PHOTO_S - 3600));
    assert_eq!(eastern.offset_min, Some(-240));
    match eastern.state {
        route::State::Found(Pick::Several(v)) => assert_eq!(v.len(), 2),
        s => panic!("{s:?}"),
    }
}

#[test]
fn a_transponder_code_typed_is_used_and_kept_when_the_registration_is_asked_again() {
    let mut w = World::new(&["a.jpg"]);
    let http = Arc::new(Unknown {
        asked: Mutex::new(Vec::new()),
    });
    let client = lookup::AdsbDb::new(http.clone());
    lookup::lookup_cached(&mut w.cat, Some(&client), "PH-BKO").unwrap();
    assert_eq!(route::icao24_of(&w.cat, "PH-BKO").unwrap(), None);

    assert!(lookup::set_typed_code(&mut w.cat, "PH-BKO", Some("PH-BKO")).is_err());
    lookup::set_typed_code(&mut w.cat, "PH-BKO", Some(" 0x484f6e ")).unwrap();
    assert_eq!(
        route::icao24_of(&w.cat, "PH-BKO").unwrap().as_deref(),
        Some("484f6e")
    );

    // Asked again 30 days on: adsbdb still does not know it; the code stays.
    let (f, _) = lookup::kept_at(&w.cat, "PH-BKO").unwrap().unwrap();
    let json = serde_json::to_string(&f).unwrap();
    let long_ago = keepsake_core::jobs::now_ms() - lookup::RETRY_UNKNOWN_MS - 86_400_000;
    w.cat
        .write(|tx| tx.ai().put_aircraft_lookup("PH-BKO", &json, long_ago))
        .unwrap();
    assert_eq!(
        lookup::refresh_one(&mut w.cat, &client).unwrap().as_deref(),
        Some("PH-BKO")
    );
    assert_eq!(http.asked.lock().unwrap().len(), 2);
    assert_eq!(
        route::icao24_of(&w.cat, "PH-BKO").unwrap().as_deref(),
        Some("484f6e")
    );

    // A code typed for a registration never asked about; and forgotten.
    lookup::set_typed_code(&mut w.cat, "PH-BKP", Some("484F70")).unwrap();
    assert_eq!(
        route::icao24_of(&w.cat, "PH-BKP").unwrap().as_deref(),
        Some("484f70")
    );
    lookup::set_typed_code(&mut w.cat, "PH-BKO", None).unwrap();
    assert_eq!(route::icao24_of(&w.cat, "PH-BKO").unwrap(), None);
}

#[test]
fn a_time_zone_set_for_a_photo_keeps_its_clock_time_and_wins_over_the_eastern_setting() {
    let (mut w, ids) = route_world(&["a.jpg"], &[PHOTO_S]);
    let db = rusqlite::Connection::open(w.lib.catalog_path()).unwrap();
    db.execute(
        "UPDATE assets SET capture_utc_offset_min = -300 WHERE id = ?1",
        [ids[0]],
    )
    .unwrap();
    drop(db);
    let airports = Airports::builtin();
    let mut svc = MetadataService::new(&w.lib, &mut w.cat);
    assert!(svc.set_capture_offset(ids[0], 900).is_err());
    // 07:31 on the camera's clock, in UTC+01:00: 06:31 UTC.
    let a = svc.set_capture_offset(ids[0], 60).unwrap();
    let c = a.capture.unwrap();
    assert_eq!(c.utc_ms, (PHOTO_S - 6 * 3600) * 1000);
    assert_eq!(c.utc_offset_min, Some(60));
    assert_eq!(
        a.capture_source,
        Some(keepsake_core::catalog::CaptureSource::Sidecar)
    );
    assert!(sidecars(&w).contains("2026-05-31T07:31:11.000+01:00"));
    let it = route::review(&w.cat, &airports, ids[0], Clock::Eastern, Source::OpenSky).unwrap();
    assert_eq!(
        (it.taken_s, it.offset_min),
        (Some(PHOTO_S - 6 * 3600), Some(60))
    );

    // Set back to UTC−05:00 by hand: the Eastern setting leaves it.
    MetadataService::new(&w.lib, &mut w.cat)
        .set_capture_offset(ids[0], -300)
        .unwrap();
    let it = route::review(&w.cat, &airports, ids[0], Clock::Eastern, Source::OpenSky).unwrap();
    assert_eq!((it.taken_s, it.offset_min), (Some(PHOTO_S), Some(-300)));
}

/// Flightradar24 as the tests see it: PH-BKO's flight on 2025-04-01, by
/// registration.
struct FakeFr24 {
    asked: Mutex<Vec<String>>,
}

impl route::Flights for FakeFr24 {
    fn flights(&self, id: &str, _: i64, _: i64) -> Result<Vec<Flight>, AiError> {
        self.asked.lock().unwrap().push(id.to_owned());
        // KL692 left Toronto at 19:20 UTC.
        Ok(vec![Flight {
            first_seen: 1_743_535_200,
            last_seen: 1_743_565_500,
            departure: Some("CYYZ".into()),
            arrival: Some("EHAM".into()),
            callsign: Some("KLM692".into()),
            number: Some("KL692".into()),
            ..Flight::default()
        }])
    }
    fn source(&self) -> Source {
        Source::Flightradar
    }
}

#[test]
fn flightradar24_finds_the_flight_by_registration_and_forgets_it_after_30_days() {
    // A7_05212.ARW: 2025-04-01 19:56 UTC, PH-BKO, no transponder code.
    let (mut w, ids) = route_world(&["a.jpg"], &[1_743_537_378]);
    let reg = TagPath::parse_lightroom("Aircraft|Registration|PH-BKO").unwrap();
    MetadataService::new(&w.lib, &mut w.cat)
        .set_user_tags(ids[0], std::slice::from_ref(&reg))
        .unwrap();
    assert_eq!(route::icao24_of(&w.cat, "PH-BKO").unwrap(), None);
    let fr = FakeFr24 {
        asked: Mutex::new(Vec::new()),
    };
    let airports = Airports::builtin();
    // Not asked yet.
    assert_eq!(
        route::review(
            &w.cat,
            &airports,
            ids[0],
            Clock::AsRecorded,
            Source::Flightradar
        )
        .unwrap()
        .state,
        route::State::Waiting
    );
    let asked = route::run_job(&mut w.cat, &fr, None, ids[0], Clock::AsRecorded).unwrap();
    assert_eq!(asked.requests, 1);
    assert_eq!(*fr.asked.lock().unwrap(), ["PH-BKO"]);
    let it = route::review(
        &w.cat,
        &airports,
        ids[0],
        Clock::AsRecorded,
        Source::Flightradar,
    )
    .unwrap();
    match it.state {
        route::State::Found(Pick::One(c)) => {
            let k: Vec<String> = route::keywords(&c.flight, &airports)
                .unwrap()
                .iter()
                .map(TagPath::to_lightroom)
                .collect();
            assert_eq!(
                k,
                [
                    "Aircraft|From|YYZ",
                    "Aircraft|From country|Canada",
                    "Aircraft|To|AMS",
                    "Aircraft|To country|Netherlands",
                    "Aircraft|Flight|KL692"
                ]
            );
        }
        s => panic!("{s:?}"),
    }
    // Kept: not asked again. OpenSky's answers are apart, but with OpenSky
    // chosen the photo keeps Flightradar24's answer (ADR-050).
    route::run_job(&mut w.cat, &fr, None, ids[0], Clock::AsRecorded).unwrap();
    assert_eq!(fr.asked.lock().unwrap().len(), 1);
    let it = route::review(
        &w.cat,
        &airports,
        ids[0],
        Clock::AsRecorded,
        Source::OpenSky,
    )
    .unwrap();
    assert_eq!(it.source, Some(Source::Flightradar));
    assert!(matches!(it.state, route::State::Found(Pick::One(_))));

    // Ask again: asked once more.
    route::again(&mut w.cat, &ids, Clock::AsRecorded, Source::Flightradar).unwrap();
    route::run_job(&mut w.cat, &fr, None, ids[0], Clock::AsRecorded).unwrap();
    assert_eq!(fr.asked.lock().unwrap().len(), 2);

    // 31 days on: deleted (Flightradar24's terms), not used before that.
    let now = keepsake_core::jobs::now_ms();
    assert_eq!(route::forget_expired(&mut w.cat, now).unwrap(), 0);
    let day = w
        .cat
        .read(|tx| tx.ai().fr24_flights("PH-BKO", "2025-04-01"))
        .unwrap()
        .unwrap();
    w.cat
        .write(|tx| {
            tx.ai().put_fr24_flights(
                "PH-BKO",
                "2025-04-01",
                &day.0,
                now - route::FR24_KEEP_MS - 1,
            )
        })
        .unwrap();
    assert_eq!(
        route::review(
            &w.cat,
            &airports,
            ids[0],
            Clock::AsRecorded,
            Source::Flightradar
        )
        .unwrap()
        .state,
        route::State::Waiting
    );
    assert_eq!(route::forget_expired(&mut w.cat, now).unwrap(), 1);
    assert!(w
        .cat
        .read(|tx| tx.ai().fr24_flights("PH-BKO", "2025-04-01"))
        .unwrap()
        .is_none());
}

/// Flightradar24 as the tests see it: knows PH-BKO's latest flight (in the
/// weeks before last), not C-GBIJ's.
struct FakeFr24Aircraft {
    asked: Mutex<Vec<(String, i64, i64)>>,
}

impl route::Fr24Aircrafts for FakeFr24Aircraft {
    fn aircraft(
        &self,
        registration: &str,
        begin: i64,
        end: i64,
    ) -> Result<Option<lookup::Fr24Aircraft>, AiError> {
        let n = {
            let mut a = self.asked.lock().unwrap();
            a.push((registration.to_owned(), begin, end));
            a.iter().filter(|x| x.0 == registration).count()
        };
        Ok(
            (registration == "PH-BKO" && n == 2).then(|| lookup::Fr24Aircraft {
                asked_ms: keepsake_core::jobs::now_ms(),
                found: true,
                icao_type: Some("B78X".into()),
                operator: Some("KLM".into()),
                painted_as: Some("KLM".into()),
                hex: Some("485E31".into()),
                seen: Some("2026-11-01".into()),
                expired: false,
            }),
        )
    }
}

#[test]
fn looking_aircraft_up_on_flightradar24_keeps_type_airline_and_code_for_30_days() {
    use keepsake_core::ai::aircraft::fr24;
    use std::sync::atomic::AtomicBool;
    let (mut w, ids) = route_world(&["a.jpg", "b.jpg"], &[PHOTO_S, PHOTO_S]);
    let bko = TagPath::parse_lightroom("Aircraft|Registration|PH-BKO").unwrap();
    MetadataService::new(&w.lib, &mut w.cat)
        .set_user_tags(ids[1], std::slice::from_ref(&bko))
        .unwrap();
    let regs: Vec<String> = fr24::registrations(&w.cat)
        .unwrap()
        .into_iter()
        .map(|r| r.0)
        .collect();
    assert_eq!(regs, ["C-GBIJ", "PH-BKO"]);
    assert_eq!(fr24::status(&w.cat).unwrap().registrations, 2);

    // One batch job.
    assert!(fr24::enqueue(&mut w.cat).unwrap());
    assert!(fr24::status(&w.cat).unwrap().running);
    fr24::stop(&mut w.cat).unwrap();
    assert!(!fr24::status(&w.cat).unwrap().running);

    let client = FakeFr24Aircraft {
        asked: Mutex::new(Vec::new()),
    };
    let b = fr24::run_batch(
        &mut w.cat,
        &client,
        &AtomicBool::new(false),
        std::time::Duration::ZERO,
    )
    .unwrap();
    // Each: the last two weeks, then the two before; PH-BKO found then.
    assert_eq!((b.asked, b.found, b.left), (4, 1, 0));
    {
        let a = client.asked.lock().unwrap();
        assert!(a.iter().all(|x| x.2 - x.1 < 14 * 86_400));
        assert_eq!(a[0].2 - a[1].2, 14 * 86_400);
    }
    let st = fr24::status(&w.cat).unwrap();
    assert_eq!((st.asked, st.found), (2, 1));
    // Not asked again within the 30 days.
    let b = fr24::run_batch(
        &mut w.cat,
        &client,
        &AtomicBool::new(false),
        std::time::Duration::ZERO,
    )
    .unwrap();
    assert_eq!(b.asked, 0);

    // Its code lets OpenSky find PH-BKO's routes; the type and airline are
    // offered where adsbdb knows nothing.
    assert_eq!(
        route::icao24_of(&w.cat, "PH-BKO").unwrap().as_deref(),
        Some("485e31")
    );
    let now = keepsake_core::jobs::now_ms();
    let f = lookup::kept(&w.cat, "PH-BKO").unwrap().unwrap();
    let merged = fr24::with_fr24(&f, now).unwrap();
    assert_eq!(merged.airline.as_deref(), Some("KLM"));
    assert_eq!(merged.aircraft_type.as_deref(), Some("Boeing 787-10"));
    // No operator (as Flightradar24 answers): the livery's airline.
    let mut no_op = f.clone();
    if let Some(a) = no_op.fr24.as_mut() {
        a.operator = None;
    }
    assert_eq!(
        fr24::with_fr24(&no_op, now).unwrap().airline.as_deref(),
        Some("KLM")
    );
    // For the inspector.
    let shown = fr24::shown(&w.cat, "PH-BKO", now).unwrap().unwrap();
    assert_eq!(shown.aircraft_type.as_deref(), Some("Boeing 787-10"));
    assert_eq!(shown.airline.as_deref(), Some("KLM"));
    assert_eq!(shown.hex.as_deref(), Some("485E31"));
    assert_eq!(shown.until_ms - shown.asked_ms, route::FR24_KEEP_MS);
    assert!(!fr24::shown(&w.cat, "C-GBIJ", now).unwrap().unwrap().found);
    // C-GBIJ (adsbdb knows it): its own answer is kept.
    assert!(lookup::kept(&w.cat, "C-GBIJ").unwrap().unwrap().known);

    // 31 days on: deleted, and no longer used.
    let later = now + route::FR24_KEEP_MS + 86_400_000;
    assert!(fr24::with_fr24(&f, later).is_none());
    assert_eq!(fr24::forget_expired(&mut w.cat, later).unwrap(), 2);
    // Only when it was asked stays: nothing Flightradar24 said.
    let left = lookup::kept(&w.cat, "PH-BKO")
        .unwrap()
        .unwrap()
        .fr24
        .unwrap();
    assert!(left.expired);
    assert_eq!(
        (left.icao_type, left.hex, left.operator, left.seen),
        (None, None, None, None)
    );
    assert_eq!(route::icao24_of(&w.cat, "PH-BKO").unwrap(), None);
    // Asked about before: not asked again (new registrations only).
    assert!(fr24::due(&w.cat, later).unwrap().is_empty());
    assert_eq!(fr24::forget_expired(&mut w.cat, later).unwrap(), 0);
    let st = fr24::status(&w.cat).unwrap();
    assert_eq!((st.new, st.asked), (0, 0));
    // A new registration tagged: due.
    let new = TagPath::parse_lightroom("Aircraft|Registration|C-FNEW").unwrap();
    MetadataService::new(&w.lib, &mut w.cat)
        .set_user_tags(ids[0], &[new])
        .unwrap();
    assert_eq!(fr24::due(&w.cat, later).unwrap(), ["C-FNEW"]);
    assert_eq!(fr24::status(&w.cat).unwrap().new, 1);
}

#[test]
fn accepting_a_route_replaces_the_route_keywords_accepted_before() {
    let (mut w, ids) = route_world(&["a.jpg"], &[PHOTO_S]);
    let p = |s: &str| TagPath::parse_lightroom(s).unwrap();
    // The wrong one first (WAW → YYZ, LOT8N), then the right one.
    route::accept(
        &mut w.cat,
        &w.lib,
        ids[0],
        &[
            p("Aircraft|From|WAW"),
            p("Aircraft|To|YYZ"),
            p("Aircraft|Flight|LOT8N"),
        ],
    )
    .unwrap();
    route::accept(
        &mut w.cat,
        &w.lib,
        ids[0],
        &[p("Aircraft|From|YYZ"), p("Aircraft|To|WAW")],
    )
    .unwrap();
    let mut tags: Vec<String> = MetadataService::new(&w.lib, &mut w.cat)
        .user_tags(ids[0])
        .unwrap()
        .iter()
        .map(TagPath::to_lightroom)
        .collect();
    tags.sort();
    assert_eq!(
        tags,
        [
            "Aircraft|From|YYZ",
            "Aircraft|Registration|C-GBIJ",
            "Aircraft|To|WAW"
        ]
    );
    assert!(route::accept(&mut w.cat, &w.lib, ids[0], &[p("Aircraft|Type|X")]).is_err());
}

#[test]
fn a_national_register_gives_build_year_code_type_and_airline_and_puts_misreads_right() {
    use keepsake_core::ai::aircraft::register::{self, Register};
    use keepsake_core::catalog::RegisterRow;
    let mut w = World::new(&["a.jpg"]);
    let row = |reg: &str, current: bool| RegisterRow {
        registration: reg.into(),
        source: "tc".into(),
        maker: Some("Boeing".into()),
        model: Some("777-333ER".into()),
        aircraft_type: Some("Boeing 777-300ER".into()),
        serial: Some("62400".into()),
        built: Some("2016".into()),
        mode_s: Some("C01A61".into()),
        owner: Some("Air Canada".into()),
        airline: Some("Air Canada".into()),
        current,
    };
    // Not downloaded: nothing.
    assert_eq!(register::get(&w.cat, "C-FJZS").unwrap(), None);
    assert_eq!(register::near(&w.cat, "C-FJ2S").unwrap(), None);
    register::import(
        &mut w.cat,
        Register::Tc,
        &[row("C-FJZS", true), row("C-GOLD", false)],
        Some("Thu, 19 Nov 2026 16:44:22 GMT"),
    )
    .unwrap();
    let src = register::sources(&w.cat).unwrap();
    assert_eq!(src.len(), 1);
    assert_eq!(src[0].rows, 2);
    assert_eq!(
        src[0].file_modified.as_deref(),
        Some("Thu, 19 Nov 2026 16:44:22 GMT")
    );
    assert_eq!(
        register::get(&w.cat, "cfjzs")
            .unwrap()
            .unwrap()
            .registration,
        "C-FJZS"
    );

    // The build year (before OpenSky's), and the transponder code.
    let k = built::kept(&w.cat, "C-FJZS").unwrap().unwrap();
    assert_eq!((k.built.as_str(), k.source.as_str()), ("2016", "tc"));
    assert_eq!(
        route::icao24_of(&w.cat, "C-FJZS").unwrap().as_deref(),
        Some("c01a61")
    );
    // Typed wins over the register.
    built::set_typed(&mut w.cat, &w.lib, "C-FJZS", Some("2015-06-01")).unwrap();
    assert_eq!(
        built::kept(&w.cat, "C-FJZS").unwrap().unwrap().source,
        "typed"
    );

    // What the lookup does not know, the register fills.
    let f = register::with_register(&w.cat, "C-FJZS", None)
        .unwrap()
        .unwrap();
    assert!(f.known);
    assert_eq!(f.aircraft_type.as_deref(), Some("Boeing 777-300ER"));
    assert_eq!(f.airline.as_deref(), Some("Air Canada"));
    assert_eq!(f.mode_s.as_deref(), Some("C01A61"));

    // A misread one character off a registered aircraft: that one. Not one
    // no longer registered, nor a registration listed.
    assert_eq!(
        register::near(&w.cat, "C-FJ2S").unwrap().as_deref(),
        Some("C-FJZS")
    );
    assert_eq!(register::near(&w.cat, "C-FJZS").unwrap(), None);
    assert_eq!(register::near(&w.cat, "C-G0LD").unwrap(), None);
    assert_eq!(register::near(&w.cat, "PH-BK0").unwrap(), None);

    // Read aircraft offers the real one ticked, the one read unticked, and
    // the register's type and airline.
    let item = scan::ReviewItem {
        asset: w.asset("a.jpg"),
        candidates: keepsake_core::ai::aircraft::marks::Candidates {
            registrations: vec![("C-FJ2S".into(), 0.97)],
            airlines: vec![],
            types: vec![],
            fins: vec![],
        },
        looks_like: None,
        full_size: true,
        made_ms: 0,
        review: scan::Review::Waiting,
    };
    let o = scan::offer_for(&w.cat, &item, &Confidence::default()).unwrap();
    let k: Vec<String> = o.keywords.iter().map(TagPath::to_lightroom).collect();
    assert!(
        k.contains(&"Aircraft|Registration|C-FJZS".to_owned()),
        "{k:?}"
    );
    assert!(
        k.contains(&"Aircraft|Registration|C-FJ2S".to_owned()),
        "{k:?}"
    );
    assert!(
        k.contains(&"Aircraft|Type|Boeing 777-300ER".to_owned()),
        "{k:?}"
    );
    assert!(
        k.contains(&"Aircraft|Airline|Air Canada".to_owned()),
        "{k:?}"
    );
    let unticked: Vec<String> = o.unticked.iter().map(TagPath::to_lightroom).collect();
    assert_eq!(unticked, ["Aircraft|Registration|C-FJ2S"]);

    // Removed: nothing again.
    register::remove(&mut w.cat, Register::Tc).unwrap();
    assert_eq!(register::get(&w.cat, "C-FJZS").unwrap(), None);
    assert!(register::sources(&w.cat).unwrap().is_empty());
}

/// OpenSky as it saw CN-RGY on 2026-06-08: only RAM211B leaving Toronto at
/// 02:30 UTC, not where it went.
struct RgySky;

impl route::Flights for RgySky {
    fn flights(&self, _: &str, begin: i64, end: i64) -> Result<Vec<Flight>, AiError> {
        // The day 2026-06-08, when the question covers it.
        Ok(if (begin..=end).contains(&1_780_876_800) {
            vec![Flight {
                first_seen: 1_780_885_806,
                last_seen: 1_780_909_269,
                departure: Some("CYYZ".into()),
                callsign: Some("RAM211B".into()),
                ..Flight::default()
            }]
        } else {
            Vec::new()
        })
    }
}

/// adsbdb's callsign routes, counting what is asked.
struct RgyRoutes(Mutex<u32>);

impl lookup::Lookup for RgyRoutes {
    fn lookup(&self, _: &str) -> Result<lookup::Found, AiError> {
        Ok(lookup::Found::default())
    }
    fn callsign_route(&self, cs: &str) -> Result<Option<(String, String)>, AiError> {
        *self.0.lock().unwrap() += 1;
        Ok((cs == "RAM211B").then(|| ("CYYZ".into(), "GMMN".into())))
    }
}

#[test]
fn a_flight_in_from_over_the_ocean_is_offered_from_the_next_take_off_and_its_callsign() {
    // A7402578.ARW: 2026-06-08 00:00:43 UTC, CN-RGY landing at Toronto
    // from Casablanca.
    let (mut w, ids) = route_world(&["a.jpg"], &[1_780_876_843]);
    let sky = RgySky;
    let routes = RgyRoutes(Mutex::new(0));
    let airports = Airports::builtin();
    let asked = route::run_job(&mut w.cat, &sky, Some(&routes), ids[0], Clock::AsRecorded).unwrap();
    // Two days asked, and RAM211B's route once.
    assert_eq!(asked.requests, 3);
    assert_eq!(*routes.0.lock().unwrap(), 1);
    let kept = route::kept_day(&w.cat, "c04826", "2026-06-08")
        .unwrap()
        .unwrap();
    assert_eq!(kept[0].arrival.as_deref(), Some("GMMN"));
    assert!(kept[0].by_callsign);
    // Run again: nothing asked.
    let again = route::run_job(&mut w.cat, &sky, Some(&routes), ids[0], Clock::AsRecorded).unwrap();
    assert_eq!(again.requests, 0);
    assert_eq!(*routes.0.lock().unwrap(), 1);
    let it = route::review(
        &w.cat,
        &airports,
        ids[0],
        Clock::AsRecorded,
        Source::OpenSky,
    )
    .unwrap();
    match it.state {
        route::State::Found(Pick::Several(v)) => {
            assert_eq!(v.len(), 2);
            let k: Vec<String> = route::keywords(&v[0].flight, &airports)
                .unwrap()
                .iter()
                .map(TagPath::to_lightroom)
                .collect();
            assert_eq!(
                k,
                [
                    "Aircraft|From|CMN",
                    "Aircraft|From country|Morocco",
                    "Aircraft|To|YYZ",
                    "Aircraft|To country|Canada"
                ]
            );
            assert_eq!(v[1].flight.callsign.as_deref(), Some("RAM211B"));
            assert_eq!(v[1].flight.arrival.as_deref(), Some("GMMN"));
        }
        s => panic!("{s:?}"),
    }
}

/// OpenSky out of credits until an hour from now, counting what is asked.
struct LimitedSky(Mutex<u32>, i64);

impl route::Flights for LimitedSky {
    fn flights(&self, _: &str, _: i64, _: i64) -> Result<Vec<Flight>, AiError> {
        *self.0.lock().unwrap() += 1;
        Err(route::limit_reached(self.1))
    }
}

#[test]
fn at_opensky_s_limit_the_route_jobs_wait_until_it_allows_more_and_do_not_fail() {
    // A7402789.ARW: OpenSky said 429, more in 61,899 s.
    let (mut w, ids) = route_world(&["a.jpg", "b.jpg"], &[PHOTO_S, PHOTO_S + 5 * 86_400]);
    assert_eq!(route::enqueue(&mut w.cat, &ids).unwrap(), 2);
    let until = keepsake_core::jobs::now_ms() + 3_600_000;
    let sky = Arc::new(LimitedSky(Mutex::new(0), until));
    let seen = Arc::new(Mutex::new(Vec::new()));
    w.service(fake(&AIR_CANADA, &seen))
        .with_flights(sky.clone())
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    // Asked once; the other photo waits without asking.
    assert_eq!(*sky.0.lock().unwrap(), 1);
    for id in &ids {
        let j = w
            .cat
            .read(|tx| tx.jobs().find(route::JOB_AI_ROUTE, *id))
            .unwrap()
            .unwrap();
        assert_eq!(j.state, JobState::Queued);
        assert_eq!(j.not_before_ms, until);
        assert!(j.error.unwrap().contains("OpenSky's limit is reached"));
    }
    let (st, _) = route::status(&w.cat, &ids).unwrap();
    assert_eq!((st.pending, st.failed), (2, 0));
}

/// adsbdb's callsign routes for Fill in route keywords, counting asks.
struct FillRoutes(Mutex<Vec<String>>);

impl lookup::Lookup for FillRoutes {
    fn lookup(&self, _: &str) -> Result<lookup::Found, AiError> {
        Ok(lookup::Found::default())
    }
    fn callsign_route(&self, cs: &str) -> Result<Option<(String, String)>, AiError> {
        self.0.lock().unwrap().push(cs.to_owned());
        Ok(match cs {
            "AT210" | "RAM210" => Some(("GMMN".into(), "CYYZ".into())),
            "RAM212" => Some(("GMMN".into(), "KJFK".into())),
            "JZA893" | "JZA895" => Some(("CYYZ".into(), "KIND".into())),
            "DAL8885" => Some(("KSAN".into(), "KPHL".into())),
            _ => None,
        })
    }
}

fn tag(w: &mut World, id: AssetId, keywords: &[&str]) {
    let mut all: Vec<TagPath> = MetadataService::new(&w.lib, &mut w.cat)
        .user_tags(id)
        .unwrap();
    all.extend(
        keywords
            .iter()
            .map(|k| TagPath::parse_lightroom(k).unwrap()),
    );
    MetadataService::new(&w.lib, &mut w.cat)
        .set_user_tags(id, &all)
        .unwrap();
}

#[test]
fn fill_in_route_keywords_adds_only_what_is_missing() {
    use keepsake_core::ai::aircraft::complete::{self, Filled};
    let (mut w, ids) = route_world(
        &["a.jpg", "b.jpg", "c.jpg", "d.jpg", "e.jpg"],
        &[PHOTO_S; 5],
    );
    let before = hashes(&w.media);
    let airports = Airports::builtin();
    let routes = FillRoutes(Mutex::new(Vec::new()));
    // A flight number: its airports and their countries.
    tag(&mut w, ids[0], &["Aircraft|Flight|AT210"]);
    let (f, asked) = complete::fill(
        &mut w.cat,
        &w.lib,
        &airports,
        Some(&routes),
        ids[0],
        Clock::AsRecorded,
    )
    .unwrap();
    assert_eq!(asked, 1);
    let added: Vec<String> = match f {
        Filled::Added(v) => v.iter().map(TagPath::to_lightroom).collect(),
        f => panic!("{f:?}"),
    };
    assert_eq!(
        added,
        [
            "Aircraft|From|CMN",
            "Aircraft|From country|Morocco",
            "Aircraft|To|YYZ",
            "Aircraft|To country|Canada"
        ]
    );
    let kw = w.keywords(ids[0]);
    assert!(kw.contains(&"Aircraft|Flight|AT210".to_owned()), "{kw:?}");
    assert!(kw.contains(&"Aircraft|From|CMN".to_owned()), "{kw:?}");
    assert!(sidecars(&w).contains("Aircraft|To country|Canada"));
    // Again: nothing missing, nothing asked.
    let (f, asked) = complete::fill(
        &mut w.cat,
        &w.lib,
        &airports,
        Some(&routes),
        ids[0],
        Clock::AsRecorded,
    )
    .unwrap();
    assert!(matches!(f, Filled::Nothing(_)), "{f:?}");
    assert_eq!(asked, 0);

    // A flight that does not fly what the photo says: nothing added.
    tag(
        &mut w,
        ids[1],
        &["Aircraft|Flight|AT210", "Aircraft|From|LHR"],
    );
    let (f, _) = complete::fill(
        &mut w.cat,
        &w.lib,
        &airports,
        Some(&routes),
        ids[1],
        Clock::AsRecorded,
    )
    .unwrap();
    match f {
        Filled::Nothing(why) => assert!(why.contains("AT210 flies CMN → YYZ"), "{why}"),
        f => panic!("{f:?}"),
    }
    assert!(!w.keywords(ids[1]).contains(&"Aircraft|To|YYZ".to_owned()));

    // Only From: the flight from the aircraft's flights kept that day
    // (ROU2000 left YYZ at 12:16, 15 min before the photo); and only the
    // country added where the airport was typed as its ICAO code.
    let sky = FakeSky {
        asked: Mutex::new(Vec::new()),
    };
    route::run_job(&mut w.cat, &sky, None, ids[2], Clock::AsRecorded).unwrap();
    tag(&mut w, ids[2], &["Aircraft|From|YYZ"]);
    let (f, asked) = complete::fill(
        &mut w.cat,
        &w.lib,
        &airports,
        None,
        ids[2],
        Clock::AsRecorded,
    )
    .unwrap();
    assert_eq!(asked, 0);
    match f {
        Filled::Added(v) => {
            let v: Vec<String> = v.iter().map(TagPath::to_lightroom).collect();
            assert_eq!(
                v,
                ["Aircraft|From country|Canada", "Aircraft|Flight|RV2000"]
            );
        }
        f => panic!("{f:?}"),
    }

    // No route keyword: nothing.
    let (f, _) = complete::fill(
        &mut w.cat,
        &w.lib,
        &airports,
        None,
        ids[3],
        Clock::AsRecorded,
    )
    .unwrap();
    assert!(matches!(f, Filled::Nothing(why) if why.contains("no route keyword")));
    // A flight number without adsbdb: says to turn it on.
    tag(&mut w, ids[4], &["Aircraft|Flight|AT210"]);
    assert!(matches!(
        complete::fill(&mut w.cat, &w.lib, &airports, None, ids[4], Clock::AsRecorded),
        Err(AiError::Cloud { retry: false, message }) if message.contains("Look registrations up online")
    ));
    // The media is never touched.
    assert_eq!(hashes(&w.media), before);

    // The worker: queued, filled in the background, states for the window.
    tag(&mut w, ids[3], &["Aircraft|Flight|AT210"]);
    complete::enqueue(&mut w.cat, &ids[3..4]).unwrap();
    assert_eq!(
        complete::state(&w.cat, ids[3]).unwrap(),
        complete::State::Waiting
    );
    let seen = Arc::new(Mutex::new(Vec::new()));
    w.service(fake(&AIR_CANADA, &seen))
        .with_lookup(Arc::new(FillRoutes(Mutex::new(Vec::new()))))
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    match complete::state(&w.cat, ids[3]).unwrap() {
        complete::State::Done(now) => {
            let now: Vec<String> = now.iter().map(TagPath::to_lightroom).collect();
            assert!(now.contains(&"Aircraft|To|YYZ".to_owned()), "{now:?}");
        }
        s => panic!("{s:?}"),
    }
}

#[test]
fn the_flight_report_lists_each_flight_photographed_once() {
    let (mut w, ids) = route_world(
        &["a.jpg", "b.jpg", "c.jpg", "d.jpg"],
        &[PHOTO_S + 60, PHOTO_S, PHOTO_S + 86_400, PHOTO_S],
    );
    let flight = [
        "Aircraft|Flight|RV2000",
        "Aircraft|From|YYZ",
        "Aircraft|From country|Canada",
        "Aircraft|Airline|Air Canada Rouge",
        "Aircraft|Type|Airbus A321",
    ];
    // Two photos of one flight; the same aircraft the next day, another.
    tag(&mut w, ids[0], &flight);
    tag(&mut w, ids[1], &flight);
    tag(&mut w, ids[2], &["Aircraft|To|YUL"]);
    // ids[3]: a registration and no route.
    let r = w.cat.read(|tx| tx.flight_report().flights()).unwrap();
    assert_eq!(r.without_route, 1);
    assert_eq!(r.flights.len(), 2);
    // Newest first.
    assert_eq!(r.flights[0].to.as_deref(), Some("YUL"));
    let f = &r.flights[1];
    assert_eq!(f.photos, 2);
    assert_eq!(f.flight.as_deref(), Some("RV2000"));
    assert_eq!(f.from.as_deref(), Some("YYZ"));
    assert_eq!(f.from_country.as_deref(), Some("Canada"));
    assert_eq!(f.airline.as_deref(), Some("Air Canada Rouge"));
    assert_eq!(f.aircraft_type.as_deref(), Some("Airbus A321"));
    assert_eq!(f.registration.as_deref(), Some("C-GBIJ"));
    assert_eq!(f.first_photo.as_deref(), Some("b.jpg"));
    assert_eq!(f.first_id, ids[1]);
    // Written only where asked, with a byte-order mark.
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("flights.csv");
    keepsake_core::report::write_flights_csv(&r, &out).unwrap();
    assert!(std::fs::read(&out)
        .unwrap()
        .starts_with(&[0xEF, 0xBB, 0xBF]));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    let csv = keepsake_core::report::flights_to_csv(&r);
    let lines: Vec<&str> = csv.split("\r\n").collect();
    assert_eq!(lines[0], keepsake_core::report::FLIGHT_CSV_HEADER);
    assert!(
        lines[2].contains(",RV2000,YYZ,Canada,,,Air Canada Rouge,C-GBIJ,Airbus A321,2,b.jpg"),
        "{}",
        lines[2]
    );
}

#[test]
fn fill_in_finds_the_flight_number_from_the_flight_the_other_way() {
    use keepsake_core::ai::aircraft::complete::{self, Filled};
    // A7402561_1.jpg: CN-RGY landing at YYZ from CMN at 00:00 UTC, From and
    // To accepted from the flight in; OpenSky saw only RAM211B leave YYZ.
    let (mut w, ids) = route_world(&["a.jpg"], &[1_780_876_833]);
    route::run_job(&mut w.cat, &RgySky, None, ids[0], Clock::AsRecorded).unwrap();
    tag(&mut w, ids[0], &["Aircraft|From|CMN", "Aircraft|To|YYZ"]);
    let routes = FillRoutes(Mutex::new(Vec::new()));
    let (f, asked) = complete::fill(
        &mut w.cat,
        &w.lib,
        &Airports::builtin(),
        Some(&routes),
        ids[0],
        Clock::AsRecorded,
    )
    .unwrap();
    // RAM210 and RAM212 asked; only RAM210 flies CMN to YYZ.
    assert_eq!(*routes.0.lock().unwrap(), ["RAM210", "RAM212"]);
    assert_eq!(asked, 2);
    match f {
        Filled::Added(v) => {
            let v: Vec<String> = v.iter().map(TagPath::to_lightroom).collect();
            assert_eq!(
                v,
                [
                    "Aircraft|From country|Morocco",
                    "Aircraft|To country|Canada",
                    "Aircraft|Flight|AT210"
                ]
            );
        }
        f => panic!("{f:?}"),
    }
    assert_eq!(complete::neighbours("RAM211B"), ["RAM210", "RAM212"]);
    assert_eq!(complete::neighbours("ACA1"), ["ACA2"]);
    assert!(complete::neighbours("N12345").is_empty());
}

/// OpenSky as it saw N698DL on 2026-06-07/08 and C-FEJC on 2026-06-12/13.
struct LegsSky;

impl route::Flights for LegsSky {
    fn flights(&self, icao: &str, begin: i64, end: i64) -> Result<Vec<Flight>, AiError> {
        // Each day's flights, for every day the question covers.
        Ok((0..=(end - begin) / 86_400)
            .flat_map(|d| self.day(icao, begin + d * 86_400))
            .collect())
    }
}

impl LegsSky {
    fn day(&self, icao: &str, begin: i64) -> Vec<Flight> {
        let f = |first: i64, last: i64, dep: &str, arr: &str, cs: &str| Flight {
            first_seen: first,
            last_seen: last,
            departure: Some(dep.into()),
            arrival: Some(arr.into()),
            callsign: Some(cs.into()),
            ..Flight::default()
        };
        match (icao, begin) {
            ("c04826", 1_780_790_400) => {
                vec![f(1_780_851_600, 1_780_856_880, "KATL", "KPHL", "DAL8885")]
            }
            ("c04826", 1_780_876_800) => {
                vec![f(1_780_881_900, 1_780_888_440, "CYYZ", "KMSP", "DAL8885")]
            }
            ("c04826", 1_781_222_400) => {
                vec![f(1_781_282_760, 1_781_287_560, "KBOS", "CYYZ", "JZA693")]
            }
            ("c04826", 1_781_308_800) => {
                vec![f(1_781_360_820, 1_781_364_840, "KIND", "CYYZ", "JZA894")]
            }
            _ => Vec::new(),
        }
    }
}

#[test]
fn fill_in_takes_a_through_flight_number_and_offers_a_choice_of_two() {
    use keepsake_core::ai::aircraft::complete::{self, Filled};
    let airports = Airports::builtin();
    // N698DL, 2026-06-07 22:52 UTC: DAL8885 landed at PHL, next left YYZ;
    // the photo PHL -> YYZ between.
    let (mut w, ids) = route_world(&["a.jpg", "b.jpg"], &[1_780_872_720, 1_781_307_660]);
    for id in &ids {
        route::run_job(&mut w.cat, &LegsSky, None, *id, Clock::AsRecorded).unwrap();
    }
    tag(&mut w, ids[0], &["Aircraft|From|PHL", "Aircraft|To|YYZ"]);
    let routes = FillRoutes(Mutex::new(Vec::new()));
    let (f, _) = complete::fill(
        &mut w.cat,
        &w.lib,
        &airports,
        Some(&routes),
        ids[0],
        Clock::AsRecorded,
    )
    .unwrap();
    match f {
        Filled::Added(v) => assert!(
            v.iter()
                .any(|k| k.to_lightroom() == "Aircraft|Flight|DL8885"),
            "{v:?}"
        ),
        f => panic!("{f:?}"),
    }
    // C-FEJC, 2026-06-12 23:41 UTC, YYZ -> IND: QK893 and QK895 both fly it.
    tag(&mut w, ids[1], &["Aircraft|From|YYZ", "Aircraft|To|IND"]);
    let (f, _) = complete::fill(
        &mut w.cat,
        &w.lib,
        &airports,
        Some(&routes),
        ids[1],
        Clock::AsRecorded,
    )
    .unwrap();
    let (why, numbers) = match f {
        Filled::Choose(why, n) => (why, n),
        f => panic!("{f:?}"),
    };
    assert_eq!(
        why,
        "QK893 and QK895 fly YYZ → IND: choose the one photographed"
    );
    assert_eq!(numbers, ["QK893", "QK895"]);
    assert!(!w
        .keywords(ids[1])
        .iter()
        .any(|k| k.starts_with("Aircraft|Flight|")));
    // Kept in the job and read back.
    let kept = complete::with_choices(&why, &numbers);
    assert_eq!(complete::choices_of(&kept), (why.clone(), numbers.clone()));
    assert_eq!(complete::choices_of("plain"), ("plain".into(), Vec::new()));
    // Chosen: only the flight number added.
    let k = complete::choose(&mut w.cat, &w.lib, ids[1], "QK895").unwrap();
    assert_eq!(k.to_lightroom(), "Aircraft|Flight|QK895");
    assert!(w
        .keywords(ids[1])
        .contains(&"Aircraft|Flight|QK895".to_owned()));
}

#[test]
fn a_typed_flight_number_suggests_the_routes_other_photos_have() {
    use keepsake_core::ai::aircraft::complete;
    let n = names(7);
    let refs: Vec<&str> = n.iter().map(String::as_str).collect();
    // Older photos first: the last two are the most recent.
    let times: Vec<i64> = (0..7).map(|i| PHOTO_S + i * 86_400).collect();
    let (mut w, ids) = route_world(&refs, &times);
    let airports = Airports::builtin();
    // Two photos of AC856 YYZ to LHR (countries from the airport list);
    // one of AC856 YVR to LHR, the most recent; one written ACA856 the
    // same as YYZ to LHR; one of another flight; one with no airports.
    tag(
        &mut w,
        ids[0],
        &[
            "Aircraft|Flight|AC856",
            "Aircraft|From|YYZ",
            "Aircraft|To|LHR",
        ],
    );
    tag(
        &mut w,
        ids[1],
        &[
            "Aircraft|Flight|AC856",
            "Aircraft|From|YYZ",
            "Aircraft|From country|Canada",
            "Aircraft|To|LHR",
        ],
    );
    tag(
        &mut w,
        ids[2],
        &[
            "Aircraft|Flight|ACA856",
            "Aircraft|From|YYZ",
            "Aircraft|To|LHR",
        ],
    );
    tag(
        &mut w,
        ids[3],
        &[
            "Aircraft|Flight|AC857",
            "Aircraft|From|LHR",
            "Aircraft|To|YYZ",
        ],
    );
    tag(&mut w, ids[4], &["Aircraft|Flight|AC856"]);
    tag(
        &mut w,
        ids[5],
        &[
            "Aircraft|Flight|AC856",
            "Aircraft|From|YVR",
            "Aircraft|To|LHR",
        ],
    );
    let before = hashes(&w.media);
    let tags_before: Vec<_> = ids
        .iter()
        .map(|id| w.cat.read(|tx| tx.tags().for_asset(*id)).unwrap())
        .collect();

    let s = complete::suggest(&w.cat, &airports, "ac 856", &[ids[6]]).unwrap();
    assert_eq!(s.len(), 2, "{s:?}");
    // Most photos first.
    assert_eq!(s[0].from.as_deref(), Some("YYZ"));
    assert_eq!(s[0].to.as_deref(), Some("LHR"));
    assert_eq!(s[0].photos, 3);
    assert_eq!(s[0].latest_ms, Some(times[2] * 1000));
    let k: Vec<String> = s[0].keywords.iter().map(TagPath::to_lightroom).collect();
    assert_eq!(
        k,
        [
            "Aircraft|From|YYZ",
            "Aircraft|From country|Canada",
            "Aircraft|To|LHR",
            "Aircraft|To country|United Kingdom",
            "Aircraft|Flight|AC856"
        ]
    );
    assert_eq!(s[1].from.as_deref(), Some("YVR"));
    assert_eq!(s[1].photos, 1);

    // Equal counts: the most recent first.
    tag(
        &mut w,
        ids[6],
        &[
            "Aircraft|Flight|AC856",
            "Aircraft|From|YVR",
            "Aircraft|To|LHR",
        ],
    );
    let s = complete::suggest(&w.cat, &airports, "AC856", &[ids[0]]).unwrap();
    assert_eq!((s[0].photos, s[1].photos), (2, 2));
    assert_eq!(s[0].from.as_deref(), Some("YVR"));

    // A photo is not its own suggestion; an unknown or blank number: none.
    assert!(complete::suggest(&w.cat, &airports, "AC857", &[ids[3]])
        .unwrap()
        .is_empty());
    assert!(complete::suggest(&w.cat, &airports, "ZZ1", &[])
        .unwrap()
        .is_empty());
    assert!(complete::suggest(&w.cat, &airports, "  ", &[])
        .unwrap()
        .is_empty());
    // Nothing changed: no keyword, no file.
    for (id, had) in ids.iter().zip(&tags_before).take(6) {
        assert_eq!(&w.cat.read(|tx| tx.tags().for_asset(*id)).unwrap(), had);
    }
    assert_eq!(hashes(&w.media), before);
}

#[test]
fn flight_numbers_are_compared_as_sold() {
    use keepsake_core::ai::aircraft::complete::flight_key;
    assert_eq!(flight_key("aca 856"), "AC856");
    assert_eq!(flight_key("AC-856"), "AC856");
    assert_eq!(flight_key("ACA856"), flight_key("AC856"));
    assert_eq!(flight_key(" "), "");
}

// ---- Add route chooses OpenSky or Flightradar24 by the photo's date (ADR-050) ----

/// Flightradar24 with a plan that gives flights from `from` (Unix s):
/// earlier questions are refused, saying so, as the real API does.
struct PlanFr24 {
    from: i64,
    asked: Mutex<Vec<(i64, i64)>>,
    said: Mutex<Option<i64>>,
}

impl PlanFr24 {
    fn new(from: i64) -> Self {
        Self {
            from,
            asked: Mutex::new(Vec::new()),
            said: Mutex::new(None),
        }
    }
}

impl route::Flights for PlanFr24 {
    fn flights(&self, _: &str, begin: i64, end: i64) -> Result<Vec<Flight>, AiError> {
        self.asked.lock().unwrap().push((begin, end));
        if begin < self.from {
            *self.said.lock().unwrap() = Some(self.from);
            return Err(AiError::Cloud {
                message: "Flightradar24: your API plan gives flights from a later date".into(),
                retry: false,
            });
        }
        Ok(Vec::new())
    }
    fn source(&self) -> Source {
        Source::Flightradar
    }
    fn take_history_from(&self) -> Option<i64> {
        self.said.lock().unwrap().take()
    }
}

const DAY: i64 = 86_400;

#[test]
fn automatic_asks_flightradar24_for_recent_photos_and_opensky_for_older_ones() {
    use keepsake_core::ai::aircraft::route::{Mode, Sources};
    let now = PHOTO_S + 10 * DAY;
    let (mut w, ids) = route_world(&["recent.jpg", "old.jpg"], &[PHOTO_S, now - 60 * DAY]);
    let sky = Arc::new(FakeSky {
        asked: Mutex::new(Vec::new()),
    });
    let fr = Arc::new(PlanFr24::new(now - 30 * DAY));
    let sources = Sources::new(Mode::Auto, Some(sky.clone()), Some(fr.clone()), 30);

    // Ten days old: within the plan's 30 days, Flightradar24.
    let a =
        route::run_job_auto(&mut w.cat, &sources, None, ids[0], Clock::AsRecorded, now).unwrap();
    assert_eq!(a.source, Some(Source::Flightradar));
    assert_eq!(fr.asked.lock().unwrap().len(), 1);
    assert!(sky.asked.lock().unwrap().is_empty());
    // Sixty days old: before it, OpenSky; Flightradar24 is not asked.
    let a =
        route::run_job_auto(&mut w.cat, &sources, None, ids[1], Clock::AsRecorded, now).unwrap();
    assert_eq!(a.source, Some(Source::OpenSky));
    assert_eq!(fr.asked.lock().unwrap().len(), 1);
    assert_eq!(sky.asked.lock().unwrap().len(), 1);

    // The window says where each photo's flights come from.
    let airports = Airports::builtin();
    let choose = sources.choose(now);
    let r = route::review_with(&w.cat, &airports, ids[0], Clock::AsRecorded, &choose).unwrap();
    assert_eq!(r.source, Some(Source::Flightradar));
    assert!(
        matches!(r.state, route::State::Found(Pick::Nothing)),
        "{r:?}"
    );
    let r = route::review_with(&w.cat, &airports, ids[1], Clock::AsRecorded, &choose).unwrap();
    assert_eq!(r.source, Some(Source::OpenSky));
    assert!(matches!(r.state, route::State::Found(_)), "{r:?}");

    // Forced to one: only it, whatever the date.
    let only = Sources::new(
        Mode::Only(Source::OpenSky),
        Some(sky.clone()),
        Some(fr.clone()),
        30,
    );
    route::again(&mut w.cat, &ids[..1], Clock::AsRecorded, Source::OpenSky).unwrap();
    let a = route::run_job_auto(&mut w.cat, &only, None, ids[0], Clock::AsRecorded, now).unwrap();
    assert_eq!(a.source, Some(Source::OpenSky));
    assert_eq!(fr.asked.lock().unwrap().len(), 1);
}

#[test]
fn flightradar24_s_plan_is_learnt_from_its_answer_and_older_photos_go_to_opensky() {
    use keepsake_core::ai::aircraft::route::{Mode, Sources};
    let now = PHOTO_S + 10 * DAY;
    // The plan gives five days, not the thirty assumed.
    let (mut w, ids) = route_world(
        &["a.jpg", "b.jpg", "c.jpg"],
        &[PHOTO_S, PHOTO_S + DAY, now - 2 * DAY],
    );
    let sky = Arc::new(FakeSky {
        asked: Mutex::new(Vec::new()),
    });
    let fr = Arc::new(PlanFr24::new(now - 5 * DAY));
    let learnt = Arc::new(Mutex::new(Vec::new()));
    let keep = learnt.clone();
    let sources = Sources::new(Mode::Auto, Some(sky.clone()), Some(fr.clone()), 30)
        .on_learn(Arc::new(move |d| keep.lock().unwrap().push(d)));

    // Ten days old: Flightradar24 refuses it; its plan's days are learnt
    // and kept, and OpenSky answers instead (not failed).
    let a =
        route::run_job_auto(&mut w.cat, &sources, None, ids[0], Clock::AsRecorded, now).unwrap();
    assert_eq!(a.source, Some(Source::OpenSky));
    assert_eq!(a.fr24_days, Some(5));
    assert_eq!(sources.fr24_days(), 5);
    assert_eq!(*learnt.lock().unwrap(), [5]);
    assert_eq!(fr.asked.lock().unwrap().len(), 1);
    assert_eq!(sky.asked.lock().unwrap().len(), 1);
    // Nine days old: straight to OpenSky now.
    let a =
        route::run_job_auto(&mut w.cat, &sources, None, ids[1], Clock::AsRecorded, now).unwrap();
    assert_eq!(a.source, Some(Source::OpenSky));
    assert_eq!(fr.asked.lock().unwrap().len(), 1);
    // Two days old: still Flightradar24's.
    let a =
        route::run_job_auto(&mut w.cat, &sources, None, ids[2], Clock::AsRecorded, now).unwrap();
    assert_eq!(a.source, Some(Source::Flightradar));
    assert_eq!(*learnt.lock().unwrap(), [5]);

    // Forced to Flightradar24: its refusal is the job's error.
    let forced = Sources::new(
        Mode::Only(Source::Flightradar),
        Some(sky.clone()),
        Some(fr.clone()),
        30,
    );
    let (mut w2, ids2) = route_world(&["d.jpg"], &[PHOTO_S]);
    assert!(matches!(
        route::run_job_auto(&mut w2.cat, &forced, None, ids2[0], Clock::AsRecorded, now),
        Err(AiError::Cloud { retry: false, .. })
    ));
}

#[test]
fn automatic_uses_the_one_set_up_and_says_when_neither_can_be_asked() {
    use keepsake_core::ai::aircraft::route::{Mode, Sources};
    let now = PHOTO_S + 10 * DAY;
    let (mut w, ids) = route_world(&["recent.jpg", "old.jpg"], &[PHOTO_S, now - 60 * DAY]);
    let sky = Arc::new(FakeSky {
        asked: Mutex::new(Vec::new()),
    });
    // No Flightradar24 token: OpenSky for the recent photo too.
    let sources = Sources::new(Mode::Auto, Some(sky.clone()), None, 30);
    let a =
        route::run_job_auto(&mut w.cat, &sources, None, ids[0], Clock::AsRecorded, now).unwrap();
    assert_eq!(a.source, Some(Source::OpenSky));

    // No OpenSky client: the old photo cannot be asked, and it says why.
    let fr = Arc::new(PlanFr24::new(now - 30 * DAY));
    let sources = Sources::new(Mode::Auto, None, Some(fr.clone()), 30);
    match route::run_job_auto(&mut w.cat, &sources, None, ids[1], Clock::AsRecorded, now) {
        Err(AiError::Cloud { message, retry }) => {
            assert!(!retry);
            assert!(
                message.contains("older than your Flightradar24 plan's 30 days"),
                "{message}"
            );
        }
        r => panic!("{r:?}"),
    }
    assert!(fr.asked.lock().unwrap().is_empty());
}

/// Stands in for adsbdb: knows JA02WJ (a JAL A350-1000) and JA02MJ (a
/// 787-9, to choose between); counts what it is asked.
struct JalLookup {
    asked: Mutex<Vec<String>>,
}

impl lookup::Lookup for JalLookup {
    fn lookup(&self, registration: &str) -> Result<lookup::Found, AiError> {
        self.asked.lock().unwrap().push(registration.to_owned());
        let (status, body) = match registration {
            "JA02WJ" => (
                200,
                r#"{"response":{"aircraft":{"type":"A350 1041","icao_type":"A35K","manufacturer":"Airbus","registered_owner":"Japan Airlines","registered_owner_country_name":"Japan"}}}"#,
            ),
            "JA02MJ" => (
                200,
                r#"{"response":{"aircraft":{"type":"787 9","icao_type":"B789","manufacturer":"Boeing","registered_owner":"Someone Else","registered_owner_country_name":"Japan"}}}"#,
            ),
            _ => (404, r#"{"response":"unknown aircraft"}"#),
        };
        lookup::parse(
            registration,
            status,
            body,
            &keepsake_core::ai::aircraft::marks::Airlines::builtin(),
        )
    }
}

#[test]
fn a_registration_read_one_letter_off_is_asked_about_and_offered_as_the_known_aircraft() {
    use keepsake_core::ai::aircraft::near;
    let mut w = World::new(&["a.jpg"]);
    let a = w.asset("a.jpg");
    scan::enqueue(&mut w.cat, &[a]).unwrap();
    let registry = Arc::new(JalLookup {
        asked: Mutex::new(Vec::new()),
    });
    let seen = Arc::new(Mutex::new(Vec::new()));
    // A7405081: the W of JA02WJ read as N.
    w.service(fake(&["JA02NJ"], &seen))
        .with_lookup(registry.clone())
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    let asked = registry.asked.lock().unwrap().clone();
    assert_eq!(asked[0], "JA02NJ", "{asked:?}");
    assert!(asked.contains(&"JA02WJ".to_owned()), "{asked:?}");
    assert!(asked.len() <= 1 + near::MAX_ASKS, "{asked:?}");
    assert!(
        asked.iter().all(|r| r.starts_with("JA02")),
        "the country and the digits stay: {asked:?}"
    );

    // Two known aircraft are one letter off: with nothing else to go by,
    // neither is offered.
    let conf = Confidence::default();
    let item = scan::review_items(&w.cat, &[a]).unwrap().remove(0);
    let (offer, _) = scan::offer_full(&w.cat, &item, &conf).unwrap();
    assert!(offer.near.is_empty(), "{:?}", offer.near);
    // The photo looks like an A350-1000: that one is offered, ticked, with
    // what was read beside it unticked, and what adsbdb says of it.
    let mut item = item;
    item.looks_like = Some("Airbus A350-1000".into());
    let (offer, found) = scan::offer_full(&w.cat, &item, &conf).unwrap();
    let names = |v: &[TagPath]| -> Vec<String> { v.iter().map(TagPath::to_lightroom).collect() };
    assert_eq!(
        offer.near,
        [scan::Near {
            read: "JA02NJ".into(),
            real: "JA02WJ".into(),
            by: lookup::SERVICE,
        }]
    );
    let kw = names(&offer.keywords);
    assert_eq!(kw[0], "Aircraft|Registration|JA02WJ", "{kw:?}");
    assert_eq!(kw[1], "Aircraft|Registration|JA02NJ", "{kw:?}");
    assert!(
        kw.contains(&"Aircraft|Type|Airbus A350-1000".to_owned()),
        "{kw:?}"
    );
    assert_eq!(names(&offer.unticked), ["Aircraft|Registration|JA02NJ"]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].registration, "JA02WJ");

    // Read again: nothing asked again.
    let before = registry.asked.lock().unwrap().len();
    scan::enqueue(&mut w.cat, &[a]).unwrap();
    w.service(fake(&["JA02NJ"], &seen))
        .with_lookup(registry.clone())
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(registry.asked.lock().unwrap().len(), before);
    // A registration adsbdb knows is not changed.
    assert_eq!(
        near::fix(&w.cat, "JA02WJ", &near::Hints::default()).unwrap(),
        None
    );
}

#[test]
fn a_burst_is_one_shot_and_what_its_photos_read_is_offered_together() {
    use keepsake_core::catalog::AiAnalysis;
    let files = ["a.jpg", "b.jpg", "c.jpg", "d.jpg", "e.jpg"];
    let mut w = World::new(&files);
    let [a, b, c, d, e] = files.map(|n| w.asset(n));
    // A burst 3 s apart at most, 10.2 s long (like A7405080 to A7405100).
    let db = rusqlite::Connection::open(w.lib.catalog_path()).unwrap();
    for (id, t) in [(a, 0i64), (b, 2_100), (c, 4_600), (d, 7_400), (e, 10_200)] {
        db.execute(
            "UPDATE assets SET capture_ts_ms = ?2, camera_make = 'SONY', camera_model = 'ILCE-7M4' WHERE id = ?1",
            rusqlite::params![id, 1_000_000 + t],
        )
        .unwrap();
    }
    drop(db);
    let v = Embedding::new((0..64).map(|i| ((i as f32) * 0.3).sin()).collect()).unwrap();
    let (q, scale) = v.quantize();
    for id in [a, b, c, d, e] {
        w.cat
            .write(|tx| {
                tx.ai().put_analysis(&AiAnalysis {
                    asset_id: id,
                    model: keepsake_core::ai::MODEL_ID.to_owned(),
                    embedding: q.clone(),
                    emb_scale: scale,
                    dhash: 0,
                    made_ms: 0,
                    tag_list: 0,
                })
            })
            .unwrap();
    }
    // The shot reaches e, 10.2 s after a: each photo is 3 s or less after
    // the one before (10 s either side of a alone did not reach it).
    let n = scan::neighbours(&w.cat, a, scan::NEIGHBOUR_MS).unwrap();
    let ids: Vec<AssetId> = n.iter().map(|x| x.asset).collect();
    assert_eq!(ids, [b, c, d, e]);
    assert!(n.iter().all(|x| x.alike));
    // From the middle, both ways.
    assert_eq!(
        scan::neighbours(&w.cat, c, scan::NEIGHBOUR_MS)
            .unwrap()
            .len(),
        4
    );
    // A gap of 2.5 s: the 2.8 s from c to d ends the shot there.
    let n = scan::neighbours(&w.cat, a, 2_500).unwrap();
    let ids: Vec<AssetId> = n.iter().map(|x| x.asset).collect();
    assert_eq!(ids, [b, c]);

    // a and c read the registration, c also a slip; b reads only the
    // airline; d and e nothing.
    let put = |w: &mut World, id: AssetId, json: &str| {
        w.cat
            .write(|tx| tx.ai().put_aircraft_read(id, json, None, true, 10))
            .unwrap();
    };
    put(
        &mut w,
        a,
        r#"{"registrations":[["JA02WJ",0.82]],"airlines":[],"types":[]}"#,
    );
    put(
        &mut w,
        b,
        r#"{"registrations":[],"airlines":[["Japan Airlines",0.9]],"types":[]}"#,
    );
    put(
        &mut w,
        c,
        r#"{"registrations":[["JA02NJ",0.95],["JA02WJ",0.9]],"airlines":[],"types":[]}"#,
    );
    put(
        &mut w,
        d,
        r#"{"registrations":[],"airlines":[],"types":[]}"#,
    );
    let item = scan::review_items(&w.cat, &[a]).unwrap().remove(0);
    let (it, support) = scan::with_shot(&w.cat, &item, &[b, c, d, e]).unwrap();
    // Read on two photos, JA02WJ comes first, at the surest it was read.
    assert_eq!(
        support,
        [("JA02WJ".to_owned(), 2), ("JA02NJ".to_owned(), 1)]
    );
    assert_eq!(it.candidates.registrations[0], ("JA02WJ".to_owned(), 0.9));
    assert_eq!(it.candidates.airlines, [("Japan Airlines".to_owned(), 0.9)]);
    let offer = scan::offer_for(&w.cat, &it, &Confidence::default()).unwrap();
    let kw: Vec<String> = offer.keywords.iter().map(TagPath::to_lightroom).collect();
    assert_eq!(kw[0], "Aircraft|Registration|JA02WJ", "{kw:?}");
    assert!(
        kw.contains(&"Aircraft|Airline|Japan Airlines".to_owned()),
        "{kw:?}"
    );
    // With no shot, the read is as it was.
    let (alone, support) = scan::with_shot(&w.cat, &item, &[]).unwrap();
    assert_eq!(alone, item);
    assert_eq!(support, [("JA02WJ".to_owned(), 1)]);
}

/// Stands in for Flightradar24: saw JA02WJ (a JAL A350-1000) fly; counts
/// what it is asked.
struct JalFr24 {
    asked: Mutex<Vec<String>>,
}

impl route::Fr24Aircrafts for JalFr24 {
    fn aircraft(
        &self,
        registration: &str,
        _begin: i64,
        _end: i64,
    ) -> Result<Option<lookup::Fr24Aircraft>, AiError> {
        self.asked.lock().unwrap().push(registration.to_owned());
        Ok((registration == "JA02WJ").then(|| lookup::Fr24Aircraft {
            asked_ms: keepsake_core::jobs::now_ms(),
            found: true,
            icao_type: Some("A35K".into()),
            operator: Some("JAL".into()),
            painted_as: Some("JAL".into()),
            hex: Some("840A8E".into()),
            seen: Some("2026-11-22".into()),
            expired: false,
        }))
    }
}

#[test]
fn flightradar24_is_asked_about_a_mark_one_letter_off_when_adsbdb_knows_none() {
    use keepsake_core::ai::aircraft::{fr24, near};
    let mut w = World::new(&["a.jpg"]);
    let a = w.asset("a.jpg");
    scan::enqueue(&mut w.cat, &[a]).unwrap();
    // adsbdb knows none of JA02NJ and the marks one letter off it.
    let registry = Arc::new(FakeLookup {
        asked: Mutex::new(Vec::new()),
    });
    let seen = Arc::new(Mutex::new(Vec::new()));
    w.service(fake(&["JA02NJ"], &seen))
        .with_lookup(registry.clone())
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    let variants = near::variants("JA02NJ");
    assert_eq!(registry.asked.lock().unwrap().len(), 1 + variants.len());

    // Flightradar24 is asked about what was read and each mark, nothing
    // accepted yet: the read photo anchors the job.
    let due: Vec<String> = fr24::near_due(&w.cat)
        .unwrap()
        .into_iter()
        .map(|(r, id)| {
            assert_eq!(id, a);
            r
        })
        .collect();
    let mut want = vec!["JA02NJ".to_owned()];
    want.extend(variants.iter().cloned());
    assert_eq!(due, want);
    assert!(fr24::enqueue(&mut w.cat).unwrap());
    fr24::stop(&mut w.cat).unwrap();
    let client = JalFr24 {
        asked: Mutex::new(Vec::new()),
    };
    let b = fr24::run_batch(
        &mut w.cat,
        &client,
        &AtomicBool::new(false),
        std::time::Duration::ZERO,
    )
    .unwrap();
    assert_eq!((b.found, b.left), (1, 0));

    // It saw JA02WJ fly: offered for JA02NJ, with its type and airline.
    let item = scan::review_items(&w.cat, &[a]).unwrap().remove(0);
    let (offer, _) = scan::offer_full(&w.cat, &item, &Confidence::default()).unwrap();
    assert_eq!(
        offer.near,
        [scan::Near {
            read: "JA02NJ".into(),
            real: "JA02WJ".into(),
            by: fr24::FR24_NAME,
        }]
    );
    let kw: Vec<String> = offer.keywords.iter().map(TagPath::to_lightroom).collect();
    assert_eq!(kw[0], "Aircraft|Registration|JA02WJ", "{kw:?}");
    assert!(
        kw.contains(&"Aircraft|Type|Airbus A350-1000".to_owned()),
        "{kw:?}"
    );

    // Each is asked once: nothing is due again.
    assert!(fr24::near_due(&w.cat).unwrap().is_empty());
    let n = client.asked.lock().unwrap().len();
    fr24::run_batch(
        &mut w.cat,
        &client,
        &AtomicBool::new(false),
        std::time::Duration::ZERO,
    )
    .unwrap();
    assert_eq!(client.asked.lock().unwrap().len(), n);
}

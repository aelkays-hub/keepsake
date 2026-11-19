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
            std::fs::write(file, jpeg(&photo(i as u8 * 40, 800, 600))).unwrap();
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
            number: None,
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
    assert_eq!(
        route::run_job(&mut w.cat, &sky, None, ids[0], Clock::AsRecorded)
            .unwrap()
            .requests,
        2
    );

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
    // Kept: not asked again; OpenSky's answers are apart.
    route::run_job(&mut w.cat, &fr, None, ids[0], Clock::AsRecorded).unwrap();
    assert_eq!(fr.asked.lock().unwrap().len(), 1);
    assert_eq!(
        route::review(
            &w.cat,
            &airports,
            ids[0],
            Clock::AsRecorded,
            Source::OpenSky
        )
        .unwrap()
        .state,
        route::State::NoTransponder
    );

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

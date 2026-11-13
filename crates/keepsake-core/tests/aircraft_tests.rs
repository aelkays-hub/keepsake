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
    let n = scan::neighbours(&w.cat, a).unwrap();
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
    // A photo with no capture time has none.
    assert!(scan::neighbours(&w.cat, d)
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
    svc.set_user_tags(b, &[reg.clone()]).unwrap();
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

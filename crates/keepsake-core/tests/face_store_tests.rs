//! Searching photos for faces and storing them (ADR-040, step 2), with a
//! stand-in for the face models: always run, also in CI.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use image::{DynamicImage, Rgb, RgbImage};
use keepsake_core::ai::faces::scan::{self, FaceFinder, FinderLoader, JOB_AI_FACES, MIN_FACE_PX};
use keepsake_core::ai::faces::{self, Face, EMBEDDING_LEN};
use keepsake_core::ai::{Accel, AiError, Embedding};
use keepsake_core::catalog::{
    AssetId, Catalog, CatalogError, FaceState, JobState, NewFace, VolumeKind,
};
use keepsake_core::library::Library;
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
        let root = tmp.path().to_path_buf();
        let lib = Library::create(&root.join("lib")).unwrap();
        let mut cat = lib.open_catalog().unwrap();
        let media = root.join("media");
        std::fs::create_dir_all(&media).unwrap();
        for (i, name) in names.iter().enumerate() {
            std::fs::write(media.join(name), jpeg(&photo(i as u8 * 40, 800, 600))).unwrap();
        }
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

    fn service(&self, loader: FinderLoader) -> RenderService {
        let tools = RenderTools {
            exiftool: PathBuf::from("none"),
            ffprobe: PathBuf::from("none"),
            ffmpeg: PathBuf::from("none"),
            dcraw: PathBuf::from("none"),
        };
        let opts = RenderOptions {
            workers: 1,
            proxy_workers: 0,
            max_bytes: u64::MAX,
            pause_on_battery: false,
            until_empty: true,
            forced_encoder: None,
            evict_every: 25,
            ai: false,
            ai_gpu: false,
            faces: true,
        };
        RenderService::new(&self.lib, tools, self.cache.clone(), opts).with_faces(loader)
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

/// Stands in for the face models: every picture has one big face and one
/// too small to keep; the sizes of the pictures it was given are kept.
struct FakeFinder {
    seen: Arc<Mutex<Vec<(u32, u32)>>>,
}

fn fake_face(x: f32, w: f32) -> Face {
    Face {
        x,
        y: 100.0,
        w,
        h: w * 1.2,
        landmarks: [[x + w / 2.0, 150.0]; 5],
        score: 0.97,
    }
}

impl FaceFinder for FakeFinder {
    fn find_and_describe(&mut self, img: &DynamicImage) -> Result<Vec<(Face, Embedding)>, AiError> {
        self.seen.lock().unwrap().push((img.width(), img.height()));
        let e = |v: f32| Embedding::new((0..EMBEDDING_LEN).map(|i| v + i as f32).collect());
        Ok(vec![
            (fake_face(200.0, 256.0), e(1.0)?),
            (fake_face(700.0, MIN_FACE_PX - 1.0), e(2.0)?),
        ])
    }

    fn accel(&self) -> Accel {
        Accel::Cpu
    }
}

fn fake(seen: &Arc<Mutex<Vec<(u32, u32)>>>) -> FinderLoader {
    let seen = Arc::clone(seen);
    Arc::new(move |_gpu| {
        Ok(Box::new(FakeFinder {
            seen: Arc::clone(&seen),
        }) as Box<dyn FaceFinder + Send>)
    })
}

#[test]
fn the_face_worker_searches_thumbnails_and_stores_the_faces() {
    let mut w = World::new(&["a.jpg", "b.jpg"]);
    let before = hashes(&w.media);
    let (a, b) = (w.asset("a.jpg"), w.asset("b.jpg"));
    w.thumb(a);
    // Only the face jobs: b's thumbnail is not queued, so b has to wait.
    assert_eq!(scan::enqueue(&mut w.cat, &[a, b], false).unwrap(), 2);
    let st = scan::status(&w.cat).unwrap();
    assert_eq!((st.photos, st.scanned, st.pending), (2, 0, 2));

    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut events = Vec::new();
    w.service(fake(&seen))
        .run(&AtomicBool::new(false), &mut |e| events.push(e.clone()))
        .unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, RenderEvent::FacesReady { accel: Accel::Cpu })));
    // Only a had a thumbnail; it was searched at the thumbnail's size.
    assert_eq!(*seen.lock().unwrap(), [(1024, 768)]);
    let found = w.cat.read(|tx| tx.faces().for_asset(a)).unwrap();
    assert_eq!(found.len(), 1, "the tiny face is not kept");
    let f = &found[0];
    assert!((f.face.x - 200.0 / 1024.0).abs() < 1e-5);
    assert!((f.face.w - 0.25).abs() < 1e-5);
    assert_eq!(f.face.embedding.len(), EMBEDDING_LEN);
    assert_eq!((f.person, f.state), (None, FaceState::Unknown));
    let s = w.cat.read(|tx| tx.faces().scan(a)).unwrap().unwrap();
    assert_eq!(
        (s.model.as_str(), s.width, s.height),
        (faces::MODEL_ID, 1024, 768)
    );
    // b waits for its thumbnail (queued by the job).
    let job = w
        .cat
        .read(|tx| tx.jobs().find(JOB_AI_FACES, b))
        .unwrap()
        .unwrap();
    assert_eq!(job.state, JobState::Queued);
    let st = scan::status(&w.cat).unwrap();
    assert_eq!(
        (st.scanned, st.with_faces, st.faces, st.pending),
        (1, 1, 1, 1)
    );
    // Asked again, a photo already searched is not searched again.
    scan::enqueue(&mut w.cat, &[a], true).unwrap();
    seen.lock().unwrap().clear();
    w.service(fake(&seen))
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert!(seen.lock().unwrap().is_empty());
    assert_eq!(hashes(&w.media), before, "originals unchanged");
}

fn new_face(x: f32) -> NewFace {
    NewFace {
        x,
        y: 0.1,
        w: 0.2,
        h: 0.3,
        landmarks: [[x + 0.1, 0.2]; 5],
        score: 0.9,
        embedding: vec![1; EMBEDDING_LEN],
        emb_scale: 0.01,
    }
}

#[test]
fn searching_again_keeps_the_faces_the_user_named() {
    let w = World::new(&["a.jpg"]);
    let mut cat = w.cat;
    let a = {
        let path = std::fs::canonicalize(w.media.join("a.jpg")).unwrap();
        w.lib.locate(&cat, &path).unwrap().unwrap().asset_id
    };
    cat.write(|tx| {
        tx.faces()
            .save_scan(a, "old", 1024, 768, &[new_face(0.1), new_face(0.6)], 1)
    })
    .unwrap();
    let found = cat.read(|tx| tx.faces().for_asset(a)).unwrap();
    let named = found.iter().find(|f| f.face.x < 0.5).unwrap().id;
    let sara = cat
        .write(|tx| {
            let p = tx.faces().add_person("  Sara ", 2)?;
            tx.faces()
                .set_person(named, Some((p, FaceState::Confirmed)))?;
            Ok::<_, CatalogError>(p)
        })
        .unwrap();
    // The same person whatever the case; an empty name is refused.
    assert_eq!(
        cat.write(|tx| tx.faces().add_person("sara", 3)).unwrap(),
        sara
    );
    assert!(cat.write(|tx| tx.faces().add_person(" ", 3)).is_err());
    // A new search finds the named face again (a little moved) and another.
    let added = cat
        .write(|tx| {
            tx.faces()
                .save_scan(a, "new", 1024, 768, &[new_face(0.11), new_face(0.6)], 4)
        })
        .unwrap();
    assert_eq!(added, 1, "the named face is not added twice");
    let found = cat.read(|tx| tx.faces().for_asset(a)).unwrap();
    assert_eq!(found.len(), 2);
    let kept = found.iter().find(|f| f.id == named).unwrap();
    assert_eq!(
        (kept.person, kept.state),
        (Some(sara), FaceState::Confirmed)
    );
    assert_eq!(
        cat.read(|tx| tx.faces().scan(a)).unwrap().unwrap().model,
        "new"
    );
    // "Not this person": no longer hers, and remembered.
    cat.write(|tx| tx.faces().reject(named, sara)).unwrap();
    let f = cat.read(|tx| tx.faces().get(named)).unwrap().unwrap();
    assert_eq!((f.person, f.state), (None, FaceState::Unknown));
    assert_eq!(
        cat.read(|tx| tx.faces().rejected_for(named)).unwrap(),
        [sara]
    );
    // A face with a person is always suggested or confirmed.
    assert!(cat
        .write(|tx| tx
            .faces()
            .set_person(named, Some((sara, FaceState::Unknown))))
        .is_err());
    let people = cat.read(|tx| tx.faces().people()).unwrap();
    assert_eq!(people.len(), 1);
    assert_eq!(people[0].name, "Sara");
}

#[test]
fn delete_all_face_data_forgets_everything_and_stops_the_searches() {
    let mut w = World::new(&["a.jpg", "b.jpg"]);
    let (a, b) = (w.asset("a.jpg"), w.asset("b.jpg"));
    w.cat
        .write(|tx| {
            tx.faces()
                .save_scan(a, faces::MODEL_ID, 1024, 768, &[new_face(0.1)], 1)?;
            tx.faces().add_person("Sara", 1)
        })
        .unwrap();
    scan::enqueue(&mut w.cat, &[b], false).unwrap();
    assert_eq!(scan::delete_all(&mut w.cat).unwrap(), 1);
    let st = scan::status(&w.cat).unwrap();
    assert_eq!((st.scanned, st.faces, st.people, st.pending), (0, 0, 0, 0));
    assert!(w.cat.read(|tx| tx.faces().scan(a)).unwrap().is_none());
    // Every photo is to be searched again.
    let todo = w
        .cat
        .read(|tx| tx.faces().unscanned_photos(faces::MODEL_ID))
        .unwrap();
    assert_eq!(todo.len(), 2);
}

#[test]
fn searching_all_photos_queues_their_thumbnails_too() {
    let mut w = World::new(&["a.jpg", "b.jpg"]);
    let (a, b) = (w.asset("a.jpg"), w.asset("b.jpg"));
    w.cat
        .write(|tx| {
            tx.faces()
                .save_scan(a, faces::MODEL_ID, 1024, 768, &[], 1)
                .map(|_| ())
        })
        .unwrap();
    // a was searched already (and had no faces): only b is queued.
    assert_eq!(scan::enqueue_unscanned(&mut w.cat).unwrap(), 1);
    let job = |kind: &str, id| w.cat.read(|tx| tx.jobs().find(kind, id)).unwrap();
    assert!(job(JOB_AI_FACES, a).is_none());
    assert!(job(JOB_AI_FACES, b).is_some());
    assert!(job(keepsake_core::render::JOB_STILLS, b).is_some());
    let st = scan::status(&w.cat).unwrap();
    assert_eq!(
        (st.photos, st.scanned, st.with_faces, st.pending),
        (2, 1, 0, 1)
    );
}

#[test]
fn without_the_models_the_face_worker_says_why_and_the_rest_goes_on() {
    let mut w = World::new(&["a.jpg"]);
    let a = w.asset("a.jpg");
    w.thumb(a);
    scan::enqueue(&mut w.cat, &[a], false).unwrap();
    let missing: FinderLoader =
        Arc::new(|_| Err(AiError::ModelMissing(PathBuf::from("models/yunet"))));
    let mut events = Vec::new();
    w.service(missing)
        .run(&AtomicBool::new(false), &mut |e| events.push(e.clone()))
        .unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        RenderEvent::FacesUnavailable { reason } if reason.contains("not installed")
    )));
    assert!(matches!(events.last(), Some(RenderEvent::Finished { .. })));
    // The photo is still to be searched.
    assert_eq!(scan::status(&w.cat).unwrap().pending, 1);
}

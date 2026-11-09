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
    assert_eq!(scan::delete_all(&w.lib, &mut w.cat).unwrap(), 1);
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

// ── Name and find (step 3) ─────────────────────────────────────────────────

use keepsake_core::ai::faces::people;
use keepsake_core::catalog::CaptureTime;

const YEAR: i64 = 365 * 24 * 3600 * 1000;
const Y2020: i64 = 1_577_836_800_000;

/// A 128-number int8 embedding: `plus` of the numbers +100, the rest -100.
/// Two of these have a cosine of (2·shared − 128) / 128.
fn emb(plus: usize) -> Vec<i8> {
    (0..EMBEDDING_LEN)
        .map(|i| if i < plus { 100 } else { -100 })
        .collect()
}

fn face_with(x: f32, embedding: Vec<i8>) -> NewFace {
    NewFace {
        embedding,
        ..new_face(x)
    }
}

/// Store one search of `name`'s photo, taken at `at`, with these faces.
fn photo_with(w: &mut World, name: &str, at: Option<i64>, faces_in: &[NewFace]) -> AssetId {
    let id = w.asset(name);
    w.cat
        .write(|tx| {
            if let Some(ms) = at {
                let mut a = tx.assets().require(id)?;
                a.capture = Some(CaptureTime {
                    utc_ms: ms,
                    utc_offset_min: Some(0),
                });
                tx.assets().update(&a)?;
            }
            tx.faces()
                .save_scan(id, faces::MODEL_ID, 1024, 768, faces_in, 1)
                .map(|_| ())
        })
        .unwrap();
    id
}

fn face_of(w: &World, photo: AssetId, n: usize) -> i64 {
    let mut f = w.cat.read(|tx| tx.faces().for_asset(photo)).unwrap();
    f.sort_by(|a, b| a.face.x.total_cmp(&b.face.x));
    f[n].id
}

fn suggested(w: &World, person: i64) -> Vec<AssetId> {
    let mut v: Vec<AssetId> = people::review(&w.cat, person)
        .unwrap()
        .iter()
        .map(|f| f.asset_id)
        .collect();
    v.sort();
    v
}

#[test]
fn name_and_find_suggests_the_faces_most_like_the_named_one() {
    let mut w = World::new(&["a.jpg", "b.jpg", "c.jpg", "d.jpg", "e.jpg"]);
    let t = Some(Y2020);
    // a: Sara; b: almost the same face; c: someone else (cosine 0) and
    // someone only a little like her (0.1, under the threshold, and still
    // under it next to the faces confirmed later); d: two
    // faces like her (only the better one); e: like her, rejected later.
    let a = photo_with(
        &mut w,
        "a.jpg",
        t,
        &[face_with(0.1, emb(128)), face_with(0.6, emb(64))],
    );
    let b = photo_with(&mut w, "b.jpg", t, &[face_with(0.1, emb(124))]);
    let c = photo_with(
        &mut w,
        "c.jpg",
        t,
        &[face_with(0.1, emb(64)), face_with(0.6, emb(70))],
    );
    let d = photo_with(
        &mut w,
        "d.jpg",
        t,
        &[face_with(0.1, emb(110)), face_with(0.6, emb(120))],
    );
    let e = photo_with(&mut w, "e.jpg", t, &[face_with(0.1, emb(120))]);
    let fa = face_of(&w, a, 0);
    let sara = people::name_face(&w.lib, &mut w.cat, fa, "Sara", 1).unwrap();
    // Nothing is suggested before a face is named for that person.
    let nobody = w.cat.write(|tx| tx.faces().add_person("Ali", 1)).unwrap();
    assert!(people::suggest(&mut w.cat, nobody, people::DEFAULT_MATCH).is_err());

    let r = people::suggest(&mut w.cat, sara, people::DEFAULT_MATCH).unwrap();
    assert_eq!((r.added, r.waiting, r.confirmed), (3, 3, 1));
    // The years around the confirmed photo (2020), by year.
    assert_eq!((r.from_year, r.to_year), (Some(2017), Some(2023)));
    let mut want = vec![b, d, e];
    want.sort();
    assert_eq!(suggested(&w, sara), want, "not c; not the other face in a");
    // d's better face, best first overall.
    let rev = people::review(&w.cat, sara).unwrap();
    assert_eq!(rev[0].asset_id, b);
    assert!(rev[0].likeness.unwrap() > rev[1].likeness.unwrap());
    let dd = rev.iter().find(|f| f.asset_id == d).unwrap();
    assert_eq!(dd.id, face_of(&w, d, 1), "the better of d's two faces");
    // Suggesting again adds nothing new.
    assert_eq!(
        people::suggest(&mut w.cat, sara, people::DEFAULT_MATCH)
            .unwrap()
            .added,
        0
    );

    // The answers: b and d are Sara; e is not.
    let fe = face_of(&w, e, 0);
    let (fb, fd) = (face_of(&w, b, 0), dd.id);
    let (yes, no) = people::decide(&w.lib, &mut w.cat, sara, &[fb, fd], &[fe]).unwrap();
    assert_eq!((yes, no), (2, 1));
    assert!(suggested(&w, sara).is_empty());
    let f = w.cat.read(|tx| tx.faces().get(fe)).unwrap().unwrap();
    assert_eq!((f.person, f.state), (None, FaceState::Unknown));
    // A rejected face is not suggested for her again.
    assert_eq!(
        people::suggest(&mut w.cat, sara, people::DEFAULT_MATCH)
            .unwrap()
            .added,
        0
    );
    // Faces not suggested for her are left alone by her answers.
    let other = face_of(&w, c, 0);
    assert_eq!(
        people::decide(&w.lib, &mut w.cat, sara, &[other], &[]).unwrap(),
        (0, 0)
    );
    // Naming a rejected face her after all forgets the rejection.
    assert_eq!(
        people::name_face(&w.lib, &mut w.cat, fe, "sara", 2).unwrap(),
        sara
    );
    assert!(w
        .cat
        .read(|tx| tx.faces().rejected_for(fe))
        .unwrap()
        .is_empty());
    assert_eq!(
        w.cat
            .read(|tx| tx.faces().of_person(sara, FaceState::Confirmed))
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn find_more_steps_back_through_the_years() {
    let mut w = World::new(&[
        "y2020.jpg",
        "y2018.jpg",
        "y2016.jpg",
        "y2014.jpg",
        "none.jpg",
    ]);
    let at = |years: i64| Some(Y2020 - years * YEAR);
    let p2020 = photo_with(&mut w, "y2020.jpg", at(0), &[face_with(0.1, emb(128))]);
    let p2018 = photo_with(&mut w, "y2018.jpg", at(2), &[face_with(0.1, emb(120))]);
    let p2016 = photo_with(&mut w, "y2016.jpg", at(4), &[face_with(0.1, emb(118))]);
    let p2014 = photo_with(&mut w, "y2014.jpg", at(6), &[face_with(0.1, emb(116))]);
    // A photo with no date is always looked at.
    let undated = photo_with(&mut w, "none.jpg", None, &[face_with(0.1, emb(126))]);
    let f2020 = face_of(&w, p2020, 0);
    let sara = people::name_face(&w.lib, &mut w.cat, f2020, "Sara", 1).unwrap();

    let mut reached = Vec::new();
    for want in [vec![p2018, undated], vec![p2016], vec![p2014]] {
        people::suggest(&mut w.cat, sara, people::DEFAULT_MATCH).unwrap();
        let mut want = want;
        want.sort();
        let got = suggested(&w, sara);
        assert_eq!(got, want, "after {reached:?}");
        let ids: Vec<i64> = people::review(&w.cat, sara)
            .unwrap()
            .iter()
            .map(|f| f.id)
            .collect();
        people::decide(&w.lib, &mut w.cat, sara, &ids, &[]).unwrap();
        reached.extend(got);
    }
    assert_eq!(
        people::suggest(&mut w.cat, sara, people::DEFAULT_MATCH)
            .unwrap()
            .added,
        0
    );
}

// ── Names in the sidecars and search (step 4) ──────────────────────────────

use keepsake_core::catalog::TagPath;
use keepsake_core::metadata::MetadataService;
use keepsake_core::rebuild::rebuild_catalog;

fn keywords_of(w: &mut World, id: AssetId) -> Vec<String> {
    let mut k: Vec<String> = MetadataService::new(&w.lib, &mut w.cat)
        .user_tags(id)
        .unwrap()
        .iter()
        .map(TagPath::to_lightroom)
        .collect();
    k.sort();
    k
}

/// Throw the catalog away and rebuild it from the files and sidecars.
fn rebuild(w: &mut World) {
    let vol = w.cat.read(|tx| tx.volumes().list()).unwrap()[0].id;
    drop(std::mem::replace(
        &mut w.cat,
        Catalog::open_in_memory().unwrap(),
    ));
    Catalog::delete_files(&w.lib.catalog_path()).unwrap();
    w.cat = w.lib.open_catalog().unwrap();
    rebuild_catalog(&mut w.cat, &w.lib, vol, &w.media).unwrap();
}

#[test]
fn names_go_into_keywords_and_sidecars_and_a_rebuild_keeps_them() {
    let mut w = World::new(&["a.jpg", "b.jpg"]);
    let before = hashes(&w.media);
    let a = photo_with(
        &mut w,
        "a.jpg",
        Some(Y2020),
        &[face_with(0.1, emb(128)), face_with(0.6, emb(64))],
    );
    let b = photo_with(&mut w, "b.jpg", Some(Y2020), &[face_with(0.1, emb(126))]);
    MetadataService::new(&w.lib, &mut w.cat)
        .set_user_tags(a, &[TagPath::parse_lightroom("Holiday").unwrap()])
        .unwrap();
    let fa = face_of(&w, a, 0);
    let sara = people::name_face(&w.lib, &mut w.cat, fa, "Sara", 1).unwrap();
    assert_eq!(keywords_of(&mut w, a), ["Holiday", "People|Sara"]);
    // b's face: suggested, then confirmed; a's other face: not Ali.
    people::suggest(&mut w.cat, sara, people::DEFAULT_MATCH).unwrap();
    let fb = face_of(&w, b, 0);
    people::decide(&w.lib, &mut w.cat, sara, &[fb], &[]).unwrap();
    assert_eq!(keywords_of(&mut w, b), ["People|Sara"]);
    let other = face_of(&w, a, 1);
    let ali = people::name_face(&w.lib, &mut w.cat, other, "Ali", 2).unwrap();
    w.cat
        .write(|tx| {
            tx.faces()
                .set_person(other, Some((ali, FaceState::Suggested)))
        })
        .unwrap();
    people::decide(&w.lib, &mut w.cat, ali, &[], &[other]).unwrap();
    assert_eq!(
        keywords_of(&mut w, a),
        ["Holiday", "People|Sara"],
        "Ali's keyword went"
    );

    // Rebuilt from the sidecars: Sara with her two faces, and "not Ali".
    rebuild(&mut w);
    let people_now: Vec<String> = w
        .cat
        .read(|tx| tx.faces().people())
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(people_now, ["Ali", "Sara"]);
    let sara = w
        .cat
        .read(|tx| tx.faces().person_named("sara"))
        .unwrap()
        .unwrap()
        .id;
    let ali = w
        .cat
        .read(|tx| tx.faces().person_named("Ali"))
        .unwrap()
        .unwrap()
        .id;
    let hers = w
        .cat
        .read(|tx| tx.faces().of_person(sara, FaceState::Confirmed))
        .unwrap();
    assert_eq!(hers.len(), 2);
    assert!(hers
        .iter()
        .all(|f| f.model == keepsake_core::catalog::RESTORED_MODEL));
    assert!((hers.iter().find(|f| f.asset_id == a).unwrap().face.x - 0.1).abs() < 1e-5);
    assert_eq!(keywords_of(&mut w, a), ["Holiday", "People|Sara"]);

    // The next search of a finds both faces again: Sara's keeps her name
    // and gets its description; the other is "not Ali" again.
    w.cat
        .write(|tx| {
            tx.faces()
                .save_scan(
                    a,
                    faces::MODEL_ID,
                    1024,
                    768,
                    &[face_with(0.1, emb(128)), face_with(0.6, emb(64))],
                    5,
                )
                .map(|_| ())
        })
        .unwrap();
    let found = w.cat.read(|tx| tx.faces().for_asset(a)).unwrap();
    assert_eq!(found.len(), 2);
    let hers = found.iter().find(|f| f.face.x < 0.5).unwrap();
    assert_eq!(
        (hers.person, hers.state),
        (Some(sara), FaceState::Confirmed)
    );
    assert_eq!(hers.model, faces::MODEL_ID);
    assert_eq!(hers.face.embedding, emb(128));
    let not_ali = found.iter().find(|f| f.face.x > 0.5).unwrap();
    assert_eq!(
        w.cat
            .read(|tx| tx.faces().rejected_for(not_ali.id))
            .unwrap(),
        [ali]
    );

    // Taking Sara's only face off b takes her keyword off b.
    let fb = face_of(&w, b, 0);
    w.cat.write(|tx| tx.faces().set_person(fb, None)).unwrap();
    MetadataService::new(&w.lib, &mut w.cat)
        .sync_faces(b)
        .unwrap();
    assert!(keywords_of(&mut w, b).is_empty());
    assert_eq!(hashes(&w.media), before, "originals unchanged");
}

#[test]
fn delete_all_face_data_takes_the_names_off_the_photos_too() {
    let mut w = World::new(&["a.jpg"]);
    let a = photo_with(&mut w, "a.jpg", Some(Y2020), &[face_with(0.1, emb(128))]);
    MetadataService::new(&w.lib, &mut w.cat)
        .set_user_tags(a, &[TagPath::parse_lightroom("People|Bob").unwrap()])
        .unwrap();
    let fa = face_of(&w, a, 0);
    people::name_face(&w.lib, &mut w.cat, fa, "Sara", 1).unwrap();
    assert_eq!(keywords_of(&mut w, a), ["People|Bob", "People|Sara"]);
    scan::delete_all(&w.lib, &mut w.cat).unwrap();
    // Bob is the user's own keyword (not a person here): it stays.
    assert_eq!(keywords_of(&mut w, a), ["People|Bob"]);
    // Nothing comes back from the sidecar.
    rebuild(&mut w);
    assert!(w.cat.read(|tx| tx.faces().people()).unwrap().is_empty());
    assert_eq!(keywords_of(&mut w, a), ["People|Bob"]);
}

// ── The viewer's face menu (step 5) ────────────────────────────────────────

#[test]
fn not_this_person_and_not_a_face() {
    let mut w = World::new(&["a.jpg"]);
    let a = photo_with(
        &mut w,
        "a.jpg",
        Some(Y2020),
        &[face_with(0.1, emb(128)), face_with(0.6, emb(64))],
    );
    let (f0, f1) = (face_of(&w, a, 0), face_of(&w, a, 1));
    let sara = people::name_face(&w.lib, &mut w.cat, f0, "Sara", 1).unwrap();
    assert_eq!(keywords_of(&mut w, a), ["People|Sara"]);
    // Not Sara: her name and keyword come off, and it is remembered.
    assert!(people::not_this_person(&w.lib, &mut w.cat, f0).unwrap());
    assert!(keywords_of(&mut w, a).is_empty());
    assert_eq!(
        w.cat.read(|tx| tx.faces().rejected_for(f0)).unwrap(),
        [sara]
    );
    assert!(
        !people::not_this_person(&w.lib, &mut w.cat, f0).unwrap(),
        "no one's now"
    );
    // Not a face: hidden, not counted, and a new search does not bring it
    // back.
    people::not_a_face(&w.lib, &mut w.cat, f1).unwrap();
    let f = w.cat.read(|tx| tx.faces().get(f1)).unwrap().unwrap();
    assert_eq!((f.person, f.state), (None, FaceState::Hidden));
    assert_eq!(scan::status(&w.cat).unwrap().faces, 1);
    w.cat
        .write(|tx| {
            tx.faces()
                .save_scan(
                    a,
                    faces::MODEL_ID,
                    1024,
                    768,
                    &[face_with(0.1, emb(128)), face_with(0.6, emb(64))],
                    9,
                )
                .map(|_| ())
        })
        .unwrap();
    let found = w.cat.read(|tx| tx.faces().for_asset(a)).unwrap();
    assert_eq!(found.len(), 2, "the hidden face is not added again");
    assert!(found
        .iter()
        .any(|f| f.id == f1 && f.state == FaceState::Hidden));
    // A hidden face is never suggested.
    let ok = people::name_face(&w.lib, &mut w.cat, f0, "Sara", 2).unwrap();
    people::suggest(&mut w.cat, ok, people::DEFAULT_MATCH).unwrap();
    assert!(people::review(&w.cat, ok).unwrap().is_empty());
}

// ── Better matching (ADR-040, tuned on the product owner's library) ────────

/// An embedding unlike `emb(n)`: alternate signs (cosine 0 with all +).
fn stranger(shift: usize) -> Vec<i8> {
    (0..EMBEDDING_LEN)
        .map(|i| {
            if (i + shift).is_multiple_of(2) {
                100
            } else {
                -100
            }
        })
        .collect()
}

fn photos(w: &mut World, n: usize) -> Vec<AssetId> {
    (0..n)
        .map(|i| {
            let name = format!("p{i}.jpg");
            photo_with(w, &name, Some(Y2020), &[face_with(0.1, emb(128 - i % 3))])
        })
        .collect()
}

fn world(n: usize) -> World {
    let names: Vec<String> = (0..n).map(|i| format!("p{i}.jpg")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    World::new(&refs)
}

#[test]
fn one_wrong_confirmation_does_not_pull_in_that_stranger() {
    // Five photos of Sara and one of a stranger, wrongly confirmed as her;
    // then another photo of the stranger.
    let mut w = world(7);
    let sara_photos = photos(&mut w, 5);
    let wrong = w.asset("p5.jpg");
    let again = w.asset("p6.jpg");
    for (id, e) in [(wrong, stranger(0)), (again, stranger(0))] {
        w.cat
            .write(|tx| {
                tx.faces()
                    .save_scan(
                        id,
                        faces::MODEL_ID,
                        1024,
                        768,
                        &[face_with(0.1, e.clone())],
                        1,
                    )
                    .map(|_| ())
            })
            .unwrap();
    }
    let f0 = face_of(&w, sara_photos[0], 0);
    let sara = people::name_face(&w.lib, &mut w.cat, f0, "Sara", 1).unwrap();
    let mut confirm = Vec::new();
    for p in &sara_photos[1..] {
        confirm.push(face_of(&w, *p, 0));
    }
    confirm.push(face_of(&w, wrong, 0));
    for f in &confirm {
        w.cat
            .write(|tx| {
                tx.faces()
                    .set_person(*f, Some((sara, FaceState::Confirmed)))
            })
            .unwrap();
    }
    // The nearest-face matching suggested the stranger's other photo (it is
    // the same face as a confirmed one); the average face does not.
    people::suggest(&mut w.cat, sara, people::DEFAULT_MATCH).unwrap();
    assert!(suggested(&w, sara).is_empty(), "{:?}", suggested(&w, sara));
    // And Check puts the wrong one first.
    let checked = people::check(&w.cat, sara).unwrap();
    assert_eq!(checked.len(), 6);
    assert_eq!(checked[0].0.asset_id, wrong);
    assert!(
        checked[0].1 < 0.3 && checked[5].1 > 0.9,
        "{:?}",
        checked.iter().map(|c| c.1).collect::<Vec<_>>()
    );
}

#[test]
fn a_face_like_one_rejected_for_her_is_not_suggested() {
    let mut w = world(3);
    let a = photo_with(&mut w, "p0.jpg", Some(Y2020), &[face_with(0.1, emb(128))]);
    // 104 of 128 alike: cosine 0.625 with Sara, over the cut-off.
    let b = photo_with(&mut w, "p1.jpg", Some(Y2020), &[face_with(0.1, emb(104))]);
    let fa = face_of(&w, a, 0);
    let sara = people::name_face(&w.lib, &mut w.cat, fa, "Sara", 1).unwrap();
    people::suggest(&mut w.cat, sara, people::DEFAULT_MATCH).unwrap();
    assert_eq!(suggested(&w, sara), [b]);
    let fb = face_of(&w, b, 0);
    people::decide(&w.lib, &mut w.cat, sara, &[], &[fb]).unwrap();
    // The same face in another photo: more like the rejected one than her.
    let c = photo_with(&mut w, "p2.jpg", Some(Y2020), &[face_with(0.1, emb(104))]);
    people::suggest(&mut w.cat, sara, people::DEFAULT_MATCH).unwrap();
    assert!(!suggested(&w, sara).contains(&c));
}

#[test]
fn small_faces_are_not_matched_and_the_cut_off_can_be_stricter() {
    let mut w = world(3);
    let a = photo_with(&mut w, "p0.jpg", Some(Y2020), &[face_with(0.1, emb(128))]);
    // 0.04 of 1024 px: 41 px wide, too small.
    let small = NewFace {
        w: 0.04,
        h: 0.05,
        ..face_with(0.5, emb(127))
    };
    let tiny = photo_with(&mut w, "p1.jpg", Some(Y2020), &[small]);
    let fair = photo_with(&mut w, "p2.jpg", Some(Y2020), &[face_with(0.1, emb(104))]);
    let fa = face_of(&w, a, 0);
    let sara = people::name_face(&w.lib, &mut w.cat, fa, "Sara", 1).unwrap();
    let r = people::suggest(&mut w.cat, sara, people::DEFAULT_MATCH).unwrap();
    assert_eq!(suggested(&w, sara), [fair], "not the tiny face");
    assert_eq!((r.from_year, r.to_year), (Some(2017), Some(2023)));
    // Stricter: the 0.625 suggestion is taken off; looser again: back.
    assert_eq!(people::rescore(&mut w.cat, sara, 0.7).unwrap(), 1);
    assert!(suggested(&w, sara).is_empty());
    let ff = face_of(&w, fair, 0);
    let f = w.cat.read(|tx| tx.faces().get(ff)).unwrap().unwrap();
    assert_eq!(
        (f.person, f.state, f.likeness),
        (None, FaceState::Unknown, None)
    );
    people::suggest(&mut w.cat, sara, 0.6).unwrap();
    assert_eq!(suggested(&w, sara), [fair]);
    assert!(!suggested(&w, sara).contains(&tiny));
    // rescore_all scores everyone's again.
    assert_eq!(people::rescore_all(&mut w.cat, 0.75).unwrap(), 1);
}

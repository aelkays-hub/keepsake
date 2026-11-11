//! Edit suggestions (ADR-042) through the catalog and the render service,
//! with a stand-in for the AI: always run, also in CI. Nothing is sent
//! anywhere, and the originals stay as they were.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use image::{DynamicImage, Rgb, RgbImage};
use keepsake_core::ai::cloud::Provider;
use keepsake_core::ai::suggest::{
    self, Adviser, AiAdvice, Measures, Suggestion, JOB_SUGGEST_CLAUDE, JOB_SUGGEST_GEMINI,
};
use keepsake_core::ai::AiError;
use keepsake_core::catalog::{AssetId, Catalog, JobState, VolumeKind};
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

/// A dark photo with a blown-out patch (the sun) in its corner.
fn dark_photo(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
        if x < w / 8 && y < h / 8 {
            Rgb([255, 255, 255])
        } else {
            let v = 20 + ((x / 9 + y / 7) % 30) as u8;
            Rgb([v, v, v + 4])
        }
    }))
}

impl World {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let media = tmp.path().join("media");
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(media.join("a.jpg"), jpeg(&dark_photo(800, 600))).unwrap();
        let lib = Library::create(&tmp.path().join("lib")).unwrap();
        let mut cat = lib.open_catalog().unwrap();
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
        let cache = Cache::new(tmp.path().join("cache"));
        Self {
            _tmp: tmp,
            lib,
            cat,
            media,
            cache,
        }
    }

    fn asset(&self) -> AssetId {
        let path = std::fs::canonicalize(self.media.join("a.jpg")).unwrap();
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
                &jpeg(&dark_photo(1024, 768)),
            )
            .unwrap();
    }

    fn run(&self, adviser: Arc<dyn Adviser>) -> Vec<RenderEvent> {
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
            faces: false,
            aircraft: false,
        };
        let mut events = Vec::new();
        RenderService::new(&self.lib, tools, self.cache.clone(), opts)
            .with_adviser(adviser)
            .run(&AtomicBool::new(false), &mut |e| events.push(e.clone()))
            .unwrap();
        events
    }

    fn job_state(&self, kind: &str, id: AssetId) -> Option<JobState> {
        self.cat
            .read(|tx| tx.jobs().find(kind, id))
            .unwrap()
            .map(|j| j.state)
    }
}

/// Stands in for the AI: records what it was sent, answers with one
/// suggestion, or fails.
#[derive(Default)]
struct Fake {
    seen: Mutex<Vec<(Provider, (u32, u32), f32)>>,
    fail: Option<(String, bool)>,
}

impl Adviser for Fake {
    fn advise(&self, provider: Provider, jpeg: &[u8], m: &Measures) -> Result<AiAdvice, AiError> {
        let img = image::load_from_memory(jpeg).unwrap();
        self.seen.lock().unwrap().push((
            provider,
            (img.width(), img.height()),
            m.highlights,
        ));
        if let Some((message, retry)) = &self.fail {
            return Err(AiError::Cloud {
                message: message.clone(),
                retry: *retry,
            });
        }
        Ok(AiAdvice {
            provider: provider.id().to_owned(),
            model: "stand-in".to_owned(),
            summary: "A moody street; the sun is blown.".to_owned(),
            suggestions: vec![Suggestion {
                title: "Tame the sun".to_owned(),
                why: "The top left is white.".to_owned(),
                photoshop: "Camera Raw > Light: Highlights -60".to_owned(),
                topaz: String::new(),
            }],
            made_ms: 1,
        })
    }
}

#[test]
fn measuring_needs_the_thumbnail_and_is_kept() {
    let mut w = World::new();
    let id = w.asset();
    let original = std::fs::read(w.media.join("a.jpg")).unwrap();
    assert!(matches!(
        suggest::advice(&mut w.cat, &w.cache, id, false),
        Err(AiError::NoThumbnail)
    ));
    assert!(suggest::stored(&w.cat, id).unwrap().is_none());
    w.thumb(id);
    let a = suggest::advice(&mut w.cat, &w.cache, id, false).unwrap();
    assert_eq!((a.measures.width, a.measures.height), (1024, 768));
    assert!(a.measures.highlights > 0.01, "{:?}", a.measures);
    let titles: Vec<String> = a.suggestions().into_iter().map(|s| s.title).collect();
    assert!(
        titles.contains(&"Recover the highlights".to_owned()),
        "{titles:?}"
    );
    assert_eq!(a.measured()[0].label, "Brightness");
    // Kept: the same next time, without measuring again.
    let again = suggest::stored(&w.cat, id).unwrap().unwrap();
    assert_eq!(again.measures, a.measures);
    assert_eq!(again.made_ms, a.made_ms);
    assert_eq!(std::fs::read(w.media.join("a.jpg")).unwrap(), original);
}

#[test]
fn asking_an_ai_runs_as_a_job_and_keeps_the_answer() {
    let mut w = World::new();
    let id = w.asset();
    w.thumb(id);
    let original = std::fs::read(w.media.join("a.jpg")).unwrap();
    // Asking Gemini, then Claude: one question at a time.
    suggest::enqueue_ask(&mut w.cat, Provider::Gemini, id).unwrap();
    suggest::enqueue_ask(&mut w.cat, Provider::Claude, id).unwrap();
    assert_eq!(w.job_state(JOB_SUGGEST_GEMINI, id), None);
    let pending = suggest::asking(&w.cat, id, None).unwrap().unwrap();
    assert_eq!(pending.provider, Provider::Claude);
    assert!(pending.pending);

    let fake = Arc::new(Fake::default());
    let events = w.run(fake.clone());
    let seen = fake.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, Provider::Claude);
    assert_eq!(seen[0].1, (1024, 768), "the thumbnail, not the original");
    assert!(seen[0].2 > 0.01, "with the measurements");
    assert_eq!(w.job_state(JOB_SUGGEST_CLAUDE, id), Some(JobState::Done));
    assert!(events.iter().any(|e| matches!(
        e,
        RenderEvent::Done { kind, made, .. } if kind == JOB_SUGGEST_CLAUDE && made == &["Tame the sun"]
    )));

    let kept = suggest::stored(&w.cat, id).unwrap().unwrap();
    let ai = kept.ai.clone().unwrap();
    assert_eq!(ai.provider, "claude");
    assert_eq!(ai.suggestions[0].title, "Tame the sun");
    assert_eq!(suggest::asking(&w.cat, id, Some(ai.made_ms)).unwrap(), None);
    // The measurements were made on the way and kept too.
    assert_eq!(kept.measures.width, 1024);

    // Measuring again drops the answer to the old measurements.
    let fresh = suggest::advice(&mut w.cat, &w.cache, id, true).unwrap();
    assert_eq!(fresh.ai, None);
    assert_eq!(suggest::stored(&w.cat, id).unwrap().unwrap().ai, None);
    assert_eq!(std::fs::read(w.media.join("a.jpg")).unwrap(), original);
}

#[test]
fn a_refused_question_fails_and_says_why_and_stop_drops_it() {
    let mut w = World::new();
    let id = w.asset();
    w.thumb(id);
    suggest::enqueue_ask(&mut w.cat, Provider::OpenAi, id).unwrap();
    let fake = Arc::new(Fake {
        fail: Some(("ChatGPT declined to look at this photo".to_owned(), false)),
        ..Fake::default()
    });
    w.run(fake);
    let failed = suggest::asking(&w.cat, id, None).unwrap().unwrap();
    assert_eq!(failed.provider, Provider::OpenAi);
    assert!(!failed.pending);
    assert_eq!(
        failed.problem.as_deref(),
        Some("ChatGPT declined to look at this photo")
    );
    // An answer given after the failure hides it.
    assert_eq!(
        suggest::asking(&w.cat, id, Some(i64::MAX)).unwrap(),
        None
    );
    // Stop: a question not sent yet is not sent.
    suggest::enqueue_ask(&mut w.cat, Provider::Claude, id).unwrap();
    suggest::stop_asking(&mut w.cat, id).unwrap();
    assert_eq!(w.job_state(JOB_SUGGEST_CLAUDE, id), None);
    let fake = Arc::new(Fake::default());
    w.run(fake.clone());
    assert!(fake.seen.lock().unwrap().is_empty());
}

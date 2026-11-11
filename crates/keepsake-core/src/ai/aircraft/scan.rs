//! Reading the aircraft in the photos the user picked (ADR-041).
//!
//! One `ai.aircraft` job per item, run by the aircraft worker in the render
//! service, which loads the text models once.
//!
//! - **The picture.** Registrations are small: on a 1024 px thumbnail they
//!   are a few pixels high and cannot be read. So a photo is read at full
//!   size from its file (the JPEG of a RAW + JPEG pair, else the RAW through
//!   LibRaw). The original is only read, never changed, and nothing is added
//!   to the cache. When the file cannot be reached (its drive is not
//!   connected) the thumbnail is read instead: the airline's name is usually
//!   big enough there. A video is read from its thumbnail frame.
//! - **What is kept.** The registration, the airline and the type become
//!   the item's keywords under [`TAG_PARENT`] (`Aircraft|Registration|C-FJZS`,
//!   `Aircraft|Airline|Air Canada`, `Aircraft|Type|Boeing 777-300ER`), in the
//!   catalog and its XMP sidecars like typed ones, with a record that the
//!   aircraft reader gave them ([`SOURCE`], ADR-039). Keywords the item has
//!   stay.
//! - **The type** is the one painted on the aircraft when it was read;
//!   otherwise the type the picture looks like, when one stands out
//!   ([`super::types`]). That needs the image encoder of On-device AI: the
//!   item's stored analysis, or the encoder itself if it is installed.

use std::sync::Arc;

use image::DynamicImage;

use super::marks::{self, Airlines, Marks};
use super::ocr::{TextLine, TextReader};
use super::types::TypeList;
use super::{install, MODEL_ID};
use crate::ai::cloud::{job_status, CloudStatus};
use crate::ai::embedding::Embedding;
use crate::ai::model::installed_runtime;
use crate::ai::{analyze, Accel, AiError, ImageModel};
use crate::catalog::{AssetId, Catalog, MediaKind, NewJob, Priority, TagPath};
use crate::library::{Library, LibraryError};
use crate::metadata::MetadataService;
use crate::render::{full, source, Cache, RenderError, RenderTools};

/// The job kind: read the aircraft in one item.
pub const JOB_AI_AIRCRAFT: &str = "ai.aircraft";
/// Who gave the keywords, in `ai_tag_sources` and the sidecars.
pub const SOURCE: &str = "aircraft";
/// For people.
pub const SOURCE_NAME: &str = "Aircraft reader";
/// The parent of the keywords.
pub const TAG_PARENT: &str = "Aircraft";
/// The detector sees a full-size picture with its long side at most this:
/// big enough to find a registration, which is then read from the picture
/// itself.
pub const DETECT_SIDE: u32 = 2560;

/// What the aircraft worker needs: the text in a picture, and the
/// picture's embedding for guessing the type. [`InstalledReader`] in the
/// app; a stand-in in tests.
pub trait Reader {
    fn read(&mut self, img: &DynamicImage) -> Result<Vec<TextLine>, AiError>;
    /// `None` when the image encoder is not installed.
    fn embed(&mut self, img: &DynamicImage) -> Result<Option<Embedding>, AiError>;
    /// Where the text detector runs.
    fn accel(&self) -> Accel;
}

/// The installed models: the text reader, and the image encoder when a
/// picture's look is first asked for (if it is installed).
pub struct InstalledReader {
    text: TextReader,
    image: Option<ImageModel>,
    image_tried: bool,
    gpu: bool,
}

impl InstalledReader {
    pub fn load(gpu: bool) -> Result<Self, AiError> {
        let text = TextReader::load(
            &install::DETECTOR.require()?,
            &install::RECOGNIZER.require()?,
            &installed_runtime()?,
            gpu,
            DETECT_SIDE,
        )?;
        Ok(Self {
            text,
            image: None,
            image_tried: false,
            gpu,
        })
    }
}

impl Reader for InstalledReader {
    fn read(&mut self, img: &DynamicImage) -> Result<Vec<TextLine>, AiError> {
        self.text.read(img)
    }

    fn embed(&mut self, img: &DynamicImage) -> Result<Option<Embedding>, AiError> {
        if !self.image_tried {
            self.image_tried = true;
            match ImageModel::load_installed(self.gpu) {
                Ok(m) => self.image = Some(m),
                Err(e) => tracing::info!("aircraft: the type is not guessed from the look: {e}"),
            }
        }
        match self.image.as_mut() {
            Some(m) => Ok(Some(m.embed(img)?)),
            None => Ok(None),
        }
    }

    fn accel(&self) -> Accel {
        self.text.accel()
    }
}

/// Loads the aircraft worker's [`Reader`]; the argument is "use the
/// graphics card".
pub type ReaderLoader = Arc<dyn Fn(bool) -> Result<Box<dyn Reader + Send>, AiError> + Send + Sync>;

/// The installed models ([`InstalledReader::load`]).
pub fn installed_loader() -> ReaderLoader {
    Arc::new(|gpu| Ok(Box::new(InstalledReader::load(gpu)?) as Box<dyn Reader + Send>))
}

/// The built-in lists the reader checks text and looks against.
#[derive(Debug, Clone)]
pub struct Lists {
    pub airlines: Airlines,
    pub types: TypeList,
}

impl Lists {
    pub fn builtin() -> Result<Self, AiError> {
        Ok(Self {
            airlines: Airlines::builtin(),
            types: TypeList::builtin()?,
        })
    }
}

/// What reading one item found and did.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    pub marks: Marks,
    /// The type guessed from the look (only without a painted one).
    pub looks_like: Option<String>,
    /// The keywords added, as `A|B|C`.
    pub added: Vec<String>,
    /// The picture was read at full size (not from the thumbnail).
    pub full_size: bool,
}

/// The picture to read for `asset`, and whether it is the full-size one.
pub fn picture(
    cat: &mut Catalog,
    lib: &Library,
    cache: &Cache,
    tools: &RenderTools,
    asset: AssetId,
) -> Result<(DynamicImage, bool), AiError> {
    match source::locate(cat, lib, asset) {
        Ok(src) if src.media_kind == MediaKind::Photo => {
            match full::decode_full(tools, &src.still) {
                Ok(img) => return Ok((img, true)),
                Err(e) => tracing::debug!(asset = %asset, "aircraft: reading the thumbnail: {e}"),
            }
        }
        Ok(_) => {}
        Err(RenderError::Catalog(e)) => return Err(e.into()),
        Err(e) => tracing::debug!(asset = %asset, "aircraft: reading the thumbnail: {e}"),
    }
    Ok((analyze::thumbnail(cat, cache, asset)?, false))
}

/// The keywords for what was found: registrations, airlines, then the
/// painted types or, without one, the type it looks like.
pub fn keyword_paths(marks: &Marks, looks_like: Option<&str>) -> Result<Vec<TagPath>, AiError> {
    let mut out = Vec::new();
    let mut add = |kind: &str, name: &str| -> Result<(), AiError> {
        // `/` and `|` cannot be in a keyword.
        let name = name.replace(['/', '|'], "-");
        out.push(TagPath::new([TAG_PARENT, kind, name.as_str()])?);
        Ok(())
    };
    for r in &marks.registrations {
        add("Registration", r)?;
    }
    for a in &marks.airlines {
        add("Airline", a)?;
    }
    for t in &marks.types {
        add("Type", t)?;
    }
    if marks.types.is_empty() {
        if let Some(t) = looks_like {
            add("Type", t)?;
        }
    }
    Ok(out)
}

/// What a picture shows: its marks and, without a painted type, the type
/// it looks like. `stored`: the item's embedding, if it has been analysed.
pub fn examine(
    reader: &mut dyn Reader,
    lists: &Lists,
    img: &DynamicImage,
    stored: Option<Embedding>,
) -> Result<(Marks, Option<String>), AiError> {
    let lines = reader.read(img)?;
    let marks = marks::interpret(&lines, &lists.airlines);
    let looks_like = if marks.types.is_empty() {
        let emb = match stored {
            Some(e) => Some(e),
            None => reader.embed(img)?,
        };
        emb.and_then(|e| lists.types.guess(&e).map(|(name, _)| name.to_owned()))
    } else {
        None
    };
    Ok((marks, looks_like))
}

fn lib_err(e: LibraryError) -> AiError {
    match e {
        LibraryError::Catalog(e) => AiError::Catalog(e),
        e => AiError::Image(e.to_string()),
    }
}

/// One aircraft job: read `asset`'s picture and add what it shows to the
/// asset's keywords (those it has stay), recording that the aircraft
/// reader gave them.
pub fn read_asset(
    cat: &mut Catalog,
    lib: &Library,
    cache: &Cache,
    tools: &RenderTools,
    reader: &mut dyn Reader,
    lists: &Lists,
    asset: AssetId,
) -> Result<Outcome, AiError> {
    let (img, full_size) = picture(cat, lib, cache, tools, asset)?;
    let stored = cat
        .read(|tx| tx.ai().analysis(asset))?
        .filter(|a| a.model == crate::ai::MODEL_ID)
        .and_then(|a| Embedding::from_quantized(&a.embedding, a.emb_scale).ok());
    let (marks, looks_like) = examine(reader, lists, &img, stored)?;
    drop(img);
    let paths = keyword_paths(&marks, looks_like.as_deref())?;
    let added: Vec<String> = if paths.is_empty() {
        Vec::new()
    } else {
        MetadataService::new(lib, cat)
            .add_ai_keywords(asset, SOURCE, &paths)
            .map_err(lib_err)?
            .iter()
            .map(TagPath::to_lightroom)
            .collect()
    };
    tracing::info!(asset = %asset, model = MODEL_ID, full_size, "aircraft read: {added:?}");
    Ok(Outcome {
        marks,
        looks_like,
        added,
        full_size,
    })
}

/// Queue `ids` to be read (again, if they were before), ahead of the
/// backlog.
pub fn enqueue(cat: &mut Catalog, ids: &[AssetId]) -> Result<u64, AiError> {
    let now = crate::jobs::now_ms();
    let size = crate::render::ENQUEUE_CHUNK;
    let mut n = 0;
    for (c, chunk) in ids.chunks(size).enumerate() {
        n += cat.write(|tx| {
            let jobs: Vec<NewJob<'_>> = chunk
                .iter()
                .enumerate()
                .map(|(i, id)| NewJob {
                    kind: JOB_AI_AIRCRAFT,
                    asset_id: *id,
                    rank: 0,
                    sort_key: i64::try_from(c * size + i).unwrap_or(i64::MAX),
                })
                .collect();
            tx.jobs().enqueue(&jobs, Priority::Recent, now, true)
        })?;
    }
    Ok(n)
}

/// How `ids` are getting on, and the latest problem.
pub fn status(cat: &Catalog, ids: &[AssetId]) -> Result<(CloudStatus, Option<String>), AiError> {
    job_status(cat, JOB_AI_AIRCRAFT, ids)
}

/// Stop: drop the queued jobs of `ids` (items read stay read).
pub fn stop(cat: &mut Catalog, ids: &[AssetId]) -> Result<u64, AiError> {
    Ok(cat.write(|tx| tx.jobs().remove_queued_for(JOB_AI_AIRCRAFT, ids))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_was_found_becomes_keywords_under_aircraft() {
        let marks = Marks {
            registrations: vec!["C-FJZS".into()],
            airlines: vec!["Air Canada".into()],
            types: vec!["Boeing 777-300ER".into()],
        };
        let paths: Vec<String> = keyword_paths(&marks, Some("Boeing 777"))
            .unwrap()
            .iter()
            .map(TagPath::to_lightroom)
            .collect();
        assert_eq!(
            paths,
            [
                "Aircraft|Registration|C-FJZS",
                "Aircraft|Airline|Air Canada",
                "Aircraft|Type|Boeing 777-300ER",
            ],
            "a painted type is preferred to the look"
        );
        let looks: Vec<String> = keyword_paths(&Marks::default(), Some("Boeing 737"))
            .unwrap()
            .iter()
            .map(TagPath::to_lightroom)
            .collect();
        assert_eq!(looks, ["Aircraft|Type|Boeing 737"]);
        assert!(keyword_paths(&Marks::default(), None).unwrap().is_empty());
    }

    struct Fake {
        lines: Vec<TextLine>,
        embedding: Option<Embedding>,
        embedded: u32,
    }

    impl Reader for Fake {
        fn read(&mut self, _: &DynamicImage) -> Result<Vec<TextLine>, AiError> {
            Ok(self.lines.clone())
        }
        fn embed(&mut self, _: &DynamicImage) -> Result<Option<Embedding>, AiError> {
            self.embedded += 1;
            Ok(self.embedding.clone())
        }
        fn accel(&self) -> Accel {
            Accel::Cpu
        }
    }

    fn line(text: &str) -> TextLine {
        TextLine {
            text: text.to_owned(),
            score: 0.99,
            quad: [[0.0, 0.0], [100.0, 0.0], [100.0, 30.0], [0.0, 30.0]],
        }
    }

    #[test]
    fn the_look_is_only_asked_for_without_a_painted_type() {
        let lists = Lists::builtin().unwrap();
        let img = DynamicImage::new_rgb8(8, 8);
        let mut painted = Fake {
            lines: vec![line("AIR CANADA"), line("BOEING 787-9")],
            embedding: None,
            embedded: 0,
        };
        let (marks, looks) = examine(&mut painted, &lists, &img, None).unwrap();
        assert_eq!(marks.airlines, ["Air Canada"]);
        assert_eq!(marks.types, ["Boeing 787-9"]);
        assert_eq!((looks, painted.embedded), (None, 0));

        // No painted type and no image encoder: no guess.
        let mut bare = Fake {
            lines: vec![line("KLM")],
            embedding: None,
            embedded: 0,
        };
        let (marks, looks) = examine(&mut bare, &lists, &img, None).unwrap();
        assert_eq!(marks.airlines, ["KLM"]);
        assert_eq!((looks, bare.embedded), (None, 1));
    }
}

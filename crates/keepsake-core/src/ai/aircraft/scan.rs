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
//! - **What is kept.** Every registration, airline and type the text could
//!   give, each with the confidence it was read with ([`marks::Candidates`]),
//!   and the type the picture looks like, in `aircraft_reads`, waiting for
//!   review. Nothing becomes a keyword by itself: in the review window the
//!   user sets how sure each kind must be ([`marks::Confidence`]), unticks
//!   mistakes and accepts ([`accept`]) or dismisses ([`dismiss`]). What is
//!   accepted becomes the item's keywords under [`TAG_PARENT`]
//!   (`Aircraft|Registration|C-FJZS`, `Aircraft|Airline|Air Canada`,
//!   `Aircraft|Type|Boeing 777-300ER`), in the catalog and its XMP sidecars
//!   like typed ones, with a record that the aircraft reader gave them
//!   ([`SOURCE`], ADR-039). Keywords the item has stay.
//! - **A folder or dates at once** ([`enqueue_target`]): the items there
//!   that carry one of [`AIRCRAFT_TAGS`] (Airplane, Helicopter, Airport; as
//!   a keyword or as an on-device AI tag) and have not been read yet (by
//!   this version: a photo read before reads were kept is read again). So
//!   Tag photos comes first: a photo with no such tag is not read.
//! - **The type** is the one painted on the aircraft when it was read;
//!   otherwise the type the picture looks like, when one stands out
//!   ([`super::types`]). That needs the image encoder of On-device AI: the
//!   item's stored analysis, or the encoder itself if it is installed.

use std::sync::Arc;

use image::DynamicImage;

use super::marks::{self, Airlines, Candidates, Confidence, Marks};
use super::ocr::{TextLine, TextReader};
use super::types::TypeList;
use super::{install, MODEL_ID};
use crate::ai::analyze::{RangeStatus, Target};
use crate::ai::cloud::{job_status, CloudStatus};
use crate::ai::embedding::Embedding;
use crate::ai::model::installed_runtime;
use crate::ai::{analyze, Accel, AiError, ImageModel};
use crate::catalog::{AssetId, Catalog, JobState, MediaKind, NewJob, Priority, TagPath};
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
/// The tags that mark an item as showing aircraft: a folder or dates are
/// read for the items with one of them (or a tag below it).
pub const AIRCRAFT_TAGS: [&str; 3] = ["Airplane", "Helicopter", "Airport"];
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

/// What reading one item found (kept for review; no keyword is added).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Outcome {
    pub candidates: Candidates,
    /// What the default confidence would give.
    pub marks: Marks,
    /// The type the picture looks like, when one stands out.
    pub looks_like: Option<String>,
    /// The picture was read at full size (not from the thumbnail).
    pub full_size: bool,
}

impl Outcome {
    /// What it found at the default confidence, as keywords (`A|B|C`).
    pub fn found(&self) -> Vec<String> {
        keyword_paths(&self.marks, self.looks_like.as_deref())
            .map(|p| p.iter().map(TagPath::to_lightroom).collect())
            .unwrap_or_default()
    }
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

/// What a picture shows: every possible mark and the type it looks like
/// (asked for even with a painted type: the user may set the type's
/// confidence above it). `stored`: the item's embedding, if it has been
/// analysed.
pub fn examine(
    reader: &mut dyn Reader,
    lists: &Lists,
    img: &DynamicImage,
    stored: Option<Embedding>,
) -> Result<(Candidates, Option<String>), AiError> {
    let lines = reader.read(img)?;
    let found = marks::candidates(&lines, &lists.airlines);
    let emb = match stored {
        Some(e) => Some(e),
        None => reader.embed(img)?,
    };
    let looks_like = emb.and_then(|e| lists.types.guess(&e).map(|(name, _)| name.to_owned()));
    Ok((found, looks_like))
}

fn lib_err(e: LibraryError) -> AiError {
    match e {
        LibraryError::Catalog(e) => AiError::Catalog(e),
        e => AiError::Image(e.to_string()),
    }
}

/// One aircraft job: read `asset`'s picture and keep what it shows for
/// review (a read before is replaced, and waits for review again).
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
    let (candidates, looks_like) = examine(reader, lists, &img, stored)?;
    drop(img);
    let json = serde_json::to_string(&candidates)
        .map_err(|e| AiError::Image(format!("the read could not be kept: {e}")))?;
    let now = crate::jobs::now_ms();
    cat.write(|tx| {
        tx.ai()
            .put_aircraft_read(asset, &json, looks_like.as_deref(), full_size, now)
    })?;
    let out = Outcome {
        marks: candidates.at(&Confidence::default()),
        candidates,
        looks_like,
        full_size,
    };
    tracing::info!(asset = %asset, model = MODEL_ID, full_size, "aircraft read: {:?}", out.found());
    Ok(out)
}

/// Where a read stands in review.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Review {
    /// Read; nothing accepted yet.
    Waiting,
    /// The user accepted (some of) it: those are the keywords.
    Accepted,
    /// The user said there is nothing to keep.
    Dismissed,
}

impl Review {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Accepted => "accepted",
            Self::Dismissed => "dismissed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "accepted" => Self::Accepted,
            "dismissed" => Self::Dismissed,
            _ => Self::Waiting,
        }
    }
}

/// One read photo, for the review window.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewItem {
    pub asset: AssetId,
    pub candidates: Candidates,
    pub looks_like: Option<String>,
    pub full_size: bool,
    pub made_ms: i64,
    pub review: Review,
}

/// The reads of `ids`, in their order (those not read yet are left out).
pub fn review_items(cat: &Catalog, ids: &[AssetId]) -> Result<Vec<ReviewItem>, AiError> {
    let rows = cat.read(|tx| {
        let mut out = Vec::new();
        for id in ids {
            if let Some(r) = tx.ai().aircraft_read(*id)? {
                out.push(r);
            }
        }
        Ok::<_, crate::catalog::CatalogError>(out)
    })?;
    Ok(rows
        .into_iter()
        .map(|r| ReviewItem {
            asset: r.asset_id,
            // A read this version cannot understand counts as no mark.
            candidates: serde_json::from_str(&r.candidates).unwrap_or_default(),
            looks_like: r.looks_like,
            full_size: r.full_size,
            made_ms: r.made_ms,
            review: Review::parse(&r.review),
        })
        .collect())
}

/// The keywords to offer for a read at `conf`: its marks, then the type it
/// looks like when no painted type is sure enough.
pub fn offered(item: &ReviewItem, conf: &Confidence) -> Result<Vec<TagPath>, AiError> {
    keyword_paths(&item.candidates.at(conf), item.looks_like.as_deref())
}

/// Is `path` a keyword the aircraft reader can give
/// (`Aircraft|Registration|…`, `…|Airline|…`, `…|Type|…`)?
pub fn is_aircraft_keyword(path: &TagPath) -> bool {
    let parts = path.segments();
    parts.len() == 3
        && parts[0] == TAG_PARENT
        && matches!(parts[1].as_str(), "Registration" | "Airline" | "Type")
}

/// Accept a read: `paths` (those the user kept ticked) become the item's
/// keywords, recorded as given by the aircraft reader. Returns the keywords
/// added (`A|B|C`). Paths that are not aircraft keywords are refused.
pub fn accept(
    cat: &mut Catalog,
    lib: &Library,
    asset: AssetId,
    paths: &[TagPath],
) -> Result<Vec<String>, AiError> {
    if let Some(p) = paths.iter().find(|p| !is_aircraft_keyword(p)) {
        return Err(AiError::Image(format!(
            "{} is not an aircraft keyword",
            p.to_lightroom()
        )));
    }
    let added: Vec<String> = if paths.is_empty() {
        Vec::new()
    } else {
        MetadataService::new(lib, cat)
            .add_ai_keywords(asset, SOURCE, paths)
            .map_err(lib_err)?
            .iter()
            .map(TagPath::to_lightroom)
            .collect()
    };
    let now = crate::jobs::now_ms();
    let review = if paths.is_empty() {
        Review::Dismissed
    } else {
        Review::Accepted
    };
    cat.write(|tx| tx.ai().set_aircraft_review(asset, review.as_str(), now))?;
    Ok(added)
}

/// What the review window offers for a read: the keywords (ticked unless
/// in `unticked`), and which came from an online lookup.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Offer {
    pub keywords: Vec<TagPath>,
    /// Offered but not ticked: a painted type the lookup disagrees with.
    pub unticked: Vec<TagPath>,
    /// Those the lookup gave.
    pub from_lookup: Vec<TagPath>,
}

/// The keywords to offer for a read at `conf`, with what lookups of its
/// registrations said (`found`, those known): a looked-up type and airline
/// are offered beside what was read; a painted type that disagrees is
/// offered unticked; the type from the look only when neither gives one.
pub fn offer(
    item: &ReviewItem,
    conf: &Confidence,
    found: &[super::lookup::Found],
) -> Result<Offer, AiError> {
    let marks = item.candidates.at(conf);
    let known: Vec<&super::lookup::Found> = found
        .iter()
        .filter(|f| f.known && marks.registrations.contains(&f.registration))
        .collect();
    let looked_type = known.iter().find_map(|f| f.aircraft_type.clone());
    let look = if marks.types.is_empty() && looked_type.is_none() {
        item.looks_like.as_deref()
    } else {
        None
    };
    let mut out = Offer {
        keywords: keyword_paths(&marks, look)?,
        ..Offer::default()
    };
    let path = |kind: &str, name: &str| {
        TagPath::new([TAG_PARENT, kind, name.replace(['/', '|'], "-").as_str()])
    };
    for f in &known {
        for (kind, name) in [("Airline", &f.airline), ("Type", &f.aircraft_type)] {
            if let Some(name) = name {
                let p = path(kind, name)?;
                if !out.keywords.contains(&p) {
                    out.keywords.push(p.clone());
                }
                out.from_lookup.push(p);
            }
        }
    }
    if let Some(t) = &looked_type {
        for painted in &marks.types {
            if painted != t {
                out.unticked.push(path("Type", painted)?);
            }
        }
    }
    Ok(out)
}

/// What lookups said about the registrations a read offers at `conf`
/// (kept ones only: nothing is asked here).
pub fn found_for(
    cat: &Catalog,
    item: &ReviewItem,
    conf: &Confidence,
) -> Result<Vec<super::lookup::Found>, AiError> {
    let mut out = Vec::new();
    for reg in item.candidates.at(conf).registrations {
        if let Some(f) = super::lookup::kept(cat, &reg)? {
            out.push(f);
        }
    }
    Ok(out)
}

/// Photos taken this close (either side) are offered with a read.
pub const NEIGHBOUR_MS: i64 = 10_000;

/// A photo taken just before or after a read one.
#[derive(Debug, Clone, PartialEq)]
pub struct Neighbour {
    pub asset: AssetId,
    /// Milliseconds after the read photo (before: below zero).
    pub ms: i64,
    /// How alike the two pictures are (On-device AI's comparison), when
    /// both have been analysed.
    pub similarity: Option<f32>,
    /// Alike enough to be the same shot of the same aircraft.
    pub alike: bool,
}

/// The photos taken by the same camera within [`NEIGHBOUR_MS`] of `asset`,
/// nearest first, each with how alike it is (alike: at least
/// [`crate::ai::duplicates::COSINE_BURST`], the bar Duplicates uses for
/// photos taken moments apart). Not analysed by On-device AI: no
/// similarity, not alike.
pub fn neighbours(cat: &Catalog, asset: AssetId) -> Result<Vec<Neighbour>, AiError> {
    let emb = |id: AssetId| -> Result<Option<Embedding>, AiError> {
        Ok(cat
            .read(|tx| tx.ai().analysis(id))?
            .filter(|a| a.model == crate::ai::MODEL_ID)
            .and_then(|a| Embedding::from_quantized(&a.embedding, a.emb_scale).ok()))
    };
    let mine = emb(asset)?;
    let mut out = Vec::new();
    for (id, ms) in cat.read(|tx| tx.ai().taken_near(asset, NEIGHBOUR_MS))? {
        let similarity = match (&mine, emb(id)?) {
            (Some(a), Some(b)) => Some(a.cosine(&b)),
            _ => None,
        };
        out.push(Neighbour {
            asset: id,
            ms,
            similarity,
            alike: similarity.is_some_and(|s| s >= crate::ai::duplicates::COSINE_BURST),
        });
    }
    Ok(out)
}

/// Other photos with `registration` as a keyword that lack some of
/// `wanted` (an airline or type accepted for it): each with what it lacks.
pub fn same_registration(
    cat: &Catalog,
    registration: &str,
    wanted: &[TagPath],
    except: &[AssetId],
) -> Result<Vec<(AssetId, Vec<TagPath>)>, AiError> {
    let reg = TagPath::new([TAG_PARENT, "Registration", registration])?;
    let ids = cat.read(|tx| tx.tags().assets_with(&reg))?;
    let mut out = Vec::new();
    for id in ids {
        if except.contains(&id) {
            continue;
        }
        let has: Vec<TagPath> = cat
            .read(|tx| tx.tags().for_asset(id))?
            .into_iter()
            .map(|t| t.path)
            .collect();
        let missing: Vec<TagPath> = wanted
            .iter()
            .filter(|w| !has.contains(w))
            .cloned()
            .collect();
        if !missing.is_empty() {
            out.push((id, missing));
        }
    }
    Ok(out)
}

/// Add aircraft keywords to photos (the same aircraft's other photos, or a
/// read photo's neighbours), recorded as given by the aircraft reader.
/// Returns how many photos got something new.
pub fn add_to(
    cat: &mut Catalog,
    lib: &Library,
    ids: &[AssetId],
    paths: &[TagPath],
) -> Result<u64, AiError> {
    if let Some(p) = paths.iter().find(|p| !is_aircraft_keyword(p)) {
        return Err(AiError::Image(format!(
            "{} is not an aircraft keyword",
            p.to_lightroom()
        )));
    }
    let mut n = 0;
    for id in ids {
        let added = MetadataService::new(lib, cat)
            .add_ai_keywords(*id, SOURCE, paths)
            .map_err(lib_err)?;
        if !added.is_empty() {
            n += 1;
        }
    }
    Ok(n)
}

/// Dismiss a read: nothing becomes a keyword.
pub fn dismiss(cat: &mut Catalog, asset: AssetId) -> Result<(), AiError> {
    let now = crate::jobs::now_ms();
    cat.write(|tx| {
        tx.ai()
            .set_aircraft_review(asset, Review::Dismissed.as_str(), now)
    })?;
    Ok(())
}

/// Queue `ids` to be read (again, if they were before), ahead of the
/// backlog.
pub fn enqueue(cat: &mut Catalog, ids: &[AssetId]) -> Result<u64, AiError> {
    enqueue_ids(cat, ids, true)
}

/// Queue `ids`; `again`: also those already read.
fn enqueue_ids(cat: &mut Catalog, ids: &[AssetId], again: bool) -> Result<u64, AiError> {
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
            tx.jobs().enqueue(&jobs, Priority::Recent, now, again)
        })?;
    }
    Ok(n)
}

/// The items of a folder (and those in it) or of dates that carry one of
/// [`AIRCRAFT_TAGS`], newest first within each tag, each once.
pub fn target_items(cat: &Catalog, target: &Target) -> Result<Vec<AssetId>, AiError> {
    let mut out: Vec<AssetId> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for tag in AIRCRAFT_TAGS {
        let mut q = target.query()?;
        q.tag = Some(tag.to_owned());
        for it in cat.read(|tx| tx.ai().matching(&q, crate::ai::MODEL_ID))? {
            if seen.insert(it.id) {
                out.push(it.id);
            }
        }
    }
    Ok(out)
}

fn status_of(cat: &Catalog, ids: &[AssetId]) -> Result<RangeStatus, AiError> {
    let mut st = RangeStatus {
        items: ids.len() as u64,
        ..RangeStatus::default()
    };
    cat.read(|tx| {
        for id in ids {
            match tx.jobs().find(JOB_AI_AIRCRAFT, *id)?.map(|j| j.state) {
                Some(JobState::Done) => st.analysed += 1,
                Some(JobState::Failed) | None => {}
                Some(_) => st.pending += 1,
            }
        }
        Ok::<_, crate::catalog::CatalogError>(())
    })?;
    Ok(st)
}

/// Read the aircraft photos of a folder or dates: those with an aircraft
/// tag not read yet are queued (those read stay as they are). Returns how
/// far it has got: `items` the aircraft photos there, `analysed` those
/// read, `pending` those waiting.
pub fn enqueue_target(cat: &mut Catalog, target: &Target) -> Result<RangeStatus, AiError> {
    let ids = target_items(cat, target)?;
    // Read before reads were kept: read again, to have something to review.
    let unkept = cat.read(|tx| {
        let mut out = Vec::new();
        for id in &ids {
            if tx.ai().aircraft_read(*id)?.is_none() {
                out.push(*id);
            }
        }
        Ok::<_, crate::catalog::CatalogError>(out)
    })?;
    enqueue_ids(cat, &unkept, true)?;
    status_of(cat, &ids)
}

/// How far reading a folder's or dates' aircraft photos has got.
pub fn target_status(cat: &Catalog, target: &Target) -> Result<RangeStatus, AiError> {
    status_of(cat, &target_items(cat, target)?)
}

/// Stop reading a folder or dates: the queued jobs are dropped (one being
/// read finishes; what was read stays). Returns how many were dropped.
pub fn stop_target(cat: &mut Catalog, target: &Target) -> Result<u64, AiError> {
    let ids = target_items(cat, target)?;
    stop(cat, &ids)
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
    fn the_look_is_asked_for_and_marks_come_at_a_confidence() {
        let lists = Lists::builtin().unwrap();
        let img = DynamicImage::new_rgb8(8, 8);
        let mut painted = Fake {
            lines: vec![line("AIR CANADA"), line("BOEING 787-9")],
            embedding: None,
            embedded: 0,
        };
        let (found, looks) = examine(&mut painted, &lists, &img, None).unwrap();
        let marks = found.at(&Confidence::default());
        assert_eq!(marks.airlines, ["Air Canada"]);
        assert_eq!(marks.types, ["Boeing 787-9"]);
        // The look is asked for too (the type's confidence may be set
        // above the painted one), but no encoder: no guess.
        assert_eq!((looks, painted.embedded), (None, 1));

        // No painted type and no image encoder: no guess.
        let mut bare = Fake {
            lines: vec![line("KLM")],
            embedding: None,
            embedded: 0,
        };
        let (found, looks) = examine(&mut bare, &lists, &img, None).unwrap();
        assert_eq!(found.at(&Confidence::default()).airlines, ["KLM"]);
        assert_eq!((looks, bare.embedded), (None, 1));
    }

    #[test]
    fn only_aircraft_keywords_can_be_accepted() {
        let ok = TagPath::new(["Aircraft", "Registration", "VT-JEX"]).unwrap();
        assert!(is_aircraft_keyword(&ok));
        for bad in [
            TagPath::new(["People", "Ahmad"]).unwrap(),
            TagPath::new(["Aircraft", "Pilot", "Ahmad"]).unwrap(),
            TagPath::new(["Aircraft", "Registration"]).unwrap(),
        ] {
            assert!(!is_aircraft_keyword(&bad), "{}", bad.to_lightroom());
        }
        assert_eq!(Review::parse("accepted"), Review::Accepted);
        assert_eq!(Review::parse(Review::Dismissed.as_str()), Review::Dismissed);
        assert_eq!(Review::parse("?"), Review::Waiting);
    }

    #[test]
    fn what_is_offered_follows_the_confidence() {
        let item = ReviewItem {
            asset: AssetId(uuid::Uuid::nil()),
            candidates: Candidates {
                registrations: vec![("VT-JEX".into(), 0.97)],
                airlines: vec![("Jet Airways".into(), 0.82)],
                types: vec![("Boeing 777-300ER".into(), 0.66)],
            },
            looks_like: Some("Boeing 777".into()),
            full_size: true,
            made_ms: 0,
            review: Review::Waiting,
        };
        let at = |r: f32, a: f32, t: f32| -> Vec<String> {
            offered(
                &item,
                &Confidence {
                    registration: r,
                    airline: a,
                    aircraft_type: t,
                },
            )
            .unwrap()
            .iter()
            .map(TagPath::to_lightroom)
            .collect()
        };
        // The defaults: the airline (0.82) and the painted type (0.66) are
        // not sure enough; the look stands in for the type.
        assert_eq!(
            at(0.8, 0.85, 0.7),
            ["Aircraft|Registration|VT-JEX", "Aircraft|Type|Boeing 777"]
        );
        assert_eq!(
            at(0.8, 0.8, 0.6),
            [
                "Aircraft|Registration|VT-JEX",
                "Aircraft|Airline|Jet Airways",
                "Aircraft|Type|Boeing 777-300ER"
            ]
        );
        assert_eq!(at(0.99, 0.99, 0.99), ["Aircraft|Type|Boeing 777"]);
    }
}

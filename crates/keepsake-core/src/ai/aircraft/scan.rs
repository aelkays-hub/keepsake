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
//! - **A closer look** ([`read_lines`]). The detector sees the picture at
//!   most [`DETECT_SIDE`] across, so on a big photo of a distant aircraft
//!   the registration is too small to be found. When an airline's name is
//!   read but no registration, the fuselage around the name is read again
//!   at full size ([`closer_look`]).

use std::sync::Arc;

use image::DynamicImage;

use super::marks::{self, Airlines, Candidates, Confidence, Marks};
use super::ocr::{TextLine, TextReader};
use super::planes::{Plane, PlaneFinder};
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
    /// Where the aircraft are in `img` (none when the aircraft finder is
    /// not installed: the whole picture is then read).
    fn planes(&mut self, _img: &DynamicImage) -> Result<Vec<Plane>, AiError> {
        Ok(Vec::new())
    }
    /// `None` when the image encoder is not installed.
    fn embed(&mut self, img: &DynamicImage) -> Result<Option<Embedding>, AiError>;
    /// Where the text detector runs.
    fn accel(&self) -> Accel;
}

/// The installed models: the text reader, and the image encoder when a
/// picture's look is first asked for (if it is installed).
pub struct InstalledReader {
    text: TextReader,
    finder: Option<PlaneFinder>,
    finder_tried: bool,
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
            finder: None,
            finder_tried: false,
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

    fn planes(&mut self, img: &DynamicImage) -> Result<Vec<Plane>, AiError> {
        if !self.finder_tried {
            self.finder_tried = true;
            match install::PLANES
                .find()
                .ok_or_else(|| AiError::ModelMissing(install::PLANES.file.into()))
                .and_then(|m| PlaneFinder::load(&m, &installed_runtime()?, self.gpu))
            {
                Ok(f) => self.finder = Some(f),
                Err(e) => tracing::info!("aircraft: the whole picture is read: {e}"),
            }
        }
        match self.finder.as_mut() {
            Some(f) => f.find(img),
            None => Ok(Vec::new()),
        }
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

/// Around an airline's name, how far the fuselage is looked at: this many
/// times the name's width to either side, half that above and below.
const LOOK_AROUND: f32 = 5.0;

/// Where to read `w`×`h` again at full size, after `lines` were read from
/// it whole: around the airline names, when no registration was read and
/// the picture was shrunk for the detector (the region is at most 3/4 of
/// the picture's long side, or nothing is gained). `[x, y, w, h]`.
pub fn closer_look(lines: &[TextLine], airlines: &Airlines, w: u32, h: u32) -> Option<[u32; 4]> {
    let long = w.max(h) as f32;
    if long <= DETECT_SIDE as f32 * 1.25 {
        return None;
    }
    let found = marks::candidates(lines, airlines);
    if !found.registrations.is_empty() {
        return None;
    }
    let names: Vec<&TextLine> = lines
        .iter()
        .filter(|l| airlines.find(&l.text).is_some())
        .collect();
    if names.is_empty() {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for l in names {
        let [cx, cy] = l.center();
        let size = l.width().max(l.height() * 3.0);
        x0 = x0.min(cx - size * LOOK_AROUND);
        x1 = x1.max(cx + size * LOOK_AROUND);
        y0 = y0.min(cy - size * LOOK_AROUND / 2.0);
        y1 = y1.max(cy + size * LOOK_AROUND / 2.0);
    }
    let x0 = x0.clamp(0.0, w as f32) as u32;
    let y0 = y0.clamp(0.0, h as f32) as u32;
    let x1 = x1.clamp(0.0, w as f32) as u32;
    let y1 = y1.clamp(0.0, h as f32) as u32;
    let (cw, ch) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    let small = cw.max(ch) as f32 <= long * 0.75;
    (small && cw >= 32 && ch >= 32).then_some([x0, y0, cw, ch])
}

/// Around an aircraft found, how much more is read: this share of its
/// size on every side (a registration at the very end of the tail).
pub const PLANE_MARGIN: f32 = 0.08;
/// At most this many aircraft are read in one picture, surest first.
pub const MAX_PLANES: usize = 4;
/// An aircraft taking at least this share of the picture's long side is
/// read in the whole picture (only the text inside it kept): cutting it
/// out gains no detail, and the text reader reads a picture cut at another
/// size a little differently (SAUDIA beside its Arabic, DSC03440).
pub const BIG_PLANE: f32 = 0.6;

/// The text in `img`. With aircraft found in it (`planes`, from
/// [`Reader::planes`]), only the text on them, so that a sign, a jet
/// bridge, a vehicle or a signature beside it is not read: a big one in the
/// whole picture, keeping the text inside it; a smaller one cut out with a
/// small margin and read at full size. With none (the finder is not
/// installed, or a close-up of a tail is not seen as an aircraft), the
/// whole picture. Either way, when an airline is read but no registration,
/// closer around the airline ([`closer_look`]). In the picture's
/// coordinates, biggest letters first.
pub fn read_lines(
    read: &mut dyn FnMut(&DynamicImage) -> Result<Vec<TextLine>, AiError>,
    planes: &[Plane],
    airlines: &Airlines,
    img: &DynamicImage,
) -> Result<Vec<TextLine>, AiError> {
    if planes.is_empty() {
        return read_part(read, airlines, img);
    }
    let (iw, ih) = (img.width(), img.height());
    let long = iw.max(ih) as f32;
    let regions: Vec<[u32; 4]> = planes
        .iter()
        .take(MAX_PLANES)
        .map(|p| p.grown(PLANE_MARGIN, iw, ih))
        .filter(|[_, _, w, h]| *w >= 16 && *h >= 16)
        .collect();
    let (big, small): (Vec<[u32; 4]>, Vec<[u32; 4]>) = regions
        .into_iter()
        .partition(|[_, _, w, h]| (*w).max(*h) as f32 >= long * BIG_PLANE);
    let mut lines = Vec::new();
    if !big.is_empty() {
        let inside = |l: &TextLine| {
            let [cx, cy] = l.center();
            big.iter().any(|&[x, y, w, h]| {
                cx >= x as f32 && cx <= (x + w) as f32 && cy >= y as f32 && cy <= (y + h) as f32
            })
        };
        lines.extend(read_part(read, airlines, img)?.into_iter().filter(inside));
    }
    for [x, y, w, h] in small {
        let part = img.crop_imm(x, y, w, h);
        for mut l in read_part(read, airlines, &part)? {
            shift(&mut l, x, y);
            lines.push(l);
        }
    }
    lines.sort_by(|a, b| b.height().total_cmp(&a.height()));
    Ok(lines)
}

fn shift(l: &mut TextLine, x: u32, y: u32) {
    for p in &mut l.quad {
        p[0] += x as f32;
        p[1] += y as f32;
    }
}

/// Every line of text in `img` ([`Reader::read`]), and then, when an
/// airline is read but no registration, the lines read closer around it
/// ([`closer_look`]), in `img`'s coordinates.
fn read_part(
    read: &mut dyn FnMut(&DynamicImage) -> Result<Vec<TextLine>, AiError>,
    airlines: &Airlines,
    img: &DynamicImage,
) -> Result<Vec<TextLine>, AiError> {
    let mut lines = read(img)?;
    if let Some([x, y, w, h]) = closer_look(&lines, airlines, img.width(), img.height()) {
        let part = img.crop_imm(x, y, w, h);
        let closer = read(&part)?;
        tracing::debug!(
            x,
            y,
            w,
            h,
            "aircraft: read closer: {:?}",
            closer.iter().map(|l| &l.text).collect::<Vec<_>>()
        );
        for mut l in closer {
            shift(&mut l, x, y);
            lines.push(l);
        }
        lines.sort_by(|a, b| b.height().total_cmp(&a.height()));
    }
    Ok(lines)
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
    let planes = reader.planes(img)?;
    tracing::debug!("aircraft: {} aircraft found", planes.len());
    let lines = read_lines(&mut |i| reader.read(i), &planes, &lists.airlines, img)?;
    let found = marks::candidates(&lines, &lists.airlines);
    let emb = match stored {
        Some(e) => Some(e),
        None => reader.embed(img)?,
    };
    let looks_like = emb.and_then(|e| lists.types.guess(&e).map(|(name, _)| name.to_owned()));
    Ok((found, looks_like))
}

pub(crate) fn lib_err(e: LibraryError) -> AiError {
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
            .put_aircraft_read(asset, &json, looks_like.as_deref(), full_size, now)?;
        // Nothing found: nothing to review.
        tx.ai().dismiss_empty_aircraft_reads(&[asset], now)
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
/// (`Aircraft|Registration|…`, `…|Airline|…`, `…|Type|…`, `…|Built|…`)?
pub fn is_aircraft_keyword(path: &TagPath) -> bool {
    let parts = path.segments();
    parts.len() == 3
        && parts[0] == TAG_PARENT
        && matches!(
            parts[1].as_str(),
            "Registration" | "Airline" | "Type" | super::built::KIND
        )
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
/// registrations said (`found`, those known): a looked-up type, airline and
/// build year are offered beside what was read; a painted type that disagrees is
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
        // The build year from OpenSky's database (ADR-044).
        let built = f.built.as_deref().map(|b| super::built::year(b).to_owned());
        for (kind, name) in [
            ("Airline", &f.airline),
            ("Type", &f.aircraft_type),
            (super::built::KIND, &built),
        ] {
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
            if !same_type(painted, t) {
                out.unticked.push(path("Type", painted)?);
            }
        }
    }
    Ok(out)
}

/// A painted type agrees with the looked-up one when it is the same, or
/// the same maker and model said less exactly: "Boeing 787 Dreamliner" is
/// a "Boeing 787-9" (A7202202.ARW, C-FVLU), "Airbus A320neo" an "Airbus
/// A320-251N"; "Boeing 777-300ER" is not a "Boeing 777-200LR".
fn same_type(painted: &str, looked: &str) -> bool {
    if painted.eq_ignore_ascii_case(looked) {
        return true;
    }
    // The maker, then the model's family ("787", "A320") and whether a
    // variant follows it ("-9", "-300ER").
    fn split(s: &str) -> Option<(String, String, bool)> {
        let mut words = s.split_whitespace();
        let maker = words.next()?.to_ascii_lowercase();
        let model = words.next()?.to_ascii_lowercase();
        let digits_end = model
            .char_indices()
            .skip_while(|(_, c)| c.is_ascii_alphabetic())
            .find(|(_, c)| !c.is_ascii_digit())
            .map_or(model.len(), |(i, _)| i);
        let (family, after) = model.split_at(digits_end);
        if !family.chars().any(|c| c.is_ascii_digit()) {
            return None;
        }
        Some((maker, family.to_owned(), after.starts_with('-')))
    }
    match (split(painted), split(looked)) {
        (Some((pm, pf, p_variant)), Some((lm, lf, _))) => pm == lm && pf == lf && !p_variant,
        _ => false,
    }
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

/// Photos taken this close (either side) are offered with a read, unless
/// the user sets another window (`ai.same_shot_s`).
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

/// The photos taken by the same camera and lens within `window_ms` of `asset`,
/// nearest first, each with how alike it is (alike: at least
/// [`crate::ai::duplicates::COSINE_BURST`], the bar Duplicates uses for
/// photos taken moments apart). Not analysed by On-device AI: no
/// similarity, not alike.
pub fn neighbours(
    cat: &Catalog,
    asset: AssetId,
    window_ms: i64,
) -> Result<Vec<Neighbour>, AiError> {
    let emb = |id: AssetId| -> Result<Option<Embedding>, AiError> {
        Ok(cat
            .read(|tx| tx.ai().analysis(id))?
            .filter(|a| a.model == crate::ai::MODEL_ID)
            .and_then(|a| Embedding::from_quantized(&a.embedding, a.emb_scale).ok()))
    };
    let mine = emb(asset)?;
    let mut out = Vec::new();
    for (id, ms) in cat.read(|tx| tx.ai().taken_near(asset, window_ms))? {
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

/// The photos taken beside an accepted one (its same shot, ticked in the
/// review window): `paths` are added to each, and each waiting for review
/// (or not read yet) counts as accepted, so the window does not ask about
/// it again (also when it is read later); one already accepted or
/// dismissed keeps its review. With no `paths` (the photo was dismissed)
/// nothing happens. Returns the photos accepted by this.
pub fn accept_neighbours(
    cat: &mut Catalog,
    lib: &Library,
    ids: &[AssetId],
    paths: &[TagPath],
) -> Result<Vec<AssetId>, AiError> {
    if paths.is_empty() || ids.is_empty() {
        return Ok(Vec::new());
    }
    add_to(cat, lib, ids, paths)?;
    let empty = serde_json::to_string(&Candidates::default())
        .map_err(|e| AiError::Image(format!("the read could not be kept: {e}")))?;
    let now = crate::jobs::now_ms();
    Ok(cat.write(|tx| {
        let mut out = Vec::new();
        for id in ids {
            if tx.ai().accept_aircraft_unread(*id, &empty, now)? {
                out.push(*id);
            }
        }
        Ok::<_, crate::catalog::CatalogError>(out)
    })?)
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

/// Set aside the reads of `ids` waiting for review that found nothing at
/// all (read before this was done at read time): the review window opens
/// on what can be accepted. Returns how many.
pub fn dismiss_empty(cat: &mut Catalog, ids: &[AssetId]) -> Result<u64, AiError> {
    let now = crate::jobs::now_ms();
    Ok(cat.write(|tx| tx.ai().dismiss_empty_aircraft_reads(ids, now))?)
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

/// Queue `ids`, the photos the user picked, to be read (again, if they
/// were before), ahead of the backlog. Each read then waits for review,
/// also that of a photo accepted beside another before it was read: the
/// user asked for these, so the window shows what was read.
pub fn enqueue(cat: &mut Catalog, ids: &[AssetId]) -> Result<u64, AiError> {
    for chunk in ids.chunks(crate::render::ENQUEUE_CHUNK) {
        cat.write(|tx| tx.ai().forget_aircraft_accepted_unread(chunk))?;
    }
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

    fn at(text: &str, x: f32, y: f32, w: f32, h: f32) -> TextLine {
        TextLine {
            text: text.to_owned(),
            score: 0.95,
            quad: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        }
    }

    #[test]
    fn a_distant_aircraft_is_read_again_closer_around_its_airline() {
        // A7202535: Interjet's title read, XA-MBA too small to be found.
        let airlines = Airlines::builtin();
        let title = at("interJet", 2000.0, 1100.0, 226.0, 34.0);
        let [x, y, w, h] =
            closer_look(std::slice::from_ref(&title), &airlines, 4519, 2468).unwrap();
        assert!(x < 2000 && x + w > 2226 + 1000 && y < 1100 && y + h > 1134);
        assert!(w.max(h) as f32 <= 4519.0 * 0.75);
        // Not when a registration was read, nor without an airline.
        let reg = at("XA-MBA", 2900.0, 1100.0, 80.0, 17.0);
        assert_eq!(
            closer_look(&[title.clone(), reg], &airlines, 4519, 2468),
            None
        );
        let other = at("ARE YOU READY", 2000.0, 1100.0, 226.0, 34.0);
        assert_eq!(closer_look(&[other], &airlines, 4519, 2468), None);
        // Nor when the picture was barely shrunk, or the title fills it.
        assert_eq!(closer_look(&[title], &airlines, 2600, 1700), None);
        let big = at("interJet", 1000.0, 1100.0, 1500.0, 220.0);
        assert_eq!(closer_look(&[big], &airlines, 4519, 2468), None);

        // What is read closer is put in the picture's coordinates.
        let img = DynamicImage::new_rgb8(4519, 2468);
        let mut calls = 0;
        let mut read = |i: &DynamicImage| {
            calls += 1;
            Ok(if i.width() == 4519 {
                vec![at("interJet", 2000.0, 1100.0, 226.0, 34.0)]
            } else {
                vec![at("XA-MBA", 10.0, 20.0, 80.0, 17.0)]
            })
        };
        let lines = read_lines(&mut read, &[], &airlines, &img).unwrap();
        assert_eq!(calls, 2);
        let reg = lines.iter().find(|l| l.text == "XA-MBA").unwrap();
        let [x, y, ..] = closer_look(&lines[..1], &airlines, 4519, 2468).unwrap();
        assert_eq!(reg.quad[0], [x as f32 + 10.0, y as f32 + 20.0]);
        assert_eq!(
            marks::candidates(&lines, &airlines).registrations[0].0,
            "XA-MBA"
        );
    }

    #[test]
    fn only_the_text_on_the_aircraft_found_is_read() {
        // A KLM 747 at (1000, 800), 2000×600, beside a terminal sign saying
        // "Air Canada" and the photographer's signature.
        let airlines = Airlines::builtin();
        let img = DynamicImage::new_rgb8(4000, 3000);
        let plane = Plane {
            x: 1000.0,
            y: 800.0,
            w: 2000.0,
            h: 600.0,
            score: 0.9,
        };
        let seen = std::cell::RefCell::new(Vec::new());
        let mut read = |i: &DynamicImage| {
            seen.borrow_mut().push((i.width(), i.height()));
            Ok(if i.width() == 4000 {
                vec![
                    at("Air Canada", 100.0, 100.0, 400.0, 60.0),
                    at("KLM", 1500.0, 1000.0, 200.0, 60.0),
                    at("ahmad el-kays", 3500.0, 2900.0, 400.0, 60.0),
                ]
            } else {
                // The crop: the plane with its margin, at full size.
                vec![
                    at("KLM", 340.0, 248.0, 200.0, 60.0),
                    at("PH-BFA", 1500.0, 300.0, 120.0, 30.0),
                ]
            })
        };
        let lines = read_lines(&mut read, &[plane], &airlines, &img).unwrap();
        let [x, y, w, h] = plane.grown(PLANE_MARGIN, 4000, 3000);
        assert_eq!((x, y, w, h), (840, 752, 2320, 696));
        assert_eq!(*seen.borrow(), [(w, h)], "only the aircraft is read");
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["KLM", "PH-BFA"]);
        // In the picture's coordinates.
        assert_eq!(lines[0].quad[0], [340.0 + x as f32, 248.0 + y as f32]);
        let found = marks::candidates(&lines, &airlines);
        assert_eq!(found.airlines.len(), 1);
        assert_eq!(found.airlines[0].0, "KLM");

        // A big one (most of the picture): the whole picture is read, and
        // only the text inside it kept.
        seen.borrow_mut().clear();
        let big = Plane {
            x: 300.0,
            y: 700.0,
            w: 3000.0,
            h: 900.0,
            score: 0.9,
        };
        let lines = read_lines(&mut read, &[big], &airlines, &img).unwrap();
        // (Then closer around KLM, as no registration was read.)
        assert_eq!(seen.borrow()[0], (4000, 3000));
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["KLM"], "not the sign nor the signature");

        // No aircraft found: the whole picture, as before.
        seen.borrow_mut().clear();
        let lines = read_lines(&mut read, &[], &airlines, &img).unwrap();
        assert_eq!(seen.borrow()[0], (4000, 3000));
        assert!(lines.iter().any(|l| l.text == "Air Canada"));
    }

    #[test]
    fn a_painted_type_said_less_exactly_agrees_with_the_registry() {
        assert!(same_type("Boeing 787 Dreamliner", "Boeing 787-9"));
        assert!(same_type("Boeing 787", "Boeing 787-9"));
        assert!(same_type("Airbus A320neo", "Airbus A320-251N"));
        assert!(same_type("Boeing 777-300ER", "boeing 777-300er"));
        assert!(!same_type("Boeing 777-300ER", "Boeing 777-200LR"));
        assert!(!same_type("Boeing 787 Dreamliner", "Boeing 777-300ER"));
        assert!(!same_type("Airbus A330", "Boeing 787-9"));
        assert!(!same_type("Airbus A321", "Airbus A320-251N"));
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

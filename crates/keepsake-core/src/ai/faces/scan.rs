//! Searching photos for faces in the background (ADR-040, step 2).
//!
//! One `ai.faces` job per photo (not videos). The face worker in the render
//! service loads the face models once, then for each job reads the photo's
//! thumbnail (never the original), finds and describes its faces and
//! stores them ([`crate::catalog::Faces::save_scan`]): faces the user named
//! stay. A photo already searched with these models is left alone. Faces
//! smaller than [`MIN_FACE_PX`] in the thumbnail are not kept: too small
//! to recognise.

use std::sync::Arc;

use image::DynamicImage;

use super::{Face, FaceModels, MODEL_ID};
use crate::ai::embedding::Embedding;
use crate::ai::{analyze, Accel, AiError};
use crate::catalog::{
    AssetId, Catalog, CatalogError, FaceCounts, NewFace, NewJob, Priority, WriteTx,
};
use crate::render::Cache;

/// The job kind: search one photo for faces.
pub const JOB_AI_FACES: &str = "ai.faces";

/// Faces narrower than this in the thumbnail (pixels) are not kept.
pub const MIN_FACE_PX: f32 = 40.0;

/// What the face worker needs: finding and describing the faces in a
/// picture. [`FaceModels`] in the app; a stand-in in tests.
pub trait FaceFinder {
    fn find_and_describe(&mut self, img: &DynamicImage) -> Result<Vec<(Face, Embedding)>, AiError>;
    /// Where it runs.
    fn accel(&self) -> Accel;
}

impl FaceFinder for FaceModels {
    fn find_and_describe(&mut self, img: &DynamicImage) -> Result<Vec<(Face, Embedding)>, AiError> {
        FaceModels::find_and_describe(self, img)
    }

    fn accel(&self) -> Accel {
        FaceModels::accel(self)
    }
}

/// Loads the face worker's [`FaceFinder`]; the argument is "use the
/// graphics card".
pub type FinderLoader =
    Arc<dyn Fn(bool) -> Result<Box<dyn FaceFinder + Send>, AiError> + Send + Sync>;

/// The installed models ([`FaceModels::load_installed`]).
pub fn installed_loader() -> FinderLoader {
    Arc::new(|gpu| Ok(Box::new(FaceModels::load_installed(gpu)?) as Box<dyn FaceFinder + Send>))
}

/// One photo's faces, worked out and ready to store.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    pub asset: AssetId,
    /// Already searched with these models: nothing to store.
    pub already: bool,
    pub width: u32,
    pub height: u32,
    pub faces: Vec<NewFace>,
}

/// The face as stored: box and landmarks in 0–1 units of a `w` × `h`
/// picture, the embedding as int8.
pub fn to_new_face(face: &Face, emb: &Embedding, w: u32, h: u32) -> NewFace {
    let (x, y, bw, bh) = face.normalised(w, h);
    let (pw, ph) = (w.max(1) as f32, h.max(1) as f32);
    let (embedding, emb_scale) = emb.quantize();
    NewFace {
        x,
        y,
        w: bw.max(f32::MIN_POSITIVE),
        h: bh.max(f32::MIN_POSITIVE),
        landmarks: face.landmarks.map(|[lx, ly]| [lx / pw, ly / ph]),
        score: face.score,
        embedding,
        emb_scale,
    }
}

/// Find and describe `asset`'s faces from its thumbnail.
pub fn prepare(
    cat: &mut Catalog,
    cache: &Cache,
    finder: &mut dyn FaceFinder,
    asset: AssetId,
) -> Result<Prepared, AiError> {
    if let Some(s) = cat.read(|tx| tx.faces().scan(asset))? {
        if s.model == MODEL_ID {
            return Ok(Prepared {
                asset,
                already: true,
                width: s.width,
                height: s.height,
                faces: Vec::new(),
            });
        }
    }
    let img = analyze::thumbnail(cat, cache, asset)?;
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Err(AiError::Image("empty thumbnail".into()));
    }
    let faces = finder
        .find_and_describe(&img)?
        .iter()
        .filter(|(f, _)| f.w >= MIN_FACE_PX)
        .map(|(f, e)| to_new_face(f, e, w, h))
        .collect();
    Ok(Prepared {
        asset,
        already: false,
        width: w,
        height: h,
        faces,
    })
}

/// Store a prepared photo's faces. Returns how many were added.
pub fn save(tx: &WriteTx<'_>, p: &Prepared, now_ms: i64) -> Result<u64, CatalogError> {
    if p.already {
        return Ok(0);
    }
    tx.faces()
        .save_scan(p.asset, MODEL_ID, p.width, p.height, &p.faces, now_ms)
}

/// Queue these photos to be searched for faces (photos already searched
/// are skipped by the job). `again`: also queue ones whose job finished.
pub fn enqueue(cat: &mut Catalog, assets: &[AssetId], again: bool) -> Result<u64, AiError> {
    enqueue_at(cat, assets, Priority::Recent, again)
}

/// Search one photo now, ahead of the rest (Name and find on a photo not
/// searched yet): its thumbnail first if it has none.
pub fn search_now(cat: &mut Catalog, asset: AssetId) -> Result<(), AiError> {
    crate::render::enqueue(cat, &[asset], Priority::Visible, false)?;
    enqueue_at(cat, &[asset], Priority::Visible, true)?;
    Ok(())
}

fn enqueue_at(
    cat: &mut Catalog,
    assets: &[AssetId],
    priority: Priority,
    again: bool,
) -> Result<u64, AiError> {
    let now = crate::jobs::now_ms();
    let size = crate::render::ENQUEUE_CHUNK;
    let mut n = 0;
    for (c, chunk) in assets.chunks(size).enumerate() {
        n += cat.write(|tx| {
            let jobs: Vec<NewJob<'_>> = chunk
                .iter()
                .enumerate()
                .map(|(i, id)| NewJob {
                    kind: JOB_AI_FACES,
                    asset_id: *id,
                    rank: 3,
                    sort_key: i64::try_from(c * size + i).unwrap_or(i64::MAX),
                })
                .collect();
            tx.jobs().enqueue(&jobs, priority, now, again)
        })?;
    }
    Ok(n)
}

/// Queue every photo not yet searched with these models, newest first,
/// and the thumbnails they still need (those made stay made), so both are
/// done in one run. Returns how many photos were queued.
pub fn enqueue_unscanned(cat: &mut Catalog) -> Result<u64, AiError> {
    let ids = cat.read(|tx| tx.faces().unscanned_photos(MODEL_ID))?;
    crate::render::enqueue(cat, &ids, Priority::Recent, false)?;
    enqueue(cat, &ids, true)
}

/// How far finding faces has got.
pub fn status(cat: &Catalog) -> Result<FaceCounts, AiError> {
    Ok(cat.read(|tx| tx.faces().counts(MODEL_ID, JOB_AI_FACES))?)
}

/// Stop: drop the queued face jobs (one running finishes). What was found
/// stays. Returns how many were dropped.
pub fn stop(cat: &mut Catalog) -> Result<u64, AiError> {
    Ok(cat.write(|tx| tx.jobs().remove_all_queued(JOB_AI_FACES))?)
}

/// Delete all face data: every face, scan, person and decision, the queued
/// face jobs, and the face regions and `People|<name>` keywords it wrote
/// to photos (their sidecars are written again). Originals and the face
/// models are not touched. Returns how many faces there were.
pub fn delete_all(lib: &crate::library::Library, cat: &mut Catalog) -> Result<u64, AiError> {
    stop(cat)?;
    // First every name comes off (while the people are known, so their
    // keywords and regions are found), then the tables are emptied.
    let photos = cat.read(|tx| tx.faces().photos_with_decisions())?;
    cat.write(|tx| tx.faces().forget_decisions())?;
    let mut svc = crate::metadata::MetadataService::new(lib, cat);
    for p in &photos {
        svc.sync_faces(*p).map_err(|e| match e {
            crate::library::LibraryError::Catalog(e) => AiError::Catalog(e),
            e => AiError::Image(e.to_string()),
        })?;
    }
    Ok(cat.write(|tx| tx.faces().delete_all())?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_face_is_stored_in_picture_units() {
        let face = Face {
            x: 100.0,
            y: 50.0,
            w: 200.0,
            h: 250.0,
            landmarks: [[150.0, 120.0]; 5],
            score: 0.95,
        };
        let emb = Embedding::new(vec![0.5, -0.5, 0.5, -0.5]).unwrap();
        let n = to_new_face(&face, &emb, 1000, 500);
        assert!((n.x - 0.1).abs() < 1e-6 && (n.y - 0.1).abs() < 1e-6);
        assert!((n.w - 0.2).abs() < 1e-6 && (n.h - 0.5).abs() < 1e-6);
        assert!((n.landmarks[0][0] - 0.15).abs() < 1e-6);
        assert!((n.landmarks[0][1] - 0.24).abs() < 1e-6);
        assert_eq!(n.embedding, vec![127, -127, 127, -127]);
        let back = Embedding::from_quantized(&n.embedding, n.emb_scale).unwrap();
        assert!(back.cosine(&emb) > 0.999);
    }
}

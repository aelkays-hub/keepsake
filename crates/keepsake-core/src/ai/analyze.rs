//! The `ai.analyze` job: one asset's thumbnail in; its embedding,
//! difference hash and AI tags out.
//!
//! It reads the 1024 px thumbnail from the render cache (else the 256 px
//! one), never the original, so a network drive is not read again. Without
//! a thumbnail it fails with [`AiError::NoThumbnail`]; the worker then
//! queues the thumbnail and tries later. A video uses its thumbnail frame.
//!
//! The results go to the catalog only (`ai_analysis`, `asset_tags` with
//! source `ai`): they are a cache that can be made again, so the job does
//! not rewrite the asset's XMP sidecars. AI tags the user removed are never
//! given again.

use image::DynamicImage;

use super::{dhash, AiError, ImageModel, LabelSet, JOB_AI, MODEL_ID};
use crate::catalog::{
    AiAnalysis, AssetId, AssetTag, Catalog, CatalogError, NewJob, Priority, TagSource,
};
use crate::render::{Cache, Variant};

/// What one job did.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzeOutcome {
    /// AI tags given, most likely first.
    pub tags: Vec<(String, f64)>,
}

/// The asset's thumbnail, decoded.
pub fn thumbnail(
    cat: &mut Catalog,
    cache: &Cache,
    asset: AssetId,
) -> Result<DynamicImage, AiError> {
    let path = match cache.lookup(cat, asset, Variant::Thumb1024)? {
        Some(p) => p,
        None => cache
            .lookup(cat, asset, Variant::Thumb256)?
            .ok_or(AiError::NoThumbnail)?,
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        // Evicted between the lookup and the read.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(AiError::NoThumbnail),
        Err(e) => return Err(e.into()),
    };
    Ok(image::load_from_memory(&bytes)?)
}

/// Analyse an image and store the results for `asset`.
pub fn store_analysis(
    cat: &mut Catalog,
    model: &mut ImageModel,
    labels: &LabelSet,
    asset: AssetId,
    img: &DynamicImage,
) -> Result<AnalyzeOutcome, AiError> {
    let hash = dhash::dhash(img)?;
    let emb = model.embed(img)?;
    store_results(cat, labels, asset, &emb, hash)
}

/// Store an embedding and hash for `asset` and give its AI tags.
pub fn store_results(
    cat: &mut Catalog,
    labels: &LabelSet,
    asset: AssetId,
    emb: &super::Embedding,
    hash: u64,
) -> Result<AnalyzeOutcome, AiError> {
    let (embedding, emb_scale) = emb.quantize();
    let now = crate::jobs::now_ms();
    let rejected = cat.read(|tx| tx.ai().rejections(asset))?;
    let tags = labels.tags_for(emb, &rejected);
    let rows: Vec<AssetTag> = tags
        .iter()
        .map(|(p, c)| AssetTag::ai(p.clone(), *c))
        .collect();
    cat.write(|tx| {
        if tx.assets().get(asset)?.is_none() {
            return Ok(());
        }
        tx.ai().put_analysis(&AiAnalysis {
            asset_id: asset,
            model: MODEL_ID.to_owned(),
            embedding: embedding.clone(),
            emb_scale,
            dhash: hash,
            made_ms: now,
        })?;
        tx.tags().set_for_asset(asset, TagSource::Ai, &rows)?;
        tx.tags().prune_unused()?;
        Ok::<_, CatalogError>(())
    })?;
    Ok(AnalyzeOutcome {
        tags: tags
            .into_iter()
            .map(|(p, c)| (p.to_lightroom(), c))
            .collect(),
    })
}

/// Run one `ai.analyze` job.
pub fn analyze_asset(
    cat: &mut Catalog,
    cache: &Cache,
    model: &mut ImageModel,
    labels: &LabelSet,
    asset: AssetId,
) -> Result<AnalyzeOutcome, AiError> {
    let img = thumbnail(cat, cache, asset)?;
    store_analysis(cat, model, labels, asset, &img)
}

/// Give AI tags again from the stored embeddings (after the tag list or
/// thresholds changed, or a rejection was undone): no model needed.
/// Returns how many assets were re-tagged.
pub fn retag_all(cat: &mut Catalog, labels: &LabelSet) -> Result<u64, AiError> {
    let ids = cat.read(|tx| tx.ai().analysed(MODEL_ID))?;
    let mut n = 0;
    for chunk in ids.chunks(200) {
        cat.write(|tx| {
            for id in chunk {
                let Some(a) = tx.ai().analysis(*id)? else {
                    continue;
                };
                let Ok(emb) = super::Embedding::from_quantized(&a.embedding, a.emb_scale) else {
                    continue;
                };
                let rejected = tx.ai().rejections(*id)?;
                let rows: Vec<AssetTag> = labels
                    .tags_for(&emb, &rejected)
                    .into_iter()
                    .map(|(p, c)| AssetTag::ai(p, c))
                    .collect();
                tx.tags().set_for_asset(*id, TagSource::Ai, &rows)?;
                n += 1;
            }
            tx.tags().prune_unused()?;
            Ok::<_, CatalogError>(())
        })?;
    }
    Ok(n)
}

/// Queue `ai.analyze` for every asset not yet analysed with the current
/// model (backlog priority, newest capture first). Returns how many were
/// added.
pub fn enqueue_missing(cat: &mut Catalog) -> Result<u64, CatalogError> {
    let ids = cat.read(|tx| tx.ai().without_analysis(MODEL_ID))?;
    enqueue(cat, &ids, Priority::Backlog, false)
}

/// Queue `ai.analyze` for these assets. `again`: also re-run finished ones.
pub fn enqueue(
    cat: &mut Catalog,
    assets: &[AssetId],
    priority: Priority,
    again: bool,
) -> Result<u64, CatalogError> {
    let now = crate::jobs::now_ms();
    let mut n = 0;
    let size = crate::render::ENQUEUE_CHUNK;
    for (c, chunk) in assets.chunks(size).enumerate() {
        n += cat.write(|tx| {
            let mut jobs = Vec::with_capacity(chunk.len());
            for (i, id) in chunk.iter().enumerate() {
                jobs.push(NewJob {
                    kind: JOB_AI,
                    asset_id: *id,
                    rank: 3,
                    // Keep the given order (newest first from the query).
                    sort_key: i64::try_from(c * size + i).unwrap_or(i64::MAX),
                });
            }
            tx.jobs().enqueue(&jobs, priority, now, again)
        })?;
    }
    Ok(n)
}

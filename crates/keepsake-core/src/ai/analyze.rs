//! The AI jobs: `ai.analyze` ("Find duplicates") takes one asset's
//! thumbnail and stores its embedding and difference hash; `ai.tag` ("Tag
//! photos") gives its AI tags, from the stored embedding when there is one
//! (else it analyses first). They are separate: one never does the other.
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

use super::{dhash, AiError, ImageModel, LabelSet, JOB_AI, JOB_AI_TAG, MODEL_ID};
use crate::catalog::{
    AiAnalysis, AssetId, AssetTag, Catalog, CatalogError, NewJob, Priority, RangeItem, TagSource,
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

/// Analyse an image and store the results for `asset`; with `labels`, give
/// its AI tags too.
pub fn store_analysis(
    cat: &mut Catalog,
    model: &mut ImageModel,
    labels: Option<&LabelSet>,
    asset: AssetId,
    img: &DynamicImage,
) -> Result<AnalyzeOutcome, AiError> {
    let hash = dhash::dhash(img)?;
    let emb = model.embed(img)?;
    store_results(cat, labels, asset, &emb, hash)
}

/// Store an embedding and hash for `asset`; with `labels`, give its AI
/// tags from it (without, any tags it already has stay as they are).
pub fn store_results(
    cat: &mut Catalog,
    labels: Option<&LabelSet>,
    asset: AssetId,
    emb: &super::Embedding,
    hash: u64,
) -> Result<AnalyzeOutcome, AiError> {
    let (embedding, emb_scale) = emb.quantize();
    let now = crate::jobs::now_ms();
    let rejected = cat.read(|tx| tx.ai().rejections(asset))?;
    let tagged_with = cat
        .read(|tx| tx.ai().analysis(asset))?
        .filter(|a| a.model == MODEL_ID)
        .map_or(0, |a| a.tag_list);
    let tags = labels.map(|l| l.tags_for(emb, &rejected));
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
            tag_list: if tags.is_some() {
                super::labels::LIST_VERSION
            } else {
                tagged_with
            },
        })?;
        if let Some(tags) = &tags {
            let rows: Vec<AssetTag> = tags
                .iter()
                .map(|(p, c)| AssetTag::ai(p.clone(), *c))
                .collect();
            tx.tags().set_for_asset(asset, TagSource::Ai, &rows)?;
            tx.tags().prune_unused()?;
        }
        Ok::<_, CatalogError>(())
    })?;
    Ok(AnalyzeOutcome {
        tags: tags
            .unwrap_or_default()
            .into_iter()
            .map(|(p, c)| (p.to_lightroom(), c))
            .collect(),
    })
}

/// What a date range is analysed for. They are separate: finding
/// duplicates gives no tags, and tagging leaves duplicates alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// "Find duplicates": the range's RAW photos, analysed (no tags).
    Duplicates,
    /// "Tag photos": every item of the range gets AI tags.
    Tags,
}

impl Purpose {
    /// Its job kind.
    pub fn job(self) -> &'static str {
        match self {
            Purpose::Duplicates => JOB_AI,
            Purpose::Tags => JOB_AI_TAG,
        }
    }

    pub fn of_job(kind: &str) -> Option<Self> {
        match kind {
            JOB_AI => Some(Purpose::Duplicates),
            JOB_AI_TAG => Some(Purpose::Tags),
            _ => None,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "duplicates" => Some(Purpose::Duplicates),
            "tags" => Some(Purpose::Tags),
            _ => None,
        }
    }
}

/// Run one AI job. An item already analysed is not analysed again: to tag
/// it, its stored embedding is scored (no thumbnail, no model).
pub fn analyze_asset(
    cat: &mut Catalog,
    cache: &Cache,
    model: &mut ImageModel,
    labels: &LabelSet,
    asset: AssetId,
    purpose: Purpose,
) -> Result<AnalyzeOutcome, AiError> {
    let stored = cat
        .read(|tx| tx.ai().analysis(asset))?
        .filter(|a| a.model == MODEL_ID);
    if let Some(a) = stored {
        if purpose == Purpose::Duplicates {
            return Ok(AnalyzeOutcome { tags: Vec::new() });
        }
        let emb = super::Embedding::from_quantized(&a.embedding, a.emb_scale)?;
        return store_results(cat, Some(labels), asset, &emb, a.dhash);
    }
    let img = thumbnail(cat, cache, asset)?;
    let labels = (purpose == Purpose::Tags).then_some(labels);
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
                tx.ai().set_tag_list(*id, super::labels::LIST_VERSION)?;
                n += 1;
            }
            tx.tags().prune_unused()?;
            Ok::<_, CatalogError>(())
        })?;
    }
    Ok(n)
}

/// How far a requested date range has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
pub struct RangeStatus {
    /// Items the purpose covers in the range: its RAW photos for
    /// duplicates, every item for tags.
    pub items: u64,
    /// Of those, done (analysed for duplicates; tagged for tags).
    pub analysed: u64,
    /// Jobs of the purpose still to run for the range.
    pub pending: u64,
}

fn valid_day(d: &str) -> bool {
    let b = d.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

fn check_range(from: &str, to: &str) -> Result<(), AiError> {
    if !valid_day(from) || !valid_day(to) || from > to {
        return Err(AiError::Inference(format!(
            "not a date range (YYYY-MM-DD): {from} to {to}"
        )));
    }
    Ok(())
}

fn covers(purpose: Purpose, it: &RangeItem) -> bool {
    purpose == Purpose::Tags || it.raw
}

fn done(purpose: Purpose, it: &RangeItem) -> bool {
    match purpose {
        Purpose::Duplicates => it.analysed,
        Purpose::Tags => it.tag_list >= super::labels::LIST_VERSION,
    }
}

/// Do `purpose` for the items captured from `from` to `to` (`YYYY-MM-DD`,
/// local, inclusive): their jobs are queued ahead of other AI work, and
/// the thumbnails of items not yet analysed ahead of the backlog. Items
/// already done are left alone. Returns the range's status.
pub fn enqueue_range(
    cat: &mut Catalog,
    from: &str,
    to: &str,
    purpose: Purpose,
) -> Result<RangeStatus, AiError> {
    check_range(from, to)?;
    let all = cat.read(|tx| tx.ai().in_dates(from, to, MODEL_ID))?;
    let todo: Vec<&RangeItem> = all
        .iter()
        .filter(|it| covers(purpose, it) && !done(purpose, it))
        .collect();
    let need_thumbs: Vec<AssetId> = todo
        .iter()
        .filter(|it| !it.analysed)
        .map(|it| it.id)
        .collect();
    let ids: Vec<AssetId> = todo.iter().map(|it| it.id).collect();
    // Thumbnails first: their jobs move ahead of the backlog (finished ones
    // stay finished; a thumbnail evicted since is queued by the job itself).
    // No video proxies: the AI does not need them.
    enqueue_kind(cat, &need_thumbs, crate::render::JOB_STILLS, 0, false)?;
    enqueue_for(cat, &ids, purpose, Priority::Recent, true)?;
    range_status(cat, from, to, purpose)
}

/// How far `purpose` has got for the range.
pub fn range_status(
    cat: &Catalog,
    from: &str,
    to: &str,
    purpose: Purpose,
) -> Result<RangeStatus, AiError> {
    check_range(from, to)?;
    let all = cat.read(|tx| tx.ai().in_dates(from, to, MODEL_ID))?;
    let (pending, _) = cat.read(|tx| tx.jobs().in_dates(purpose.job(), from, to))?;
    let mine: Vec<&RangeItem> = all.iter().filter(|it| covers(purpose, it)).collect();
    Ok(RangeStatus {
        items: mine.len() as u64,
        analysed: mine.iter().filter(|it| done(purpose, it)).count() as u64,
        pending,
    })
}

/// Queue `purpose`'s jobs for these assets. `again`: also re-run finished
/// ones.
pub fn enqueue_for(
    cat: &mut Catalog,
    assets: &[AssetId],
    purpose: Purpose,
    priority: Priority,
    again: bool,
) -> Result<u64, CatalogError> {
    enqueue_kind_at(cat, assets, purpose.job(), 3, priority, again)
}

/// Queue duplicate analysis (`ai.analyze`) for these assets.
pub fn enqueue(
    cat: &mut Catalog,
    assets: &[AssetId],
    priority: Priority,
    again: bool,
) -> Result<u64, CatalogError> {
    enqueue_for(cat, assets, Purpose::Duplicates, priority, again)
}

fn enqueue_kind(
    cat: &mut Catalog,
    assets: &[AssetId],
    kind: &'static str,
    rank: i64,
    again: bool,
) -> Result<u64, CatalogError> {
    enqueue_kind_at(cat, assets, kind, rank, Priority::Recent, again)
}

fn enqueue_kind_at(
    cat: &mut Catalog,
    assets: &[AssetId],
    kind: &'static str,
    rank: i64,
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
                    kind,
                    asset_id: *id,
                    rank,
                    // Keep the given order (newest first from the query).
                    sort_key: i64::try_from(c * size + i).unwrap_or(i64::MAX),
                });
            }
            tx.jobs().enqueue(&jobs, priority, now, again)
        })?;
    }
    Ok(n)
}

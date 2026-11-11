//! Edit suggestions (ADR-042): for one photo the user picks, what to
//! improve and how to do it in Photoshop and in Topaz Photo.
//!
//! - [`measure`]: Keepsake measures the photo's 1024 px thumbnail on this
//!   computer (brightness, blown highlights, black shadows, contrast, a
//!   colour cast, noise, softness, a tilted horizon). Free and instant.
//! - [`rules`]: fixed rules turn the measurements into suggestions.
//! - [`ask`]: when the user asks, the thumbnail and the measurements go to
//!   Claude, ChatGPT or Gemini (the keys and models of Tag with AI), which
//!   can see what the photo is of. One job per photo and provider
//!   ([`SUGGEST_JOBS`]), run by the render service's cloud worker.
//!
//! The measurements and the AI's answer are kept in the catalog
//! (`edit_suggestions`), so looking again is instant and costs nothing.
//! The original is not read, and nothing is written next to it.

pub mod ask;
pub mod measure;
pub mod rules;

use serde::{Deserialize, Serialize};

use super::cloud::Provider;
use super::{analyze, AiError};
use crate::catalog::{AssetId, Catalog, CatalogError, JobState, NewJob, Priority};
use crate::render::Cache;

pub use ask::{Adviser, CloudAdviser};
pub use measure::{Cast, Measures};
pub use rules::Measured;

/// The job kind: ask Claude for one asset's edit suggestions.
pub const JOB_SUGGEST_CLAUDE: &str = "ai.suggest.claude";
/// The job kind: ask ChatGPT.
pub const JOB_SUGGEST_OPENAI: &str = "ai.suggest.openai";
/// The job kind: ask Gemini.
pub const JOB_SUGGEST_GEMINI: &str = "ai.suggest.gemini";
/// Every job kind that asks an AI for edit suggestions.
pub const SUGGEST_JOBS: [&str; 3] = [JOB_SUGGEST_CLAUDE, JOB_SUGGEST_OPENAI, JOB_SUGGEST_GEMINI];

/// The job kind that asks `provider`.
pub fn job_kind(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => JOB_SUGGEST_CLAUDE,
        Provider::OpenAi => JOB_SUGGEST_OPENAI,
        Provider::Gemini => JOB_SUGGEST_GEMINI,
    }
}

/// The provider a job kind asks.
pub fn provider_of_job(kind: &str) -> Option<Provider> {
    Provider::ALL.into_iter().find(|p| job_kind(*p) == kind)
}

/// One edit: what to do, why, and how in each program. `topaz` (or
/// `photoshop`) is empty when that program has no tool for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    pub title: String,
    pub why: String,
    pub photoshop: String,
    pub topaz: String,
}

/// What an AI advised.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiAdvice {
    /// The provider's id (`claude`).
    pub provider: String,
    /// The model that answered.
    pub model: String,
    pub summary: String,
    pub suggestions: Vec<Suggestion>,
    pub made_ms: i64,
}

/// A photo's edit suggestions, as kept.
#[derive(Debug, Clone, PartialEq)]
pub struct Advice {
    pub measures: Measures,
    pub made_ms: i64,
    /// What an AI advised, once asked.
    pub ai: Option<AiAdvice>,
}

impl Advice {
    /// The suggestions the rules give for what was measured.
    pub fn suggestions(&self) -> Vec<Suggestion> {
        rules::suggest(&self.measures)
    }

    /// What was measured, a line each.
    pub fn measured(&self) -> Vec<Measured> {
        rules::describe(&self.measures)
    }

    /// One sentence on what the rules found.
    pub fn summary(&self) -> String {
        rules::summary(self.suggestions().len())
    }
}

fn json_err(e: serde_json::Error) -> AiError {
    AiError::Image(format!("edit suggestions could not be stored: {e}"))
}

/// The asset's kept suggestions. Measurements this version cannot read
/// (kept by another version) count as none; so does an unreadable answer.
pub fn stored(cat: &Catalog, asset: AssetId) -> Result<Option<Advice>, AiError> {
    let Some(row) = cat.read(|tx| tx.ai().edit_advice(asset))? else {
        return Ok(None);
    };
    let Ok(measures) = serde_json::from_str::<Measures>(&row.measures) else {
        return Ok(None);
    };
    Ok(Some(Advice {
        measures,
        made_ms: row.made_ms,
        ai: row.ai.and_then(|a| serde_json::from_str(&a).ok()),
    }))
}

/// Measure the asset's thumbnail now and keep the result. An AI's answer
/// to the old measurements is dropped. [`AiError::NoThumbnail`] if there is
/// no thumbnail yet.
pub fn measure_asset(cat: &mut Catalog, cache: &Cache, asset: AssetId) -> Result<Advice, AiError> {
    let img = analyze::thumbnail(cat, cache, asset)?;
    let measures = measure::measure(&img)?;
    drop(img);
    let made_ms = crate::jobs::now_ms();
    let json = serde_json::to_string(&measures).map_err(json_err)?;
    cat.write(|tx| tx.ai().put_edit_measures(asset, &json, made_ms, false))?;
    Ok(Advice {
        measures,
        made_ms,
        ai: None,
    })
}

/// The asset's suggestions: those kept, or (with `again`, or when none are
/// kept) measured now.
pub fn advice(
    cat: &mut Catalog,
    cache: &Cache,
    asset: AssetId,
    again: bool,
) -> Result<Advice, AiError> {
    if !again {
        if let Some(a) = stored(cat, asset)? {
            return Ok(a);
        }
    }
    measure_asset(cat, cache, asset)
}

/// One job: send the asset's thumbnail and measurements to `provider`
/// through `adviser` and keep what it advises.
pub fn ask_asset(
    cat: &mut Catalog,
    cache: &Cache,
    adviser: &dyn Adviser,
    provider: Provider,
    asset: AssetId,
) -> Result<AiAdvice, AiError> {
    let img = analyze::thumbnail(cat, cache, asset)?;
    let measures = match stored(cat, asset)? {
        Some(a) => a.measures,
        None => {
            let m = measure::measure(&img)?;
            let json = serde_json::to_string(&m).map_err(json_err)?;
            let now = crate::jobs::now_ms();
            cat.write(|tx| tx.ai().put_edit_measures(asset, &json, now, false))?;
            m
        }
    };
    let jpeg = crate::render::still::encode_jpeg(&img)?;
    drop(img);
    let got = adviser.advise(provider, &jpeg, &measures)?;
    let json = serde_json::to_string(&got).map_err(json_err)?;
    cat.write(|tx| tx.ai().put_edit_ai(asset, &json))?;
    tracing::info!(
        asset = %asset, provider = provider.id(), model = %got.model,
        "edit suggestions: {}", got.suggestions.len()
    );
    Ok(got)
}

/// Queue `asset` to be sent to `provider` (again, if it was before). A
/// question still waiting for another provider is dropped: one at a time.
pub fn enqueue_ask(cat: &mut Catalog, provider: Provider, asset: AssetId) -> Result<(), AiError> {
    let now = crate::jobs::now_ms();
    let job = NewJob {
        kind: job_kind(provider),
        asset_id: asset,
        rank: 0,
        sort_key: 0,
    };
    cat.write(|tx| {
        for other in Provider::ALL.into_iter().filter(|p| *p != provider) {
            tx.jobs().remove_queued_for(job_kind(other), &[asset])?;
        }
        tx.jobs().enqueue(&[job], Priority::Recent, now, true)?;
        Ok::<_, CatalogError>(())
    })?;
    Ok(())
}

/// Stop: a question not sent yet is not sent.
pub fn stop_asking(cat: &mut Catalog, asset: AssetId) -> Result<(), AiError> {
    cat.write(|tx| {
        for p in Provider::ALL {
            tx.jobs().remove_queued_for(job_kind(p), &[asset])?;
        }
        Ok::<_, CatalogError>(())
    })?;
    Ok(())
}

/// A question to an AI that is on its way, or that failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asking {
    pub provider: Provider,
    /// Still to be answered (queued, waiting for a thumbnail, being sent or
    /// to be tried again); else it failed for good.
    pub pending: bool,
    /// Why it failed, or why it is being tried again.
    pub problem: Option<String>,
}

/// The asset's question to an AI, if one is on its way or failed after the
/// answer that is kept (`answered_ms`) was given.
pub fn asking(
    cat: &Catalog,
    asset: AssetId,
    answered_ms: Option<i64>,
) -> Result<Option<Asking>, AiError> {
    let mut failed = None;
    let pending = cat.read(|tx| {
        for provider in Provider::ALL {
            let Some(job) = tx.jobs().find(job_kind(provider), asset)? else {
                continue;
            };
            match job.state {
                JobState::Done => {}
                JobState::Failed => {
                    if answered_ms.is_none_or(|at| job.enqueued_ms >= at) {
                        failed = Some(Asking {
                            provider,
                            pending: false,
                            problem: job.error,
                        });
                    }
                }
                _ => {
                    return Ok(Some(Asking {
                        provider,
                        pending: true,
                        problem: job.error.filter(|e| !e.starts_with("waiting")),
                    }))
                }
            }
        }
        Ok::<_, CatalogError>(None)
    })?;
    Ok(pending.or(failed))
}

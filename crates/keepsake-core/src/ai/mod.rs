//! On-device AI (ADR-035): content tags and same-day duplicates.
//!
//! - [`model`]: the SigLIP 2 image encoder, run with ONNX Runtime (loaded at
//!   run time from `vendor/` or the app's resources, never linked), on the
//!   GPU where it can (DirectML on Windows, CoreML on macOS), else the CPU.
//! - [`install`]: where the model and runtime are, and downloading the model
//!   into the user's cache folder (checked by BLAKE3).
//! - [`embedding`]: unit image vectors, stored as int8.
//! - [`dhash`]: a 64-bit difference hash, for exact and near-exact copies.
//! - [`labels`]: the built-in tag list and its text vectors (made once by
//!   `scripts/make-label-vectors.py`), and scoring an image against it.
//! - [`analyze`]: the `ai.analyze` job: one asset's thumbnail in, its
//!   embedding, hash and AI tags out. Originals are never read or changed.
//! - [`duplicates`]: groups of near-identical items within one day.
//!
//! The user's decisions (removed AI tags, keepers) are kept by
//! [`crate::metadata::MetadataService`], in the catalog and XMP sidecars.

pub mod analyze;
pub mod dhash;
pub mod duplicates;
pub mod embedding;
pub mod install;
pub mod labels;
pub mod model;

use std::path::PathBuf;

use thiserror::Error;

use crate::catalog::CatalogError;
use crate::render::RenderError;

pub use analyze::{
    analyze_asset, enqueue_range, enqueue_target, range_status, stop_target, target_status,
    AnalyzeOutcome, Purpose, Target,
};
pub use duplicates::{DuplicateGroup, DuplicateMember, DuplicateSummary};
pub use embedding::Embedding;
pub use labels::LabelSet;
pub use model::{Accel, ImageModel};

/// The job kind that analyses one asset to find duplicates (no tags).
pub const JOB_AI: &str = "ai.analyze";
/// The job kind that gives one asset its AI tags.
pub const JOB_AI_TAG: &str = "ai.tag";

/// Model id stored with each analysis. Bump the version after `@` when the
/// model file or preprocessing changes: every asset is then analysed again.
pub const MODEL_ID: &str = "siglip2-b16-224@1";

/// Errors from the AI features.
#[derive(Debug, Error)]
pub enum AiError {
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The model file is not installed.
    #[error("the AI model is not installed (looked for {0})")]
    ModelMissing(PathBuf),
    /// ONNX Runtime is missing or could not be loaded.
    #[error("ONNX Runtime could not be loaded: {0}")]
    Runtime(String),
    #[error("inference failed: {0}")]
    Inference(String),
    #[error("image: {0}")]
    Image(String),
    /// The asset has no thumbnail yet (the job waits for it).
    #[error("no thumbnail yet")]
    NoThumbnail,
    #[error("label vectors: {0}")]
    Labels(String),
    #[error("model download failed: {0}")]
    Download(String),
    #[error("cancelled")]
    Cancelled,
}

impl AiError {
    pub fn is_busy(&self) -> bool {
        match self {
            AiError::Catalog(e) => e.is_busy(),
            AiError::Render(e) => e.is_busy(),
            _ => false,
        }
    }
}

impl From<image::ImageError> for AiError {
    fn from(e: image::ImageError) -> Self {
        AiError::Image(e.to_string())
    }
}

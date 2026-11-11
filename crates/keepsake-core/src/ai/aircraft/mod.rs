//! Reading aircraft, on this computer (ADR-041): the registration and the
//! airline painted on a plane, and its type.
//!
//! - [`ocr`]: the text in a picture, found and read by PP-OCR's two models
//!   with the ONNX Runtime the other AI features use.
//! - [`marks`]: what that text says: which words are a registration, which
//!   an airline (`airlines.tsv`), which a type spelt out by the door.
//! - [`types`]: the type a picture looks like, from its SigLIP 2 embedding
//!   and a list of types (`aircraft-types.tsv`), when no type is painted.
//! - [`install`]: where the text models are, and downloading them.
//! - [`scan`]: the `ai.aircraft` job: one item's picture in, its
//!   `Aircraft|…` keywords out. Originals are only read.

pub mod install;
pub mod marks;
pub mod ocr;
pub mod scan;
pub mod types;

pub use scan::{JOB_AI_AIRCRAFT, SOURCE, SOURCE_NAME};

/// The reader's id, logged with each item read. Bump the version after
/// `@` when the models, the lists or the rules change.
pub const MODEL_ID: &str = "ppocr-v5-server-det+en-rec@1";

//! The two text models (ADR-041): where they are, and installing them.
//!
//! Like the other models ([`crate::ai::install`]), they are never committed
//! or bundled. Turning on Read aircraft downloads them once into the
//! user's cache folder, pinned to a revision and kept only if the BLAKE3
//! matches. Developers can put them in `vendor/models/` instead (setup
//! scripts, `-WithAi` / `--with-ai`).

use std::sync::atomic::AtomicBool;

use crate::ai::install::{download_checked, remove_model_at, ModelFile};
use crate::ai::AiError;

/// Finds text: PP-OCRv5 server detector (Apache-2.0), ONNX from Hugging
/// Face `monkt/paddleocr-onnx` at revision `7b02d0a3`.
pub const DETECTOR: ModelFile = ModelFile {
    dir: "ppocr-v5-server-det",
    file: "det.onnx",
    url: "https://huggingface.co/monkt/paddleocr-onnx/resolve/7b02d0a30a07ba2b92ad1ff5a8941ae2c633de65/detection/v5/det.onnx",
    bytes: 88_030_804,
    blake3: "17bf3a843853f825dfc41f23f1cc2d8c471136cf5a505fe3a949887e0ec2e712",
    env: "KEEPSAKE_TEXT_DETECTOR",
};

/// Reads text: PP-OCRv5 English mobile recogniser (Apache-2.0), same
/// repository and revision.
pub const RECOGNIZER: ModelFile = ModelFile {
    dir: "ppocr-v5-en-rec",
    file: "rec.onnx",
    url: "https://huggingface.co/monkt/paddleocr-onnx/resolve/7b02d0a30a07ba2b92ad1ff5a8941ae2c633de65/languages/english/rec.onnx",
    bytes: 7_830_888,
    blake3: "066e68e1174837dbcc39f72b1aeedf241415da27817f589d1975189410917075",
    env: "KEEPSAKE_TEXT_RECOGNIZER",
};

/// Both, in the order they are downloaded.
pub const MODELS: [ModelFile; 2] = [DETECTOR, RECOGNIZER];

/// Bytes to download for both.
pub const TOTAL_BYTES: u64 = DETECTOR.bytes + RECOGNIZER.bytes;

/// Both text models are installed.
pub fn installed() -> bool {
    MODELS.iter().all(|m| m.find().is_some())
}

/// Download both into the user's cache folder. `progress` gets `(bytes so
/// far, total)` for the two together. One already there and matching is
/// kept as is.
pub fn install(cancel: &AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<(), AiError> {
    let mut before = 0;
    for m in MODELS {
        let target = m
            .user_path()
            .ok_or_else(|| AiError::Download("no cache folder on this computer".into()))?;
        download_checked(m.url, &target, m.bytes, m.blake3, cancel, &mut |got, _| {
            progress(before + got.min(m.bytes), TOTAL_BYTES)
        })?;
        before += m.bytes;
    }
    Ok(())
}

/// Remove both from the user's cache folder. `true` if one was there.
pub fn remove() -> Result<bool, AiError> {
    let mut any = false;
    for m in MODELS {
        if let Some(p) = m.user_path() {
            any |= remove_model_at(&p)?;
        }
    }
    Ok(any)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pins_are_whole() {
        for m in MODELS {
            assert_eq!(m.blake3.len(), 64, "{}", m.file);
            assert!(m.url.contains("/resolve/") && m.url.ends_with(m.file));
            // A pinned revision, not a branch.
            let rev = m.url.split("/resolve/").nth(1).unwrap().split('/').next();
            assert_eq!(rev.map(str::len), Some(40), "{}", m.url);
        }
        assert_eq!(TOTAL_BYTES, 88_030_804 + 7_830_888);
        assert_ne!(DETECTOR.env, RECOGNIZER.env);
    }
}

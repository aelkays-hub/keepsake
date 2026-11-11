//! The two face models (ADR-040): where they are, and installing them.
//!
//! Like the image encoder ([`crate::ai::install`]), they are never committed
//! or bundled. Turning on Recognise faces downloads them once into the
//! user's cache folder, pinned to a revision and kept only if the BLAKE3
//! matches. Developers can put them in `vendor/models/` instead (setup
//! scripts, `-WithAi` / `--with-ai`).

use std::sync::atomic::AtomicBool;

pub use crate::ai::install::ModelFile;
use crate::ai::install::{download_checked, remove_model_at};
use crate::ai::AiError;

/// Finds faces: YuNet, OpenCV Zoo `face_detection_yunet_2023mar.onnx`
/// (MIT), Hugging Face `opencv/face_detection_yunet` at revision `3cc26e7f`.
pub const DETECTOR: ModelFile = ModelFile {
    dir: "yunet-2023mar",
    file: "face_detection_yunet_2023mar.onnx",
    url: "https://huggingface.co/opencv/face_detection_yunet/resolve/3cc26e7f1014a5ee5d74a42acee58bafc9d0a310/face_detection_yunet_2023mar.onnx",
    bytes: 232_589,
    blake3: "3d5938c4cd5a02dc416f1cd1f7fc1f662a22adc370477112c871954587e63431",
    env: "KEEPSAKE_FACE_DETECTOR",
};

/// Describes faces: SFace, OpenCV Zoo `face_recognition_sface_2021dec.onnx`
/// (Apache-2.0), Hugging Face `opencv/face_recognition_sface` at revision
/// `3d708243`.
pub const RECOGNIZER: ModelFile = ModelFile {
    dir: "sface-2021dec",
    file: "face_recognition_sface_2021dec.onnx",
    url: "https://huggingface.co/opencv/face_recognition_sface/resolve/3d7082438a6e4551e840c9b2bb60b71e8da4b524/face_recognition_sface_2021dec.onnx",
    bytes: 38_696_353,
    blake3: "584d0f5792e9e776ab5e5bf3f559f2310ff8fdd3b39f94dd364751046592b512",
    env: "KEEPSAKE_FACE_RECOGNIZER",
};

/// Both, in the order they are downloaded.
pub const MODELS: [ModelFile; 2] = [DETECTOR, RECOGNIZER];

/// Bytes to download for both.
pub const TOTAL_BYTES: u64 = DETECTOR.bytes + RECOGNIZER.bytes;

/// Both face models are installed.
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
        assert_eq!(TOTAL_BYTES, 232_589 + 38_696_353);
    }

    #[test]
    fn a_model_named_by_its_variable_is_used_only_if_it_is_there() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("m.onnx");
        let m = ModelFile {
            env: "KEEPSAKE_TEST_FACE_MODEL_ONLY_HERE",
            ..DETECTOR
        };
        std::env::set_var(m.env, &f);
        assert_eq!(m.find(), None, "missing");
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(m.find(), Some(f));
        std::env::remove_var(m.env);
    }
}

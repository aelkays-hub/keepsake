//! Where the model and ONNX Runtime are, and installing the model.
//!
//! The model is never committed or bundled: "Turn on AI" downloads it once
//! into the user's cache folder (`%LOCALAPPDATA%\keepsake\models\…`,
//! `~/Library/Caches/keepsake/models/…`) with the operating system's own
//! `curl` (part of Windows 10+ and macOS), and keeps it only if its BLAKE3
//! matches. Developers can put it in `vendor/models/` instead (setup
//! scripts, `-WithAi` / `--with-ai`).
//!
//! ONNX Runtime ships beside the other sidecars, in `vendor/onnxruntime/`
//! (or the installed app's resources): it is loaded at run time, not linked.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use super::AiError;

/// Folder name of this model, under `models/`.
pub const MODEL_DIR: &str = "siglip2-b16-224";
/// The image encoder file.
pub const MODEL_FILE: &str = "vision_model.onnx";
/// Pinned download: onnx-community/siglip2-base-patch16-224-ONNX at
/// revision `ba1f3b08` (Apache-2.0, converted from google/siglip2-base-patch16-224).
pub const MODEL_URL: &str = "https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/ba1f3b0843f24bc5417d38e19c37b287d719b2f4/onnx/vision_model.onnx";
/// Its size in bytes.
pub const MODEL_BYTES: u64 = 371_807_752;
/// Its BLAKE3 (hex).
pub const MODEL_BLAKE3: &str = "2bc84d1b801ca6796f8a050445eb251a3c4c87a3231803342834250e6131856b";

/// The per-user folder models are installed into.
pub fn user_models_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("keepsake").join("models"))
}

/// Where an installed model would be in the user's cache folder.
pub fn user_model_path() -> Option<PathBuf> {
    user_models_dir().map(|d| d.join(MODEL_DIR).join(MODEL_FILE))
}

pub(crate) fn non_empty(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.len() > 0)
}

/// The model file, if installed. In order: `KEEPSAKE_AI_MODEL` (a file),
/// the user's cache folder, then `vendor/models/` (developers).
pub fn find_model() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KEEPSAKE_AI_MODEL") {
        let p = PathBuf::from(p);
        return non_empty(&p).then_some(p);
    }
    user_model_path().filter(|p| non_empty(p)).or_else(|| {
        crate::probe::tools::vendor_root()
            .map(|v| v.join("models").join(MODEL_DIR).join(MODEL_FILE))
            .filter(|p| non_empty(p))
    })
}

/// One pinned model file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelFile {
    /// Folder under `models/`.
    pub dir: &'static str,
    pub file: &'static str,
    pub url: &'static str,
    pub bytes: u64,
    /// BLAKE3 (hex).
    pub blake3: &'static str,
    /// Overrides where it is (a file), for tests and developers.
    pub env: &'static str,
}

impl ModelFile {
    /// Where it is installed in the user's cache folder.
    pub fn user_path(&self) -> Option<PathBuf> {
        user_models_dir().map(|d| d.join(self.dir).join(self.file))
    }

    /// The file, if installed. In order: its environment variable (a file),
    /// the user's cache folder, then `vendor/models/` (developers).
    pub fn find(&self) -> Option<PathBuf> {
        if let Some(p) = std::env::var_os(self.env) {
            let p = PathBuf::from(p);
            return non_empty(&p).then_some(p);
        }
        self.user_path().filter(|p| non_empty(p)).or_else(|| {
            crate::probe::tools::vendor_root()
                .map(|v| v.join("models").join(self.dir).join(self.file))
                .filter(|p| non_empty(p))
        })
    }

    /// The file, or the error saying where it was looked for.
    pub fn require(&self) -> Result<PathBuf, AiError> {
        self.find()
            .ok_or_else(|| AiError::ModelMissing(self.user_path().unwrap_or_default()))
    }
}

/// File name of the ONNX Runtime library on this OS.
pub fn runtime_file_name() -> &'static str {
    if cfg!(windows) {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    }
}

/// ONNX Runtime, if present. In order: `KEEPSAKE_ORT_DYLIB`,
/// `<vendor>/onnxruntime/` (the setup scripts put this machine's build
/// there; the installed app's resources are laid out the same), then the
/// running program's folder.
pub fn find_runtime() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("KEEPSAKE_ORT_DYLIB") {
        let p = PathBuf::from(p);
        return non_empty(&p).then_some(p);
    }
    let name = runtime_file_name();
    crate::probe::tools::vendor_root()
        .map(|v| v.join("onnxruntime").join(name))
        .filter(|p| non_empty(p))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|e| e.parent().map(|d| d.join(name)))
                .filter(|p| non_empty(p))
        })
}

/// BLAKE3 of a file, hex.
pub fn file_blake3(path: &Path) -> Result<String, AiError> {
    let mut h = blake3::Hasher::new();
    h.update_reader(std::fs::File::open(path)?)?;
    Ok(h.finalize().to_hex().to_string())
}

fn curl() -> PathBuf {
    if cfg!(windows) {
        // Windows' own curl, not whatever else is on PATH.
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(root).join("System32").join("curl.exe")
    } else {
        PathBuf::from("/usr/bin/curl")
    }
}

/// Download the model into the user's cache folder. `progress` gets
/// `(bytes so far, total)` about twice a second. Returns the model's path.
/// A download that fails, is cancelled or does not match is deleted.
pub fn install_model(
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, AiError> {
    let target = user_model_path()
        .ok_or_else(|| AiError::Download("no cache folder on this computer".into()))?;
    download_checked(
        MODEL_URL,
        &target,
        MODEL_BYTES,
        MODEL_BLAKE3,
        cancel,
        progress,
    )?;
    Ok(target)
}

/// Download `url` to `target` (through `target.part`), keeping it only if
/// its BLAKE3 is `blake3_hex`. An existing matching `target` is kept as is.
pub fn download_checked(
    url: &str,
    target: &Path,
    total: u64,
    blake3_hex: &str,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), AiError> {
    if non_empty(target) && file_blake3(target)? == blake3_hex {
        progress(total, total);
        return Ok(());
    }
    let dir = target
        .parent()
        .ok_or_else(|| AiError::Download("bad model path".into()))?;
    std::fs::create_dir_all(dir)?;
    let part = target.with_extension("part");
    let _ = std::fs::remove_file(&part);
    let mut child = Command::new(curl())
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--output",
        ])
        .arg(&part)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| AiError::Download(format!("could not start curl: {e}")))?;
    let status = loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&part);
            return Err(AiError::Cancelled);
        }
        if let Some(s) = child.try_wait()? {
            break s;
        }
        progress(
            std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0),
            total,
        );
        std::thread::sleep(Duration::from_millis(500));
    };
    if !status.success() {
        let mut err = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = std::io::Read::read_to_string(&mut e, &mut err);
        }
        let _ = std::fs::remove_file(&part);
        return Err(AiError::Download(format!(
            "curl failed ({status}): {}",
            err.trim()
        )));
    }
    let got = file_blake3(&part)?;
    if got != blake3_hex {
        let _ = std::fs::remove_file(&part);
        return Err(AiError::Download(format!(
            "the downloaded file does not match (BLAKE3 {got}, expected {blake3_hex})"
        )));
    }
    std::fs::rename(&part, target)?;
    progress(total, total);
    Ok(())
}

/// Remove the model from the user's cache folder (turning AI off for good).
pub fn remove_user_model() -> Result<bool, AiError> {
    match user_model_path() {
        Some(p) => remove_model_at(&p),
        None => Ok(false),
    }
}

/// Delete one model file (and a leftover `.part` beside it). `false` when
/// there was none.
pub fn remove_model_at(model: &Path) -> Result<bool, AiError> {
    let _ = std::fs::remove_file(model.with_extension("part"));
    match std::fs::remove_file(model) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_the_model_deletes_only_it() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join(MODEL_FILE);
        let other = dir.path().join("keep.txt");
        std::fs::write(&model, b"m").unwrap();
        std::fs::write(model.with_extension("part"), b"p").unwrap();
        std::fs::write(&other, b"o").unwrap();
        assert!(remove_model_at(&model).unwrap());
        assert!(!model.exists() && !model.with_extension("part").exists());
        assert!(other.exists());
        assert!(!remove_model_at(&model).unwrap());
    }

    #[test]
    fn a_file_that_does_not_match_is_not_kept() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.bin");
        std::fs::write(&src, b"hello").unwrap();
        let url = format!("file:///{}", src.display().to_string().replace('\\', "/"));
        let target = dir.path().join("out").join("m.onnx");
        let cancel = AtomicBool::new(false);
        let r = download_checked(&url, &target, 5, "00", &cancel, &mut |_, _| {});
        if matches!(&r, Err(AiError::Download(m)) if m.contains("could not start curl")) {
            return; // no curl on this machine
        }
        assert!(matches!(r, Err(AiError::Download(_))), "{r:?}");
        assert!(!target.exists());
        assert!(!target.with_extension("part").exists());
        // With the right hash it is kept.
        let good = file_blake3(&src).unwrap();
        download_checked(&url, &target, 5, &good, &cancel, &mut |_, _| {}).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    }

    #[test]
    fn a_cancelled_download_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("m.onnx");
        let cancel = AtomicBool::new(true);
        let r = download_checked(
            "https://127.0.0.1:9/never",
            &target,
            1,
            "00",
            &cancel,
            &mut |_, _| {},
        );
        if matches!(&r, Err(AiError::Download(m)) if m.contains("could not start curl")) {
            return;
        }
        assert!(matches!(r, Err(AiError::Cancelled)), "{r:?}");
        assert!(!target.exists());
    }
}

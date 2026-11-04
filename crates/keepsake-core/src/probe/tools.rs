//! Locating the ExifTool and ffprobe sidecar binaries.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use super::error::ProbeError;

/// Paths to the sidecar tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPaths {
    pub exiftool: PathBuf,
    pub ffprobe: PathBuf,
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

impl ToolPaths {
    /// Tools inside a `vendor/` folder laid out by the setup scripts.
    pub fn in_vendor(vendor: &Path) -> Self {
        Self {
            exiftool: vendor.join("exiftool").join(exe("exiftool")),
            ffprobe: vendor.join(exe("ffprobe")),
        }
    }

    /// Resolve the tools. In order:
    /// 1. `KEEPSAKE_VENDOR` (tests, and anyone overriding the tools);
    /// 2. the folder the installed app registered with [`set_bundled`]
    ///    (its resource folder, laid out like `vendor/`);
    /// 3. a `vendor/` folder walking up from the current directory, then
    ///    from the running program (development builds);
    /// 4. the running program's own folder, if the tools are there.
    ///
    /// `KEEPSAKE_EXIFTOOL` / `KEEPSAKE_FFPROBE` override single tools.
    pub fn discover() -> Self {
        let vendor = std::env::var_os("KEEPSAKE_VENDOR")
            .map(PathBuf::from)
            .or_else(|| BUNDLED.get().cloned())
            .or_else(|| std::env::current_dir().ok().and_then(|d| vendor_above(&d)))
            .or_else(|| exe_dir().and_then(|d| vendor_above(&d)))
            .or_else(|| exe_dir().filter(|d| Self::in_vendor(d).ffprobe.is_file()))
            .unwrap_or_else(|| PathBuf::from("vendor"));
        let mut t = Self::in_vendor(&vendor);
        if let Some(p) = std::env::var_os("KEEPSAKE_EXIFTOOL") {
            t.exiftool = PathBuf::from(p);
        }
        if let Some(p) = std::env::var_os("KEEPSAKE_FFPROBE") {
            t.ffprobe = PathBuf::from(p);
        }
        t
    }

    /// Error unless both tools exist and are non-empty files.
    pub fn check(&self) -> Result<(), ProbeError> {
        for (tool, path) in [("exiftool", &self.exiftool), ("ffprobe", &self.ffprobe)] {
            let ok = std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() > 0);
            if !ok {
                return Err(ProbeError::ToolMissing {
                    tool,
                    path: path.clone(),
                });
            }
        }
        Ok(())
    }
}

static BUNDLED: OnceLock<PathBuf> = OnceLock::new();

/// Tell [`ToolPaths::discover`] where the installed app keeps its tools
/// (laid out like `vendor/`). Called once at start-up; later calls are
/// ignored.
pub fn set_bundled(dir: PathBuf) {
    let _ = BUNDLED.set(dir);
}

/// The folder the installed app registered, if any.
pub fn bundled() -> Option<&'static Path> {
    BUNDLED.get().map(PathBuf::as_path)
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)
}

fn vendor_above(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let v = dir.join("vendor");
        if v.is_dir() {
            return Some(v);
        }
        if !dir.pop() {
            return None;
        }
    }
}

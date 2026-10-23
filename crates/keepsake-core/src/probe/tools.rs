//! Locating the ExifTool and ffprobe sidecar binaries.

use std::path::{Path, PathBuf};

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

    /// Resolve tools for development and tests: `KEEPSAKE_EXIFTOOL` /
    /// `KEEPSAKE_FFPROBE` if set, else `KEEPSAKE_VENDOR`, else the first
    /// `vendor/` folder found walking up from the current directory.
    /// The Tauri app passes its bundled paths instead of calling this.
    pub fn discover() -> Self {
        let vendor = std::env::var_os("KEEPSAKE_VENDOR")
            .map(PathBuf::from)
            .or_else(find_vendor_upwards)
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

fn find_vendor_upwards() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
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

//! Machine settings: `config.toml` in the OS configuration folder, shared
//! by the CLI and the app. `KEEPSAKE_CONFIG_DIR` overrides the folder.
//!
//! ```toml
//! default_library = 'D:\Keepsake'
//!
//! [render]
//! cache_dir = 'E:\KeepsakeCache'   # an SSD; default: the OS cache folder
//! max_bytes = 21474836480          # 20 GiB
//! workers = 4                      # thumbnails/previews in parallel
//! proxy_workers = 1                # video proxies in parallel
//! pause_on_battery = true
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::LibraryError;

/// Default rendition cache size limit: 20 GiB.
pub const DEFAULT_CACHE_BYTES: u64 = 20 * 1024 * 1024 * 1024;

/// Rendition settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderSettings {
    /// Where renditions are stored (ideally an SSD). `None`: the OS cache
    /// folder (`…/keepsake/cache/renditions`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_dir: Option<PathBuf>,
    /// Cache size limit in bytes. Renditions of offline media are kept even
    /// above it.
    pub max_bytes: u64,
    /// Thumbnail/preview workers. `None`: half the CPU cores, 1 to 4.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workers: Option<usize>,
    /// Video proxy workers (each one runs an encoder).
    pub proxy_workers: usize,
    /// Wait while the computer runs on battery.
    pub pause_on_battery: bool,
}

impl Default for RenderSettings {
    fn default() -> Self {
        Self {
            cache_dir: None,
            max_bytes: DEFAULT_CACHE_BYTES,
            workers: None,
            proxy_workers: 1,
            pause_on_battery: true,
        }
    }
}

impl RenderSettings {
    /// The cache folder to use.
    pub fn cache_root(&self) -> Result<PathBuf, LibraryError> {
        match &self.cache_dir {
            Some(d) => Ok(d.clone()),
            None => crate::paths::cache_dir()
                .map(|d| d.join("renditions"))
                .ok_or_else(|| LibraryError::Invalid("no cache folder on this system".into())),
        }
    }

    /// Thumbnail/preview workers to run.
    pub fn image_workers(&self) -> usize {
        self.workers.unwrap_or_else(default_workers).max(1)
    }
}

/// Half the logical CPUs, at least 1 and at most 4.
pub fn default_workers() -> usize {
    std::thread::available_parallelism()
        .map(|n| (n.get() / 2).clamp(1, 4))
        .unwrap_or(1)
}

/// "Open in" applications per file type (`[open.apps]`, lower-case
/// extensions without the dot). Types not listed use the system default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenSettings {
    pub apps: std::collections::BTreeMap<String, String>,
}

impl OpenSettings {
    /// The application for `path`, if one is set for its type.
    pub fn app_for(&self, path: &Path) -> Option<&str> {
        let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
        self.apps
            .get(&ext)
            .map(String::as_str)
            .filter(|a| !a.trim().is_empty())
    }
}

/// Everything in `config.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_library: Option<PathBuf>,
    pub render: RenderSettings,
    pub open: OpenSettings,
}

impl Settings {
    /// The configuration folder.
    pub fn dir() -> Result<PathBuf, LibraryError> {
        match std::env::var_os("KEEPSAKE_CONFIG_DIR") {
            Some(d) => Ok(PathBuf::from(d)),
            None => crate::paths::config_dir().ok_or_else(|| {
                LibraryError::Invalid("no configuration folder on this system".into())
            }),
        }
    }

    pub fn path() -> Result<PathBuf, LibraryError> {
        Ok(Self::dir()?.join("config.toml"))
    }

    /// Load `config.toml` (defaults if it does not exist).
    pub fn load() -> Result<Self, LibraryError> {
        Self::load_from(&Self::path()?)
    }

    pub fn load_from(path: &Path) -> Result<Self, LibraryError> {
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s)
                .map_err(|e| LibraryError::Invalid(format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Save to `config.toml`; returns its path.
    pub fn save(&self) -> Result<PathBuf, LibraryError> {
        let p = Self::path()?;
        let text = toml::to_string_pretty(self)
            .map_err(|e| LibraryError::Invalid(format!("settings: {e}")))?;
        crate::library::write_bytes_atomic(&p, text.as_bytes())?;
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_and_partial_sections_load_with_defaults() {
        let s: Settings = toml::from_str("default_library = 'D:/K'\n").unwrap();
        assert_eq!(s.render, RenderSettings::default());
        let s: Settings = toml::from_str("[render]\nworkers = 2\n").unwrap();
        assert_eq!(s.render.workers, Some(2));
        assert_eq!(s.render.max_bytes, DEFAULT_CACHE_BYTES);
        assert!(s.render.pause_on_battery);
    }

    #[test]
    fn round_trip() {
        let mut s = Settings::default();
        s.render.cache_dir = Some(PathBuf::from("E:/cache"));
        s.render.max_bytes = 5;
        s.open
            .apps
            .insert("arw".into(), r"C:\Apps\Raw Editor.exe".into());
        s.open.apps.insert("mp4".into(), " ".into());
        assert_eq!(
            s.open.app_for(Path::new("x/DSC1.ARW")),
            Some(r"C:\Apps\Raw Editor.exe")
        );
        assert_eq!(s.open.app_for(Path::new("C1.MP4")), None, "blank: default");
        assert_eq!(s.open.app_for(Path::new("a.jpg")), None);
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), s);
        assert!((1..=4).contains(&default_workers()));
    }
}

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

/// The applications for one file type: one, or several in order (the first
/// is the default). Written as a string when there is one, so files from
/// earlier versions read and write the same way.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenApps {
    One(String),
    Many(Vec<String>),
}

impl OpenApps {
    /// From a list (one app is written as a plain string).
    pub fn from_list(mut apps: Vec<String>) -> Self {
        if apps.len() == 1 {
            Self::One(apps.remove(0))
        } else {
            Self::Many(apps)
        }
    }

    /// The non-blank applications, in order, without repeats.
    pub fn list(&self) -> Vec<&str> {
        let all: Vec<&str> = match self {
            Self::One(a) => vec![a.as_str()],
            Self::Many(v) => v.iter().map(String::as_str).collect(),
        };
        let mut out: Vec<&str> = Vec::new();
        for a in all.into_iter().map(str::trim).filter(|a| !a.is_empty()) {
            if !out.iter().any(|o| o.eq_ignore_ascii_case(a)) {
                out.push(a);
            }
        }
        out
    }
}

/// "Open in" applications per file type (`[open.apps]`, lower-case
/// extensions without the dot): `jpg = 'C:\…\Photoshop.exe'`, or a list,
/// `arw = ['C:\…\Photoshop.exe', 'C:\…\Topaz Photo.exe']`. The first is the
/// default (Ctrl+O); all are offered in the Open in menu. Types not listed
/// use the system default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenSettings {
    pub apps: std::collections::BTreeMap<String, OpenApps>,
}

impl OpenSettings {
    /// The applications for `path`'s type, default first (empty: none set).
    pub fn apps_for(&self, path: &Path) -> Vec<&str> {
        let Some(ext) = path.extension() else {
            return Vec::new();
        };
        let ext = ext.to_string_lossy().to_ascii_lowercase();
        self.apps.get(&ext).map(OpenApps::list).unwrap_or_default()
    }

    /// The default application for `path`, if one is set for its type.
    pub fn app_for(&self, path: &Path) -> Option<&str> {
        self.apps_for(path).first().copied()
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
        s.open.apps.insert(
            "arw".into(),
            OpenApps::One(r"C:\Apps\Raw Editor.exe".into()),
        );
        s.open.apps.insert("mp4".into(), OpenApps::One(" ".into()));
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

    #[test]
    fn several_apps_per_type_first_is_the_default() {
        let text = r#"
[open.apps]
jpg = 'C:\Apps\Photoshop.exe'
arw = ['C:\Apps\Photoshop.exe', ' ', 'C:\Apps\Topaz Photo.exe', 'c:\apps\photoshop.exe']
"#;
        let s: Settings = toml::from_str(text).unwrap();
        assert_eq!(
            s.open.apps_for(Path::new("DSC1.ARW")),
            [r"C:\Apps\Photoshop.exe", r"C:\Apps\Topaz Photo.exe"],
            "blanks and repeats dropped, order kept"
        );
        assert_eq!(
            s.open.app_for(Path::new("DSC1.arw")),
            Some(r"C:\Apps\Photoshop.exe")
        );
        assert_eq!(
            s.open.apps_for(Path::new("a.JPG")),
            [r"C:\Apps\Photoshop.exe"]
        );
        assert!(s.open.apps_for(Path::new("noext")).is_empty());
        // One app is still written as a plain string.
        let out = toml::to_string_pretty(&s).unwrap();
        assert!(out.contains(r#"jpg = 'C:\Apps\Photoshop.exe'"#), "{out}");
        assert_eq!(toml::from_str::<Settings>(&out).unwrap(), s);
        assert_eq!(
            OpenApps::from_list(vec!["a".into()]),
            OpenApps::One("a".into())
        );
    }
}

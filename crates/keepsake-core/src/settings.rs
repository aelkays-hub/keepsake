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
//!
//! [ai]
//! enabled = true                   # tags and duplicates (ADR-035)
//! gpu = true
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::library::LibraryError;

/// Default rendition cache size limit: 20 GiB.
/// `[ai] cloud_provider` when "All" was used last (ADR-039).
pub const ALL_PROVIDERS: &str = "all";

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

/// What happens when a camera card is inserted (`[import]`).
///
/// ```toml
/// [import]
/// auto = true                       # import every card by itself
/// destination = 'E:\Pictures'
/// template = '{yyyy}/{month}/{yyyy}-{mm}-{dd}/{orig}'
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportSettings {
    /// Import a card's new items by itself when it is inserted (the app);
    /// otherwise the import window opens for you to choose.
    pub auto: bool,
    /// Where an automatic import copies to; folders the template names are
    /// made as needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<PathBuf>,
    /// The file-naming template (`ingest::template`).
    pub template: String,
}

impl Default for ImportSettings {
    fn default() -> Self {
        Self {
            auto: false,
            destination: None,
            template: crate::ingest::template::YEAR_MONTH_DAY_TEMPLATE.to_owned(),
        }
    }
}

impl ImportSettings {
    /// The destination for an automatic import, when it is on and set.
    pub fn auto_destination(&self) -> Option<&Path> {
        self.destination.as_deref().filter(|_| self.auto)
    }
}

/// On-device AI (ADR-035).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    /// Tag photos and find duplicates in the background. Off until the
    /// user turns it on (which downloads the model).
    pub enabled: bool,
    /// Use the graphics card when it can (DirectML / CoreML).
    pub gpu: bool,
    /// Recognise faces (ADR-040): search photos for faces when asked. Off
    /// until the user turns it on (which downloads the face models); needs
    /// `enabled`.
    pub faces: bool,
    /// Read aircraft (ADR-041): read the registration, airline and type of
    /// the planes in the photos the user picks. Off until the user turns it
    /// on (which downloads the text models). It does not need `enabled`,
    /// though the type is only guessed from the look with On-device AI's
    /// model installed.
    pub aircraft: bool,
    /// How alike a face must be to a person's average face to be suggested
    /// for them, in percent (cosine × 100; ADR-040): stricter is higher.
    pub face_match_pct: u8,
    /// Read aircraft's review window (ADR-041): how sure, in percent, a
    /// line must be for a registration, an airline and a painted type to
    /// be offered. Set with the window's sliders; kept for next time.
    pub aircraft_registration_pct: u8,
    pub aircraft_airline_pct: u8,
    pub aircraft_type_pct: u8,
    /// Look registrations up online (adsbdb.com) and offer what it knows
    /// in the review window (ADR-041). Off until the user turns it on.
    pub aircraft_lookup: bool,
    /// Photos taken this many seconds either side, by the same camera and
    /// lens, that look alike count as the same shot: Read aircraft offers
    /// them its keywords, Tag with AI gives them its tags. 1–60.
    pub same_shot_s: u8,
    /// Tag with AI (ADR-036, ADR-038): each provider's model, as its API
    /// id. The API keys are in the OS's credential store, never here.
    pub claude_model: String,
    pub openai_model: String,
    pub gemini_model: String,
    /// The provider offered first (the one used last): `claude`, `openai`
    /// or `gemini`, or `all` (every AI with a key, ADR-039).
    pub cloud_provider: String,
}

impl AiSettings {
    /// The provider offered first (Claude if the setting names none known).
    pub fn cloud_provider(&self) -> crate::ai::Provider {
        crate::ai::Provider::parse(&self.cloud_provider).unwrap_or_default()
    }

    /// Read aircraft's confidence for each kind of mark, kept in range.
    pub fn aircraft_confidence(&self) -> crate::ai::aircraft::marks::Confidence {
        let f = |p: u8| f32::from(p) / 100.0;
        crate::ai::aircraft::marks::Confidence {
            registration: f(self.aircraft_registration_pct),
            airline: f(self.aircraft_airline_pct),
            aircraft_type: f(self.aircraft_type_pct),
        }
        .clamped()
    }

    /// Keep Read aircraft's confidence (rounded to whole percents).
    pub fn set_aircraft_confidence(&mut self, c: crate::ai::aircraft::marks::Confidence) {
        let c = c.clamped();
        let p = |v: f32| (v * 100.0).round().clamp(0.0, 100.0) as u8;
        self.aircraft_registration_pct = p(c.registration);
        self.aircraft_airline_pct = p(c.airline);
        self.aircraft_type_pct = p(c.aircraft_type);
    }

    /// The same-shot window in milliseconds (1–60 seconds).
    pub fn same_shot_ms(&self) -> i64 {
        i64::from(self.same_shot_s.clamp(1, 60)) * 1000
    }

    /// The face cut-off as a cosine, kept in range.
    pub fn face_match(&self) -> f32 {
        crate::ai::faces::people::clamp_match(f32::from(self.face_match_pct) / 100.0)
    }

    /// "All" was used last: every AI with a key is offered first.
    pub fn cloud_all(&self) -> bool {
        self.cloud_provider == ALL_PROVIDERS
    }

    /// `provider`'s chosen model (its default if the setting names none of
    /// its models).
    pub fn cloud_model(&self, provider: crate::ai::Provider) -> crate::ai::CloudModel {
        use crate::ai::Provider;
        let id = match provider {
            Provider::Claude => &self.claude_model,
            Provider::OpenAi => &self.openai_model,
            Provider::Gemini => &self.gemini_model,
        };
        crate::ai::CloudModel::parse(id)
            .filter(|m| m.provider() == provider)
            .unwrap_or_else(|| provider.default_model())
    }

    /// Choose a model (for its provider).
    pub fn set_cloud_model(&mut self, model: crate::ai::CloudModel) {
        use crate::ai::Provider;
        let slot = match model.provider() {
            Provider::Claude => &mut self.claude_model,
            Provider::OpenAi => &mut self.openai_model,
            Provider::Gemini => &mut self.gemini_model,
        };
        *slot = model.id().to_owned();
    }
}

impl Default for AiSettings {
    fn default() -> Self {
        use crate::ai::Provider;
        Self {
            enabled: false,
            gpu: true,
            faces: false,
            aircraft: false,
            face_match_pct: 55,
            aircraft_registration_pct: 80,
            aircraft_airline_pct: 85,
            aircraft_type_pct: 70,
            aircraft_lookup: false,
            same_shot_s: 10,
            claude_model: Provider::Claude.default_model().id().to_owned(),
            openai_model: Provider::OpenAi.default_model().id().to_owned(),
            gemini_model: Provider::Gemini.default_model().id().to_owned(),
            cloud_provider: Provider::default().id().to_owned(),
        }
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
    pub import: ImportSettings,
    pub ai: AiSettings,
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
    use crate::ai::{CloudModel, Provider};

    #[test]
    fn each_provider_keeps_its_own_model() {
        // A file from before ChatGPT and Gemini: Claude's model stays.
        let s: Settings = toml::from_str("[ai]\nclaude_model = 'claude-haiku-5-5'\n").unwrap();
        assert_eq!(
            s.ai.cloud_model(Provider::Claude),
            CloudModel::ClaudeHaiku55
        );
        assert_eq!(s.ai.cloud_model(Provider::OpenAi), CloudModel::Gpt61Sol);
        assert_eq!(s.ai.cloud_provider(), Provider::Claude);
        let mut a = s.ai;
        a.set_cloud_model(CloudModel::Gemini35FlashLite);
        assert_eq!(a.gemini_model, "gemini-3.5-flash-lite");
        assert_eq!(a.cloud_model(Provider::Claude), CloudModel::ClaudeHaiku55);
        // A model of another provider, or an unknown one, is not used.
        a.openai_model = "claude-opus-5-5".into();
        assert_eq!(a.cloud_model(Provider::OpenAi), CloudModel::Gpt61Sol);
        a.cloud_provider = "nobody".into();
        assert_eq!(a.cloud_provider(), Provider::Claude);
        assert!(!a.cloud_all());
        // "All" is remembered; one provider is still Claude.
        a.cloud_provider = ALL_PROVIDERS.into();
        assert!(a.cloud_all());
        assert_eq!(a.cloud_provider(), Provider::Claude);
    }

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
    fn import_settings_default_off_and_round_trip() {
        let s: Settings = toml::from_str("default_library = 'D:/K'\n").unwrap();
        assert!(!s.import.auto);
        assert_eq!(s.import.auto_destination(), None);
        assert_eq!(s.import.template, "{yyyy}/{month}/{yyyy}-{mm}-{dd}/{orig}");
        let s: Settings =
            toml::from_str("[import]\nauto = true\ndestination = 'E:/Pictures'\n").unwrap();
        assert_eq!(s.import.auto_destination(), Some(Path::new("E:/Pictures")));
        let text = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), s);
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

use serde::{Deserialize, Serialize};

/// Detected file extension kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Mp4,
    Xml,
    Mts,
    Jpg,
    Arw,
    Hif,
    Cpi,
    Mpl,
    Bdm,
    Other,
}

impl FileKind {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "mp4" => Self::Mp4,
            "xml" => Self::Xml,
            "mts" | "m2ts" => Self::Mts,
            "jpg" | "jpeg" => Self::Jpg,
            "arw" => Self::Arw,
            "hif" | "heif" | "heic" => Self::Hif,
            "cpi" => Self::Cpi,
            "mpl" => Self::Mpl,
            "bdm" => Self::Bdm,
            _ => Self::Other,
        }
    }
}

/// Which Sony card folder a file lives in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FolderRole {
    /// `PRIVATE/M4ROOT/CLIP/` — XAVC main clip and XML sidecar.
    Clip,
    /// `PRIVATE/M4ROOT/SUB/` — camera-generated proxy.
    Proxy,
    /// `AVCHD/BDMV/STREAM/` — MPEG transport stream segments.
    Stream,
    /// `AVCHD/BDMV/CLIPINF/` — CPI clip information.
    ClipInfo,
    /// `AVCHD/BDMV/PLAYLIST/` — MPL playlist files.
    Playlist,
    /// `AVCHD/BDMV/` root — INDEX.BDM, MOVIEOBJ.BDM.
    BdmvRoot,
    /// `DCIM/**` — still photos.
    Dcim,
    /// `MP_ROOT/**` — older MP4 cameras.
    MpRoot,
    Unknown,
}

/// One file entry in the manifest — no media bytes, only metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub rel_path: String,
    pub size_bytes: u64,
    /// BLAKE3 hash as lowercase hex.
    pub blake3: String,
    pub kind: FileKind,
    pub folder_role: FolderRole,
}

/// Why the scanner skipped a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    Hidden,
    System,
    HiddenAndSystem,
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hidden => f.write_str("hidden"),
            Self::System => f.write_str("system"),
            Self::HiddenAndSystem => f.write_str("hidden+system"),
        }
    }
}

/// A file that was found on disk but intentionally not hashed or indexed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedFile {
    pub rel_path: String,
    pub reason: SkipReason,
}

/// All files from one camera card.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraCard {
    /// Subdirectory name under `KEEPSAKE_CORPUS/cards/`.
    pub name: String,
    pub files: Vec<FileEntry>,
    /// Files found on disk but intentionally not indexed (hidden, system).
    #[serde(default)]
    pub skipped: Vec<SkippedFile>,
}

/// Top-level corpus manifest written to `corpus/manifest.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub scanned_at: String,
    pub corpus_root: String,
    pub cameras: Vec<CameraCard>,
}

/// One logical asset group with placeholder metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetGroup {
    /// Camera card name (subdirectory under `cards/`).
    pub camera: String,
    /// Relative paths of all files in this asset, sorted.
    pub files: Vec<String>,
    /// ISO 8601 capture timestamp — fill in after ExifTool integration.
    pub capture_ts: String,
    /// Camera model string — fill in after ExifTool integration.
    pub camera_model: String,
    /// Duration in milliseconds; None for still photos.
    pub duration_ms: Option<u64>,
    /// Set true after manual spot-check against camera playback.
    pub confirmed: bool,
    /// Set true by `init-expected --force` when this asset no longer appears
    /// in the current manifest (files deleted or moved). Review and remove manually.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub removed: bool,
    /// Set by `init-expected --probe` when `capture_ts`, `camera_model` and
    /// `duration_ms` were filled in from Keepsake's own probe. Check them
    /// against the camera before setting `confirmed = true`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub prefilled: bool,
}

/// Root of `corpus/expected.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expected {
    pub asset: Vec<AssetGroup>,
}

//! Domain types persisted by the catalog.

use std::fmt;
use std::path::PathBuf;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};

use super::error::CatalogError;
use super::ids::{
    AssetId, Blake3Hash, EventId, FileId, ImportSessionId, RecipeId, RenditionId, SequenceId,
    VolumeId,
};
use crate::relpath::{nfc, CaseSensitivity, RelPath};

/// Declares a string-backed enum stored as TEXT.
macro_rules! text_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $s),+ }
            }

            pub fn parse(s: &str) -> Option<Self> {
                match s { $($s => Some($name::$variant),)+ _ => None }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(self.as_str()))
            }
        }

        impl FromSql for $name {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                let s = value.as_str()?;
                $name::parse(s).ok_or_else(|| FromSqlError::Other(
                    format!("invalid {} value: {s}", stringify!($name)).into(),
                ))
            }
        }
    };
}

text_enum!(
    /// What kind of storage a volume is.
    VolumeKind { Card => "card", Internal => "internal", External => "external", Network => "network" }
);
text_enum!(
    /// Photo or video.
    MediaKind { Photo => "photo", Video => "video" }
);
text_enum!(
    /// The part a physical file plays within its asset.
    FileRole {
        Original => "original",
        RawPair => "raw_pair",
        Sidecar => "sidecar",
        CameraProxy => "camera_proxy",
        AppProxy => "app_proxy",
        Export => "export",
        Segment => "segment",
    }
);
text_enum!(
    /// Whether a tag was set by the user or by on-device AI.
    TagSource { User => "user", Ai => "ai" }
);
text_enum!(
    /// What the user decided about an asset in a group of duplicates
    /// (ADR-035). Only recorded: no file is moved or deleted.
    DuplicateChoice {
        Keeper => "keeper",
        NotKept => "not_kept",
        NotDuplicate => "not_duplicate",
    }
);
text_enum!(
    /// Cached derivative kind.
    RenditionKind { Thumbnail => "thumbnail", Preview => "preview", Proxy => "proxy", Export => "export" }
);
text_enum!(
    /// State of an import run.
    ImportStatus { Running => "running", Completed => "completed", Failed => "failed", Cancelled => "cancelled" }
);
text_enum!(
    /// State of one file × destination in an import run (`reused`: the
    /// destination already held an identical file).
    ImportItemStatus {
        Pending => "pending",
        Copying => "copying",
        Copied => "copied",
        Verified => "verified",
        Failed => "failed",
        Skipped => "skipped",
        Reused => "reused",
    }
);
text_enum!(
    /// Quick-diff result for an asset on a card.
    ImportDiff { New => "new", AlreadyImported => "already_imported", Modified => "modified" }
);
text_enum!(
    /// Ingest state machine of one asset.
    ImportAssetState {
        Discovered => "discovered",
        Selected => "selected",
        Copying => "copying",
        Copied => "copied",
        Verified => "verified",
        Catalogued => "catalogued",
        Failed => "failed",
        Skipped => "skipped",
    }
);
text_enum!(
    /// Where an asset's capture time came from.
    CaptureSource {
        SonyXml => "sony_xml",
        Exif => "exif",
        AvchdMdpm => "avchd_mdpm",
        Container => "container",
        Sidecar => "sidecar",
    }
);
text_enum!(
    /// How much to trust a capture time (ordered: low < medium < high).
    Confidence { Low => "low", Medium => "medium", High => "high" }
);
text_enum!(
    /// Kind of still sequence.
    SequenceKind { Burst => "burst", Bracket => "bracket", Interval => "interval" }
);
text_enum!(
    /// Where an asset's XMP sidecar lives.
    SidecarLocation { Beside => "beside", Mirror => "mirror" }
);

/// A storage location (card, drive, NAS).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Volume {
    pub id: VolumeId,
    pub label: String,
    pub fs_id: String,
    pub kind: VolumeKind,
    pub case: CaseSensitivity,
    pub read_only: bool,
    pub online: bool,
    /// Absolute mount path while online. The only absolute path in the catalog.
    pub mount_path: Option<PathBuf>,
    pub last_seen_ms: Option<i64>,
}

/// A star rating, 0–5 (0 = unrated).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(try_from = "u8", into = "u8")]
pub struct Rating(u8);

impl Rating {
    pub const MAX: u8 = 5;

    pub fn new(v: u8) -> Result<Self, CatalogError> {
        if v <= Self::MAX {
            Ok(Self(v))
        } else {
            Err(CatalogError::Invalid(format!(
                "rating {v} out of range 0-5"
            )))
        }
    }

    /// Clamp an arbitrary XMP rating (Lightroom uses -1 for "rejected").
    pub fn from_xmp(v: i64) -> Self {
        Self(v.clamp(0, i64::from(Self::MAX)) as u8)
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Rating {
    type Error = CatalogError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        Rating::new(v)
    }
}

impl From<Rating> for u8 {
    fn from(r: Rating) -> u8 {
        r.0
    }
}

/// Resolved capture time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CaptureTime {
    /// UTC instant in Unix milliseconds, with any user time-shift applied.
    pub utc_ms: i64,
    /// Local UTC offset at capture, in minutes, when known.
    pub utc_offset_min: Option<i16>,
}

/// A GPS position in decimal degrees.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GpsPoint {
    pub lat: f64,
    pub lon: f64,
    pub alt_m: Option<f64>,
}

/// The logical memory of a moment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub media_kind: MediaKind,
    pub capture: Option<CaptureTime>,
    /// User time-shift in seconds already applied to `capture`.
    pub capture_shift_s: i64,
    pub duration_ms: Option<i64>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub lens_model: Option<String>,
    pub event_id: Option<EventId>,
    pub rating: Rating,
    /// Colour label name (e.g. `"Red"`).
    pub label: Option<String>,
    pub comment: Option<String>,
    pub created_ms: i64,
    pub metadata_ms: i64,
    pub capture_source: Option<CaptureSource>,
    pub capture_confidence: Option<Confidence>,
    pub gps: Option<GpsPoint>,
}

impl Asset {
    /// A new asset with no user metadata.
    pub fn new(id: AssetId, media_kind: MediaKind, now_ms: i64) -> Self {
        Self {
            id,
            media_kind,
            capture: None,
            capture_shift_s: 0,
            duration_ms: None,
            camera_make: None,
            camera_model: None,
            lens_model: None,
            event_id: None,
            rating: Rating::default(),
            label: None,
            comment: None,
            created_ms: now_ms,
            metadata_ms: now_ms,
            capture_source: None,
            capture_confidence: None,
            gps: None,
        }
    }
}

/// One physical file on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub id: FileId,
    pub asset_id: AssetId,
    pub volume_id: VolumeId,
    pub rel_path: RelPath,
    pub role: FileRole,
    pub size_bytes: u64,
    pub hash: FileHash,
    pub xxh3: Option<u64>,
    pub mtime_ms: Option<i64>,
    /// AVCHD recording position of a `segment` file (1, 2, …); `None` for
    /// every other role, including the recording's original (position 0).
    pub segment_index: Option<u32>,
}

/// A file to add to the catalog. Its id is derived from its location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFile {
    pub volume_id: VolumeId,
    pub rel_path: RelPath,
    pub role: FileRole,
    pub size_bytes: u64,
    pub hash: FileHash,
    pub xxh3: Option<u64>,
    pub mtime_ms: Option<i64>,
    pub segment_index: Option<u32>,
}

/// A file's fingerprint. Files added in place are catalogued before they
/// are read in full: until a background job has read one, it has a stand-in
/// key, which only names its renditions in the cache. Anything that checks
/// content (verify, backup, duplicates) needs [`FileHash::full`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "state", content = "blake3", rename_all = "snake_case")]
pub enum FileHash {
    /// The BLAKE3 of the whole file.
    Full(Blake3Hash),
    /// Not read in full yet: a key made by [`FileHash::pending_for`].
    Pending(Blake3Hash),
}

impl FileHash {
    /// The stand-in key of a file not yet read in full: from its place,
    /// size and quick hash, so it is unique and never equals a real BLAKE3.
    pub fn pending_for(
        volume: VolumeId,
        rel_path_key: &str,
        size_bytes: u64,
        xxh3: Option<u64>,
    ) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"keepsake:pending-fingerprint:v1|");
        h.update(volume.as_uuid().as_bytes());
        h.update(&(rel_path_key.len() as u64).to_le_bytes());
        h.update(rel_path_key.as_bytes());
        h.update(&size_bytes.to_le_bytes());
        h.update(&[u8::from(xxh3.is_some())]);
        h.update(&xxh3.unwrap_or(0).to_le_bytes());
        FileHash::Pending(Blake3Hash(*h.finalize().as_bytes()))
    }

    /// The real BLAKE3, once the whole file has been read.
    pub fn full(&self) -> Option<Blake3Hash> {
        match self {
            FileHash::Full(h) => Some(*h),
            FileHash::Pending(_) => None,
        }
    }

    /// What the file's renditions are cached under (the `blake3` column).
    pub fn key(&self) -> Blake3Hash {
        match self {
            FileHash::Full(h) | FileHash::Pending(h) => *h,
        }
    }

    pub fn is_full(&self) -> bool {
        matches!(self, FileHash::Full(_))
    }
}

/// A time-and-place cluster of assets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub title: String,
    pub place_name: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub created_ms: i64,
}

/// A colour label definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub id: i64,
    pub name: String,
    pub color_hex: Option<String>,
    pub sort_order: i64,
}

/// A hierarchical tag such as `Travel|Japan|Tokyo`.
///
/// Segments are NFC-normalised, trimmed, non-empty and may not contain `|` or `/`
/// (the Lightroom and digiKam hierarchy separators).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TagPath(Vec<String>);

impl TagPath {
    pub fn new<I, S>(segments: I) -> Result<Self, CatalogError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut out = Vec::new();
        for s in segments {
            let seg = nfc(s.as_ref().trim());
            if seg.is_empty() {
                return Err(CatalogError::Invalid("empty tag segment".into()));
            }
            if seg.contains('|') || seg.contains('/') {
                return Err(CatalogError::Invalid(format!(
                    "tag segment {seg:?} contains '|' or '/'"
                )));
            }
            out.push(seg);
        }
        if out.is_empty() {
            return Err(CatalogError::Invalid("empty tag".into()));
        }
        Ok(Self(out))
    }

    /// Parse Lightroom form (`A|B|C`).
    pub fn parse_lightroom(s: &str) -> Result<Self, CatalogError> {
        Self::new(s.split('|'))
    }

    /// Parse digiKam form (`A/B/C`).
    pub fn parse_digikam(s: &str) -> Result<Self, CatalogError> {
        Self::new(s.split('/'))
    }

    pub fn segments(&self) -> &[String] {
        &self.0
    }

    pub fn leaf(&self) -> &str {
        self.0.last().map(String::as_str).unwrap_or_default()
    }

    pub fn to_lightroom(&self) -> String {
        self.0.join("|")
    }

    pub fn to_digikam(&self) -> String {
        self.0.join("/")
    }
}

impl fmt::Display for TagPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_lightroom())
    }
}

/// A tag attached to an asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetTag {
    pub path: TagPath,
    pub source: TagSource,
    /// Required for AI tags, absent for user tags.
    pub confidence: Option<f64>,
}

impl AssetTag {
    pub fn user(path: TagPath) -> Self {
        Self {
            path,
            source: TagSource::User,
            confidence: None,
        }
    }

    pub fn ai(path: TagPath, confidence: f64) -> Self {
        Self {
            path,
            source: TagSource::Ai,
            confidence: Some(confidence),
        }
    }
}

/// A non-destructive edit stored as a JSON file in the library's `recipes/` folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditRecipe {
    pub id: RecipeId,
    pub asset_id: AssetId,
    pub kind: String,
    pub schema_version: u32,
    /// Relative to the library's `recipes/` directory.
    pub rel_path: RelPath,
    pub content_hash: Blake3Hash,
    pub created_ms: i64,
    pub updated_ms: i64,
}

/// A cached derived file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rendition {
    pub id: RenditionId,
    pub source_hash: Blake3Hash,
    pub recipe_hash: Blake3Hash,
    pub kind: RenditionKind,
    /// Relative to the rendition cache root.
    pub rel_path: RelPath,
    pub size_bytes: u64,
    pub created_ms: i64,
    pub last_used_ms: i64,
}

/// One import run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportSession {
    pub id: ImportSessionId,
    pub source_volume_id: VolumeId,
    pub started_ms: i64,
    pub finished_ms: Option<i64>,
    pub status: ImportStatus,
    pub verified: bool,
    /// File-naming template used by this session.
    pub template: Option<String>,
    /// Every selected asset is verified on two destinations.
    pub safe_to_format: bool,
}

/// One asset of an import run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportAsset {
    pub session_id: ImportSessionId,
    /// `rel_path_key` of the source original.
    pub source_key: String,
    pub asset_id: AssetId,
    pub diff: ImportDiff,
    pub state: ImportAssetState,
    pub error: Option<String>,
    pub attempts: u32,
    pub updated_ms: i64,
}

/// One destination of an import run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportDestination {
    pub session_id: ImportSessionId,
    pub volume_id: VolumeId,
    pub verified: bool,
}

/// One file × destination of an import run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportItem {
    pub session_id: ImportSessionId,
    pub source_rel_path: RelPath,
    pub dest_volume_id: VolumeId,
    /// The asset (by source original key) this file belongs to.
    pub source_key: String,
    pub role: FileRole,
    pub segment_index: Option<u32>,
    pub size_bytes: u64,
    /// xxh3 of size + first and last MiB.
    pub quick_hash: u64,
    /// Final name on the destination, decided before copying.
    pub dest_rel_path: RelPath,
    pub file_id: Option<FileId>,
    /// BLAKE3 of the source stream.
    pub blake3: Option<Blake3Hash>,
    /// BLAKE3 read back from the destination.
    pub verified_blake3: Option<Blake3Hash>,
    pub status: ImportItemStatus,
    pub error: Option<String>,
}

/// A burst, bracket or interval run of stills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StillSequence {
    pub id: SequenceId,
    pub volume_id: VolumeId,
    pub kind: SequenceKind,
    /// Asset ids in capture order.
    pub members: Vec<AssetId>,
}

/// Where an asset's sidecar was last read or written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SidecarRecord {
    pub asset_id: AssetId,
    /// Volume holding the copy of the original this sidecar describes.
    pub volume_id: VolumeId,
    pub location: SidecarLocation,
    /// `Beside`: relative to the original's volume. `Mirror`: relative to
    /// `<library>/sidecars/<volume-uuid>/`.
    pub rel_path: RelPath,
    pub blake3: Blake3Hash,
    pub mtime_ms: i64,
}

/// What the `ai.analyze` job made from an asset's thumbnail (ADR-035).
#[derive(Debug, Clone, PartialEq)]
pub struct AiAnalysis {
    pub asset_id: AssetId,
    /// Model id and version, e.g. `siglip2-b16-224@1`.
    pub model: String,
    /// The image embedding, int8; times `emb_scale` gives the unit vector.
    pub embedding: Vec<i8>,
    pub emb_scale: f32,
    /// 64-bit difference hash of the thumbnail.
    pub dhash: u64,
    pub made_ms: i64,
}

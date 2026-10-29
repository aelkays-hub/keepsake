//! A Keepsake library folder: catalog, volume registry, recipes and mirrored sidecars.
//!
//! ```text
//! library-<uuid>/
//!   library.json      identity
//!   volumes.json      durable volume registry (survives catalog deletion)
//!   catalog.db        SQLite catalog (rebuildable)
//!   recipes/          JSON edit recipes
//!   sidecars/<volume-uuid>/…  sidecars for read-only or offline volumes
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::catalog::{now_ms, Catalog, CatalogError, File, Volume, VolumeId, VolumeKind};
use crate::relpath::{CaseSensitivity, PathError, RelPath};
use crate::xmp::XmpError;

/// Errors from library-level operations (metadata edits, rebuild).
#[derive(Debug, Error)]
pub enum LibraryError {
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error(transparent)]
    Xmp(#[from] XmpError),
    #[error(transparent)]
    Path(#[from] PathError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("volume {0} is not registered in this library")]
    UnknownVolume(VolumeId),
    #[error("volume {0} is offline")]
    VolumeOffline(VolumeId),
    #[error("not a Keepsake library: {0}")]
    NotALibrary(String),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("stopped before finishing")]
    Cancelled,
}

/// What a library keeps at its root. When a library sits inside a folder of
/// photos (made there by mistake, say), a scan of that folder leaves these out.
pub const LIBRARY_ENTRIES: &[&str] = &[
    "library.json",
    "volumes.json",
    "catalog.db",
    "catalog.db-wal",
    "catalog.db-shm",
    "catalog.db-journal",
    "recipes",
    "sidecars",
    "history.json",
    "cards.json",
    "cards",
    "presets.json",
];

/// Does `dir` hold a Keepsake library?
pub fn is_library_dir(dir: &Path) -> bool {
    dir.join("library.json").is_file()
        && (dir.join("catalog.db").is_file() || dir.join("volumes.json").is_file())
}

/// Is `name` one of the entries a library keeps at its root?
pub fn is_library_entry(name: &str) -> bool {
    LIBRARY_ENTRIES.iter().any(|e| e.eq_ignore_ascii_case(name))
}

/// The durable description of a volume (everything except online state).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeRecord {
    pub id: VolumeId,
    pub label: String,
    pub fs_id: String,
    pub kind: VolumeKind,
    pub case: CaseSensitivity,
    pub read_only: bool,
    /// Where the volume was last mounted. A hint for finding it again, not
    /// part of its identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_mount: Option<PathBuf>,
}

impl VolumeRecord {
    /// A catalog row for this volume, online at `mount` when given.
    pub fn to_volume(&self, mount: Option<&Path>) -> Volume {
        Volume {
            id: self.id,
            label: self.label.clone(),
            fs_id: self.fs_id.clone(),
            kind: self.kind,
            case: self.case,
            read_only: self.read_only,
            online: mount.is_some(),
            mount_path: mount.map(Path::to_path_buf),
            last_seen_ms: mount.map(|_| now_ms()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LibraryFile {
    id: uuid::Uuid,
    created_ms: i64,
}

/// An open library folder.
#[derive(Debug, Clone)]
pub struct Library {
    root: PathBuf,
    id: uuid::Uuid,
}

impl Library {
    /// Create a new library in `root` (which may exist but must not already be a library).
    pub fn create(root: &Path) -> Result<Self, LibraryError> {
        std::fs::create_dir_all(root)?;
        let meta = root.join("library.json");
        if meta.exists() {
            return Err(LibraryError::Invalid(format!(
                "{} already contains a library",
                root.display()
            )));
        }
        let lf = LibraryFile {
            id: uuid::Uuid::new_v4(),
            created_ms: now_ms(),
        };
        write_json_atomic(&meta, &lf)?;
        let lib = Self {
            root: root.to_path_buf(),
            id: lf.id,
        };
        std::fs::create_dir_all(lib.recipes_dir())?;
        std::fs::create_dir_all(lib.sidecars_dir())?;
        write_json_atomic(&lib.volumes_file(), &Vec::<VolumeRecord>::new())?;
        Ok(lib)
    }

    /// Open an existing library.
    pub fn open(root: &Path) -> Result<Self, LibraryError> {
        let meta = root.join("library.json");
        let bytes = std::fs::read(&meta)
            .map_err(|_| LibraryError::NotALibrary(root.display().to_string()))?;
        let lf: LibraryFile = serde_json::from_slice(&bytes)?;
        Ok(Self {
            root: root.to_path_buf(),
            id: lf.id,
        })
    }

    pub fn id(&self) -> uuid::Uuid {
        self.id
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn catalog_path(&self) -> PathBuf {
        self.root.join("catalog.db")
    }

    pub fn recipes_dir(&self) -> PathBuf {
        self.root.join("recipes")
    }

    pub fn sidecars_dir(&self) -> PathBuf {
        self.root.join("sidecars")
    }

    fn volumes_file(&self) -> PathBuf {
        self.root.join("volumes.json")
    }

    /// Open (creating or migrating) this library's catalog.
    pub fn open_catalog(&self) -> Result<Catalog, LibraryError> {
        let mut cat = Catalog::open(&self.catalog_path())?;
        let id = self.id.to_string();
        cat.write(|tx| -> Result<(), CatalogError> {
            if tx.meta().get("library_id")?.is_none() {
                tx.meta().set("library_id", &id)?;
                tx.meta().set("created_ms", &now_ms().to_string())?;
            }
            tx.meta().set("last_app_version", crate::version())
        })?;
        Ok(cat)
    }

    /// All registered volumes.
    pub fn volumes(&self) -> Result<Vec<VolumeRecord>, LibraryError> {
        match std::fs::read(self.volumes_file()) {
            Ok(b) => Ok(serde_json::from_slice(&b)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn volume(&self, id: VolumeId) -> Result<VolumeRecord, LibraryError> {
        self.volumes()?
            .into_iter()
            .find(|v| v.id == id)
            .ok_or(LibraryError::UnknownVolume(id))
    }

    /// Find the catalogued file at an absolute path on an online volume.
    pub fn locate(&self, cat: &Catalog, path: &Path) -> Result<Option<File>, LibraryError> {
        let path = std::fs::canonicalize(path)?;
        for v in cat.read(|tx| tx.volumes().list())? {
            let Some(mount) = &v.mount_path else { continue };
            if let Ok(rel) = RelPath::from_fs(mount, &path) {
                return Ok(cat.read(|tx| tx.files().by_location(v.id, &rel))?);
            }
        }
        Ok(None)
    }

    /// Add or replace a volume in the registry.
    pub fn register_volume(&self, rec: &VolumeRecord) -> Result<(), LibraryError> {
        let mut all = self.volumes()?;
        all.retain(|v| v.id != rec.id);
        if let Some(other) = all.iter().find(|v| v.fs_id == rec.fs_id) {
            return Err(LibraryError::Invalid(format!(
                "file-system id {} already belongs to volume {}",
                rec.fs_id, other.id
            )));
        }
        all.push(rec.clone());
        all.sort_by_key(|v| v.id);
        write_json_atomic(&self.volumes_file(), &all)
    }
}

/// Write JSON through a temporary file and rename.
pub(crate) fn write_json_atomic<T: Serialize>(path: &Path, v: &T) -> Result<(), LibraryError> {
    let bytes = serde_json::to_vec_pretty(v)?;
    write_bytes_atomic(path, &bytes)
}

pub(crate) fn write_bytes_atomic(path: &Path, bytes: &[u8]) -> Result<(), LibraryError> {
    use std::io::Write;
    let dir = path
        .parent()
        .ok_or_else(|| LibraryError::Invalid(format!("{} has no parent", path.display())))?;
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
    let res = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    Ok(res?)
}

/// What a quick probe of a mounted volume found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeProbe {
    pub case: CaseSensitivity,
    pub read_only: bool,
}

/// Whether the file system at `dir` ignores case, found without writing
/// anything: look the folder up under its name with the case swapped.
/// A drive root, or a name with no letters, gets the platform default
/// (Windows and macOS ignore case).
pub fn case_of(dir: &Path) -> CaseSensitivity {
    let (Some(parent), Some(name)) = (dir.parent(), dir.file_name()) else {
        return CaseSensitivity::Insensitive;
    };
    let name = name.to_string_lossy();
    let swapped: String = name
        .chars()
        .map(|c| {
            if c.is_lowercase() {
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c.to_lowercase().next().unwrap_or(c)
            }
        })
        .collect();
    if swapped == name {
        return CaseSensitivity::Insensitive;
    }
    CaseSensitivity::from_bool(!parent.join(swapped).is_dir())
}

/// Detect whether a mounted volume is writable and case-sensitive by creating
/// (and removing) a small hidden probe file at its root. Never touches media.
pub fn probe_volume(mount: &Path) -> VolumeProbe {
    let name = format!(".keepsake-probe-{}", uuid::Uuid::new_v4().simple());
    let lower = mount.join(&name);
    match std::fs::File::create(&lower) {
        Ok(f) => {
            drop(f);
            let upper = mount.join(name.to_uppercase());
            let insensitive = upper.exists();
            let _ = std::fs::remove_file(&lower);
            VolumeProbe {
                case: CaseSensitivity::from_bool(!insensitive),
                read_only: false,
            }
        }
        // Cannot write: assume the platform default (Windows and macOS are
        // case-insensitive by default).
        Err(_) => VolumeProbe {
            case: CaseSensitivity::Insensitive,
            read_only: true,
        },
    }
}

//! Where an item's original file is right now, for "Show in Explorer" /
//! "Show in Finder". The catalog keeps paths relative to a volume; this
//! joins one to the volume's current mount point. Only reads: the file is
//! never touched.

use std::path::PathBuf;

use crate::catalog::{AssetId, Catalog, FileRole};
use crate::library::{Library, LibraryError};

/// Where the original is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginalLocation {
    /// On a connected drive, at this absolute path.
    Online(PathBuf),
    /// No drive holding it is connected; `volume` is the first one's label.
    Offline { volume: String },
    /// Its drive is connected but the file is not where the catalog says
    /// (moved or deleted outside Keepsake); the path it should be at.
    Missing(PathBuf),
}

/// Find the item's original file. With copies on several volumes, the first
/// connected one that has the file wins.
pub fn original_location(
    cat: &Catalog,
    lib: &Library,
    asset: AssetId,
) -> Result<OriginalLocation, LibraryError> {
    cat.read(|tx| tx.assets().require(asset))?;
    let files = cat.read(|tx| tx.files().for_asset(asset))?;
    let mut originals: Vec<_> = files
        .iter()
        .filter(|f| f.role == FileRole::Original)
        .collect();
    if originals.is_empty() {
        // An item with no file marked original: any of its files will do.
        originals = files.iter().collect();
    }
    let Some(first) = originals.first() else {
        return Err(LibraryError::Invalid("the item has no files".into()));
    };
    let mut missing = None;
    for f in &originals {
        let Some(mount) = crate::verify::mount_of(cat, lib, f.volume_id) else {
            continue;
        };
        let path = f.rel_path.to_fs_path(&mount);
        if path.is_file() {
            return Ok(OriginalLocation::Online(path));
        }
        missing.get_or_insert(path);
    }
    if let Some(p) = missing {
        return Ok(OriginalLocation::Missing(p));
    }
    let volume = cat
        .read(|tx| tx.volumes().get(first.volume_id))?
        .map(|v| v.label)
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| "the drive".into());
    Ok(OriginalLocation::Offline { volume })
}

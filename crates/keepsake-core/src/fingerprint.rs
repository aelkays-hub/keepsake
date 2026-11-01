//! Full fingerprints, made in the background, for files added in place
//! (ADR-029).
//!
//! Adding a folder reads only the first and last MiB of each file (the quick
//! hash), so a large folder on a network drive is in the library in minutes
//! instead of hours. Its files get a stand-in key ([`crate::catalog::FileHash::Pending`]).
//! A [`JOB_FINGERPRINT`] job per asset then reads each such file in full,
//! checks it is still the file that was catalogued (size, modification time
//! and quick hash), and stores its BLAKE3; its renditions move to that key.

use std::path::Path;

use thiserror::Error;

use crate::catalog::{AssetId, Blake3Hash, Catalog, CatalogError, File, NewJob, Priority};
use crate::library::Library;

/// Read one asset's files in full and store their fingerprints.
pub const JOB_FINGERPRINT: &str = "fingerprint";

#[derive(Debug, Error)]
pub enum FingerprintError {
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The file's drive is not connected (tried again later).
    #[error("the drive is not connected")]
    Offline,
    /// The file is not the one that was catalogued.
    #[error("{0} changed since it was added; scan its folder again")]
    Changed(String),
}

/// Queue a job for every asset with a file not yet read in full. Jobs that
/// finished are queued again (a scan may have found the file changed).
/// Returns how many jobs were added or queued again.
pub fn enqueue_pending(cat: &mut Catalog) -> Result<u64, CatalogError> {
    let ids = cat.read(|tx| tx.files().assets_unfingerprinted())?;
    let now = crate::jobs::now_ms();
    let mut n = 0;
    for chunk in ids.chunks(2000) {
        n += cat.write(|tx| {
            let jobs: Vec<NewJob<'_>> = chunk
                .iter()
                .enumerate()
                .map(|(i, id)| NewJob {
                    kind: JOB_FINGERPRINT,
                    asset_id: *id,
                    rank: 0,
                    // Already newest first.
                    sort_key: i as i64,
                })
                .collect();
            tx.jobs().enqueue(&jobs, Priority::Backlog, now, true)
        })?;
    }
    Ok(n)
}

/// Fingerprint the asset's files that do not have one yet. Returns how many
/// were stored. Files on a drive that is not connected are left for later
/// ([`FingerprintError::Offline`], after the others are done).
pub fn fingerprint_asset(
    cat: &mut Catalog,
    lib: &Library,
    asset: AssetId,
) -> Result<u64, FingerprintError> {
    let files = cat.read(|tx| tx.files().for_asset(asset))?;
    let mut done = 0;
    let mut offline = false;
    for f in files.iter().filter(|f| !f.hash.is_full()) {
        let Some(mount) = crate::verify::mount_of(cat, lib, f.volume_id) else {
            offline = true;
            continue;
        };
        let path = f.rel_path.to_fs_path(&mount);
        let hash = read_unchanged(&path, f)?;
        if cat.write(|tx| tx.files().set_fingerprint(f.id, &hash))? {
            done += 1;
        }
    }
    if offline {
        return Err(FingerprintError::Offline);
    }
    tracing::debug!(%asset, files = done, "fingerprinted");
    Ok(done)
}

/// The BLAKE3 of `path`, if it is still the file catalogued as `f`.
fn read_unchanged(path: &Path, f: &File) -> Result<Blake3Hash, FingerprintError> {
    let changed = || FingerprintError::Changed(f.rel_path.to_string());
    let same = |m: &std::fs::Metadata| {
        m.len() == f.size_bytes
            && (f.mtime_ms.is_none() || crate::rebuild::mtime_ms(m) == f.mtime_ms)
    };
    let before = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(changed()),
        Err(e) => return Err(e.into()),
    };
    if !same(&before) {
        return Err(changed());
    }
    if let Some(want) = f.xxh3 {
        if crate::ingest::source::quick_hash(path)? != want {
            return Err(changed());
        }
    }
    let hash = crate::rebuild::hash_file(path)?;
    if !same(&std::fs::metadata(path)?) {
        return Err(changed());
    }
    Ok(hash)
}

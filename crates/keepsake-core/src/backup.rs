//! Incremental, verified backup of the library to another drive.
//!
//! Every asset's files are copied from an online volume that holds them to
//! the backup volume, keeping their relative paths, through the same safe
//! copy path as ingest (hidden temporary, fsync, read back past the cache,
//! rename without replacing, read the final file again). A copy is made only
//! when the source still matches the catalog's BLAKE3, so damage is never
//! propagated. Files already on the backup with the same content are skipped.
//! The library itself (a consistent catalog snapshot, the volume registry,
//! recipes, mirrored sidecars, card memory and presets) goes to `<dest>/.keepsake-library/`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::catalog::{AssetId, Catalog, File, NewFile, VolumeId};
use crate::ingest::fault::FaultPlan;
use crate::ingest::source::SourceVolume;
use crate::ingest::{copy, Destination, IngestError};
use crate::library::Library;
use crate::relpath::RelPath;

/// Temporary files of a backup carry this tag (so a later run can clean them).
const TAG: &str = "backup";

/// Folder (hidden) on the backup drive holding the library copy.
pub const LIBRARY_DIR: &str = ".keepsake-library";

/// Progress callbacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupProgress<'a> {
    Planned { files: u64, bytes: u64 },
    FileStarted { path: &'a str, bytes: u64 },
    FileDone { path: &'a str },
}

/// What a backup did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BackupReport {
    pub destination: String,
    pub assets: u64,
    pub files_copied: u64,
    pub bytes_copied: u64,
    pub files_already_there: u64,
    /// `(path, error)`.
    pub failed: Vec<(String, String)>,
    /// Originals of assets with no online copy to back up from.
    pub unavailable: Vec<String>,
    pub library_snapshot: Option<PathBuf>,
    pub stale_temporaries_removed: u64,
    pub cancelled: bool,
}

struct Job {
    asset: AssetId,
    src_mount: PathBuf,
    file: File,
    target: RelPath,
}

fn same_logical(a: &File, b: &File) -> bool {
    a.role == b.role
        && a.blake3 == b.blake3
        && a.rel_path.file_name_nfc() == b.rel_path.file_name_nfc()
}

fn with_suffix(rel: &RelPath, n: u32) -> RelPath {
    if n <= 1 {
        return rel.clone();
    }
    let name = rel.file_name();
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_owned(), format!(".{e}")),
        None => (name.to_owned(), String::new()),
    };
    let file = format!("{stem} ({n}){ext}");
    match rel.parent() {
        Some(p) => p.join(&file).unwrap_or_else(|_| rel.clone()),
        None => RelPath::parse(&file).unwrap_or_else(|_| rel.clone()),
    }
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if !from.is_dir() {
        return Ok(());
    }
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dst = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &dst)?;
        } else {
            std::fs::copy(e.path(), dst)?;
        }
    }
    Ok(())
}

/// Copy the library folder (catalog snapshot, registry, recipes, mirrored
/// sidecars) to `<dest>/.keepsake-library`, replacing the previous copy.
fn snapshot_library(cat: &Catalog, lib: &Library, dest: &Path) -> Result<PathBuf, IngestError> {
    let final_dir = dest.join(LIBRARY_DIR);
    let tmp = dest.join(format!("{LIBRARY_DIR}.partial"));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    std::fs::create_dir_all(&tmp)?;
    cat.snapshot_to(&tmp.join("catalog.db"))?;
    for f in [
        "library.json",
        "volumes.json",
        "cards.json",
        "presets.json",
        "history.json",
    ] {
        let p = lib.root().join(f);
        if p.is_file() {
            std::fs::copy(&p, tmp.join(f))?;
        }
    }
    copy_tree(&lib.recipes_dir(), &tmp.join("recipes"))?;
    copy_tree(&lib.sidecars_dir(), &tmp.join("sidecars"))?;
    copy_tree(&lib.root().join("cards"), &tmp.join("cards"))?;
    let old = dest.join(format!("{LIBRARY_DIR}.old"));
    if old.exists() {
        std::fs::remove_dir_all(&old)?;
    }
    if final_dir.exists() {
        std::fs::rename(&final_dir, &old)?;
    }
    std::fs::rename(&tmp, &final_dir)?;
    if old.exists() {
        std::fs::remove_dir_all(&old)?;
    }
    Ok(final_dir)
}

fn remove_stale_temps(dest: &Path) -> u64 {
    let mut n = 0;
    for p in crate::ingest::leftovers(dest) {
        let ours = p
            .file_name()
            .and_then(|f| f.to_str())
            .is_some_and(|f| f.ends_with(&format!(".keepsake-partial-{TAG}")));
        if ours && std::fs::remove_file(&p).is_ok() {
            n += 1;
        }
    }
    n
}

/// Back the library up to `dest` (a registered, writable destination).
pub fn run(
    cat: &mut Catalog,
    lib: &Library,
    dest: &Destination,
    fault: &FaultPlan,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(&BackupProgress<'_>),
) -> Result<BackupReport, IngestError> {
    let mut report = BackupReport {
        destination: dest.volume.label.clone(),
        ..Default::default()
    };
    // Mounts of every other online volume, and overlap checks.
    let mut mounts: HashMap<VolumeId, PathBuf> = HashMap::new();
    for v in cat.read(|tx| tx.volumes().list())? {
        if v.id == dest.volume.id {
            continue;
        }
        if let Some(m) = crate::verify::mount_of(cat, lib, v.id) {
            let m = std::fs::canonicalize(&m).unwrap_or(m);
            if dest.mount.starts_with(&m) || m.starts_with(&dest.mount) {
                return Err(IngestError::Unsafe(format!(
                    "backup folder {} overlaps volume {} ({})",
                    dest.mount.display(),
                    v.label,
                    m.display()
                )));
            }
            mounts.insert(v.id, m);
        }
    }
    let lib_root = std::fs::canonicalize(lib.root()).unwrap_or_else(|_| lib.root().to_path_buf());
    if dest.mount.starts_with(&lib_root) || lib_root.starts_with(&dest.mount) {
        return Err(IngestError::Unsafe(
            "the backup folder overlaps the library folder".into(),
        ));
    }
    cat.write(|tx| {
        tx.volumes()
            .upsert(&dest.volume.to_volume(Some(&dest.mount)))
    })?;
    report.stale_temporaries_removed = remove_stale_temps(&dest.mount);

    // Plan.
    let ids = cat.read(|tx| tx.assets().list_ids())?;
    report.assets = ids.len() as u64;
    let mut taken: HashMap<String, AssetId> = cat
        .read(|tx| tx.files().for_volume(dest.volume.id))?
        .into_iter()
        .map(|f| (f.rel_path.key(dest.volume.case), f.asset_id))
        .collect();
    let mut jobs: Vec<Job> = Vec::new();
    for id in &ids {
        let files = cat.read(|tx| tx.files().for_asset(*id))?;
        let on_dest: Vec<&File> = files
            .iter()
            .filter(|f| f.volume_id == dest.volume.id)
            .collect();
        let mut by_vol: BTreeMap<VolumeId, Vec<&File>> = BTreeMap::new();
        for f in files.iter().filter(|f| f.volume_id != dest.volume.id) {
            by_vol.entry(f.volume_id).or_default().push(f);
        }
        // Back up from the online volume holding the most files of this asset.
        let primary = by_vol
            .iter()
            .filter(|(v, _)| mounts.contains_key(v))
            .max_by_key(|(v, fs)| (fs.len(), std::cmp::Reverse(**v)));
        let Some((pvol, pfiles)) = primary else {
            let complete = by_vol.values().map(Vec::len).max().unwrap_or(0) <= on_dest.len();
            if !complete {
                let name = files
                    .iter()
                    .find(|f| f.role == crate::catalog::FileRole::Original)
                    .map(|f| f.rel_path.to_string())
                    .unwrap_or_else(|| id.to_string());
                report.unavailable.push(name);
            }
            continue;
        };
        let needed: Vec<&File> = pfiles
            .iter()
            .copied()
            .filter(|f| !on_dest.iter().any(|d| same_logical(d, f)))
            .collect();
        report.files_already_there += (pfiles.len() - needed.len()) as u64;
        if needed.is_empty() {
            continue;
        }
        // Same relative paths if free (on disk and in the catalog), else a suffix.
        let mut n = 1;
        let targets = loop {
            let t: Vec<RelPath> = needed.iter().map(|f| with_suffix(&f.rel_path, n)).collect();
            let free = t.iter().all(|r| {
                !taken.contains_key(&r.key(dest.volume.case)) && !r.to_fs_path(&dest.mount).exists()
            });
            if free || n > 9999 {
                break t;
            }
            n += 1;
        };
        for (f, t) in needed.into_iter().zip(targets) {
            taken.insert(t.key(dest.volume.case), *id);
            jobs.push(Job {
                asset: *id,
                src_mount: mounts[pvol].clone(),
                file: f.clone(),
                target: t,
            });
        }
    }
    progress(&BackupProgress::Planned {
        files: jobs.len() as u64,
        bytes: jobs.iter().map(|j| j.file.size_bytes).sum(),
    });

    // Copy, file by file (Ctrl-C stops between files).
    let mut touched: Vec<AssetId> = Vec::new();
    for job in &jobs {
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let label = job.target.to_string();
        progress(&BackupProgress::FileStarted {
            path: &label,
            bytes: job.file.size_bytes,
        });
        match copy_one(cat, dest, job, fault) {
            Ok(()) => {
                report.files_copied += 1;
                report.bytes_copied += job.file.size_bytes;
                if !touched.contains(&job.asset) {
                    touched.push(job.asset);
                }
            }
            Err(IngestError::SimulatedCrash) => return Err(IngestError::SimulatedCrash),
            Err(e) => report.failed.push((label.clone(), e.to_string())),
        }
        progress(&BackupProgress::FileDone { path: &label });
    }
    // Sidecars beside the new copies of originals.
    for a in touched {
        cat.write(|tx| crate::metadata::sync_sidecars(tx, lib, a))?;
    }
    if !report.cancelled {
        report.library_snapshot = Some(snapshot_library(cat, lib, &dest.mount)?);
    }
    Ok(report)
}

fn copy_one(
    cat: &mut Catalog,
    dest: &Destination,
    job: &Job,
    fault: &FaultPlan,
) -> Result<(), IngestError> {
    let src = SourceVolume::new(&job.src_mount)?;
    let fin = job.target.to_fs_path(&dest.mount);
    let tmp = copy::temp_path(&fin, TAG);
    copy::remove_if_exists(&tmp)?;
    let teed = copy::tee(
        &src,
        &job.file.rel_path,
        job.file.size_bytes,
        std::slice::from_ref(&tmp),
        &[dest.volume.id],
        fault,
        &mut |_| {},
    );
    let teed = match teed {
        Ok(t) => t,
        Err(copy::SourceStop::Crash) => return Err(IngestError::SimulatedCrash),
        Err(copy::SourceStop::Read(e)) => {
            copy::remove_if_exists(&tmp)?;
            return Err(IngestError::Io(e));
        }
        Err(copy::SourceStop::Changed(why)) => {
            copy::remove_if_exists(&tmp)?;
            return Err(IngestError::Unsafe(why));
        }
    };
    let cleanup = |e: IngestError| -> Result<(), IngestError> {
        copy::remove_if_exists(&tmp)?;
        Err(e)
    };
    if let Some(Err(e)) = teed.writes.into_iter().next() {
        return cleanup(IngestError::Io(e));
    }
    if teed.hash != job.file.blake3 {
        // The library's copy no longer matches its recorded hash: never spread it.
        return cleanup(IngestError::Unsafe(format!(
            "{} on its volume differs from the catalog (run `keepsake verify`)",
            job.file.rel_path
        )));
    }
    let got = copy::rehash(
        &tmp,
        fault.corrupt_verify(dest.volume.id, job.file.rel_path.as_str()),
    )?;
    if got != teed.hash {
        return cleanup(IngestError::Unsafe(format!(
            "{}: read-back did not match",
            fin.display()
        )));
    }
    if let Err(e) = copy::finalize(
        &tmp,
        &fin,
        dest.volume.id,
        job.file.rel_path.as_str(),
        fault,
    ) {
        return cleanup(IngestError::Io(e));
    }
    let again = copy::rehash(
        &fin,
        fault.corrupt_final(dest.volume.id, job.file.rel_path.as_str()),
    )?;
    if again != teed.hash {
        std::fs::rename(&fin, copy::bad_path(&fin, TAG))?;
        return Err(IngestError::Unsafe(format!(
            "{}: verification failed",
            fin.display()
        )));
    }
    cat.write(|tx| {
        tx.files().insert(
            job.asset,
            &NewFile {
                volume_id: dest.volume.id,
                rel_path: job.target.clone(),
                role: job.file.role,
                size_bytes: job.file.size_bytes,
                blake3: teed.hash,
                xxh3: job.file.xxh3,
                mtime_ms: None,
                segment_index: job.file.segment_index,
            },
        )
    })?;
    Ok(())
}

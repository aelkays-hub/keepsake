//! Re-hash catalogued files and report what is missing or changed.
//!
//! Reads only. Files are hashed past the OS cache where possible, so the
//! check reads the disk. Cancellation is honoured between files.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::catalog::{AssetId, Blake3Hash, Catalog, VolumeId};
use crate::library::{Library, LibraryError};

/// What is wrong with one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Problem {
    Missing,
    Changed {
        expected: Blake3Hash,
        actual: Blake3Hash,
    },
    Unreadable {
        error: String,
    },
}

/// One problem file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    pub volume: String,
    pub path: String,
    pub asset: AssetId,
    #[serde(flatten)]
    pub problem: Problem,
}

/// Result for one volume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VolumeResult {
    pub id: VolumeId,
    pub label: String,
    /// `None` when the volume is not mounted (nothing checked).
    pub mount: Option<PathBuf>,
    pub files_checked: u64,
    pub bytes_checked: u64,
    pub ok: u64,
}

/// The whole report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct VerifyReport {
    pub volumes: Vec<VolumeResult>,
    pub issues: Vec<Issue>,
    pub cancelled: bool,
}

impl VerifyReport {
    pub fn missing(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| i.problem == Problem::Missing)
            .count()
    }

    pub fn changed(&self) -> usize {
        self.issues
            .iter()
            .filter(|i| matches!(i.problem, Problem::Changed { .. }))
            .count()
    }

    pub fn offline(&self) -> Vec<&str> {
        self.volumes
            .iter()
            .filter(|v| v.mount.is_none())
            .map(|v| v.label.as_str())
            .collect()
    }
}

/// Progress callbacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyProgress<'a> {
    Volume {
        label: &'a str,
        files: u64,
        bytes: u64,
    },
    File {
        path: &'a str,
        bytes: u64,
    },
}

/// Where a volume is mounted right now, if anywhere.
pub fn mount_of(cat: &Catalog, lib: &Library, id: VolumeId) -> Option<PathBuf> {
    let from_catalog = cat
        .read(|tx| tx.volumes().get(id))
        .ok()
        .flatten()
        .and_then(|v| v.mount_path.filter(|_| v.online));
    from_catalog
        .or_else(|| lib.volume(id).ok().and_then(|r| r.last_mount))
        .filter(|p| p.is_dir())
}

/// Find a volume by label (case-insensitive) or id.
pub fn find_volume(cat: &Catalog, name: &str) -> Result<Option<VolumeId>, LibraryError> {
    let vols = cat.read(|tx| tx.volumes().list())?;
    Ok(vols
        .iter()
        .find(|v| v.id.to_string() == name || v.label.eq_ignore_ascii_case(name))
        .map(|v| v.id))
}

/// Verify one volume (or all when `only` is `None`).
pub fn verify(
    cat: &Catalog,
    lib: &Library,
    only: Option<VolumeId>,
    progress: &mut dyn FnMut(&VerifyProgress<'_>),
    cancel: &AtomicBool,
) -> Result<VerifyReport, LibraryError> {
    let mut report = VerifyReport::default();
    let volumes = cat.read(|tx| tx.volumes().list())?;
    for v in volumes.iter().filter(|v| only.is_none_or_eq(v.id)) {
        let files = cat.read(|tx| tx.files().for_volume(v.id))?;
        if files.is_empty() && only.is_none() {
            continue; // e.g. a card that was only an import source
        }
        let mount = mount_of(cat, lib, v.id);
        let mut res = VolumeResult {
            id: v.id,
            label: v.label.clone(),
            mount: mount.clone(),
            files_checked: 0,
            bytes_checked: 0,
            ok: 0,
        };
        let Some(mount) = mount else {
            report.volumes.push(res);
            continue;
        };
        progress(&VerifyProgress::Volume {
            label: &v.label,
            files: files.len() as u64,
            bytes: files.iter().map(|f| f.size_bytes).sum(),
        });
        for f in &files {
            if cancel.load(Ordering::Relaxed) {
                report.cancelled = true;
                break;
            }
            let path = f.rel_path.to_fs_path(&mount);
            let problem = check(&path, &f.blake3);
            res.files_checked += 1;
            res.bytes_checked += f.size_bytes;
            progress(&VerifyProgress::File {
                path: f.rel_path.as_str(),
                bytes: f.size_bytes,
            });
            match problem {
                None => res.ok += 1,
                Some(p) => report.issues.push(Issue {
                    volume: v.label.clone(),
                    path: f.rel_path.to_string(),
                    asset: f.asset_id,
                    problem: p,
                }),
            }
        }
        report.volumes.push(res);
        if report.cancelled {
            break;
        }
    }
    Ok(report)
}

fn check(path: &Path, want: &Blake3Hash) -> Option<Problem> {
    if !path.exists() {
        return Some(Problem::Missing);
    }
    match crate::ingest::sys::hash_uncached(path, |_, _| {}) {
        Ok(h) if &h == want => None,
        Ok(h) => Some(Problem::Changed {
            expected: *want,
            actual: h,
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(Problem::Missing),
        Err(e) => Some(Problem::Unreadable {
            error: e.to_string(),
        }),
    }
}

trait OptEq {
    fn is_none_or_eq(&self, v: VolumeId) -> bool;
}

impl OptEq for Option<VolumeId> {
    fn is_none_or_eq(&self, v: VolumeId) -> bool {
        self.is_none_or(|o| o == v)
    }
}

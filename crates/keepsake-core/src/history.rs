//! Backup and verification history, in `<library>/history.json`.
//!
//! A library file, not the catalog: it is user history that cannot be
//! rebuilt from media, so it survives `rebuild-catalog --fresh` and is
//! copied into backups. The newest 500 runs are kept.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::catalog::VolumeId;
use crate::library::{write_json_atomic, Library, LibraryError};

const KEEP: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    Backup,
    Verify,
}

/// One backup or verify run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub kind: RunKind,
    pub started_ms: i64,
    pub finished_ms: i64,
    /// Backup destination folder, or the verified volume's mount.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<VolumeId>,
    #[serde(default)]
    pub label: String,
    /// Files copied or already there (backup), or checked (verify).
    #[serde(default)]
    pub files: u64,
    #[serde(default)]
    pub bytes: u64,
    /// Failed, unavailable, missing or changed files.
    #[serde(default)]
    pub problems: u64,
    #[serde(default)]
    pub cancelled: bool,
}

impl Run {
    /// Completed with no problems.
    pub fn ok(&self) -> bool {
        self.problems == 0 && !self.cancelled
    }
}

fn file(lib: &Library) -> PathBuf {
    lib.root().join("history.json")
}

/// All runs, newest first.
pub fn list(lib: &Library) -> Result<Vec<Run>, LibraryError> {
    let mut v: Vec<Run> = match std::fs::read(file(lib)) {
        Ok(b) => serde_json::from_slice(&b)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    v.sort_by_key(|r| std::cmp::Reverse(r.started_ms));
    Ok(v)
}

pub fn record(lib: &Library, run: Run) -> Result<(), LibraryError> {
    let mut all = list(lib)?;
    all.insert(0, run);
    all.sort_by_key(|r| std::cmp::Reverse(r.started_ms));
    all.truncate(KEEP);
    write_json_atomic(&file(lib), &all)
}

/// When `volume` was last verified without problems.
pub fn last_verified(lib: &Library, volume: VolumeId) -> Result<Option<i64>, LibraryError> {
    Ok(list(lib)?
        .into_iter()
        .find(|r| r.kind == RunKind::Verify && r.volume == Some(volume) && r.ok())
        .map(|r| r.finished_ms))
}

/// Record a backup report.
pub fn record_backup(
    lib: &Library,
    started_ms: i64,
    dest: &crate::ingest::Destination,
    rep: &crate::backup::BackupReport,
) -> Result<(), LibraryError> {
    record(
        lib,
        Run {
            kind: RunKind::Backup,
            started_ms,
            finished_ms: crate::jobs::now_ms(),
            destination: Some(dest.mount.clone()),
            volume: Some(dest.volume.id),
            label: rep.destination.clone(),
            files: rep.files_copied + rep.files_already_there,
            bytes: rep.bytes_copied,
            problems: (rep.failed.len() + rep.unavailable.len()) as u64,
            cancelled: rep.cancelled,
        },
    )
}

/// Record each checked volume of a verify report.
pub fn record_verify(
    lib: &Library,
    started_ms: i64,
    rep: &crate::verify::VerifyReport,
) -> Result<(), LibraryError> {
    for v in rep.volumes.iter().filter(|v| v.mount.is_some()) {
        let problems = rep.issues.iter().filter(|i| i.volume == v.label).count() as u64;
        record(
            lib,
            Run {
                kind: RunKind::Verify,
                started_ms,
                finished_ms: crate::jobs::now_ms(),
                destination: v.mount.clone(),
                volume: Some(v.id),
                label: v.label.clone(),
                files: v.files_checked,
                bytes: v.bytes_checked,
                problems,
                cancelled: rep.cancelled,
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_newest_first_and_finds_last_good_verify() {
        let d = tempfile::tempdir().unwrap();
        let lib = Library::create(d.path()).unwrap();
        let v = VolumeId::new_random();
        let run = |kind, t, problems| Run {
            kind,
            started_ms: t,
            finished_ms: t + 5,
            destination: None,
            volume: Some(v),
            label: "B".into(),
            files: 3,
            bytes: 10,
            problems,
            cancelled: false,
        };
        record(&lib, run(RunKind::Verify, 100, 0)).unwrap();
        record(&lib, run(RunKind::Backup, 200, 0)).unwrap();
        record(&lib, run(RunKind::Verify, 300, 2)).unwrap();
        let all = list(&lib).unwrap();
        let order: Vec<_> = all.iter().map(|r| r.started_ms).collect();
        assert_eq!(order, [300, 200, 100]);
        assert_eq!(
            last_verified(&lib, v).unwrap(),
            Some(105),
            "the verify with problems does not count"
        );
        assert_eq!(last_verified(&lib, VolumeId::new_random()).unwrap(), None);
    }
}

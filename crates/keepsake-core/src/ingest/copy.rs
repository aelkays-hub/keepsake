//! Copy one source file to one or more destinations.
//!
//! The source is read once; each chunk is hashed (BLAKE3) and sent to every
//! destination writer thread. Each writer writes a hidden temporary file
//! and flushes it to disk. The caller then re-reads each temporary file,
//! compares hashes, and only then renames it into place.

use std::fs::OpenOptions;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use super::fault::{CrashPoint, FaultPlan};
use super::source::SourceVolume;
use super::sys;
use crate::catalog::{Blake3Hash, VolumeId};
use crate::relpath::RelPath;

const CHUNK: usize = 1 << 20;

/// Why a copy stopped on the source side.
#[derive(Debug)]
pub(crate) enum SourceStop {
    Read(io::Error),
    Changed(String),
    /// A simulated crash: the caller must not clean up.
    Crash,
}

/// Result of streaming a source file into temporary files.
pub(crate) struct Teed {
    pub hash: Blake3Hash,
    pub len: u64,
    /// Per destination slot: `Ok` when the temporary file was fully written and flushed.
    pub writes: Vec<Result<(), io::Error>>,
}

/// Hidden temporary name next to `final_path`.
pub(crate) fn temp_path(final_path: &Path, session: &str) -> PathBuf {
    let name = final_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    final_path.with_file_name(format!(".{name}.keepsake-partial-{session}"))
}

/// Hidden name a final file that failed verification is moved to.
pub(crate) fn bad_path(final_path: &Path, session: &str) -> PathBuf {
    let name = final_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    final_path.with_file_name(format!(".{name}.keepsake-bad-{session}"))
}

fn writer(
    path: &Path,
    rx: mpsc::Receiver<Arc<Vec<u8>>>,
    ready: mpsc::Sender<()>,
    slot: VolumeId,
    label: &str,
    fault: &FaultPlan,
) -> io::Result<()> {
    let opened = (|| {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // create_new: never write over anything, including a stale temporary.
        OpenOptions::new().write(true).create_new(true).open(path)
    })();
    // Tell the reader we are ready (or failed) before it reads the source.
    let _ = ready.send(());
    let mut f = opened?;
    let mut done = 0u64;
    let mut corrupt = fault.corrupt_write(slot, label);
    for chunk in rx {
        if let Some(e) = fault.dest_write(slot, label, done) {
            return Err(e);
        }
        if corrupt && !chunk.is_empty() {
            let mut c = chunk.as_ref().clone();
            c[0] ^= 0xFF;
            f.write_all(&c)?;
            corrupt = false;
        } else {
            f.write_all(&chunk)?;
        }
        done += chunk.len() as u64;
    }
    f.sync_all()
}

/// Stream `rel` from the source into every path in `temps`; `slots[i]` is
/// the destination number of `temps[i]` (for fault injection).
/// Never interrupted by Ctrl-C: callers check for cancellation between files.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tee(
    src: &SourceVolume,
    rel: &RelPath,
    expected_len: u64,
    temps: &[PathBuf],
    slots: &[VolumeId],
    fault: &FaultPlan,
    progress: &mut dyn FnMut(u64),
) -> Result<Teed, SourceStop> {
    let label = rel.as_str().to_owned();
    let before = src.metadata(rel).map_err(SourceStop::Read)?;
    if before.len() != expected_len {
        return Err(SourceStop::Changed(format!(
            "size is {} bytes, {} expected",
            before.len(),
            expected_len
        )));
    }
    let mut file = src.open(rel).map_err(SourceStop::Read)?;
    let mut hasher = blake3::Hasher::new();
    let mut len = 0u64;

    let outcome = std::thread::scope(|s| {
        let mut senders = Vec::new();
        let mut handles = Vec::new();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();
        for (path, &slot) in temps.iter().zip(slots) {
            let (tx, rx) = mpsc::sync_channel::<Arc<Vec<u8>>>(4);
            senders.push(Some(tx));
            let label = label.clone();
            let ready = ready_tx.clone();
            handles.push(s.spawn(move || writer(path, rx, ready, slot, &label, fault)));
        }
        drop(ready_tx);
        // Every temporary exists (or has failed) before a byte is read, so a
        // crash mid-copy always leaves its temporaries for resume to clear.
        for _ in 0..temps.len() {
            if ready_rx.recv().is_err() {
                break;
            }
        }
        let mut stop: Option<SourceStop> = None;
        let mut buf = vec![0u8; CHUNK];
        loop {
            if let Some(e) = fault.source_read(&label, len) {
                stop = Some(SourceStop::Read(e));
                break;
            }
            let n = match file.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    stop = Some(SourceStop::Read(e));
                    break;
                }
            };
            hasher.update(&buf[..n]);
            len += n as u64;
            let chunk = Arc::new(buf[..n].to_vec());
            for tx in senders.iter_mut() {
                // A writer that failed has hung up; keep feeding the others.
                if tx
                    .as_ref()
                    .is_some_and(|t| t.send(Arc::clone(&chunk)).is_err())
                {
                    *tx = None;
                }
            }
            progress(len);
            if fault.crash(&CrashPoint::MidCopy {
                file: label.clone(),
                after_bytes: len,
            }) {
                stop = Some(SourceStop::Crash);
                break;
            }
        }
        drop(senders);
        let writes: Vec<io::Result<()>> = handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err(io::Error::other("writer thread panicked")))
            })
            .collect();
        (stop, writes)
    });

    let (stop, writes) = outcome;
    if let Some(s) = stop {
        return Err(s);
    }
    let after = src.metadata(rel).map_err(SourceStop::Read)?;
    if after.len() != len || len != expected_len || fault.source_changes(&label) {
        return Err(SourceStop::Changed(format!(
            "file changed while it was read ({len} bytes read, {} now)",
            after.len()
        )));
    }
    if before.modified().ok() != after.modified().ok() {
        return Err(SourceStop::Changed(
            "modification time changed while it was read".into(),
        ));
    }
    Ok(Teed {
        hash: Blake3Hash(*hasher.finalize().as_bytes()),
        len,
        writes,
    })
}

/// Re-read a file past the OS cache and hash it. `corrupt` flips a byte
/// (fault injection).
pub(crate) fn rehash(path: &Path, corrupt: bool) -> io::Result<Blake3Hash> {
    let mut once = corrupt;
    sys::hash_uncached(path, |buf, _| {
        if once && !buf.is_empty() {
            buf[0] ^= 0xFF;
            once = false;
        }
    })
}

/// Rename a verified temporary file to its final name. Refuses to replace an
/// existing file. Retries briefly when something (antivirus, indexer) holds
/// the file open.
pub(crate) fn finalize(
    temp: &Path,
    final_path: &Path,
    slot: VolumeId,
    label: &str,
    fault: &FaultPlan,
) -> io::Result<()> {
    let mut wait = Duration::from_millis(50);
    for attempt in 0..5 {
        if final_path.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", final_path.display()),
            ));
        }
        let r = match fault.rename(slot, label) {
            Some(e) => Err(e),
            None => std::fs::rename(temp, final_path),
        };
        match r {
            Ok(()) => {
                if let Some(dir) = final_path.parent() {
                    sys::sync_dir(dir)?;
                }
                return Ok(());
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::AlreadyExists
                ) =>
            {
                return Err(e)
            }
            Err(e) if attempt == 4 => return Err(e),
            Err(_) => {
                std::thread::sleep(wait);
                wait *= 2;
            }
        }
    }
    Err(io::Error::other("rename retries exhausted"))
}

/// Delete a file if it exists.
pub(crate) fn remove_if_exists(p: &Path) -> io::Result<()> {
    match std::fs::remove_file(p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

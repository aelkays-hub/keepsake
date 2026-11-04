//! Fault injection for ingest tests.
//!
//! A [`FaultPlan`] is passed to the engine explicitly; production callers pass
//! [`FaultPlan::none`]. Each fault fires once (or `times` times) when its file
//! name matches.

use std::io;
use std::sync::{Arc, Mutex};

use crate::catalog::VolumeId;

/// Where a simulated or real crash happens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrashPoint {
    /// After `after_bytes` of `file` were written to the temporary files.
    MidCopy { file: String, after_bytes: u64 },
    /// Right after `file` was renamed into place on any destination,
    /// before the catalog records it.
    AfterRename { file: String },
    /// After an asset (by original file name) is verified, before it is catalogued.
    AfterVerify { file: String },
}

/// How a crash behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashMode {
    /// `std::process::abort()`: the process dies without unwinding or cleanup.
    Abort,
    /// Return [`super::IngestError::SimulatedCrash`] without any cleanup.
    Simulate,
}

/// One injected fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    SourceRead {
        file: String,
        after_bytes: u64,
        kind: io::ErrorKind,
    },
    /// The source file changes size while it is read.
    SourceChanges {
        file: String,
    },
    DestWrite {
        dest: VolumeId,
        file: String,
        after_bytes: u64,
        kind: io::ErrorKind,
    },
    /// Flip one byte while writing to a destination.
    CorruptWrite {
        dest: VolumeId,
        file: String,
    },
    /// Flip one byte while re-reading the temporary file.
    CorruptVerify {
        dest: VolumeId,
        file: String,
        times: u32,
    },
    /// Flip one byte while re-reading the final file in the verify pass.
    CorruptFinal {
        dest: VolumeId,
        file: String,
    },
    Rename {
        dest: VolumeId,
        file: String,
        kind: io::ErrorKind,
        times: u32,
    },
    /// Fail the catalog transaction that records `file`'s asset.
    Catalog {
        file: String,
    },
    Crash {
        at: CrashPoint,
        mode: CrashMode,
    },
}

/// A shared, mutable list of faults (cheap to clone).
#[derive(Debug, Clone, Default)]
pub struct FaultPlan {
    faults: Arc<Mutex<Vec<Fault>>>,
}

fn matches(want: &str, file: &str) -> bool {
    file.ends_with(want)
}

impl FaultPlan {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn with(faults: Vec<Fault>) -> Self {
        Self {
            faults: Arc::new(Mutex::new(faults)),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.faults.lock().map(|f| f.is_empty()).unwrap_or(true)
    }

    /// Remove and return the first fault for which `pick` returns `Some`.
    fn take<T>(&self, mut pick: impl FnMut(&mut Fault) -> Option<(T, bool)>) -> Option<T> {
        let mut g = self.faults.lock().ok()?;
        for i in 0..g.len() {
            if let Some((v, consume)) = pick(&mut g[i]) {
                if consume {
                    g.remove(i);
                }
                return Some(v);
            }
        }
        None
    }

    pub(crate) fn source_read(&self, file: &str, done: u64) -> Option<io::Error> {
        self.take(|f| match f {
            Fault::SourceRead {
                file: w,
                after_bytes,
                kind,
            } if matches(w, file) && done >= *after_bytes => {
                Some((io::Error::new(*kind, "injected source read error"), true))
            }
            _ => None,
        })
    }

    pub(crate) fn source_changes(&self, file: &str) -> bool {
        self.take(|f| match f {
            Fault::SourceChanges { file: w } if matches(w, file) => Some(((), true)),
            _ => None,
        })
        .is_some()
    }

    pub(crate) fn dest_write(&self, dest: VolumeId, file: &str, done: u64) -> Option<io::Error> {
        self.take(|f| match f {
            Fault::DestWrite {
                dest: d,
                file: w,
                after_bytes,
                kind,
            } if *d == dest && matches(w, file) && done >= *after_bytes => {
                Some((io::Error::new(*kind, "injected write error"), true))
            }
            _ => None,
        })
    }

    pub(crate) fn corrupt_write(&self, dest: VolumeId, file: &str) -> bool {
        self.take(|f| match f {
            Fault::CorruptWrite { dest: d, file: w } if *d == dest && matches(w, file) => {
                Some(((), true))
            }
            _ => None,
        })
        .is_some()
    }

    pub(crate) fn corrupt_verify(&self, dest: VolumeId, file: &str) -> bool {
        self.take(|f| match f {
            Fault::CorruptVerify {
                dest: d,
                file: w,
                times,
            } if *d == dest && matches(w, file) => {
                *times = times.saturating_sub(1);
                Some(((), *times == 0))
            }
            _ => None,
        })
        .is_some()
    }

    pub(crate) fn corrupt_final(&self, dest: VolumeId, file: &str) -> bool {
        self.take(|f| match f {
            Fault::CorruptFinal { dest: d, file: w } if *d == dest && matches(w, file) => {
                Some(((), true))
            }
            _ => None,
        })
        .is_some()
    }

    pub(crate) fn rename(&self, dest: VolumeId, file: &str) -> Option<io::Error> {
        self.take(|f| match f {
            Fault::Rename {
                dest: d,
                file: w,
                kind,
                times,
            } if *d == dest && matches(w, file) => {
                *times = times.saturating_sub(1);
                Some((io::Error::new(*kind, "injected rename error"), *times == 0))
            }
            _ => None,
        })
    }

    pub(crate) fn catalog(&self, file: &str) -> bool {
        self.take(|f| match f {
            Fault::Catalog { file: w } if matches(w, file) => Some(((), true)),
            _ => None,
        })
        .is_some()
    }

    /// Fire a crash at `here` if one is planned: aborts the process or
    /// returns `true` (simulate) so the caller stops without cleanup.
    pub(crate) fn crash(&self, here: &CrashPoint) -> bool {
        let hit = self.take(|f| match f {
            Fault::Crash { at, mode } => {
                let fire = match (at, here) {
                    (
                        CrashPoint::MidCopy {
                            file: w,
                            after_bytes,
                        },
                        CrashPoint::MidCopy {
                            file,
                            after_bytes: done,
                        },
                    ) => matches(w, file) && done >= after_bytes,
                    (CrashPoint::AfterRename { file: w }, CrashPoint::AfterRename { file }) => {
                        matches(w, file)
                    }
                    (CrashPoint::AfterVerify { file: w }, CrashPoint::AfterVerify { file }) => {
                        matches(w, file)
                    }
                    _ => false,
                };
                fire.then_some((*mode, true))
            }
            _ => None,
        });
        match hit {
            Some(CrashMode::Abort) => std::process::abort(),
            Some(CrashMode::Simulate) => true,
            None => false,
        }
    }
}

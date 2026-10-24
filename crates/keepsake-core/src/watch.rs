//! The logic of `keepsake watch`: turn device events into card events and,
//! for trusted cards, automatic imports.
//!
//! [`Watcher::handle`] takes one [`DeviceEvent`] at a time and reports what
//! it means as [`WatchEvent`]s. It knows nothing about the OS, so it is
//! tested with scripted events. Imports run one at a time: while one runs,
//! later device events wait in the caller's queue.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::Serialize;

use crate::cards::{self, CardId, CardLayout, CardStatus};
use crate::catalog::{ImportSessionId, VolumeKind};
use crate::devices::DeviceEvent;
use crate::ingest::fault::FaultPlan;
use crate::ingest::template::Template;
use crate::ingest::{
    self, IngestError, IngestReport, PlanOptions, Progress, RunContext, Selection,
};
use crate::library::Library;
use crate::presets::{self, Preset};
use crate::probe::Prober;

/// How the watcher behaves.
#[derive(Debug, Clone, Default)]
pub struct WatchOptions {
    /// Import trusted cards automatically with their preset.
    pub auto_ingest: bool,
    /// Testing only: allow destinations on the same disk (see
    /// [`PlanOptions::allow_same_filesystem`]).
    pub allow_same_filesystem: bool,
}

/// What the watcher reports. Serialised as one JSON object per event with
/// an `event` field, for the app to read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum WatchEvent {
    /// A Sony card is mounted.
    CardInserted {
        card: CardId,
        label: String,
        mount: PathBuf,
        /// Seen by this library before.
        known: bool,
        layouts: Vec<CardLayout>,
        items: usize,
        /// Assets not imported from this card before.
        new_items: usize,
        #[serde(skip_serializing_if = "Option::is_none")]
        last_import_ms: Option<i64>,
        trusted: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        preset: Option<String>,
    },
    /// A card was removed.
    CardRemoved {
        card: CardId,
        label: String,
        mount: PathBuf,
    },
    /// A volume that is not a Sony card was mounted.
    VolumeIgnored { mount: PathBuf, reason: String },
    /// A trusted card was not imported automatically.
    AutoIngestSkipped { card: CardId, reason: String },
    AutoIngestStarted {
        card: CardId,
        session: ImportSessionId,
        preset: String,
        destinations: Vec<PathBuf>,
    },
    AutoIngestProgress {
        card: CardId,
        session: ImportSessionId,
        files_done: u64,
        bytes_done: u64,
    },
    AutoIngestFinished {
        card: CardId,
        session: ImportSessionId,
        report: IngestReport,
        /// Assets still not imported from the card (failed or skipped).
        new_items_left: usize,
    },
    /// Stopped by the user after the current file; `ingest --resume`
    /// continues.
    AutoIngestCancelled {
        card: CardId,
        session: ImportSessionId,
    },
    AutoIngestFailed {
        card: CardId,
        #[serde(skip_serializing_if = "Option::is_none")]
        session: Option<ImportSessionId>,
        error: String,
        /// `keepsake ingest --resume` can continue it.
        resumable: bool,
    },
    /// Something went wrong looking at a volume.
    Error { mount: PathBuf, error: String },
}

/// Mount points compare without case and trailing separators.
fn mount_key(p: &Path) -> String {
    let s = p.to_string_lossy();
    let t = s.trim_end_matches(['/', '\\']);
    let t = if t.is_empty() { s.as_ref() } else { t };
    t.to_lowercase()
}

struct Mounted {
    card: CardId,
    label: String,
    mount: PathBuf,
}

/// Turns device events into [`WatchEvent`]s.
pub struct Watcher<'a> {
    lib: &'a Library,
    prober: Option<&'a Prober>,
    opts: WatchOptions,
    fault: FaultPlan,
    mounted: HashMap<String, Mounted>,
}

impl<'a> Watcher<'a> {
    pub fn new(lib: &'a Library, prober: Option<&'a Prober>, opts: WatchOptions) -> Self {
        Self {
            lib,
            prober,
            opts,
            fault: FaultPlan::default(),
            mounted: HashMap::new(),
        }
    }

    /// Testing only: inject faults into automatic imports.
    pub fn with_fault(mut self, fault: FaultPlan) -> Self {
        self.fault = fault;
        self
    }

    /// Cards currently mounted, as `(card, mount)`.
    pub fn mounted(&self) -> Vec<(CardId, PathBuf)> {
        let mut v: Vec<_> = self
            .mounted
            .values()
            .map(|m| (m.card, m.mount.clone()))
            .collect();
        v.sort();
        v
    }

    /// Handle one device event. `cancel` stops an automatic import after the
    /// current file. Never fails: problems are reported as events.
    pub fn handle(
        &mut self,
        ev: &DeviceEvent,
        cancel: &AtomicBool,
        emit: &mut dyn FnMut(&WatchEvent),
    ) {
        match ev {
            DeviceEvent::Arrived { mount, identity } => {
                let key = mount_key(mount);
                if self.mounted.contains_key(&key) {
                    return; // already handled (backends may repeat arrivals)
                }
                let status = match cards::identify(self.lib, mount, identity) {
                    Ok(Some(s)) => s,
                    Ok(None) => {
                        emit(&WatchEvent::VolumeIgnored {
                            mount: mount.clone(),
                            reason: "not a Sony card".into(),
                        });
                        return;
                    }
                    Err(e) => {
                        emit(&WatchEvent::Error {
                            mount: mount.clone(),
                            error: e.to_string(),
                        });
                        return;
                    }
                };
                self.mounted.insert(
                    key,
                    Mounted {
                        card: status.card.id,
                        label: status.card.label.clone(),
                        mount: mount.clone(),
                    },
                );
                emit(&inserted(&status, mount));
                if self.opts.auto_ingest && status.card.trusted {
                    self.auto_ingest(&status, mount, cancel, emit);
                }
            }
            DeviceEvent::Removed { mount } => {
                if let Some(m) = self.mounted.remove(&mount_key(mount)) {
                    emit(&WatchEvent::CardRemoved {
                        card: m.card,
                        label: m.label,
                        mount: m.mount,
                    });
                }
            }
        }
    }

    fn auto_ingest(
        &self,
        status: &CardStatus,
        mount: &Path,
        cancel: &AtomicBool,
        emit: &mut dyn FnMut(&WatchEvent),
    ) {
        let card = &status.card;
        let skip = |reason: String| WatchEvent::AutoIngestSkipped {
            card: card.id,
            reason,
        };
        let Some(name) = card.preset.as_deref() else {
            emit(&skip("the card has no preset".into()));
            return;
        };
        let preset = match presets::get(self.lib, name) {
            Ok(Some(p)) => p,
            Ok(None) => {
                emit(&skip(format!("preset {name} does not exist")));
                return;
            }
            Err(e) => {
                emit(&skip(format!("cannot read presets: {e}")));
                return;
            }
        };
        if status.new_items == 0 {
            emit(&skip("nothing new on the card".into()));
            return;
        }
        if let Some(d) = preset.destinations.iter().find(|d| !d.is_dir()) {
            emit(&skip(format!(
                "destination {} not found (is the drive connected?)",
                d.display()
            )));
            return;
        }
        let mut session = None;
        let result = self.import(
            card.id,
            &card.key,
            &card.label,
            mount,
            &preset,
            &mut session,
            cancel,
            emit,
        );
        // Whatever happened, assets that made it into the library are no
        // longer new.
        if let Some(s) = session {
            if let Ok(cat) = self.lib.open_catalog() {
                if let Err(e) = cards::record_import(self.lib, &cat, card.id, mount, s) {
                    emit(&WatchEvent::Error {
                        mount: mount.to_path_buf(),
                        error: format!("could not update the card's memory: {e}"),
                    });
                }
            }
        }
        match (result, session) {
            (Ok(report), Some(s)) => {
                let left = cards::status(self.lib, card, mount)
                    .map(|st| st.new_items)
                    .unwrap_or(0);
                emit(&WatchEvent::AutoIngestFinished {
                    card: card.id,
                    session: s,
                    report,
                    new_items_left: left,
                });
            }
            (Err(IngestError::Cancelled), Some(s)) => emit(&WatchEvent::AutoIngestCancelled {
                card: card.id,
                session: s,
            }),
            (Err(e), s) => emit(&WatchEvent::AutoIngestFailed {
                card: card.id,
                session: s,
                error: e.to_string(),
                resumable: s.is_some(),
            }),
            (Ok(_), None) => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn import(
        &self,
        card: CardId,
        key: &str,
        label: &str,
        mount: &Path,
        preset: &Preset,
        session_out: &mut Option<ImportSessionId>,
        cancel: &AtomicBool,
        emit: &mut dyn FnMut(&WatchEvent),
    ) -> Result<IngestReport, IngestError> {
        let mut dests = Vec::new();
        for d in &preset.destinations {
            let label = d
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| d.display().to_string());
            dests.push(ingest::destination_volume(
                self.lib,
                d,
                &label,
                VolumeKind::External,
            )?);
        }
        let opts = PlanOptions {
            template: Template::parse(&preset.template)?,
            selection: Selection::NewOnly,
            event: preset.event.clone(),
            allow_same_filesystem: self.opts.allow_same_filesystem,
            source_id: Some(key.to_owned()),
        };
        let mut cat = self.lib.open_catalog()?;
        let session = ingest::plan(&mut cat, self.lib, self.prober, mount, label, &dests, &opts)?;
        *session_out = Some(session);
        emit(&WatchEvent::AutoIngestStarted {
            card,
            session,
            preset: preset.name.clone(),
            destinations: preset.destinations.clone(),
        });
        let (mut files_done, mut bytes_done, mut current) = (0u64, 0u64, 0u64);
        let mut progress = |p: &Progress| match p {
            Progress::FileStarted { bytes, .. } => current = *bytes,
            Progress::FileDone { .. } => {
                files_done += 1;
                bytes_done += current;
                emit(&WatchEvent::AutoIngestProgress {
                    card,
                    session,
                    files_done,
                    bytes_done,
                });
            }
            _ => {}
        };
        let mut ctx = RunContext {
            prober: self.prober,
            fault: &self.fault,
            cancel,
            progress: &mut progress,
            retry_failed: false,
        };
        ingest::run(&mut cat, self.lib, session, &mut ctx)
    }
}

fn inserted(s: &CardStatus, mount: &Path) -> WatchEvent {
    WatchEvent::CardInserted {
        card: s.card.id,
        label: s.card.label.clone(),
        mount: mount.to_path_buf(),
        known: s.known,
        layouts: s.card.layouts.clone(),
        items: s.items,
        new_items: s.new_items,
        last_import_ms: s.card.last_import_ms,
        trusted: s.card.trusted,
        preset: s.card.preset.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_keys_ignore_case_and_trailing_separator() {
        assert_eq!(mount_key(Path::new("E:\\")), mount_key(Path::new("e:")));
        assert_eq!(
            mount_key(Path::new("/Volumes/Untitled/")),
            mount_key(Path::new("/Volumes/untitled"))
        );
        assert_eq!(mount_key(Path::new("/")), "/");
    }
}

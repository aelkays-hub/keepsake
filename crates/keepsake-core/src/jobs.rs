//! The persistent job queue: long-running work (renditions now, exports
//! later) runs from here, never on the UI or a command thread.
//!
//! Jobs live in the catalog (`jobs` table), so the queue survives restarts:
//! on start, [`recover`] puts jobs that were running back in the queue.
//! Order: [`Priority`] (visible on screen, recent import, backlog), then the
//! kind's rank, then newest capture first. Failures retry with backoff up to
//! [`MAX_ATTEMPTS`] times.
//!
//! Workers pause while an ingest runs anywhere on the library (any process):
//! ingest keeps a heartbeat in the catalog ([`IngestHeartbeat`]).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use serde::Serialize;

use crate::catalog::{Catalog, CatalogError};
pub use crate::catalog::{Job, JobCounts, JobState, NewJob, Priority};
use crate::library::Library;

/// Attempts before a job is marked failed for good.
pub const MAX_ATTEMPTS: u32 = 3;
/// First retry delay; doubled for each further attempt.
pub const RETRY_BASE_MS: i64 = 30_000;

/// When to retry after `attempts` failed attempts (`None`: give up).
pub fn retry_at(attempts: u32, now_ms: i64) -> Option<i64> {
    if attempts >= MAX_ATTEMPTS {
        return None;
    }
    let factor = 1i64 << attempts.saturating_sub(1).min(16);
    Some(now_ms + RETRY_BASE_MS * factor)
}

/// After a crash or restart, running jobs go back to the queue.
pub fn recover(cat: &mut Catalog) -> Result<u64, CatalogError> {
    cat.write(|tx| tx.jobs().recover())
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Why workers are waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    /// The computer runs on battery.
    OnBattery,
    /// An import is copying files (reads and writes compete with it).
    Ingesting,
}

impl PauseReason {
    pub fn describe(self) -> &'static str {
        match self {
            PauseReason::OnBattery => "on battery power",
            PauseReason::Ingesting => "an import is running",
        }
    }
}

const HEARTBEAT_KEY: &str = "ingest_heartbeat_ms";
/// How often a running ingest refreshes its heartbeat.
pub const HEARTBEAT_EVERY: Duration = Duration::from_secs(2);
/// A heartbeat older than this means no ingest is running (it crashed or
/// finished without clearing it).
pub const HEARTBEAT_STALE_MS: i64 = 10_000;

/// Is an ingest running on this library right now (any process)?
pub fn ingest_running(cat: &Catalog, now_ms: i64) -> bool {
    let beat = cat
        .read(|tx| tx.meta().get(HEARTBEAT_KEY))
        .ok()
        .flatten()
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(0);
    beat > 0 && now_ms - beat < HEARTBEAT_STALE_MS
}

fn beat(cat: &mut Catalog, value: i64) {
    let v = value.to_string();
    if let Err(e) = cat.write(|tx| tx.meta().set(HEARTBEAT_KEY, &v)) {
        tracing::warn!("ingest heartbeat: {e}");
    }
}

/// Keeps "an ingest is running" visible to render workers while alive:
/// a thread with its own catalog connection writes the time every
/// [`HEARTBEAT_EVERY`]; dropping it clears the heartbeat.
pub struct IngestHeartbeat {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl IngestHeartbeat {
    /// Start beating. Failures are logged, never fatal: at worst renders
    /// keep running during the import.
    pub fn start(lib: &Library) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let path = lib.catalog_path();
        let flag = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("ingest-heartbeat".into())
            .spawn(move || {
                let Ok(mut cat) = Catalog::open(&path) else {
                    return;
                };
                while !flag.load(Ordering::SeqCst) {
                    beat(&mut cat, now_ms());
                    let mut waited = Duration::ZERO;
                    while waited < HEARTBEAT_EVERY && !flag.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(100));
                        waited += Duration::from_millis(100);
                    }
                }
                beat(&mut cat, 0);
            })
            .ok();
        Self { stop, thread }
    }
}

impl Drop for IngestHeartbeat {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_then_gives_up() {
        assert_eq!(retry_at(1, 0), Some(30_000));
        assert_eq!(retry_at(2, 0), Some(60_000));
        assert_eq!(retry_at(3, 0), None);
    }
}

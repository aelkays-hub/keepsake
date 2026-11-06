//! Render workers on the job queue.
//!
//! [`RenderService::run`] starts the workers and calls back with
//! [`RenderEvent`]s on the calling thread:
//! - `workers` threads take [`super::JOB_STILLS`] jobs, `proxy_workers`
//!   threads take [`super::JOB_PROXY`] jobs, and one thread takes
//!   [`crate::fingerprint::JOB_FINGERPRINT`] jobs; each has its own catalog
//!   connection and claims one job at a time;
//! - fingerprints wait while a thumbnail job is ready: each reads a whole
//!   file, and on a network drive they would take the disk from thumbnails;
//! - before each job a worker checks the pauses (on battery, an ingest is
//!   running) and waits while one applies;
//! - media that is offline is deferred (not counted as an attempt); other
//!   failures retry with backoff, then stay failed;
//! - every few jobs, and at the end, the cache is trimmed: once over its
//!   limit, down to [`EVICT_TO_PCT`] of it;
//! - while the cache is above [`FULL_PCT`] of its limit, jobs not on screen
//!   make thumbnails only (no preview, no proxy): a large library's previews
//!   and proxies do not fit, and making them would only push out the
//!   thumbnails the grid needs. Opening the item makes the rest;
//! - before the workers start, renditions whose file is not at its
//!   content-addressed name are moved there ([`Cache::repair_names`]);
//! - the cancel flag stops workers after their current job.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::cache::{Cache, CacheStats, EvictReport, RepairReport};
use super::source::locate;
use super::{still, video, RenderError, RenderTools, Variant, JOB_PROXY, JOB_STILLS};
use crate::catalog::{
    AssetId, Catalog, CatalogError, Job, JobCounts, JobState, MediaKind, Priority,
};
use crate::fingerprint::{self, FingerprintError, JOB_FINGERPRINT};
use crate::jobs::{self, now_ms, PauseReason};
use crate::library::Library;
use crate::probe::ExifTool;
use crate::settings::RenderSettings;

/// Offline media is looked at again after this long.
const OFFLINE_RETRY_MS: i64 = 10 * 60 * 1000;
/// How soon a job is tried again after the catalog was busy.
const BUSY_RETRY: Duration = Duration::from_secs(2);
/// How long the fingerprint worker waits before looking again while
/// thumbnail jobs are ready.
const FINGERPRINT_WAIT: Duration = Duration::from_secs(5);
/// Once over its limit, the cache is trimmed to this share of it (percent).
pub const EVICT_TO_PCT: u64 = 90;
/// Above this share of its limit (percent), the cache counts as full.
pub const FULL_PCT: u64 = 80;

fn pct_of(max_bytes: u64, pct: u64) -> u64 {
    max_bytes / 100 * pct
}

/// How the service runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions {
    pub workers: usize,
    pub proxy_workers: usize,
    pub max_bytes: u64,
    pub pause_on_battery: bool,
    /// Stop when nothing is ready to run (the CLI); otherwise keep waiting
    /// for new jobs until cancelled (the app).
    pub until_empty: bool,
    /// Use this H.264 encoder instead of detecting one.
    pub forced_encoder: Option<String>,
    /// Trim the cache after this many finished jobs.
    pub evict_every: u64,
}

impl RenderOptions {
    pub fn from_settings(s: &RenderSettings) -> Self {
        Self {
            workers: s.image_workers(),
            proxy_workers: s.proxy_workers,
            max_bytes: s.max_bytes,
            pause_on_battery: s.pause_on_battery,
            until_empty: true,
            forced_encoder: None,
            evict_every: 25,
        }
    }
}

/// Progress, for the CLI and the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RenderEvent {
    Started {
        job: i64,
        kind: String,
        asset: AssetId,
    },
    Done {
        job: i64,
        kind: String,
        asset: AssetId,
        ms: u64,
        /// Variants made (empty: they already existed).
        made: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        encoder: Option<String>,
    },
    Failed {
        job: i64,
        kind: String,
        asset: AssetId,
        error: String,
        /// It will be tried again later.
        retry: bool,
    },
    /// Not counted as an attempt; tried again later.
    Deferred {
        job: i64,
        kind: String,
        asset: AssetId,
        reason: String,
    },
    Paused {
        reason: PauseReason,
    },
    Resumed,
    Evicted {
        report: EvictReport,
    },
    /// Renditions moved to their content-addressed names at start (ADR-034).
    Repaired {
        report: RepairReport,
    },
    Finished {
        done: u64,
        failed: u64,
        cancelled: bool,
    },
}

/// The queue and cache at a glance (`keepsake renders status`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RenderStatus {
    pub jobs: JobCounts,
    pub running: Vec<Job>,
    /// The first failed jobs, with their errors.
    pub failed: Vec<Job>,
    pub cache: CacheStats,
    /// What would pause workers right now.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paused: Option<PauseReason>,
}

/// `true` when the computer runs on battery.
pub type PowerCheck = Arc<dyn Fn() -> bool + Send + Sync>;

fn pause_reason(cat: &Catalog, on_battery: &PowerCheck, battery_rule: bool) -> Option<PauseReason> {
    if battery_rule && on_battery() {
        return Some(PauseReason::OnBattery);
    }
    if jobs::ingest_running(cat, now_ms()) {
        return Some(PauseReason::Ingesting);
    }
    None
}

/// The status of the queue and cache.
pub fn status(
    cat: &Catalog,
    lib: &Library,
    cache: &Cache,
    settings: &RenderSettings,
    on_battery: &PowerCheck,
) -> Result<RenderStatus, RenderError> {
    Ok(RenderStatus {
        jobs: cat.read(|tx| tx.jobs().counts())?,
        running: cat.read(|tx| tx.jobs().list(JobState::Running, 50))?,
        failed: cat.read(|tx| tx.jobs().list(JobState::Failed, 20))?,
        cache: cache.stats(cat, lib, settings.max_bytes)?,
        paused: pause_reason(cat, on_battery, settings.pause_on_battery),
    })
}

/// Runs render jobs for one library.
pub struct RenderService {
    lib: Library,
    tools: RenderTools,
    cache: Cache,
    opts: RenderOptions,
    on_battery: PowerCheck,
}

struct Shared {
    done: AtomicU64,
    failed: AtomicU64,
    since_evict: AtomicU64,
    paused: Mutex<Option<PauseReason>>,
    evicting: Mutex<()>,
    /// The cache is above [`FULL_PCT`] of its limit (as of the last trim).
    full: AtomicBool,
}

impl RenderService {
    pub fn new(lib: &Library, tools: RenderTools, cache: Cache, opts: RenderOptions) -> Self {
        Self {
            lib: lib.clone(),
            tools,
            cache,
            opts,
            on_battery: Arc::new(|| false),
        }
    }

    /// How to tell whether the computer runs on battery
    /// (`keepsake_platform::on_battery`).
    pub fn with_power(mut self, on_battery: PowerCheck) -> Self {
        self.on_battery = on_battery;
        self
    }

    pub fn cache(&self) -> &Cache {
        &self.cache
    }

    /// Run until the queue is empty (`until_empty`) or `cancel` is set.
    /// Events are delivered on this thread.
    pub fn run(
        &self,
        cancel: &AtomicBool,
        events: &mut dyn FnMut(&RenderEvent),
    ) -> Result<(), RenderError> {
        let (bytes, repaired) = {
            let mut cat = self.lib.open_catalog()?;
            jobs::recover(&mut cat)?;
            // Renditions left under an old name when their source was
            // re-keyed (ADR-034); quick when there are none.
            let repaired = match self.cache.repair_names(&mut cat) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!("repairing rendition names: {e}");
                    RepairReport::default()
                }
            };
            (cat.read(|tx| tx.renditions().totals())?.1, repaired)
        };
        if repaired != RepairReport::default() {
            events(&RenderEvent::Repaired { report: repaired });
        }
        self.cache.clear_temp();
        std::fs::create_dir_all(self.cache.root())?;
        let exiftool = ExifTool::start(&self.tools.exiftool, Duration::from_secs(30)).ok();
        let shared = Shared {
            done: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            since_evict: AtomicU64::new(0),
            paused: Mutex::new(None),
            evicting: Mutex::new(()),
            full: AtomicBool::new(self.is_full(bytes)),
        };
        let (tx, rx) = channel::<RenderEvent>();
        let mut errors: Vec<RenderError> = Vec::new();
        std::thread::scope(|s| {
            let mut handles = Vec::new();
            let plan = std::iter::repeat_n(JOB_STILLS, self.opts.workers.max(1))
                .chain(std::iter::repeat_n(JOB_PROXY, self.opts.proxy_workers))
                .chain(std::iter::once(JOB_FINGERPRINT));
            for kind in plan {
                let tx = tx.clone();
                let shared = &shared;
                let et = exiftool.as_ref();
                handles.push(s.spawn(move || self.worker(kind, cancel, &tx, shared, et)));
            }
            drop(tx);
            for ev in rx {
                events(&ev);
            }
            for h in handles {
                match h.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => errors.push(e),
                    Err(_) => errors.push(RenderError::Failed("a render worker panicked".into())),
                }
            }
        });
        if let Some(e) = errors.into_iter().next() {
            return Err(e);
        }
        let mut cat = self.lib.open_catalog()?;
        let rep = self.trim(&mut cat)?;
        if rep.removed > 0 {
            events(&RenderEvent::Evicted { report: rep });
        }
        events(&RenderEvent::Finished {
            done: shared.done.load(Ordering::SeqCst),
            failed: shared.failed.load(Ordering::SeqCst),
            cancelled: cancel.load(Ordering::SeqCst),
        });
        Ok(())
    }

    fn note_pause(&self, shared: &Shared, now: Option<PauseReason>, tx: &Sender<RenderEvent>) {
        if let Ok(mut p) = shared.paused.lock() {
            if *p != now {
                *p = now;
                let _ = tx.send(match now {
                    Some(reason) => RenderEvent::Paused { reason },
                    None => RenderEvent::Resumed,
                });
            }
        }
    }

    /// A job's bookkeeping write, tried again while the catalog is busy so a
    /// slow moment does not stop the worker (or leave its job running).
    fn write_busy<T>(
        cat: &mut Catalog,
        cancel: &AtomicBool,
        mut f: impl FnMut(&crate::catalog::WriteTx<'_>) -> Result<T, CatalogError>,
    ) -> Result<T, CatalogError> {
        loop {
            match cat.write(&mut f) {
                Err(e) if e.is_busy() && !cancel.load(Ordering::SeqCst) => {
                    tracing::warn!("catalog busy; trying the job's update again");
                    Self::sleep(cancel, BUSY_RETRY);
                }
                r => return r,
            }
        }
    }

    /// A thumbnail job is ready to run (a busy catalog counts as yes: look
    /// again later).
    fn stills_waiting(cat: &Catalog) -> bool {
        cat.read(|t| t.jobs().next(&[JOB_STILLS], now_ms()))
            .map(|j| j.is_some())
            .unwrap_or(true)
    }

    fn sleep(cancel: &AtomicBool, d: Duration) {
        let end = Instant::now() + d;
        while Instant::now() < end && !cancel.load(Ordering::SeqCst) {
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn worker(
        &self,
        kind: &'static str,
        cancel: &AtomicBool,
        tx: &Sender<RenderEvent>,
        shared: &Shared,
        exiftool: Option<&ExifTool>,
    ) -> Result<(), RenderError> {
        let mut cat = self.lib.open_catalog()?;
        while !cancel.load(Ordering::SeqCst) {
            let pause = pause_reason(&cat, &self.on_battery, self.opts.pause_on_battery);
            self.note_pause(shared, pause, tx);
            if pause.is_some() {
                Self::sleep(cancel, Duration::from_secs(1));
                continue;
            }
            // Fingerprints read every byte of a file; on a network drive
            // that leaves thumbnails waiting for the disk. They wait while
            // a thumbnail job is ready instead.
            if kind == JOB_FINGERPRINT && Self::stills_waiting(&cat) {
                Self::sleep(cancel, FINGERPRINT_WAIT);
                continue;
            }
            let claimed = match cat.write(|t| t.jobs().claim(&[kind], now_ms())) {
                Err(e) if e.is_busy() => {
                    Self::sleep(cancel, BUSY_RETRY);
                    continue;
                }
                r => r?,
            };
            let Some(job) = claimed else {
                if self.opts.until_empty {
                    break;
                }
                Self::sleep(cancel, Duration::from_secs(1));
                continue;
            };
            let _ = tx.send(RenderEvent::Started {
                job: job.id,
                kind: job.kind.clone(),
                asset: job.asset_id,
            });
            let t0 = Instant::now();
            let thumbs_only =
                job.priority != Priority::Visible && shared.full.load(Ordering::SeqCst);
            let result = self.process(&mut cat, &job, exiftool, thumbs_only);
            let now = now_ms();
            match result {
                Ok((made, encoder)) => {
                    Self::write_busy(&mut cat, cancel, |t| t.jobs().complete(job.id, now))?;
                    shared.done.fetch_add(1, Ordering::SeqCst);
                    let _ = tx.send(RenderEvent::Done {
                        job: job.id,
                        kind: job.kind.clone(),
                        asset: job.asset_id,
                        ms: u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX),
                        made,
                        encoder,
                    });
                    self.maybe_evict(&mut cat, shared, tx);
                }
                Err(e) if e.is_busy() => {
                    // Not the job's fault: again soon, attempt not counted.
                    let reason = e.to_string();
                    let at = now + BUSY_RETRY.as_millis() as i64;
                    Self::write_busy(&mut cat, cancel, |t| t.jobs().defer(job.id, &reason, at))?;
                    tracing::debug!(job = job.id, kind = %job.kind, "catalog busy; job deferred");
                    let _ = tx.send(RenderEvent::Deferred {
                        job: job.id,
                        kind: job.kind.clone(),
                        asset: job.asset_id,
                        reason,
                    });
                }
                Err(RenderError::Offline) => {
                    let reason = RenderError::Offline.to_string();
                    Self::write_busy(&mut cat, cancel, |t| {
                        t.jobs().defer(job.id, &reason, now + OFFLINE_RETRY_MS)
                    })?;
                    let _ = tx.send(RenderEvent::Deferred {
                        job: job.id,
                        kind: job.kind.clone(),
                        asset: job.asset_id,
                        reason,
                    });
                }
                Err(e) => {
                    let retry = if e.is_permanent() {
                        None
                    } else {
                        jobs::retry_at(job.attempts, now)
                    };
                    let msg = e.to_string();
                    Self::write_busy(&mut cat, cancel, |t| {
                        t.jobs().fail(job.id, &msg, retry, now)
                    })?;
                    if retry.is_none() {
                        shared.failed.fetch_add(1, Ordering::SeqCst);
                    }
                    tracing::warn!(job = job.id, kind = %job.kind, "render failed: {msg}");
                    let _ = tx.send(RenderEvent::Failed {
                        job: job.id,
                        kind: job.kind.clone(),
                        asset: job.asset_id,
                        error: msg,
                        retry: retry.is_some(),
                    });
                }
            }
        }
        Ok(())
    }

    fn maybe_evict(&self, cat: &mut Catalog, shared: &Shared, tx: &Sender<RenderEvent>) {
        let n = shared.since_evict.fetch_add(1, Ordering::SeqCst) + 1;
        if n < self.opts.evict_every.max(1) {
            return;
        }
        let Ok(_guard) = shared.evicting.try_lock() else {
            return;
        };
        shared.since_evict.store(0, Ordering::SeqCst);
        match self.trim(cat) {
            Ok(rep) => {
                shared
                    .full
                    .store(self.is_full(rep.bytes_after), Ordering::SeqCst);
                if rep.removed > 0 {
                    let _ = tx.send(RenderEvent::Evicted { report: rep });
                }
            }
            Err(e) => tracing::warn!("evict: {e}"),
        }
    }

    /// Trim the cache: once over its limit, down to [`EVICT_TO_PCT`] of it.
    fn trim(&self, cat: &mut Catalog) -> Result<EvictReport, RenderError> {
        let max = self.opts.max_bytes;
        self.cache
            .evict_to(cat, &self.lib, max, pct_of(max, EVICT_TO_PCT))
    }

    /// Does a cache of `bytes` count as full?
    fn is_full(&self, bytes: u64) -> bool {
        bytes > pct_of(self.opts.max_bytes, FULL_PCT)
    }

    /// Run one job. Returns the variants made and the encoder used.
    /// `thumbs_only`: make the thumbnails, not the preview or the proxy.
    fn process(
        &self,
        cat: &mut Catalog,
        job: &Job,
        exiftool: Option<&ExifTool>,
        thumbs_only: bool,
    ) -> Result<(Vec<String>, Option<String>), RenderError> {
        match job.kind.as_str() {
            JOB_STILLS => {
                let src = locate(cat, &self.lib, job.asset_id)?;
                let wanted: &[Variant] = if thumbs_only {
                    &[Variant::Thumb1024, Variant::Thumb256]
                } else {
                    &Variant::STILLS
                };
                let made =
                    still::render_stills_of(cat, &self.cache, &src, &self.tools, exiftool, wanted)?;
                Ok((
                    made.iter().map(|(v, _)| v.name().to_owned()).collect(),
                    None,
                ))
            }
            JOB_PROXY if thumbs_only => Ok((Vec::new(), None)),
            JOB_PROXY => {
                let a = cat.read(|t| t.assets().require(job.asset_id))?;
                if a.media_kind != MediaKind::Video {
                    return Ok((Vec::new(), None));
                }
                let src = locate(cat, &self.lib, job.asset_id)?;
                let made = video::render_proxy(
                    cat,
                    &self.cache,
                    &src,
                    &self.tools,
                    self.opts.forced_encoder.as_deref(),
                )?;
                Ok(match made {
                    Some((v, _, enc)) => (vec![v.name().to_owned()], enc),
                    None => (Vec::new(), None),
                })
            }
            JOB_FINGERPRINT => {
                match fingerprint::fingerprint_asset(cat, &self.lib, &self.cache, job.asset_id) {
                    Ok(_) => Ok((Vec::new(), None)),
                    Err(FingerprintError::Offline) => Err(RenderError::Offline),
                    Err(FingerprintError::Catalog(e)) => Err(RenderError::Catalog(e)),
                    Err(e) => Err(RenderError::Failed(e.to_string())),
                }
            }
            other => Err(RenderError::Unsupported(format!("job kind {other}"))),
        }
    }
}

/// Where a library's renditions go on this machine.
pub fn cache_for(settings: &RenderSettings) -> Result<Cache, RenderError> {
    Ok(Cache::new(settings.cache_root()?))
}

/// Paths of an asset's existing renditions, for display.
pub fn paths_of(
    cat: &mut Catalog,
    cache: &Cache,
    asset: AssetId,
) -> Result<Vec<(super::Variant, PathBuf)>, RenderError> {
    let mut out = Vec::new();
    for v in super::Variant::ALL {
        if let Some(p) = cache.lookup(cat, asset, v)? {
            out.push((v, p));
        }
    }
    Ok(out)
}

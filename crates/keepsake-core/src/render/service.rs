//! Render workers on the job queue.
//!
//! [`RenderService::run`] starts the workers and calls back with
//! [`RenderEvent`]s on the calling thread:
//! - `workers` threads take [`super::JOB_STILLS`] jobs, `proxy_workers`
//!   threads take [`super::JOB_PROXY`] jobs, and one thread takes
//!   [`crate::fingerprint::JOB_FINGERPRINT`] jobs; with AI on, one more
//!   takes [`crate::ai::JOB_AI`] and [`crate::ai::JOB_AI_TAG`] jobs (ADR-035), and with faces on,
//!   one takes [`crate::ai::JOB_AI_FACES`] jobs (ADR-040), and with Read aircraft on, one
//!   takes [`crate::ai::JOB_AI_AIRCRAFT`] jobs (ADR-041). Each has its own catalog
//!   connection and claims one job at a time;
//! - fingerprints wait while a thumbnail job is ready: each reads a whole
//!   file, and on a network drive they would take the disk from thumbnails.
//!   AI jobs wait too: they read the thumbnails those jobs make;
//! - the AI worker loads the model when it starts (GPU first, else CPU);
//!   if it cannot, it reports why and stops, and the other workers go on.
//!   It analyses only what was asked for (a date range, `ai::enqueue_range`;
//!   the whole library only through `keepsake ai run`): on start it drops
//!   queued background AI jobs left by earlier builds;
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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::cache::{Cache, CacheStats, EvictReport, RepairReport};
use super::source::locate;
use super::{still, video, RenderError, RenderTools, Variant, JOB_PROXY, JOB_STILLS};
use crate::ai::aircraft::scan::{self as aircraft_scan, ReaderLoader};
use crate::ai::faces::scan::{self as face_scan, FaceFinder, FinderLoader};
use crate::ai::suggest::{
    self, Adviser, JOB_SUGGEST_CLAUDE, JOB_SUGGEST_GEMINI, JOB_SUGGEST_OPENAI, SUGGEST_JOBS,
};
use crate::ai::{
    self, AiError, ImageModel, LabelSet, Purpose, CLOUD_JOBS, JOB_AI, JOB_AI_AIRCRAFT,
    JOB_AI_CLAUDE, JOB_AI_FACES, JOB_AI_GEMINI, JOB_AI_OPENAI, JOB_AI_TAG,
};
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
/// A job that is waiting (an AI job for its thumbnail) is looked at again
/// after this long.
const WAITING_RETRY_MS: i64 = 5 * 60 * 1000;
/// An AI job whose thumbnail is being made is looked at again after this.
const NO_THUMB_RETRY_MS: i64 = 15_000;
/// AI jobs claimed and saved together.
const AI_BATCH: usize = 32;
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
    /// Run the AI worker (tags, duplicates; ADR-035).
    pub ai: bool,
    /// Let it use the graphics card.
    pub ai_gpu: bool,
    /// Run the face worker (ADR-040).
    pub faces: bool,
    /// Run the aircraft worker (ADR-041).
    pub aircraft: bool,
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
            ai: false,
            ai_gpu: true,
            faces: false,
            aircraft: false,
        }
    }

    /// Turn the AI, face and aircraft workers on or off as the settings
    /// say.
    pub fn with_ai(mut self, s: &crate::settings::AiSettings) -> Self {
        self.ai = s.enabled;
        self.ai_gpu = s.gpu;
        self.faces = s.enabled && s.faces;
        self.aircraft = s.aircraft;
        self
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
    /// The AI model is loaded and the AI worker runs.
    AiReady {
        accel: ai::Accel,
    },
    /// The AI worker could not start (no model, no runtime, ...).
    AiUnavailable {
        reason: String,
    },
    /// The face models are loaded and the face worker runs (ADR-040).
    FacesReady {
        accel: ai::Accel,
    },
    /// The face worker could not start.
    FacesUnavailable {
        reason: String,
    },
    /// The text models are loaded and the aircraft worker runs (ADR-041).
    AircraftReady {
        accel: ai::Accel,
    },
    /// The aircraft worker could not start.
    AircraftUnavailable {
        reason: String,
    },
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
    /// Tag with AI (ADR-036, ADR-038): with it, a worker runs the cloud
    /// jobs of every provider (`ai.claude`, `ai.openai`, `ai.gemini`).
    cloud: Option<Arc<dyn ai::cloud::Tagger>>,
    /// Edit suggestions (ADR-042): with it, the cloud worker also asks the
    /// AIs for edit suggestions (`ai.suggest.*`).
    adviser: Option<Arc<dyn Adviser>>,
    /// Loads the face worker's models (ADR-040); the installed models when
    /// not set.
    faces: Option<FinderLoader>,
    /// Loads the aircraft worker's reader (ADR-041); the installed models
    /// when not set.
    aircraft: Option<ReaderLoader>,
}

struct Shared {
    done: AtomicU64,
    failed: AtomicU64,
    since_evict: AtomicU64,
    paused: Mutex<Option<PauseReason>>,
    evicting: Mutex<()>,
    /// The cache is above [`FULL_PCT`] of its limit (as of the last trim).
    full: AtomicBool,
    /// Thumbnail workers still running. Workers that wait for thumbnails
    /// (AI, faces) stop waiting once none is: when the service stops at an
    /// empty queue, a thumbnail queued after they quit would never be made.
    stills_live: AtomicUsize,
}

/// Counts a thumbnail worker out of [`Shared::stills_live`] when it ends,
/// however it ends.
struct StillsLive<'a>(&'a AtomicUsize);

impl Drop for StillsLive<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl RenderService {
    pub fn new(lib: &Library, tools: RenderTools, cache: Cache, opts: RenderOptions) -> Self {
        Self {
            lib: lib.clone(),
            tools,
            cache,
            opts,
            on_battery: Arc::new(|| false),
            cloud: None,
            adviser: None,
            faces: None,
            aircraft: None,
        }
    }

    /// Load the aircraft worker's reader with this (tests use a stand-in).
    pub fn with_aircraft(mut self, loader: ReaderLoader) -> Self {
        self.aircraft = Some(loader);
        self
    }

    /// Load the face worker's finder with this (tests use a stand-in).
    pub fn with_faces(mut self, loader: FinderLoader) -> Self {
        self.faces = Some(loader);
        self
    }

    /// Run the cloud tagging jobs with this tagger (ADR-036, ADR-038).
    pub fn with_cloud(mut self, tagger: Arc<dyn ai::cloud::Tagger>) -> Self {
        self.cloud = Some(tagger);
        self
    }

    /// Ask for edit suggestions with this adviser (ADR-042).
    pub fn with_adviser(mut self, adviser: Arc<dyn Adviser>) -> Self {
        self.adviser = Some(adviser);
        self
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
            stills_live: AtomicUsize::new(self.opts.workers.max(1)),
        };
        let (tx, rx) = channel::<RenderEvent>();
        let mut errors: Vec<RenderError> = Vec::new();
        std::thread::scope(|s| {
            let mut handles = Vec::new();
            let plan = std::iter::repeat_n(JOB_STILLS, self.opts.workers.max(1))
                .chain(std::iter::repeat_n(JOB_PROXY, self.opts.proxy_workers))
                .chain(std::iter::once(JOB_FINGERPRINT))
                .chain(self.opts.ai.then_some(JOB_AI))
                .chain(self.opts.faces.then_some(JOB_AI_FACES))
                .chain(self.opts.aircraft.then_some(JOB_AI_AIRCRAFT))
                // One worker sends the photos of every provider.
                .chain((self.cloud.is_some() || self.adviser.is_some()).then_some(JOB_AI_CLAUDE));
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

    /// A thumbnail job that was asked for (on screen or requested, not the
    /// backlog) is ready or running: AI jobs read the thumbnails those make
    /// (a busy catalog counts as yes).
    fn stills_active(cat: &Catalog) -> bool {
        cat.read(|t| {
            t.jobs()
                .active_up_to(JOB_STILLS, Priority::Recent, now_ms())
        })
        .unwrap_or(true)
    }

    /// Wait for thumbnails: one is ready to be made and a thumbnail worker
    /// is still there to make it.
    fn stills_wait(cat: &Catalog, shared: &Shared) -> bool {
        shared.stills_live.load(Ordering::SeqCst) > 0 && Self::stills_active(cat)
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
        let _live = (kind == JOB_STILLS).then(|| StillsLive(&shared.stills_live));
        let mut cat = self.lib.open_catalog()?;
        if kind == JOB_AI_FACES {
            return self.face_worker(&mut cat, cancel, tx, shared);
        }
        if kind == JOB_AI_AIRCRAFT {
            return self.aircraft_worker(&mut cat, cancel, tx, shared);
        }
        // The cloud worker sends the photos of every provider: tagging and
        // edit suggestions.
        let cloud_kinds: Vec<&str> = CLOUD_JOBS.iter().chain(&SUGGEST_JOBS).copied().collect();
        let mut ai_state = None;
        if kind == JOB_AI {
            let dropped = cat.write(|t| {
                Ok::<_, CatalogError>(
                    t.jobs().remove_queued(JOB_AI, Priority::Backlog)?
                        + t.jobs().remove_queued(JOB_AI_TAG, Priority::Backlog)?,
                )
            });
            match dropped {
                Ok(n) if n > 0 => tracing::info!("AI: dropped {n} queued background jobs"),
                Ok(_) => {}
                Err(e) => tracing::debug!("AI: dropping background jobs: {e}"),
            }
            match Self::load_ai(self.opts.ai_gpu) {
                Ok(state) => {
                    let _ = tx.send(RenderEvent::AiReady {
                        accel: state.0.accel(),
                    });
                    ai_state = Some(state);
                }
                Err(e) => {
                    tracing::warn!("AI worker not started: {e}");
                    let _ = tx.send(RenderEvent::AiUnavailable {
                        reason: e.to_string(),
                    });
                    return Ok(());
                }
            }
        }
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
            let wait = match kind {
                JOB_FINGERPRINT => Self::stills_waiting(&cat),
                JOB_AI => Self::stills_wait(&cat, shared),
                _ => false,
            };
            if wait {
                Self::sleep(cancel, FINGERPRINT_WAIT);
                continue;
            }
            // The AI worker does both, a batch at a time.
            if let Some(state) = ai_state.as_mut() {
                match self.ai_batch(&mut cat, cancel, tx, shared, state) {
                    Ok(true) => {}
                    Ok(false) if self.opts.until_empty => break,
                    Ok(false) => Self::sleep(cancel, Duration::from_secs(1)),
                    Err(e) if e.is_busy() => Self::sleep(cancel, BUSY_RETRY),
                    Err(e) => return Err(e),
                }
                continue;
            }
            // The AI worker does both: finding duplicates and tagging. The
            // cloud worker does every provider.
            let kinds: &[&str] = if kind == JOB_AI {
                &[JOB_AI, JOB_AI_TAG]
            } else if kind == JOB_AI_CLAUDE {
                &cloud_kinds
            } else {
                std::slice::from_ref(&kind)
            };
            let claimed = match cat.write(|t| t.jobs().claim(kinds, now_ms())) {
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
            let result = self.process(&mut cat, &job, exiftool, thumbs_only, ai_state.as_mut());
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
                Err(e) => self.settle_err(&mut cat, cancel, tx, shared, kind, &job, e)?,
            }
        }
        Ok(())
    }

    /// Record a job that did not succeed: deferred (catalog busy, waiting
    /// for something, drive offline) or failed.
    #[allow(clippy::too_many_arguments)]
    fn settle_err(
        &self,
        cat: &mut Catalog,
        cancel: &AtomicBool,
        tx: &Sender<RenderEvent>,
        shared: &Shared,
        kind: &'static str,
        job: &Job,
        e: RenderError,
    ) -> Result<(), RenderError> {
        let now = now_ms();
        match e {
            e if e.is_busy() => {
                // Not the job's fault: again soon, attempt not counted.
                let reason = e.to_string();
                let at = now + BUSY_RETRY.as_millis() as i64;
                Self::write_busy(cat, cancel, |t| t.jobs().defer(job.id, &reason, at))?;
                tracing::debug!(job = job.id, kind = %job.kind, "catalog busy; job deferred");
                let _ = tx.send(RenderEvent::Deferred {
                    job: job.id,
                    kind: job.kind.clone(),
                    asset: job.asset_id,
                    reason,
                });
            }
            RenderError::Waiting(reason) => {
                let wait = if matches!(
                    kind,
                    JOB_AI | JOB_AI_CLAUDE | JOB_AI_FACES | JOB_AI_AIRCRAFT
                ) {
                    NO_THUMB_RETRY_MS
                } else {
                    WAITING_RETRY_MS
                };
                Self::write_busy(cat, cancel, |t| t.jobs().defer(job.id, &reason, now + wait))?;
                let _ = tx.send(RenderEvent::Deferred {
                    job: job.id,
                    kind: job.kind.clone(),
                    asset: job.asset_id,
                    reason,
                });
            }
            RenderError::Offline => {
                let reason = RenderError::Offline.to_string();
                Self::write_busy(cat, cancel, |t| {
                    t.jobs().defer(job.id, &reason, now + OFFLINE_RETRY_MS)
                })?;
                let _ = tx.send(RenderEvent::Deferred {
                    job: job.id,
                    kind: job.kind.clone(),
                    asset: job.asset_id,
                    reason,
                });
            }
            e => {
                let retry = if e.is_permanent() {
                    None
                } else {
                    jobs::retry_at(job.attempts, now)
                };
                let msg = e.to_string();
                Self::write_busy(cat, cancel, |t| t.jobs().fail(job.id, &msg, retry, now))?;
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
        Ok(())
    }

    /// The AI worker's turn: claim up to [`AI_BATCH`] jobs in one write,
    /// work out their results, then save them all and mark them done in
    /// one more write. On a USB drive a write can take seconds, so this is
    /// far quicker than three writes per photo. Returns whether there was
    /// work.
    fn ai_batch(
        &self,
        cat: &mut Catalog,
        cancel: &AtomicBool,
        tx: &Sender<RenderEvent>,
        shared: &Shared,
        state: &mut (ImageModel, LabelSet),
    ) -> Result<bool, RenderError> {
        let now = now_ms();
        let jobs: Vec<Job> = cat.write(|t| {
            let mut v = Vec::new();
            while v.len() < AI_BATCH {
                match t.jobs().claim(&[JOB_AI, JOB_AI_TAG], now)? {
                    Some(j) => v.push(j),
                    None => break,
                }
            }
            Ok::<_, CatalogError>(v)
        })?;
        if jobs.is_empty() {
            return Ok(false);
        }
        let mut ready = Vec::new();
        let mut failed = Vec::new();
        for job in jobs {
            let _ = tx.send(RenderEvent::Started {
                job: job.id,
                kind: job.kind.clone(),
                asset: job.asset_id,
            });
            let t0 = Instant::now();
            let purpose = Purpose::of_job(&job.kind).unwrap_or(Purpose::Duplicates);
            let (model, labels) = (&mut state.0, &state.1);
            match ai::analyze::prepare(cat, &self.cache, model, labels, job.asset_id, purpose) {
                Ok(p) => ready.push((job, p, t0.elapsed())),
                Err(e) => {
                    let e = Self::ai_error(cat, &job, e)?;
                    failed.push((job, e));
                }
            }
        }
        let at = now_ms();
        Self::write_busy(cat, cancel, |t| {
            for (job, p, _) in &ready {
                ai::analyze::save(t, p, at)?;
                t.jobs().complete(job.id, at)?;
            }
            t.tags().prune_unused()?;
            Ok(())
        })?;
        for (job, p, took) in ready {
            shared.done.fetch_add(1, Ordering::SeqCst);
            let _ = tx.send(RenderEvent::Done {
                job: job.id,
                kind: job.kind.clone(),
                asset: job.asset_id,
                ms: u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
                made: p.outcome().tags.into_iter().map(|(t, _)| t).collect(),
                encoder: None,
            });
        }
        for (job, e) in failed {
            self.settle_err(cat, cancel, tx, shared, JOB_AI, &job, e)?;
        }
        Ok(true)
    }

    /// The face worker (ADR-040): load the models, then search the queued
    /// photos a batch at a time, like the AI worker. If the models cannot
    /// be loaded it says why and stops; the other workers go on.
    fn face_worker(
        &self,
        cat: &mut Catalog,
        cancel: &AtomicBool,
        tx: &Sender<RenderEvent>,
        shared: &Shared,
    ) -> Result<(), RenderError> {
        let loader = self
            .faces
            .clone()
            .unwrap_or_else(face_scan::installed_loader);
        let mut finder = match loader(self.opts.ai_gpu) {
            Ok(f) => {
                let _ = tx.send(RenderEvent::FacesReady { accel: f.accel() });
                f
            }
            Err(e) => {
                tracing::warn!("face worker not started: {e}");
                let _ = tx.send(RenderEvent::FacesUnavailable {
                    reason: e.to_string(),
                });
                return Ok(());
            }
        };
        while !cancel.load(Ordering::SeqCst) {
            let pause = pause_reason(cat, &self.on_battery, self.opts.pause_on_battery);
            self.note_pause(shared, pause, tx);
            if pause.is_some() {
                Self::sleep(cancel, Duration::from_secs(1));
                continue;
            }
            // Like the AI worker, it reads thumbnails: they come first.
            if Self::stills_wait(cat, shared) {
                Self::sleep(cancel, FINGERPRINT_WAIT);
                continue;
            }
            match self.face_batch(cat, cancel, tx, shared, finder.as_mut()) {
                Ok(true) => {}
                Ok(false) if self.opts.until_empty => break,
                Ok(false) => Self::sleep(cancel, Duration::from_secs(1)),
                Err(e) if e.is_busy() => Self::sleep(cancel, BUSY_RETRY),
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    /// The face worker's turn: claim up to [`AI_BATCH`] face jobs, find
    /// their faces, then store them all in one write. Returns whether there
    /// was work.
    fn face_batch(
        &self,
        cat: &mut Catalog,
        cancel: &AtomicBool,
        tx: &Sender<RenderEvent>,
        shared: &Shared,
        finder: &mut dyn FaceFinder,
    ) -> Result<bool, RenderError> {
        let now = now_ms();
        let jobs: Vec<Job> = cat.write(|t| {
            let mut v = Vec::new();
            while v.len() < AI_BATCH {
                match t.jobs().claim(&[JOB_AI_FACES], now)? {
                    Some(j) => v.push(j),
                    None => break,
                }
            }
            Ok::<_, CatalogError>(v)
        })?;
        if jobs.is_empty() {
            return Ok(false);
        }
        let mut ready = Vec::new();
        let mut failed = Vec::new();
        for job in jobs {
            let _ = tx.send(RenderEvent::Started {
                job: job.id,
                kind: job.kind.clone(),
                asset: job.asset_id,
            });
            let t0 = Instant::now();
            match face_scan::prepare(cat, &self.cache, finder, job.asset_id) {
                Ok(p) => ready.push((job, p, t0.elapsed())),
                Err(e) => {
                    let e = Self::ai_error(cat, &job, e)?;
                    failed.push((job, e));
                }
            }
        }
        let at = now_ms();
        Self::write_busy(cat, cancel, |t| {
            for (job, p, _) in &ready {
                face_scan::save(t, p, at)?;
                t.jobs().complete(job.id, at)?;
            }
            Ok(())
        })?;
        for (job, p, took) in ready {
            shared.done.fetch_add(1, Ordering::SeqCst);
            let _ = tx.send(RenderEvent::Done {
                job: job.id,
                kind: job.kind.clone(),
                asset: job.asset_id,
                ms: u64::try_from(took.as_millis()).unwrap_or(u64::MAX),
                made: vec![format!("{} faces", p.faces.len())],
                encoder: None,
            });
        }
        for (job, e) in failed {
            self.settle_err(cat, cancel, tx, shared, JOB_AI_FACES, &job, e)?;
        }
        Ok(true)
    }

    /// The aircraft worker (ADR-041): load the text models, then read the
    /// queued items one at a time (each takes seconds, and its keywords are
    /// saved with its sidecars). If the models cannot be loaded it says why
    /// and stops; the other workers go on.
    fn aircraft_worker(
        &self,
        cat: &mut Catalog,
        cancel: &AtomicBool,
        tx: &Sender<RenderEvent>,
        shared: &Shared,
    ) -> Result<(), RenderError> {
        let loader = self
            .aircraft
            .clone()
            .unwrap_or_else(aircraft_scan::installed_loader);
        let loaded = loader(self.opts.ai_gpu)
            .and_then(|reader| Ok((reader, aircraft_scan::Lists::builtin()?)));
        let (mut reader, lists) = match loaded {
            Ok(v) => {
                let _ = tx.send(RenderEvent::AircraftReady { accel: v.0.accel() });
                v
            }
            Err(e) => {
                tracing::warn!("aircraft worker not started: {e}");
                let _ = tx.send(RenderEvent::AircraftUnavailable {
                    reason: e.to_string(),
                });
                return Ok(());
            }
        };
        while !cancel.load(Ordering::SeqCst) {
            let pause = pause_reason(cat, &self.on_battery, self.opts.pause_on_battery);
            self.note_pause(shared, pause, tx);
            if pause.is_some() {
                Self::sleep(cancel, Duration::from_secs(1));
                continue;
            }
            let claimed = match cat.write(|t| t.jobs().claim(&[JOB_AI_AIRCRAFT], now_ms())) {
                Err(e) if e.is_busy() => {
                    Self::sleep(cancel, BUSY_RETRY);
                    continue;
                }
                r => r?,
            };
            let Some(job) = claimed else {
                // An item whose file is away waits for its thumbnail: stay
                // while one is being made.
                if self.opts.until_empty && !Self::stills_wait(cat, shared) {
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
            let read = aircraft_scan::read_asset(
                cat,
                &self.lib,
                &self.cache,
                &self.tools,
                reader.as_mut(),
                &lists,
                job.asset_id,
            );
            match read {
                Ok(out) => {
                    let now = now_ms();
                    Self::write_busy(cat, cancel, |t| t.jobs().complete(job.id, now))?;
                    shared.done.fetch_add(1, Ordering::SeqCst);
                    let _ = tx.send(RenderEvent::Done {
                        job: job.id,
                        kind: job.kind.clone(),
                        asset: job.asset_id,
                        ms: u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX),
                        made: out.added,
                        encoder: None,
                    });
                }
                Err(e) => {
                    let e = Self::ai_error(cat, &job, e)?;
                    self.settle_err(cat, cancel, tx, shared, JOB_AI_AIRCRAFT, &job, e)?;
                }
            }
        }
        Ok(())
    }

    /// An AI job's error as a render error. Without a thumbnail, it is made
    /// first (as urgently) and the job waits.
    fn ai_error(cat: &mut Catalog, job: &Job, e: AiError) -> Result<RenderError, RenderError> {
        Ok(match e {
            AiError::NoThumbnail => {
                super::enqueue(cat, &[job.asset_id], job.priority.class(), true)?;
                RenderError::Waiting("no thumbnail yet".into())
            }
            AiError::Render(e) => e,
            AiError::Catalog(e) => RenderError::Catalog(e),
            AiError::Image(m) => RenderError::Image(m),
            AiError::Cloud { message, retry } if !retry => RenderError::Refused(message),
            e @ AiError::NoApiKey(_) => RenderError::Refused(e.to_string()),
            e => RenderError::Failed(e.to_string()),
        })
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

    fn load_ai(gpu: bool) -> Result<(ImageModel, LabelSet), AiError> {
        Ok((ImageModel::load_installed(gpu)?, LabelSet::builtin()?))
    }

    /// Run one job. Returns the variants made and the encoder used.
    /// `thumbs_only`: make the thumbnails, not the preview or the proxy.
    fn process(
        &self,
        cat: &mut Catalog,
        job: &Job,
        exiftool: Option<&ExifTool>,
        thumbs_only: bool,
        ai_state: Option<&mut (ImageModel, LabelSet)>,
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
            JOB_AI | JOB_AI_TAG => {
                let Some((model, labels)) = ai_state else {
                    return Err(RenderError::Waiting("the AI model is not loaded".into()));
                };
                let purpose = Purpose::of_job(&job.kind).unwrap_or(Purpose::Duplicates);
                match ai::analyze_asset(cat, &self.cache, model, labels, job.asset_id, purpose) {
                    Ok(out) => Ok((out.tags.into_iter().map(|(t, _)| t).collect(), None)),
                    Err(AiError::NoThumbnail) => {
                        // Make the thumbnail first (as urgently), then again.
                        super::enqueue(cat, &[job.asset_id], job.priority.class(), true)?;
                        Err(RenderError::Waiting("no thumbnail yet".into()))
                    }
                    Err(AiError::Render(e)) => Err(e),
                    Err(AiError::Catalog(e)) => Err(RenderError::Catalog(e)),
                    Err(AiError::Image(m)) => Err(RenderError::Image(m)),
                    Err(e) => Err(RenderError::Failed(e.to_string())),
                }
            }
            JOB_AI_CLAUDE | JOB_AI_OPENAI | JOB_AI_GEMINI => {
                let Some(provider) = ai::Provider::of_job(&job.kind) else {
                    return Err(RenderError::Unsupported(format!("job kind {}", job.kind)));
                };
                let Some(tagger) = self.cloud.as_deref() else {
                    return Err(RenderError::Waiting(format!("{provider} is not set up")));
                };
                match ai::cloud::tag_asset(
                    cat,
                    &self.lib,
                    &self.cache,
                    tagger,
                    provider,
                    job.asset_id,
                ) {
                    Ok(added) => Ok((added, None)),
                    Err(e) => Err(Self::ai_error(cat, job, e)?),
                }
            }
            JOB_SUGGEST_CLAUDE | JOB_SUGGEST_OPENAI | JOB_SUGGEST_GEMINI => {
                let Some(provider) = suggest::provider_of_job(&job.kind) else {
                    return Err(RenderError::Unsupported(format!("job kind {}", job.kind)));
                };
                let Some(adviser) = self.adviser.as_deref() else {
                    return Err(RenderError::Waiting(format!("{provider} is not set up")));
                };
                match suggest::ask_asset(cat, &self.cache, adviser, provider, job.asset_id) {
                    Ok(got) => Ok((got.suggestions.into_iter().map(|s| s.title).collect(), None)),
                    Err(e) => Err(Self::ai_error(cat, job, e)?),
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

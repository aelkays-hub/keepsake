//! Renditions for the UI: a background `RenderService` for the open library,
//! status, and "these are on screen" requests. Finished thumbnails are
//! announced with `render://thumbs` so tiles refresh.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use keepsake_core::catalog::JobState;
use keepsake_core::render::service::{self, PowerCheck};
use keepsake_core::render::{self, RenderEvent, RenderOptions, RenderService, RenderTools};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::{RenderSummary, ThumbsReady};
use crate::convert;
use crate::state::{err, AppState, CmdResult};

/// The running service. Each run has its own stop flag, so a restart can
/// start the new service while the old one finishes its current job.
#[derive(Default)]
pub struct RenderState {
    cancel: Mutex<Arc<AtomicBool>>,
    worker: Mutex<Option<JoinHandle<()>>>,
    /// Services stopped by a restart, finishing their current jobs.
    retired: Mutex<Vec<JoinHandle<()>>>,
}

impl RenderState {
    /// Ask the service to stop after its current jobs.
    pub(crate) fn stop(&self) {
        if let Ok(c) = self.cancel.lock() {
            c.store(true, Ordering::SeqCst);
        }
    }

    /// No service thread is running, a restarted one included (finished
    /// ones are joined). A service's ExifTool ends with its thread.
    pub(crate) fn finished(&self) -> bool {
        let (Ok(mut slot), Ok(mut retired)) = (self.worker.lock(), self.retired.lock()) else {
            return false;
        };
        retired.extend(slot.take());
        let (done, running): (Vec<_>, Vec<_>) =
            retired.drain(..).partition(JoinHandle::is_finished);
        for h in done {
            let _ = h.join();
        }
        *retired = running;
        retired.is_empty()
    }

    /// Track `worker`, stopped by `cancel` (tests).
    #[cfg(test)]
    pub(crate) fn adopt(&self, cancel: Arc<AtomicBool>, worker: JoinHandle<()>) {
        if let Ok(mut c) = self.cancel.lock() {
            *c = cancel;
        }
        if let Ok(mut w) = self.worker.lock() {
            *w = Some(worker);
        }
    }
}

/// Stop the service (it finishes its current job in the background) and
/// start one with the current settings.
pub fn restart_service(app: &AppHandle) -> CmdResult<bool> {
    let state = app.state::<AppState>();
    state.render.stop();
    if let (Ok(mut slot), Ok(mut retired)) =
        (state.render.worker.lock(), state.render.retired.lock())
    {
        retired.extend(slot.take());
    }
    start_service(app)
}

fn power() -> PowerCheck {
    Arc::new(keepsake_platform::on_battery)
}

/// Start the render service for the open library (no-op if running).
pub fn start_service(app: &AppHandle) -> CmdResult<bool> {
    let state = app.state::<AppState>();
    let open = state.open()?;
    let mut slot = state
        .render
        .worker
        .lock()
        .map_err(|_| "render state poisoned")?;
    if slot.as_ref().is_some_and(|h| !h.is_finished()) {
        return Ok(false);
    }
    let mut opts = RenderOptions::from_settings(&open.settings.render).with_ai(&open.settings.ai);
    opts.until_empty = false;
    let svc = RenderService::new(&open.lib, RenderTools::discover(), open.cache.clone(), opts)
        .with_power(power())
        .with_cloud(crate::cloud::tagger())
        .with_adviser(crate::cloud::adviser());
    let svc = if open.settings.ai.aircraft_lookup {
        svc.with_lookup(crate::aircraft::lookup())
    } else {
        svc
    };
    let cancel = Arc::new(AtomicBool::new(false));
    *state
        .render
        .cancel
        .lock()
        .map_err(|_| "render state poisoned")? = Arc::clone(&cancel);
    let app = app.clone();
    let lib = open.lib.clone();
    let handle = std::thread::Builder::new()
        .name("render-service".into())
        .spawn(move || {
            if let Ok(mut cat) = lib.open_catalog() {
                let _ = render::enqueue_backlog(&mut cat);
            }
            let result = svc.run(&cancel, &mut |ev| match ev {
                RenderEvent::AiReady { accel } => {
                    let st = app.state::<AppState>();
                    st.ai.note(Some(accel.describe().to_owned()), None);
                    let _ = app.emit("ai://status", ());
                }
                RenderEvent::AiUnavailable { reason } => {
                    let st = app.state::<AppState>();
                    st.ai.note(None, Some(reason.clone()));
                    let _ = app.emit("ai://status", ());
                }
                RenderEvent::FacesReady { accel } => {
                    let st = app.state::<AppState>();
                    st.faces.note(Some(accel.describe().to_owned()), None);
                    let _ = app.emit("faces://status", ());
                }
                RenderEvent::FacesUnavailable { reason } => {
                    let st = app.state::<AppState>();
                    st.faces.note(None, Some(reason.clone()));
                    let _ = app.emit("faces://status", ());
                }
                RenderEvent::AircraftReady { accel } => {
                    let st = app.state::<AppState>();
                    st.aircraft.note(Some(accel.describe().to_owned()), None);
                    let _ = app.emit("aircraft://status", ());
                }
                RenderEvent::AircraftUnavailable { reason } => {
                    let st = app.state::<AppState>();
                    st.aircraft.note(None, Some(reason.clone()));
                    let _ = app.emit("aircraft://status", ());
                }
                // A new thumbnail, or a fingerprint that moved the item's
                // thumbnails to their new name (ADR-034).
                RenderEvent::Done {
                    asset, made, kind, ..
                } if made.iter().any(|m| m == "thumb-256")
                    || kind == keepsake_core::fingerprint::JOB_FINGERPRINT =>
                {
                    let _ = app.emit(
                        "render://thumbs",
                        ThumbsReady {
                            ids: vec![asset.to_string()],
                            all: false,
                        },
                    );
                }
                RenderEvent::Repaired { report } if report.renamed > 0 => {
                    let _ = app.emit(
                        "render://thumbs",
                        ThumbsReady {
                            ids: Vec::new(),
                            all: true,
                        },
                    );
                }
                _ => {}
            });
            if let Err(e) = result {
                tracing::error!("render service: {e}");
            }
        })
        .map_err(err)?;
    *slot = Some(handle);
    Ok(true)
}

#[tauri::command(async)]
pub fn render_start(app: AppHandle) -> CmdResult<bool> {
    start_service(&app)
}

#[tauri::command(async)]
pub fn render_stop(state: State<'_, AppState>) -> CmdResult<()> {
    state.render.stop();
    Ok(())
}

#[tauri::command(async)]
pub fn render_status(state: State<'_, AppState>) -> CmdResult<RenderSummary> {
    let open = state.open()?;
    // Its own connection: polling never holds up the grid's reads.
    let cat = open.writer()?;
    let st = service::status(
        &cat,
        &open.lib,
        &open.cache,
        &open.settings.render,
        &power(),
    )
    .map_err(err)?;
    Ok(RenderSummary {
        queued: st.jobs.total(JobState::Queued),
        running: st.jobs.total(JobState::Running),
        done: st.jobs.total(JobState::Done),
        failed: st.jobs.total(JobState::Failed),
        paused: st.paused.map(|p| p.describe().to_owned()),
        cache_bytes: st.cache.bytes,
        cache_max: st.cache.max_bytes,
        kept_offline: st.cache.kept_offline_bytes,
    })
}

/// The assets on screen: rendered first (and again if evicted).
#[tauri::command(async)]
pub fn render_set_visible(state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<u64> {
    let ids = convert::ids(&ids)?;
    let open = state.open()?;
    let (lib, cache) = (open.lib.clone(), open.cache.clone());
    // It writes the catalog, which can wait for other writers: never on a
    // command thread (a filter or a scroll would wait behind it). Only the
    // latest set matters: one waiting replaces the one before.
    let n = ids.len() as u64;
    let start = {
        let mut slot = VISIBLE.lock().map_err(|_| "visible state poisoned")?;
        let idle = slot.0.is_none();
        slot.1 = Some(ids);
        if idle {
            slot.0 = Some(());
        }
        idle
    };
    if start {
        std::thread::Builder::new()
            .name("render-visible".into())
            .spawn(move || loop {
                let next = match VISIBLE.lock() {
                    Ok(mut slot) => match slot.1.take() {
                        Some(ids) => ids,
                        None => {
                            slot.0 = None;
                            return;
                        }
                    },
                    Err(_) => return,
                };
                let t0 = std::time::Instant::now();
                let r = lib
                    .open_catalog()
                    .map_err(|e| e.to_string())
                    .and_then(|mut cat| {
                        render::set_visible(&mut cat, &cache, &next).map_err(|e| e.to_string())
                    });
                if let Err(e) = r {
                    tracing::warn!("render_set_visible: {e}");
                }
                if t0.elapsed().as_millis() >= 1000 {
                    tracing::warn!(ms = t0.elapsed().as_millis(), "slow render_set_visible");
                }
            })
            .map_err(err)?;
    }
    Ok(n)
}

/// The on-screen update's thread: running, and the set waiting for it.
static VISIBLE: std::sync::Mutex<(Option<()>, Option<Vec<keepsake_core::catalog::AssetId>>)> =
    std::sync::Mutex::new((None, None));

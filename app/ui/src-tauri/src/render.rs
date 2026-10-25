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
}

impl RenderState {
    fn stop(&self) {
        if let Ok(c) = self.cancel.lock() {
            c.store(true, Ordering::SeqCst);
        }
    }
}

/// Stop the service (it finishes its current job in the background) and
/// start one with the current settings.
pub fn restart_service(app: &AppHandle) -> CmdResult<bool> {
    let state = app.state::<AppState>();
    state.render.stop();
    if let Ok(mut slot) = state.render.worker.lock() {
        slot.take();
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
    let mut opts = RenderOptions::from_settings(&open.settings.render);
    opts.until_empty = false;
    let svc = RenderService::new(&open.lib, RenderTools::discover(), open.cache.clone(), opts)
        .with_power(power());
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
            let result = svc.run(&cancel, &mut |ev| {
                if let RenderEvent::Done { asset, made, .. } = ev {
                    if made.iter().any(|m| m == "thumb-256") {
                        let _ = app.emit(
                            "render://thumbs",
                            ThumbsReady {
                                ids: vec![asset.to_string()],
                            },
                        );
                    }
                }
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
    let mut cat = open.writer()?;
    render::set_visible(&mut cat, &open.cache, &ids).map_err(err)
}

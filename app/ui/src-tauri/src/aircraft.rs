//! Read aircraft for the UI (ADR-041): turning it on (which downloads the
//! two text models on a background thread, `aircraft://install`) or off,
//! its status, and queueing, following and stopping the photos the user
//! picked. The work is in `keepsake_core::ai::aircraft`; reading runs in
//! the render service.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use keepsake_core::ai::aircraft::{install, scan};
use keepsake_core::ai::install as ai_install;
use keepsake_core::catalog::AssetId;
use keepsake_core::settings::Settings;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::{AiInstallProgress, AircraftStatus, CloudStarted, CloudStatusDto, Selection};
use crate::convert;
use crate::state::{err, AppState, CmdResult};

/// What the aircraft worker and the download last said.
#[derive(Default)]
pub struct AircraftState {
    /// Where the text detector runs, once the aircraft worker loaded it.
    accel: Mutex<Option<String>>,
    /// Why the aircraft worker did not start.
    problem: Mutex<Option<String>>,
    /// The running download: its stop flag and bytes so far.
    install: Mutex<Option<(Arc<AtomicBool>, u64)>>,
}

fn set<T>(m: &Mutex<T>, v: T) {
    if let Ok(mut g) = m.lock() {
        *g = v;
    }
}

fn get<T: Clone>(m: &Mutex<T>) -> Option<T> {
    m.lock().ok().map(|g| g.clone())
}

impl AircraftState {
    /// The render service loaded the text models (`accel`) or could not
    /// (`problem`).
    pub fn note(&self, accel: Option<String>, problem: Option<String>) {
        set(&self.accel, accel);
        set(&self.problem, problem);
    }
}

#[tauri::command(async)]
pub fn aircraft_status(state: State<'_, AppState>) -> CmdResult<AircraftStatus> {
    let settings = Settings::load().map_err(err)?;
    let on = settings.ai.aircraft;
    let st = &state.aircraft;
    Ok(AircraftStatus {
        enabled: on,
        models_installed: install::installed(),
        models_bytes: install::TOTAL_BYTES,
        runtime_found: ai_install::find_runtime().is_some(),
        looks: ai_install::find_model().is_some(),
        accel: get(&st.accel).flatten().filter(|_| on),
        problem: get(&st.problem).flatten().filter(|_| on),
        installing: get(&st.install).flatten().map(|(_, done)| done),
    })
}

fn save_and_restart(app: &AppHandle, enabled: bool) -> CmdResult<()> {
    let mut s = Settings::load().map_err(err)?;
    s.ai.aircraft = enabled;
    s.save().map_err(err)?;
    let state = app.state::<AppState>();
    state.aircraft.note(None, None);
    if state.try_open().is_some() {
        state.reload_settings()?;
        crate::render::restart_service(app)?;
    }
    let _ = app.emit("aircraft://status", ());
    Ok(())
}

/// Turn Read aircraft on or off. On: downloads the two text models first
/// if needed (progress on `aircraft://install`).
#[tauri::command(async)]
pub fn aircraft_enable(app: AppHandle, enabled: bool) -> CmdResult<()> {
    if !enabled || install::installed() {
        return save_and_restart(&app, enabled);
    }
    let state = app.state::<AppState>();
    let cancel = {
        let mut slot = state
            .aircraft
            .install
            .lock()
            .map_err(|_| "aircraft state poisoned")?;
        if slot.is_some() {
            return Ok(()); // already downloading
        }
        let c = Arc::new(AtomicBool::new(false));
        *slot = Some((Arc::clone(&c), 0));
        c
    };
    let app2 = app.clone();
    std::thread::Builder::new()
        .name("aircraft-install".into())
        .spawn(move || {
            let st = app2.state::<AppState>();
            let r = install::install(&cancel, &mut |done, total| {
                if let Ok(mut s) = st.aircraft.install.lock() {
                    if let Some(slot) = s.as_mut() {
                        slot.1 = done;
                    }
                }
                let _ = app2.emit(
                    "aircraft://install",
                    AiInstallProgress {
                        done,
                        total,
                        finished: false,
                        error: None,
                    },
                );
            });
            set(&st.aircraft.install, None);
            let error = match r {
                Ok(()) => save_and_restart(&app2, true).err(),
                Err(keepsake_core::ai::AiError::Cancelled) => Some("cancelled".to_owned()),
                Err(e) => Some(e.to_string()),
            };
            let _ = app2.emit(
                "aircraft://install",
                AiInstallProgress {
                    done: 0,
                    total: install::TOTAL_BYTES,
                    finished: true,
                    error,
                },
            );
        })
        .map_err(err)?;
    Ok(())
}

#[tauri::command(async)]
pub fn aircraft_install_cancel(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some((c, _)) = get(&state.aircraft.install).flatten() {
        c.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Turn Read aircraft off and delete the downloaded text models. Keywords
/// it gave stay.
#[tauri::command(async)]
pub fn aircraft_remove_models(app: AppHandle) -> CmdResult<bool> {
    save_and_restart(&app, false)?;
    install::remove().map_err(err)
}

fn dto((s, problem): (keepsake_core::ai::cloud::CloudStatus, Option<String>)) -> CloudStatusDto {
    CloudStatusDto {
        items: s.items,
        analysed: s.done,
        pending: s.pending,
        failed: s.failed,
        problem,
    }
}

fn unique(ids: impl IntoIterator<Item = AssetId>) -> Vec<AssetId> {
    let mut out: Vec<AssetId> = Vec::new();
    for id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// Read the aircraft in the selected items (in the background).
#[tauri::command(async)]
pub fn aircraft_read(
    app: AppHandle,
    state: State<'_, AppState>,
    selection: Selection,
) -> CmdResult<CloudStarted> {
    if !Settings::load().map_err(err)?.ai.aircraft {
        return Err("Turn on Read aircraft in Settings first.".into());
    }
    let ids = unique(crate::edit::resolve(&state, &selection)?);
    if ids.is_empty() {
        return Err("Select the photos to read first.".into());
    }
    let open = state.open()?;
    {
        let mut cat = open.writer()?;
        scan::enqueue(&mut cat, &ids).map_err(err)?;
    }
    crate::render::start_service(&app)?;
    let cat = open.cat()?;
    Ok(CloudStarted {
        provider: scan::SOURCE.to_owned(),
        ids: ids.iter().map(ToString::to_string).collect(),
        status: dto(scan::status(&cat, &ids).map_err(err)?),
    })
}

#[tauri::command(async)]
pub fn aircraft_read_status(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> CmdResult<CloudStatusDto> {
    let ids = unique(convert::ids(&ids)?);
    let open = state.open()?;
    let cat = open.cat()?;
    let mut st = dto(scan::status(&cat, &ids).map_err(err)?);
    // Nothing moves while the worker is not running: say why.
    if st.pending > 0 && st.problem.is_none() {
        st.problem = get(&state.aircraft.problem)
            .flatten()
            .map(|p| format!("Not running: {p}"));
    }
    Ok(st)
}

/// Read the aircraft photos of a folder or dates (those tagged Airplane,
/// Helicopter or Airport and not read yet), in the background.
pub fn target_start(
    app: &AppHandle,
    state: &AppState,
    target: &keepsake_core::ai::Target,
) -> CmdResult<keepsake_core::ai::analyze::RangeStatus> {
    if !Settings::load().map_err(err)?.ai.aircraft {
        return Err("Turn on Read aircraft in Settings first.".into());
    }
    let open = state.open()?;
    let st = {
        let mut cat = open.writer()?;
        scan::enqueue_target(&mut cat, target).map_err(err)?
    };
    crate::render::start_service(app)?;
    Ok(st)
}

pub fn target_status(
    state: &AppState,
    target: &keepsake_core::ai::Target,
) -> CmdResult<keepsake_core::ai::analyze::RangeStatus> {
    let open = state.open()?;
    let cat = open.cat()?;
    scan::target_status(&cat, target).map_err(err)
}

pub fn target_stop(state: &AppState, target: &keepsake_core::ai::Target) -> CmdResult<()> {
    let open = state.open()?;
    let mut cat = open.writer()?;
    scan::stop_target(&mut cat, target).map_err(err)?;
    Ok(())
}

/// Stop: items not read yet are not read; keywords already added stay.
#[tauri::command(async)]
pub fn aircraft_read_stop(state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<()> {
    let ids = unique(convert::ids(&ids)?);
    let open = state.open()?;
    let mut cat = open.writer()?;
    scan::stop(&mut cat, &ids).map_err(err)?;
    Ok(())
}

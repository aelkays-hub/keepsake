//! Faces for the UI (ADR-040, step 2): turning Recognise faces on (which
//! downloads the two face models on a background thread, `faces://install`)
//! or off, its status, searching every photo now, stopping, and deleting
//! all face data. The work is in `keepsake_core::ai::faces`; searching runs
//! in the render service.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use keepsake_core::ai::faces::{install, scan};
use keepsake_core::ai::install as ai_install;
use keepsake_core::settings::Settings;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::{AiInstallProgress, FacesStatus};
use crate::state::{err, AppState, CmdResult};

/// What the face worker and the download last said.
#[derive(Default)]
pub struct FacesState {
    /// Where the face models run, once the face worker loaded them.
    accel: Mutex<Option<String>>,
    /// Why the face worker did not start.
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

impl FacesState {
    /// The render service loaded the face models (`accel`) or could not
    /// (`problem`).
    pub fn note(&self, accel: Option<String>, problem: Option<String>) {
        set(&self.accel, accel);
        set(&self.problem, problem);
    }
}

#[tauri::command(async)]
pub fn faces_status(state: State<'_, AppState>) -> CmdResult<FacesStatus> {
    let settings = Settings::load().map_err(err)?;
    let counts = match state.try_open() {
        Some(open) => {
            let cat = open.cat()?;
            scan::status(&cat).map_err(err)?
        }
        None => Default::default(),
    };
    let on = settings.ai.enabled && settings.ai.faces;
    let st = &state.faces;
    Ok(FacesStatus {
        enabled: settings.ai.faces,
        ai_enabled: settings.ai.enabled,
        models_installed: install::installed(),
        models_bytes: install::TOTAL_BYTES,
        runtime_found: ai_install::find_runtime().is_some(),
        accel: get(&st.accel).flatten().filter(|_| on),
        problem: get(&st.problem).flatten().filter(|_| on),
        installing: get(&st.install).flatten().map(|(_, done)| done),
        photos: counts.photos,
        scanned: counts.scanned,
        with_faces: counts.with_faces,
        faces: counts.faces,
        people: counts.people,
        pending: counts.pending,
        failed: counts.failed,
    })
}

fn save_and_restart(app: &AppHandle, enabled: bool) -> CmdResult<()> {
    let mut s = Settings::load().map_err(err)?;
    s.ai.faces = enabled;
    s.save().map_err(err)?;
    let state = app.state::<AppState>();
    state.faces.note(None, None);
    if state.try_open().is_some() {
        state.reload_settings()?;
        crate::render::restart_service(app)?;
    }
    let _ = app.emit("faces://status", ());
    Ok(())
}

/// Turn Recognise faces on or off. On: downloads the two face models first
/// if needed (progress on `faces://install`). Needs On-device AI on.
#[tauri::command(async)]
pub fn faces_enable(app: AppHandle, enabled: bool) -> CmdResult<()> {
    if enabled && !Settings::load().map_err(err)?.ai.enabled {
        return Err("Turn on On-device AI first (just above).".into());
    }
    if !enabled || install::installed() {
        return save_and_restart(&app, enabled);
    }
    let state = app.state::<AppState>();
    let cancel = {
        let mut slot = state
            .faces
            .install
            .lock()
            .map_err(|_| "faces state poisoned")?;
        if slot.is_some() {
            return Ok(()); // already downloading
        }
        let c = Arc::new(AtomicBool::new(false));
        *slot = Some((Arc::clone(&c), 0));
        c
    };
    let app2 = app.clone();
    std::thread::Builder::new()
        .name("faces-install".into())
        .spawn(move || {
            let st = app2.state::<AppState>();
            let r = install::install(&cancel, &mut |done, total| {
                if let Ok(mut s) = st.faces.install.lock() {
                    if let Some(slot) = s.as_mut() {
                        slot.1 = done;
                    }
                }
                let _ = app2.emit(
                    "faces://install",
                    AiInstallProgress {
                        done,
                        total,
                        finished: false,
                        error: None,
                    },
                );
            });
            set(&st.faces.install, None);
            let error = match r {
                Ok(()) => save_and_restart(&app2, true).err(),
                Err(keepsake_core::ai::AiError::Cancelled) => Some("cancelled".to_owned()),
                Err(e) => Some(e.to_string()),
            };
            let _ = app2.emit(
                "faces://install",
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
pub fn faces_install_cancel(state: State<'_, AppState>) -> CmdResult<()> {
    if let Some((c, _)) = get(&state.faces.install).flatten() {
        c.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Search every photo not yet searched, in the background. Returns how
/// many were queued.
#[tauri::command(async)]
pub fn faces_scan_all(app: AppHandle, state: State<'_, AppState>) -> CmdResult<u64> {
    let s = Settings::load().map_err(err)?;
    if !(s.ai.enabled && s.ai.faces) {
        return Err("Turn on Recognise faces first.".into());
    }
    let open = state.open()?;
    let n = {
        let mut cat = open.writer()?;
        scan::enqueue_unscanned(&mut cat).map_err(err)?
    };
    crate::render::start_service(&app)?;
    Ok(n)
}

/// Stop searching: the photos not searched yet are not; what was found
/// stays.
#[tauri::command(async)]
pub fn faces_stop(state: State<'_, AppState>) -> CmdResult<u64> {
    let open = state.open()?;
    let mut cat = open.writer()?;
    scan::stop(&mut cat).map_err(err)
}

/// Delete all face data: every face, person and decision, and the queued
/// searches. Originals and the face models are not touched. Returns how
/// many faces there were.
#[tauri::command(async)]
pub fn faces_delete_all(app: AppHandle, state: State<'_, AppState>) -> CmdResult<u64> {
    let open = state.open()?;
    let n = {
        let mut cat = open.writer()?;
        scan::delete_all(&mut cat).map_err(err)?
    };
    let _ = app.emit("faces://status", ());
    Ok(n)
}

/// Turn Recognise faces off and delete the downloaded face models. Face
/// data stays (Delete all face data removes it).
#[tauri::command(async)]
pub fn faces_remove_models(app: AppHandle) -> CmdResult<bool> {
    save_and_restart(&app, false)?;
    install::remove().map_err(err)
}

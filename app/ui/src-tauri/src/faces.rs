//! Faces for the UI (ADR-040, step 2): turning Recognise faces on (which
//! downloads the two face models on a background thread, `faces://install`)
//! or off, its status, searching every photo now, stopping, and deleting
//! all face data. The work is in `keepsake_core::ai::faces`; searching runs
//! in the render service.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use keepsake_core::ai::faces::{install, people, scan};
use keepsake_core::ai::install as ai_install;
use keepsake_core::catalog::{AssetId, Catalog, FaceRow, FaceState};
use keepsake_core::render::{source, Cache, Variant};
use keepsake_core::settings::Settings;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::{AiInstallProgress, FaceDto, FacesStatus, PersonDto, PhotoFacesDto, SuggestDto};
use crate::convert;
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
        scan::delete_all(&open.lib, &mut cat).map_err(err)?
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

// ── Name and find (ADR-040, step 3) ────────────────────────────────────────

fn faces_on() -> CmdResult<()> {
    let s = Settings::load().map_err(err)?;
    if s.ai.enabled && s.ai.faces {
        Ok(())
    } else {
        Err("Turn on Recognise faces first (Settings).".into())
    }
}

fn thumb(cat: &Catalog, cache: &Cache, asset: AssetId) -> Option<String> {
    let hash = source::source_hash(cat, asset).ok()??;
    convert::cached(cache, Some(&hash), Variant::Thumb1024)
        .or_else(|| convert::cached(cache, Some(&hash), Variant::Thumb256))
}

fn face_dto(cat: &Catalog, cache: &Cache, f: &FaceRow) -> CmdResult<FaceDto> {
    let person = match f.person {
        Some(p) => cat
            .read(|tx| tx.faces().person(p))
            .map_err(err)?
            .map(|p| p.name),
        None => None,
    };
    let (pw, ph) = cat
        .read(|tx| tx.faces().scan(f.asset_id))
        .map_err(err)?
        .map_or((1024, 768), |s| (s.width, s.height));
    Ok(FaceDto {
        id: f.id,
        photo: f.asset_id.to_string(),
        thumb: thumb(cat, cache, f.asset_id),
        pw,
        ph,
        x: f.face.x,
        y: f.face.y,
        w: f.face.w,
        h: f.face.h,
        person,
        state: f.state.as_str().to_owned(),
        likeness: f
            .likeness
            .map(|l| (l.clamp(0.0, 1.0) * 100.0).round() as u8),
        date: f
            .capture_ms
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|d| d.format("%Y-%m-%d").to_string()),
    })
}

fn suggest_dto(o: people::SuggestOutcome) -> SuggestDto {
    SuggestDto {
        added: o.added,
        waiting: o.waiting,
        confirmed: o.confirmed,
        from_year: o.from_year,
        to_year: o.to_year,
    }
}

/// The faces in a photo (not those hidden as "not a face"). One not
/// searched yet is searched now, ahead of the rest (`searched` false until
/// it is: ask again), unless `search` is false (the Inspector, the viewer).
#[tauri::command(async)]
pub fn faces_of_photo(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    search: Option<bool>,
) -> CmdResult<PhotoFacesDto> {
    faces_on()?;
    let open = state.open()?;
    let asset: AssetId = convert::id(&id, "asset")?;
    let (searched, rows) = {
        let cat = open.cat()?;
        let searched = cat
            .read(|tx| tx.faces().scan(asset))
            .map_err(err)?
            .is_some_and(|s| s.model == keepsake_core::ai::faces::MODEL_ID);
        (
            searched,
            cat.read(|tx| tx.faces().for_asset(asset)).map_err(err)?,
        )
    };
    if !searched && search.unwrap_or(true) {
        {
            let mut cat = open.writer()?;
            scan::search_now(&mut cat, asset).map_err(err)?;
        }
        crate::render::start_service(&app)?;
    }
    let cat = open.cat()?;
    let faces = rows
        .iter()
        .filter(|f| f.state != FaceState::Hidden)
        .map(|f| face_dto(&cat, &open.cache, f))
        .collect::<CmdResult<_>>()?;
    Ok(PhotoFacesDto { searched, faces })
}

/// Everyone named, by name.
#[tauri::command(async)]
pub fn faces_people(state: State<'_, AppState>) -> CmdResult<Vec<PersonDto>> {
    let open = state.open()?;
    let cat = open.cat()?;
    let list = cat.read(|tx| tx.faces().people()).map_err(err)?;
    list.into_iter()
        .map(|p| {
            let confirmed = cat
                .read(|tx| tx.faces().of_person(p.id, FaceState::Confirmed))
                .map_err(err)?
                .len() as u64;
            Ok(PersonDto {
                id: p.id,
                name: p.name,
                confirmed,
            })
        })
        .collect()
}

/// Name a face: it is that person's (a new name adds a person).
#[tauri::command(async)]
pub fn faces_name(state: State<'_, AppState>, face: i64, name: String) -> CmdResult<PersonDto> {
    let open = state.open()?;
    let mut cat = open.writer()?;
    let id = people::name_face(
        &open.lib,
        &mut cat,
        face,
        &name,
        keepsake_core::jobs::now_ms(),
    )
    .map_err(err)?;
    let p = cat
        .read(|tx| tx.faces().person(id))
        .map_err(err)?
        .ok_or("the person was not saved")?;
    let confirmed = cat
        .read(|tx| tx.faces().of_person(id, FaceState::Confirmed))
        .map_err(err)?
        .len() as u64;
    Ok(PersonDto {
        id,
        name: p.name,
        confirmed,
    })
}

/// Find: search every photo not searched yet (in the background) so the
/// person can be looked for in all of them. Returns how many were queued;
/// follow it with `faces_status`, then `faces_suggest`.
#[tauri::command(async)]
pub fn faces_find(app: AppHandle, state: State<'_, AppState>) -> CmdResult<u64> {
    faces_on()?;
    let open = state.open()?;
    let n = {
        let mut cat = open.writer()?;
        scan::enqueue_unscanned(&mut cat).map_err(err)?
    };
    if n > 0 {
        crate::render::start_service(&app)?;
    }
    Ok(n)
}

/// Suggest the person's faces among the faces found so far (Find more:
/// again, with the faces confirmed since).
#[tauri::command(async)]
pub fn faces_suggest(state: State<'_, AppState>, person: i64) -> CmdResult<SuggestDto> {
    let open = state.open()?;
    let mut cat = open.writer()?;
    Ok(suggest_dto(people::suggest(&mut cat, person).map_err(err)?))
}

/// The person's suggestions waiting for an answer, best first.
#[tauri::command(async)]
pub fn faces_review(state: State<'_, AppState>, person: i64) -> CmdResult<Vec<FaceDto>> {
    let open = state.open()?;
    let cat = open.cat()?;
    people::review(&cat, person)
        .map_err(err)?
        .iter()
        .map(|f| face_dto(&cat, &open.cache, f))
        .collect()
}

/// The user's answers on suggestions: `confirm` are the person's, `reject`
/// are not. Returns `[confirmed, rejected]`.
#[tauri::command(async)]
pub fn faces_decide(
    state: State<'_, AppState>,
    person: i64,
    confirm: Vec<i64>,
    reject: Vec<i64>,
) -> CmdResult<[u64; 2]> {
    let open = state.open()?;
    let mut cat = open.writer()?;
    let (c, r) = people::decide(&open.lib, &mut cat, person, &confirm, &reject).map_err(err)?;
    Ok([c, r])
}

/// "Not this person" on a face (the viewer's face menu). Returns whether it
/// had a person.
#[tauri::command(async)]
pub fn faces_not_person(state: State<'_, AppState>, face: i64) -> CmdResult<bool> {
    let open = state.open()?;
    let mut cat = open.writer()?;
    people::not_this_person(&open.lib, &mut cat, face).map_err(err)
}

/// "Not a face": hidden for good.
#[tauri::command(async)]
pub fn faces_not_face(state: State<'_, AppState>, face: i64) -> CmdResult<()> {
    let open = state.open()?;
    let mut cat = open.writer()?;
    people::not_a_face(&open.lib, &mut cat, face).map_err(err)
}

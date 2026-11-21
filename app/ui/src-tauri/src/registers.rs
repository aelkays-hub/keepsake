//! National aircraft registers for the UI (ADR-045): Transport Canada's
//! and the FAA's, downloaded and imported on request (Settings → Read
//! aircraft), in the background; and one registration's entry for the
//! inspector. The work is in `keepsake_core::ai::aircraft::register`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use keepsake_core::ai::aircraft::register::{self, Register, Updated};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::{RegisterEntryDto, RegisterProgress, RegisterStatusDto};
use crate::state::{err, AppState, CmdResult};

/// The registers being updated: their stop flag, bytes so far, and
/// whether the download is done (being imported).
/// A register being updated: its stop flag, bytes so far, and whether it
/// is being imported.
type Running = (Arc<AtomicBool>, u64, bool);

#[derive(Default)]
pub struct RegistersState {
    running: Mutex<HashMap<&'static str, Running>>,
}

fn date(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_default()
}

/// Each register: downloaded or not, when, how many aircraft, and an
/// update under way.
#[tauri::command(async)]
pub fn aircraft_registers(state: State<'_, AppState>) -> CmdResult<Vec<RegisterStatusDto>> {
    let open = state.open()?;
    let cat = open.cat()?;
    let sources = register::sources(&cat).map_err(err)?;
    let running = state
        .registers
        .running
        .lock()
        .map(|m| m.clone())
        .unwrap_or_default();
    Ok(Register::ALL
        .into_iter()
        .map(|r| {
            let s = sources.iter().find(|s| s.source == r.id());
            let run = running.get(r.id());
            RegisterStatusDto {
                id: r.id().to_owned(),
                name: r.name().to_owned(),
                bytes: r.bytes(),
                downloaded: s.is_some(),
                file_modified: s.and_then(|s| s.file_modified.clone()),
                imported: s.map(|s| date(s.imported_ms)),
                rows: s.map_or(0, |s| u64::try_from(s.rows).unwrap_or(0)),
                running: run.is_some(),
                done: run.map_or(0, |r| r.1),
                importing: run.is_some_and(|r| r.2),
            }
        })
        .collect())
}

fn which(id: &str) -> CmdResult<Register> {
    Register::parse(id).ok_or_else(|| format!("{id} is not a register"))
}

/// Download `id`'s register if it changed since the one imported, and
/// import it, in the background. Progress: `aircraft://register` events.
#[tauri::command(async)]
pub fn aircraft_register_update(app: AppHandle, id: String) -> CmdResult<()> {
    let r = which(&id)?;
    let state = app.state::<AppState>();
    let open = state.open()?;
    let cancel = {
        let mut m = state
            .registers
            .running
            .lock()
            .map_err(|_| "register state poisoned")?;
        if m.contains_key(r.id()) {
            return Ok(()); // already updating
        }
        let c = Arc::new(AtomicBool::new(false));
        m.insert(r.id(), (Arc::clone(&c), 0, false));
        c
    };
    let app2 = app.clone();
    std::thread::Builder::new()
        .name(format!("register-{}", r.id()))
        .spawn(move || {
            let st = app2.state::<AppState>();
            let total = r.bytes();
            let emit = |p: RegisterProgress| {
                let _ = app2.emit("aircraft://register", p);
            };
            let result = (|| -> Result<Updated, String> {
                let mut cat = open.writer()?;
                register::update(&mut cat, r, &cancel, &mut |done, total| {
                    // The download is done: it is being read and imported.
                    let importing = total > 0 && done >= total;
                    if let Ok(mut m) = st.registers.running.lock() {
                        if let Some(e) = m.get_mut(r.id()) {
                            e.1 = done;
                            e.2 = importing;
                        }
                    }
                    emit(RegisterProgress {
                        id: r.id().to_owned(),
                        done,
                        total,
                        importing,
                        finished: false,
                        message: None,
                        error: None,
                    });
                })
                .map_err(|e| e.to_string())
            })();
            if let Ok(mut m) = st.registers.running.lock() {
                m.remove(r.id());
            }
            let (message, error) = match result {
                Ok(Updated::Unchanged) => (Some("Already up to date.".to_owned()), None),
                Ok(Updated::Imported(n)) => (Some(format!("{n} aircraft imported.")), None),
                Err(e) if cancel.load(Ordering::SeqCst) => (None, Some(format!("Stopped ({e})."))),
                Err(e) => (None, Some(e)),
            };
            emit(RegisterProgress {
                id: r.id().to_owned(),
                done: 0,
                total,
                importing: false,
                finished: true,
                message,
                error,
            });
            let _ = app2.emit("aircraft://status", ());
        })
        .map_err(err)?;
    Ok(())
}

/// Stop `id`'s download (an import under way finishes).
#[tauri::command(async)]
pub fn aircraft_register_cancel(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let r = which(&id)?;
    if let Ok(m) = state.registers.running.lock() {
        if let Some((c, _, _)) = m.get(r.id()) {
            c.store(true, Ordering::SeqCst);
        }
    }
    Ok(())
}

/// Delete `id`'s register from the catalog.
#[tauri::command(async)]
pub fn aircraft_register_remove(app: AppHandle, id: String) -> CmdResult<()> {
    let r = which(&id)?;
    let state = app.state::<AppState>();
    let open = state.open()?;
    let mut cat = open.writer()?;
    register::remove(&mut cat, r).map_err(err)?;
    let _ = app.emit("aircraft://status", ());
    Ok(())
}

/// What a national register says about `registration` (the inspector).
#[tauri::command(async)]
pub fn aircraft_register(
    state: State<'_, AppState>,
    registration: String,
) -> CmdResult<Option<RegisterEntryDto>> {
    let open = state.open()?;
    let cat = open.cat()?;
    let row = register::get(&cat, &registration).map_err(err)?;
    Ok(row.map(|r| RegisterEntryDto {
        registration: r.registration,
        register: Register::parse(&r.source)
            .map(|x| x.name().to_owned())
            .unwrap_or(r.source),
        aircraft_type: r.aircraft_type,
        maker: r.maker,
        model: r.model,
        serial: r.serial,
        built: r.built,
        mode_s: r.mode_s,
        owner: r.owner,
        airline: r.airline,
        current: r.current,
    }))
}

//! The fin table for the UI (ADR-052; Settings → Read aircraft → Fin
//! numbers): which registration an airline's fin stood for, learned from
//! accepted photos or imported from a CSV file the user chose. The work is
//! in `keepsake_core::ai::aircraft::fins`.

use std::path::PathBuf;

use keepsake_core::ai::aircraft::fins;
use keepsake_core::ai::aircraft::marks::Airlines;
use tauri::State;

use crate::api::{FinDto, FinImportDto};
use crate::state::{err, AppState, CmdResult};

fn count(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Every fin kept, by airline and fin, each marked when another
/// registration has it at the same time.
#[tauri::command(async)]
pub fn aircraft_fins(state: State<'_, AppState>) -> CmdResult<Vec<FinDto>> {
    let open = state.open()?;
    let cat = open.cat()?;
    let rows = cat.read(|tx| tx.ai().fins_all()).map_err(err)?;
    let conflicts = fins::conflicts(&rows);
    Ok(rows
        .into_iter()
        .zip(conflicts)
        .map(|(r, conflict)| FinDto {
            airline: r.airline,
            fin: r.fin,
            registration: r.registration,
            from: r.first_ms.map(fins::day_text),
            to: r.last_ms.map(fins::day_text),
            source: r.source,
            seen: count(u64::try_from(r.seen).unwrap_or(0)),
            conflict,
        })
        .collect())
}

/// Import the CSV file at `path` (from the open dialog): its rows added or
/// updated, and the lines that could not be kept.
#[tauri::command(async)]
pub fn aircraft_fins_import(state: State<'_, AppState>, path: String) -> CmdResult<FinImportDto> {
    let path = path.trim();
    if path.is_empty() {
        return Err("choose a CSV file of fins".into());
    }
    let bytes = std::fs::read(path).map_err(|e| format!("{path} could not be read: {e}"))?;
    let parsed = fins::parse_csv(&String::from_utf8_lossy(&bytes), &Airlines::builtin());
    let (added, updated) = if parsed.rows.is_empty() {
        (0, 0)
    } else {
        let open = state.open()?;
        let mut cat = open.writer()?;
        fins::import(&mut cat, &parsed.rows).map_err(err)?
    };
    Ok(FinImportDto {
        added: count(added),
        updated: count(updated),
        problems: parsed.problems,
    })
}

/// Write the fin table as CSV to `path` (from the save dialog).
#[tauri::command(async)]
pub fn aircraft_fins_csv(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("choose where to save the fins".into());
    }
    let mut out = PathBuf::from(path);
    if out.extension().is_none() {
        out.set_extension("csv");
    }
    let open = state.open()?;
    let cat = open.cat()?;
    let rows = cat.read(|tx| tx.ai().fins_all()).map_err(err)?;
    fins::write_csv(&rows, &out).map_err(err)?;
    Ok(crate::convert::path_text(&out))
}

/// Fill the table from the photos already tagged with a registration and
/// an airline whose read kept a fin. Returns how many taught one.
#[tauri::command(async)]
pub fn aircraft_fins_learn(state: State<'_, AppState>) -> CmdResult<u32> {
    let open = state.open()?;
    let mut cat = open.writer()?;
    Ok(count(fins::learn_from_library(&mut cat).map_err(err)?))
}

/// Delete one fin row.
#[tauri::command(async)]
pub fn aircraft_fin_forget(
    state: State<'_, AppState>,
    airline: String,
    fin: String,
    registration: String,
    source: String,
) -> CmdResult<()> {
    let open = state.open()?;
    let mut cat = open.writer()?;
    cat.write(|tx| tx.ai().forget_fin(&airline, &fin, &registration, &source))
        .map_err(err)?;
    Ok(())
}

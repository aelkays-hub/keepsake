//! Tools → Report: photo and video counts and sizes by date, camera, lens,
//! file type and drive, and the same as a CSV file. Read-only; the numbers
//! come from `keepsake_core::catalog::Report` (catalog queries only, no
//! media file is read). The aircraft report (`aircraft_report`) is the
//! photos of aircraft and the airlines, types and registrations on them.

use std::path::PathBuf;

use keepsake_core::catalog::{
    AircraftGroup, AircraftReport as CoreAircraft, AircraftRow, LibraryReport as CoreReport,
    ReportRow,
};
use keepsake_core::report;
use tauri::State;

use crate::api::{
    AircraftReport, AircraftReportGroup, AircraftReportRow, AircraftReportTotals,
    AircraftReportYear, LibraryReport, ReportDrive, ReportGroup,
};
use crate::state::{err, AppState, CmdResult};

fn group(r: &ReportRow) -> ReportGroup {
    ReportGroup {
        group: r.group.clone(),
        items: r.items,
        photos: r.photos,
        videos: r.videos,
        files: r.files,
        bytes: r.bytes,
    }
}

fn groups(rows: &[ReportRow]) -> Vec<ReportGroup> {
    rows.iter().map(group).collect()
}

fn dto(r: &CoreReport) -> LibraryReport {
    LibraryReport {
        totals: group(&r.totals),
        years: groups(&r.years),
        months: groups(&r.months),
        days: groups(&r.days),
        cameras: groups(&r.cameras),
        lenses: groups(&r.lenses),
        file_types: groups(&r.file_types),
        drives: r
            .drives
            .iter()
            .map(|d| ReportDrive {
                volume: d.volume_id.to_string(),
                online: d.online,
                row: group(&d.row),
            })
            .collect(),
    }
}

fn build(state: &AppState) -> CmdResult<CoreReport> {
    let open = state.open()?;
    let cat = open.cat()?;
    cat.read(|tx| tx.report().library()).map_err(err)
}

/// The whole report.
#[tauri::command(async)]
pub fn library_report(state: State<'_, AppState>) -> CmdResult<LibraryReport> {
    Ok(dto(&build(&state)?))
}

/// Write the report as CSV to `path` (from the save dialog); `.csv` is added
/// when the name has no extension. Returns the path written.
#[tauri::command(async)]
pub fn library_report_csv(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("choose where to save the report".into());
    }
    let mut out = PathBuf::from(path);
    if out.extension().is_none() {
        out.set_extension("csv");
    }
    report::write_csv(&build(&state)?, &out).map_err(err)?;
    Ok(crate::convert::path_text(&out))
}

fn aircraft_row(r: &AircraftRow) -> AircraftReportRow {
    AircraftReportRow {
        name: r.name.clone(),
        path: r.path.clone(),
        photos: r.photos,
        registrations: r.registrations,
        airlines: r.airlines.clone(),
        types: r.types.clone(),
        first: r.first.clone(),
        last: r.last.clone(),
    }
}

fn aircraft_group(g: &AircraftGroup) -> AircraftReportGroup {
    AircraftReportGroup {
        name: g.name.clone(),
        photos: g.photos,
        children: g.children.iter().map(aircraft_group).collect(),
    }
}

fn aircraft_dto(r: &CoreAircraft) -> AircraftReport {
    let t = &r.totals;
    let rows = |v: &[AircraftRow]| v.iter().map(aircraft_row).collect();
    AircraftReport {
        totals: AircraftReportTotals {
            photos: t.photos,
            shots: t.shots,
            with_marks: t.with_marks,
            with_airline: t.with_airline,
            with_type: t.with_type,
            with_registration: t.with_registration,
            airlines: t.airlines,
            types: t.types,
            registrations: t.registrations,
            waiting_review: t.waiting_review,
        },
        airlines: rows(&r.airlines),
        types: rows(&r.types),
        registrations: rows(&r.registrations),
        years: r
            .years
            .iter()
            .map(|y| AircraftReportYear {
                year: y.year.clone(),
                photos: y.photos,
                with_marks: y.with_marks,
            })
            .collect(),
        by_airline: r.by_airline.iter().map(aircraft_group).collect(),
    }
}

fn build_aircraft(state: &AppState) -> CmdResult<CoreAircraft> {
    let open = state.open()?;
    let cat = open.cat()?;
    cat.read(|tx| tx.aircraft_report().aircraft()).map_err(err)
}

/// Tools → Report → Aircraft: aircraft photos and the airlines, types and
/// registrations named on them.
#[tauri::command(async)]
pub fn aircraft_report(state: State<'_, AppState>) -> CmdResult<AircraftReport> {
    Ok(aircraft_dto(&build_aircraft(&state)?))
}

/// Write the aircraft report as CSV to `path` (from the save dialog).
#[tauri::command(async)]
pub fn aircraft_report_csv(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("choose where to save the report".into());
    }
    let mut out = PathBuf::from(path);
    if out.extension().is_none() {
        out.set_extension("csv");
    }
    report::write_aircraft_csv(&build_aircraft(&state)?, &out).map_err(err)?;
    Ok(crate::convert::path_text(&out))
}

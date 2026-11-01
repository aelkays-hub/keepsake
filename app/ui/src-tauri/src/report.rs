//! Tools → Report: photo and video counts and sizes by date, camera, lens,
//! file type and drive, and the same as a CSV file. Read-only; the numbers
//! come from `keepsake_core::catalog::Report` (catalog queries only, no
//! media file is read).

use std::path::PathBuf;

use keepsake_core::catalog::{LibraryReport as CoreReport, ReportRow};
use keepsake_core::report;
use tauri::State;

use crate::api::{LibraryReport, ReportDrive, ReportGroup};
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

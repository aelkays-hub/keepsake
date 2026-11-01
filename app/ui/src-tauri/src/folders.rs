//! Folders added in place: photos and videos already on disk, catalogued
//! where they are (`keepsake_core::rebuild::add_folder`). Runs as the one
//! maintenance job, reports with `maint://progress` (kind `folder`), sends
//! `folder://saved` each time a batch of its items is in the library, and
//! finishes with `folder://done`. Items are saved as they are read, so a scan
//! that stops keeps them, and the next scan carries on from there.

use std::path::PathBuf;

use keepsake_core::catalog::{Catalog, VolumeId};
use keepsake_core::library::{Library, LibraryError, VolumeRecord};
use keepsake_core::rebuild::{
    add_folder, rescan_folder, RebuildOptions, RebuildProgress, RebuildReport, BATCH_FILES,
};
use keepsake_core::relpath::RelPath;
use tauri::{AppHandle, Emitter, State};

use crate::api::{FolderDone, FolderSaved};
use crate::convert;
use crate::maintenance::{finish, spawn, Reporter};
use crate::state::{err, AppState, CmdResult};

/// Add the folder at `path` to the library (or scan it again). Returns at
/// once; progress and the result come as events.
#[tauri::command(async)]
pub fn folder_add(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let folder = PathBuf::from(path.trim());
    if folder.as_os_str().is_empty() {
        return Err("choose a folder".into());
    }
    if !folder.is_dir() {
        return Err(format!("{} is not a folder", path.trim()));
    }
    let shown = convert::path_text(&folder);
    scan(app, &state, shown, false, move |cat, lib, opts| {
        add_folder(cat, lib, &folder, opts)
    })
}

/// Scan again one folder of a volume already in the library (`path` on the
/// volume, e.g. `2025/May`), with what is below it; `None` for the whole
/// volume. The rest of the volume is left as it is. Like [`folder_add`],
/// returns at once and reports with events.
#[tauri::command(async)]
pub fn folder_rescan(
    app: AppHandle,
    state: State<'_, AppState>,
    volume: String,
    path: Option<String>,
) -> CmdResult<()> {
    let open = state.open()?;
    let id: VolumeId = convert::id(&volume, "volume")?;
    let rel = match path.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
        Some(p) => Some(RelPath::parse(p).map_err(err)?),
        None => None,
    };
    let rec = open.lib.volume(id).map_err(err)?;
    let shown = match (&rec.last_mount, &rel) {
        (Some(m), Some(r)) => convert::path_text(&r.to_fs_path(m)),
        (Some(m), None) => convert::path_text(m),
        (None, _) => rec.label.clone(),
    };
    scan(app, &state, shown, true, move |cat, lib, opts| {
        rescan_folder(cat, lib, id, rel.as_ref(), opts)
    })
}

type ScanResult = Result<(VolumeRecord, RebuildReport), LibraryError>;

/// Run a folder scan as the maintenance job: progress as `maint://progress`
/// (kind `folder`), `folder://saved` per batch, `folder://done` at the end.
/// `shown` names the folder in the events; `again` marks a scan of a folder
/// already in the library.
fn scan(
    app: AppHandle,
    state: &AppState,
    shown: String,
    again: bool,
    run: impl FnOnce(&mut Catalog, &Library, &mut RebuildOptions<'_>) -> ScanResult + Send + 'static,
) -> CmdResult<()> {
    let open = state.open()?;
    spawn(state, "add-folder", move |cancel| {
        let mut rep = Reporter::new(&app, "folder");
        let mut done = FolderDone {
            folder: shown.clone(),
            volume: None,
            label: None,
            items: 0,
            files: 0,
            with_sidecars: 0,
            other_files: 0,
            problems: Vec::new(),
            cancelled: false,
            error: None,
            scanned_again: again,
        };
        let result = (|| -> Result<(), String> {
            let mut cat = open.writer()?;
            let prober = keepsake_core::probe::Prober::new(
                &keepsake_core::probe::ToolPaths::discover(),
                Default::default(),
            )
            .ok();
            let mut progress = |p: &RebuildProgress<'_>| {
                match p {
                    RebuildProgress::Listed { bytes, .. } => rep.p.bytes_total = *bytes,
                    RebuildProgress::File { path, bytes } => {
                        rep.p.bytes_done += *bytes;
                        rep.p.files_done += 1;
                        rep.p.current = Some((*path).to_owned());
                    }
                    RebuildProgress::Phase(name) => rep.p.current = Some((*name).to_owned()),
                    RebuildProgress::Saved { assets, files } => {
                        let _ = app.emit(
                            "folder://saved",
                            FolderSaved {
                                folder: shown.clone(),
                                items: *assets,
                                files: *files,
                            },
                        );
                    }
                }
                rep.send(matches!(p, RebuildProgress::Phase(_)));
            };
            let mut opts = RebuildOptions {
                prober: prober.as_ref(),
                // The thumbnail cache is never catalogued, even inside the folder.
                cache_dirs: vec![open.cache_root()],
                cancel: Some(&cancel),
                progress: Some(&mut progress),
                batch_files: Some(BATCH_FILES),
                only: None,
            };
            match run(&mut cat, &open.lib, &mut opts) {
                Ok((rec, r)) => {
                    done.volume = Some(rec.id.to_string());
                    done.label = Some(rec.label);
                    done.items = r.assets;
                    done.files = r.files_indexed;
                    done.with_sidecars = r.sidecars_used;
                    done.other_files = r.skipped.len() as u64;
                    done.problems = r.problems.into_iter().take(20).collect();
                    // Thumbnails for the new items, in the background.
                    keepsake_core::render::enqueue_backlog(&mut cat).map_err(err)?;
                    Ok(())
                }
                Err(LibraryError::Cancelled) => {
                    done.cancelled = true;
                    Ok(())
                }
                Err(LibraryError::Invalid(m)) => Err(m),
                Err(e) => Err(err(e)),
            }
        })();
        if let Err(e) = result {
            done.error = Some(e);
        }
        finish(&app);
        if done.volume.is_some() {
            if let Err(e) = crate::render::start_service(&app) {
                tracing::warn!("render service: {e}");
            }
        }
        let _ = app.emit("folder://done", done);
    })
}

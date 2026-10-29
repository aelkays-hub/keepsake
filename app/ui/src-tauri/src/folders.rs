//! Folders added in place: photos and videos already on disk, catalogued
//! where they are (`keepsake_core::rebuild::add_folder`). Runs as the one
//! maintenance job, reports with `maint://progress` (kind `folder`) and
//! finishes with `folder://done`.

use std::path::PathBuf;

use keepsake_core::library::LibraryError;
use keepsake_core::rebuild::{add_folder, RebuildOptions, RebuildProgress};
use tauri::{AppHandle, Emitter, State};

use crate::api::FolderDone;
use crate::convert;
use crate::maintenance::{finish, spawn, Reporter};
use crate::state::{err, AppState, CmdResult};

/// Add the folder at `path` to the library (or scan it again). Returns at
/// once; progress and the result come as events.
#[tauri::command(async)]
pub fn folder_add(app: AppHandle, state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let open = state.open()?;
    let folder = PathBuf::from(path.trim());
    if folder.as_os_str().is_empty() {
        return Err("choose a folder".into());
    }
    if !folder.is_dir() {
        return Err(format!("{} is not a folder", path.trim()));
    }
    spawn(&state, "add-folder", move |cancel| {
        let mut rep = Reporter::new(&app, "folder");
        let mut done = FolderDone {
            folder: convert::path_text(&folder),
            volume: None,
            label: None,
            items: 0,
            files: 0,
            with_sidecars: 0,
            other_files: 0,
            problems: Vec::new(),
            cancelled: false,
            error: None,
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
                }
                rep.send(matches!(p, RebuildProgress::Phase(_)));
            };
            let mut opts = RebuildOptions {
                prober: prober.as_ref(),
                // The thumbnail cache is never catalogued, even inside the folder.
                cache_dirs: vec![open.cache_root()],
                cancel: Some(&cancel),
                progress: Some(&mut progress),
            };
            match add_folder(&mut cat, &open.lib, &folder, &mut opts) {
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

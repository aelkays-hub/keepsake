//! The import window: what is on the card, thumbnails of it, presets, a
//! pre-check, and the import itself on a background thread
//! (`import://progress`, then `import://done`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use keepsake_core::catalog::{ImportAssetState, ImportDiff, ImportSessionId, VolumeKind};
use keepsake_core::devices::VolumeIdentity;
use keepsake_core::ingest::fault::FaultPlan;
use keepsake_core::ingest::template::Template;
use keepsake_core::ingest::{self, IngestError, PlanOptions, Progress, RunContext, Selection};
use keepsake_core::probe::ExifTool;
use keepsake_core::render::{card, RenderTools};
use keepsake_core::{cards, presets};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::api::{
    CardItem, CardScan, DestinationSpace, ImportCheck, ImportProgress, ImportRequest, ImportResult,
    Preset,
};
use crate::convert;
use crate::state::{err, ui_test, AppState, CmdResult};

fn exiftool() -> Option<&'static ExifTool> {
    static ET: OnceLock<Option<ExifTool>> = OnceLock::new();
    ET.get_or_init(|| {
        ExifTool::start(&RenderTools::discover().exiftool, Duration::from_secs(30)).ok()
    })
    .as_ref()
}

fn mount_path(mount: &str) -> CmdResult<PathBuf> {
    std::fs::canonicalize(mount).map_err(|e| format!("{mount}: {e}"))
}

/// The card's identity (real cards), or a stand-in for folders in tests.
fn identity(mount: &Path) -> VolumeIdentity {
    keepsake_platform::volume_root(mount)
        .and_then(|r| keepsake_platform::identify_volume(&r))
        .unwrap_or_default()
}

fn local_day(ms: Option<i64>) -> Option<String> {
    let t = chrono::DateTime::from_timestamp_millis(ms?)?;
    Some(
        t.with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string(),
    )
}

#[tauri::command(async)]
pub fn import_scan(state: State<'_, AppState>, mount: String) -> CmdResult<CardScan> {
    let open = state.open()?;
    let m = mount_path(&mount)?;
    let id = identity(&m);
    let status = cards::identify(&open.lib, &m, &id).map_err(err)?;
    let card = status.as_ref().map(|s| &s.card);
    let items = {
        let cat = open.cat()?;
        ingest::scan_card(&cat, &open.lib, &m, card).map_err(err)?
    };
    Ok(CardScan {
        mount: convert::path_text(&m),
        label: card
            .map(|c| c.label.clone())
            .or(id.label.clone())
            .unwrap_or_else(|| {
                m.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "Card".into())
            }),
        card: card.map(|c| c.id.to_string()),
        known: status.as_ref().is_some_and(|s| s.known),
        preset: card.and_then(|c| c.preset.clone()),
        items: items
            .into_iter()
            .map(|a| CardItem {
                key: a.key,
                kind: convert::kind(
                    a.kind,
                    a.files
                        .iter()
                        .any(|f| f.to_ascii_lowercase().ends_with(".arw")),
                ),
                files: a.files,
                size: a.size,
                day: local_day(a.mtime_ms),
                new: a.diff != ImportDiff::AlreadyImported,
            })
            .collect(),
    })
}

/// A thumbnail of an item on the card (made on demand, never on the card).
#[tauri::command(async)]
pub fn card_thumb(
    state: State<'_, AppState>,
    mount: String,
    files: Vec<String>,
) -> CmdResult<Option<String>> {
    let open = state.open()?;
    let m = mount_path(&mount)?;
    match card::card_thumbnail(
        &open.cache,
        &RenderTools::discover(),
        exiftool(),
        &m,
        &files,
    ) {
        Ok(p) => Ok(Some(convert::path_text(&p))),
        Err(e) => {
            tracing::debug!("card thumbnail {files:?}: {e}");
            Ok(None)
        }
    }
}

#[tauri::command(async)]
pub fn presets_list(state: State<'_, AppState>) -> CmdResult<Vec<Preset>> {
    let open = state.open()?;
    Ok(presets::list(&open.lib)
        .map_err(err)?
        .into_iter()
        .map(|p| Preset {
            name: p.name,
            destinations: p
                .destinations
                .iter()
                .map(|d| convert::path_text(d))
                .collect(),
            template: p.template,
            event: p.event,
        })
        .collect())
}

#[tauri::command(async)]
pub fn preset_save(state: State<'_, AppState>, preset: Preset) -> CmdResult<()> {
    let open = state.open()?;
    presets::save(
        &open.lib,
        &presets::Preset {
            name: preset.name,
            destinations: preset.destinations.iter().map(PathBuf::from).collect(),
            template: preset.template,
            event: preset.event,
        },
    )
    .map_err(err)
}

fn problem(e: IngestError) -> String {
    match e {
        IngestError::Unsafe(m) => m,
        other => other.to_string(),
    }
}

/// Is this import possible, how much space does it need, and what will the
/// first file be called? Writes nothing.
#[tauri::command(async)]
pub fn import_check(
    state: State<'_, AppState>,
    request: ImportRequest,
    sizes: Vec<u64>,
    first: Option<String>,
    first_ms: Option<i64>,
) -> CmdResult<ImportCheck> {
    let open = state.open()?;
    let m = mount_path(&request.mount)?;
    let needed: u64 = sizes.iter().sum();
    let fail = |p: String| ImportCheck {
        ok: false,
        problem: Some(p),
        space: Vec::new(),
        example: None,
    };
    if request.destinations.is_empty() {
        return Ok(fail("choose a destination".into()));
    }
    let template = match Template::parse(&request.template) {
        Ok(t) => t,
        Err(e) => return Ok(fail(format!("template: {e}"))),
    };
    let dests: Vec<PathBuf> = request.destinations.iter().map(PathBuf::from).collect();
    let space = match ingest::check_destinations_for(&open.lib, &m, &dests, ui_test()) {
        Ok(s) => s,
        Err(e) => return Ok(fail(problem(e))),
    };
    let example = first.and_then(|f| {
        let t = first_ms
            .and_then(chrono::DateTime::from_timestamp_millis)
            .map(|d| d.with_timezone(&chrono::Local).naive_local());
        let name = Path::new(&f).file_name()?.to_string_lossy().into_owned();
        ingest::example_name(&template, &name, t, request.event.as_deref()).ok()
    });
    let short = space
        .iter()
        .find(|(_, free)| free.is_some_and(|f| f < needed));
    Ok(ImportCheck {
        ok: short.is_none() && needed > 0,
        problem: match short {
            Some((d, _)) => Some(format!("not enough space on {}", convert::path_text(d))),
            None if needed == 0 => Some("nothing selected".into()),
            None => None,
        },
        space: space
            .into_iter()
            .map(|(d, free)| DestinationSpace {
                destination: convert::path_text(&d),
                needed,
                free,
            })
            .collect(),
        example,
    })
}

#[tauri::command(async)]
pub fn import_cancel(state: State<'_, AppState>) {
    if let Ok(g) = state.import_cancel.lock() {
        if let Some(c) = g.as_ref() {
            c.store(true, Ordering::SeqCst);
        }
    }
}

/// Start the import; returns at once. Progress and the result come as
/// events.
#[tauri::command(async)]
pub fn import_start(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ImportRequest,
) -> CmdResult<()> {
    let open = state.open()?;
    let m = mount_path(&request.mount)?;
    if state.import_running.swap(true, Ordering::SeqCst) {
        return Err("an import is already running".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut g) = state.import_cancel.lock() {
        *g = Some(Arc::clone(&cancel));
    }
    let spawned = std::thread::Builder::new()
        .name("import".into())
        .spawn(move || {
            let result = run_import(&app, &open, &m, &request, &cancel);
            app.state::<AppState>()
                .import_running
                .store(false, Ordering::SeqCst);
            let _ = app.emit("import://done", result);
        });
    if let Err(e) = spawned {
        state.import_running.store(false, Ordering::SeqCst);
        return Err(err(e));
    }
    Ok(())
}

fn run_import(
    app: &AppHandle,
    open: &crate::state::Open,
    mount: &Path,
    req: &ImportRequest,
    cancel: &AtomicBool,
) -> ImportResult {
    let mut out = ImportResult {
        session: None,
        safe_to_format: false,
        not_safe: Vec::new(),
        catalogued: 0,
        failed: Vec::new(),
        cancelled: false,
        error: None,
    };
    let r = (|| -> Result<ImportSessionId, String> {
        let lib = &open.lib;
        let mut cat = lib.open_catalog().map_err(err)?;
        let id = identity(mount);
        let card = cards::identify(lib, mount, &id)
            .map_err(err)?
            .map(|s| s.card);
        let mut dests = Vec::new();
        for (i, d) in req.destinations.iter().enumerate() {
            let p = PathBuf::from(d);
            let label = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| format!("Destination {}", i + 1));
            dests.push(
                ingest::destination_volume(lib, &p, &label, VolumeKind::External).map_err(err)?,
            );
        }
        let opts = PlanOptions {
            template: Template::parse(&req.template).map_err(err)?,
            selection: Selection::Nothing,
            event: req.event.clone().filter(|e| !e.is_empty()),
            allow_same_filesystem: ui_test(),
            source_id: card.as_ref().map(|c| c.key.clone()),
        };
        let prober = keepsake_core::probe::Prober::new(
            &keepsake_core::probe::ToolPaths::discover(),
            Default::default(),
        )
        .ok();
        let session = ingest::plan(
            &mut cat,
            lib,
            prober.as_ref(),
            mount,
            &req.label,
            &dests,
            &opts,
        )
        .map_err(problem)?;
        out.session = Some(session.to_string());
        ingest::select(&mut cat, session, &req.keys, true).map_err(err)?;
        // Bytes to copy: each selected source file once.
        let keys: std::collections::HashSet<&String> = req.keys.iter().collect();
        let first = dests.first().map(|d| d.volume.id);
        let total: u64 = cat
            .read(|tx| tx.imports().items(session))
            .map_err(err)?
            .iter()
            .filter(|i| keys.contains(&i.source_key) && Some(i.dest_volume_id) == first)
            .map(|i| i.size_bytes)
            .sum();
        let (mut files_done, mut base, mut size, mut last) = (0u64, 0u64, 0u64, Instant::now());
        let mut current: Option<String> = None;
        let emit = |p: ImportProgress| {
            let _ = app.emit("import://progress", p);
        };
        let s = session.to_string();
        let mut progress = |p: &Progress| match p {
            Progress::FileStarted { file, bytes } => {
                current = Some(file.clone());
                size = *bytes;
            }
            Progress::FileBytes { done, .. } => {
                if last.elapsed() > Duration::from_millis(150) {
                    last = Instant::now();
                    emit(ImportProgress {
                        session: s.clone(),
                        files_done,
                        bytes_done: base + done,
                        bytes_total: total,
                        current: current.clone(),
                        phase: None,
                    });
                }
            }
            Progress::FileDone { .. } => {
                files_done += 1;
                base += size;
                size = 0;
            }
            Progress::Phase { name } => emit(ImportProgress {
                session: s.clone(),
                files_done,
                bytes_done: base,
                bytes_total: total,
                current: None,
                phase: Some(name.clone()),
            }),
            Progress::AssetState { .. } => {}
        };
        let fault = FaultPlan::none();
        let mut ctx = RunContext {
            prober: prober.as_ref(),
            fault: &fault,
            cancel,
            progress: &mut progress,
            retry_failed: true,
        };
        let res = ingest::run(&mut cat, lib, session, &mut ctx);
        let _ = cards::record_session(lib, &cat, session);
        match res {
            Ok(_) => {}
            Err(IngestError::Cancelled) => out.cancelled = true,
            Err(e) => return Err(problem(e)),
        }
        let rep = ingest::report(&cat, session).map_err(err)?;
        out.safe_to_format = rep.safe_to_format;
        out.not_safe = rep.not_safe.clone();
        out.failed = rep
            .failed
            .iter()
            .map(|(f, e)| format!("{f}: {e}"))
            .collect();
        out.catalogued = cat
            .read(|tx| tx.imports().assets(session))
            .map_err(err)?
            .iter()
            .filter(|a| a.state == ImportAssetState::Catalogued)
            .count() as u64;
        Ok(session)
    })();
    if let Err(e) = r {
        out.error = Some(e);
    }
    out
}

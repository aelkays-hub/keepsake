//! The Backup screen: incremental verified backups, verification of a
//! backup (or any volume), and their history (`<library>/history.json`).
//!
//! One backup or verify runs at a time, on its own thread. It reports with
//! `maint://progress` and finishes with `maint://done`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use keepsake_core::backup::{self, BackupProgress};
use keepsake_core::catalog::{VolumeId, VolumeKind};
use keepsake_core::history::{self, Run, RunKind};
use keepsake_core::ingest::{self, fault::FaultPlan};
use keepsake_core::verify::{self, Problem, VerifyProgress};
use tauri::{AppHandle, Emitter, State};

use crate::api::{BackupDestination, BackupOverview, HistoryRun, MaintDone, MaintProgress};
use crate::convert;
use crate::state::{err, AppState, CmdResult, Open};

#[derive(Default)]
pub struct MaintState {
    running: AtomicBool,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

pub fn run_dto(r: &Run) -> HistoryRun {
    HistoryRun {
        kind: match r.kind {
            RunKind::Backup => "backup",
            RunKind::Verify => "verify",
        }
        .to_owned(),
        started_ms: r.started_ms,
        finished_ms: r.finished_ms,
        destination: r.destination.as_deref().map(convert::path_text),
        volume: r.volume.map(|v| v.to_string()),
        label: r.label.clone(),
        files: r.files,
        bytes: r.bytes,
        problems: r.problems,
        cancelled: r.cancelled,
        ok: r.ok(),
    }
}

/// Destinations backed up to, and every run, newest first.
#[tauri::command(async)]
pub fn backup_overview(state: State<'_, AppState>) -> CmdResult<BackupOverview> {
    let open = state.open()?;
    let runs = history::list(&open.lib).map_err(err)?;
    let cat = open.cat();
    let mut destinations: Vec<BackupDestination> = Vec::new();
    for r in runs.iter().filter(|r| r.kind == RunKind::Backup) {
        let Some(v) = r.volume else { continue };
        let id = v.to_string();
        if destinations.iter().any(|d| d.volume == id) {
            continue;
        }
        let mount = verify::mount_of(&cat, &open.lib, v);
        destinations.push(BackupDestination {
            volume: id,
            label: r.label.clone(),
            path: mount
                .as_deref()
                .or(r.destination.as_deref())
                .map(convert::path_text),
            online: mount.is_some(),
            last_backup_ms: Some(r.finished_ms),
            last_verified_ms: history::last_verified(&open.lib, v).map_err(err)?,
        });
    }
    Ok(BackupOverview {
        destinations,
        runs: runs.iter().map(run_dto).collect(),
        busy: state.maint.running.load(Ordering::SeqCst),
    })
}

/// Throttled progress events.
struct Reporter {
    app: AppHandle,
    kind: &'static str,
    last: Instant,
    p: MaintProgress,
}

impl Reporter {
    fn new(app: &AppHandle, kind: &'static str) -> Self {
        Self {
            app: app.clone(),
            kind,
            last: Instant::now() - Duration::from_secs(1),
            p: MaintProgress {
                kind: kind.to_owned(),
                bytes_done: 0,
                bytes_total: 0,
                files_done: 0,
                current: None,
            },
        }
    }

    fn send(&mut self, force: bool) {
        if force || self.last.elapsed() >= Duration::from_millis(150) {
            self.last = Instant::now();
            let _ = self.app.emit("maint://progress", self.p.clone());
        }
    }

    fn done(&self, run: Option<HistoryRun>, problems: Vec<String>, error: Option<String>) {
        let _ = self.app.emit(
            "maint://done",
            MaintDone {
                kind: self.kind.to_owned(),
                run,
                problems,
                error,
            },
        );
    }
}

/// Run `work` on its own thread, as the one maintenance job.
fn spawn(
    state: &AppState,
    name: &str,
    work: impl FnOnce(Arc<AtomicBool>) + Send + 'static,
) -> CmdResult<()> {
    if state.maint.running.swap(true, Ordering::SeqCst) {
        return Err("a backup or verification is already running".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut slot) = state.maint.cancel.lock() {
        *slot = Some(Arc::clone(&cancel));
    }
    let spawned = std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || work(cancel));
    if let Err(e) = spawned {
        state.maint.running.store(false, Ordering::SeqCst);
        return Err(err(e));
    }
    Ok(())
}

fn finish(app: &AppHandle) {
    use tauri::Manager;
    app.state::<AppState>()
        .maint
        .running
        .store(false, Ordering::SeqCst);
}

fn latest(open: &Open, kind: RunKind, volume: Option<VolumeId>) -> Option<HistoryRun> {
    history::list(&open.lib)
        .ok()?
        .iter()
        .find(|r| r.kind == kind && (volume.is_none() || r.volume == volume))
        .map(run_dto)
}

fn backup_job(
    open: &Open,
    dest: &Path,
    label: &str,
    cancel: &AtomicBool,
    rep: &mut Reporter,
) -> Result<(Option<HistoryRun>, Vec<String>), String> {
    let mut cat = open.writer()?;
    let d =
        ingest::destination_volume(&open.lib, dest, label, VolumeKind::External).map_err(err)?;
    let started = keepsake_core::jobs::now_ms();
    let mut current = 0u64;
    let mut progress = |p: &BackupProgress<'_>| {
        match p {
            BackupProgress::Planned { bytes, .. } => rep.p.bytes_total = *bytes,
            BackupProgress::FileStarted { path, bytes } => {
                rep.p.current = Some((*path).to_owned());
                current = *bytes;
            }
            BackupProgress::FileDone { .. } => {
                rep.p.bytes_done += current;
                rep.p.files_done += 1;
            }
        }
        rep.send(false);
    };
    let r = backup::run(
        &mut cat,
        &open.lib,
        &d,
        &FaultPlan::none(),
        cancel,
        &mut progress,
    )
    .map_err(err)?;
    rep.send(true);
    history::record_backup(&open.lib, started, &d, &r).map_err(err)?;
    let mut problems: Vec<String> = r.failed.iter().map(|(f, e)| format!("{f}: {e}")).collect();
    problems.extend(
        r.unavailable
            .iter()
            .map(|u| format!("{u}: no connected copy to back up from")),
    );
    problems.truncate(20);
    Ok((latest(open, RunKind::Backup, Some(d.volume.id)), problems))
}

/// Back up the library to `dest` (a folder or drive; created if missing).
/// Incremental: only what is not there yet is copied, and every copy is
/// verified against the catalog's hash.
#[tauri::command(async)]
pub fn backup_run(
    app: AppHandle,
    state: State<'_, AppState>,
    dest: String,
    label: Option<String>,
) -> CmdResult<()> {
    let open = state.open()?;
    let dest = PathBuf::from(dest.trim());
    if dest.as_os_str().is_empty() {
        return Err("choose a backup folder".into());
    }
    let label = label.filter(|l| !l.trim().is_empty()).unwrap_or_else(|| {
        dest.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Backup".into())
    });
    spawn(&state, "backup", move |cancel| {
        let mut rep = Reporter::new(&app, "backup");
        match backup_job(&open, &dest, &label, &cancel, &mut rep) {
            Ok((run, problems)) => rep.done(run, problems, None),
            Err(e) => rep.done(None, Vec::new(), Some(e)),
        }
        finish(&app);
    })
}

/// Check every file on `volume` against the catalog's hashes.
#[tauri::command(async)]
pub fn verify_run(app: AppHandle, state: State<'_, AppState>, volume: String) -> CmdResult<()> {
    let open = state.open()?;
    let v: VolumeId = convert::id(&volume, "volume")?;
    if verify::mount_of(&open.cat(), &open.lib, v).is_none() {
        return Err("that drive is not connected".into());
    }
    spawn(&state, "verify", move |cancel| {
        let mut rep = Reporter::new(&app, "verify");
        let started = keepsake_core::jobs::now_ms();
        let result = (|| -> Result<(Option<HistoryRun>, Vec<String>), String> {
            let cat = open.writer()?;
            let mut progress = |p: &VerifyProgress<'_>| {
                match p {
                    VerifyProgress::Volume { bytes, .. } => rep.p.bytes_total = *bytes,
                    VerifyProgress::File { path, bytes } => {
                        rep.p.current = Some((*path).to_owned());
                        rep.p.bytes_done += *bytes;
                        rep.p.files_done += 1;
                    }
                }
                rep.send(false);
            };
            let r =
                verify::verify(&cat, &open.lib, Some(v), &mut progress, &cancel).map_err(err)?;
            history::record_verify(&open.lib, started, &r).map_err(err)?;
            let problems = r
                .issues
                .iter()
                .take(20)
                .map(|i| {
                    let what = match &i.problem {
                        Problem::Missing => "missing",
                        Problem::Changed { .. } => "changed",
                        _ => "unreadable",
                    };
                    format!("{}: {what}", i.path)
                })
                .collect();
            Ok((latest(&open, RunKind::Verify, Some(v)), problems))
        })();
        rep.send(true);
        match result {
            Ok((run, problems)) => rep.done(run, problems, None),
            Err(e) => rep.done(None, Vec::new(), Some(e)),
        }
        finish(&app);
    })
}

/// Stop the running backup or verify after the current file.
#[tauri::command(async)]
pub fn maint_cancel(state: State<'_, AppState>) {
    if let Ok(slot) = state.maint.cancel.lock() {
        if let Some(c) = slot.as_ref() {
            c.store(true, Ordering::SeqCst);
        }
    }
}

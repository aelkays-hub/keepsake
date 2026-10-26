//! Local logs, crash reports and the diagnostic report. No telemetry:
//! everything stays in the log folder until the user copies a report.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use keepsake_core::logs;
use keepsake_core::render::RenderTools;
use tauri::State;
use tracing_subscriber::prelude::*;

use crate::api::CrashNotice;
use crate::state::{err, AppState, CmdResult};

static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn log_dir() -> Option<&'static PathBuf> {
    LOG_DIR.get()
}

/// Logs to stderr and this run's log file, and writes crash reports.
/// `RUST_LOG` sets the level (default `info`).
pub fn init() {
    let filter = || {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
    };
    let stderr = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(filter());
    let dir = logs::log_dir();
    let file = dir
        .as_ref()
        .and_then(|d| logs::LogFile::create(d).ok())
        .map(|f| {
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(Mutex::new(f))
                .with_filter(filter())
        });
    tracing_subscriber::registry()
        .with(stderr)
        .with(file)
        .try_init()
        .ok();
    if let Some(d) = dir {
        logs::install_crash_hook(d.clone(), "keepsake-app");
        let _ = LOG_DIR.set(d);
    }
    tracing::info!(
        version = keepsake_core::version(),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        "Keepsake started"
    );
}

/// The report the user can paste into a bug report.
#[tauri::command]
pub async fn diagnostic_report(state: State<'_, AppState>) -> CmdResult<String> {
    let open = state.try_open();
    tauri::async_runtime::spawn_blocking(move || {
        let tools = RenderTools::discover();
        let cat = match &open {
            Some(o) => Some(o.writer()?),
            None => None,
        };
        let library = match (&cat, &open) {
            (Some(c), Some(o)) => Some((c, &o.lib)),
            _ => None,
        };
        Ok(keepsake_core::diag::report(
            &keepsake_core::diag::ReportInput {
                app: "keepsake-app",
                library,
                cache: open.as_ref().map(|o| &o.cache),
                tools: &tools,
                log_dir: log_dir().map(PathBuf::as_path),
            },
        ))
    })
    .await
    .map_err(err)?
}

#[tauri::command(async)]
pub fn open_log_folder() -> CmdResult<String> {
    let d = log_dir().ok_or("no log folder on this system")?;
    keepsake_core::external::open_folder(d).map_err(err)?;
    Ok(crate::convert::path_text(d))
}

/// An error in the UI (an exception, or a failed promise nobody handled).
#[tauri::command(async)]
pub fn log_ui_error(message: String, detail: Option<String>) {
    let cut = |s: &str| s.chars().take(4000).collect::<String>();
    tracing::error!(
        target: "ui",
        detail = detail.as_deref().map(cut).unwrap_or_default(),
        "{}",
        cut(&message)
    );
}

/// The last crash, if the user has not seen it yet.
#[tauri::command(async)]
pub fn crash_notice() -> Option<CrashNotice> {
    let c = logs::latest_crash(log_dir()?)?;
    if c.seen {
        return None;
    }
    let summary = c
        .text
        .lines()
        .skip_while(|l| !l.starts_with("panicked") && !l.contains("panicked at"))
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    Some(CrashNotice {
        file: c
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        summary: if summary.is_empty() {
            c.text.lines().next().unwrap_or_default().to_owned()
        } else {
            summary
        },
    })
}

#[tauri::command(async)]
pub fn crash_dismiss() -> CmdResult<()> {
    if let Some(d) = log_dir() {
        logs::mark_crashes_seen(d).map_err(err)?;
    }
    Ok(())
}

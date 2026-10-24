//! `keepsake watch [--auto-ingest]`

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Args;

use keepsake_core::ingest::IngestReport;
use keepsake_core::watch::{WatchEvent, WatchOptions, Watcher};
use keepsake_platform::{DeviceEvents, FakeDevices};

use super::common::{
    cancel_flag, human_bytes, maybe_prober, path_str, Ctx, EXIT_CANCELLED, EXIT_OK,
};

#[derive(Args)]
pub struct WatchArgs {
    /// Import trusted cards automatically, with their preset.
    #[arg(long)]
    pub auto_ingest: bool,
    /// Don't read capture dates with ExifTool/ffprobe during automatic imports.
    #[arg(long)]
    pub no_probe: bool,
    /// Testing only: read device events from a JSON Lines file instead of
    /// the OS, and exit at its end.
    #[arg(long, hide = true)]
    pub fake_events: Option<PathBuf>,
    #[arg(long, hide = true)]
    pub unsafe_allow_same_disk: bool,
}

fn print_human(ev: &WatchEvent) {
    match ev {
        WatchEvent::CardInserted {
            label,
            mount,
            known,
            layouts,
            items,
            new_items,
            last_import_ms,
            trusted,
            preset,
            ..
        } => {
            let kinds: Vec<&str> = layouts.iter().map(|l| l.as_str()).collect();
            let what = match (known, last_import_ms) {
                (true, Some(_)) => {
                    format!("{new_items} new item(s) since last import ({items} on the card)")
                }
                (true, None) => format!("known card, never imported: {items} item(s)"),
                (false, _) => format!("new card: {items} item(s)"),
            };
            println!(
                "Card inserted: {label} ({}) [{}] — {what}",
                path_str(mount),
                kinds.join(", ")
            );
            if *trusted {
                println!(
                    "  trusted, preset {}",
                    preset.as_deref().unwrap_or("(none)")
                );
            }
        }
        WatchEvent::CardRemoved { label, mount, .. } => {
            println!("Card removed: {label} ({})", path_str(mount));
        }
        // Every other disk shows up at start; not worth a line.
        WatchEvent::VolumeIgnored { .. } | WatchEvent::AutoIngestProgress { .. } => {}
        WatchEvent::AutoIngestSkipped { reason, .. } => {
            println!("  Not importing automatically: {reason}");
        }
        WatchEvent::AutoIngestStarted {
            preset,
            destinations,
            ..
        } => {
            let to: Vec<String> = destinations.iter().map(|d| path_str(d)).collect();
            println!("  Importing with preset {preset} to {}…", to.join(" and "));
        }
        WatchEvent::AutoIngestFinished {
            session,
            report,
            new_items_left,
            ..
        } => print_finished(*session, report, *new_items_left),
        WatchEvent::AutoIngestCancelled { session, .. } => {
            println!("  Stopped after the current file. Continue with: keepsake ingest --resume --session {session}");
        }
        WatchEvent::AutoIngestFailed {
            session,
            error,
            resumable,
            ..
        } => {
            println!("  Import failed: {error}");
            if let (true, Some(s)) = (resumable, session) {
                println!("  Continue with: keepsake ingest --resume --session {s}");
            }
        }
        WatchEvent::Error { mount, error } => {
            println!("Problem with {}: {error}", path_str(mount));
        }
    }
}

fn print_finished(session: keepsake_core::catalog::ImportSessionId, r: &IngestReport, left: usize) {
    let catalogued = r.by_state.get("catalogued").copied().unwrap_or(0);
    println!(
        "  Imported: {catalogued} asset(s) catalogued, {} copied (session {session})",
        human_bytes(r.bytes_copied)
    );
    if r.safe_to_format {
        println!("  SAFE TO FORMAT: every selected asset is verified on two destinations.");
    } else {
        println!("  NOT safe to format:");
        for why in &r.not_safe {
            println!("    - {why}");
        }
    }
    if left > 0 {
        println!("  {left} item(s) on the card are still not imported.");
    }
}

pub fn run(ctx: &Ctx, a: WatchArgs) -> Result<u8> {
    let lib = ctx.library()?;
    let prober = if a.auto_ingest {
        maybe_prober(a.no_probe, ctx.json)
    } else {
        None
    };
    let opts = WatchOptions {
        auto_ingest: a.auto_ingest,
        allow_same_filesystem: a.unsafe_allow_same_disk,
    };
    let mut watcher = Watcher::new(&lib, prober.as_ref(), opts);
    let mut backend: Box<dyn DeviceEvents> = match &a.fake_events {
        Some(p) => Box::new(FakeDevices::from_jsonl(p)?.closing()),
        None => Box::new(keepsake_platform::system_devices()?),
    };
    let (tx, rx) = channel();
    backend.start(tx).context("starting device notifications")?;
    let cancel = cancel_flag();
    if ctx.json {
        println!(
            "{}",
            serde_json::json!({ "event": "watching", "auto_ingest": a.auto_ingest })
        );
    } else {
        println!(
            "Watching for cards (automatic import {}). Press Ctrl-C to stop.",
            if a.auto_ingest { "on" } else { "off" }
        );
    }
    let mut interrupted = false;
    let mut out = std::io::stdout();
    let mut emit = |ev: &WatchEvent| {
        if matches!(ev, WatchEvent::AutoIngestCancelled { .. }) {
            interrupted = true;
        }
        if ctx.json {
            // One JSON object per line (JSON Lines), flushed at once so the
            // app sees events as they happen.
            if let Ok(line) = serde_json::to_string(ev) {
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
            }
        } else {
            print_human(ev);
            let _ = out.flush();
        }
    };
    while !cancel.load(Ordering::SeqCst) {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(ev) => watcher.handle(&ev, cancel, &mut emit),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    backend.stop();
    if ctx.json {
        println!("{}", serde_json::json!({ "event": "stopped" }));
    } else {
        println!("Stopped watching.");
    }
    Ok(if interrupted { EXIT_CANCELLED } else { EXIT_OK })
}

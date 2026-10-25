//! `keepsake backup <dest>`

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use keepsake_core::backup::{self, BackupProgress};
use keepsake_core::catalog::VolumeKind;
use keepsake_core::ingest::{self, fault::FaultPlan};

use super::common::{
    bytes_bar, cancel_flag, human_bytes, Ctx, EXIT_CANCELLED, EXIT_OK, EXIT_PROBLEMS,
};

#[derive(Args)]
pub struct BackupArgs {
    /// Backup folder or drive. Created if missing.
    pub dest: PathBuf,
    /// Name for the backup volume (default: the folder name).
    #[arg(long)]
    pub label: Option<String>,
}

pub fn run(ctx: &Ctx, a: BackupArgs) -> Result<u8> {
    let lib = ctx.library()?;
    let mut cat = lib.open_catalog()?;
    let label = a.label.clone().unwrap_or_else(|| {
        a.dest
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Backup".into())
    });
    let dest = ingest::destination_volume(&lib, &a.dest, &label, VolumeKind::External)?;
    let cancel = cancel_flag();
    let fault = FaultPlan::none();
    let mut bar = bytes_bar(ctx.json, 0, "backup");
    let mut current = 0u64;
    let mut progress = |p: &BackupProgress<'_>| match p {
        BackupProgress::Planned { bytes, .. } => {
            bar = bytes_bar(ctx.json, *bytes, "backup");
        }
        BackupProgress::FileStarted { path, bytes } => {
            bar.set_message(path.to_string());
            current = *bytes;
        }
        BackupProgress::FileDone { .. } => bar.inc(current),
    };
    let started = keepsake_core::jobs::now_ms();
    let r = backup::run(&mut cat, &lib, &dest, &fault, cancel, &mut progress);
    bar.finish_and_clear();
    let r = r?;
    keepsake_core::history::record_backup(&lib, started, &dest, &r)?;
    ctx.emit(&serde_json::to_value(&r)?, || {
        println!(
            "Backup to {}: {} file(s) copied ({}), {} already there",
            r.destination,
            r.files_copied,
            human_bytes(r.bytes_copied),
            r.files_already_there
        );
        for (f, e) in &r.failed {
            println!("  failed: {f}: {e}");
        }
        if !r.unavailable.is_empty() {
            println!(
                "  {} asset(s) have no mounted copy to back up from:",
                r.unavailable.len()
            );
            for u in r.unavailable.iter().take(10) {
                println!("    {u}");
            }
        }
        if r.stale_temporaries_removed > 0 {
            println!(
                "  removed {} unfinished file(s) from an earlier backup",
                r.stale_temporaries_removed
            );
        }
        match &r.library_snapshot {
            Some(p) => println!("  library copy: {}", super::common::path_str(p)),
            None if r.cancelled => println!(
                "Stopped after the current file (Ctrl-C). Run the same command again to continue."
            ),
            None => {}
        }
    })?;
    Ok(if r.cancelled {
        EXIT_CANCELLED
    } else if r.failed.is_empty() && r.unavailable.is_empty() {
        EXIT_OK
    } else {
        EXIT_PROBLEMS
    })
}

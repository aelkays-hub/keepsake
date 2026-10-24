//! `keepsake ingest <source> --dest A [--dest B] …`

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;
use serde_json::json;

use keepsake_core::catalog::{
    Catalog, ImportAssetState, ImportDiff, ImportSessionId, ImportStatus, VolumeKind,
};
use keepsake_core::ingest::fault::FaultPlan;
use keepsake_core::ingest::template::{Template, DEFAULT_TEMPLATE};
use keepsake_core::ingest::{
    self, IngestError, IngestReport, PlanOptions, Progress, RunContext, Selection,
};
use keepsake_core::library::Library;

use super::common::{
    bytes_bar, cancel_flag, human_bytes, maybe_prober, Ctx, EXIT_CANCELLED, EXIT_OK, EXIT_PROBLEMS,
};

#[derive(Args)]
pub struct IngestArgs {
    /// Root of the card (or a copy of one). Never written to.
    pub source: Option<PathBuf>,
    /// Destination folder or drive. Give it twice for two copies (needed for "safe to format").
    #[arg(long = "dest")]
    pub dest: Vec<PathBuf>,
    /// File-naming template. Tokens: {yyyy} {mm} {dd} {hh} {event} {camera} {lens} {seq} {orig}.
    #[arg(long, default_value = DEFAULT_TEMPLATE)]
    pub template: String,
    /// Value for {event}.
    #[arg(long)]
    pub event: Option<String>,
    /// Name for the card volume.
    #[arg(long, default_value = "Card")]
    pub label: String,
    /// Also copy assets that are already in the library.
    #[arg(long)]
    pub all: bool,
    /// Show what would be copied and where; write nothing.
    #[arg(long)]
    pub dry_run: bool,
    /// Do not read capture dates (files go to 0000-undated).
    #[arg(long)]
    pub no_probe: bool,
    /// Resume an interrupted import (the latest unfinished one, or --session).
    #[arg(long, conflicts_with_all = ["status", "list", "dry_run"])]
    pub resume: bool,
    /// Show an import's state (the latest, or --session).
    #[arg(long, conflicts_with_all = ["resume", "list", "dry_run"])]
    pub status: bool,
    /// List import sessions.
    #[arg(long, conflicts_with_all = ["resume", "status", "dry_run"])]
    pub list: bool,
    /// Session id for --resume / --status.
    #[arg(long)]
    pub session: Option<String>,
    /// TESTING ONLY: allow destinations on the same disk as the card or each
    /// other. "Safe to format" then means two folders, not two disks.
    #[arg(long, hide = true)]
    pub unsafe_allow_same_disk: bool,
}

fn pick_session(cat: &Catalog, s: Option<&str>, unfinished: bool) -> Result<ImportSessionId> {
    if let Some(s) = s {
        return s.parse().context("not a session id");
    }
    cat.read(|tx| tx.imports().list())?
        .into_iter()
        .find(|x| !unfinished || x.status != ImportStatus::Completed)
        .map(|x| x.id)
        .context(if unfinished {
            "no unfinished import to resume"
        } else {
            "no imports yet"
        })
}

fn print_report(r: &IngestReport) {
    if r.safe_to_format {
        println!("SAFE TO FORMAT: every selected asset is verified on two destinations.");
    } else {
        println!("NOT safe to format.");
    }
    let states: Vec<String> = r.by_state.iter().map(|(k, v)| format!("{v} {k}")).collect();
    println!("  {} assets: {}", r.assets, states.join(", "));
    if r.bytes_copied > 0 {
        println!("  {} copied", human_bytes(r.bytes_copied));
    }
    if r.unselected > 0 {
        println!(
            "  WARNING: {} asset(s) were not selected and are NOT copied; formatting the card loses them",
            r.unselected
        );
    }
    for (f, e) in &r.failed {
        println!("  failed: {f}: {e}");
    }
    if !r.safe_to_format {
        for n in r.not_safe.iter().take(20) {
            println!("  - {n}");
        }
        if r.not_safe.len() > 20 {
            println!("  … and {} more", r.not_safe.len() - 20);
        }
    }
}

fn run_session(ctx: &Ctx, lib: &Library, session: ImportSessionId, no_probe: bool) -> Result<u8> {
    let mut cat = lib.open_catalog()?;
    // Bytes still to copy (each source file is read once for both drives).
    let items = cat.read(|tx| tx.imports().items(session))?;
    let active: std::collections::HashSet<String> = cat
        .read(|tx| tx.imports().assets(session))?
        .into_iter()
        .filter(|a| {
            !matches!(
                a.state,
                ImportAssetState::Skipped
                    | ImportAssetState::Discovered
                    | ImportAssetState::Catalogued
            )
        })
        .map(|a| a.source_key)
        .collect();
    let mut seen = std::collections::HashSet::new();
    let total: u64 = items
        .iter()
        .filter(|i| active.contains(&i.source_key))
        .filter(|i| seen.insert(i.source_rel_path.clone()))
        .map(|i| i.size_bytes)
        .sum();
    let bar = bytes_bar(ctx.json, total, "ingest");
    let prober = maybe_prober(no_probe, ctx.json);
    let cancel = cancel_flag();
    let fault = FaultPlan::none();
    let mut base = 0u64;
    let mut progress = |p: &Progress| match p {
        Progress::FileStarted { file, .. } => bar.set_message(file.clone()),
        Progress::FileBytes { done, .. } => bar.set_position(base + done),
        Progress::FileDone { .. } => base = bar.position(),
        Progress::Phase { name } => bar.set_message(name.clone()),
        Progress::AssetState { .. } => {}
    };
    let mut rc = RunContext {
        prober: prober.as_ref(),
        fault: &fault,
        cancel,
        progress: &mut progress,
        retry_failed: true,
    };
    let result = ingest::run(&mut cat, lib, session, &mut rc);
    bar.finish_and_clear();
    match result {
        Ok(r) => {
            ctx.emit(&json!({ "session": session, "report": r }), || {
                println!("Session {session}");
                print_report(&r);
            })?;
            Ok(if r.safe_to_format {
                EXIT_OK
            } else {
                EXIT_PROBLEMS
            })
        }
        Err(IngestError::Cancelled) => {
            let r = ingest::report(&cat, session)?;
            ctx.emit(&json!({ "session": session, "cancelled": true, "report": r }), || {
                println!("Stopped after the current file. Nothing partial was left under a final name.");
                println!("Continue with: keepsake ingest --resume --session {session}");
                print_report(&r);
            })?;
            Ok(EXIT_CANCELLED)
        }
        Err(e) => Err(e.into()),
    }
}

pub fn run(ctx: &Ctx, a: IngestArgs) -> Result<u8> {
    let lib = ctx.library()?;
    if a.list {
        let cat = lib.open_catalog()?;
        let list = cat.read(|tx| tx.imports().list())?;
        ctx.emit(&serde_json::to_value(&list)?, || {
            if list.is_empty() {
                println!("No imports yet.");
            }
            for s in &list {
                println!(
                    "{}  {}  {:<9}  safe_to_format={}",
                    s.id,
                    chrono::DateTime::from_timestamp_millis(s.started_ms)
                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default(),
                    s.status.as_str(),
                    s.safe_to_format
                );
            }
        })?;
        return Ok(EXIT_OK);
    }
    if a.status {
        let cat = lib.open_catalog()?;
        let s = pick_session(&cat, a.session.as_deref(), false)?;
        let r = ingest::report(&cat, s)?;
        let left: Vec<PathBuf> = cat
            .read(|tx| tx.imports().destinations(s))?
            .iter()
            .filter_map(|d| lib.volume(d.volume_id).ok()?.last_mount)
            .flat_map(|m| ingest::leftovers(&m))
            .collect();
        ctx.emit(
            &json!({ "session": s, "report": r, "unfinished_temporaries": left }),
            || {
                println!("Session {s}");
                print_report(&r);
                if !left.is_empty() {
                    println!(
                        "  {} unfinished temporary file(s) remain; run `keepsake ingest --resume`",
                        left.len()
                    );
                }
            },
        )?;
        return Ok(if r.safe_to_format {
            EXIT_OK
        } else {
            EXIT_PROBLEMS
        });
    }
    if a.resume {
        let s = {
            let cat = lib.open_catalog()?;
            pick_session(&cat, a.session.as_deref(), true)?
        };
        if !ctx.json {
            println!("Resuming session {s}…");
        }
        return run_session(ctx, &lib, s, a.no_probe);
    }

    let source = a
        .source
        .clone()
        .context("give the card folder: keepsake ingest <source> --dest <folder>")?;
    if a.dest.is_empty() || a.dest.len() > 2 {
        bail!("give one or two --dest folders (two are needed for \"safe to format\")");
    }
    let opts = PlanOptions {
        template: Template::parse(&a.template)?,
        selection: if a.all {
            Selection::All
        } else {
            Selection::NewOnly
        },
        event: a.event.clone(),
        allow_same_filesystem: a.unsafe_allow_same_disk,
    };
    if a.unsafe_allow_same_disk && !ctx.json {
        eprintln!(
            "WARNING: --unsafe-allow-same-disk is for testing; do not format a card on this basis."
        );
    }
    let name_of = |d: &PathBuf, i: usize| {
        d.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("Destination {}", i + 1))
    };

    if a.dry_run {
        let cat = lib.open_catalog()?;
        let dests = a
            .dest
            .iter()
            .enumerate()
            .map(|(i, d)| ingest::preview_destination(&lib, d, &name_of(d, i)))
            .collect::<Result<Vec<_>, _>>()?;
        let prober = maybe_prober(a.no_probe, ctx.json);
        let p = ingest::preview(&cat, &lib, prober.as_ref(), &source, &dests, &opts)?;
        ctx.emit(&json!({ "dry_run": true, "plan": p }), || {
            let new = p
                .assets
                .iter()
                .filter(|x| x.state == ImportAssetState::Selected)
                .count();
            let known = p
                .assets
                .iter()
                .filter(|x| x.diff == ImportDiff::AlreadyImported)
                .count();
            println!("Dry run: nothing was written.");
            println!(
                "  {} assets on the card: {new} to copy, {known} already in the library",
                p.assets.len()
            );
            for s in &p.space {
                let free = s.bytes_free.map(human_bytes).unwrap_or_else(|| "?".into());
                println!(
                    "  {} ({}): needs {}, {} free",
                    s.destination,
                    super::common::path_str(&s.mount),
                    human_bytes(s.bytes_needed),
                    free
                );
            }
            for x in p
                .assets
                .iter()
                .filter(|x| x.state == ImportAssetState::Selected)
                .take(30)
            {
                for f in &x.files {
                    println!("  {} → {}", f.source, f.dest);
                }
            }
            if new > 30 {
                println!("  … ({} more assets)", new - 30);
            }
        })?;
        return Ok(EXIT_OK);
    }

    let mut dests = Vec::new();
    for (i, d) in a.dest.iter().enumerate() {
        dests.push(ingest::destination_volume(
            &lib,
            d,
            &name_of(d, i),
            VolumeKind::External,
        )?);
    }
    let session = {
        let mut cat = lib.open_catalog()?;
        let prober = maybe_prober(a.no_probe, ctx.json);
        ingest::plan(
            &mut cat,
            &lib,
            prober.as_ref(),
            &source,
            &a.label,
            &dests,
            &opts,
        )?
    };
    run_session(ctx, &lib, session, a.no_probe)
}

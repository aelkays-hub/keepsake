//! `keepsake ingest …` — copy a card to one or two drives, verify, catalogue.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use anyhow::{bail, Context, Result};
use clap::Subcommand;

use keepsake_core::catalog::{ImportSessionId, ImportStatus, VolumeKind};
use keepsake_core::ingest::fault::FaultPlan;
use keepsake_core::ingest::template::{Template, DEFAULT_TEMPLATE};
use keepsake_core::ingest::{self, IngestReport, PlanOptions, Progress, RunContext, Selection};
use keepsake_core::library::Library;

use super::library::maybe_prober;

#[derive(Subcommand)]
pub enum IngestAction {
    /// Copy a card to one or two destination folders/drives, verify every
    /// byte, catalogue it, and report whether the card is safe to format.
    Start {
        library: PathBuf,
        /// Root of the card (or a copy of one). Never written to.
        card: PathBuf,
        /// Destination folder. Give it twice for two drives.
        #[arg(long = "to", required = true)]
        to: Vec<PathBuf>,
        /// Name for the card volume.
        #[arg(long, default_value = "Card")]
        label: String,
        /// File-naming template. Tokens: {yyyy} {mm} {dd} {hh} {event} {camera} {lens} {seq} {orig}.
        #[arg(long, default_value = DEFAULT_TEMPLATE)]
        template: String,
        /// Value for {event}.
        #[arg(long)]
        event: Option<String>,
        /// Also copy assets that are already in the library.
        #[arg(long)]
        all: bool,
        /// Do not read capture dates (files go to 0000-undated).
        #[arg(long)]
        no_probe: bool,
        /// TESTING ONLY: allow destinations on the same disk as the card or
        /// each other. "Safe to format" then means two folders, not two disks.
        #[arg(long, hide = true)]
        unsafe_allow_same_disk: bool,
    },
    /// Resume an interrupted import (the most recent unfinished one by default).
    Resume {
        library: PathBuf,
        session: Option<String>,
        #[arg(long)]
        no_probe: bool,
    },
    /// Show an import's state and whether the card is safe to format.
    Status {
        library: PathBuf,
        session: Option<String>,
    },
    /// List import sessions.
    List { library: PathBuf },
}

fn open(library: &std::path::Path) -> Result<Library> {
    Library::open(library)
        .with_context(|| format!("{} is not a Keepsake library", library.display()))
}

fn pick_session(
    cat: &keepsake_core::catalog::Catalog,
    s: Option<String>,
    unfinished: bool,
) -> Result<ImportSessionId> {
    if let Some(s) = s {
        return s.parse().context("not a session id");
    }
    let list = cat.read(|tx| tx.imports().list())?;
    list.into_iter()
        .find(|x| !unfinished || x.status != ImportStatus::Completed)
        .map(|x| x.id)
        .context("no matching import session")
}

fn print_report(r: &IngestReport) {
    if let Some(s) = r.session {
        println!("Session {s}");
    }
    let states: Vec<String> = r.by_state.iter().map(|(k, v)| format!("{v} {k}")).collect();
    println!("  {} assets: {}", r.assets, states.join(", "));
    if r.bytes_copied > 0 {
        println!("  {:.1} MB copied", r.bytes_copied as f64 / 1_000_000.0);
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
    if r.safe_to_format {
        println!("  SAFE TO FORMAT: every selected asset is verified on two destinations.");
    } else {
        println!("  NOT safe to format:");
        for n in r.not_safe.iter().take(20) {
            println!("    - {n}");
        }
        if r.not_safe.len() > 20 {
            println!("    … and {} more", r.not_safe.len() - 20);
        }
    }
}

fn run_session(lib: &Library, session: ImportSessionId, no_probe: bool) -> Result<IngestReport> {
    let mut cat = lib.open_catalog()?;
    let prober = maybe_prober(no_probe);
    let cancel = AtomicBool::new(false);
    let fault = FaultPlan::none();
    let mut last = String::new();
    let mut progress = |p: &Progress| match p {
        Progress::FileStarted { file, bytes } => {
            if *bytes > 50_000_000 {
                println!("  copying {file} ({:.0} MB)", *bytes as f64 / 1_000_000.0);
            }
        }
        Progress::AssetState { asset, state } => {
            let line = format!("  {state}: {asset}");
            if line != last {
                if matches!(
                    state,
                    keepsake_core::catalog::ImportAssetState::Failed
                        | keepsake_core::catalog::ImportAssetState::Catalogued
                ) {
                    println!("{line}");
                }
                last = line;
            }
        }
        _ => {}
    };
    let mut ctx = RunContext {
        prober: prober.as_ref(),
        fault: &fault,
        cancel: &cancel,
        progress: &mut progress,
        retry_failed: true,
    };
    Ok(ingest::run(&mut cat, lib, session, &mut ctx)?)
}

pub fn run(action: IngestAction) -> Result<()> {
    match action {
        IngestAction::Start {
            library,
            card,
            to,
            label,
            template,
            event,
            all,
            no_probe,
            unsafe_allow_same_disk,
        } => {
            if to.len() > 2 {
                bail!("give one or two --to destinations");
            }
            let lib = open(&library)?;
            let mut dests = Vec::new();
            for (i, d) in to.iter().enumerate() {
                let name = d
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| format!("Destination {}", i + 1));
                dests.push(ingest::destination_volume(
                    &lib,
                    d,
                    &name,
                    VolumeKind::External,
                )?);
            }
            let opts = PlanOptions {
                template: Template::parse(&template)?,
                selection: if all {
                    Selection::All
                } else {
                    Selection::NewOnly
                },
                event,
                allow_same_filesystem: unsafe_allow_same_disk,
            };
            if unsafe_allow_same_disk {
                eprintln!("WARNING: --unsafe-allow-same-disk is for testing; do not format a card on this basis.");
            }
            let prober = maybe_prober(no_probe);
            let session = {
                let mut cat = lib.open_catalog()?;
                ingest::plan(
                    &mut cat,
                    &lib,
                    prober.as_ref(),
                    &card,
                    &label,
                    &dests,
                    &opts,
                )?
            };
            drop(prober);
            println!("Planned session {session}; copying…");
            let r = run_session(&lib, session, no_probe)?;
            print_report(&r);
        }
        IngestAction::Resume {
            library,
            session,
            no_probe,
        } => {
            let lib = open(&library)?;
            let s = {
                let cat = lib.open_catalog()?;
                pick_session(&cat, session, true)?
            };
            println!("Resuming session {s}…");
            let r = run_session(&lib, s, no_probe)?;
            print_report(&r);
        }
        IngestAction::Status { library, session } => {
            let lib = open(&library)?;
            let cat = lib.open_catalog()?;
            let s = pick_session(&cat, session, false)?;
            print_report(&ingest::report(&cat, s)?);
            let left: Vec<_> = cat
                .read(|tx| tx.imports().destinations(s))?
                .iter()
                .filter_map(|d| lib.volume(d.volume_id).ok()?.last_mount)
                .flat_map(|m| ingest::leftovers(&m))
                .collect();
            if !left.is_empty() {
                println!(
                    "  {} unfinished temporary file(s) remain; run `ingest resume`",
                    left.len()
                );
            }
        }
        IngestAction::List { library } => {
            let lib = open(&library)?;
            let cat = lib.open_catalog()?;
            for s in cat.read(|tx| tx.imports().list())? {
                println!(
                    "{}  {}  {}  safe_to_format={}",
                    s.id,
                    chrono::DateTime::from_timestamp_millis(s.started_ms)
                        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_default(),
                    s.status,
                    s.safe_to_format
                );
            }
        }
    }
    Ok(())
}

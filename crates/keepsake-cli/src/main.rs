use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod commands;

use commands::common::Ctx;

#[derive(Parser)]
#[command(
    name = "keepsake",
    version = keepsake_core::version(),
    about = "Keepsake: import, verify, search and back up your camera media",
    long_about = "Keepsake: import, verify, search and back up your camera media.\n\nSee docs/CLI.md for the full manual. Exit status: 0 ok, 1 error, 2 finished with problems, 130 stopped with Ctrl-C."
)]
struct Cli {
    /// Library folder (default: KEEPSAKE_LIBRARY, else the one set by `init`).
    #[arg(long, global = true, value_name = "PATH")]
    library: Option<PathBuf>,
    /// Print one JSON document on stdout instead of text (no progress bars).
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a library (or adopt an existing one) and make it the default.
    Init(commands::init::InitArgs),
    /// Import a card: copy to one or two drives, verify every byte,
    /// catalogue, and report whether the card is safe to format.
    Ingest(Box<commands::ingest::IngestArgs>),
    /// Add a folder of photos and videos already on disk, where they are:
    /// nothing is copied, and nothing is written into the folder.
    AddFolder(commands::add_folder::AddFolderArgs),
    /// Re-hash catalogued files and report missing or changed ones. Read-only.
    Verify(commands::verify::VerifyArgs),
    /// Find assets by words, date range, camera and type.
    Search(commands::search::SearchArgs),
    /// Shift capture times (XMP sidecars and catalog only; originals are never touched).
    ShiftTime(commands::shift::ShiftArgs),
    /// Copy the library to a drive: incremental and verified.
    Backup(commands::backup::BackupArgs),
    /// Rebuild the catalog from the files and XMP sidecars on every volume.
    RebuildCatalog(commands::rebuild::RebuildArgs),
    /// Show what Keepsake reads from a folder or card (capture time and
    /// confidence, camera, lens, video format, sequences). Read-only.
    Probe(commands::probe::ProbeArgs),
    /// Watch for camera cards: report each card and how many items are new
    /// since its last import; optionally import trusted cards automatically.
    Watch(commands::watch::WatchArgs),
    /// Cards this library remembers: list, trust for automatic import, rename, forget.
    Card {
        #[command(subcommand)]
        action: commands::card::CardAction,
    },
    /// Saved import settings for automatic imports.
    Preset {
        #[command(subcommand)]
        action: commands::preset::PresetAction,
    },
    /// Thumbnails, previews and video proxies: status, run, queue, bench.
    Renders {
        #[command(subcommand)]
        action: commands::renders::RendersAction,
    },
    /// Developer tools (synthetic libraries).
    #[command(hide = true)]
    Dev {
        #[command(subcommand)]
        action: commands::dev::DevAction,
    },
    /// Test-corpus tools for developers.
    #[command(hide = true)]
    Corpus {
        #[command(subcommand)]
        action: commands::corpus::CorpusAction,
    },
}

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();
    // Crashes are written to the local log folder (never sent anywhere).
    if let Some(d) = keepsake_core::logs::log_dir() {
        keepsake_core::logs::install_crash_hook(d, "keepsake");
    }

    let cli = Cli::parse();
    let ctx = Ctx {
        json: cli.json,
        library: cli.library,
    };
    let result = match cli.command {
        Commands::Init(a) => commands::init::run(&ctx, a),
        Commands::Ingest(a) => commands::ingest::run(&ctx, *a),
        Commands::AddFolder(a) => commands::add_folder::run(&ctx, a),
        Commands::Verify(a) => commands::verify::run(&ctx, a),
        Commands::Search(a) => commands::search::run(&ctx, a),
        Commands::ShiftTime(a) => commands::shift::run(&ctx, a),
        Commands::Backup(a) => commands::backup::run(&ctx, a),
        Commands::RebuildCatalog(a) => commands::rebuild::run(&ctx, a),
        Commands::Probe(a) => commands::probe::run(&ctx, a),
        Commands::Watch(a) => commands::watch::run(&ctx, a),
        Commands::Dev { action } => commands::dev::run(&ctx, action),
        Commands::Renders { action } => commands::renders::run(&ctx, action),
        Commands::Card { action } => commands::card::run(&ctx, action),
        Commands::Preset { action } => commands::preset::run(&ctx, action),
        Commands::Corpus { action } => commands::corpus::run(action).map(|()| 0),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            if ctx.json {
                eprintln!("{}", serde_json::json!({ "error": format!("{e:#}") }));
            } else {
                eprintln!("error: {e:#}");
            }
            ExitCode::from(1)
        }
    }
}

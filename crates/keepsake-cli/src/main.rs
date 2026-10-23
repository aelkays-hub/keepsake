use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(
    name = "keepsake",
    version = keepsake_core::version(),
    about = "Keepsake media library CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Test corpus tools: scan real Sony cards and manage expected results.
    Corpus {
        #[command(subcommand)]
        action: commands::corpus::CorpusAction,
    },
    /// Library and catalog: index volumes, rebuild, search and edit metadata.
    Library {
        #[command(subcommand)]
        action: commands::library::LibraryAction,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Corpus { action } => commands::corpus::run(action)?,
        Commands::Library { action } => commands::library::run(action)?,
    }

    Ok(())
}

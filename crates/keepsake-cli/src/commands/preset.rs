//! `keepsake preset save | list | delete`

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::Subcommand;
use serde_json::json;

use keepsake_core::cards;
use keepsake_core::ingest::template::DEFAULT_TEMPLATE;
use keepsake_core::presets::{self, Preset};

use super::common::{path_str, plain, Ctx, EXIT_OK};

#[derive(Subcommand)]
pub enum PresetAction {
    /// Save (or replace) a preset: where and how to import.
    Save {
        name: String,
        /// Destination folder (once or twice; two are needed for "safe to format").
        #[arg(long = "dest", required = true)]
        dest: Vec<PathBuf>,
        /// File-naming template (see `keepsake ingest --help`).
        #[arg(long, default_value = DEFAULT_TEMPLATE)]
        template: String,
        /// Value for {event} in the template.
        #[arg(long)]
        event: Option<String>,
    },
    /// List presets.
    List,
    /// Delete a preset (refused while a trusted card uses it).
    Delete { name: String },
}

pub fn run(ctx: &Ctx, action: PresetAction) -> Result<u8> {
    let lib = ctx.library()?;
    match action {
        PresetAction::Save {
            name,
            dest,
            template,
            event,
        } => {
            let mut destinations = Vec::new();
            for d in dest {
                if !d.is_dir() {
                    bail!(
                        "{} is not a folder; create it first (the drive must be connected)",
                        d.display()
                    );
                }
                destinations.push(plain(&std::fs::canonicalize(&d)?));
            }
            let p = Preset {
                name,
                destinations,
                template,
                event,
            };
            presets::save(&lib, &p)?;
            ctx.emit(&json!({ "preset": p }), || {
                let to: Vec<String> = p.destinations.iter().map(|d| path_str(d)).collect();
                println!(
                    "Saved preset {}: {} → {}",
                    p.name,
                    p.template,
                    to.join(" and ")
                );
            })?;
        }
        PresetAction::List => {
            let all = presets::list(&lib)?;
            ctx.emit(&json!({ "presets": all }), || {
                if all.is_empty() {
                    println!("No presets. Create one with `keepsake preset save <name> --dest A --dest B`.");
                }
                for p in &all {
                    let to: Vec<String> = p.destinations.iter().map(|d| path_str(d)).collect();
                    println!(
                        "{}: {} → {}{}",
                        p.name,
                        p.template,
                        to.join(" and "),
                        p.event
                            .as_deref()
                            .map(|e| format!(" (event {e})"))
                            .unwrap_or_default()
                    );
                }
            })?;
        }
        PresetAction::Delete { name } => {
            let users: Vec<String> = cards::list(&lib)?
                .into_iter()
                .filter(|c| {
                    c.trusted
                        && c.preset
                            .as_deref()
                            .is_some_and(|p| p.eq_ignore_ascii_case(name.trim()))
                })
                .map(|c| c.label)
                .collect();
            if !users.is_empty() {
                bail!(
                    "preset {name} is used by trusted card(s) {}; `keepsake card untrust` them first",
                    users.join(", ")
                );
            }
            if !presets::delete(&lib, &name)? {
                bail!("no preset called {name}");
            }
            ctx.emit(&json!({ "deleted": name }), || {
                println!("Deleted preset {name}.")
            })?;
        }
    }
    Ok(EXIT_OK)
}

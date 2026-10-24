//! `keepsake card list | trust | untrust | rename | forget`

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde_json::json;

use keepsake_core::cards::{self, CardRecord};
use keepsake_core::library::Library;
use keepsake_core::presets;

use super::common::{path_str, Ctx, EXIT_OK};

#[derive(Subcommand)]
pub enum CardAction {
    /// List the cards this library has seen.
    List,
    /// Import this card automatically (with `keepsake watch --auto-ingest`)
    /// using a preset.
    Trust {
        /// Card label or id (or the first characters of the id).
        card: String,
        /// Preset to import with (see `keepsake preset`).
        #[arg(long)]
        preset: String,
    },
    /// Stop importing this card automatically.
    Untrust { card: String },
    /// Give a card a name (shown instead of the volume label).
    Rename { card: String, name: String },
    /// Forget a card (its record and import history; media stays).
    Forget { card: String },
}

fn find(lib: &Library, name: &str) -> Result<CardRecord> {
    cards::find(lib, name)?
        .with_context(|| format!("no card called {name} (see `keepsake card list`)"))
}

fn when(ms: Option<i64>) -> String {
    ms.and_then(chrono::DateTime::from_timestamp_millis)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "never".into())
}

fn short(c: &CardRecord) -> String {
    c.id.to_string().chars().take(8).collect()
}

pub fn run(ctx: &Ctx, action: CardAction) -> Result<u8> {
    let lib = ctx.library()?;
    match action {
        CardAction::List => {
            let all = cards::list(&lib)?;
            ctx.emit(&json!({ "cards": all }), || {
                if all.is_empty() {
                    println!(
                        "No cards yet. Insert one while `keepsake watch` runs, or import one."
                    );
                    return;
                }
                for c in &all {
                    let trust = if c.trusted {
                        format!("trusted (preset {})", c.preset.as_deref().unwrap_or("none"))
                    } else {
                        "not trusted".into()
                    };
                    println!(
                        "{}  {}  last seen {}{}  last import {}  {trust}",
                        short(c),
                        c.label,
                        when(Some(c.last_seen_ms)),
                        c.last_mount
                            .as_deref()
                            .map(|m| format!(" at {}", path_str(m)))
                            .unwrap_or_default(),
                        when(c.last_import_ms),
                    );
                }
            })?;
        }
        CardAction::Trust { card, preset } => {
            let mut c = find(&lib, &card)?;
            let p = presets::get(&lib, &preset)?.with_context(|| {
                format!("no preset called {preset} (see `keepsake preset list`)")
            })?;
            if p.destinations.len() < 2 {
                eprintln!(
                    "Note: preset {} has one destination, so automatic imports will never be safe to format.",
                    p.name
                );
            }
            c.trusted = true;
            c.preset = Some(p.name.clone());
            cards::save(&lib, &c)?;
            ctx.emit(&json!({ "card": c }), || {
                println!(
                    "{} is trusted: `keepsake watch --auto-ingest` imports it with preset {}.",
                    c.label, p.name
                );
            })?;
        }
        CardAction::Untrust { card } => {
            let mut c = find(&lib, &card)?;
            c.trusted = false;
            cards::save(&lib, &c)?;
            ctx.emit(&json!({ "card": c }), || {
                println!("{} is no longer imported automatically.", c.label);
            })?;
        }
        CardAction::Rename { card, name } => {
            let name = name.trim().to_owned();
            if name.is_empty() {
                bail!("the new name is empty");
            }
            let mut c = find(&lib, &card)?;
            c.label = name;
            cards::save(&lib, &c)?;
            ctx.emit(&json!({ "card": c }), || {
                println!("Renamed to {}.", c.label)
            })?;
        }
        CardAction::Forget { card } => {
            let c = find(&lib, &card)?;
            cards::forget(&lib, c.id)?;
            ctx.emit(&json!({ "forgotten": c.id }), || {
                println!(
                    "Forgot {}. Its media and import sessions stay in the library.",
                    c.label
                );
            })?;
        }
    }
    Ok(EXIT_OK)
}

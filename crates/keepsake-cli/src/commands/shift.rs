//! `keepsake shift-time <selection> --by ±HH:MM[:SS]`

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Args;
use serde_json::json;

use keepsake_core::catalog::CaptureTime;
use keepsake_core::metadata::MetadataService;
use keepsake_core::select;
use keepsake_core::xmp::format_date;

use super::common::{describe, Ctx, EXIT_OK};
use super::search::Filters;

#[derive(Args)]
pub struct ShiftArgs {
    /// Catalogued files, or folders (every asset with a file inside).
    pub selection: Vec<PathBuf>,
    /// Shift, e.g. +01:00:00, -0:30 (hours may exceed 24).
    #[arg(long, allow_hyphen_values = true)]
    pub by: String,
    /// Also select assets matching these words.
    #[arg(long)]
    pub query: Option<String>,
    #[command(flatten)]
    pub filters: Filters,
    /// Show what would change without writing anything.
    #[arg(long)]
    pub dry_run: bool,
}

fn shifted(c: CaptureTime, secs: i64) -> CaptureTime {
    CaptureTime {
        utc_ms: c.utc_ms + secs * 1000,
        ..c
    }
}

pub fn run(ctx: &Ctx, a: ShiftArgs) -> Result<u8> {
    let secs = select::parse_shift(&a.by)
        .with_context(|| format!("--by must look like +01:00:00 or -0:30, not {}", a.by))?;
    if secs == 0 {
        bail!("--by is zero; nothing to do");
    }
    let lib = ctx.library()?;
    let mut cat = lib.open_catalog()?;
    let query = if a.query.is_some() || !a.filters.is_empty() {
        Some(a.filters.to_query(a.query.clone(), None)?)
    } else {
        None
    };
    if a.selection.is_empty() && query.is_none() {
        bail!("select assets with paths and/or --query/--from/--to/--camera/--type");
    }
    let (ids, unmatched) = select::resolve(&cat, &lib, &a.selection, query.as_ref())?;
    if !unmatched.is_empty() {
        let list: Vec<String> = unmatched.iter().map(|p| p.display().to_string()).collect();
        bail!("not in the catalog: {}", list.join(", "));
    }
    if ids.is_empty() {
        bail!("the selection matches no assets");
    }
    let mut changed = Vec::new();
    let mut skipped = Vec::new();
    for id in &ids {
        let row = describe(&cat, &lib, *id)?;
        let asset = cat.read(|tx| tx.assets().require(*id))?;
        let Some(before) = asset.capture else {
            skipped.push(json!({ "asset": id, "file": row.file, "reason": "no capture time" }));
            continue;
        };
        let after = shifted(before, secs);
        if !a.dry_run {
            MetadataService::new(&lib, &mut cat).shift_capture_time(*id, secs)?;
        }
        changed.push(json!({
            "asset": id,
            "file": row.file,
            "before": format_date(before),
            "after": format_date(after),
        }));
    }
    let doc =
        json!({ "by_seconds": secs, "dry_run": a.dry_run, "changed": changed, "skipped": skipped });
    ctx.emit(&doc, || {
        let verb = if a.dry_run { "Would shift" } else { "Shifted" };
        println!(
            "{verb} {} asset(s) by {} (XMP sidecars and catalog only; originals untouched)",
            changed.len(),
            a.by
        );
        for c in &changed {
            println!(
                "  {} → {}  {}",
                c["before"].as_str().unwrap_or(""),
                c["after"].as_str().unwrap_or(""),
                c["file"].as_str().unwrap_or("")
            );
        }
        for s in &skipped {
            println!(
                "  skipped (no capture time): {}",
                s["file"].as_str().unwrap_or("")
            );
        }
    })?;
    Ok(EXIT_OK)
}

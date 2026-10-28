//! `keepsake rebuild-catalog`

use anyhow::Result;
use clap::Args;
use serde_json::json;

use keepsake_core::catalog::Catalog;
use keepsake_core::rebuild::{rebuild_library_opts, RebuildOptions};

use super::common::{maybe_prober, Ctx, EXIT_OK, EXIT_PROBLEMS};

#[derive(Args)]
pub struct RebuildArgs {
    /// Delete the catalog database first and rebuild from scratch.
    #[arg(long)]
    pub fresh: bool,
    /// Do not read capture dates etc. from the media (sidecars only).
    #[arg(long)]
    pub no_probe: bool,
}

pub fn run(ctx: &Ctx, a: RebuildArgs) -> Result<u8> {
    let lib = ctx.library()?;
    if a.fresh {
        Catalog::delete_files(&lib.catalog_path())?;
    }
    let mut cat = lib.open_catalog()?;
    let prober = maybe_prober(a.no_probe, ctx.json);
    // The thumbnail cache is never catalogued, even inside a volume.
    let results = rebuild_library_opts(
        &mut cat,
        &lib,
        &mut RebuildOptions {
            prober: prober.as_ref(),
            skip: super::add_folder::cache_dirs(),
            ..Default::default()
        },
    )?;
    let problems: usize = results
        .iter()
        .filter_map(|(_, r)| r.as_ref())
        .map(|r| r.problems.len())
        .sum();
    let doc = json!({
        "fresh": a.fresh,
        "volumes": results.iter().map(|(v, r)| json!({
            "id": v.id,
            "label": v.label,
            "online": r.is_some(),
            "report": r,
        })).collect::<Vec<_>>(),
        "assets": cat.read(|tx| tx.assets().count())?,
    });
    ctx.emit(&doc, || {
        if a.fresh {
            println!("Deleted the catalog; rebuilt from files and sidecars.");
        }
        for (v, r) in &results {
            match r {
                Some(r) => {
                    println!(
                        "{}: {} assets ({} probed), {} files, {} sidecars, {} sequences",
                        v.label, r.assets, r.probed, r.files_indexed, r.sidecars_used, r.sequences
                    );
                    for p in r.problems.iter().take(10) {
                        println!("  problem: {p}");
                    }
                }
                None => println!("{}: offline, skipped", v.label),
            }
        }
    })?;
    Ok(if problems > 0 { EXIT_PROBLEMS } else { EXIT_OK })
}

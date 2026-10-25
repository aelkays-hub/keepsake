//! Hidden developer tools: `keepsake dev seed`.

use anyhow::{bail, Result};
use clap::Subcommand;
use serde_json::json;

use keepsake_core::render::Cache;
use keepsake_core::seed::{seed, SeedOptions};
use keepsake_core::settings::Settings;

use super::common::{Ctx, EXIT_OK};

#[derive(Subcommand)]
pub enum DevAction {
    /// Fill an EMPTY library with synthetic assets (UI tests, profiling).
    Seed {
        #[arg(long, default_value_t = 1000)]
        assets: u64,
        #[arg(long, default_value_t = 64)]
        distinct: u32,
        /// Share of assets on the unplugged volume (0–1).
        #[arg(long, default_value_t = 0.25)]
        offline: f64,
    },
}

pub fn run(ctx: &Ctx, action: DevAction) -> Result<u8> {
    match action {
        DevAction::Seed {
            assets,
            distinct,
            offline,
        } => {
            let lib = ctx.library()?;
            let cat = lib.open_catalog()?;
            if cat.read(|tx| tx.assets().count())? > 0 {
                bail!("the library is not empty; seed only throwaway libraries");
            }
            drop(cat);
            let cache = Cache::new(Settings::load()?.render.cache_root()?);
            let t = std::time::Instant::now();
            let rep = seed(
                &lib,
                &cache,
                &SeedOptions {
                    assets,
                    distinct,
                    offline_share: offline.clamp(0.0, 1.0),
                },
            )?;
            ctx.emit(
                &json!({ "seed": rep, "seconds": t.elapsed().as_secs_f64() }),
                || {
                    println!(
                        "Seeded {} assets in {:.1}s ({} renditions in {}).",
                        rep.assets,
                        t.elapsed().as_secs_f64(),
                        rep.renditions,
                        cache.root().display()
                    );
                },
            )?;
            Ok(EXIT_OK)
        }
    }
}

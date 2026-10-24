//! `keepsake init <library-path>`

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use serde_json::json;

use keepsake_core::library::Library;

use super::common::{path_str, Config, Ctx, EXIT_OK};

#[derive(Args)]
pub struct InitArgs {
    /// Folder for the new library (created if missing).
    pub path: PathBuf,
}

pub fn run(ctx: &Ctx, a: InitArgs) -> Result<u8> {
    let (lib, created) = match Library::open(&a.path) {
        Ok(l) => (l, false),
        Err(_) => (Library::create(&a.path)?, true),
    };
    lib.open_catalog()?;
    let root = super::common::plain(&std::fs::canonicalize(lib.root())?);
    let mut cfg = Config::load()?;
    cfg.default_library = Some(root.clone());
    let cfg_path = cfg.save()?;
    ctx.emit(
        &json!({
            "library": root,
            "id": lib.id(),
            "created": created,
            "default": true,
            "config": cfg_path,
        }),
        || {
            if created {
                println!("Created library {} at {}", lib.id(), path_str(&root));
            } else {
                println!("{} is already a library ({})", path_str(&root), lib.id());
            }
            println!(
                "It is now the default library (saved in {}).",
                path_str(&cfg_path)
            );
        },
    )?;
    Ok(EXIT_OK)
}

//! `keepsake add-folder`

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use serde_json::json;

use keepsake_core::library::LibraryError;
use keepsake_core::rebuild::{add_folder, RebuildOptions, RebuildProgress};

use super::common::{
    bytes_bar, cancel_flag, maybe_prober, path_str, Config, Ctx, EXIT_CANCELLED, EXIT_OK,
    EXIT_PROBLEMS,
};

#[derive(Args)]
pub struct AddFolderArgs {
    /// A folder of photos and videos already on this computer or a drive.
    pub folder: PathBuf,
    /// Do not read capture dates etc. from the media (sidecars only).
    #[arg(long)]
    pub no_probe: bool,
}

/// The thumbnail cache: never catalogued, even when it is inside the folder.
pub fn cache_dirs() -> Vec<PathBuf> {
    Config::load()
        .ok()
        .and_then(|c| c.render.cache_root().ok())
        .into_iter()
        .collect()
}

pub fn run(ctx: &Ctx, a: AddFolderArgs) -> Result<u8> {
    let lib = ctx.library()?;
    let mut cat = lib.open_catalog()?;
    let prober = maybe_prober(a.no_probe, ctx.json);
    let bar = bytes_bar(ctx.json, 0, "Reading");
    let mut progress = |p: &RebuildProgress<'_>| match p {
        RebuildProgress::Listed { bytes, .. } => bar.set_length(*bytes),
        RebuildProgress::File { path, bytes } => {
            bar.inc(*bytes);
            bar.set_message(path.to_string());
        }
        RebuildProgress::Phase(name) => {
            bar.set_prefix(name.to_string());
            bar.set_message("");
        }
    };
    let mut opts = RebuildOptions {
        prober: prober.as_ref(),
        skip: cache_dirs(),
        cancel: Some(cancel_flag()),
        progress: Some(&mut progress),
    };
    let result = add_folder(&mut cat, &lib, &a.folder, &mut opts);
    bar.finish_and_clear();
    let (rec, r) = match result {
        Ok(x) => x,
        Err(LibraryError::Cancelled) => {
            ctx.emit(&json!({ "cancelled": true }), || {
                println!("Stopped. Nothing was added; run the command again to start over.");
            })?;
            return Ok(EXIT_CANCELLED);
        }
        Err(e) => return Err(e.into()),
    };
    let folder = rec.last_mount.as_deref().map(path_str).unwrap_or_default();
    let doc = json!({
        "volume": rec.id,
        "label": rec.label,
        "folder": folder,
        "report": r,
    });
    ctx.emit(&doc, || {
        println!(
            "Added {} ({folder}): {} items, {} files, {} with Lightroom or Keepsake sidecars.",
            rec.label, r.assets, r.files_indexed, r.sidecars_used
        );
        println!(
            "Nothing in the folder was changed. Ratings and other edits are kept in the library."
        );
        let other = r.skipped.len();
        if other > 0 {
            println!("{other} other files are not photos or videos and were left out.");
        }
        for p in r.problems.iter().take(10) {
            println!("  problem: {p}");
        }
    })?;
    Ok(if r.problems.is_empty() {
        EXIT_OK
    } else {
        EXIT_PROBLEMS
    })
}

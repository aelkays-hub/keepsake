//! `keepsake verify [--volume X]`

use anyhow::{bail, Result};
use clap::Args;

use keepsake_core::verify::{self, Problem, VerifyProgress};

use super::common::{bytes_bar, cancel_flag, Ctx, EXIT_CANCELLED, EXIT_OK, EXIT_PROBLEMS};

#[derive(Args)]
pub struct VerifyArgs {
    /// Only this volume (label or id). Default: every mounted volume.
    #[arg(long)]
    pub volume: Option<String>,
}

pub fn run(ctx: &Ctx, a: VerifyArgs) -> Result<u8> {
    let lib = ctx.library()?;
    let cat = lib.open_catalog()?;
    let only = match &a.volume {
        Some(name) => match verify::find_volume(&cat, name)? {
            Some(id) => {
                if verify::mount_of(&cat, &lib, id).is_none() {
                    bail!("volume {name} is not mounted");
                }
                Some(id)
            }
            None => bail!("no volume called {name}"),
        },
        None => None,
    };
    let cancel = cancel_flag();
    let mut bar = None;
    let mut progress = |p: &VerifyProgress<'_>| match p {
        VerifyProgress::Volume { label, bytes, .. } => {
            if let Some(b) = bar.take() {
                let b: indicatif::ProgressBar = b;
                b.finish_and_clear();
            }
            bar = Some(bytes_bar(ctx.json, *bytes, label));
        }
        VerifyProgress::File { path, bytes } => {
            if let Some(b) = &bar {
                b.set_message(path.to_string());
                b.inc(*bytes);
            }
        }
    };
    let r = verify::verify(&cat, &lib, only, &mut progress, cancel)?;
    if let Some(b) = bar {
        b.finish_and_clear();
    }
    ctx.emit(&serde_json::to_value(&r)?, || {
        if r.issues.is_empty() && !r.cancelled {
            println!("All checked files are intact.");
        } else if !r.issues.is_empty() {
            println!(
                "{} problem(s): {} missing, {} changed, {} unreadable",
                r.issues.len(),
                r.missing(),
                r.changed(),
                r.issues.len() - r.missing() - r.changed()
            );
        }
        for v in &r.volumes {
            match &v.mount {
                Some(m) => println!(
                    "  {} ({}): {} files checked, {} ok",
                    v.label,
                    super::common::path_str(m),
                    v.files_checked,
                    v.ok
                ),
                None => println!("  {}: not mounted, not checked", v.label),
            }
        }
        for i in &r.issues {
            let what = match &i.problem {
                Problem::Missing => "MISSING".to_owned(),
                Problem::Changed { .. } => "CHANGED".to_owned(),
                Problem::Unreadable { error } => format!("UNREADABLE ({error})"),
            };
            println!("  {what}: {} on {}", i.path, i.volume);
        }
        if r.cancelled {
            println!("Stopped early (Ctrl-C); the list above covers what was checked.");
        }
    })?;
    Ok(if r.cancelled {
        EXIT_CANCELLED
    } else if r.issues.is_empty() {
        EXIT_OK
    } else {
        EXIT_PROBLEMS
    })
}

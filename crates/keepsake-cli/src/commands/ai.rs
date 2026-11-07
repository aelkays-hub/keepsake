//! `keepsake ai status | install | run | duplicates | retag` (ADR-035)

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde_json::json;

use keepsake_core::ai::{
    self, duplicates, install, LabelSet, Purpose, JOB_AI, JOB_AI_TAG, MODEL_ID,
};
use keepsake_core::render::{Cache, RenderEvent, RenderOptions, RenderService, RenderTools};
use keepsake_core::settings::Settings;

use super::common::{cancel_flag, describe, human_bytes, path_str, Ctx, EXIT_CANCELLED, EXIT_OK};

#[derive(Subcommand)]
pub enum AiAction {
    /// What is installed and how far the analysis has got.
    Status,
    /// Download the model into this computer's cache folder (372 MB).
    Install,
    /// Find duplicates among the RAW photos of some dates (`--from`/`--to`,
    /// default every dated item), or tag them all (`--tags`). Makes missing
    /// thumbnails first.
    Run(RunArgs),
    /// Groups of same-day duplicates: every day, or one day's groups.
    Duplicates {
        /// A day, `YYYY-MM-DD`.
        date: Option<String>,
    },
    /// Give AI tags again from the stored analyses (no model needed).
    Retag,
}

#[derive(Args)]
pub struct RunArgs {
    /// First day, `YYYY-MM-DD` (with --to; default: the whole library).
    #[arg(long, requires = "to")]
    pub from: Option<String>,
    /// Last day, `YYYY-MM-DD`, inclusive.
    #[arg(long, requires = "from")]
    pub to: Option<String>,
    /// Tag the photos instead of finding duplicates.
    #[arg(long)]
    pub tags: bool,
    /// Use the processor only, not the graphics card.
    #[arg(long)]
    pub cpu: bool,
    /// Keep working on battery.
    #[arg(long)]
    pub ignore_battery: bool,
}

pub fn run(ctx: &Ctx, action: AiAction) -> Result<u8> {
    match action {
        AiAction::Status => status(ctx),
        AiAction::Install => {
            let mut last = 0u64;
            let path = install::install_model(cancel_flag(), &mut |done, total| {
                if !ctx.json && done >= last + 25 * 1024 * 1024 {
                    last = done;
                    eprintln!("{} of {}", human_bytes(done), human_bytes(total));
                }
            });
            match path {
                Ok(p) => {
                    ctx.emit(&json!({ "model": path_str(&p) }), || {
                        println!("Installed the model at {}.", p.display())
                    })?;
                    Ok(EXIT_OK)
                }
                Err(ai::AiError::Cancelled) => Ok(EXIT_CANCELLED),
                Err(e) => Err(e.into()),
            }
        }
        AiAction::Run(a) => run_all(ctx, &a),
        AiAction::Duplicates { date } => dups(ctx, date.as_deref()),
        AiAction::Retag => {
            let lib = ctx.library()?;
            let mut cat = lib.open_catalog()?;
            let n = ai::analyze::retag_all(&mut cat, &LabelSet::builtin()?)?;
            ctx.emit(&json!({ "retagged": n }), || {
                println!("Tagged {n} item(s) again.")
            })?;
            Ok(EXIT_OK)
        }
    }
}

fn status(ctx: &Ctx) -> Result<u8> {
    let lib = ctx.library()?;
    let cat = lib.open_catalog()?;
    let counts = cat.read(|tx| tx.ai().counts(MODEL_ID, JOB_AI))?;
    let settings = Settings::load()?;
    let model = install::find_model();
    let runtime = install::find_runtime();
    ctx.emit(
        &json!({
            "enabled": settings.ai.enabled,
            "gpu": settings.ai.gpu,
            "model": model.as_deref().map(path_str),
            "runtime": runtime.as_deref().map(path_str),
            "model_id": MODEL_ID,
            "counts": counts,
        }),
        || {
            println!(
                "AI in the app: {}",
                if settings.ai.enabled { "on" } else { "off" }
            );
            match &model {
                Some(p) => println!("Model:   {}", p.display()),
                None => println!("Model:   not installed (keepsake ai install)"),
            }
            match &runtime {
                Some(p) => println!("Runtime: {}", p.display()),
                None => println!("Runtime: ONNX Runtime not found"),
            }
            println!(
                "Analysed {} of {} item(s); {} queued, {} failed.",
                counts.analysed, counts.assets, counts.pending, counts.failed
            );
        },
    )?;
    Ok(EXIT_OK)
}

fn run_all(ctx: &Ctx, a: &RunArgs) -> Result<u8> {
    let lib = ctx.library()?;
    let settings = Settings::load()?;
    let cache = Cache::new(settings.render.cache_root()?);
    {
        let mut cat = lib.open_catalog()?;
        let purpose = if a.tags {
            Purpose::Tags
        } else {
            Purpose::Duplicates
        };
        let from = a.from.as_deref().unwrap_or("0000-01-01");
        let to = a.to.as_deref().unwrap_or("9999-12-31");
        ai::analyze::enqueue_range(&mut cat, from, to, purpose)?;
    }
    let mut opts = RenderOptions::from_settings(&settings.render);
    opts.proxy_workers = 0;
    opts.ai = true;
    opts.ai_gpu = !a.cpu && settings.ai.gpu;
    opts.pause_on_battery = settings.render.pause_on_battery && !a.ignore_battery;
    let svc = RenderService::new(&lib, RenderTools::discover(), cache, opts)
        .with_power(Arc::new(keepsake_platform::on_battery));
    let cancel: &AtomicBool = cancel_flag();
    let mut analysed = 0u64;
    let mut problem: Option<String> = None;
    svc.run(cancel, &mut |ev| match ev {
        RenderEvent::AiReady { accel } => {
            if !ctx.json {
                eprintln!("Model loaded on the {}.", accel.describe());
            }
        }
        RenderEvent::AiUnavailable { reason } => problem = Some(reason.clone()),
        RenderEvent::Done { kind, .. } if kind == JOB_AI || kind == JOB_AI_TAG => {
            analysed += 1;
            if !ctx.json && analysed.is_multiple_of(100) {
                eprintln!("{analysed} analysed…");
            }
        }
        _ => {}
    })
    .context("running the AI worker")?;
    if let Some(p) = problem {
        anyhow::bail!("the AI could not start: {p}");
    }
    ctx.emit(&json!({ "analysed": analysed }), || {
        println!("Analysed {analysed} item(s).")
    })?;
    Ok(if cancel.load(std::sync::atomic::Ordering::SeqCst) {
        EXIT_CANCELLED
    } else {
        EXIT_OK
    })
}

fn dups(ctx: &Ctx, date: Option<&str>) -> Result<u8> {
    let lib = ctx.library()?;
    let cat = lib.open_catalog()?;
    match date {
        None => {
            let s = duplicates::summary(&cat)?;
            ctx.emit(&json!({ "duplicates": s }), || {
                for d in &s.days {
                    println!(
                        "{}  {} group(s), {} item(s), {} to decide",
                        d.date, d.groups, d.items, d.undecided
                    );
                }
                println!(
                    "{} group(s) of {} item(s); {} to decide.",
                    s.groups, s.items, s.undecided
                );
            })?;
        }
        Some(d) => {
            let groups = duplicates::day_groups(&cat, d)?;
            let mut rows = Vec::new();
            for g in &groups {
                let mut members = Vec::new();
                for m in &g.members {
                    let file = describe(&cat, &lib, m.asset_id).ok().and_then(|r| r.file);
                    members.push(json!({
                        "asset": m.asset_id,
                        "file": file,
                        "keeper": m.asset_id == g.keeper,
                        "choice": m.choice,
                        "similarity": m.similarity,
                        "exact": m.exact,
                    }));
                }
                rows.push(json!({ "keeper": g.keeper, "decided": g.decided, "members": members }));
            }
            ctx.emit(&json!({ "date": d, "groups": rows }), || {
                for (i, g) in rows.iter().enumerate() {
                    println!("Group {}:", i + 1);
                    for m in g["members"].as_array().into_iter().flatten() {
                        let mark = if m["keeper"].as_bool() == Some(true) {
                            "*"
                        } else {
                            " "
                        };
                        println!(
                            "  {mark} {}  {}",
                            m["file"].as_str().unwrap_or("?"),
                            m["choice"].as_str().unwrap_or("")
                        );
                    }
                }
                if rows.is_empty() {
                    println!("No duplicates on {d}.");
                }
            })?;
        }
    }
    Ok(EXIT_OK)
}

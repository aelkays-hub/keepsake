//! `keepsake renders status | run | enqueue | show | retry | evict | bench`

use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Args, Subcommand};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use serde_json::json;

use keepsake_core::catalog::{AssetId, JobState, MediaKind, Priority, VolumeKind};
use keepsake_core::library::Library;
use keepsake_core::render::service::{self, PowerCheck};
use keepsake_core::render::{
    self, Cache, RenderEvent, RenderOptions, RenderService, RenderTools, Variant, JOB_PROXY,
    JOB_STILLS,
};
use keepsake_core::select;
use keepsake_core::settings::Settings;

use super::common::{
    cancel_flag, describe, human_bytes, path_str, Ctx, EXIT_CANCELLED, EXIT_OK, EXIT_PROBLEMS,
};
use super::search::Filters;

#[derive(Subcommand)]
pub enum RendersAction {
    /// Jobs by state and class, cache use, and whether workers would pause.
    Status,
    /// Make renditions now: queue the backlog, then work through the queue
    /// (visible first, then recent imports, then the rest).
    Run(RunArgs),
    /// Queue (or re-queue) renditions for some assets.
    Enqueue(EnqueueArgs),
    /// Print where an asset's renditions are.
    Show {
        /// A catalogued file.
        file: PathBuf,
    },
    /// Queue failed jobs again.
    Retry,
    /// Trim the cache to its limit now.
    Evict {
        /// Limit in bytes (default: the configured `max_bytes`).
        #[arg(long)]
        max_bytes: Option<u64>,
    },
    /// Measure items per minute per type on this machine (writes nothing
    /// outside a temporary folder).
    Bench(BenchArgs),
}

#[derive(Args)]
pub struct RunArgs {
    /// Thumbnail/preview workers (default: settings, else half the cores, 1–4).
    #[arg(long)]
    pub workers: Option<usize>,
    /// Proxy workers (default: settings, else 1).
    #[arg(long)]
    pub proxy_workers: Option<usize>,
    /// H.264 encoder to use instead of detecting one (e.g. libopenh264).
    #[arg(long)]
    pub encoder: Option<String>,
    /// Keep working on battery.
    #[arg(long)]
    pub ignore_battery: bool,
    /// Keep running and wait for new jobs until Ctrl-C.
    #[arg(long)]
    pub keep_running: bool,
    /// Only run what is queued; don't queue the backlog first.
    #[arg(long)]
    pub queued_only: bool,
}

#[derive(Args)]
pub struct EnqueueArgs {
    /// Every asset not queued yet.
    #[arg(long)]
    pub all: bool,
    /// Catalogued files or folders.
    pub paths: Vec<PathBuf>,
    #[arg(long)]
    pub query: Option<String>,
    #[command(flatten)]
    pub filters: Filters,
    /// Run these first (as if on screen).
    #[arg(long)]
    pub visible: bool,
}

#[derive(Args)]
pub struct BenchArgs {
    /// A folder of media (e.g. $KEEPSAKE_CORPUS/library-sample). Only read.
    pub folder: PathBuf,
    /// Items per photo type.
    #[arg(long, default_value_t = 20)]
    pub per_type: usize,
    /// Video clips (posters and proxies).
    #[arg(long, default_value_t = 4)]
    pub videos: usize,
    #[arg(long)]
    pub workers: Option<usize>,
    #[arg(long)]
    pub encoder: Option<String>,
}

struct Env {
    lib: Library,
    settings: Settings,
    cache: Cache,
    tools: RenderTools,
}

fn env(ctx: &Ctx) -> Result<Env> {
    let lib = ctx.library()?;
    let settings = Settings::load()?;
    let cache = Cache::new(settings.render.cache_root()?);
    Ok(Env {
        lib,
        settings,
        cache,
        tools: RenderTools::discover(),
    })
}

fn power() -> PowerCheck {
    Arc::new(keepsake_platform::on_battery)
}

pub fn run(ctx: &Ctx, action: RendersAction) -> Result<u8> {
    match action {
        RendersAction::Status => status(ctx),
        RendersAction::Run(a) => run_queue(ctx, a),
        RendersAction::Enqueue(a) => enqueue(ctx, a),
        RendersAction::Show { file } => show(ctx, &file),
        RendersAction::Retry => {
            let e = env(ctx)?;
            let mut cat = e.lib.open_catalog()?;
            let n = cat.write(|tx| tx.jobs().retry_failed(&[]))?;
            ctx.emit(&json!({ "requeued": n }), || {
                println!("Queued {n} failed job(s) again.")
            })?;
            Ok(EXIT_OK)
        }
        RendersAction::Evict { max_bytes } => {
            let e = env(ctx)?;
            let mut cat = e.lib.open_catalog()?;
            let max = max_bytes.unwrap_or(e.settings.render.max_bytes);
            let rep = e.cache.evict(&mut cat, &e.lib, max)?;
            ctx.emit(&json!({ "evict": rep }), || {
                println!(
                    "Removed {} rendition(s), {} freed; cache now {}.",
                    rep.removed,
                    human_bytes(rep.freed_bytes),
                    human_bytes(rep.bytes_after)
                );
                if rep.kept_offline_bytes > 0 {
                    println!(
                        "Kept {} for media that is offline (never evicted).",
                        human_bytes(rep.kept_offline_bytes)
                    );
                }
            })?;
            Ok(EXIT_OK)
        }
        RendersAction::Bench(a) => bench(ctx, a),
    }
}

fn count(c: &keepsake_core::catalog::JobCounts, kind: &str, state: JobState) -> u64 {
    c.by_kind
        .get(kind)
        .and_then(|m| m.get(state.as_str()))
        .copied()
        .unwrap_or(0)
}

fn status(ctx: &Ctx) -> Result<u8> {
    let e = env(ctx)?;
    let cat = e.lib.open_catalog()?;
    let st = service::status(&cat, &e.lib, &e.cache, &e.settings.render, &power())?;
    let failed_rows: Vec<_> = st
        .failed
        .iter()
        .map(|j| {
            let file = describe(&cat, &e.lib, j.asset_id).ok().and_then(|r| r.file);
            json!({ "job": j.id, "kind": j.kind, "asset": j.asset_id, "file": file, "error": j.error })
        })
        .collect();
    ctx.emit(&json!({ "status": st, "failed_files": failed_rows }), || {
        let c = &st.jobs;
        let q = |p: &str| c.queued_by_priority.get(p).copied().unwrap_or(0);
        println!(
            "Renditions: {} done, {} queued ({} visible, {} recent, {} backlog), {} running, {} failed",
            c.total(JobState::Done),
            c.total(JobState::Queued),
            q("visible"),
            q("recent"),
            q("backlog"),
            c.total(JobState::Running),
            c.total(JobState::Failed)
        );
        for (name, kind) in [("thumbnails/previews", JOB_STILLS), ("proxies", JOB_PROXY)] {
            println!(
                "  {name}: {} done, {} queued, {} failed",
                count(c, kind, JobState::Done),
                count(c, kind, JobState::Queued),
                count(c, kind, JobState::Failed)
            );
        }
        println!(
            "Cache: {} of {} in {} ({} file(s))",
            human_bytes(st.cache.bytes),
            human_bytes(st.cache.max_bytes),
            path_str(&st.cache.root),
            st.cache.files
        );
        if st.cache.kept_offline_bytes > 0 {
            println!(
                "  {} belong to offline media and are never evicted.",
                human_bytes(st.cache.kept_offline_bytes)
            );
        }
        if let Some(p) = st.paused {
            println!("Workers would pause now: {}.", p.describe());
        }
        for f in &failed_rows {
            println!(
                "  FAILED {}: {}",
                f["file"].as_str().unwrap_or("?"),
                f["error"].as_str().unwrap_or("")
            );
        }
    })?;
    Ok(EXIT_OK)
}

fn bar(json: bool, total: u64) -> ProgressBar {
    if json || !std::io::stderr().is_terminal() {
        return ProgressBar::hidden();
    }
    let pb = ProgressBar::with_draw_target(Some(total), ProgressDrawTarget::stderr());
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner} renders [{bar:30}] {pos}/{len} eta {eta} {wide_msg}",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=> "),
    );
    pb
}

fn run_queue(ctx: &Ctx, a: RunArgs) -> Result<u8> {
    let e = env(ctx)?;
    let mut cat = e.lib.open_catalog()?;
    if !a.queued_only {
        render::enqueue_backlog(&mut cat)?;
    }
    let queued = cat.read(|tx| tx.jobs().counts())?.total(JobState::Queued);
    let mut opts = RenderOptions::from_settings(&e.settings.render);
    if let Some(w) = a.workers {
        opts.workers = w.max(1);
    }
    if let Some(w) = a.proxy_workers {
        opts.proxy_workers = w;
    }
    opts.forced_encoder = a.encoder.clone();
    opts.until_empty = !a.keep_running;
    if a.ignore_battery {
        opts.pause_on_battery = false;
    }
    if !ctx.json {
        println!(
            "Rendering {queued} job(s) with {} image and {} proxy worker(s) into {}.",
            opts.workers,
            opts.proxy_workers,
            path_str(e.cache.root())
        );
    }
    let svc =
        RenderService::new(&e.lib, e.tools.clone(), e.cache.clone(), opts).with_power(power());
    let pb = bar(ctx.json, queued);
    let cancel = cancel_flag();
    let mut failures = 0u64;
    let mut cancelled = false;
    let mut out = std::io::stdout();
    svc.run(cancel, &mut |ev| {
        match ev {
            RenderEvent::Done { .. } => pb.inc(1),
            RenderEvent::Failed { retry, error, .. } => {
                pb.inc(1);
                if !retry {
                    failures += 1;
                }
                pb.println(format!("failed: {error}"));
            }
            RenderEvent::Deferred { .. } => pb.inc(1),
            RenderEvent::Paused { reason } => {
                pb.set_message(format!("paused: {}", reason.describe()))
            }
            RenderEvent::Resumed => pb.set_message(""),
            RenderEvent::Finished { cancelled: c, .. } => cancelled = *c,
            _ => {}
        }
        if ctx.json {
            if let Ok(line) = serde_json::to_string(ev) {
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
            }
        } else if let RenderEvent::Finished {
            done,
            failed,
            cancelled,
        } = ev
        {
            pb.finish_and_clear();
            println!("Done: {done} job(s), {failed} failed for good.");
            if *cancelled {
                println!(
                    "Stopped after the current job(s). Run `keepsake renders run` to continue."
                );
            }
        } else if let RenderEvent::Evicted { report } = ev {
            pb.println(format!(
                "cache trimmed: {} rendition(s), {} freed",
                report.removed,
                human_bytes(report.freed_bytes)
            ));
        }
    })?;
    Ok(if cancelled {
        EXIT_CANCELLED
    } else if failures > 0 {
        EXIT_PROBLEMS
    } else {
        EXIT_OK
    })
}

fn enqueue(ctx: &Ctx, a: EnqueueArgs) -> Result<u8> {
    let e = env(ctx)?;
    let mut cat = e.lib.open_catalog()?;
    let query = if a.query.is_some() || !a.filters.is_empty() {
        Some(a.filters.to_query(a.query.clone(), None)?)
    } else {
        None
    };
    if !a.all && a.paths.is_empty() && query.is_none() {
        bail!("say which assets: --all, paths, or --query/--from/--to/--camera/--type");
    }
    let mut n = 0;
    if a.all {
        n += render::enqueue_backlog(&mut cat)?;
    }
    if !a.paths.is_empty() || query.is_some() {
        let (ids, unmatched) = select::resolve(&cat, &e.lib, &a.paths, query.as_ref())?;
        if !unmatched.is_empty() {
            let list: Vec<String> = unmatched.iter().map(|p| p.display().to_string()).collect();
            bail!("not in the catalog: {}", list.join(", "));
        }
        if a.visible {
            n += render::set_visible(&mut cat, &e.cache, &ids)?;
        } else {
            let mut stale = Vec::new();
            for id in &ids {
                if render::needs_render(&mut cat, &e.cache, *id)? {
                    stale.push(*id);
                }
            }
            n += render::enqueue(&mut cat, &stale, Priority::Recent, true)?;
        }
    }
    ctx.emit(&json!({ "queued": n }), || {
        println!("Queued {n} job(s). Run `keepsake renders run` to make them.");
    })?;
    Ok(EXIT_OK)
}

fn show(ctx: &Ctx, file: &Path) -> Result<u8> {
    let e = env(ctx)?;
    let mut cat = e.lib.open_catalog()?;
    let f = e
        .lib
        .locate(&cat, file)?
        .with_context(|| format!("{} is not in the catalog", file.display()))?;
    let found = service::paths_of(&mut cat, &e.cache, f.asset_id)?;
    let map: BTreeMap<&str, String> = found.iter().map(|(v, p)| (v.name(), path_str(p))).collect();
    ctx.emit(&json!({ "asset": f.asset_id, "renditions": map }), || {
        if map.is_empty() {
            println!("No renditions yet. Run `keepsake renders run`.");
        }
        for (v, p) in &map {
            println!("{v}: {p}");
        }
    })?;
    Ok(EXIT_OK)
}

// ── bench ────────────────────────────────────────────────────────────────────

const PHOTO_TYPES: [(&str, &[&str]); 4] = [
    ("JPEG", &["jpg", "jpeg"]),
    ("ARW (embedded preview)", &["arw"]),
    ("PNG / TIFF", &["png", "tif", "tiff"]),
    ("HEIF", &["hif", "heif", "heic"]),
];
const VIDEO_EXTS: [&str; 5] = ["mp4", "mts", "m2ts", "mov", "avi"];

fn files_by_ext(root: &Path) -> BTreeMap<String, Vec<(u64, PathBuf)>> {
    let mut out: BTreeMap<String, Vec<(u64, PathBuf)>> = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if let Some(x) = p.extension() {
                let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                out.entry(x.to_string_lossy().to_ascii_lowercase())
                    .or_default()
                    .push((len, p));
            }
        }
    }
    for v in out.values_mut() {
        v.sort();
    }
    out
}

/// `n` files spread evenly across the size range (skipping tiny ones).
fn spread(mut files: Vec<(u64, PathBuf)>, n: usize, min_bytes: u64) -> Vec<PathBuf> {
    files.retain(|(l, _)| *l >= min_bytes);
    if files.len() <= n {
        return files.into_iter().map(|(_, p)| p).collect();
    }
    (0..n)
        .map(|i| files[i * (files.len() - 1) / (n - 1).max(1)].1.clone())
        .collect()
}

fn cpu_name() -> String {
    let out = if cfg!(windows) {
        std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_Processor).Name",
            ])
            .output()
    } else {
        std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
    };
    out.ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| std::env::consts::ARCH.to_owned())
}

struct Row {
    kind: String,
    items: u64,
    secs: f64,
    note: String,
}

fn bench(ctx: &Ctx, a: BenchArgs) -> Result<u8> {
    let tools = RenderTools::discover();
    let all = files_by_ext(&a.folder);
    let tmp = tempfile::tempdir()?;
    let media = tmp.path().join("media");
    std::fs::create_dir_all(&media)?;
    let mut groups: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for (name, exts) in PHOTO_TYPES {
        let files: Vec<(u64, PathBuf)> = exts
            .iter()
            .flat_map(|x| all.get(*x).cloned().unwrap_or_default())
            .collect();
        let pick = spread(files, a.per_type, 50 * 1024);
        if !pick.is_empty() {
            groups.push((name.to_owned(), pick));
        }
    }
    let vids: Vec<(u64, PathBuf)> = VIDEO_EXTS
        .iter()
        .flat_map(|x| all.get(*x).cloned().unwrap_or_default())
        .collect();
    // Videos: moderate sizes so the bench stays short.
    let vids: Vec<(u64, PathBuf)> = vids.into_iter().filter(|(l, _)| *l <= 600 << 20).collect();
    let vpick = spread(vids, a.videos, 100 * 1024);
    if !ctx.json {
        eprintln!("Copying the sample to {} …", tmp.path().display());
    }
    let mut copied: Vec<(String, Vec<PathBuf>)> = Vec::new();
    for (name, files) in groups
        .iter()
        .chain(std::iter::once(&("video".to_owned(), vpick.clone())))
    {
        let mut out = Vec::new();
        for (i, f) in files.iter().enumerate() {
            let to = media.join(format!(
                "{}_{i:03}_{}",
                name.split_whitespace().next().unwrap_or("x"),
                f.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            ));
            std::fs::copy(f, &to)?;
            out.push(to);
        }
        copied.push((name.clone(), out));
    }
    let lib = Library::create(&tmp.path().join("lib"))?;
    let mut cat = lib.open_catalog()?;
    keepsake_core::rebuild::add_volume(
        &mut cat,
        &lib,
        "Bench",
        VolumeKind::External,
        &media,
        false,
        None,
    )?;
    let cache = Cache::new(tmp.path().join("cache"));
    let workers = a
        .workers
        .unwrap_or_else(keepsake_core::settings::default_workers);
    let cancel = AtomicBool::new(false);
    let mut rows: Vec<Row> = Vec::new();
    let mut encoder: Option<String> = None;
    let mut total_footage_s = 0.0;
    let mut fail_notes: Vec<String> = Vec::new();

    let ids_of = |files: &[PathBuf]| -> Result<Vec<AssetId>> {
        let mut ids = Vec::new();
        for f in files {
            if let Some(file) = lib.locate(&cat, f)? {
                if file.role == keepsake_core::catalog::FileRole::Original
                    && !ids.contains(&file.asset_id)
                {
                    ids.push(file.asset_id);
                }
            }
        }
        Ok(ids)
    };
    let mut phases: Vec<(String, Vec<AssetId>, &str)> = Vec::new();
    for (name, files) in &copied {
        let ids = ids_of(files)?;
        if ids.is_empty() {
            continue;
        }
        if name == "video" {
            phases.push(("Video poster frame".into(), ids.clone(), JOB_STILLS));
            phases.push(("Video proxy (720p H.264)".into(), ids, JOB_PROXY));
        } else {
            phases.push((name.clone(), ids, JOB_STILLS));
        }
    }
    let mut cat = lib.open_catalog()?;
    for (name, ids, kind) in &phases {
        cat.write(|tx| {
            let jobs: Vec<_> = ids
                .iter()
                .map(|id| keepsake_core::catalog::NewJob {
                    kind,
                    asset_id: *id,
                    rank: 0,
                    sort_key: 0,
                })
                .collect();
            tx.jobs().enqueue(&jobs, Priority::Backlog, 0, false)
        })?;
        let opts = RenderOptions {
            workers: if *kind == JOB_STILLS { workers } else { 0 },
            proxy_workers: if *kind == JOB_PROXY { 1 } else { 0 },
            max_bytes: u64::MAX,
            pause_on_battery: false,
            until_empty: true,
            forced_encoder: a.encoder.clone(),
            evict_every: u64::MAX,
        };
        let svc = RenderService::new(&lib, tools.clone(), cache.clone(), opts);
        if !ctx.json {
            eprintln!("{name}: {} item(s)…", ids.len());
        }
        let t0 = Instant::now();
        let mut done = 0u64;
        svc.run(&cancel, &mut |ev| match ev {
            RenderEvent::Done {
                encoder: Some(e), ..
            } => {
                encoder = Some(e.clone());
                done += 1;
            }
            RenderEvent::Done { .. } => done += 1,
            RenderEvent::Failed { error, .. } => fail_notes.push(format!("{name}: {error}")),
            _ => {}
        })?;
        let secs = t0.elapsed().as_secs_f64();
        let mut note = String::new();
        if *kind == JOB_PROXY {
            let mut footage = 0.0;
            for id in ids {
                if let Some(a) = cat.read(|tx| tx.assets().get(*id))? {
                    if a.media_kind == MediaKind::Video {
                        let src = keepsake_core::render::source::locate(&cat, &lib, *id)?;
                        footage +=
                            keepsake_core::render::still::duration_s(&src, &tools).unwrap_or(0.0);
                    }
                }
            }
            total_footage_s = footage;
            if footage > 0.0 {
                note = format!(
                    "{:.1} min of footage; {:.1}x real time ({})",
                    footage / 60.0,
                    footage / secs,
                    encoder.as_deref().unwrap_or("?")
                );
            }
        } else if name.starts_with("ARW") {
            if let Some(id) = ids.first() {
                if let Some(p) = cache.lookup(&mut cat, *id, Variant::Preview2560)? {
                    if let Ok((w, h)) = image::image_dimensions(&p) {
                        note = format!("embedded preview {w}x{h}");
                    }
                }
            }
        }
        rows.push(Row {
            kind: name.clone(),
            items: done,
            secs,
            note,
        });
    }
    let cpu = cpu_name();
    let os = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let table: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            json!({ "type": r.kind, "items": r.items, "seconds": r.secs,
                    "items_per_minute": if r.secs > 0.0 { r.items as f64 * 60.0 / r.secs } else { 0.0 },
                    "note": r.note })
        })
        .collect();
    ctx.emit(
        &json!({ "machine": { "cpu": cpu, "os": os, "threads": threads }, "workers": workers,
                 "encoder": encoder, "footage_s": total_footage_s, "results": table, "failures": fail_notes }),
        || {
            println!("Machine: {cpu}, {threads} threads, {os}; {workers} image worker(s), 1 proxy worker");
            println!();
            println!("| Type | Items | Time (s) | Items/min | Notes |");
            println!("|---|---:|---:|---:|---|");
            for r in &rows {
                let per_min = if r.secs > 0.0 { r.items as f64 * 60.0 / r.secs } else { 0.0 };
                println!("| {} | {} | {:.1} | {:.0} | {} |", r.kind, r.items, r.secs, per_min, r.note);
            }
            for f in &fail_notes {
                println!("failure: {f}");
            }
        },
    )?;
    Ok(if fail_notes.is_empty() {
        EXIT_OK
    } else {
        EXIT_PROBLEMS
    })
}

//! `keepsake ai status | install | run | duplicates | retag | explain` (ADR-035)
//! and `keepsake ai faces install | scan | status | delete-all` (ADR-040)
//! and `keepsake ai aircraft install | status | scan | read` (ADR-041)

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
    /// Every tag's score for one picture file, best first (to check why a
    /// tag was or was not given).
    Explain {
        /// A JPEG, PNG or other picture the app can read.
        file: std::path::PathBuf,
        /// How many tags to list.
        #[arg(long, default_value_t = 15)]
        top: usize,
        /// Use the processor only, not the graphics card.
        #[arg(long)]
        cpu: bool,
    },
    /// Faces, on this computer (ADR-040).
    Faces {
        #[command(subcommand)]
        action: FacesAction,
    },
    /// Aircraft: the text painted on a plane and its type (ADR-041).
    Aircraft {
        #[command(subcommand)]
        action: AircraftAction,
    },
}

#[derive(Subcommand)]
pub enum AircraftAction {
    /// Download the two text models into this computer's cache folder
    /// (96 MB).
    Install,
    /// Which text models are installed, and whether they are the pinned
    /// ones.
    Status,
    /// Read the aircraft in the items with a tag (`--tagged Airplane`), or
    /// in every item, and add what is found to their keywords, as Read
    /// aircraft does in the app. Originals are only read.
    Scan {
        /// Only the items with this tag (default: every item).
        #[arg(long)]
        tagged: Option<String>,
        /// At most this many items.
        #[arg(long)]
        limit: Option<usize>,
        /// Use the processor only, not the graphics card.
        #[arg(long)]
        cpu: bool,
        /// Keep working on battery.
        #[arg(long)]
        ignore_battery: bool,
    },
    /// Read picture files, or the library's cached pictures of the items
    /// with a tag (`--tagged`), and print what was found (to check it).
    Read {
        /// JPEGs, PNGs or other pictures the app can read.
        files: Vec<std::path::PathBuf>,
        /// Read the items with this tag instead (their cached previews).
        #[arg(long)]
        tagged: Option<String>,
        /// With --tagged: at most this many items.
        #[arg(long, default_value_t = 20)]
        limit: usize,
        /// With --tagged: read the originals at full size, not the cache.
        #[arg(long)]
        full: bool,
        /// With --tagged: skip this many first.
        #[arg(long, default_value_t = 0)]
        skip: usize,
        /// Another text detector model file.
        #[arg(long)]
        detector: Option<std::path::PathBuf>,
        /// The detector sees the picture with its long side at most this.
        #[arg(long, default_value_t = keepsake_core::ai::aircraft::scan::DETECT_SIDE)]
        max_side: u32,
        /// Use the processor only, not the graphics card.
        #[arg(long)]
        cpu: bool,
    },
}

#[derive(Subcommand)]
pub enum FacesAction {
    /// Download the two face models into this computer's cache folder
    /// (39 MB).
    Install,
    /// Search every photo not yet searched for faces (from the thumbnails;
    /// makes missing ones first).
    Scan {
        /// Use the processor only, not the graphics card.
        #[arg(long)]
        cpu: bool,
        /// Keep working on battery.
        #[arg(long)]
        ignore_battery: bool,
    },
    /// What is installed and how many faces were found.
    Status,
    /// Forget every face, person and decision (originals are not touched).
    DeleteAll {
        /// Do it without asking.
        #[arg(long)]
        yes: bool,
    },
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
        AiAction::Explain { file, top, cpu } => explain(ctx, &file, top, !cpu),
        AiAction::Faces { action } => faces(ctx, action),
        AiAction::Aircraft { action } => aircraft(ctx, action),
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

fn explain(ctx: &Ctx, file: &std::path::Path, top: usize, gpu: bool) -> Result<u8> {
    let img = image::open(file).with_context(|| format!("reading {}", file.display()))?;
    let mut model = ai::ImageModel::load_installed(gpu)?;
    let emb = model.embed(&img)?;
    let labels = LabelSet::builtin()?;
    let given: Vec<String> = labels
        .tags_for(&emb, &[])
        .into_iter()
        .map(|(t, _)| t.to_string())
        .collect();
    let rows: Vec<(String, f32, f32)> = labels
        .labels()
        .iter()
        .map(|l| {
            let c = l.cosine(&emb);
            (l.name.clone(), c, ai::labels::probability(c))
        })
        .collect::<Vec<_>>();
    let mut rows = rows;
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    rows.truncate(top);
    ctx.emit(
        &json!({
            "file": path_str(file),
            "given": given,
            "scores": rows.iter().map(|(n, c, p)| json!({"tag": n, "cosine": c, "probability": p})).collect::<Vec<_>>(),
        }),
        || {
            println!("Tags given: {}", if given.is_empty() { "none".to_string() } else { given.join(", ") });
            for (n, c, p) in &rows {
                println!("{p:>8.4}  cos {c:>7.4}  {n}");
            }
        },
    )?;
    Ok(EXIT_OK)
}

fn faces(ctx: &Ctx, action: FacesAction) -> Result<u8> {
    use keepsake_core::ai::faces::{install as face_install, scan, MODEL_ID as FACE_MODEL};
    match action {
        FacesAction::Install => {
            let mut last = 0u64;
            match face_install::install(cancel_flag(), &mut |done, total| {
                if !ctx.json && done >= last + 5 * 1024 * 1024 {
                    last = done;
                    eprintln!("{} of {}", human_bytes(done), human_bytes(total));
                }
            }) {
                Ok(()) => {
                    ctx.emit(&json!({ "installed": true }), || {
                        println!("Installed the face models.")
                    })?;
                    Ok(EXIT_OK)
                }
                Err(ai::AiError::Cancelled) => Ok(EXIT_CANCELLED),
                Err(e) => Err(e.into()),
            }
        }
        FacesAction::Status => {
            let lib = ctx.library()?;
            let cat = lib.open_catalog()?;
            let c = scan::status(&cat)?;
            let det = face_install::DETECTOR.find();
            let rec = face_install::RECOGNIZER.find();
            ctx.emit(
                &json!({
                    "model_id": FACE_MODEL,
                    "detector": det.as_deref().map(path_str),
                    "recognizer": rec.as_deref().map(path_str),
                    "counts": c,
                }),
                || {
                    for (what, p) in [("Detector:  ", &det), ("Recogniser:", &rec)] {
                        match p {
                            Some(p) => println!("{what} {}", p.display()),
                            None => println!("{what} not installed (keepsake ai faces install)"),
                        }
                    }
                    println!(
                        "Searched {} of {} photo(s): {} face(s) in {} photo(s); {} people named; \
                         {} queued, {} failed.",
                        c.scanned, c.photos, c.faces, c.with_faces, c.people, c.pending, c.failed
                    );
                },
            )?;
            Ok(EXIT_OK)
        }
        FacesAction::DeleteAll { yes } => {
            if !yes {
                anyhow::bail!("this forgets every face, person and decision; run again with --yes");
            }
            let lib = ctx.library()?;
            let mut cat = lib.open_catalog()?;
            let n = scan::delete_all(&lib, &mut cat)?;
            ctx.emit(&json!({ "deleted_faces": n }), || {
                println!("Deleted all face data ({n} face(s)).")
            })?;
            Ok(EXIT_OK)
        }
        FacesAction::Scan {
            cpu,
            ignore_battery,
        } => {
            let lib = ctx.library()?;
            let settings = Settings::load()?;
            let cache = Cache::new(settings.render.cache_root()?);
            let queued = {
                let mut cat = lib.open_catalog()?;
                scan::enqueue_unscanned(&mut cat)?
            };
            if !ctx.json {
                eprintln!("{queued} photo(s) to search.");
            }
            let mut opts = RenderOptions::from_settings(&settings.render);
            opts.proxy_workers = 0;
            opts.faces = true;
            opts.ai_gpu = !cpu && settings.ai.gpu;
            opts.pause_on_battery = settings.render.pause_on_battery && !ignore_battery;
            let svc = RenderService::new(&lib, RenderTools::discover(), cache, opts)
                .with_power(Arc::new(keepsake_platform::on_battery));
            let cancel: &AtomicBool = cancel_flag();
            let mut searched = 0u64;
            let mut problem: Option<String> = None;
            svc.run(cancel, &mut |ev| match ev {
                RenderEvent::FacesReady { accel } => {
                    if !ctx.json {
                        eprintln!("Face models loaded on the {}.", accel.describe());
                    }
                }
                RenderEvent::FacesUnavailable { reason } => problem = Some(reason.clone()),
                RenderEvent::Done { kind, .. } if kind == ai::JOB_AI_FACES => {
                    searched += 1;
                    if !ctx.json && searched.is_multiple_of(100) {
                        eprintln!("{searched} searched…");
                    }
                }
                _ => {}
            })
            .context("running the face worker")?;
            if let Some(p) = problem {
                anyhow::bail!("the face models could not start: {p}");
            }
            let c = scan::status(&lib.open_catalog()?)?;
            ctx.emit(&json!({ "searched": searched, "counts": c }), || {
                println!(
                    "Searched {searched} photo(s); {} face(s) in {} photo(s) so far.",
                    c.faces, c.with_faces
                )
            })?;
            Ok(if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                EXIT_CANCELLED
            } else {
                EXIT_OK
            })
        }
    }
}

fn aircraft(ctx: &Ctx, action: AircraftAction) -> Result<u8> {
    use keepsake_core::ai::aircraft::{install as text_install, marks, ocr, types};
    use keepsake_core::catalog::TagPath;
    use keepsake_core::render::Variant;
    match action {
        AircraftAction::Install => {
            let mut last = 0u64;
            match text_install::install(cancel_flag(), &mut |done, total| {
                if !ctx.json && done >= last + 10 * 1024 * 1024 {
                    last = done;
                    eprintln!("{} of {}", human_bytes(done), human_bytes(total));
                }
            }) {
                Ok(()) => {
                    ctx.emit(&json!({ "installed": true }), || {
                        println!("Installed the text models.")
                    })?;
                    Ok(EXIT_OK)
                }
                Err(ai::AiError::Cancelled) => Ok(EXIT_CANCELLED),
                Err(e) => Err(e.into()),
            }
        }
        AircraftAction::Status => {
            let mut rows = Vec::new();
            for (what, m) in [
                ("Detector", text_install::DETECTOR),
                ("Recogniser", text_install::RECOGNIZER),
            ] {
                let found = m.find();
                let hash = match &found {
                    Some(p) => Some(install::file_blake3(p)?),
                    None => None,
                };
                rows.push(json!({
                    "model": what,
                    "file": found.as_deref().map(path_str),
                    "blake3": hash,
                    "pinned": hash.as_deref() == Some(m.blake3),
                }));
            }
            let settings = Settings::load()?;
            ctx.emit(
                &json!({
                    "enabled": settings.ai.aircraft,
                    "model_id": keepsake_core::ai::aircraft::MODEL_ID,
                    "models": rows,
                }),
                || {
                    println!(
                        "Read aircraft in the app: {}",
                        if settings.ai.aircraft { "on" } else { "off" }
                    );
                    for r in &rows {
                        let what = r["model"].as_str().unwrap_or("?");
                        match r["file"].as_str() {
                            Some(f) if r["pinned"].as_bool() == Some(true) => {
                                println!("{what}: {f}")
                            }
                            Some(f) => println!(
                                "{what}: {f} is not the pinned file (BLAKE3 {})",
                                r["blake3"].as_str().unwrap_or("?")
                            ),
                            None => {
                                println!("{what}: not installed (keepsake ai aircraft install)")
                            }
                        }
                    }
                },
            )?;
            Ok(EXIT_OK)
        }
        AircraftAction::Scan {
            tagged,
            limit,
            cpu,
            ignore_battery,
        } => {
            use keepsake_core::ai::aircraft::scan;
            let lib = ctx.library()?;
            let settings = Settings::load()?;
            let cache = Cache::new(settings.render.cache_root()?);
            let ids = {
                let mut cat = lib.open_catalog()?;
                let mut ids = match &tagged {
                    Some(tag) => {
                        let path = TagPath::parse_lightroom(tag)?;
                        cat.read(|tx| tx.tags().assets_with(&path))?
                    }
                    None => cat
                        .read(|tx| tx.ai().matching(&Default::default(), MODEL_ID))?
                        .into_iter()
                        .map(|it| it.id)
                        .collect(),
                };
                ids.truncate(limit.unwrap_or(usize::MAX));
                scan::enqueue(&mut cat, &ids)?;
                ids
            };
            if !ctx.json {
                eprintln!("{} item(s) to read.", ids.len());
            }
            let mut opts = RenderOptions::from_settings(&settings.render);
            opts.proxy_workers = 0;
            opts.aircraft = true;
            opts.ai_gpu = !cpu && settings.ai.gpu;
            opts.pause_on_battery = settings.render.pause_on_battery && !ignore_battery;
            let svc = RenderService::new(&lib, RenderTools::discover(), cache, opts)
                .with_power(Arc::new(keepsake_platform::on_battery));
            let cancel: &AtomicBool = cancel_flag();
            let mut rows = Vec::new();
            let mut problem: Option<String> = None;
            svc.run(cancel, &mut |ev| match ev {
                RenderEvent::AircraftReady { accel } => {
                    if !ctx.json {
                        eprintln!("Text models loaded on the {}.", accel.describe());
                    }
                }
                RenderEvent::AircraftUnavailable { reason } => problem = Some(reason.clone()),
                RenderEvent::Done {
                    kind,
                    asset,
                    made,
                    ms,
                    ..
                } if kind == ai::JOB_AI_AIRCRAFT => {
                    if !ctx.json {
                        eprintln!("{asset}  {ms} ms  {}", made.join(", "));
                    }
                    rows.push(json!({ "asset": asset, "ms": ms, "added": made }));
                }
                _ => {}
            })
            .context("running the aircraft worker")?;
            if let Some(p) = problem {
                anyhow::bail!("the text models could not start: {p}");
            }
            let cat = lib.open_catalog()?;
            let (st, why) = scan::status(&cat, &ids)?;
            ctx.emit(
                &json!({
                    "items": st.items, "read": st.done, "pending": st.pending,
                    "failed": st.failed, "problem": why, "results": rows,
                }),
                || {
                    println!(
                        "Read {} of {} item(s); {} waiting, {} failed.",
                        st.done, st.items, st.pending, st.failed
                    );
                    if let Some(w) = &why {
                        println!("{w}");
                    }
                },
            )?;
            Ok(if cancel.load(std::sync::atomic::Ordering::SeqCst) {
                EXIT_CANCELLED
            } else {
                EXIT_OK
            })
        }
        AircraftAction::Read {
            files,
            tagged,
            limit,
            full,
            skip,
            detector,
            max_side,
            cpu,
        } => {
            let mut pictures: Vec<(String, std::path::PathBuf)> =
                files.iter().map(|f| (path_str(f), f.clone())).collect();
            if let Some(tag) = &tagged {
                let lib = ctx.library()?;
                let mut cat = lib.open_catalog()?;
                let cache = Cache::new(Settings::load()?.render.cache_root()?);
                let path = TagPath::parse_lightroom(tag)?;
                let ids = cat.read(|tx| tx.tags().assets_with(&path))?;
                for id in ids.into_iter().skip(skip) {
                    if pictures.len() >= limit {
                        break;
                    }
                    let found = if full {
                        keepsake_core::render::source::locate(&cat, &lib, id)
                            .ok()
                            .map(|s| s.still)
                    } else {
                        match cache.lookup(&mut cat, id, Variant::Preview2560)? {
                            Some(p) => Some(p),
                            None => cache.lookup(&mut cat, id, Variant::Thumb1024)?,
                        }
                    };
                    if let Some(p) = found {
                        let name = describe(&cat, &lib, id)
                            .ok()
                            .and_then(|r| r.file)
                            .unwrap_or_else(|| id.to_string());
                        pictures.push((name, p));
                    }
                }
            }
            let det = match detector {
                Some(d) => d,
                None => text_install::DETECTOR.require()?,
            };
            let runtime = install::find_runtime().context("ONNX Runtime not found")?;
            let mut reader = ocr::TextReader::load(
                &det,
                &text_install::RECOGNIZER.require()?,
                &runtime,
                !cpu,
                max_side,
            )?;
            if !ctx.json {
                eprintln!("Text models loaded on the {}.", reader.accel().describe());
            }
            let tools = RenderTools::discover();
            let airlines = marks::Airlines::builtin();
            let types = types::TypeList::builtin()?;
            // The look is only compared when the image encoder is installed.
            let mut image_model = ai::ImageModel::load_installed(!cpu).ok();
            let mut rows = Vec::new();
            for (name, file) in &pictures {
                let t_load = std::time::Instant::now();
                let img = keepsake_core::render::full::decode_full(&tools, file)
                    .with_context(|| format!("reading {}", file.display()))?;
                let load_ms = t_load.elapsed().as_millis() as u64;
                let t0 = std::time::Instant::now();
                let lines = reader.read(&img)?;
                let ms = t0.elapsed().as_millis() as u64;
                let marks = marks::interpret(&lines, &airlines);
                let looks: Vec<(String, f32)> = match image_model.as_mut() {
                    Some(m) => types
                        .ranked(&m.embed(&img)?)
                        .into_iter()
                        .take(3)
                        .map(|(n, p)| (n.to_owned(), p))
                        .collect(),
                    None => Vec::new(),
                };
                rows.push(json!({
                    "file": name,
                    "picture": path_str(file),
                    "width": img.width(),
                    "height": img.height(),
                    "ms": ms,
                    "load_ms": load_ms,
                    "registrations": marks.registrations,
                    "airlines": marks.airlines,
                    "types": marks.types,
                    "looks_like": looks.iter().map(|(n, p)| json!({"type": n, "probability": p})).collect::<Vec<_>>(),
                    "lines": lines.iter().map(|l| json!({
                        "text": l.text,
                        "score": l.score,
                        "height": l.height(),
                    })).collect::<Vec<_>>(),
                }));
            }
            ctx.emit(&json!({ "pictures": rows }), || {
                for r in &rows {
                    println!(
                        "{}  ({}x{}, {} ms)\n  {}",
                        r["file"].as_str().unwrap_or("?"),
                        r["width"],
                        r["height"],
                        r["ms"],
                        r["picture"].as_str().unwrap_or("?")
                    );
                    for key in ["registrations", "airlines", "types"] {
                        let v: Vec<&str> = r[key]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|x| x.as_str())
                            .collect();
                        if !v.is_empty() {
                            println!("  {key}: {}", v.join(", "));
                        }
                    }
                    let looks: Vec<String> = r["looks_like"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|x| {
                            format!(
                                "{} {:.3}",
                                x["type"].as_str().unwrap_or("?"),
                                x["probability"].as_f64().unwrap_or(0.0)
                            )
                        })
                        .collect();
                    if !looks.is_empty() {
                        println!("  looks like: {}", looks.join("; "));
                    }
                    for l in r["lines"].as_array().into_iter().flatten() {
                        println!(
                            "    {:>5.2}  {:>4.0} px  {}",
                            l["score"].as_f64().unwrap_or(0.0),
                            l["height"].as_f64().unwrap_or(0.0),
                            l["text"].as_str().unwrap_or("")
                        );
                    }
                }
            })?;
            Ok(EXIT_OK)
        }
    }
}

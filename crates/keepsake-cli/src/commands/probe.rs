//! `keepsake probe <folder>` — show what Keepsake reads from a card.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Args;

use keepsake_core::probe::{Prober, ToolPaths};
use keepsake_core::rebuild::{group_folder, list_media_files};
use keepsake_core::xmp::format_date;

#[derive(Args)]
pub struct ProbeArgs {
    /// Folder to probe (a card root or a copy of one).
    pub folder: PathBuf,
}

pub fn run(ctx: &super::common::Ctx, args: ProbeArgs) -> Result<u8> {
    let root = std::fs::canonicalize(&args.folder)
        .with_context(|| format!("{} not found", args.folder.display()))?;
    let prober = Prober::new(&ToolPaths::discover(), Default::default())
        .context("ExifTool/ffprobe not available (run scripts/setup.ps1 or setup.sh)")?;
    let (files, walk) = list_media_files(&root);
    let grouping = group_folder(&root, &files);
    let report = prober.probe(&root, &files, &grouping);

    if ctx.json {
        let out = serde_json::json!({
            "groups": grouping.groups.iter().zip(&report.assets).map(|(g, a)| serde_json::json!({
                "kind": g.kind,
                "files": g.members.iter().map(|m| serde_json::json!({
                    "path": files[m.index].as_str(), "role": m.role, "segment": m.segment,
                })).collect::<Vec<_>>(),
                "probe": a,
                "capture_ts": a.capture.as_ref().map(|c| format_date(c.time)),
            })).collect::<Vec<_>>(),
            "sequences": report.sequences.iter().map(|s| serde_json::json!({
                "kind": s.kind,
                "members": s.members.iter().map(|gi| files[grouping.groups[*gi].original()].as_str()).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "card_files": grouping.card_files.iter().map(|i| files[*i].as_str()).collect::<Vec<_>>(),
            "ungrouped": grouping.ungrouped.iter().map(|i| files[*i].as_str()).collect::<Vec<_>>(),
            "problems": grouping.problems.iter().chain(&walk.problems).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(0);
    }

    println!(
        "{} assets, {} card files, {} other files\n",
        grouping.groups.len(),
        grouping.card_files.len(),
        grouping.ungrouped.len()
    );
    for (g, a) in grouping.groups.iter().zip(&report.assets) {
        let orig = files[g.original()].as_str();
        let (when, how) = match &a.capture {
            Some(c) => (
                format_date(c.time),
                format!("{} / {}", c.source, c.confidence),
            ),
            None => ("-".into(), "no date".into()),
        };
        println!("{orig}");
        println!(
            "    {when}  [{how}]  {}  {}",
            a.model.as_deref().unwrap_or("-"),
            a.lens.as_deref().unwrap_or("")
        );
        if let Some(d) = a.duration_ms {
            let v = a.video.as_ref();
            println!(
                "    {:.3} s  {}  {}x{}  {}-bit {}  rot {}  tc {}",
                d as f64 / 1000.0,
                v.map(|v| v.codec.as_str()).unwrap_or("-"),
                v.map(|v| v.width).unwrap_or(0),
                v.map(|v| v.height).unwrap_or(0),
                v.and_then(|v| v.bit_depth)
                    .map(|b| b.to_string())
                    .unwrap_or("-".into()),
                v.and_then(|v| v.chroma)
                    .map(|c| format!("{c:?}"))
                    .unwrap_or_default(),
                v.map(|v| v.rotation_cw).unwrap_or(0),
                a.timecode.as_deref().unwrap_or("-"),
            );
        }
        for m in &g.members[1..] {
            println!("    + {} ({})", files[m.index].as_str(), m.role);
        }
        if let Some(c) = &a.capture {
            for n in &c.notes {
                println!("    note: {n}");
            }
        }
        for p in &a.problems {
            println!("    problem: {p}");
        }
    }
    if !report.sequences.is_empty() {
        println!();
        for s in &report.sequences {
            let first = files[grouping.groups[s.members[0]].original()].as_str();
            println!("{} of {} starting at {first}", s.kind, s.members.len());
        }
    }
    for p in grouping.problems.iter().chain(&walk.problems) {
        println!("problem: {p}");
    }
    Ok(0)
}

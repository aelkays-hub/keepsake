//! `keepsake search "<query>" [--from] [--to] [--camera] [--type]`

use anyhow::{bail, Result};
use clap::Args;

use keepsake_core::catalog::{MediaKind, SearchQuery};

use super::common::{day, describe, Ctx, EXIT_OK};

/// Filters shared by `search` and `shift-time`.
#[derive(Args, Clone, Default)]
pub struct Filters {
    /// First capture day (local time), YYYY-MM-DD, inclusive.
    #[arg(long)]
    pub from: Option<String>,
    /// Last capture day (local time), YYYY-MM-DD, inclusive.
    #[arg(long)]
    pub to: Option<String>,
    /// Camera model contains this text (case-insensitive).
    #[arg(long)]
    pub camera: Option<String>,
    /// photo or video.
    #[arg(long = "type", value_name = "photo|video")]
    pub kind: Option<String>,
}

impl Filters {
    pub fn is_empty(&self) -> bool {
        self.from.is_none() && self.to.is_none() && self.camera.is_none() && self.kind.is_none()
    }

    pub fn to_query(&self, text: Option<String>, limit: Option<u32>) -> Result<SearchQuery> {
        let kind = match self.kind.as_deref() {
            None => None,
            Some(k) => match MediaKind::parse(&k.to_ascii_lowercase()) {
                Some(k) => Some(k),
                None => bail!("--type must be photo or video, not {k}"),
            },
        };
        Ok(SearchQuery {
            text,
            from: self.from.as_deref().map(day).transpose()?,
            to: self.to.as_deref().map(day).transpose()?,
            camera: self.camera.clone(),
            kind,
            limit,
            ..Default::default()
        })
    }
}

#[derive(Args)]
pub struct SearchArgs {
    /// Words to find in file names, comments and tags (may be empty).
    pub query: Option<String>,
    #[command(flatten)]
    pub filters: Filters,
    /// Show at most this many results.
    #[arg(long, default_value_t = 100)]
    pub limit: u32,
}

pub fn run(ctx: &Ctx, a: SearchArgs) -> Result<u8> {
    if a.query.as_deref().is_none_or(|q| q.trim().is_empty()) && a.filters.is_empty() {
        bail!("give search words or at least one filter (--from, --to, --camera, --type)");
    }
    let lib = ctx.library()?;
    let cat = lib.open_catalog()?;
    let q = a.filters.to_query(a.query.clone(), Some(a.limit))?;
    let ids = cat.read(|tx| tx.search().query(&q))?;
    let rows = ids
        .iter()
        .map(|id| describe(&cat, &lib, *id))
        .collect::<Result<Vec<_>>>()?;
    ctx.emit(
        &serde_json::json!({ "query": q, "count": rows.len(), "results": rows }),
        || {
            if rows.is_empty() {
                println!("No matches.");
                return;
            }
            println!("{} match(es)", rows.len());
            for r in &rows {
                println!(
                    "  {}  {}  {}  {}{}",
                    r.captured.as_deref().unwrap_or("(no date)"),
                    r.kind,
                    r.camera.as_deref().unwrap_or("-"),
                    r.file.as_deref().unwrap_or("?"),
                    if r.rating > 0 {
                        format!("  {}★", r.rating)
                    } else {
                        String::new()
                    }
                );
            }
        },
    )?;
    Ok(EXIT_OK)
}

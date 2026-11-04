//! Shared CLI plumbing: which library, saved config, output, exit codes,
//! progress bars and Ctrl-C.

use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::OnceLock;

use anyhow::{bail, Context, Result};
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use serde::Serialize;

use keepsake_core::catalog::{AssetId, Catalog};
use keepsake_core::library::Library;
use keepsake_core::probe::{Prober, ToolPaths};
use keepsake_core::xmp::format_date;

/// Exit codes (see docs/CLI.md).
pub const EXIT_OK: u8 = 0;
pub const EXIT_PROBLEMS: u8 = 2;
pub const EXIT_CANCELLED: u8 = 130;

/// Options every command receives.
pub struct Ctx {
    pub json: bool,
    pub library: Option<PathBuf>,
}

impl Ctx {
    /// Print a JSON document (with `--json`) or a human text (without).
    pub fn emit(&self, json: &serde_json::Value, human: impl FnOnce()) -> Result<()> {
        if self.json {
            println!("{}", serde_json::to_string_pretty(json)?);
        } else {
            human();
        }
        Ok(())
    }

    /// The library to use: `--library`, `KEEPSAKE_LIBRARY`, or the default set by `init`.
    pub fn library(&self) -> Result<Library> {
        let path = match &self.library {
            Some(p) => p.clone(),
            None => match std::env::var_os("KEEPSAKE_LIBRARY") {
                Some(p) => PathBuf::from(p),
                None => Config::load()?
                    .default_library
                    .context("no library: run `keepsake init <path>` or pass --library <path>")?,
            },
        };
        Library::open(&path)
            .with_context(|| format!("{} is not a Keepsake library", path.display()))
    }
}

/// `config.toml` (shared with the app): `keepsake_core::settings`.
pub type Config = keepsake_core::settings::Settings;

/// Start ExifTool/ffprobe unless disabled; note it when they are missing.
pub fn maybe_prober(disabled: bool, json: bool) -> Option<Prober> {
    if disabled {
        return None;
    }
    match Prober::new(&ToolPaths::discover(), Default::default()) {
        Ok(p) => Some(p),
        Err(e) => {
            if !json {
                eprintln!("note: not reading media metadata ({e}); dates may be missing");
            }
            None
        }
    }
}

static CANCEL: OnceLock<&'static AtomicBool> = OnceLock::new();
static PRESSES: AtomicU8 = AtomicU8::new(0);

/// Ctrl-C (or Ctrl-Break on Windows): the first press asks the running
/// command to finish the current file and stop; the second exits at once.
pub fn cancel_flag() -> &'static AtomicBool {
    CANCEL.get_or_init(|| {
        let flag: &'static AtomicBool = Box::leak(Box::new(AtomicBool::new(false)));
        let _ = ctrlc::set_handler(move || {
            if PRESSES.fetch_add(1, Ordering::SeqCst) == 0 {
                flag.store(true, Ordering::SeqCst);
                eprintln!("\nStopping after the current file… (press Ctrl-C again to stop now)");
            } else {
                eprintln!("\nStopped immediately. Run the command again (or `ingest --resume`) to continue.");
                std::process::exit(i32::from(EXIT_CANCELLED));
            }
        });
        flag
    })
}

/// A bytes progress bar on stderr, drawn only on a terminal and never with `--json`.
pub fn bytes_bar(json: bool, total: u64, what: &str) -> ProgressBar {
    if json || !std::io::stderr().is_terminal() {
        return ProgressBar::hidden();
    }
    let pb = ProgressBar::with_draw_target(Some(total), ProgressDrawTarget::stderr());
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner} {prefix} [{bar:30}] {bytes}/{total_bytes} {binary_bytes_per_sec} eta {eta} {wide_msg}",
        )
        .unwrap_or_else(|_| ProgressStyle::default_bar())
        .progress_chars("=> "),
    );
    pb.set_prefix(what.to_owned());
    pb
}

/// A short description of an asset for listings.
#[derive(Debug, Serialize)]
pub struct AssetRow {
    pub asset: AssetId,
    pub file: Option<String>,
    pub volume: Option<String>,
    pub kind: String,
    pub captured: Option<String>,
    pub camera: Option<String>,
    pub rating: u8,
    pub label: Option<String>,
}

pub fn describe(cat: &Catalog, lib: &Library, id: AssetId) -> Result<AssetRow> {
    let a = cat.read(|tx| tx.assets().require(id))?;
    let files = cat.read(|tx| tx.files().originals(id))?;
    // Prefer the first original on a mounted volume.
    let mut best: Option<(String, String)> = None;
    for f in &files {
        let label = cat
            .read(|tx| tx.volumes().get(f.volume_id))?
            .map(|v| v.label)
            .unwrap_or_default();
        match keepsake_core::verify::mount_of(cat, lib, f.volume_id) {
            Some(m) => {
                best = Some((path_str(&f.rel_path.to_fs_path(&m)), label));
                break;
            }
            None if best.is_none() => best = Some((f.rel_path.to_string(), label)),
            None => {}
        }
    }
    Ok(AssetRow {
        asset: id,
        file: best.as_ref().map(|b| b.0.clone()),
        volume: best.map(|b| b.1),
        kind: a.media_kind.as_str().to_owned(),
        captured: a.capture.map(format_date),
        camera: a.camera_model,
        rating: a.rating.get(),
        label: a.label,
    })
}

/// Parse a `YYYY-MM-DD` day.
pub fn day(s: &str) -> Result<String> {
    match chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        Ok(_) => Ok(s.to_owned()),
        Err(_) => bail!("not a date (YYYY-MM-DD): {s}"),
    }
}

pub fn human_bytes(n: u64) -> String {
    let n = n as f64;
    if n >= 1e9 {
        format!("{:.2} GB", n / 1e9)
    } else if n >= 1e6 {
        format!("{:.1} MB", n / 1e6)
    } else if n >= 1e3 {
        format!("{:.0} KB", n / 1e3)
    } else {
        format!("{n} B")
    }
}

/// A path for people: without Windows' verbatim `\\?\` prefix.
pub fn path_str(p: &Path) -> String {
    plain(p).display().to_string()
}

/// Remove Windows' verbatim prefix (`\\?\C:\x` becomes `C:\x`, `\\?\UNC\srv\x` becomes `\\srv\x`).
pub fn plain(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        p.to_path_buf()
    }
}

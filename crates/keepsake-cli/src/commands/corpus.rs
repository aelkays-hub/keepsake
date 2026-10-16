use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Subcommand;

use keepsake_core::corpus::{
    grouper, scanner,
    types::{FileKind, FolderRole, Manifest},
};

#[derive(Subcommand)]
pub enum CorpusAction {
    /// Walk `<path>/cards/<camera>/` and write corpus/manifest.json.
    ///
    /// `<path>` defaults to the `KEEPSAKE_CORPUS` environment variable.
    /// No media bytes are written — only paths, sizes, BLAKE3 hashes,
    /// detected types, and folder roles.
    Scan {
        /// Corpus root directory (overrides KEEPSAKE_CORPUS).
        path: Option<PathBuf>,

        /// Output path for manifest.json.
        #[arg(long, default_value = "corpus/manifest.json")]
        output: PathBuf,

        /// Print one summary line per camera with per-type file counts, then
        /// compare against the raw filesystem count and report any unaccounted
        /// paths or intentionally skipped (hidden/system) files.
        #[arg(long)]
        summary: bool,
    },

    /// Generate corpus/expected.toml from an existing manifest.
    ///
    /// Groups files into logical assets and writes placeholder metadata
    /// (`capture_ts`, `camera_model`, `duration_ms`) for you to fill in.
    /// Set `confirmed = true` after spot-checking an asset against the
    /// camera's own playback.
    InitExpected {
        /// Path to manifest.json.
        #[arg(long, default_value = "corpus/manifest.json")]
        manifest: PathBuf,

        /// Output path for expected.toml.
        #[arg(long, default_value = "corpus/expected.toml")]
        output: PathBuf,

        /// Overwrite an existing expected.toml (preserves nothing).
        #[arg(long)]
        force: bool,
    },
}

pub fn run(action: CorpusAction) -> Result<()> {
    match action {
        CorpusAction::Scan {
            path,
            output,
            summary,
        } => run_scan(path, output, summary),
        CorpusAction::InitExpected {
            manifest,
            output,
            force,
        } => run_init_expected(manifest, output, force),
    }
}

fn run_scan(path: Option<PathBuf>, output: PathBuf, summary: bool) -> Result<()> {
    let root = match path {
        Some(p) => p,
        None => std::env::var("KEEPSAKE_CORPUS")
            .map(PathBuf::from)
            .context("pass a path or set KEEPSAKE_CORPUS")?,
    };

    println!("Scanning {}…", root.display());
    let manifest = scanner::scan(&root)?;

    let total: usize = manifest.cameras.iter().map(|c| c.files.len()).sum();
    println!(
        "  {} file(s) across {} camera card(s)",
        total,
        manifest.cameras.len()
    );

    if summary {
        let cards_dir = root.join("cards");
        for cam in &manifest.cameras {
            print_camera_summary(cam, &cards_dir.join(&cam.name))?;
        }
    } else {
        for cam in &manifest.cameras {
            println!("  {} — {} file(s)", cam.name, cam.files.len());
        }
    }

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let json = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(&output, json)?;
    println!("Written {}", output.display());

    Ok(())
}

fn print_camera_summary(
    cam: &keepsake_core::corpus::types::CameraCard,
    card_root: &Path,
) -> Result<()> {
    // Count by logical type.
    let mut jpeg = 0usize;
    let mut arw = 0usize;
    let mut hif = 0usize;
    let mut mp4 = 0usize;
    let mut xml = 0usize;
    let mut mts = 0usize;
    let mut proxy = 0usize;
    let mut other = 0usize;

    for f in &cam.files {
        match (&f.kind, &f.folder_role) {
            (FileKind::Mp4, FolderRole::Proxy) => proxy += 1,
            (FileKind::Mp4, _) => mp4 += 1,
            (FileKind::Jpg, _) => jpeg += 1,
            (FileKind::Arw, _) => arw += 1,
            (FileKind::Hif, _) => hif += 1,
            (FileKind::Xml, _) => xml += 1,
            (FileKind::Mts, _) => mts += 1,
            _ => other += 1,
        }
    }

    println!(
        "  {:<20}  {:>4} files  jpeg={} arw={} hif={} mp4={} xml={} mts={} proxy={} other={}",
        cam.name,
        cam.files.len(),
        jpeg,
        arw,
        hif,
        mp4,
        xml,
        mts,
        proxy,
        other,
    );

    // Mismatch check: count every file on disk (hidden/system included).
    let mut fs_paths: Vec<String> = Vec::new();
    if card_root.is_dir() {
        collect_all_fs_paths(card_root, card_root, &mut fs_paths)?;
        fs_paths.sort();
    }

    let scanned: HashSet<&str> = cam.files.iter().map(|f| f.rel_path.as_str()).collect();
    let skipped_set: HashSet<&str> = cam.skipped.iter().map(|f| f.rel_path.as_str()).collect();

    let unaccounted: Vec<&str> = fs_paths
        .iter()
        .map(String::as_str)
        .filter(|p| !scanned.contains(p) && !skipped_set.contains(p))
        .collect();

    if !unaccounted.is_empty() {
        println!(
            "    [MISMATCH] {} file(s) on disk not accounted for:",
            unaccounted.len()
        );
        for p in &unaccounted {
            println!("      {p}");
        }
    }

    if !cam.skipped.is_empty() {
        println!("    [SKIPPED] {} file(s):", cam.skipped.len());
        for s in &cam.skipped {
            println!("      {}  ({})", s.rel_path, s.reason);
        }
    }

    Ok(())
}

/// Recursively collect every file path under `dir`, relative to `root`,
/// including hidden and system files.
fn collect_all_fs_paths(dir: &Path, root: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_all_fs_paths(&path, root, out)?;
        } else if path.is_file() {
            let rel = path.strip_prefix(root).expect("path under root");
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

fn run_init_expected(manifest_path: PathBuf, output: PathBuf, force: bool) -> Result<()> {
    if output.exists() && !force {
        anyhow::bail!(
            "{} already exists — pass --force to overwrite",
            output.display()
        );
    }

    let data = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: Manifest = serde_json::from_str(&data)
        .with_context(|| format!("parsing {}", manifest_path.display()))?;

    let mut all_assets = Vec::new();
    for camera in &manifest.cameras {
        let expected = grouper::group(&camera.name, &camera.files);
        all_assets.extend(expected.asset);
    }
    all_assets.sort_by(|a, b| {
        a.camera
            .cmp(&b.camera)
            .then(a.files.first().cmp(&b.files.first()))
    });

    let expected = keepsake_core::corpus::types::Expected { asset: all_assets };

    let header = "# Generated by `keepsake corpus init-expected`.\n\
                  # Fill in capture_ts, camera_model, duration_ms for each asset.\n\
                  # Set confirmed = true after spot-checking against camera playback.\n\n";

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let body = toml::to_string_pretty(&expected).context("serialising expected.toml")?;
    std::fs::write(&output, format!("{header}{body}"))?;
    println!(
        "Written {} ({} asset(s))",
        output.display(),
        expected.asset.len()
    );

    Ok(())
}

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Subcommand;

use keepsake_core::corpus::{
    grouper, scanner,
    scanner::detect_role,
    types::{AssetGroup, Expected, FileKind, FolderRole, Manifest},
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
    ///
    /// With --force: merges with an existing expected.toml, preserving every
    /// entry whose `confirmed = true`. Entries that are no longer in the
    /// manifest are kept with `removed = true` so you can review them.
    /// Fails loudly if the manifest contains zero entries.
    InitExpected {
        /// Path to manifest.json.
        #[arg(long, default_value = "corpus/manifest.json")]
        manifest: PathBuf,

        /// Output path for expected.toml.
        #[arg(long, default_value = "corpus/expected.toml")]
        output: PathBuf,

        /// Regenerate, merging with the existing file. Confirmed entries are
        /// never overwritten; removed entries are flagged with `removed = true`.
        #[arg(long)]
        force: bool,

        /// Pre-fill capture_ts, camera_model and duration_ms of unconfirmed
        /// entries by probing the corpus with ExifTool/ffprobe. Entries are
        /// marked `prefilled = true`: check them against the camera before
        /// setting `confirmed = true`.
        #[arg(long)]
        probe: bool,

        /// Corpus root for --probe (defaults to KEEPSAKE_CORPUS).
        #[arg(long)]
        corpus: Option<PathBuf>,
    },

    /// Print a sample of assets for manual spot-checking.
    ///
    /// Shows file path, camera, type, and expected capture time from
    /// corpus/expected.toml (if available). Use this to verify a representative
    /// slice of your corpus against the camera's own playback display.
    Sample {
        /// Number of assets to sample.
        #[arg(short = 'n', long, default_value = "10")]
        n: usize,

        /// Spread the sample across cameras and file types (round-robin).
        #[arg(long)]
        mixed: bool,

        /// Path to manifest.json.
        #[arg(long, default_value = "corpus/manifest.json")]
        manifest: PathBuf,

        /// Path to expected.toml (optional; used for capture_ts column).
        #[arg(long, default_value = "corpus/expected.toml")]
        expected: PathBuf,
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
            probe,
            corpus,
        } => run_init_expected(manifest, output, force, probe, corpus),
        CorpusAction::Sample {
            n,
            mixed,
            manifest,
            expected,
        } => run_sample(n, mixed, manifest, expected),
    }
}

// ── scan ─────────────────────────────────────────────────────────────────────

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

// ── init-expected ─────────────────────────────────────────────────────────────

fn run_init_expected(
    manifest_path: PathBuf,
    output: PathBuf,
    force: bool,
    probe: bool,
    corpus: Option<PathBuf>,
) -> Result<()> {
    if output.exists() && !force {
        anyhow::bail!(
            "{} already exists — pass --force to regenerate (confirmed entries are preserved)",
            output.display()
        );
    }

    let data = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: Manifest = serde_json::from_str(&data)
        .with_context(|| format!("parsing {}", manifest_path.display()))?;

    let corpus_root = match (&corpus, probe) {
        (Some(c), _) => Some(c.clone()),
        (None, true) => Some(
            std::env::var("KEEPSAKE_CORPUS")
                .map(PathBuf::from)
                .context("--probe needs --corpus or KEEPSAKE_CORPUS")?,
        ),
        (None, false) => None,
    };

    // Generate fresh groups from the manifest (reading AVCHD playlists when
    // the corpus is available).
    let mut fresh: Vec<AssetGroup> = Vec::new();
    for camera in &manifest.cameras {
        let expected = match &corpus_root {
            Some(root) => grouper::group_on_disk(
                &camera.name,
                &camera.files,
                &root.join("cards").join(&camera.name),
            ),
            None => grouper::group(&camera.name, &camera.files),
        };
        fresh.extend(expected.asset);
    }
    fresh.sort_by(|a, b| {
        a.camera
            .cmp(&b.camera)
            .then(a.files.first().cmp(&b.files.first()))
    });

    if fresh.is_empty() {
        anyhow::bail!(
            "manifest at {} has zero entries — run `keepsake corpus scan` first",
            manifest_path.display()
        );
    }

    // If an existing file is present (only reachable with --force), merge.
    let merged = if output.exists() {
        merge_with_existing(fresh, &output)?
    } else {
        MergeResult {
            assets: fresh,
            preserved: 0,
            flagged_removed: 0,
            added: 0,
        }
    };

    let header = "# Generated by `keepsake corpus init-expected`.\n\
                  # Fill in capture_ts, camera_model, duration_ms for each asset\n\
                  # (`--probe` pre-fills them and sets prefilled = true).\n\
                  # Set confirmed = true after spot-checking against camera playback.\n\
                  # Entries with removed = true are no longer in the manifest; review and delete them.\n\n";

    if let Some(parent) = output.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut assets = merged.assets;
    if probe {
        let root = corpus_root.as_deref().context("corpus root")?;
        let tools = keepsake_core::probe::ToolPaths::discover();
        let prober = keepsake_core::probe::Prober::new(&tools, Default::default())
            .context("ExifTool/ffprobe not available (run scripts/setup)")?;
        let mut filled = 0;
        for camera in &manifest.cameras {
            let probed = keepsake_core::corpus::probed::probe_card(
                &root.join("cards").join(&camera.name),
                &camera.files,
                &prober,
            );
            let mut cam: Vec<AssetGroup> = assets
                .iter()
                .filter(|a| a.camera == camera.name)
                .cloned()
                .collect();
            filled += keepsake_core::corpus::probed::prefill(&mut cam, &probed);
            let mut it = cam.into_iter();
            for a in assets.iter_mut().filter(|a| a.camera == camera.name) {
                if let Some(n) = it.next() {
                    *a = n;
                }
            }
        }
        println!(
            "  pre-filled {filled} unconfirmed entries from probe; review them, then set confirmed = true"
        );
    }
    let expected = Expected { asset: assets };
    let body = toml::to_string_pretty(&expected).context("serialising expected.toml")?;
    std::fs::write(&output, format!("{header}{body}"))?;

    println!(
        "Written {} ({} asset(s))",
        output.display(),
        expected.asset.len()
    );
    if merged.preserved > 0 || merged.flagged_removed > 0 {
        println!(
            "  preserved {} confirmed  |  {} flagged removed  |  {} new",
            merged.preserved, merged.flagged_removed, merged.added
        );
    }

    Ok(())
}

struct MergeResult {
    assets: Vec<AssetGroup>,
    preserved: usize,
    flagged_removed: usize,
    added: usize,
}

fn merge_with_existing(fresh: Vec<AssetGroup>, existing_path: &Path) -> Result<MergeResult> {
    let raw = std::fs::read_to_string(existing_path)
        .with_context(|| format!("reading {}", existing_path.display()))?;
    // Tolerate a header-only placeholder (missing `asset` key → empty list).
    let existing: Expected = toml::from_str(&raw).unwrap_or(Expected { asset: Vec::new() });

    // Key each group by its sorted file set.
    let asset_key = |a: &AssetGroup| -> Vec<String> {
        let mut k = a.files.clone();
        k.sort();
        k
    };

    // Build lookup of existing groups by key.
    let existing_map: HashMap<Vec<String>, AssetGroup> = existing
        .asset
        .into_iter()
        .map(|a| (asset_key(&a), a))
        .collect();

    let fresh_keys: HashSet<Vec<String>> = fresh.iter().map(asset_key).collect();

    let mut merged: Vec<AssetGroup> = Vec::with_capacity(fresh.len());
    let mut preserved = 0usize;
    let mut added = 0usize;

    for mut new_entry in fresh {
        let key = asset_key(&new_entry);
        if let Some(old) = existing_map.get(&key) {
            if old.confirmed {
                // Preserve every field the user may have filled in.
                new_entry.capture_ts = old.capture_ts.clone();
                new_entry.camera_model = old.camera_model.clone();
                new_entry.duration_ms = old.duration_ms;
                new_entry.confirmed = true;
                preserved += 1;
            }
        } else {
            added += 1;
        }
        merged.push(new_entry);
    }

    // Append entries that no longer appear in the manifest.
    let mut flagged_removed = 0usize;
    for (key, mut old) in existing_map {
        if !fresh_keys.contains(&key) && !old.removed {
            old.removed = true;
            flagged_removed += 1;
            merged.push(old);
        }
    }

    // Already-removed entries from a prior merge: keep them at the end.
    // (They were not in existing_map iteration above because .removed = true
    //  means they were preserved there already.)

    Ok(MergeResult {
        assets: merged,
        preserved,
        flagged_removed,
        added,
    })
}

// ── sample ────────────────────────────────────────────────────────────────────

fn run_sample(n: usize, mixed: bool, manifest_path: PathBuf, expected_path: PathBuf) -> Result<()> {
    let data = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("reading {}", manifest_path.display()))?;
    let manifest: Manifest = serde_json::from_str(&data)
        .with_context(|| format!("parsing {}", manifest_path.display()))?;

    // Build capture_ts lookup from expected.toml (best-effort).
    let capture_ts_map = load_capture_ts_map(&expected_path);

    // Gather all assets across cameras.
    let mut all_assets: Vec<AssetGroup> = Vec::new();
    for camera in &manifest.cameras {
        let expected = grouper::group(&camera.name, &camera.files);
        all_assets.extend(expected.asset);
    }

    if all_assets.is_empty() {
        anyhow::bail!(
            "manifest at {} has zero entries — run `keepsake corpus scan` first",
            manifest_path.display()
        );
    }

    let selected = if mixed {
        sample_mixed(all_assets, n)
    } else {
        all_assets.into_iter().take(n).collect()
    };

    // Header
    println!(
        "{:>3}  {:<20}  {:<8}  {:<52}  capture_ts",
        "#", "camera", "type", "key file"
    );
    println!("{}", "-".repeat(110));

    for (i, asset) in selected.iter().enumerate() {
        let kind = asset_type(&asset.files);
        let rep = representative_file(&asset.files);
        let ts = capture_ts_map
            .get(&sorted_key(&asset.files))
            .map(String::as_str)
            .unwrap_or("");
        println!(
            "{:>3}  {:<20}  {:<8}  {:<52}  {}",
            i + 1,
            asset.camera,
            kind,
            rep,
            ts
        );
    }

    Ok(())
}

/// Round-robin across (camera, type) buckets to pick up to `n` assets.
fn sample_mixed(assets: Vec<AssetGroup>, n: usize) -> Vec<AssetGroup> {
    let mut buckets: BTreeMap<(String, &'static str), Vec<AssetGroup>> = BTreeMap::new();
    for a in assets {
        let kind = asset_type(&a.files);
        buckets.entry((a.camera.clone(), kind)).or_default().push(a);
    }

    let mut queues: Vec<std::collections::VecDeque<AssetGroup>> = buckets
        .into_values()
        .map(std::collections::VecDeque::from)
        .collect();

    let mut result = Vec::with_capacity(n);
    let mut i = 0;
    while result.len() < n {
        let before = result.len();
        for q in &mut queues {
            if result.len() >= n {
                break;
            }
            if let Some(a) = q.pop_front() {
                result.push(a);
            }
        }
        if result.len() == before {
            break; // all queues exhausted
        }
        i += 1;
        if i > n {
            break; // safety valve
        }
    }
    result
}

fn load_capture_ts_map(path: &Path) -> HashMap<Vec<String>, String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(expected) = toml::from_str::<Expected>(&raw) else {
        return HashMap::new();
    };
    expected
        .asset
        .into_iter()
        .filter(|a| !a.capture_ts.is_empty())
        .map(|a| (sorted_key(&a.files), a.capture_ts))
        .collect()
}

fn sorted_key(files: &[String]) -> Vec<String> {
    let mut k = files.to_vec();
    k.sort();
    k
}

/// Infer a short type label from a set of file paths using role detection.
fn asset_type(files: &[String]) -> &'static str {
    for f in files {
        match detect_role(f) {
            FolderRole::Clip | FolderRole::Proxy => return "xavc",
            FolderRole::Stream
            | FolderRole::ClipInfo
            | FolderRole::Playlist
            | FolderRole::BdmvRoot => return "avchd",
            FolderRole::Dcim => return "dcim",
            FolderRole::MpRoot => return "mp_root",
            FolderRole::Unknown => {}
        }
    }
    "other"
}

/// Pick the most representative single file from an asset for display.
fn representative_file(files: &[String]) -> &str {
    // Prefer main clip / still (not sidecar, not proxy).
    for f in files {
        let role = detect_role(f);
        let ext = f.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let is_primary = matches!(
            (role, ext.as_str()),
            (FolderRole::Clip, "mp4")
                | (FolderRole::Stream, "mts")
                | (FolderRole::Dcim, "jpg" | "jpeg" | "arw")
                | (FolderRole::MpRoot, "mp4")
        );
        if is_primary {
            return f;
        }
    }
    files.first().map(String::as_str).unwrap_or("")
}

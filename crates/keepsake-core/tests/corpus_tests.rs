/// Corpus tests — skipped unless KEEPSAKE_CORPUS is set.
///
/// Run with:
///   $env:KEEPSAKE_CORPUS = "C:\path\to\corpus"
///   cargo test --workspace -- --include-ignored corpus
use keepsake_core::corpus::scanner;

fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn corpus_root() -> Option<std::path::PathBuf> {
    std::env::var("KEEPSAKE_CORPUS")
        .ok()
        .map(std::path::PathBuf::from)
}

#[test]
#[ignore = "requires KEEPSAKE_CORPUS"]
fn corpus_scan_finds_cameras() {
    let root = match corpus_root() {
        Some(r) => r,
        None => {
            eprintln!("SKIP: KEEPSAKE_CORPUS not set");
            return;
        }
    };

    let manifest = scanner::scan(&root).expect("scan should succeed");
    assert!(
        !manifest.cameras.is_empty(),
        "corpus must contain at least one camera card"
    );
    for cam in &manifest.cameras {
        assert!(!cam.files.is_empty(), "camera {} has no files", cam.name);
        eprintln!("  {} — {} file(s)", cam.name, cam.files.len());
    }
}

#[test]
#[ignore = "requires KEEPSAKE_CORPUS"]
fn corpus_scan_roundtrip() {
    let root = match corpus_root() {
        Some(r) => r,
        None => {
            eprintln!("SKIP: KEEPSAKE_CORPUS not set");
            return;
        }
    };

    let manifest = scanner::scan(&root).expect("scan failed");
    let json = serde_json::to_string_pretty(&manifest).expect("serialise failed");
    let back: keepsake_core::corpus::types::Manifest =
        serde_json::from_str(&json).expect("deserialise failed");

    assert_eq!(manifest.cameras.len(), back.cameras.len());
    for (a, b) in manifest.cameras.iter().zip(back.cameras.iter()) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.files.len(), b.files.len());
        for (fa, fb) in a.files.iter().zip(b.files.iter()) {
            assert_eq!(fa.blake3, fb.blake3);
        }
    }
}

fn tools() -> keepsake_core::probe::ToolPaths {
    keepsake_core::probe::ToolPaths::in_vendor(&workspace_root().join("vendor"))
}

/// Every confirmed entry in corpus/expected.toml must be reproduced exactly:
/// the same file grouping, capture time (instant and UTC offset, to the
/// millisecond), camera model and duration. Fails when no entry is confirmed,
/// so the test can never pass without checking anything.
#[test]
#[ignore = "requires KEEPSAKE_CORPUS"]
fn corpus_matches_every_confirmed_entry() {
    let root = match corpus_root() {
        Some(r) => r,
        None => {
            eprintln!("SKIP: KEEPSAKE_CORPUS not set");
            return;
        }
    };
    let expected_path = workspace_root().join("corpus/expected.toml");
    let toml_str = std::fs::read_to_string(&expected_path)
        .expect("corpus/expected.toml missing: run `keepsake corpus init-expected --probe`");
    let expected: keepsake_core::corpus::types::Expected =
        toml::from_str(&toml_str).expect("parse expected.toml");
    let confirmed: Vec<_> = expected
        .asset
        .iter()
        .filter(|a| a.confirmed && !a.removed)
        .collect();
    assert!(
        !confirmed.is_empty(),
        "no confirmed entries in corpus/expected.toml: check some against the camera and set confirmed = true"
    );

    let prober = keepsake_core::probe::Prober::new(&tools(), Default::default())
        .expect("ExifTool/ffprobe in vendor/ (scripts/setup)");
    let manifest = scanner::scan(&root).expect("scan failed");
    let mut failures = Vec::new();
    let mut probed_cards = std::collections::HashMap::new();
    for asset in &confirmed {
        let cam = manifest
            .cameras
            .iter()
            .find(|c| c.name == asset.camera)
            .unwrap_or_else(|| panic!("camera '{}' not in corpus", asset.camera));
        let probed = probed_cards.entry(cam.name.clone()).or_insert_with(|| {
            keepsake_core::corpus::probed::probe_card(
                &root.join("cards").join(&cam.name),
                &cam.files,
                &prober,
            )
        });
        let mut want = asset.files.clone();
        want.sort();
        let Some(got) = probed.iter().find(|p| p.files == want) else {
            let near: Vec<_> = probed
                .iter()
                .filter(|p| p.files.iter().any(|f| want.contains(f)))
                .map(|p| p.files.clone())
                .collect();
            failures.push(format!("{want:?}: not grouped this way (got {near:?})"));
            continue;
        };
        let parse = |s: &str| keepsake_core::xmp::parse_date(s);
        if parse(&asset.capture_ts) != parse(&got.capture_ts) || parse(&asset.capture_ts).is_none()
        {
            failures.push(format!(
                "{want:?}: capture_ts expected {:?}, got {:?} {:?}",
                asset.capture_ts, got.capture_ts, got.notes
            ));
        }
        if asset.camera_model != got.camera_model {
            failures.push(format!(
                "{want:?}: camera_model expected {:?}, got {:?}",
                asset.camera_model, got.camera_model
            ));
        }
        if asset.duration_ms != got.duration_ms {
            failures.push(format!(
                "{want:?}: duration_ms expected {:?}, got {:?}",
                asset.duration_ms, got.duration_ms
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} confirmed entries differ:
{}",
        failures.len(),
        confirmed.len(),
        failures.join(
            "
"
        )
    );
    eprintln!("Verified {} confirmed asset(s)", confirmed.len());
}

/// Every corpus file is either part of an asset or a known card file, and
/// probing the whole corpus reports no problems.
#[test]
#[ignore = "requires KEEPSAKE_CORPUS"]
fn corpus_probes_cleanly() {
    let Some(root) = corpus_root() else {
        eprintln!("SKIP: KEEPSAKE_CORPUS not set");
        return;
    };
    let prober = keepsake_core::probe::Prober::new(&tools(), Default::default())
        .expect("ExifTool/ffprobe in vendor/");
    let manifest = scanner::scan(&root).expect("scan failed");
    for cam in &manifest.cameras {
        let probed = keepsake_core::corpus::probed::probe_card(
            &root.join("cards").join(&cam.name),
            &cam.files,
            &prober,
        );
        // Every file lands in exactly one asset or is a card/other file.
        let rels: Vec<keepsake_core::relpath::RelPath> = cam
            .files
            .iter()
            .map(|f| keepsake_core::relpath::RelPath::parse(&f.rel_path).unwrap())
            .collect();
        let card_root = root.join("cards").join(&cam.name);
        let (layout, _) = keepsake_core::grouping::read_avchd_layout(&card_root, &rels);
        let g = keepsake_core::grouping::group(&rels, layout.as_ref());
        let in_assets: usize = probed.iter().map(|p| p.files.len()).sum();
        assert_eq!(
            in_assets + g.card_files.len() + g.ungrouped.len(),
            cam.files.len(),
            "{}: files lost or counted twice",
            cam.name
        );
        for p in &probed {
            assert!(
                !p.capture_ts.is_empty(),
                "{:?} has no capture time: {:?}",
                p.files,
                p.notes
            );
            let problems: Vec<_> = p
                .notes
                .iter()
                .filter(|n| !n.starts_with("capture:"))
                .collect();
            assert!(
                problems
                    .iter()
                    .all(|n| !n.contains("failed") && !n.contains("cannot")),
                "{:?}: {problems:?}",
                p.files
            );
        }
        eprintln!(
            "  {}: {} assets from {} files",
            cam.name,
            probed.len(),
            cam.files.len()
        );
    }
}

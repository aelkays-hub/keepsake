/// Corpus tests — skipped unless KEEPSAKE_CORPUS is set.
///
/// Run with:
///   $env:KEEPSAKE_CORPUS = "C:\path\to\corpus"
///   cargo test --workspace -- --include-ignored corpus
use keepsake_core::corpus::{grouper, scanner};

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

#[test]
#[ignore = "requires KEEPSAKE_CORPUS"]
fn corpus_grouper_matches_expected() {
    let root = match corpus_root() {
        Some(r) => r,
        None => {
            eprintln!("SKIP: KEEPSAKE_CORPUS not set");
            return;
        }
    };

    let expected_path = workspace_root().join("corpus/expected.toml");
    if !expected_path.exists() {
        eprintln!("SKIP: corpus/expected.toml not found — run `keepsake corpus init-expected`");
        return;
    }

    let toml_str = std::fs::read_to_string(&expected_path).expect("read expected.toml");
    let expected: keepsake_core::corpus::types::Expected =
        toml::from_str(&toml_str).expect("parse expected.toml");

    let confirmed: Vec<_> = expected.asset.iter().filter(|a| a.confirmed).collect();
    if confirmed.is_empty() {
        eprintln!("SKIP: no confirmed assets in corpus/expected.toml yet");
        return;
    }

    let manifest = scanner::scan(&root).expect("scan failed");

    for asset in &confirmed {
        // Find the camera card for this asset.
        let cam = manifest
            .cameras
            .iter()
            .find(|c| c.name == asset.camera)
            .unwrap_or_else(|| panic!("camera '{}' not in manifest", asset.camera));

        let grouped = grouper::group(&cam.name, &cam.files);

        // The confirmed asset's file set must appear in the grouped output.
        let found = grouped.asset.iter().any(|g| {
            let mut expected_files = asset.files.clone();
            let mut actual_files = g.files.clone();
            expected_files.sort();
            actual_files.sort();
            expected_files == actual_files
        });

        assert!(
            found,
            "confirmed asset with files {:?} not reproduced by grouper",
            asset.files
        );
    }

    eprintln!("Verified {} confirmed asset(s)", confirmed.len());
}

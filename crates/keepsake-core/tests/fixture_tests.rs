use keepsake_core::corpus::{
    grouper,
    scanner::{detect_role, scan},
    types::FolderRole,
};

fn fixtures_root() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR = crates/keepsake-core
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap() // crates/
        .parent()
        .unwrap() // workspace root
        .join("tests/fixtures")
}

// ── detect_role unit tests ────────────────────────────────────────────────────

#[test]
fn role_xavc_clip() {
    assert_eq!(
        detect_role("PRIVATE/M4ROOT/CLIP/C0001.MP4"),
        FolderRole::Clip
    );
}

#[test]
fn role_xavc_xml_sidecar() {
    assert_eq!(
        detect_role("PRIVATE/M4ROOT/CLIP/C0001M01.XML"),
        FolderRole::Clip
    );
}

#[test]
fn role_xavc_proxy() {
    assert_eq!(
        detect_role("PRIVATE/M4ROOT/SUB/C0001S01.MP4"),
        FolderRole::Proxy
    );
}

#[test]
fn role_avchd_stream() {
    assert_eq!(
        detect_role("PRIVATE/AVCHD/BDMV/STREAM/00001.MTS"),
        FolderRole::Stream
    );
}

#[test]
fn role_avchd_clipinfo() {
    assert_eq!(
        detect_role("PRIVATE/AVCHD/BDMV/CLIPINF/00001.CPI"),
        FolderRole::ClipInfo
    );
}

#[test]
fn role_avchd_playlist() {
    assert_eq!(
        detect_role("PRIVATE/AVCHD/BDMV/PLAYLIST/00001.MPL"),
        FolderRole::Playlist
    );
}

#[test]
fn role_avchd_bdmv_root() {
    assert_eq!(
        detect_role("PRIVATE/AVCHD/BDMV/INDEX.BDM"),
        FolderRole::BdmvRoot
    );
    assert_eq!(
        detect_role("PRIVATE/AVCHD/BDMV/MOVIEOBJ.BDM"),
        FolderRole::BdmvRoot
    );
}

#[test]
fn role_dcim() {
    assert_eq!(detect_role("DCIM/100MSDCF/DSC00001.JPG"), FolderRole::Dcim);
}

#[test]
fn role_mp_root() {
    assert_eq!(
        detect_role("MP_ROOT/100ANV01/MAH00001.MP4"),
        FolderRole::MpRoot
    );
}

// ── Scanner integration tests ─────────────────────────────────────────────────

/// Wrap scanner::scan so fixture layouts look like a corpus with a `cards/` dir.
fn scan_fixture(layout: &str) -> Vec<keepsake_core::corpus::types::FileEntry> {
    let tmp = tempdir();
    let cards = tmp.join("cards").join("fixture");
    let src = fixtures_root().join(layout);
    copy_dir_all(&src, &cards);
    let manifest = scan(&tmp).expect("scan failed");
    assert_eq!(manifest.cameras.len(), 1);
    manifest.cameras.into_iter().next().unwrap().files
}

fn tempdir() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!(
        "keepsake-fixture-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .subsec_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let ty = entry.file_type().unwrap();
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()));
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name())).unwrap();
        }
    }
}

#[test]
fn xavc_scan_file_count() {
    let files = scan_fixture("xavc");
    // 2 clips × (MP4 + XML) + 2 proxies = 6 files
    assert_eq!(
        files.len(),
        6,
        "expected 6 files, got {}: {files:#?}",
        files.len()
    );
}

#[test]
fn xavc_scan_all_roles_assigned() {
    let files = scan_fixture("xavc");
    let clips: Vec<_> = files
        .iter()
        .filter(|f| f.folder_role == FolderRole::Clip)
        .collect();
    let proxies: Vec<_> = files
        .iter()
        .filter(|f| f.folder_role == FolderRole::Proxy)
        .collect();
    assert_eq!(clips.len(), 4, "4 clip-role files (2 MP4 + 2 XML)");
    assert_eq!(proxies.len(), 2, "2 proxy-role files");
}

#[test]
fn avchd_scan_file_count() {
    let files = scan_fixture("avchd");
    // 2 MTS + 2 CPI + 1 MPL + INDEX.BDM + MOVIEOBJ.BDM = 7
    assert_eq!(files.len(), 7, "expected 7 AVCHD files");
}

#[test]
fn avchd_scan_roles() {
    let files = scan_fixture("avchd");
    let streams: Vec<_> = files
        .iter()
        .filter(|f| f.folder_role == FolderRole::Stream)
        .collect();
    let clipinfos: Vec<_> = files
        .iter()
        .filter(|f| f.folder_role == FolderRole::ClipInfo)
        .collect();
    let playlists: Vec<_> = files
        .iter()
        .filter(|f| f.folder_role == FolderRole::Playlist)
        .collect();
    let bdmv_roots: Vec<_> = files
        .iter()
        .filter(|f| f.folder_role == FolderRole::BdmvRoot)
        .collect();
    assert_eq!(streams.len(), 2);
    assert_eq!(clipinfos.len(), 2);
    assert_eq!(playlists.len(), 1);
    assert_eq!(bdmv_roots.len(), 2);
}

#[test]
fn dcim_scan_roles() {
    let files = scan_fixture("dcim");
    // DSC00001.JPG + ARW + HIF + DSC00002.JPG = 4
    assert_eq!(files.len(), 4);
    assert!(files.iter().all(|f| f.folder_role == FolderRole::Dcim));
}

#[test]
fn mp_root_scan_roles() {
    let files = scan_fixture("mp_root");
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|f| f.folder_role == FolderRole::MpRoot));
}

// ── Grouper tests ─────────────────────────────────────────────────────────────

#[test]
fn xavc_grouper_two_assets() {
    let files = scan_fixture("xavc");
    let expected = grouper::group("test-cam", &files);
    // C0001 group (MP4 + XML + proxy) and C0002 group — 2 assets total
    assert_eq!(expected.asset.len(), 2, "expected 2 XAVC assets");
    for asset in &expected.asset {
        assert_eq!(
            asset.files.len(),
            3,
            "each XAVC asset has 3 files (clip+sidecar+proxy)"
        );
    }
}

#[test]
fn xavc_grouper_clip_stems() {
    let files = scan_fixture("xavc");
    let expected = grouper::group("test-cam", &files);
    let mut stems: Vec<String> = expected
        .asset
        .iter()
        .map(|a| {
            // The MP4 clip path should be in each group
            a.files
                .iter()
                .find(|p| p.ends_with(".MP4") && p.contains("CLIP"))
                .cloned()
                .unwrap_or_default()
        })
        .collect();
    stems.sort();
    assert!(stems[0].contains("C0001"), "first asset has C0001 clip");
    assert!(stems[1].contains("C0002"), "second asset has C0002 clip");
}

#[test]
fn avchd_grouper_produces_groups() {
    let files = scan_fixture("avchd");
    let expected = grouper::group("test-cam", &files);
    // Without reading playlists: 00001 (MTS+CPI) and 00002 (MTS+CPI).
    // INDEX.BDM, MOVIEOBJ.BDM and the playlist are card files, not assets.
    assert_eq!(expected.asset.len(), 2, "expected 2 AVCHD groups");
    for a in &expected.asset {
        assert_eq!(a.files.len(), 2, "{:?}", a.files);
        assert!(a.files.iter().any(|f| f.ends_with(".MTS")));
        assert!(a.files.iter().any(|f| f.ends_with(".CPI")));
    }
}

#[test]
fn dcim_grouper_joins_raw_pairs() {
    let files = scan_fixture("dcim");
    let expected = grouper::group("test-cam", &files);
    // DSC00001 → 3 files; DSC00002 → 1 file → 2 assets
    assert_eq!(expected.asset.len(), 2);
    let big = expected
        .asset
        .iter()
        .find(|a| a.files.len() == 3)
        .expect("DSC00001 group");
    assert!(big.files.iter().any(|f| f.ends_with(".ARW")));
    assert!(big.files.iter().any(|f| f.ends_with(".HIF")));
}

#[test]
fn mp_root_grouper_one_asset_per_file() {
    let files = scan_fixture("mp_root");
    let expected = grouper::group("test-cam", &files);
    assert_eq!(
        expected.asset.len(),
        2,
        "each MP_ROOT file is its own asset"
    );
}

#[test]
fn grouper_confirmed_false_by_default() {
    let files = scan_fixture("xavc");
    let expected = grouper::group("test-cam", &files);
    assert!(expected.asset.iter().all(|a| !a.confirmed));
}

#[test]
fn scanner_blake3_is_deterministic() {
    let files1 = scan_fixture("xavc");
    let files2 = scan_fixture("xavc");
    let h1: Vec<_> = files1.iter().map(|f| &f.blake3).collect();
    let h2: Vec<_> = files2.iter().map(|f| &f.blake3).collect();
    assert_eq!(h1, h2, "BLAKE3 hashes must be deterministic");
}

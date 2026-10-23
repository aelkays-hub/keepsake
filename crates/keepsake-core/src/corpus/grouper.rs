use std::path::Path;

use crate::corpus::types::{AssetGroup, Expected, FileEntry};
use crate::grouping::{self, AvchdLayout};
use crate::relpath::RelPath;

/// Group a camera card's file list into logical assets using path rules only
/// (AVCHD segments stay separate because playlists are not read).
///
/// Card bookkeeping files (AVF_INFO, DATABASE, MEDIAPRO.XML, BDMV indexes,
/// playlists) belong to no asset and are left out.
pub fn group(camera: &str, files: &[FileEntry]) -> Expected {
    group_inner(camera, files, None)
}

/// Like [`group`], but reads the card's AVCHD playlists from `card_root` so
/// split recordings become one asset.
pub fn group_on_disk(camera: &str, files: &[FileEntry], card_root: &Path) -> Expected {
    let rels = rel_paths(files);
    let (layout, _) = grouping::read_avchd_layout(card_root, &rels);
    group_inner(camera, files, layout.as_ref())
}

fn rel_paths(files: &[FileEntry]) -> Vec<RelPath> {
    files
        .iter()
        .filter_map(|f| RelPath::parse(&f.rel_path).ok())
        .collect()
}

fn group_inner(camera: &str, files: &[FileEntry], layout: Option<&AvchdLayout>) -> Expected {
    let rels = rel_paths(files);
    let g = grouping::group(&rels, layout);
    let asset = g
        .groups
        .iter()
        .map(|grp| {
            let mut files: Vec<String> = grp
                .members
                .iter()
                .map(|m| rels[m.index].as_str().to_owned())
                .collect();
            files.sort();
            AssetGroup {
                camera: camera.to_owned(),
                files,
                capture_ts: String::new(),
                camera_model: String::new(),
                duration_ms: None,
                confirmed: false,
                removed: false,
                prefilled: false,
            }
        })
        .collect();
    Expected { asset }
}

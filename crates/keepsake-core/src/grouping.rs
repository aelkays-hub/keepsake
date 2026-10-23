//! Turn a list of files into logical assets.
//!
//! - XAVC: `C0001.MP4` + `C0001M01.XML` + `SUB/C0001S01.MP4` + `THMBNL/C0001T01.JPG`.
//! - Stills: files sharing a stem in one folder (JPG, HEIF, RAW). The JPG is
//!   the original, then HEIF, then RAW; the others are `raw_pair`.
//! - AVCHD: segments joined by the card's playlists become one recording
//!   (first segment `original`, the rest `segment`), with their `.CPI` files.
//! - Card bookkeeping (`AVF_INFO`, `PRIVATE/DATABASE`, `MEDIAPRO.XML`,
//!   `INDEX.BDM`, playlists, …) belongs to no asset.
//!
//! Bursts, brackets and intervals are *not* merged here; see [`sequences`].

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::catalog::{FileRole, MediaKind};
use crate::probe::avchd;
use crate::relpath::RelPath;

pub mod sequences;

/// The kind of layout an asset came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    Xavc,
    Avchd,
    Still,
    Video,
}

/// One file of a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// Index into the input file list.
    pub index: usize,
    pub role: FileRole,
    /// Position among the recording's segments (0 for the original).
    pub segment: Option<u32>,
}

/// One logical asset. `members[0]` is the original.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub kind: GroupKind,
    pub members: Vec<Member>,
}

impl Group {
    pub fn original(&self) -> usize {
        self.members[0].index
    }

    pub fn media_kind(&self) -> MediaKind {
        match self.kind {
            GroupKind::Still => MediaKind::Photo,
            _ => MediaKind::Video,
        }
    }

    /// Segments in recording order (original first), for AVCHD.
    pub fn segments(&self) -> Vec<usize> {
        let mut s: Vec<(u32, usize)> = self
            .members
            .iter()
            .filter_map(|m| m.segment.map(|p| (p, m.index)))
            .collect();
        s.sort();
        s.into_iter().map(|(_, i)| i).collect()
    }
}

/// Result of grouping.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grouping {
    pub groups: Vec<Group>,
    /// Camera bookkeeping files (indices).
    pub card_files: Vec<usize>,
    /// Files that are not media and belong to no asset (indices).
    pub ungrouped: Vec<usize>,
    pub problems: Vec<String>,
}

/// AVCHD recordings read from the card's playlists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AvchdLayout {
    /// Ordered clip names (`00001`) per recording.
    pub recordings: Vec<Vec<String>>,
}

const VIDEO_EXT: &[&str] = &["mp4", "mts", "m2ts", "mov", "avi", "mkv"];
const RAW_EXT: &[&str] = &[
    "arw", "srf", "sr2", "dng", "cr2", "cr3", "nef", "raf", "orf", "rw2",
];
const HEIF_EXT: &[&str] = &["hif", "heif", "heic"];
const JPEG_EXT: &[&str] = &["jpg", "jpeg"];
const OTHER_IMAGE_EXT: &[&str] = &["png", "tif", "tiff"];

fn split_name(name: &str) -> (&str, String) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, ext.to_ascii_lowercase()),
        _ => (name, String::new()),
    }
}

/// Upper-cased path components for layout matching.
fn upper_parts(rel: &RelPath) -> Vec<String> {
    rel.components().map(|c| c.to_uppercase()).collect()
}

/// `C0001M01` / `C0001S01` / `C0001T01` → (`C0001`, Some('M'|'S'|'T')).
pub fn sony_suffix(stem: &str) -> (&str, Option<char>) {
    let b = stem.as_bytes();
    if b.len() >= 4 {
        let n = b.len();
        let tag = b[n - 3].to_ascii_uppercase();
        if matches!(tag, b'M' | b'S' | b'T')
            && b[n - 2].is_ascii_digit()
            && b[n - 1].is_ascii_digit()
        {
            let base = &stem[..n - 3];
            if base.bytes().last().is_some_and(|c| c.is_ascii_digit()) {
                return (base, Some(tag as char));
            }
        }
    }
    (stem, None)
}

/// Where a file sits in a Sony card layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    XavcClip,
    XavcSub,
    XavcThumb,
    AvchdStream,
    AvchdClipInfo,
    Card,
    Other,
}

fn place(parts: &[String]) -> Place {
    let dirs = &parts[..parts.len().saturating_sub(1)];
    let has = |seq: &[&str]| {
        dirs.windows(seq.len())
            .any(|w| w.iter().zip(seq).all(|(a, b)| a == b))
    };
    if has(&["M4ROOT", "CLIP"]) {
        return Place::XavcClip;
    }
    if has(&["M4ROOT", "SUB"]) {
        return Place::XavcSub;
    }
    if has(&["M4ROOT", "THMBNL"]) {
        return Place::XavcThumb;
    }
    if has(&["BDMV", "BACKUP"]) {
        return Place::Card;
    }
    if has(&["BDMV", "STREAM"]) {
        return Place::AvchdStream;
    }
    if has(&["BDMV", "CLIPINF"]) {
        return Place::AvchdClipInfo;
    }
    if has(&["BDMV"]) {
        // INDEX.BDM, MOVIEOBJ.BDM, PLAYLIST/*.MPL, AUXDATA, …
        return Place::Card;
    }
    let top = dirs.first().map(String::as_str);
    if top == Some("AVF_INFO") {
        return Place::Card;
    }
    if top == Some("PRIVATE") {
        // Anything under PRIVATE/ that is not one of the media folders above.
        return Place::Card;
    }
    if dirs.is_empty() && matches!(parts.last().map(String::as_str), Some("SONYCARD.IND")) {
        return Place::Card;
    }
    Place::Other
}

/// Read every playlist on an AVCHD card. Missing or unreadable playlists are
/// reported; the caller then falls back to one asset per segment.
pub fn read_avchd_layout(root: &Path, files: &[RelPath]) -> (Option<AvchdLayout>, Vec<String>) {
    let mut problems = Vec::new();
    let mut lists = Vec::new();
    for f in files {
        let parts = upper_parts(f);
        let n = parts.len();
        if n >= 2 && parts[n - 2] == "PLAYLIST" && parts.contains(&"BDMV".to_owned()) {
            if !split_name(f.file_name()).1.eq("mpl") {
                continue;
            }
            match std::fs::read(f.to_fs_path(root)) {
                Ok(b) => match avchd::parse_mpl(&b) {
                    Ok(items) => lists.push(items),
                    Err(e) => problems.push(format!("{f}: {e}")),
                },
                Err(e) => problems.push(format!("{f}: {e}")),
            }
        }
    }
    if lists.is_empty() {
        return (None, problems);
    }
    (
        Some(AvchdLayout {
            recordings: avchd::recordings(&lists),
        }),
        problems,
    )
}

#[derive(Default)]
struct Bucket {
    files: Vec<usize>,
}

/// Group files. `avchd` comes from [`read_avchd_layout`]; without it each
/// `.MTS` segment becomes its own asset.
pub fn group(files: &[RelPath], avchd: Option<&AvchdLayout>) -> Grouping {
    let mut out = Grouping::default();
    // Buckets keyed by (kind, folder, base stem), upper-cased.
    let mut buckets: BTreeMap<(u8, String, String), Bucket> = BTreeMap::new();
    let mut streams: HashMap<String, usize> = HashMap::new(); // clip → MTS index
    let mut clipinfo: HashMap<String, usize> = HashMap::new(); // clip → CPI index

    for (i, f) in files.iter().enumerate() {
        let parts = upper_parts(f);
        let (stem, ext) = split_name(f.file_name());
        let stem_u = stem.to_uppercase();
        let folder: String = parts[..parts.len() - 1].join("/");
        match place(&parts) {
            Place::Card => out.card_files.push(i),
            Place::AvchdStream if matches!(ext.as_str(), "mts" | "m2ts") => {
                streams.insert(stem_u, i);
            }
            Place::AvchdClipInfo if ext == "cpi" => {
                clipinfo.insert(stem_u, i);
            }
            Place::AvchdStream | Place::AvchdClipInfo => out.card_files.push(i),
            Place::XavcClip | Place::XavcSub | Place::XavcThumb => {
                let (base, _) = sony_suffix(&stem_u);
                // Clip, proxy and thumbnail live in sibling folders under M4ROOT.
                let m4root: String = parts
                    .iter()
                    .take_while(|p| p.as_str() != "M4ROOT")
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("/");
                buckets
                    .entry((0, m4root, base.to_owned()))
                    .or_default()
                    .files
                    .push(i);
            }
            Place::Other => {
                let (base, _) = sony_suffix(&stem_u);
                buckets
                    .entry((1, folder, base.to_owned()))
                    .or_default()
                    .files
                    .push(i);
            }
        }
    }

    // AVCHD recordings.
    let recordings: Vec<Vec<String>> = match avchd {
        Some(l) => l.recordings.clone(),
        None => {
            let mut c: Vec<String> = streams.keys().cloned().collect();
            c.sort();
            c.into_iter().map(|x| vec![x]).collect()
        }
    };
    let mut used_streams = std::collections::HashSet::new();
    for rec in &recordings {
        let mut members = Vec::new();
        let mut pos = 0u32;
        for clip in rec {
            let clip = clip.to_uppercase();
            if let Some(&s) = streams.get(&clip) {
                if !used_streams.insert(s) {
                    continue;
                }
                let role = if pos == 0 {
                    FileRole::Original
                } else {
                    FileRole::Segment
                };
                members.push(Member {
                    index: s,
                    role,
                    segment: Some(pos),
                });
                pos += 1;
                if let Some(&c) = clipinfo.get(&clip) {
                    members.push(Member {
                        index: c,
                        role: FileRole::Sidecar,
                        segment: None,
                    });
                }
            } else {
                out.problems.push(format!(
                    "playlist lists clip {clip} but STREAM/{clip}.MTS is missing"
                ));
            }
        }
        if !members.is_empty() {
            out.groups.push(Group {
                kind: GroupKind::Avchd,
                members,
            });
        }
    }
    // Segments no playlist mentions.
    let mut orphans: Vec<(&String, &usize)> = streams
        .iter()
        .filter(|(_, i)| !used_streams.contains(*i))
        .collect();
    orphans.sort();
    for (clip, &s) in orphans {
        let mut members = vec![Member {
            index: s,
            role: FileRole::Original,
            segment: Some(0),
        }];
        if let Some(&c) = clipinfo.get(clip) {
            members.push(Member {
                index: c,
                role: FileRole::Sidecar,
                segment: None,
            });
        }
        out.groups.push(Group {
            kind: GroupKind::Avchd,
            members,
        });
    }
    // CPI files whose clip is gone.
    let used: std::collections::HashSet<usize> = out
        .groups
        .iter()
        .flat_map(|g| g.members.iter().map(|m| m.index))
        .collect();
    let mut stray: Vec<usize> = clipinfo
        .values()
        .filter(|i| !used.contains(i))
        .copied()
        .collect();
    stray.sort();
    out.card_files.extend(stray);

    // Everything else.
    for ((_, _, _), b) in buckets {
        match assign(files, &b.files) {
            Some(g) => out.groups.push(g),
            None => out.ungrouped.extend(b.files),
        }
    }
    out.card_files.sort();
    out.ungrouped.sort();
    out.groups
        .sort_by(|a, b| files[a.original()].cmp(&files[b.original()]));
    out
}

/// Rank a file as a candidate original (lower is better).
fn original_rank(rel: &RelPath) -> Option<(u8, String)> {
    let parts = upper_parts(rel);
    let p = place(&parts);
    let (stem, ext) = split_name(rel.file_name());
    let (_, suffix) = sony_suffix(&stem.to_uppercase());
    if matches!(p, Place::XavcSub | Place::XavcThumb) || suffix.is_some() {
        return None;
    }
    let r = if VIDEO_EXT.contains(&ext.as_str()) {
        0
    } else if JPEG_EXT.contains(&ext.as_str()) {
        1
    } else if HEIF_EXT.contains(&ext.as_str()) {
        2
    } else if OTHER_IMAGE_EXT.contains(&ext.as_str()) {
        3
    } else if RAW_EXT.contains(&ext.as_str()) {
        4
    } else {
        return None;
    };
    Some((r, rel.as_str().to_uppercase()))
}

fn assign(files: &[RelPath], idx: &[usize]) -> Option<Group> {
    let orig = *idx
        .iter()
        .filter_map(|i| original_rank(&files[*i]).map(|r| (r, i)))
        .min()?
        .1;
    let orig_ext = split_name(files[orig].file_name()).1;
    let kind = if VIDEO_EXT.contains(&orig_ext.as_str()) {
        let parts = upper_parts(&files[orig]);
        if place(&parts) == Place::XavcClip {
            GroupKind::Xavc
        } else {
            GroupKind::Video
        }
    } else {
        GroupKind::Still
    };
    let mut members = vec![Member {
        index: orig,
        role: FileRole::Original,
        segment: None,
    }];
    let mut rest: Vec<usize> = idx.iter().copied().filter(|i| *i != orig).collect();
    rest.sort_by(|a, b| files[*a].cmp(&files[*b]));
    for i in rest {
        let rel = &files[i];
        let parts = upper_parts(rel);
        let (stem, ext) = split_name(rel.file_name());
        let (_, suffix) = sony_suffix(&stem.to_uppercase());
        let role = match (place(&parts), suffix) {
            (Place::XavcSub, _) | (_, Some('S')) if VIDEO_EXT.contains(&ext.as_str()) => {
                FileRole::CameraProxy
            }
            _ if kind == GroupKind::Still
                && (RAW_EXT.contains(&ext.as_str())
                    || HEIF_EXT.contains(&ext.as_str())
                    || JPEG_EXT.contains(&ext.as_str())
                    || OTHER_IMAGE_EXT.contains(&ext.as_str())) =>
            {
                FileRole::RawPair
            }
            // Sony XML, camera thumbnails, .THM and anything else that shares the stem.
            _ => FileRole::Sidecar,
        };
        members.push(Member {
            index: i,
            role,
            segment: None,
        });
    }
    Some(Group { kind, members })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rels(paths: &[&str]) -> Vec<RelPath> {
        paths.iter().map(|p| RelPath::parse(p).unwrap()).collect()
    }

    fn roles(files: &[RelPath], g: &Group) -> Vec<(String, FileRole)> {
        g.members
            .iter()
            .map(|m| (files[m.index].to_string(), m.role))
            .collect()
    }

    #[test]
    fn suffixes() {
        assert_eq!(sony_suffix("C0001M01"), ("C0001", Some('M')));
        assert_eq!(sony_suffix("C0001S01"), ("C0001", Some('S')));
        assert_eq!(sony_suffix("C0001T01"), ("C0001", Some('T')));
        assert_eq!(sony_suffix("C0001"), ("C0001", None));
        assert_eq!(sony_suffix("DSC00001"), ("DSC00001", None));
        assert_eq!(sony_suffix("PHOTOS01"), ("PHOTOS01", None));
    }

    #[test]
    fn xavc_with_thumbnail_and_card_files() {
        let f = rels(&[
            "PRIVATE/M4ROOT/CLIP/C0006.MP4",
            "PRIVATE/M4ROOT/CLIP/C0006M01.XML",
            "PRIVATE/M4ROOT/SUB/C0006S01.MP4",
            "PRIVATE/M4ROOT/THMBNL/C0006T01.JPG",
            "PRIVATE/M4ROOT/MEDIAPRO.XML",
            "PRIVATE/DATABASE/DATABASE.BIN",
            "AVF_INFO/AVIN0001.BNP",
            "SONYCARD.IND",
        ]);
        let g = group(&f, None);
        assert_eq!(g.groups.len(), 1);
        assert_eq!(
            roles(&f, &g.groups[0]),
            vec![
                ("PRIVATE/M4ROOT/CLIP/C0006.MP4".into(), FileRole::Original),
                ("PRIVATE/M4ROOT/CLIP/C0006M01.XML".into(), FileRole::Sidecar),
                (
                    "PRIVATE/M4ROOT/SUB/C0006S01.MP4".into(),
                    FileRole::CameraProxy
                ),
                (
                    "PRIVATE/M4ROOT/THMBNL/C0006T01.JPG".into(),
                    FileRole::Sidecar
                ),
            ]
        );
        assert_eq!(g.groups[0].kind, GroupKind::Xavc);
        assert_eq!(g.card_files, vec![4, 5, 6, 7]);
    }

    #[test]
    fn stills_prefer_jpeg_then_heif_then_raw() {
        let f = rels(&[
            "DCIM/100MSDCF/A7404967.ARW",
            "DCIM/100MSDCF/A7404967.JPG",
            "DCIM/100MSDCF/A7404967.HIF",
            "DCIM/100MSDCF/A7404968.HIF",
            "DCIM/100MSDCF/A7404968.ARW",
            "DCIM/100MSDCF/A7404969.ARW",
        ]);
        let g = group(&f, None);
        assert_eq!(g.groups.len(), 3);
        let orig: Vec<&str> = g
            .groups
            .iter()
            .map(|x| f[x.original()].file_name())
            .collect();
        assert_eq!(orig, vec!["A7404967.JPG", "A7404968.HIF", "A7404969.ARW"]);
        assert!(g.groups[0].members[1..]
            .iter()
            .all(|m| m.role == FileRole::RawPair));
    }

    #[test]
    fn avchd_segments_join_into_one_recording() {
        let f = rels(&[
            "PRIVATE/AVCHD/BDMV/STREAM/00001.MTS",
            "PRIVATE/AVCHD/BDMV/STREAM/00002.MTS",
            "PRIVATE/AVCHD/BDMV/STREAM/00003.MTS",
            "PRIVATE/AVCHD/BDMV/CLIPINF/00001.CPI",
            "PRIVATE/AVCHD/BDMV/CLIPINF/00002.CPI",
            "PRIVATE/AVCHD/BDMV/CLIPINF/00003.CPI",
            "PRIVATE/AVCHD/BDMV/PLAYLIST/00000.MPL",
            "PRIVATE/AVCHD/BDMV/INDEX.BDM",
            "PRIVATE/AVCHD/BDMV/MOVIEOBJ.BDM",
        ]);
        let layout = AvchdLayout {
            recordings: vec![vec!["00001".into(), "00002".into()], vec!["00003".into()]],
        };
        let g = group(&f, Some(&layout));
        assert_eq!(g.groups.len(), 2);
        let first = &g.groups[0];
        assert_eq!(first.kind, GroupKind::Avchd);
        assert_eq!(first.segments(), vec![0, 1]);
        assert_eq!(
            roles(&f, first),
            vec![
                (
                    "PRIVATE/AVCHD/BDMV/STREAM/00001.MTS".into(),
                    FileRole::Original
                ),
                (
                    "PRIVATE/AVCHD/BDMV/CLIPINF/00001.CPI".into(),
                    FileRole::Sidecar
                ),
                (
                    "PRIVATE/AVCHD/BDMV/STREAM/00002.MTS".into(),
                    FileRole::Segment
                ),
                (
                    "PRIVATE/AVCHD/BDMV/CLIPINF/00002.CPI".into(),
                    FileRole::Sidecar
                ),
            ]
        );
        assert_eq!(g.card_files, vec![6, 7, 8]);

        // Without playlists every segment stands alone.
        let g = group(&f, None);
        assert_eq!(g.groups.len(), 3);
    }

    #[test]
    fn copies_on_other_drives_still_group() {
        let f = rels(&[
            "2024/Trip/C0001.MP4",
            "2024/Trip/C0001M01.XML",
            "2024/Trip/C0001S01.MP4",
            "2024/Trip/notes.txt",
            "2024/Trip/MAH00001.MP4",
            "2024/Trip/MAH00001.THM",
        ]);
        let g = group(&f, None);
        assert_eq!(g.groups.len(), 2);
        let c = g
            .groups
            .iter()
            .find(|x| f[x.original()].file_name() == "C0001.MP4")
            .unwrap();
        assert_eq!(c.members.len(), 3);
        assert_eq!(c.members[1].role, FileRole::Sidecar); // the XML
        assert_eq!(c.members[2].role, FileRole::CameraProxy);
        assert_eq!(g.ungrouped, vec![3]);
    }
}

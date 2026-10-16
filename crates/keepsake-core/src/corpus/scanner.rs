use std::io::Read;
use std::path::Path;

use chrono::Utc;

use crate::corpus::types::{
    CameraCard, FileEntry, FileKind, FolderRole, Manifest, SkipReason, SkippedFile,
};
use crate::error::CoreError;

/// Scan `corpus_root/cards/` and return a manifest of every file found.
///
/// Each subdirectory of `cards/` is treated as one camera card.
/// Paths in each `FileEntry` are relative to that camera's card root.
pub fn scan(corpus_root: &Path) -> Result<Manifest, CoreError> {
    let cards_dir = corpus_root.join("cards");
    let mut cameras = Vec::new();

    if cards_dir.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(&cards_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let camera_name = entry.file_name().to_string_lossy().into_owned();
            let card_root = entry.path();
            let mut files = Vec::new();
            let mut skipped = Vec::new();
            collect_files(&card_root, &card_root, &mut files, &mut skipped)?;
            files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
            skipped.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
            cameras.push(CameraCard {
                name: camera_name,
                files,
                skipped,
            });
        }
    }

    Ok(Manifest {
        scanned_at: Utc::now().to_rfc3339(),
        corpus_root: corpus_root.display().to_string(),
        cameras,
    })
}

fn collect_files(
    card_root: &Path,
    dir: &Path,
    out: &mut Vec<FileEntry>,
    skipped: &mut Vec<SkippedFile>,
) -> Result<(), CoreError> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = std::fs::metadata(&path)?;
        if meta.is_dir() {
            collect_files(card_root, &path, out, skipped)?;
        } else if meta.is_file() {
            let rel = path
                .strip_prefix(card_root)
                .expect("path is always under card root");
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if let Some(reason) = skip_reason(&path, &meta) {
                skipped.push(SkippedFile {
                    rel_path: rel_str,
                    reason,
                });
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let kind = FileKind::from_extension(ext);
            let folder_role = detect_role(&rel_str);
            let blake3 = hash_file(&path)?;
            out.push(FileEntry {
                rel_path: rel_str,
                size_bytes: meta.len(),
                blake3,
                kind,
                folder_role,
            });
        }
    }
    Ok(())
}

/// Returns `Some(reason)` for files the scanner should skip (hidden or system).
///
/// On Windows, checks the HIDDEN (0x2) and SYSTEM (0x4) file-attribute bits.
/// On all platforms, also skips dot-files (Unix hidden-by-convention).
fn skip_reason(path: &Path, meta: &std::fs::Metadata) -> Option<SkipReason> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const HIDDEN: u32 = 0x2;
        const SYSTEM: u32 = 0x4;
        let attrs = meta.file_attributes();
        match (attrs & HIDDEN != 0, attrs & SYSTEM != 0) {
            (true, true) => return Some(SkipReason::HiddenAndSystem),
            (true, false) => return Some(SkipReason::Hidden),
            (false, true) => return Some(SkipReason::System),
            _ => {}
        }
    }
    #[cfg(not(windows))]
    let _ = meta;
    // Dot-files are hidden by convention on all platforms.
    if path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
    {
        return Some(SkipReason::Hidden);
    }
    None
}

fn hash_file(path: &Path) -> Result<String, CoreError> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

/// Infer folder role from the normalised relative path (forward slashes, any case).
pub fn detect_role(rel: &str) -> FolderRole {
    let u = rel.replace('\\', "/").to_ascii_uppercase();
    if u.contains("/M4ROOT/CLIP/") {
        FolderRole::Clip
    } else if u.contains("/M4ROOT/SUB/") {
        FolderRole::Proxy
    } else if u.contains("/BDMV/STREAM/") {
        FolderRole::Stream
    } else if u.contains("/BDMV/CLIPINF/") {
        FolderRole::ClipInfo
    } else if u.contains("/BDMV/PLAYLIST/") {
        FolderRole::Playlist
    } else if u.contains("/BDMV/") {
        FolderRole::BdmvRoot
    } else if u.contains("/DCIM/") || u.starts_with("DCIM/") {
        FolderRole::Dcim
    } else if u.contains("/MP_ROOT/") || u.starts_with("MP_ROOT/") {
        FolderRole::MpRoot
    } else {
        FolderRole::Unknown
    }
}

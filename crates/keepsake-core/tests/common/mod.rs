//! Shared helpers for catalog, XMP and rebuild integration tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use keepsake_core::catalog::{Blake3Hash, FileRole, NewFile, VolumeId, VolumeKind};
use keepsake_core::library::VolumeRecord;
use keepsake_core::relpath::{CaseSensitivity, RelPath};

pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

pub fn fixture(rel: &str) -> PathBuf {
    workspace_root().join("tests/fixtures").join(rel)
}

/// Write a file (creating folders) under `root`.
pub fn write_file(root: &Path, rel: &str, bytes: &[u8]) -> RelPath {
    let rp = RelPath::parse(rel).expect("valid rel path");
    let full = rp.to_fs_path(root);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(&full, bytes).unwrap();
    rp
}

/// Describe an existing file on disk as a `NewFile`.
pub fn new_file(root: &Path, volume: VolumeId, rel: &RelPath, role: FileRole) -> NewFile {
    let full = rel.to_fs_path(root);
    let bytes = std::fs::read(&full).unwrap();
    let meta = std::fs::metadata(&full).unwrap();
    let mtime = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    NewFile {
        volume_id: volume,
        rel_path: rel.clone(),
        role,
        size_bytes: bytes.len() as u64,
        blake3: Blake3Hash::of(&bytes),
        xxh3: None,
        mtime_ms: Some(mtime),
    }
}

pub fn volume_record(label: &str, case: CaseSensitivity, read_only: bool) -> VolumeRecord {
    VolumeRecord {
        id: VolumeId::new_random(),
        label: label.to_owned(),
        fs_id: format!("test-fs-{}", uuid::Uuid::new_v4()),
        kind: VolumeKind::External,
        case,
        read_only,
        last_mount: None,
    }
}

/// Copy a directory tree.
pub fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap() {
        let e = e.unwrap();
        let to = dst.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &to);
        } else {
            std::fs::copy(e.path(), to).unwrap();
        }
    }
}

/// All `.xmp` files under `dir`.
pub fn xmp_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !dir.exists() {
        return out;
    }
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(xmp_files(&p));
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("xmp")) {
            out.push(p);
        }
    }
    out
}

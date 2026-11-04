//! Sidecar file locations and atomic I/O.
//!
//! A sidecar sits beside the asset's original as `<name>.<ext>.xmp`. When the
//! original's volume is read-only (or offline) it goes to a mirrored tree in
//! the library instead: `<library>/sidecars/<volume-uuid>/<rel_path>.xmp`.
//! Lightroom's `<stem>.xmp` naming is read as a fallback but never written.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::{XmpError, XmpPacket};
use crate::catalog::{Blake3Hash, VolumeId};
use crate::relpath::{PathError, RelPath};

/// Sidecar extension, lowercase.
pub const EXT: &str = "xmp";

/// `<name>.<ext>.xmp` beside the original.
pub fn beside_rel(original: &RelPath) -> Result<RelPath, PathError> {
    original.with_appended_suffix(".xmp")
}

/// Lightroom-style `<stem>.xmp` in the same folder, if the name has an extension.
pub fn lightroom_rel(original: &RelPath) -> Option<RelPath> {
    let name = original.file_name();
    let (stem, _) = name.rsplit_once('.')?;
    if stem.is_empty() {
        return None;
    }
    let file = format!("{stem}.{EXT}");
    match original.parent() {
        Some(p) => p.join(&file).ok(),
        None => RelPath::parse(&file).ok(),
    }
}

/// True when `name` looks like an XMP sidecar.
pub fn is_sidecar_name(name: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case(EXT))
}

/// Root of the mirrored sidecar tree for one volume.
pub fn mirror_root(sidecars_dir: &Path, volume: VolumeId) -> PathBuf {
    sidecars_dir.join(volume.to_string())
}

/// A sidecar read from disk.
#[derive(Debug, Clone)]
pub struct LoadedSidecar {
    pub packet: XmpPacket,
    pub blake3: Blake3Hash,
    pub mtime_ms: i64,
}

fn mtime_ms(path: &Path) -> Result<i64, XmpError> {
    let m = std::fs::metadata(path)?.modified()?;
    let ms = match m.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_millis()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_millis()).unwrap_or(i64::MAX),
    };
    Ok(ms)
}

/// Read and parse a sidecar; `Ok(None)` when it does not exist.
pub fn read(path: &Path) -> Result<Option<LoadedSidecar>, XmpError> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let text = String::from_utf8(bytes.clone())
        .map_err(|_| XmpError::Parse(format!("{} is not UTF-8", path.display())))?;
    Ok(Some(LoadedSidecar {
        packet: XmpPacket::parse(&text)?,
        blake3: Blake3Hash::of(&bytes),
        mtime_ms: mtime_ms(path)?,
    }))
}

/// Write a sidecar atomically (temporary file in the same folder, then rename).
/// Returns the hash and modification time of the written file.
pub fn write_atomic(path: &Path, packet: &XmpPacket) -> Result<(Blake3Hash, i64), XmpError> {
    let dir = path
        .parent()
        .ok_or_else(|| XmpError::Parse(format!("{} has no parent folder", path.display())))?;
    std::fs::create_dir_all(dir)?;
    let bytes = packet.to_xml().into_bytes();
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
    let result = (|| -> Result<(), XmpError> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result?;
    Ok((Blake3Hash::of(&bytes), mtime_ms(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        let p = RelPath::parse("DCIM/100MSDCF/DSC00001.ARW").unwrap();
        assert_eq!(
            beside_rel(&p).unwrap().as_str(),
            "DCIM/100MSDCF/DSC00001.ARW.xmp"
        );
        assert_eq!(
            lightroom_rel(&p).unwrap().as_str(),
            "DCIM/100MSDCF/DSC00001.xmp"
        );
        assert!(is_sidecar_name("a.JPG.XMP"));
        assert!(!is_sidecar_name("a.jpg"));
        assert!(lightroom_rel(&RelPath::parse("noext").unwrap()).is_none());
    }
}

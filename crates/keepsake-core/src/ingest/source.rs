//! The source card: read-only by construction, plus the quick-diff hash.

use std::fs::{File, Metadata, OpenOptions};
use std::hash::Hasher as _;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use twox_hash::XxHash3_64;

use crate::relpath::RelPath;

/// Bytes hashed at each end of a file by [`quick_hash`].
pub const QUICK_SPAN: u64 = 1 << 20;

/// A source volume. It can open files for reading and nothing else: there is
/// deliberately no method that writes, renames, deletes or creates anything.
#[derive(Debug, Clone)]
pub struct SourceVolume {
    root: PathBuf,
}

impl SourceVolume {
    pub fn new(root: &Path) -> io::Result<Self> {
        let root = std::fs::canonicalize(root)?;
        if !root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} is not a folder", root.display()),
            ));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path(&self, rel: &RelPath) -> PathBuf {
        rel.to_fs_path(&self.root)
    }

    /// Open a file for reading only.
    pub fn open(&self, rel: &RelPath) -> io::Result<File> {
        OpenOptions::new().read(true).open(self.path(rel))
    }

    pub fn metadata(&self, rel: &RelPath) -> io::Result<Metadata> {
        std::fs::metadata(self.path(rel))
    }

    /// True when the card is still there.
    pub fn is_present(&self) -> bool {
        self.root.is_dir()
    }

    /// True when `path` lies inside this volume (a destination must not).
    pub fn contains(&self, path: &Path) -> bool {
        let p = super::sys::existing_ancestor(path)
            .and_then(|a| std::fs::canonicalize(a).ok())
            .unwrap_or_else(|| path.to_path_buf());
        p.starts_with(&self.root)
    }

    pub fn quick_hash(&self, rel: &RelPath) -> io::Result<u64> {
        let mut f = self.open(rel)?;
        let len = f.metadata()?.len();
        quick_hash_reader(&mut f, len)
    }
}

/// xxh3 of the length, the first MiB and the last MiB of a file.
pub fn quick_hash(path: &Path) -> io::Result<u64> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    quick_hash_reader(&mut f, len)
}

pub fn quick_hash_reader<R: Read + Seek>(r: &mut R, len: u64) -> io::Result<u64> {
    let mut h = XxHash3_64::default();
    h.write(&len.to_le_bytes());
    let mut buf = vec![0u8; QUICK_SPAN as usize];
    let head = len.min(QUICK_SPAN) as usize;
    r.seek(SeekFrom::Start(0))?;
    r.read_exact(&mut buf[..head])?;
    h.write(&buf[..head]);
    if len > QUICK_SPAN {
        let start = QUICK_SPAN.max(len - QUICK_SPAN);
        let n = (len - start) as usize;
        r.seek(SeekFrom::Start(start))?;
        r.read_exact(&mut buf[..n])?;
        h.write(&buf[..n]);
    }
    Ok(h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn qh(bytes: &[u8]) -> u64 {
        quick_hash_reader(&mut Cursor::new(bytes), bytes.len() as u64).unwrap()
    }

    #[test]
    fn quick_hash_covers_size_head_and_tail_only() {
        let mut a = vec![7u8; 5 << 20];
        let base = qh(&a);
        // A change in the middle is invisible (that is what BLAKE3 confirms).
        a[2 << 20] ^= 1;
        assert_eq!(qh(&a), base);
        // Head, tail and length are seen.
        a[10] ^= 1;
        assert_ne!(qh(&a), base);
        a[10] ^= 1;
        let last = a.len() - 1;
        a[last] ^= 1;
        assert_ne!(qh(&a), base);
        a[last] ^= 1;
        a.push(0);
        assert_ne!(qh(&a), base);
        // Small files are hashed whole; empty files work.
        assert_ne!(qh(b"abc"), qh(b"abd"));
        assert_eq!(qh(b""), qh(b""));
    }
}

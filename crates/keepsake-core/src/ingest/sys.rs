//! Small OS queries std does not offer: free space, which file system a path
//! is on, and reading a file past the OS cache.

use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_to_caller: *mut u64,
            total: *mut u64,
            total_free: *mut u64,
        ) -> i32;
        fn GetVolumePathNameW(file_name: *const u16, out: *mut u16, len: u32) -> i32;
    }

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn free_space(p: &Path) -> Option<u64> {
        let w = wide(p);
        let (mut free, mut total, mut total_free) = (0u64, 0u64, 0u64);
        // SAFETY: `w` is a NUL-terminated UTF-16 string that outlives the call;
        // the three out-pointers point to valid, writable u64s.
        let ok = unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut free, &mut total, &mut total_free) };
        (ok != 0).then_some(free)
    }

    pub fn volume_root(p: &Path) -> Option<String> {
        let w = wide(p);
        let mut out = vec![0u16; 1024];
        // SAFETY: `w` is NUL-terminated; `out` has `out.len()` writable u16s.
        let ok = unsafe { GetVolumePathNameW(w.as_ptr(), out.as_mut_ptr(), out.len() as u32) };
        if ok == 0 {
            return None;
        }
        let end = out.iter().position(|c| *c == 0).unwrap_or(out.len());
        Some(String::from_utf16_lossy(&out[..end]).to_lowercase())
    }
}

/// Free bytes available to this user on the file system holding `path`.
pub fn free_space(path: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        win::free_space(path)
    }
    #[cfg(not(windows))]
    {
        // `df -Pk` prints POSIX-format 1K blocks; column 4 is "Available".
        let out = std::process::Command::new("df")
            .arg("-Pk")
            .arg(path)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let line = text.lines().nth(1)?;
        let avail: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
        Some(avail * 1024)
    }
}

/// An identifier of the file system holding `path` (equal ⇔ same file system).
pub fn filesystem_id(path: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        win::volume_root(path)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path).ok().map(|m| m.dev().to_string())
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        None
    }
}

/// Nearest existing ancestor (a destination folder may not exist yet).
pub fn existing_ancestor(path: &Path) -> Option<PathBuf> {
    let mut p = path.to_path_buf();
    loop {
        if p.exists() {
            return Some(p);
        }
        if !p.pop() {
            return None;
        }
    }
}

const ALIGN: usize = 4096;
const CHUNK: usize = 1 << 20;

/// Open a file for reading that bypasses the OS cache where possible, so a
/// re-read checks what reached the disk. Falls back to a normal open.
fn open_uncached(path: &Path) -> io::Result<(File, bool)> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_NO_BUFFERING: u32 = 0x2000_0000;
        if let Ok(f) = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_NO_BUFFERING)
            .open(path)
        {
            return Ok((f, true));
        }
    }
    Ok((File::open(path)?, false))
}

/// Hash a file with BLAKE3, bypassing the OS cache where possible.
/// `tamper` may change the bytes read (fault injection).
pub fn hash_uncached(
    path: &Path,
    mut tamper: impl FnMut(&mut [u8], u64),
) -> io::Result<crate::catalog::Blake3Hash> {
    let (mut f, unbuffered) = open_uncached(path)?;
    // Unbuffered reads need a sector-aligned buffer and aligned lengths.
    let mut storage = vec![0u8; CHUNK + ALIGN];
    let off = if unbuffered {
        storage.as_ptr().align_offset(ALIGN).min(ALIGN)
    } else {
        0
    };
    let buf = &mut storage[off..off + CHUNK];
    let mut h = blake3::Hasher::new();
    let mut pos = 0u64;
    loop {
        let n = f.read(buf)?;
        if n == 0 {
            break;
        }
        tamper(&mut buf[..n], pos);
        h.update(&buf[..n]);
        pos += n as u64;
    }
    Ok(crate::catalog::Blake3Hash(*h.finalize().as_bytes()))
}

/// Flush a directory entry to disk after a rename (no-op on Windows, where
/// NTFS journals metadata and directories cannot be opened for flushing).
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        File::open(dir)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
        Ok(())
    }
}

//! Make a folder tree read-only *at the operating-system level* for a test.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// Removes the restriction when dropped.
pub struct Guard {
    path: PathBuf,
    /// Files present when the guard was taken (for diagnostics).
    pub files: Vec<PathBuf>,
}

fn all_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}

/// Windows: an ACL denying Everyone write data, append/create, write
/// attributes, write extended attributes and delete-child on the tree, plus
/// the specific DELETE right (`DE`) on every file. icacls' generic `W` and `D`
/// include SYNCHRONIZE and would block reading too, so they are not used.
/// No admin rights are needed for your own files.
///
/// Unix: remove write permission from every file and folder.
pub fn deny_writes(path: &Path) -> Guard {
    let files = all_files(path);
    #[cfg(windows)]
    {
        for ace in [
            "*S-1-1-0:(OI)(CI)(WD,AD,WA,WEA,DC)",
            "*S-1-1-0:(OI)(IO)(DE)",
        ] {
            let out = Command::new("icacls")
                .arg(path)
                .args(["/deny", ace, "/T", "/C", "/Q"])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "icacls: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
    #[cfg(unix)]
    {
        let out = Command::new("chmod")
            .args(["-R", "a-w"])
            .arg(path)
            .output()
            .unwrap();
        assert!(out.status.success());
    }
    Guard {
        path: path.to_path_buf(),
        files,
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        #[cfg(windows)]
        {
            let _ = Command::new("icacls")
                .arg(&self.path)
                .args(["/remove:d", "*S-1-1-0", "/T", "/C", "/Q"])
                .output();
        }
        #[cfg(unix)]
        {
            let _ = Command::new("chmod")
                .args(["-R", "u+w"])
                .arg(&self.path)
                .output();
        }
    }
}

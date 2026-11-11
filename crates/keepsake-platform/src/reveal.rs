//! Show a file in the system file manager, selected:
//!
//! - Windows: `explorer /select,"<path>"`.
//! - macOS: `open -R <path>` (Finder).
//! - Elsewhere: the folder holding it, with `xdg-open` (no selection).
//!
//! The file is only pointed at: nothing reads, changes or moves it.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use crate::PlatformError;

/// Shows files in the file manager.
pub trait FileReveal: Send + Sync {
    /// Open the folder holding `path` with `path` selected.
    fn reveal(&self, path: &Path) -> Result<(), PlatformError>;
}

/// The file manager's name for menus: "Explorer", "Finder".
pub fn file_manager_name() -> &'static str {
    if cfg!(windows) {
        "Explorer"
    } else if cfg!(target_os = "macos") {
        "Finder"
    } else {
        "file manager"
    }
}

/// The program to start and its arguments. On Windows the single argument
/// is passed as written (`raw_arg`): Explorer wants `/select,"path"`, not
/// the whole argument quoted.
pub fn reveal_command(path: &Path) -> (String, Vec<String>) {
    let p = path.to_string_lossy().into_owned();
    // Explorer does not understand verbatim (`\\?\`) paths.
    let p = p.strip_prefix(r"\\?\").map(str::to_owned).unwrap_or(p);
    if cfg!(windows) {
        // Windows paths cannot contain `"`, so this quoting is safe.
        ("explorer".into(), vec![format!("/select,\"{p}\"")])
    } else if cfg!(target_os = "macos") {
        ("open".into(), vec!["-R".into(), p])
    } else {
        let dir = Path::new(&p)
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or(p);
        ("xdg-open".into(), vec![dir])
    }
}

/// The real file manager.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemReveal;

impl FileReveal for SystemReveal {
    fn reveal(&self, path: &Path) -> Result<(), PlatformError> {
        if !path.is_file() {
            return Err(PlatformError::Os(format!(
                "{} is not available",
                path.display()
            )));
        }
        let (prog, args) = reveal_command(path);
        let mut cmd = Command::new(&prog);
        add_args(&mut cmd, &args);
        // Detached: Explorer's exit code is 1 even when it worked, so it is
        // not waited for.
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| PlatformError::Os(format!("could not start {prog}: {e}")))?;
        Ok(())
    }
}

#[cfg(windows)]
fn add_args(cmd: &mut Command, args: &[String]) {
    use std::os::windows::process::CommandExt;
    for a in args {
        cmd.raw_arg(a);
    }
}

#[cfg(not(windows))]
fn add_args(cmd: &mut Command, args: &[String]) {
    cmd.args(args);
}

/// Remembers what it was asked to show, for tests.
#[derive(Debug, Default)]
pub struct FakeReveal {
    pub shown: Mutex<Vec<PathBuf>>,
}

impl FileReveal for FakeReveal {
    fn reveal(&self, path: &Path) -> Result<(), PlatformError> {
        self.shown
            .lock()
            .map_err(|_| PlatformError::Os("poisoned".into()))?
            .push(path.to_path_buf());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_selects_the_file() {
        if cfg!(windows) {
            let (prog, args) = reveal_command(Path::new(r"\\?\E:\DCIM\100MSDCF\DSC 0001.JPG"));
            assert_eq!(prog, "explorer");
            assert_eq!(args, vec![r#"/select,"E:\DCIM\100MSDCF\DSC 0001.JPG""#]);
            assert_eq!(file_manager_name(), "Explorer");
        } else if cfg!(target_os = "macos") {
            let (prog, args) = reveal_command(Path::new("/Volumes/Card A/DCIM/DSC 0001.JPG"));
            assert_eq!(prog, "open");
            assert_eq!(args, vec!["-R", "/Volumes/Card A/DCIM/DSC 0001.JPG"]);
            assert_eq!(file_manager_name(), "Finder");
        } else {
            let (prog, args) = reveal_command(Path::new("/media/card/DCIM/a.JPG"));
            assert_eq!(prog, "xdg-open");
            assert_eq!(args, vec!["/media/card/DCIM"]);
        }
    }

    #[test]
    fn a_missing_file_is_not_shown() {
        let d = tempfile::tempdir().unwrap();
        assert!(SystemReveal.reveal(&d.path().join("gone.jpg")).is_err());
    }

    #[test]
    fn fake_records_and_leaves_the_file_alone() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("a.jpg");
        std::fs::write(&f, b"original").unwrap();
        let fake = FakeReveal::default();
        fake.reveal(&f).unwrap();
        assert_eq!(*fake.shown.lock().unwrap(), vec![f.clone()]);
        assert_eq!(std::fs::read(&f).unwrap(), b"original");
    }
}

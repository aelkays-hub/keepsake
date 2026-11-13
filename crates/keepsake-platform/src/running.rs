//! Is a program running? Used to warn when a file handed to a program that
//! is already open may not show (`keepsake_core::external`).
//!
//! - Windows: `tasklist /FI "IMAGENAME eq <file name>"`.
//! - Elsewhere: not asked (macOS hands files to a running app itself).

use std::path::Path;

/// Is a program with `path`'s file name running? `false` when it cannot be
/// told.
pub fn program_running(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            return false;
        };
        let Ok(out) = Command::new("tasklist")
            .args(["/FI", &format!("IMAGENAME eq {name}"), "/FO", "CSV", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
        else {
            return false;
        };
        let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
        text.contains(&format!("\"{}\"", name.to_lowercase()))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_program_that_is_not_running_is_said_not_to_be() {
        assert!(!program_running(Path::new(
            r"C:\nowhere\keepsake-not-a-program.exe"
        )));
    }

    #[cfg(windows)]
    #[test]
    fn this_test_program_is_running() {
        let me = std::env::current_exe().unwrap();
        assert!(program_running(&me), "{}", me.display());
    }
}

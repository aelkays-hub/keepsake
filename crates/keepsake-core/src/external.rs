//! Open a file in another application: the system default, or one set per
//! file type in settings (`[open.apps]`, e.g.
//! `arw = 'C:\Program Files\Editor\Editor.exe'`). No library is needed: the
//! OS launchers do it.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::library::LibraryError;

/// The program and arguments that open `path` (with `app` if given).
pub fn command_for(path: &Path, app: Option<&str>) -> (String, Vec<String>) {
    let p = plain(path);
    match app.map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) if cfg!(target_os = "macos") && (a.ends_with(".app") || !a.contains('/')) => {
            ("open".into(), vec!["-a".into(), a.into(), p])
        }
        Some(a) => (a.into(), vec![p]),
        None if cfg!(windows) => ("explorer".into(), vec![p]),
        None if cfg!(target_os = "macos") => ("open".into(), vec![p]),
        None => ("xdg-open".into(), vec![p]),
    }
}

/// The path as other programs understand it: Windows' verbatim prefix
/// removed (`\\?\E:\a` → `E:\a`, and a network share `\\?\UNC\srv\x` →
/// `\\srv\x`; dropping only `\\?\` there would leave `UNC\srv\x`, which no
/// program can open). A share mapped to a drive letter is given by its
/// letter (`Z:\x`): some programs (Topaz Photo) open files by letter only.
pub fn plain(path: &Path) -> String {
    plain_with(path, &mapped_drives())
}

/// [`plain`] with these mapped drives: `(letter path "Z:", its share
/// "\\srv\x")`.
pub fn plain_with(path: &Path, mapped: &[(String, String)]) -> String {
    let s = path.to_string_lossy();
    let unc = if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        return rest.to_owned();
    } else {
        s.into_owned()
    };
    for (letter, share) in mapped {
        let n = share.len();
        if unc.len() >= n
            && unc.is_char_boundary(n)
            && unc[..n].eq_ignore_ascii_case(share)
            && (unc.len() == n || unc[n..].starts_with('\\'))
        {
            return format!("{letter}{}", &unc[n..]);
        }
    }
    unc
}

/// The network shares mapped to a drive letter on this computer, as
/// `("Z:", "\\srv\share")`: a mapped letter's real path is its share.
pub fn mapped_drives() -> Vec<(String, String)> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for c in b'D'..=b'Z' {
        let letter = format!("{}:", c as char);
        let Ok(real) = std::fs::canonicalize(format!("{letter}\\")) else {
            continue;
        };
        let real = real.to_string_lossy().into_owned();
        if let Some(rest) = real.strip_prefix(r"\\?\UNC\") {
            let share = format!(r"\\{}", rest.trim_end_matches('\\'));
            out.push((letter, share));
        }
    }
    out
}

/// Programs that, when already running, do not show a file handed to a
/// new launch (seen with Topaz Photo, 2026-11-13): matched in the
/// program's file name, ignoring case.
pub const IGNORE_FILES_WHEN_OPEN: [&str; 1] = ["topaz"];

/// Does `app` (its path) ignore a file handed to it while it is open?
pub fn ignores_files_when_open(app: &str) -> bool {
    let name = Path::new(app)
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    IGNORE_FILES_WHEN_OPEN.iter().any(|p| name.contains(p))
}

/// What the item menu says after opening a file with `name`: that it
/// opened, or, when the program was already open and ignores files handed
/// to it then, what to do.
pub fn opened_line(name: &str, already_open_and_ignores: bool) -> String {
    if already_open_and_ignores {
        format!(
            "{name} was already open, so it may not show this photo: close {name} and choose Open in again."
        )
    } else {
        format!("Opened with {name}")
    }
}

/// Launch it, detached (the app keeps running after Keepsake). The file is
/// only handed over; Keepsake does not change it.
pub fn open(path: &Path, app: Option<&str>) -> Result<(), LibraryError> {
    if !path.is_file() {
        return Err(LibraryError::Invalid(format!(
            "{} is not available",
            path.display()
        )));
    }
    launch(command_for(path, app))
}

/// Show a folder in Explorer or Finder (e.g. the log folder).
pub fn open_folder(dir: &Path) -> Result<(), LibraryError> {
    if !dir.is_dir() {
        return Err(LibraryError::Invalid(format!(
            "{} does not exist",
            dir.display()
        )));
    }
    launch(command_for(dir, None))
}

fn launch((prog, args): (String, Vec<String>)) -> Result<(), LibraryError> {
    let command = || {
        let mut cmd = Command::new(&prog);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd
    };
    let mut cmd = command();
    outlive_app(&mut cmd);
    if cmd.spawn().is_ok() {
        return Ok(());
    }
    // Breaking away can be refused (Keepsake was started inside a job that
    // does not allow it): start it plainly then.
    command()
        .spawn()
        .map_err(|e| LibraryError::Invalid(format!("could not start {prog}: {e}")))?;
    Ok(())
}

/// A program the user opens stays open when Keepsake closes: on Windows it
/// leaves the job that ends Keepsake's helpers with the app
/// (`keepsake_platform::close_helpers_with_app`).
fn outlive_app(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
        cmd.creation_flags(CREATE_BREAKAWAY_FROM_JOB);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topaz_is_said_to_ignore_files_while_open_and_photoshop_is_not() {
        assert!(ignores_files_when_open(
            r"C:\Program Files\Topaz Labs LLC\Topaz Photo\Topaz Photo.exe"
        ));
        assert!(!ignores_files_when_open(
            r"C:\Program Files\Adobe\Adobe Photoshop 2026\Photoshop.exe"
        ));
        assert_eq!(opened_line("Photoshop", false), "Opened with Photoshop");
        assert!(opened_line("Topaz Photo", true).starts_with("Topaz Photo was already open"));
    }

    #[test]
    fn a_share_is_given_by_its_drive_letter_when_mapped() {
        let mapped = [("Z:".to_owned(), r"\\MYCLOUD-92LNT0\aelkays".to_owned())];
        let p = Path::new(r"\\?\UNC\MYCLOUD-92LNT0\aelkays\Photos-OldAcer\2018\DSC08634.ARW");
        assert_eq!(
            plain_with(p, &mapped),
            r"Z:\Photos-OldAcer\2018\DSC08634.ARW"
        );
        // Case does not matter; another share stays a share.
        let q = Path::new(r"\\?\UNC\mycloud-92lnt0\AELKAYS\a.jpg");
        assert_eq!(plain_with(q, &mapped), r"Z:\a.jpg");
        let other = Path::new(r"\\?\UNC\NAS\photos\a.jpg");
        assert_eq!(plain_with(other, &mapped), r"\\NAS\photos\a.jpg");
        // Only a whole share name: aelkays2 is not aelkays.
        let near = Path::new(r"\\?\UNC\MYCLOUD-92LNT0\aelkays2\a.jpg");
        assert_eq!(
            plain_with(near, &mapped),
            r"\\MYCLOUD-92LNT0\aelkays2\a.jpg"
        );
        assert_eq!(plain_with(Path::new(r"\\?\E:\a.jpg"), &mapped), r"E:\a.jpg");
        assert_eq!(plain_with(Path::new(r"E:\a.jpg"), &mapped), r"E:\a.jpg");
    }

    #[test]
    fn default_and_configured_apps() {
        let (f, plain) = if cfg!(windows) {
            (r"\\?\E:\DCIM\a.ARW", r"E:\DCIM\a.ARW")
        } else {
            ("/Volumes/C/a.ARW", "/Volumes/C/a.ARW")
        };
        let f = Path::new(f);
        let (prog, args) = command_for(f, None);
        assert_eq!(args.last().map(String::as_str), Some(plain));
        if cfg!(windows) {
            assert_eq!(prog, "explorer");
            assert_eq!(
                command_for(f, Some(r"C:\Apps\Edit.exe")),
                (r"C:\Apps\Edit.exe".to_string(), vec![plain.to_string()])
            );
        }
        if cfg!(target_os = "macos") {
            assert_eq!(prog, "open");
            assert_eq!(
                command_for(f, Some("Preview.app")).1,
                vec!["-a", "Preview.app", plain]
            );
            assert_eq!(
                command_for(f, Some("/usr/local/bin/edit")),
                ("/usr/local/bin/edit".to_string(), vec![plain.to_string()])
            );
        }
        assert_eq!(command_for(f, Some("  ")).0, prog, "blank means default");
    }

    #[test]
    fn network_share_paths_keep_their_double_backslash() {
        let share = Path::new(r"\\?\UNC\MYCLOUD\Photos\2018\DSC1.ARW");
        let (_, args) = command_for(share, Some(r"C:\Apps\Edit.exe"));
        assert_eq!(
            args.last().map(String::as_str),
            Some(r"\\MYCLOUD\Photos\2018\DSC1.ARW")
        );
        let (_, args) = command_for(share, None);
        assert_eq!(
            args.last().map(String::as_str),
            Some(r"\\MYCLOUD\Photos\2018\DSC1.ARW")
        );
        assert_eq!(plain(Path::new(r"\\srv\x\a.jpg")), r"\\srv\x\a.jpg");
    }

    #[test]
    fn missing_file_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(open(&d.path().join("gone.jpg"), None).is_err());
    }
}

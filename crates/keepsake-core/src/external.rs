//! Open a file in another application: the system default, or one set per
//! file type in settings (`[open.apps]`, e.g.
//! `arw = 'C:\Program Files\Editor\Editor.exe'`). No library is needed: the
//! OS launchers do it.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::library::LibraryError;

/// The program and arguments that open `path` (with `app` if given).
pub fn command_for(path: &Path, app: Option<&str>) -> (String, Vec<String>) {
    let p = path.to_string_lossy().into_owned();
    let p = p.strip_prefix(r"\\?\").map(str::to_owned).unwrap_or(p);
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

/// Launch it, detached (the app keeps running after Keepsake). The file is
/// only handed over; Keepsake does not change it.
pub fn open(path: &Path, app: Option<&str>) -> Result<(), LibraryError> {
    if !path.is_file() {
        return Err(LibraryError::Invalid(format!(
            "{} is not available",
            path.display()
        )));
    }
    let (prog, args) = command_for(path, app);
    Command::new(&prog)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| LibraryError::Invalid(format!("could not start {prog}: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn missing_file_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(open(&d.path().join("gone.jpg"), None).is_err());
    }
}

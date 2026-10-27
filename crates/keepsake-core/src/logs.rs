//! Local log files and crash reports. Nothing is ever sent anywhere: the
//! files stay in the log folder until the user copies a diagnostic report.
//!
//! | Platform | Folder |
//! |---|---|
//! | Windows | `%LOCALAPPDATA%\keepsake\logs` |
//! | macOS | `~/Library/Logs/keepsake` |
//!
//! `KEEPSAKE_LOG_DIR` overrides it (tests).
//!
//! - One log file per run, `keepsake-<time>-<pid>.log`. The newest
//!   [`KEEP_LOGS`] are kept, and each stops growing at [`MAX_LOG_BYTES`].
//! - A panic writes `crash-<time>-<pid>.txt` with the message, place,
//!   thread and backtrace. Once the user has seen it, it is renamed
//!   `*.seen.txt` (never deleted by Keepsake except by the same pruning).

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Log files kept (the oldest beyond this are removed at start-up).
pub const KEEP_LOGS: usize = 10;
/// Crash files kept.
pub const KEEP_CRASHES: usize = 20;
/// A log file stops growing here.
pub const MAX_LOG_BYTES: u64 = 10 << 20;

/// The log folder (not created).
pub fn log_dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("KEEPSAKE_LOG_DIR") {
        return Some(PathBuf::from(d));
    }
    if cfg!(target_os = "macos") {
        return dirs::home_dir().map(|h| h.join("Library").join("Logs").join("keepsake"));
    }
    crate::paths::data_dir().map(|d| d.join("logs"))
}

fn stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/// Files in `dir` whose names start with `prefix`, oldest first (by name,
/// which starts with the time).
fn named(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(prefix))
        })
        .collect();
    v.sort();
    v
}

fn prune(dir: &Path, prefix: &str, keep: usize) {
    let all = named(dir, prefix);
    let extra = all.len().saturating_sub(keep);
    for p in all.into_iter().take(extra) {
        let _ = std::fs::remove_file(p);
    }
}

/// The log file of this run. Writes go to the file until it reaches
/// [`MAX_LOG_BYTES`]; then one notice is written and the rest dropped.
pub struct LogFile {
    file: File,
    path: PathBuf,
    written: u64,
    full: bool,
}

impl LogFile {
    /// Create this run's log file in `dir` (created if needed) and remove
    /// the oldest beyond [`KEEP_LOGS`].
    pub fn create(dir: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("keepsake-{}-{}.log", stamp(), std::process::id()));
        let file = File::create(&path)?;
        prune(dir, "keepsake-", KEEP_LOGS);
        Ok(Self {
            file,
            path,
            written: 0,
            full: false,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Write for LogFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.full {
            return Ok(buf.len());
        }
        if self.written + buf.len() as u64 > MAX_LOG_BYTES {
            self.full = true;
            let _ = self
                .file
                .write_all(b"\n[log file full: later messages are not kept]\n");
            return Ok(buf.len());
        }
        self.file.write_all(buf)?;
        self.written += buf.len() as u64;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// The newest log file, if any.
pub fn latest_log(dir: &Path) -> Option<PathBuf> {
    named(dir, "keepsake-").pop()
}

/// The last `n` lines of a text file.
pub fn tail(path: &Path, n: usize) -> String {
    let text = std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Write panics to crash files in `dir` (as well as the usual message).
/// `app` names the program in the report (`keepsake-app`, `keepsake`).
pub fn install_crash_hook(dir: PathBuf, app: &'static str) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let thread = std::thread::current();
        let text = format!(
            "{app} {} crashed at {}\nOS: {} {}\nThread: {}\n{info}\n\nBacktrace:\n{bt}\n",
            crate::version(),
            chrono::Local::now().to_rfc3339(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            thread.name().unwrap_or("unnamed"),
        );
        if std::fs::create_dir_all(&dir).is_ok() {
            let p = dir.join(format!("crash-{}-{}.txt", stamp(), std::process::id()));
            let _ = std::fs::write(p, text);
            prune(&dir, "crash-", KEEP_CRASHES);
        }
        previous(info);
    }));
}

/// A crash file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Crash {
    pub path: PathBuf,
    pub text: String,
    pub seen: bool,
}

/// The newest crash report, if any.
pub fn latest_crash(dir: &Path) -> Option<Crash> {
    let path = named(dir, "crash-").pop()?;
    let seen = path.to_string_lossy().ends_with(".seen.txt");
    let text = std::fs::read_to_string(&path).ok()?;
    Some(Crash { path, text, seen })
}

/// Mark every crash report as seen (the start-up notice stops showing).
pub fn mark_crashes_seen(dir: &Path) -> io::Result<()> {
    for p in named(dir, "crash-") {
        let name = p.to_string_lossy().into_owned();
        if let Some(stem) = name.strip_suffix(".txt").filter(|s| !s.ends_with(".seen")) {
            std::fs::rename(&p, format!("{stem}.seen.txt"))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_file_per_run_capped_and_pruned() {
        let d = tempfile::tempdir().unwrap();
        for i in 0..12 {
            std::fs::write(
                d.path().join(format!("keepsake-20200101-0000{i:02}-1.log")),
                "old",
            )
            .unwrap();
        }
        std::fs::write(d.path().join("notes.txt"), "mine").unwrap();
        let mut f = LogFile::create(d.path()).unwrap();
        assert_eq!(named(d.path(), "keepsake-").len(), KEEP_LOGS);
        assert!(d.path().join("notes.txt").is_file(), "other files stay");
        assert_eq!(latest_log(d.path()).as_deref(), Some(f.path()));
        writeln!(f, "line one").unwrap();
        writeln!(f, "line two").unwrap();
        f.flush().unwrap();
        assert_eq!(tail(f.path(), 1), "line two");
        // Past the cap: one notice, then nothing more.
        f.written = MAX_LOG_BYTES - 3;
        writeln!(f, "too long for the rest").unwrap();
        writeln!(f, "dropped").unwrap();
        f.flush().unwrap();
        let text = std::fs::read_to_string(f.path()).unwrap();
        assert!(text.contains("[log file full"));
        assert!(!text.contains("dropped"));
    }

    #[test]
    fn crash_files_are_found_and_marked_seen() {
        let d = tempfile::tempdir().unwrap();
        assert!(latest_crash(d.path()).is_none());
        std::fs::write(d.path().join("crash-20260101-000000-7.txt"), "boom").unwrap();
        let c = latest_crash(d.path()).unwrap();
        assert!(!c.seen && c.text == "boom");
        mark_crashes_seen(d.path()).unwrap();
        let c = latest_crash(d.path()).unwrap();
        assert!(c.seen && c.text == "boom");
        mark_crashes_seen(d.path()).unwrap();
        assert_eq!(named(d.path(), "crash-").len(), 1, "not renamed twice");
    }
}

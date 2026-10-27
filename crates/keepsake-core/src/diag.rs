//! Diagnostics: which sidecar tools were found and whether they run.
//! Used by the app's start-up check, `--self-check`, and the diagnostic
//! report. Nothing here writes anything.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use serde::Serialize;

use crate::render::RenderTools;

/// One sidecar tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolStatus {
    pub name: &'static str,
    pub path: PathBuf,
    /// The file exists.
    pub found: bool,
    /// The first line of its version output, if it ran.
    pub version: Option<String>,
}

impl ToolStatus {
    pub fn ok(&self) -> bool {
        self.found && self.version.is_some()
    }
}

/// `(name, path, arguments, text the output must contain)`.
fn checks(t: &RenderTools) -> [(&'static str, PathBuf, &'static [&'static str], &'static str); 4] {
    [
        ("exiftool", t.exiftool.clone(), &["-ver"], "."),
        (
            "ffprobe",
            t.ffprobe.clone(),
            &["-version"],
            "ffprobe version",
        ),
        ("ffmpeg", t.ffmpeg.clone(), &["-version"], "ffmpeg version"),
        // dcraw_emu prints its usage, starting "dcraw_emu: almost complete dcraw
        // emulator". (It has no version option; the version is pinned in setup.)
        ("dcraw_emu", t.dcraw.clone(), &[], "dcraw emulator"),
    ]
}

/// Find and run each tool (a few seconds at most).
pub fn tools(t: &RenderTools) -> Vec<ToolStatus> {
    checks(t)
        .into_iter()
        .map(|(name, path, args, want)| {
            let found = std::fs::metadata(&path).is_ok_and(|m| m.is_file());
            let version = found
                .then(|| {
                    let mut cmd = Command::new(&path);
                    cmd.args(args);
                    let out =
                        crate::probe::process::run(name, cmd, Duration::from_secs(20)).ok()?;
                    let text = format!(
                        "{}{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    text.lines()
                        .find(|l| l.contains(want))
                        .map(|l| l.trim().to_owned())
                })
                .flatten();
            ToolStatus {
                name,
                path,
                found,
                version,
            }
        })
        .collect()
}

// ── Diagnostic report ───────────────────────────────────────────────────────

/// What goes into a diagnostic report.
pub struct ReportInput<'a> {
    /// `keepsake-app` or `keepsake`.
    pub app: &'a str,
    pub library: Option<(&'a crate::catalog::Catalog, &'a crate::library::Library)>,
    pub cache: Option<&'a crate::render::Cache>,
    pub tools: &'a RenderTools,
    pub log_dir: Option<&'a std::path::Path>,
}

/// Lines of the current log included.
pub const REPORT_LOG_LINES: usize = 200;

fn os_version() -> String {
    let (prog, args): (&str, &[&str]) = if cfg!(windows) {
        ("cmd", &["/c", "ver"])
    } else if cfg!(target_os = "macos") {
        ("sw_vers", &["-productVersion"])
    } else {
        ("uname", &["-sr"])
    };
    let mut cmd = Command::new(prog);
    cmd.args(args);
    crate::probe::process::run("os version", cmd, Duration::from_secs(5))
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

/// Replace `needle` with `~` wherever it appears, ignoring ASCII case.
fn replace_ignoring_case(text: &str, needle: &str) -> String {
    if needle.is_empty() {
        return text.to_owned();
    }
    let hay = text.as_bytes();
    let n = needle.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    let mut i = 0;
    while i + n.len() <= hay.len() {
        if hay[i..i + n.len()].eq_ignore_ascii_case(n)
            && text.is_char_boundary(i)
            && text.is_char_boundary(i + n.len())
        {
            out.push_str(&text[last..i]);
            out.push('~');
            i += n.len();
            last = i;
        } else {
            i += 1;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// Replace the home folder with `~` everywhere in `text` (any letter case,
/// either slash, doubled backslashes as in TOML, and without `\\?\`).
pub fn redact_home(text: &str, home: &std::path::Path) -> String {
    let h = home.to_string_lossy();
    let h = h
        .strip_prefix(r"\\?\")
        .unwrap_or(&h)
        .trim_end_matches(['\\', '/']);
    if h.len() < 3 {
        return text.to_owned();
    }
    let mut out = text.replace(r"\\?\", "");
    for form in [h.replace('\\', r"\\"), h.to_owned(), h.replace('\\', "/")] {
        out = replace_ignoring_case(&out, &form);
    }
    out
}

/// A plain-text report for a bug report. The user copies it to the
/// clipboard; nothing is sent.
pub fn report(input: &ReportInput<'_>) -> String {
    use std::fmt::Write as _;
    let mut r = String::new();
    let _ = writeln!(r, "Keepsake diagnostic report");
    let _ = writeln!(r, "==========================");
    let _ = writeln!(r, "Program: {} {}", input.app, crate::version());
    let _ = writeln!(
        r,
        "System: {} {} ({})",
        std::env::consts::OS,
        std::env::consts::ARCH,
        os_version()
    );
    let _ = writeln!(r, "Time: {}", chrono::Local::now().to_rfc3339());
    let _ = writeln!(r, "\nTools:");
    for t in tools(input.tools) {
        let state = match (&t.version, t.found) {
            (Some(v), _) => v.clone(),
            (None, true) => "found but does not run".into(),
            (None, false) => "MISSING".into(),
        };
        let _ = writeln!(r, "  {:<10} {}  [{}]", t.name, state, t.path.display());
    }
    match input.library {
        Some((cat, lib)) => library_section(&mut r, cat, lib),
        None => {
            let _ = writeln!(r, "\nLibrary: none open");
        }
    }
    if let Some(c) = input.cache {
        let _ = writeln!(r, "Cache: {}", c.root().display());
    }
    let config_path = crate::settings::Settings::path().ok();
    let _ = writeln!(
        r,
        "\nSettings ({}):",
        config_path
            .as_deref()
            .map_or_else(|| "?".into(), |p| p.display().to_string())
    );
    let config = config_path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| "(none)".into());
    for l in config.lines() {
        let _ = writeln!(r, "  {l}");
    }
    if let Some(dir) = input.log_dir {
        let _ = writeln!(r, "\nLog folder: {}", dir.display());
        match crate::logs::latest_crash(dir) {
            Some(c) => {
                let _ = writeln!(
                    r,
                    "\nLast crash ({}{}):\n{}",
                    c.path.file_name().unwrap_or_default().to_string_lossy(),
                    if c.seen { ", seen" } else { "" },
                    c.text.trim_end()
                );
            }
            None => {
                let _ = writeln!(r, "No crash reports.");
            }
        }
        if let Some(log) = crate::logs::latest_log(dir) {
            let _ = writeln!(
                r,
                "\nLast {REPORT_LOG_LINES} log lines ({}):\n{}",
                log.file_name().unwrap_or_default().to_string_lossy(),
                crate::logs::tail(&log, REPORT_LOG_LINES)
            );
        }
    }
    match dirs::home_dir() {
        Some(h) => redact_home(&r, &h),
        None => r,
    }
}

fn library_section(r: &mut String, cat: &crate::catalog::Catalog, lib: &crate::library::Library) {
    use crate::catalog::JobState as S;
    use std::fmt::Write as _;
    let _ = writeln!(r, "\nLibrary: {}", lib.root().display());
    let show =
        |v: Result<String, crate::catalog::CatalogError>| v.unwrap_or_else(|e| format!("? ({e})"));
    let _ = writeln!(
        r,
        "  schema {}, {} assets, {} files",
        show(cat.schema_version().map(|v| v.to_string())),
        show(cat.read(|tx| tx.assets().count()).map(|n| n.to_string())),
        show(cat.read(|tx| tx.files().count()).map(|n| n.to_string())),
    );
    for v in cat.read(|tx| tx.volumes().list()).unwrap_or_default() {
        let online = crate::verify::mount_of(cat, lib, v.id).is_some();
        let _ = writeln!(
            r,
            "  volume {} ({}): {}",
            v.label,
            v.id,
            if online { "connected" } else { "not connected" }
        );
    }
    if let Ok(c) = cat.read(|tx| tx.jobs().counts()) {
        let _ = writeln!(
            r,
            "  render jobs: {} queued, {} running, {} done, {} failed",
            c.total(S::Queued),
            c.total(S::Running),
            c.total(S::Done),
            c.total(S::Failed)
        );
    }
    if let Ok((n, bytes)) = cat.read(|tx| tx.renditions().totals()) {
        let _ = writeln!(r, "  renditions: {n} ({bytes} bytes)");
    }
    for h in crate::history::list(lib).unwrap_or_default().iter().take(5) {
        let _ = writeln!(
            r,
            "  {:?} {} at {}: {} files, {} problems{}",
            h.kind,
            h.label,
            h.finished_ms,
            h.files,
            h.problems,
            if h.cancelled { ", stopped" } else { "" }
        );
    }
}

#[cfg(test)]
mod report_tests {
    use super::*;

    #[test]
    fn the_home_folder_becomes_a_tilde() {
        let home = std::path::Path::new(r"C:\Users\Ann Lee");
        let t = r"lib C:\Users\Ann Lee\Pictures, c:\users\ann lee\x, C:/Users/Ann Lee/y, \\?\C:\Users\Ann Lee\z, toml 'C:\\Users\\Ann Lee\\c'";
        assert_eq!(
            redact_home(t, home),
            r"lib ~\Pictures, ~\x, ~/y, ~\z, toml '~\\c'"
        );
        let mac = std::path::Path::new("/Users/ann");
        assert_eq!(redact_home("/Users/ann/Pictures", mac), "~/Pictures");
    }

    #[test]
    fn a_report_without_a_library_still_says_what_it_can() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("crash-20260101-000000-1.txt"), "boom at x.rs").unwrap();
        let mut log = crate::logs::LogFile::create(d.path()).unwrap();
        use std::io::Write as _;
        writeln!(log, "INFO started").unwrap();
        log.flush().unwrap();
        let t = RenderTools {
            exiftool: d.path().join("none"),
            ffprobe: d.path().join("none"),
            ffmpeg: d.path().join("none"),
            dcraw: d.path().join("none"),
        };
        let r = report(&ReportInput {
            app: "keepsake-app",
            library: None,
            cache: None,
            tools: &t,
            log_dir: Some(d.path()),
        });
        assert!(
            r.contains(&format!("keepsake-app {}", crate::version())),
            "{r}"
        );
        assert!(r.contains("exiftool   MISSING"), "{r}");
        assert!(r.contains("Library: none open"));
        assert!(r.contains("boom at x.rs"));
        assert!(r.contains("INFO started"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_tools_are_reported_not_fatal() {
        let d = tempfile::tempdir().unwrap();
        let t = RenderTools {
            exiftool: d.path().join("nope.exe"),
            ffprobe: d.path().join("nope.exe"),
            ffmpeg: d.path().join("nope.exe"),
            dcraw: d.path().join("nope.exe"),
        };
        let s = tools(&t);
        assert_eq!(s.len(), 4);
        assert!(s.iter().all(|x| !x.found && !x.ok()));
    }
}

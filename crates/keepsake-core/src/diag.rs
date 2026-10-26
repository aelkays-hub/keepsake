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

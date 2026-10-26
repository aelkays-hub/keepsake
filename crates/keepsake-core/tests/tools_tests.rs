//! Finding the sidecar tools, as the installed app does.

mod common;

use std::path::Path;

use keepsake_core::probe::tools::{bundled, set_bundled};
use keepsake_core::probe::ToolPaths;
use keepsake_core::render::RenderTools;

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

fn touch(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, b"x").unwrap();
}

#[test]
fn the_installed_apps_resource_folder_is_used() {
    // An installed app has no vendor/ folder and no KEEPSAKE_VENDOR: it
    // registers its resource folder, laid out like vendor/.
    std::env::remove_var("KEEPSAKE_VENDOR");
    std::env::remove_var("KEEPSAKE_EXIFTOOL");
    std::env::remove_var("KEEPSAKE_FFPROBE");
    std::env::remove_var("KEEPSAKE_FFMPEG");
    std::env::remove_var("KEEPSAKE_DCRAW");
    let d = tempfile::tempdir().unwrap();
    let res = d.path().join("Keepsake");
    touch(&res.join(exe("ffprobe")));
    touch(&res.join(exe("ffmpeg")));
    touch(&res.join("exiftool").join(exe("exiftool")));
    touch(&res.join("libraw").join(exe("dcraw_emu")));
    set_bundled(res.clone());
    assert_eq!(bundled(), Some(res.as_path()));
    let t = RenderTools::discover();
    assert_eq!(t.ffprobe, res.join(exe("ffprobe")));
    assert_eq!(t.ffmpeg, res.join(exe("ffmpeg")));
    assert_eq!(t.exiftool, res.join("exiftool").join(exe("exiftool")));
    assert_eq!(t.dcraw, res.join("libraw").join(exe("dcraw_emu")));
    // KEEPSAKE_VENDOR still wins (tests and overrides).
    std::env::set_var("KEEPSAKE_VENDOR", d.path().join("other"));
    assert_eq!(
        ToolPaths::discover().ffprobe,
        d.path().join("other").join(exe("ffprobe"))
    );
    std::env::remove_var("KEEPSAKE_VENDOR");
}

#[test]
fn every_vendored_tool_runs() {
    if common::media::tools().is_none() {
        return;
    }
    let vendor = common::workspace_root().join("vendor");
    let t = RenderTools::from_probe(&ToolPaths::in_vendor(&vendor));
    let status = keepsake_core::diag::tools(&t);
    for s in &status {
        if s.name == "dcraw_emu" && !s.found && cfg!(target_os = "macos") {
            continue; // macOS CI has no vendor binaries
        }
        assert!(s.ok(), "{s:?}");
    }
    let lr = status.iter().find(|s| s.name == "dcraw_emu").unwrap();
    if lr.found {
        assert!(
            lr.version.as_deref().unwrap().contains("dcraw emulator"),
            "{lr:?}"
        );
    }
}

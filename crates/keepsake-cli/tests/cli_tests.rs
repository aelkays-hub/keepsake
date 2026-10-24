//! Integration tests: run the real `keepsake` binary against fixture cards.
//!
//! Each test gets its own temp folder and config folder (`KEEPSAKE_CONFIG_DIR`),
//! so your real default library is never touched.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

fn fixture(rel: &str) -> PathBuf {
    workspace().join("tests/fixtures").join(rel)
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let to = dst.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &to);
        } else {
            std::fs::copy(e.path(), to).unwrap();
        }
    }
}

/// Are ExifTool and ffprobe available (needed for capture dates)?
fn tools() -> bool {
    let v = workspace().join("vendor");
    let et = v.join("exiftool").join(if cfg!(windows) {
        "exiftool.exe"
    } else {
        "exiftool"
    });
    let ok = std::fs::metadata(&et).is_ok_and(|m| m.len() > 0)
        && Command::new(&et)
            .arg("-ver")
            .output()
            .is_ok_and(|o| o.status.success());
    // macOS CI sets KEEPSAKE_SKIP_MEDIA_TOOLS (no vendored tools there yet).
    if !ok
        && std::env::var_os("CI").is_some()
        && std::env::var_os("KEEPSAKE_SKIP_MEDIA_TOOLS").is_none()
    {
        panic!("CI must provide vendor/ tools");
    }
    ok
}

struct Env {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    card: PathBuf,
}

impl Env {
    /// A temp world with a fixture card (DCIM + XAVC + a path with spaces and accents).
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let card = root.join("card");
        copy_dir(&fixture("dcim"), &card);
        copy_dir(&fixture("xavc"), &card);
        copy_dir(&fixture("names"), &card);
        Self {
            _tmp: tmp,
            root,
            card,
        }
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_keepsake"));
        c.args(args)
            .env("KEEPSAKE_CONFIG_DIR", self.p("config"))
            .env("KEEPSAKE_VENDOR", workspace().join("vendor"))
            .env_remove("KEEPSAKE_LIBRARY")
            .env_remove("RUST_LOG");
        c
    }

    fn run(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    /// Run with `--json`, expect `code`, parse stdout.
    fn json(&self, args: &[&str], code: i32) -> Value {
        let mut a = vec!["--json"];
        a.extend_from_slice(args);
        let out = self.run(&a);
        assert_eq!(
            out.status.code(),
            Some(code),
            "{args:?}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "{args:?}: not JSON ({e}): {}",
                String::from_utf8_lossy(&out.stdout)
            )
        })
    }

    fn s(&self, rel: &str) -> String {
        self.p(rel).display().to_string()
    }

    fn init(&self) {
        self.json(&["init", &self.s("lib")], 0);
    }

    /// Ingest the card to dest-a and dest-b.
    fn ingest(&self, probe: bool) -> Value {
        let (card, a, b) = (
            self.card.display().to_string(),
            self.s("dest-a"),
            self.s("dest-b"),
        );
        let mut args = vec![
            "ingest",
            &card,
            "--dest",
            &a,
            "--dest",
            &b,
            "--unsafe-allow-same-disk",
        ];
        if !probe {
            args.push("--no-probe");
        }
        self.json(&args, 0)
    }
}

fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    if !root.exists() {
        return out;
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.insert(
                    p.strip_prefix(root).unwrap().to_path_buf(),
                    std::fs::read(&p).unwrap(),
                );
            }
        }
    }
    out
}

// ── init ─────────────────────────────────────────────────────────────────────

#[test]
fn init_creates_a_library_and_makes_it_the_default() {
    let e = Env::new();
    let v = e.json(&["init", &e.s("lib")], 0);
    assert_eq!(v["created"], true);
    assert!(e.p("lib/library.json").is_file());
    let cfg = std::fs::read_to_string(e.p("config/config.toml")).unwrap();
    assert!(cfg.contains("default_library"), "{cfg}");
    // Again: adopts the existing library.
    assert_eq!(e.json(&["init", &e.s("lib")], 0)["created"], false);
    // Other commands now find it without --library.
    e.json(&["ingest", "--list"], 0);
    // Human output.
    let out = e.run(&["init", &e.s("lib2")]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("Created library"));
}

#[test]
fn missing_library_is_a_clear_error() {
    let e = Env::new();
    let out = e.run(&["search", "x"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("keepsake init"));
    let out = e.run(&["--json", "search", "x"]);
    let err: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert!(err["error"].as_str().unwrap().contains("no library"));
    // --library and KEEPSAKE_LIBRARY both work.
    e.json(&["init", &e.s("lib")], 0);
    std::fs::remove_file(e.p("config/config.toml")).unwrap();
    e.json(&["--library", &e.s("lib"), "ingest", "--list"], 0);
    let out = e
        .cmd(&["--json", "ingest", "--list"])
        .env("KEEPSAKE_LIBRARY", e.p("lib"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
}

// ── ingest ───────────────────────────────────────────────────────────────────

#[test]
fn ingest_dry_run_writes_nothing() {
    let e = Env::new();
    e.init();
    let before_lib = tree(&e.p("lib"));
    let before_card = tree(&e.card);
    let card = e.card.display().to_string();
    let v = e.json(
        &[
            "ingest",
            &card,
            "--dest",
            &e.s("dest-a"),
            "--dest",
            &e.s("dest-b"),
            "--dry-run",
            "--no-probe",
            "--unsafe-allow-same-disk",
        ],
        0,
    );
    let assets = v["plan"]["assets"].as_array().unwrap();
    assert_eq!(assets.len(), 5);
    let names: Vec<&str> = assets
        .iter()
        .flat_map(|a| a["files"].as_array().unwrap())
        .map(|f| f["dest"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"0000-undated/0000-00-00/C0001M01.XML"),
        "{names:?}"
    );
    assert!(names.contains(&"0000-undated/0000-00-00/Fête du lac 01.JPG"));
    assert!(v["plan"]["space"][0]["bytes_needed"].as_u64().unwrap() > 0);
    // Nothing written: no destination folders, no session, library unchanged.
    assert!(!e.p("dest-a").exists() && !e.p("dest-b").exists());
    assert_eq!(tree(&e.p("lib")), before_lib);
    assert_eq!(tree(&e.card), before_card);
    assert_eq!(
        e.json(&["ingest", "--list"], 0).as_array().unwrap().len(),
        0
    );
    // The same-disk rule applies to dry runs too.
    let out = e.run(&[
        "ingest",
        &card,
        "--dest",
        &e.s("dest-a"),
        "--dest",
        &e.s("dest-b"),
        "--dry-run",
        "--no-probe",
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("same disk"));
}

#[test]
fn ingest_two_destinations_status_and_list() {
    let e = Env::new();
    e.init();
    let card_before = tree(&e.card);
    let v = e.ingest(false);
    assert_eq!(v["report"]["safe_to_format"], true, "{v}");
    assert_eq!(v["report"]["by_state"]["catalogued"], 5);
    assert_eq!(tree(&e.card), card_before);
    let s = e.json(&["ingest", "--status"], 0);
    assert_eq!(s["report"]["safe_to_format"], true);
    assert_eq!(s["unfinished_temporaries"].as_array().unwrap().len(), 0);
    assert_eq!(
        e.json(&["ingest", "--list"], 0).as_array().unwrap().len(),
        1
    );
    // Human output leads with the verdict.
    let card = e.card.display().to_string();
    let out = e.run(&[
        "ingest",
        &card,
        "--dest",
        &e.s("dest-a"),
        "--dest",
        &e.s("dest-b"),
        "--no-probe",
        "--unsafe-allow-same-disk",
    ]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("SAFE TO FORMAT"));
}

#[test]
fn ingest_one_destination_exits_2() {
    let e = Env::new();
    e.init();
    let card = e.card.display().to_string();
    let v = e.json(
        &[
            "ingest",
            &card,
            "--dest",
            &e.s("only"),
            "--no-probe",
            "--unsafe-allow-same-disk",
        ],
        2,
    );
    assert_eq!(v["report"]["safe_to_format"], false);
    assert_eq!(
        v["report"]["not_safe"][0],
        "only one destination was chosen"
    );
    // Bad arguments are errors (exit 1).
    assert_eq!(e.run(&["ingest", &card]).status.code(), Some(1));
    assert_eq!(
        e.run(&["ingest", "--resume"]).status.code(),
        Some(1),
        "nothing unfinished"
    );
}

// ── verify ───────────────────────────────────────────────────────────────────

#[test]
fn verify_reports_missing_and_changed_files() {
    let e = Env::new();
    e.init();
    e.ingest(false);
    let ok = e.json(&["verify"], 0);
    assert!(ok["issues"].as_array().unwrap().is_empty());
    let day = "0000-undated/0000-00-00";
    std::fs::remove_file(e.p(&format!("dest-a/{day}/DSC00002.JPG"))).unwrap();
    let f = e.p(&format!("dest-b/{day}/C0001.MP4"));
    let mut bytes = std::fs::read(&f).unwrap();
    bytes[0] ^= 1;
    std::fs::write(&f, bytes).unwrap();
    let v = e.json(&["verify"], 2);
    let issues = v["issues"].as_array().unwrap();
    assert_eq!(issues.len(), 2, "{v}");
    assert!(issues.iter().any(|i| i["kind"] == "missing"
        && i["path"].as_str().unwrap().ends_with("DSC00002.JPG")
        && i["volume"] == "dest-a"));
    assert!(issues.iter().any(|i| i["kind"] == "changed"
        && i["path"].as_str().unwrap().ends_with("C0001.MP4")
        && i["volume"] == "dest-b"));
    // One volume only.
    let v = e.json(&["verify", "--volume", "dest-a"], 2);
    assert_eq!(v["issues"].as_array().unwrap().len(), 1);
    assert_eq!(v["volumes"].as_array().unwrap().len(), 1);
    // Human output and unknown volume.
    let out = e.run(&["verify"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("MISSING") && text.contains("CHANGED"),
        "{text}"
    );
    assert_eq!(
        e.run(&["verify", "--volume", "nope"]).status.code(),
        Some(1)
    );
}

// ── search ───────────────────────────────────────────────────────────────────

#[test]
fn search_by_words_dates_camera_and_type() {
    if !tools() {
        eprintln!("skipping: no vendor tools");
        return;
    }
    let e = Env::new();
    e.init();
    e.ingest(true);
    let count = |args: &[&str]| e.json(args, 0)["count"].as_u64().unwrap();
    assert_eq!(count(&["search", "C0002"]), 1);
    assert_eq!(count(&["search", "fête"]), 1, "accents and spaces in names");
    assert_eq!(count(&["search", "--type", "video"]), 2);
    assert_eq!(count(&["search", "--type", "photo"]), 3);
    // The XAVC clips' XML says 2024-01-15 (+09:00).
    assert_eq!(
        count(&["search", "--from", "2024-01-15", "--to", "2024-01-15"]),
        2
    );
    assert_eq!(count(&["search", "--from", "2024-01-16"]), 0);
    assert_eq!(count(&["search", "--camera", "7m4", "--type", "video"]), 2);
    assert_eq!(count(&["search", "--camera", "canon"]), 0);
    let v = e.json(&["search", "C0001"], 0);
    let r = &v["results"][0];
    assert_eq!(r["captured"], "2024-01-15T10:30:00.000+09:00");
    assert_eq!(r["camera"], "ILCE-7M4");
    assert!(r["file"].as_str().unwrap().ends_with("C0001.MP4"));
    // Errors.
    assert_eq!(
        e.run(&["search", "--from", "15/01/2024"]).status.code(),
        Some(1)
    );
    assert_eq!(e.run(&["search", "--type", "audio"]).status.code(), Some(1));
    assert_eq!(e.run(&["search"]).status.code(), Some(1));
    let out = e.run(&["search", "C0001"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("1 match"));
}

// ── shift-time ───────────────────────────────────────────────────────────────

#[test]
fn shift_time_writes_xmp_and_catalog_only() {
    if !tools() {
        eprintln!("skipping: no vendor tools");
        return;
    }
    let e = Env::new();
    e.init();
    e.ingest(true);
    let clip = e.p("dest-a/2024/2024-01-15/C0001.MP4");
    let xmp = e.p("dest-a/2024/2024-01-15/C0001.MP4.xmp");
    let clip_bytes = std::fs::read(&clip).unwrap();
    let xmp_before = std::fs::read_to_string(&xmp).unwrap();
    // Dry run changes nothing.
    let v = e.json(
        &[
            "shift-time",
            &clip.display().to_string(),
            "--by",
            "+01:00:00",
            "--dry-run",
        ],
        0,
    );
    assert_eq!(v["changed"][0]["after"], "2024-01-15T11:30:00.000+09:00");
    assert_eq!(std::fs::read_to_string(&xmp).unwrap(), xmp_before);
    // Real shift.
    let v = e.json(
        &[
            "shift-time",
            &clip.display().to_string(),
            "--by",
            "+01:00:00",
        ],
        0,
    );
    assert_eq!(v["changed"].as_array().unwrap().len(), 1);
    assert_eq!(v["by_seconds"], 3600);
    let xmp_after = std::fs::read_to_string(&xmp).unwrap();
    assert!(
        xmp_after.contains("2024-01-15T11:30:00.000+09:00"),
        "{xmp_after}"
    );
    assert!(xmp_after.contains("<keepsake:CaptureShiftSeconds>3600</keepsake:CaptureShiftSeconds>"));
    // The copy on the other drive got the same sidecar change; originals untouched.
    assert!(
        std::fs::read_to_string(e.p("dest-b/2024/2024-01-15/C0001.MP4.xmp"))
            .unwrap()
            .contains("11:30:00")
    );
    assert_eq!(std::fs::read(&clip).unwrap(), clip_bytes);
    let s = e.json(&["search", "C0001"], 0);
    assert_eq!(s["results"][0]["captured"], "2024-01-15T11:30:00.000+09:00");
    // Selection by folder and by filters; shifts add up.
    let v = e.json(&["shift-time", &e.s("dest-a/2024"), "--by", "-0:30"], 0);
    assert_eq!(v["changed"].as_array().unwrap().len(), 2);
    let v = e.json(&["shift-time", "--type", "video", "--by", "+0:30"], 0);
    assert_eq!(v["changed"].as_array().unwrap().len(), 2);
    assert_eq!(
        e.json(&["search", "C0001"], 0)["results"][0]["captured"],
        "2024-01-15T11:30:00.000+09:00"
    );
    // Errors: nothing selected, bad offset, unknown path.
    assert_eq!(
        e.run(&["shift-time", "--by", "+1:00"]).status.code(),
        Some(1)
    );
    assert_eq!(
        e.run(&["shift-time", &clip.display().to_string(), "--by", "1h"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        e.run(&["shift-time", &e.s("nowhere.jpg"), "--by", "+1:00"])
            .status
            .code(),
        Some(1)
    );
}

// ── backup ───────────────────────────────────────────────────────────────────

#[test]
fn backup_is_incremental_and_verified() {
    let e = Env::new();
    e.init();
    e.ingest(false);
    let v = e.json(&["backup", &e.s("backup")], 0);
    let copied = v["files_copied"].as_u64().unwrap();
    assert_eq!(copied, 11, "{v}"); // 3 DCIM + 1 + 3 + 3 XAVC + 1 name fixture
    assert!(e.p("backup/0000-undated/0000-00-00/C0001M01.XML").is_file());
    assert!(
        e.p("backup/0000-undated/0000-00-00/C0001.MP4.xmp")
            .is_file(),
        "sidecars beside the backup copies"
    );
    assert!(e.p("backup/.keepsake-library/catalog.db").is_file());
    assert!(e.p("backup/.keepsake-library/volumes.json").is_file());
    // The snapshot is a valid catalog with the same assets.
    let snap =
        keepsake_core::catalog::Catalog::open(&e.p("backup/.keepsake-library/catalog.db")).unwrap();
    assert_eq!(snap.read(|tx| tx.assets().count()).unwrap(), 5);
    drop(snap);
    // Second run: nothing to copy.
    let v = e.json(&["backup", &e.s("backup")], 0);
    assert_eq!(v["files_copied"], 0);
    assert_eq!(v["files_already_there"], 11);
    // New media: only that is copied.
    std::fs::write(
        e.card.join("DCIM/100MSDCF/DSC00003.JPG"),
        b"a brand new photo",
    )
    .unwrap();
    e.ingest(false);
    let v = e.json(&["backup", &e.s("backup")], 0);
    assert_eq!(v["files_copied"], 1, "{v}");
    // Every copy on the backup verifies.
    assert!(e.json(&["verify", "--volume", "backup"], 0)["issues"]
        .as_array()
        .unwrap()
        .is_empty());
    // A damaged library copy is never propagated: damage DSC00001.JPG on
    // both library drives (the catalog still has its original hash), then
    // back up to a fresh drive.
    for d in ["dest-a", "dest-b", "backup"] {
        let p = e.p(&format!("{d}/0000-undated/0000-00-00/DSC00001.JPG"));
        let mut b = std::fs::read(&p).unwrap();
        b[0] ^= 1;
        std::fs::write(&p, b).unwrap();
    }
    let v = e.json(&["backup", &e.s("backup2")], 2);
    assert_eq!(v["failed"].as_array().unwrap().len(), 1, "{v}");
    assert!(v["failed"][0][0]
        .as_str()
        .unwrap()
        .ends_with("DSC00001.JPG"));
    assert!(
        v["failed"][0][1]
            .as_str()
            .unwrap()
            .contains("differs from the catalog"),
        "{v}"
    );
    assert!(!e.p("backup2/0000-undated/0000-00-00/DSC00001.JPG").exists());
    assert!(e
        .p("backup2/0000-undated/0000-00-00/DSC00002.JPG")
        .is_file());
    // Refuses to back up into a library volume.
    assert_eq!(
        e.run(&["backup", &e.s("dest-a/inner")]).status.code(),
        Some(1)
    );
}

// ── rebuild-catalog ──────────────────────────────────────────────────────────

#[test]
fn rebuild_catalog_restores_search_results() {
    let e = Env::new();
    e.init();
    e.ingest(false);
    let before = e.json(&["search", "DSC"], 0);
    let v = e.json(&["rebuild-catalog", "--fresh", "--no-probe"], 0);
    assert_eq!(v["fresh"], true);
    assert_eq!(v["assets"], 5);
    let vols: Vec<&str> = v["volumes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["label"].as_str().unwrap())
        .collect();
    assert!(
        !vols.contains(&"Card"),
        "cards are import sources, not library volumes: {vols:?}"
    );
    let after = e.json(&["search", "DSC"], 0);
    assert_eq!(before["count"], after["count"]);
    let ids = |v: &Value| {
        let mut x: Vec<String> = v["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["asset"].to_string())
            .collect();
        x.sort();
        x
    };
    assert_eq!(
        ids(&before),
        ids(&after),
        "asset ids survive a rebuild (from sidecars)"
    );
}

// ── probe ────────────────────────────────────────────────────────────────────

#[test]
fn probe_reads_a_card() {
    if !tools() {
        eprintln!("skipping: no vendor tools");
        return;
    }
    let e = Env::new();
    let v = e.json(&["probe", &e.card.display().to_string()], 0);
    assert_eq!(v["groups"].as_array().unwrap().len(), 5);
}

// ── Ctrl-C: finish the current file, then stop cleanly ──────────────────────

#[cfg(windows)]
fn interrupt(child: &std::process::Child) -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GenerateConsoleCtrlEvent(event: u32, group: u32) -> i32;
    }
    const CTRL_BREAK_EVENT: u32 = 1;
    // SAFETY: plain Win32 call with integer arguments.
    unsafe { GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, child.id()) != 0 }
}

#[cfg(unix)]
fn interrupt(child: &std::process::Child) -> bool {
    Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn ctrl_c_finishes_the_current_file_then_stops_and_resumes() {
    let e = Env::new();
    e.init();
    // A card big enough that the import is still running when we interrupt it.
    let card = e.p("bigcard");
    for i in 0..8u8 {
        let mut data = vec![i; 48 << 20];
        data[..8].copy_from_slice(&[i, 1, 2, 3, 4, 5, 6, 7]);
        let p = card.join(format!("DCIM/100MSDCF/DSC0010{i}.JPG"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }
    let mut c = e.cmd(&[
        "--json",
        "ingest",
        &card.display().to_string(),
        "--dest",
        &e.s("dest-a"),
        "--dest",
        &e.s("dest-b"),
        "--no-probe",
        "--unsafe-allow-same-disk",
    ]);
    c.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        c.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
    let child = c.spawn().unwrap();
    // Wait until a copy is in flight (a hidden temporary appears).
    let started = std::time::Instant::now();
    let in_flight = loop {
        let partial = ["dest-a", "dest-b"].iter().any(|d| {
            let dir = e.p(&format!("{d}/0000-undated/0000-00-00"));
            std::fs::read_dir(dir)
                .map(|rd| {
                    rd.flatten().any(|x| {
                        x.file_name()
                            .to_string_lossy()
                            .contains(".keepsake-partial-")
                    })
                })
                .unwrap_or(false)
        });
        if partial {
            break true;
        }
        if started.elapsed() > std::time::Duration::from_secs(60) {
            break false;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    };
    assert!(in_flight, "the import never started copying");
    if !interrupt(&child) {
        eprintln!("skipping: this console cannot deliver Ctrl-Break/SIGINT to a child");
        let _ = child.wait_with_output();
        return;
    }
    let out = child.wait_with_output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(130),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["cancelled"], true);
    // No partial file left anywhere; every file under a final name is complete.
    for d in ["dest-a", "dest-b"] {
        for (rel, bytes) in tree(&e.p(d)) {
            let name = rel.file_name().unwrap().to_string_lossy().into_owned();
            assert!(
                !name.contains(".keepsake-partial-"),
                "leftover {}",
                rel.display()
            );
            if name.ends_with(".JPG") {
                assert_eq!(
                    bytes,
                    std::fs::read(card.join("DCIM/100MSDCF").join(&name)).unwrap(),
                    "{name} is incomplete"
                );
            }
        }
    }
    let copied = tree(&e.p("dest-a"))
        .keys()
        .filter(|k| k.to_string_lossy().ends_with(".JPG"))
        .count();
    assert!(copied >= 1, "the file in flight was finished");
    assert!(copied < 8, "it stopped before the end");
    // Resume finishes the job.
    let v = e.json(&["ingest", "--resume", "--no-probe"], 0);
    assert_eq!(v["report"]["safe_to_format"], true, "{v}");
}

// ── Devices: watch, card, preset ─────────────────────────────────────────────

impl Env {
    /// Run with `--json`, expect exit 1 and a JSON error on stderr.
    fn fail(&self, args: &[&str]) -> String {
        let mut a = vec!["--json"];
        a.extend_from_slice(args);
        let out = self.run(&a);
        let err = String::from_utf8_lossy(&out.stderr).into_owned();
        assert_eq!(out.status.code(), Some(1), "{args:?}: {err}");
        let v: Value = serde_json::from_str(err.trim()).unwrap_or_else(|e| panic!("{e}: {err}"));
        v["error"].as_str().unwrap_or_default().to_owned()
    }

    /// Run with `--json`, expect `code`, parse stdout as JSON Lines.
    fn json_lines(&self, args: &[&str], code: i32) -> Vec<Value> {
        let mut a = vec!["--json"];
        a.extend_from_slice(args);
        let out = self.run(&a);
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert_eq!(
            out.status.code(),
            Some(code),
            "{args:?}\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        stdout
            .lines()
            .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l}")))
            .collect()
    }

    /// A script of fake device events for `watch --fake-events`.
    fn events(&self, name: &str, events: &[Value]) -> String {
        let mut text = String::from("# fake device events\n");
        for e in events {
            text.push_str(&e.to_string());
            text.push('\n');
        }
        let p = self.p(name);
        std::fs::write(&p, text).unwrap();
        p.display().to_string()
    }

    fn arrived(&self, serial: &str) -> Value {
        serde_json::json!({
            "event": "arrived",
            "mount": self.card,
            "identity": { "fs_serial": serial, "fs_type": "exFAT", "label": "Untitled",
                          "capacity": 64_000_000_000u64, "removable": true }
        })
    }

    fn removed(&self) -> Value {
        serde_json::json!({ "event": "removed", "mount": self.card })
    }

    fn watch(&self, script: &str, code: i32) -> Vec<Value> {
        self.json_lines(
            &[
                "watch",
                "--fake-events",
                script,
                "--auto-ingest",
                "--no-probe",
                "--unsafe-allow-same-disk",
            ],
            code,
        )
    }
}

fn kinds(lines: &[Value]) -> Vec<String> {
    lines
        .iter()
        .map(|l| l["event"].as_str().unwrap_or("?").to_owned())
        .collect()
}

fn first<'a>(lines: &'a [Value], kind: &str) -> &'a Value {
    lines
        .iter()
        .find(|l| l["event"] == kind)
        .unwrap_or_else(|| panic!("no {kind} in {lines:#?}"))
}

#[test]
fn presets_and_cards_are_managed_from_the_cli() {
    let e = Env::new();
    e.init();
    let (a, b) = (e.s("dest-a"), e.s("dest-b"));
    // Destinations must exist (the drive must be connected).
    e.fail(&["preset", "save", "Trip", "--dest", &a, "--dest", &b]);
    std::fs::create_dir_all(e.p("dest-a")).unwrap();
    std::fs::create_dir_all(e.p("dest-b")).unwrap();
    e.fail(&["preset", "save", "Trip", "--dest", &a, "--dest", &a]);
    e.fail(&[
        "preset",
        "save",
        "Trip",
        "--dest",
        &a,
        "--template",
        "{nope}",
    ]);
    let saved = e.json(&["preset", "save", "Trip", "--dest", &a, "--dest", &b], 0);
    assert_eq!(saved["preset"]["destinations"].as_array().unwrap().len(), 2);
    assert_eq!(
        e.json(&["preset", "list"], 0)["presets"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    assert!(e.json(&["card", "list"], 0)["cards"]
        .as_array()
        .unwrap()
        .is_empty());
    // A manual import of a card is remembered.
    let card = e.card.display().to_string();
    let r = e.json(
        &[
            "ingest",
            &card,
            "--dest",
            &a,
            "--dest",
            &b,
            "--no-probe",
            "--unsafe-allow-same-disk",
            "--card-serial",
            "1A2B",
        ],
        0,
    );
    assert_eq!(r["report"]["safe_to_format"], true);
    let list = e.json(&["card", "list"], 0);
    let c = &list["cards"][0];
    assert_eq!(c["label"], "Card");
    assert!(c["last_import_ms"].is_i64() && !c["trusted"].as_bool().unwrap());

    e.fail(&["card", "trust", "Card", "--preset", "nope"]);
    e.fail(&["card", "trust", "nobody", "--preset", "Trip"]);
    let t = e.json(&["card", "trust", "card", "--preset", "trip"], 0);
    assert_eq!(t["card"]["trusted"], true);
    assert_eq!(t["card"]["preset"], "Trip");
    // A preset in use cannot be deleted.
    e.fail(&["preset", "delete", "Trip"]);
    e.json(&["card", "rename", "Card", "Holiday"], 0);
    let id = list["cards"][0]["id"].as_str().unwrap()[..8].to_owned();
    e.json(&["card", "untrust", &id], 0);
    e.json(&["preset", "delete", "Trip"], 0);
    e.fail(&["preset", "delete", "Trip"]);
    let f = e.json(&["card", "forget", "holiday"], 0);
    assert_eq!(f["forgotten"], list["cards"][0]["id"]);
    assert!(e.json(&["card", "list"], 0)["cards"]
        .as_array()
        .unwrap()
        .is_empty());
    // The media is still catalogued.
    assert!(
        e.json(&["search", "--type", "photo"], 0)["count"]
            .as_u64()
            .unwrap()
            > 0
    );
}

#[test]
fn watch_reports_cards_and_imports_trusted_ones() {
    let e = Env::new();
    e.init();
    std::fs::create_dir_all(e.p("dest-a")).unwrap();
    std::fs::create_dir_all(e.p("dest-b")).unwrap();
    let script = e.events(
        "in-out-in.jsonl",
        &[e.arrived("1A2B"), e.removed(), e.arrived("1A2B")],
    );

    // Not trusted yet: reported, never imported.
    let lines = e.watch(&script, 0);
    assert_eq!(
        kinds(&lines),
        [
            "watching",
            "card_inserted",
            "card_removed",
            "card_inserted",
            "stopped"
        ]
    );
    let ins = first(&lines, "card_inserted");
    assert_eq!(ins["known"], false);
    assert_eq!(ins["label"], "Untitled");
    let items = ins["items"].as_u64().unwrap();
    assert!(items >= 4, "{ins}");
    assert_eq!(ins["new_items"], items);
    assert_eq!(
        lines[3]["known"], true,
        "the second insert is the same card"
    );
    assert!(std::fs::read_dir(e.p("dest-a")).unwrap().next().is_none());

    // Trusted with a preset: imported on insert, then nothing new.
    let (a, b) = (e.s("dest-a"), e.s("dest-b"));
    e.json(
        &["preset", "save", "Two drives", "--dest", &a, "--dest", &b],
        0,
    );
    e.json(&["card", "trust", "Untitled", "--preset", "Two drives"], 0);
    let lines = e.watch(&script, 0);
    let k = kinds(&lines);
    assert_eq!(
        &k[..3],
        ["watching", "card_inserted", "auto_ingest_started"],
        "{k:?}"
    );
    assert!(k.contains(&"auto_ingest_progress".to_owned()));
    let done = first(&lines, "auto_ingest_finished");
    assert_eq!(done["report"]["safe_to_format"], true, "{done}");
    assert_eq!(done["new_items_left"], 0);
    assert_eq!(first(&lines, "auto_ingest_started")["preset"], "Two drives");
    let after_removal: Vec<&Value> = lines
        .iter()
        .skip_while(|l| l["event"] != "card_removed")
        .collect();
    assert_eq!(after_removal[1]["event"], "card_inserted");
    assert_eq!(after_removal[1]["new_items"], 0);
    assert_eq!(after_removal[2]["event"], "auto_ingest_skipped");
    assert_eq!(after_removal[2]["reason"], "nothing new on the card");
    assert!(std::fs::read_dir(e.p("dest-a")).unwrap().next().is_some());

    // A new photo on the card: one new item, imported.
    std::fs::write(e.card.join("DCIM/100MSDCF/DSC00099.JPG"), b"a new photo").unwrap();
    let once = e.events("in.jsonl", &[e.arrived("1A2B")]);
    let lines = e.watch(&once, 0);
    assert_eq!(first(&lines, "card_inserted")["new_items"], 1);
    assert_eq!(first(&lines, "auto_ingest_finished")["new_items_left"], 0);

    // A different card (another serial) in the same slot: new and untrusted.
    let other = e.events("other.jsonl", &[e.arrived("9999")]);
    let lines = e.watch(&other, 0);
    assert_eq!(first(&lines, "card_inserted")["known"], false);
    assert!(!kinds(&lines).contains(&"auto_ingest_started".to_owned()));

    // Text output.
    let out = e.run(&["watch", "--fake-events", &once]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("Card inserted: Untitled")
            && text.contains("0 new item(s) since last import"),
        "{text}"
    );
}

#[test]
fn watch_skips_auto_ingest_when_a_destination_is_missing() {
    let e = Env::new();
    e.init();
    std::fs::create_dir_all(e.p("dest-a")).unwrap();
    std::fs::create_dir_all(e.p("dest-b")).unwrap();
    let script = e.events("in.jsonl", &[e.arrived("1A2B")]);
    e.watch(&script, 0);
    let (a, b) = (e.s("dest-a"), e.s("dest-b"));
    e.json(&["preset", "save", "P", "--dest", &a, "--dest", &b], 0);
    e.json(&["card", "trust", "Untitled", "--preset", "P"], 0);
    std::fs::remove_dir(e.p("dest-b")).unwrap(); // the drive is unplugged
    let lines = e.watch(&script, 0);
    let s = first(&lines, "auto_ingest_skipped");
    assert!(s["reason"].as_str().unwrap().contains("not found"), "{s}");
    assert!(!kinds(&lines).contains(&"auto_ingest_started".to_owned()));
}

// ── Renders ─────────────────────────────────────────────────────────────────

fn ffmpeg() -> PathBuf {
    workspace().join("vendor").join(if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    })
}

/// Media tools (ExifTool, ffprobe, ffmpeg) present? Required on CI except
/// where `KEEPSAKE_SKIP_MEDIA_TOOLS` is set.
fn media_tools() -> bool {
    let ok = tools()
        && Command::new(ffmpeg())
            .arg("-version")
            .output()
            .is_ok_and(|o| o.status.success());
    if !ok
        && std::env::var_os("CI").is_some()
        && std::env::var_os("KEEPSAKE_SKIP_MEDIA_TOOLS").is_none()
    {
        panic!("CI must provide ffmpeg in vendor/");
    }
    ok
}

fn gen(args: &[&str]) {
    let out = Command::new(ffmpeg())
        .args(["-y", "-v", "error"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

impl Env {
    /// Point the rendition cache at this test's temp folder.
    fn cache_here(&self) {
        let p = self.p("config").join("config.toml");
        let text = std::fs::read_to_string(&p).unwrap_or_default();
        let mut t: toml::Table = toml::from_str(&text).unwrap();
        let render = t
            .entry("render")
            .or_insert_with(|| toml::Value::Table(toml::Table::new()));
        render
            .as_table_mut()
            .unwrap()
            .insert("cache_dir".into(), toml::Value::String(self.s("cache")));
        std::fs::write(&p, toml::to_string(&t).unwrap()).unwrap();
    }

    /// A folder of real media: two photos and a clip.
    fn real_media(&self, dir: &str) -> PathBuf {
        let d = self.p(dir);
        std::fs::create_dir_all(&d).unwrap();
        for (name, size) in [("a.jpg", "2000x1500"), ("b.jpg", "1200x1600")] {
            let src = format!("testsrc2=size={size}");
            let out = d.join(name);
            gen(&[
                "-f",
                "lavfi",
                "-i",
                &src,
                "-frames:v",
                "1",
                "-q:v",
                "1",
                out.to_str().unwrap(),
            ]);
        }
        let out = d.join("c.mp4");
        gen(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1280x720:rate=30",
            "-f",
            "lavfi",
            "-i",
            "sine=sample_rate=48000",
            "-t",
            "2",
            "-c:v",
            "libopenh264",
            "-c:a",
            "aac",
            "-shortest",
            out.to_str().unwrap(),
        ]);
        d
    }
}

#[test]
fn renders_queue_run_status_show_evict_and_requeue() {
    if !media_tools() {
        return;
    }
    let e = Env::new();
    e.init();
    e.cache_here();
    let media = e.real_media("media");
    let (m, a, b) = (media.display().to_string(), e.s("dest-a"), e.s("dest-b"));
    // An import queues its assets as recent imports.
    e.json(
        &[
            "ingest",
            &m,
            "--dest",
            &a,
            "--dest",
            &b,
            "--no-probe",
            "--unsafe-allow-same-disk",
        ],
        0,
    );
    let st = e.json(&["renders", "status"], 0);
    let jobs = &st["status"]["jobs"];
    assert_eq!(jobs["queued_by_priority"]["recent"], 4, "{st}");
    assert_eq!(jobs["by_kind"]["render.stills"]["queued"], 3);
    assert_eq!(jobs["by_kind"]["render.proxy"]["queued"], 1);

    let lines = e.json_lines(&["renders", "run", "--workers", "2"], 0);
    let done = lines.iter().filter(|l| l["event"] == "done").count();
    assert_eq!(done, 4, "{lines:#?}");
    let fin = lines.last().unwrap();
    assert_eq!(fin["event"], "finished");
    assert_eq!(fin["failed"], 0);

    let st = e.json(&["renders", "status"], 0);
    assert_eq!(st["status"]["jobs"]["by_kind"]["render.stills"]["done"], 3);
    assert_eq!(st["status"]["cache"]["files"], 10, "3 x 3 stills + 1 proxy");
    assert!(e.p("cache").join("thumbnail").is_dir());

    let clip = e.p("dest-a").join("0000-undated/0000-00-00/c.mp4");
    let shown = e.json(&["renders", "show", clip.to_str().unwrap()], 0);
    let r = shown["renditions"].as_object().unwrap();
    assert!(
        r.contains_key("thumb-256") && r.contains_key("preview-2560"),
        "{shown}"
    );
    assert!(r.contains_key("proxy-720"), "{shown}");
    let thumb = PathBuf::from(r["thumb-256"].as_str().unwrap());
    assert!(thumb.is_file());

    // Evict everything (all media online), then the file on screen comes back.
    let ev = e.json(&["renders", "evict", "--max-bytes", "0"], 0);
    assert_eq!(ev["evict"]["removed"], 10);
    assert!(!thumb.exists());
    let q = e.json(
        &["renders", "enqueue", clip.to_str().unwrap(), "--visible"],
        0,
    );
    assert_eq!(q["queued"], 2, "{q}");
    let lines = e.json_lines(&["renders", "run", "--queued-only"], 0);
    assert_eq!(lines.iter().filter(|l| l["event"] == "done").count(), 2);
    assert!(thumb.is_file());
    assert_eq!(e.json(&["renders", "retry"], 0)["requeued"], 0);

    // Text status.
    let out = e.run(&["renders", "status"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("Renditions:") && text.contains("Cache:"),
        "{text}"
    );
}

#[test]
fn renders_enqueue_needs_a_selection_and_show_needs_a_catalogued_file() {
    let e = Env::new();
    e.init();
    e.cache_here();
    e.fail(&["renders", "enqueue"]);
    let nowhere = e.s("nowhere.jpg");
    std::fs::write(&nowhere, b"x").unwrap();
    e.fail(&["renders", "show", &nowhere]);
    let q = e.json(&["renders", "enqueue", "--all"], 0);
    assert_eq!(q["queued"], 0);
}

#[test]
fn renders_bench_reports_items_per_minute() {
    if !media_tools() {
        return;
    }
    let e = Env::new();
    let media = e.real_media("sample");
    let out = e.json(
        &[
            "renders",
            "bench",
            media.to_str().unwrap(),
            "--per-type",
            "2",
            "--videos",
            "1",
        ],
        0,
    );
    let rows = out["results"].as_array().unwrap();
    let kinds: Vec<&str> = rows.iter().map(|r| r["type"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"JPEG"), "{out}");
    assert!(kinds.contains(&"Video proxy (720p H.264)"), "{out}");
    for r in rows {
        assert!(r["items_per_minute"].as_f64().unwrap() > 0.0, "{r}");
    }
    assert!(out["machine"]["cpu"]
        .as_str()
        .is_some_and(|c| !c.is_empty()));
    // The sample folder is untouched.
    assert_eq!(std::fs::read_dir(&media).unwrap().count(), 3);
}

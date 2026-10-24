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
    if !ok && std::env::var_os("CI").is_some() {
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

//! Ingest: copy, verify, catalogue, resume, refuse, and survive faults.
//!
//! Every fault test ends with the same invariants: no file sits under a
//! final name unless its bytes equal the card's, the catalog never calls a
//! bad copy verified, and after a clean resume no temporary file is left.

mod common;

use std::collections::{BTreeMap, HashSet};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use common::{copy_dir, write_file};
use keepsake_core::catalog::{
    Blake3Hash, Catalog, FileRole, ImportAssetState, ImportDiff, ImportItemStatus, ImportSessionId,
    VolumeKind,
};
use keepsake_core::ingest::fault::{CrashMode, CrashPoint, Fault, FaultPlan};
use keepsake_core::ingest::template::{Template, DEFAULT_TEMPLATE};
use keepsake_core::ingest::{
    self, destination_volume, Destination, IngestError, IngestReport, PlanOptions, RunContext,
    Selection,
};
use keepsake_core::library::Library;

const BIG: &str = "DCIM/100MSDCF/DSC00009.JPG";
const BIG_LEN: usize = 3 * 1024 * 1024 + 123;

struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    lib: Library,
    card: PathBuf,
    dests: Vec<Destination>,
}

fn big_bytes() -> Vec<u8> {
    // Deterministic, non-repeating content.
    let mut x: u32 = 0x1234_5678;
    (0..BIG_LEN)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x & 0xFF) as u8
        })
        .collect()
}

fn world_at(root: &Path) -> World {
    let lib = Library::create(&root.join("library")).unwrap();
    let card = root.join("card");
    copy_dir(&common::fixture("xavc"), &card);
    copy_dir(&common::fixture("dcim"), &card);
    copy_dir(&common::fixture("names"), &card);
    write_file(&card, BIG, &big_bytes());
    let dests = vec![
        destination_volume(&lib, &root.join("dest-a"), "Drive A", VolumeKind::External).unwrap(),
        destination_volume(&lib, &root.join("dest-b"), "Drive B", VolumeKind::External).unwrap(),
    ];
    World {
        _tmp: tempfile::tempdir().unwrap(),
        root: root.to_path_buf(),
        lib,
        card,
        dests,
    }
}

fn world() -> (tempfile::TempDir, World) {
    let tmp = tempfile::tempdir().unwrap();
    let w = world_at(tmp.path());
    (tmp, w)
}

fn opts() -> PlanOptions {
    PlanOptions {
        template: Template::parse(DEFAULT_TEMPLATE).unwrap(),
        selection: Selection::NewOnly,
        event: None,
        // Tests keep everything in one temp folder, i.e. on one disk.
        allow_same_filesystem: true,
    }
}

fn plan(w: &World, cat: &mut Catalog, dests: &[Destination]) -> ImportSessionId {
    ingest::plan(cat, &w.lib, None, &w.card, "Test card", dests, &opts()).unwrap()
}

fn run_with(
    w: &World,
    cat: &mut Catalog,
    s: ImportSessionId,
    fault: &FaultPlan,
) -> Result<IngestReport, IngestError> {
    let cancel = AtomicBool::new(false);
    let mut progress = |_: &ingest::Progress| {};
    let mut ctx = RunContext {
        prober: None,
        fault,
        cancel: &cancel,
        progress: &mut progress,
        retry_failed: true,
    };
    ingest::run(cat, &w.lib, s, &mut ctx)
}

fn run(w: &World, cat: &mut Catalog, s: ImportSessionId) -> IngestReport {
    run_with(w, cat, s, &FaultPlan::none()).unwrap()
}

/// Hash of every file in a tree, with size and mtime.
fn tree(root: &Path) -> BTreeMap<PathBuf, (Blake3Hash, u64, std::time::SystemTime)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let m = std::fs::metadata(&p).unwrap();
                out.insert(
                    p.strip_prefix(root).unwrap().to_path_buf(),
                    (
                        Blake3Hash::of(&std::fs::read(&p).unwrap()),
                        m.len(),
                        m.modified().unwrap(),
                    ),
                );
            }
        }
    }
    out
}

/// Every visible, non-sidecar file on the destinations equals some card file,
/// and every item the catalog calls verified/reused/copied matches the card.
fn assert_no_bad_finals(w: &World, cat: &Catalog, s: ImportSessionId) {
    assert_only_faithful_files(w);
    assert_items_match(w, cat, s);
}

fn assert_only_faithful_files(w: &World) {
    let card: HashSet<Blake3Hash> = tree(&w.card).values().map(|v| v.0).collect();
    for d in &w.dests {
        for (rel, (h, _, _)) in tree(&d.mount) {
            let name = rel.file_name().unwrap().to_string_lossy().into_owned();
            if name.starts_with('.') || name.ends_with(".xmp") {
                continue;
            }
            assert!(
                card.contains(&h),
                "{} on {} is not a faithful copy",
                rel.display(),
                d.volume.label
            );
        }
    }
}

/// Every item the catalog calls verified/reused/copied matches the card.
fn assert_items_match(w: &World, cat: &Catalog, s: ImportSessionId) {
    for it in cat.read(|tx| tx.imports().items(s)).unwrap() {
        if matches!(
            it.status,
            ImportItemStatus::Verified | ImportItemStatus::Reused | ImportItemStatus::Copied
        ) {
            let d = w
                .dests
                .iter()
                .find(|d| d.volume.id == it.dest_volume_id)
                .unwrap();
            let fin = it.dest_rel_path.to_fs_path(&d.mount);
            let src = it.source_rel_path.to_fs_path(&w.card);
            assert_eq!(
                Blake3Hash::of(&std::fs::read(&fin).unwrap()),
                Blake3Hash::of(&std::fs::read(&src).unwrap()),
                "{} is marked {} but differs from the card",
                fin.display(),
                it.status
            );
        }
    }
}

fn leftovers(w: &World) -> Vec<PathBuf> {
    w.dests
        .iter()
        .flat_map(|d| ingest::leftovers(&d.mount))
        .collect()
}

fn state_of(cat: &Catalog, s: ImportSessionId, file: &str) -> ImportAssetState {
    let items = cat.read(|tx| tx.imports().items(s)).unwrap();
    let key = &items
        .iter()
        .find(|i| i.source_rel_path.as_str() == file)
        .unwrap_or_else(|| panic!("{file} not in session"))
        .source_key;
    cat.read(|tx| tx.imports().asset(s, key))
        .unwrap()
        .unwrap()
        .state
}

fn finish_cleanly(w: &World, cat: &mut Catalog, s: ImportSessionId) -> IngestReport {
    let r = run(w, cat, s);
    assert!(r.safe_to_format, "{:?}", r.not_safe);
    assert!(leftovers(w).is_empty(), "{:?}", leftovers(w));
    assert_no_bad_finals(w, cat, s);
    r
}

// ── Normal flows ─────────────────────────────────────────────────────────────

#[test]
fn two_destinations_copy_verify_catalogue_and_report_safe() {
    let (_t, w) = world();
    let before = tree(&w.card);
    let mut cat = w.lib.open_catalog().unwrap();
    let s = plan(&w, &mut cat, &w.dests);
    let r = run(&w, &mut cat, s);
    assert!(r.safe_to_format, "{:?}", r.not_safe);
    assert_eq!(r.by_state.get("catalogued"), Some(&6), "{:?}", r.by_state);
    assert!(r.failed.is_empty());
    assert!(r.bytes_copied >= BIG_LEN as u64);
    assert!(leftovers(&w).is_empty());
    assert_no_bad_finals(&w, &cat, s);
    assert_eq!(tree(&w.card), before, "the card changed");

    // Companions follow their clip, in the undated folder (no probing here).
    let a = &w.dests[0].mount;
    let day = a.join("0000-undated/0000-00-00");
    for f in [
        "C0001.MP4",
        "C0001M01.XML",
        "C0001S01.MP4",
        "DSC00001.JPG",
        "DSC00001.ARW",
        "DSC00001.HIF",
    ] {
        assert!(day.join(f).is_file(), "{f} missing");
    }
    assert!(day.join("Fête du lac 01.JPG").is_file());
    // Sidecars beside each copy of an original, on both destinations.
    for d in &w.dests {
        assert!(d
            .mount
            .join("0000-undated/0000-00-00/C0001.MP4.xmp")
            .is_file());
    }
    // Catalogue: one asset per group, files on both drives.
    let n = cat.read(|tx| tx.assets().count()).unwrap();
    assert_eq!(n, 6);
    let clip = cat
        .read(|tx| {
            tx.files().by_location(
                w.dests[1].volume.id,
                &keepsake_core::relpath::RelPath::parse("0000-undated/0000-00-00/C0001S01.MP4")
                    .unwrap(),
            )
        })
        .unwrap()
        .unwrap();
    assert_eq!(clip.role, FileRole::CameraProxy);
    let files = cat.read(|tx| tx.files().for_asset(clip.asset_id)).unwrap();
    assert_eq!(files.len(), 6, "3 files × 2 drives");
    let session = cat.read(|tx| tx.imports().get(s)).unwrap().unwrap();
    assert!(session.safe_to_format);
}

#[test]
fn one_destination_is_never_safe_to_format() {
    let (_t, w) = world();
    let mut cat = w.lib.open_catalog().unwrap();
    let s = plan(&w, &mut cat, &w.dests[..1]);
    let r = run(&w, &mut cat, s);
    assert!(!r.safe_to_format);
    assert_eq!(r.not_safe[0], "only one destination was chosen");
    assert!(r.failed.is_empty());
}

#[test]
fn second_ingest_finds_everything_already_imported() {
    let (_t, w) = world();
    let mut cat = w.lib.open_catalog().unwrap();
    let s1 = plan(&w, &mut cat, &w.dests);
    run(&w, &mut cat, s1);
    let s2 = plan(&w, &mut cat, &w.dests);
    let assets = cat.read(|tx| tx.imports().assets(s2)).unwrap();
    assert!(
        assets.iter().all(|a| a.diff == ImportDiff::AlreadyImported),
        "{assets:?}"
    );
    let r = run(&w, &mut cat, s2);
    assert_eq!(r.bytes_copied, 0);
    // Confirmed by full BLAKE3 against two library volumes.
    assert!(r.safe_to_format, "{:?}", r.not_safe);
}

#[test]
fn unselected_assets_are_reported_and_not_copied() {
    let (_t, w) = world();
    let mut cat = w.lib.open_catalog().unwrap();
    let mut o = opts();
    o.selection = Selection::Nothing;
    let s = ingest::plan(&mut cat, &w.lib, None, &w.card, "Test card", &w.dests, &o).unwrap();
    let keys = ingest::asset_keys(&cat, s).unwrap();
    ingest::select(&mut cat, s, &keys[..2], true).unwrap();
    let r = run(&w, &mut cat, s);
    assert_eq!(r.unselected, 4);
    assert_eq!(r.by_state.get("catalogued"), Some(&2));
    // The prompt's rule: only selected assets count.
    assert!(r.safe_to_format, "{:?}", r.not_safe);
}

#[test]
fn collisions_get_a_suffix_and_identical_files_are_reused() {
    let (_t, w) = world();
    let day = "0000-undated/0000-00-00";
    // A different file already sits where DSC00002.JPG would go (both drives).
    for d in &w.dests {
        write_file(
            &d.mount,
            &format!("{day}/DSC00002.JPG"),
            b"someone else's photo",
        );
    }
    // An identical copy of DSC00001.JPG is already on drive A.
    let same = std::fs::read(w.card.join("DCIM/100MSDCF/DSC00001.JPG")).unwrap();
    write_file(&w.dests[0].mount, &format!("{day}/DSC00001.JPG"), &same);
    let mut cat = w.lib.open_catalog().unwrap();
    let s = plan(&w, &mut cat, &w.dests);
    let r = run(&w, &mut cat, s);
    assert!(r.safe_to_format, "{:?}", r.not_safe);
    for d in &w.dests {
        let dir = d.mount.join(day);
        assert_eq!(
            std::fs::read(dir.join("DSC00002.JPG")).unwrap(),
            b"someone else's photo"
        );
        assert!(dir.join("DSC00002 (2).JPG").is_file());
    }
    let items = cat.read(|tx| tx.imports().items(s)).unwrap();
    let reused = items
        .iter()
        .find(|i| {
            i.dest_volume_id == w.dests[0].volume.id
                && i.source_rel_path.as_str() == "DCIM/100MSDCF/DSC00001.JPG"
        })
        .unwrap();
    assert_eq!(reused.status, ImportItemStatus::Reused);
    // The foreign DSC00002.JPG is still there untouched, so check items only.
    assert_items_match(&w, &cat, s);
}

#[test]
fn templates_rename_and_companions_follow() {
    let (_t, w) = world();
    let mut cat = w.lib.open_catalog().unwrap();
    let mut o = opts();
    o.template = Template::parse("{event}/{seq}_{orig}").unwrap();
    o.event = Some("Trip: day 1".into());
    let s = ingest::plan(&mut cat, &w.lib, None, &w.card, "Test card", &w.dests, &o).unwrap();
    run(&w, &mut cat, s);
    let dir = w.dests[0].mount.join("Trip_ day 1");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.ends_with(".xmp"))
        .collect();
    names.sort();
    // Undated, so numbered in source-path order: DCIM before PRIVATE.
    assert!(names.contains(&"0001_DSC00001.JPG".to_owned()), "{names:?}");
    assert!(names.contains(&"0005_C0001.MP4".to_owned()), "{names:?}");
    assert!(
        names.contains(&"0005_C0001M01.XML".to_owned()),
        "XML follows its clip"
    );
    assert!(
        names.contains(&"0005_C0001S01.MP4".to_owned()),
        "proxy follows its clip"
    );
    assert!(names.contains(&"0006_C0002.MP4".to_owned()));
    let jpg = names.iter().find(|n| n.ends_with("_DSC00001.JPG")).unwrap();
    let stem = jpg.trim_end_matches(".JPG");
    assert!(
        names.contains(&format!("{stem}.ARW")),
        "RAW follows its JPEG"
    );
}

// ── Refusals ─────────────────────────────────────────────────────────────────

#[test]
fn refuses_unsafe_destinations() {
    let (_t, w) = world();
    let mut cat = w.lib.open_catalog().unwrap();
    let inside = destination_volume(
        &w.lib,
        &w.card.join("backup"),
        "Inside",
        VolumeKind::External,
    )
    .unwrap();
    let err = ingest::plan(
        &mut cat,
        &w.lib,
        None,
        &w.card,
        "Test card",
        std::slice::from_ref(&inside),
        &opts(),
    );
    assert!(matches!(err, Err(IngestError::Unsafe(m)) if m.contains("inside the source")));
    std::fs::remove_dir(w.card.join("backup")).unwrap();
    let mut strict = opts();
    strict.allow_same_filesystem = false;
    let err = ingest::plan(
        &mut cat,
        &w.lib,
        None,
        &w.card,
        "Test card",
        &w.dests,
        &strict,
    );
    assert!(
        matches!(&err, Err(IngestError::Unsafe(m)) if m.contains("same disk")),
        "{err:?}"
    );
    let three = vec![w.dests[0].clone(), w.dests[1].clone(), w.dests[0].clone()];
    assert!(ingest::plan(
        &mut cat,
        &w.lib,
        None,
        &w.card,
        "Test card",
        &three,
        &opts()
    )
    .is_err());
    let missing = ingest::plan(
        &mut cat,
        &w.lib,
        None,
        &w.root.join("no-card"),
        "x",
        &w.dests,
        &opts(),
    );
    assert!(matches!(missing, Err(IngestError::SourceMissing(_))));
}

// ── Fault injection ──────────────────────────────────────────────────────────

type Faulted = (
    tempfile::TempDir,
    World,
    Catalog,
    ImportSessionId,
    Result<IngestReport, IngestError>,
);

/// Build a world, plan, and run once with the fault returned by `make`
/// (which receives the destination volume ids, A then B).
fn faulted_with(make: impl FnOnce(&[keepsake_core::catalog::VolumeId]) -> Fault) -> Faulted {
    let (t, w) = world();
    let ids: Vec<_> = w.dests.iter().map(|d| d.volume.id).collect();
    let mut cat = w.lib.open_catalog().unwrap();
    let s = plan(&w, &mut cat, &w.dests);
    let r = run_with(&w, &mut cat, s, &FaultPlan::with(vec![make(&ids)]));
    (t, w, cat, s, r)
}

fn faulted(fault: Fault) -> Faulted {
    faulted_with(|_| fault)
}

#[test]
fn source_removed_mid_copy() {
    let (_t, w, mut cat, s, r) = faulted(Fault::SourceRead {
        file: BIG.into(),
        after_bytes: 1 << 20,
        kind: ErrorKind::NotFound,
    });
    let r = r.unwrap();
    assert!(!r.safe_to_format);
    assert_eq!(state_of(&cat, s, BIG), ImportAssetState::Failed);
    for d in &w.dests {
        assert!(!d
            .mount
            .join("0000-undated/0000-00-00/DSC00009.JPG")
            .exists());
    }
    assert!(leftovers(&w).is_empty(), "temporaries must be removed");
    assert_no_bad_finals(&w, &cat, s);
    finish_cleanly(&w, &mut cat, s);
}

#[test]
fn source_file_changing_while_read() {
    let (_t, w, mut cat, s, r) = faulted(Fault::SourceChanges { file: BIG.into() });
    assert!(!r.unwrap().safe_to_format);
    let a = cat.read(|tx| tx.imports().assets(s)).unwrap();
    assert!(a
        .iter()
        .any(|a| a.error.as_deref().is_some_and(|e| e.contains("changed"))));
    assert!(leftovers(&w).is_empty());
    finish_cleanly(&w, &mut cat, s);
}

#[test]
fn destination_full() {
    let (_t, w, mut cat, s, r) = faulted_with(|ids| Fault::DestWrite {
        dest: ids[1],
        file: BIG.into(),
        after_bytes: 1 << 20,
        kind: ErrorKind::StorageFull,
    });
    let r = r.unwrap();
    assert!(!r.safe_to_format);
    assert!(
        r.not_safe.iter().any(|n| n.contains("DSC00009")),
        "{:?}",
        r.not_safe
    );
    // Drive A got a verified copy; drive B has nothing under the final name.
    assert!(w.dests[0]
        .mount
        .join("0000-undated/0000-00-00/DSC00009.JPG")
        .is_file());
    assert!(!w.dests[1]
        .mount
        .join("0000-undated/0000-00-00/DSC00009.JPG")
        .exists());
    assert!(leftovers(&w).is_empty());
    assert_no_bad_finals(&w, &cat, s);
    finish_cleanly(&w, &mut cat, s);
}

#[test]
fn destination_permission_denied_for_real() {
    let (_t, w) = world();
    let mut cat = w.lib.open_catalog().unwrap();
    let s = plan(&w, &mut cat, &w.dests);
    let guard = common::readonly::deny_writes(&w.dests[1].mount);
    let r = run(&w, &mut cat, s);
    assert!(!r.safe_to_format);
    assert!(
        r.failed.iter().any(|(_, e)| e.contains("writing")),
        "{:?}",
        r.failed
    );
    drop(guard);
    assert!(ingest::leftovers(&w.dests[1].mount).is_empty());
    assert_no_bad_finals(&w, &cat, s);
    finish_cleanly(&w, &mut cat, s);
}

#[test]
fn corrupted_write_is_caught_and_retried() {
    let (_t, w, cat, s, r) = faulted_with(|ids| Fault::CorruptWrite {
        dest: ids[0],
        file: BIG.into(),
    });
    let r = r.unwrap();
    assert!(
        r.safe_to_format,
        "a single bad write is retried: {:?}",
        r.not_safe
    );
    assert_no_bad_finals(&w, &cat, s);
}

#[test]
fn persistent_verify_mismatch_never_renames() {
    let (_t, w, mut cat, s, r) = faulted_with(|ids| Fault::CorruptVerify {
        dest: ids[0],
        file: BIG.into(),
        times: 2,
    });
    let r = r.unwrap();
    assert!(!r.safe_to_format);
    assert!(
        r.failed.iter().any(|(_, e)| e.contains("did not match")),
        "{:?}",
        r.failed
    );
    assert!(!w.dests[0]
        .mount
        .join("0000-undated/0000-00-00/DSC00009.JPG")
        .exists());
    assert!(leftovers(&w).is_empty());
    assert_no_bad_finals(&w, &cat, s);
    finish_cleanly(&w, &mut cat, s);
}

#[test]
fn final_file_failing_verification_is_moved_aside() {
    let (_t, w, mut cat, s, r) = faulted_with(|ids| Fault::CorruptFinal {
        dest: ids[1],
        file: BIG.into(),
    });
    let r = r.unwrap();
    assert!(!r.safe_to_format);
    let dir = w.dests[1].mount.join("0000-undated/0000-00-00");
    assert!(
        !dir.join("DSC00009.JPG").exists(),
        "never left under its final name"
    );
    assert!(std::fs::read_dir(&dir).unwrap().flatten().any(|e| e
        .file_name()
        .to_string_lossy()
        .contains("DSC00009.JPG.keepsake-bad-")));
    assert_no_bad_finals(&w, &cat, s);
    finish_cleanly(&w, &mut cat, s);
}

#[test]
fn rename_retries_then_fails_cleanly() {
    // Two busy errors: retried and succeeds.
    let (_t, w, cat, s, r) = faulted_with(|ids| Fault::Rename {
        dest: ids[0],
        file: BIG.into(),
        kind: ErrorKind::PermissionDenied,
        times: 2,
    });
    assert!(r.unwrap().safe_to_format);
    assert_no_bad_finals(&w, &cat, s);
    // Always busy: fails, temporary removed.
    let (_t, w, mut cat, s, r) = faulted_with(|ids| Fault::Rename {
        dest: ids[0],
        file: BIG.into(),
        kind: ErrorKind::PermissionDenied,
        times: 99,
    });
    assert!(!r.unwrap().safe_to_format);
    assert!(leftovers(&w).is_empty());
    finish_cleanly(&w, &mut cat, s);
}

#[test]
fn catalog_failure_keeps_verified_files_and_resumes() {
    let (_t, w, mut cat, s, r) = faulted(Fault::Catalog { file: BIG.into() });
    let r = r.unwrap();
    assert!(!r.safe_to_format);
    assert_eq!(state_of(&cat, s, BIG), ImportAssetState::Verified);
    assert!(w.dests[0]
        .mount
        .join("0000-undated/0000-00-00/DSC00009.JPG")
        .is_file());
    finish_cleanly(&w, &mut cat, s);
    assert_eq!(state_of(&cat, s, BIG), ImportAssetState::Catalogued);
}

#[test]
fn cancel_finishes_the_current_file_and_leaves_no_partial_files() {
    let (_t, w) = world();
    let mut cat = w.lib.open_catalog().unwrap();
    let s = plan(&w, &mut cat, &w.dests);
    let cancel = AtomicBool::new(false);
    let mut progress = |p: &ingest::Progress| {
        if matches!(p, ingest::Progress::FileBytes { file, done, .. } if file == BIG && *done >= 1 << 20)
        {
            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    };
    let fault = FaultPlan::none();
    let mut ctx = RunContext {
        prober: None,
        fault: &fault,
        cancel: &cancel,
        progress: &mut progress,
        retry_failed: true,
    };
    let r = ingest::run(&mut cat, &w.lib, s, &mut ctx);
    assert!(matches!(r, Err(IngestError::Cancelled)), "{r:?}");
    assert!(leftovers(&w).is_empty());
    // Ctrl-C finishes the file in flight: the big file is complete on both drives.
    for d in &w.dests {
        let fin = d.mount.join("0000-undated/0000-00-00/DSC00009.JPG");
        assert_eq!(
            std::fs::read(&fin).unwrap(),
            big_bytes(),
            "{}",
            fin.display()
        );
    }
    assert_no_bad_finals(&w, &cat, s);
    finish_cleanly(&w, &mut cat, s);
}

fn simulated_crash(at: CrashPoint) {
    let (_t, w, mut cat, s, r) = faulted(Fault::Crash {
        at,
        mode: CrashMode::Simulate,
    });
    assert!(matches!(r, Err(IngestError::SimulatedCrash)), "{r:?}");
    // Whatever the crash left must not pass as complete.
    assert_no_bad_finals(&w, &cat, s);
    let report = ingest::report(&cat, s).unwrap();
    assert!(!report.safe_to_format);
    // A new run (as after a restart) resumes and finishes.
    finish_cleanly(&w, &mut cat, s);
}

#[test]
fn crash_mid_copy_then_resume() {
    simulated_crash(CrashPoint::MidCopy {
        file: BIG.into(),
        after_bytes: 2 << 20,
    });
}

#[test]
fn crash_after_rename_then_resume() {
    simulated_crash(CrashPoint::AfterRename { file: BIG.into() });
}

#[test]
fn crash_after_verify_then_resume() {
    simulated_crash(CrashPoint::AfterVerify { file: BIG.into() });
}

#[test]
fn bad_final_from_a_crashed_run_is_not_trusted() {
    // Crash right after the rename, then tamper with the renamed file as if
    // the disk had lost the write: resume must notice and copy again.
    let (_t, w, mut cat, s, r) = faulted(Fault::Crash {
        at: CrashPoint::AfterRename { file: BIG.into() },
        mode: CrashMode::Simulate,
    });
    assert!(matches!(r, Err(IngestError::SimulatedCrash)));
    // The crash fired after the first rename, on whichever drive came first.
    let fin = w
        .dests
        .iter()
        .map(|d| d.mount.join("0000-undated/0000-00-00/DSC00009.JPG"))
        .find(|p| p.is_file())
        .expect("one drive has the renamed file");
    let mut bytes = std::fs::read(&fin).unwrap();
    bytes[100] ^= 1;
    std::fs::write(&fin, bytes).unwrap();
    finish_cleanly(&w, &mut cat, s);
}

// ── Real process kill ────────────────────────────────────────────────────────

const CHILD_ENV: &str = "KEEPSAKE_INGEST_CHILD";

/// Runs only as a child of `process_killed_mid_copy_then_resumed`.
#[test]
#[ignore = "child process of process_killed_mid_copy_then_resumed"]
fn child_ingest_until_abort() {
    let Ok(spec) = std::env::var(CHILD_ENV) else {
        return;
    };
    let (root, point) = spec.split_once('|').unwrap();
    let w = reopen(Path::new(root));
    let mut cat = w.lib.open_catalog().unwrap();
    let s = cat.read(|tx| tx.imports().list()).unwrap()[0].id;
    let at = match point {
        "mid" => CrashPoint::MidCopy {
            file: BIG.into(),
            after_bytes: 2 << 20,
        },
        "rename" => CrashPoint::AfterRename { file: BIG.into() },
        _ => CrashPoint::AfterVerify { file: BIG.into() },
    };
    let fault = FaultPlan::with(vec![Fault::Crash {
        at,
        mode: CrashMode::Abort,
    }]);
    let _ = run_with(&w, &mut cat, s, &fault);
    unreachable!("the fault plan aborts the process");
}

fn reopen(root: &Path) -> World {
    let lib = Library::open(&root.join("library")).unwrap();
    let vols = lib.volumes().unwrap();
    let dest = |label: &str| {
        let v = vols.iter().find(|v| v.label == label).unwrap().clone();
        Destination {
            mount: v.last_mount.clone().unwrap(),
            volume: v,
        }
    };
    World {
        _tmp: tempfile::tempdir().unwrap(),
        root: root.to_path_buf(),
        card: root.join("card"),
        dests: vec![dest("Drive A"), dest("Drive B")],
        lib,
    }
}

#[test]
fn process_killed_mid_copy_then_resumed() {
    for point in ["mid", "rename", "verify"] {
        let tmp = tempfile::tempdir().unwrap();
        let w = world_at(tmp.path());
        let s = {
            let mut cat = w.lib.open_catalog().unwrap();
            plan(&w, &mut cat, &w.dests)
        };
        let out = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "child_ingest_until_abort",
                "--ignored",
                "--nocapture",
            ])
            .env(CHILD_ENV, format!("{}|{point}", tmp.path().display()))
            .output()
            .unwrap();
        assert!(
            !out.status.success(),
            "{point}: the child should have been killed"
        );
        let mut cat = w.lib.open_catalog().unwrap();
        assert_no_bad_finals(&w, &cat, s);
        assert!(!ingest::report(&cat, s).unwrap().safe_to_format);
        if point == "mid" {
            assert!(
                !leftovers(&w).is_empty(),
                "a killed copy leaves its hidden temporary behind"
            );
        }
        let r = finish_cleanly(&w, &mut cat, s);
        assert_eq!(
            r.by_state.get("catalogued"),
            Some(&6),
            "{point}: {:?}",
            r.by_state
        );
    }
}

// ── The source is never written ─────────────────────────────────────────────

#[test]
fn source_mounted_read_only_is_never_written() {
    let (_t, w) = world();
    let before = tree(&w.card);
    let guard = common::readonly::deny_writes(&w.card);
    // The operating system really refuses writes now.
    assert!(std::fs::write(w.card.join("probe.txt"), b"x").is_err());
    assert!(std::fs::write(w.card.join("DCIM/100MSDCF/DSC00001.JPG"), b"x").is_err());
    assert!(std::fs::rename(
        w.card.join("DCIM/100MSDCF/DSC00001.JPG"),
        w.card.join("DCIM/100MSDCF/x.JPG")
    )
    .is_err());
    assert!(std::fs::remove_file(w.card.join("DCIM/100MSDCF/DSC00002.JPG")).is_err());
    assert!(std::fs::create_dir(w.card.join("newdir")).is_err());
    let mut cat = w.lib.open_catalog().unwrap();
    let s = plan(&w, &mut cat, &w.dests);
    let r = run(&w, &mut cat, s);
    assert!(r.safe_to_format, "{:?}", r.not_safe);
    // A second ingest (quick-diff + full BLAKE3 confirmation) reads it again.
    let s2 = plan(&w, &mut cat, &w.dests);
    assert!(run(&w, &mut cat, s2).safe_to_format);
    drop(guard);
    assert_eq!(
        tree(&w.card),
        before,
        "bytes, sizes and modification times are unchanged"
    );
}

// ── With probing: dated folders, capture metadata, sequences ────────────────

#[test]
fn probed_ingest_files_by_capture_date_and_records_metadata() {
    let Some((t, ffmpeg)) = common::media::tools() else {
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::create(&tmp.path().join("library")).unwrap();
    let card = tmp.path().join("card");
    let et = keepsake_core::probe::ExifTool::start(&t.exiftool, std::time::Duration::from_secs(30))
        .unwrap();
    for (i, ss) in ["100", "200", "300"].iter().enumerate() {
        let p = card.join(format!("DCIM/100MSDCF/DSC0000{}.JPG", i + 1));
        common::media::jpeg(&ffmpeg, &p);
        common::media::set_tags(
            &et,
            &p,
            &[
                "-DateTimeOriginal=2024:06:15 13:31:00",
                &format!("-SubSecTimeOriginal={ss}"),
                "-OffsetTimeOriginal=+09:00",
                "-Model=ILCE-7M4",
            ],
        );
    }
    let xml = std::fs::read_to_string(common::fixture("sony_xml/xavc_hs_sq_gps.xml")).unwrap();
    common::media::h264_mp4(
        &ffmpeg,
        &card.join("PRIVATE/M4ROOT/CLIP/C0001.MP4"),
        "2024-06-15T04:30:00Z",
        None,
    );
    std::fs::write(card.join("PRIVATE/M4ROOT/CLIP/C0001M01.XML"), xml).unwrap();
    drop(et);
    let dests = vec![
        destination_volume(&lib, &tmp.path().join("a"), "A", VolumeKind::External).unwrap(),
        destination_volume(&lib, &tmp.path().join("b"), "B", VolumeKind::External).unwrap(),
    ];
    let prober = keepsake_core::probe::Prober::new(&t, Default::default()).unwrap();
    let mut cat = lib.open_catalog().unwrap();
    let mut o = opts();
    o.template = Template::parse("{yyyy}/{yyyy}-{mm}-{dd}/{hh}_{camera}_{orig}").unwrap();
    let s = ingest::plan(&mut cat, &lib, Some(&prober), &card, "Card", &dests, &o).unwrap();
    let cancel = AtomicBool::new(false);
    let fault = FaultPlan::none();
    let mut progress = |_: &ingest::Progress| {};
    let mut ctx = RunContext {
        prober: Some(&prober),
        fault: &fault,
        cancel: &cancel,
        progress: &mut progress,
        retry_failed: true,
    };
    let r = ingest::run(&mut cat, &lib, s, &mut ctx).unwrap();
    assert!(r.safe_to_format, "{:?}", r.not_safe);
    let day = dests[0].mount.join("2024/2024-06-15");
    assert!(day.join("13_ILCE-7M4_DSC00001.JPG").is_file());
    assert!(day.join("13_ILCE-7SM3_C0001.MP4").is_file());
    assert!(
        day.join("13_ILCE-7SM3_C0001M01.XML").is_file(),
        "XML follows its clip"
    );
    let f = cat
        .read(|tx| {
            tx.files().by_location(
                dests[1].volume.id,
                &keepsake_core::relpath::RelPath::parse("2024/2024-06-15/13_ILCE-7SM3_C0001.MP4")
                    .unwrap(),
            )
        })
        .unwrap()
        .unwrap();
    let a = cat.read(|tx| tx.assets().require(f.asset_id)).unwrap();
    assert_eq!(
        a.capture_source,
        Some(keepsake_core::catalog::CaptureSource::SonyXml)
    );
    assert_eq!(a.duration_ms, Some(10_010));
    assert!(a.gps.is_some());
    let still = cat
        .read(|tx| {
            tx.files().by_location(
                dests[0].volume.id,
                &keepsake_core::relpath::RelPath::parse("2024/2024-06-15/13_ILCE-7M4_DSC00002.JPG")
                    .unwrap(),
            )
        })
        .unwrap()
        .unwrap();
    let seq = cat
        .read(|tx| tx.sequences().for_asset(still.asset_id))
        .unwrap()
        .unwrap();
    assert_eq!((seq.0.members.len(), seq.1), (3, 1));
    // The sidecar on the destination carries the capture provenance.
    let xmp = std::fs::read_to_string(day.join("13_ILCE-7M4_DSC00002.JPG.xmp")).unwrap();
    assert!(
        xmp.contains("<keepsake:CaptureSource>exif</keepsake:CaptureSource>"),
        "{xmp}"
    );
    assert!(xmp.contains("<keepsake:Kind>burst</keepsake:Kind>"));
}

//! Cards and the watcher: recognising Sony cards, remembering them across
//! slots, "new since last import", and automatic imports of trusted cards,
//! all driven by scripted device events (no hardware).

mod common;

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use common::{copy_dir, write_file};
use keepsake_core::cards::{self, CardLayout};
use keepsake_core::devices::{DeviceEvent, VolumeIdentity};
use keepsake_core::ingest::fault::{Fault, FaultPlan};
use keepsake_core::library::Library;
use keepsake_core::presets::{self, Preset};
use keepsake_core::watch::{WatchEvent, WatchOptions, Watcher};

const TEMPLATE: &str = "{yyyy}/{yyyy}-{mm}-{dd}/{orig}";

struct World {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    lib: Library,
    card: PathBuf,
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let lib = Library::create(&root.join("library")).unwrap();
    let card = root.join("card");
    make_card(&card);
    World {
        _tmp: tmp,
        root,
        lib,
        card,
    }
}

fn make_card(at: &Path) {
    copy_dir(&common::fixture("xavc"), at);
    copy_dir(&common::fixture("dcim"), at);
}

fn identity(serial: &str) -> VolumeIdentity {
    VolumeIdentity {
        fs_serial: Some(serial.into()),
        fs_type: Some("exFAT".into()),
        label: Some("Untitled".into()),
        capacity: Some(64_000_000_000),
        removable: true,
        hw_serial: Some("READER-1".into()),
        ..Default::default()
    }
}

fn arrived(mount: &Path, serial: &str) -> DeviceEvent {
    DeviceEvent::Arrived {
        mount: mount.to_path_buf(),
        identity: identity(serial),
    }
}

fn removed(mount: &Path) -> DeviceEvent {
    DeviceEvent::Removed {
        mount: mount.to_path_buf(),
    }
}

/// Feed events to a watcher and collect what it says.
fn drive(w: &mut Watcher<'_>, events: &[DeviceEvent]) -> Vec<WatchEvent> {
    let cancel = AtomicBool::new(false);
    let mut out = Vec::new();
    for e in events {
        w.handle(e, &cancel, &mut |ev| out.push(ev.clone()));
    }
    out
}

fn auto() -> WatchOptions {
    WatchOptions {
        auto_ingest: true,
        // Tests keep everything in one temp folder, i.e. on one disk.
        allow_same_filesystem: true,
    }
}

fn trust(w: &World, preset: Option<&str>) {
    let mut c = cards::list(&w.lib).unwrap().pop().unwrap();
    c.trusted = true;
    c.preset = preset.map(str::to_owned);
    cards::save(&w.lib, &c).unwrap();
}

fn save_preset(w: &World, dests: &[PathBuf]) {
    for d in dests {
        std::fs::create_dir_all(d).unwrap();
    }
    presets::save(
        &w.lib,
        &Preset {
            name: "Two drives".into(),
            destinations: dests.to_vec(),
            template: TEMPLATE.into(),
            event: None,
        },
    )
    .unwrap();
}

fn inserted(evs: &[WatchEvent]) -> (bool, usize, usize) {
    evs.iter()
        .find_map(|e| match e {
            WatchEvent::CardInserted {
                known,
                items,
                new_items,
                ..
            } => Some((*known, *items, *new_items)),
            _ => None,
        })
        .expect("a card_inserted event")
}

fn has_auto(evs: &[WatchEvent]) -> bool {
    evs.iter().any(|e| {
        matches!(
            e,
            WatchEvent::AutoIngestStarted { .. }
                | WatchEvent::AutoIngestFinished { .. }
                | WatchEvent::AutoIngestFailed { .. }
        )
    })
}

fn skipped(evs: &[WatchEvent]) -> Option<String> {
    evs.iter().find_map(|e| match e {
        WatchEvent::AutoIngestSkipped { reason, .. } => Some(reason.clone()),
        _ => None,
    })
}

#[test]
fn a_card_is_recognised_and_its_items_counted() {
    let w = world();
    let mut layouts = cards::recognize(&w.card);
    layouts.sort();
    assert_eq!(layouts, vec![CardLayout::Xavc, CardLayout::Stills]);
    let items = cards::items(&w.card);
    assert!(items.len() >= 3, "{items:?}");
    let mut watcher = Watcher::new(&w.lib, None, WatchOptions::default());
    let evs = drive(&mut watcher, &[arrived(&w.card, "1A2B-3C4D")]);
    let (known, n, new) = inserted(&evs);
    assert!(!known);
    assert_eq!(n, items.len());
    assert_eq!(new, n, "a card never imported from: everything is new");
    let all = cards::list(&w.lib).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].label, "Untitled");
    assert_eq!(all[0].hw_serial.as_deref(), Some("READER-1"));
}

#[test]
fn other_volumes_are_ignored() {
    let w = world();
    let usb = w.root.join("usb-stick");
    std::fs::create_dir_all(usb.join("DCIM/100CANON")).unwrap();
    let mut watcher = Watcher::new(&w.lib, None, auto());
    let evs = drive(&mut watcher, &[arrived(&usb, "9999"), removed(&usb)]);
    assert!(
        matches!(&evs[..], [WatchEvent::VolumeIgnored { .. }]),
        "{evs:?}"
    );
    assert!(cards::list(&w.lib).unwrap().is_empty());
}

#[test]
fn a_known_card_in_another_slot_shows_only_new_items() {
    let w = world();
    save_preset(&w, &[w.root.join("A"), w.root.join("B")]);
    let mut watcher = Watcher::new(&w.lib, None, auto());
    drive(
        &mut watcher,
        &[arrived(&w.card, "1A2B-3C4D"), removed(&w.card)],
    );
    trust(&w, Some("Two drives"));
    // First trusted insert imports everything.
    let evs = drive(
        &mut watcher,
        &[arrived(&w.card, "1A2B-3C4D"), removed(&w.card)],
    );
    assert!(
        evs.iter().any(|e| matches!(e,
        WatchEvent::AutoIngestFinished { report, new_items_left: 0, .. } if report.safe_to_format)),
        "{evs:#?}"
    );

    // The same card (same serial) turns up somewhere else, with one new photo.
    let slot2 = w.root.join("other-reader");
    copy_dir(&w.card, &slot2);
    write_file(&slot2, "DCIM/100MSDCF/DSC00002.JPG", b"a new photo");
    let mut plain = Watcher::new(&w.lib, None, WatchOptions::default());
    let evs = drive(&mut plain, &[arrived(&slot2, "1a2b-3c4d")]);
    let (known, _, new) = inserted(&evs);
    assert!(known, "matched by serial, not by mount point");
    assert_eq!(new, 1);
    assert_eq!(cards::list(&w.lib).unwrap().len(), 1);
}

#[test]
fn a_reformatted_card_is_a_new_card() {
    let w = world();
    let mut watcher = Watcher::new(&w.lib, None, WatchOptions::default());
    drive(
        &mut watcher,
        &[arrived(&w.card, "1111-1111"), removed(&w.card)],
    );
    let evs = drive(&mut watcher, &[arrived(&w.card, "2222-2222")]);
    assert!(!inserted(&evs).0);
    assert_eq!(cards::list(&w.lib).unwrap().len(), 2);
}

#[test]
fn untrusted_cards_are_never_imported_automatically() {
    let w = world();
    save_preset(&w, &[w.root.join("A"), w.root.join("B")]);
    let mut watcher = Watcher::new(&w.lib, None, auto());
    let evs = drive(&mut watcher, &[arrived(&w.card, "1A2B")]);
    assert!(!has_auto(&evs));
    assert!(skipped(&evs).is_none());
    assert!(!w.root.join("A").join("2024").exists());
}

#[test]
fn trusted_cards_need_auto_ingest_mode() {
    let w = world();
    save_preset(&w, &[w.root.join("A"), w.root.join("B")]);
    let mut first = Watcher::new(&w.lib, None, WatchOptions::default());
    drive(&mut first, &[arrived(&w.card, "1A2B")]);
    trust(&w, Some("Two drives"));
    let mut watcher = Watcher::new(&w.lib, None, WatchOptions::default());
    let evs = drive(&mut watcher, &[arrived(&w.card, "1A2B")]);
    assert!(!has_auto(&evs));
}

#[test]
fn trusted_card_imports_then_reports_nothing_new() {
    let w = world();
    let (a, b) = (w.root.join("A"), w.root.join("B"));
    save_preset(&w, &[a.clone(), b.clone()]);
    let mut watcher = Watcher::new(&w.lib, None, auto());
    drive(&mut watcher, &[arrived(&w.card, "1A2B"), removed(&w.card)]);
    trust(&w, Some("two DRIVES"));
    let before = common::tree_hashes(&w.card);

    let evs = drive(&mut watcher, &[arrived(&w.card, "1A2B")]);
    let order: Vec<&str> = evs
        .iter()
        .map(|e| match e {
            WatchEvent::CardInserted { .. } => "inserted",
            WatchEvent::AutoIngestStarted { .. } => "started",
            WatchEvent::AutoIngestProgress { .. } => "progress",
            WatchEvent::AutoIngestFinished { .. } => "finished",
            _ => "other",
        })
        .collect();
    assert_eq!(order.first(), Some(&"inserted"));
    assert_eq!(order.get(1), Some(&"started"));
    assert_eq!(order.last(), Some(&"finished"), "{evs:#?}");
    assert!(order.contains(&"progress"));
    assert!(a.join("2024/2024-01-15").is_dir() || a.join("0000-undated").is_dir());
    assert!(b.read_dir().unwrap().next().is_some());
    assert_eq!(
        common::tree_hashes(&w.card),
        before,
        "the card is untouched"
    );

    let rec = cards::list(&w.lib).unwrap().pop().unwrap();
    assert!(rec.last_import_ms.is_some() && rec.last_session.is_some());

    // Out and in again: nothing new, so nothing to do.
    let evs = drive(&mut watcher, &[removed(&w.card), arrived(&w.card, "1A2B")]);
    assert!(matches!(evs[0], WatchEvent::CardRemoved { .. }));
    assert_eq!(inserted(&evs).2, 0);
    assert_eq!(skipped(&evs).as_deref(), Some("nothing new on the card"));
}

#[test]
fn auto_ingest_explains_why_it_skips() {
    let w = world();
    let mut watcher = Watcher::new(&w.lib, None, auto());
    drive(&mut watcher, &[arrived(&w.card, "1A2B"), removed(&w.card)]);

    trust(&w, None);
    let evs = drive(&mut watcher, &[arrived(&w.card, "1A2B"), removed(&w.card)]);
    assert_eq!(skipped(&evs).as_deref(), Some("the card has no preset"));

    trust(&w, Some("gone"));
    let evs = drive(&mut watcher, &[arrived(&w.card, "1A2B"), removed(&w.card)]);
    assert_eq!(skipped(&evs).as_deref(), Some("preset gone does not exist"));

    // A destination drive that is not connected.
    let a = w.root.join("A");
    let b = w.root.join("B");
    save_preset(&w, &[a, b.clone()]);
    std::fs::remove_dir(&b).unwrap();
    trust(&w, Some("Two drives"));
    let evs = drive(&mut watcher, &[arrived(&w.card, "1A2B")]);
    let why = skipped(&evs).unwrap_or_default();
    assert!(why.contains("not found") && why.contains('B'), "{why}");
    assert!(!has_auto(&evs));
}

#[test]
fn repeated_arrivals_and_unknown_removals_are_harmless() {
    let w = world();
    let mut watcher = Watcher::new(&w.lib, None, WatchOptions::default());
    let evs = drive(
        &mut watcher,
        &[
            arrived(&w.card, "1A2B"),
            arrived(&w.card, "1A2B"),
            removed(&w.root.join("never-mounted")),
        ],
    );
    assert_eq!(evs.len(), 1, "{evs:?}");
    assert_eq!(watcher.mounted().len(), 1);
    let evs = drive(&mut watcher, &[removed(&w.card), removed(&w.card)]);
    assert_eq!(evs.len(), 1);
    assert!(watcher.mounted().is_empty());
}

#[test]
fn card_pulled_mid_import_is_reported_and_stays_new() {
    let w = world();
    let big = "DCIM/100MSDCF/DSC00009.JPG";
    write_file(&w.card, big, &vec![7u8; 2 * 1024 * 1024]);
    save_preset(&w, &[w.root.join("A"), w.root.join("B")]);
    let mut first = Watcher::new(&w.lib, None, WatchOptions::default());
    drive(&mut first, &[arrived(&w.card, "1A2B")]);
    trust(&w, Some("Two drives"));

    let fault = FaultPlan::with(vec![Fault::SourceRead {
        file: big.into(),
        after_bytes: 1 << 20,
        kind: ErrorKind::NotFound,
    }]);
    let mut watcher = Watcher::new(&w.lib, None, auto()).with_fault(fault);
    let evs = drive(&mut watcher, &[arrived(&w.card, "1A2B")]);
    let done = evs.iter().find_map(|e| match e {
        WatchEvent::AutoIngestFinished {
            report,
            new_items_left,
            ..
        } => Some((report.safe_to_format, *new_items_left)),
        _ => None,
    });
    assert_eq!(done, Some((false, 1)), "{evs:#?}");
    // The others are remembered as imported; only the failed one is new.
    let rec = cards::list(&w.lib).unwrap().pop().unwrap();
    let st = cards::status(&w.lib, &rec, &w.card).unwrap();
    assert_eq!(st.new, vec![big.to_owned()]);
}

#[test]
fn ctrl_c_stops_an_automatic_import_after_the_current_file() {
    let w = world();
    save_preset(&w, &[w.root.join("A"), w.root.join("B")]);
    let mut first = Watcher::new(&w.lib, None, WatchOptions::default());
    drive(&mut first, &[arrived(&w.card, "1A2B")]);
    trust(&w, Some("Two drives"));

    let cancel = AtomicBool::new(false);
    let mut watcher = Watcher::new(&w.lib, None, auto());
    let mut evs = Vec::new();
    watcher.handle(&arrived(&w.card, "1A2B"), &cancel, &mut |e| {
        if matches!(e, WatchEvent::AutoIngestProgress { .. }) {
            cancel.store(true, Ordering::SeqCst);
        }
        evs.push(e.clone());
    });
    let session = evs
        .iter()
        .find_map(|e| match e {
            WatchEvent::AutoIngestCancelled { session, .. } => Some(*session),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected a cancel: {evs:#?}"));
    let progress = evs
        .iter()
        .filter(|e| matches!(e, WatchEvent::AutoIngestProgress { .. }))
        .count();
    assert_eq!(progress, 1, "stops after the file in flight");

    // `ingest --resume` finishes it.
    let mut cat = w.lib.open_catalog().unwrap();
    let no = AtomicBool::new(false);
    let mut p = |_: &keepsake_core::ingest::Progress| {};
    let fault = FaultPlan::none();
    let mut ctx = keepsake_core::ingest::RunContext {
        prober: None,
        fault: &fault,
        cancel: &no,
        progress: &mut p,
        retry_failed: true,
    };
    let r = keepsake_core::ingest::run(&mut cat, &w.lib, session, &mut ctx).unwrap();
    assert!(r.safe_to_format);
}

#[test]
fn forgetting_a_card_keeps_the_library() {
    let w = world();
    save_preset(&w, &[w.root.join("A"), w.root.join("B")]);
    let mut watcher = Watcher::new(&w.lib, None, auto());
    drive(&mut watcher, &[arrived(&w.card, "1A2B"), removed(&w.card)]);
    trust(&w, Some("Two drives"));
    drive(&mut watcher, &[arrived(&w.card, "1A2B"), removed(&w.card)]);
    let rec = cards::list(&w.lib).unwrap().pop().unwrap();
    assert!(cards::find(&w.lib, "untitled").unwrap().is_some());
    assert!(cards::find(&w.lib, &rec.id.to_string()[..8])
        .unwrap()
        .is_some());
    assert!(cards::forget(&w.lib, rec.id).unwrap());
    assert!(cards::list(&w.lib).unwrap().is_empty());
    let cat = w.lib.open_catalog().unwrap();
    assert!(cat.read(|tx| tx.assets().count()).unwrap() > 0);
    // Seen again: a new card, everything counts as new (it is all in the
    // library already, but this card's history was forgotten).
    let evs = drive(&mut watcher, &[arrived(&w.card, "1A2B")]);
    let (known, n, new) = inserted(&evs);
    assert!(!known && n == new);
}

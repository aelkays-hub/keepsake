//! Device backends: the fake one driving the watcher end to end, and the
//! real OS backend starting, listing and stopping.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use keepsake_core::library::Library;
use keepsake_core::watch::{WatchEvent, WatchOptions, Watcher};
use keepsake_platform::{system_devices, DeviceEvent, DeviceEvents, FakeDevices, VolumeIdentity};

fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(rel)
}

fn copy_dir(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let to = dst.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &to);
        } else {
            std::fs::copy(e.path(), to).unwrap();
        }
    }
}

#[test]
fn fake_backend_drives_the_watcher() {
    let t = tempfile::tempdir().unwrap();
    let lib = Library::create(&t.path().join("lib")).unwrap();
    let card = t.path().join("card");
    copy_dir(&fixture("xavc"), &card);
    let stick = t.path().join("stick");
    std::fs::create_dir_all(&stick).unwrap();
    let id = VolumeIdentity {
        fs_serial: Some("ABCD-1234".into()),
        fs_type: Some("exFAT".into()),
        ..Default::default()
    };

    let mut fake = FakeDevices::new(vec![
        DeviceEvent::Arrived {
            mount: stick.clone(),
            identity: VolumeIdentity::default(),
        },
        DeviceEvent::Arrived {
            mount: card.clone(),
            identity: id.clone(),
        },
    ]);
    let (tx, rx) = channel();
    fake.start(tx).unwrap();
    assert!(fake.start(channel().0).is_err(), "cannot start twice");
    // Events pushed after start arrive live.
    assert!(fake.push(DeviceEvent::Removed {
        mount: card.clone()
    }));

    let mut watcher = Watcher::new(&lib, None, WatchOptions::default());
    let cancel = AtomicBool::new(false);
    let mut out = Vec::new();
    while let Ok(ev) = rx.recv_timeout(Duration::from_millis(200)) {
        watcher.handle(&ev, &cancel, &mut |e| out.push(e.clone()));
    }
    fake.stop();
    let kinds: Vec<&str> = out
        .iter()
        .map(|e| match e {
            WatchEvent::VolumeIgnored { .. } => "ignored",
            WatchEvent::CardInserted { .. } => "inserted",
            WatchEvent::CardRemoved { .. } => "removed",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["ignored", "inserted", "removed"], "{out:#?}");
}

#[test]
fn fake_backend_reads_a_script_and_hangs_up() {
    let t = tempfile::tempdir().unwrap();
    let script = t.path().join("events.jsonl");
    std::fs::write(
        &script,
        "# a card in, then out\n\
         {\"event\":\"arrived\",\"mount\":\"E:\\\\\",\"identity\":{\"fs_serial\":\"1\"}}\n\
         \n\
         {\"event\":\"removed\",\"mount\":\"E:\\\\\"}\n",
    )
    .unwrap();
    let mut fake = FakeDevices::from_jsonl(&script).unwrap().closing();
    let (tx, rx) = channel();
    fake.start(tx).unwrap();
    let got: Vec<DeviceEvent> = rx.iter().collect(); // ends: the fake hung up
    assert_eq!(got.len(), 2);

    std::fs::write(&script, "{\"event\":\"nope\"}\n").unwrap();
    let err = FakeDevices::from_jsonl(&script)
        .err()
        .map(|e| e.to_string());
    assert!(err.is_some_and(|e| e.contains(":1:")));
}

#[cfg(any(windows, target_os = "macos"))]
#[test]
fn the_system_backend_lists_mounted_volumes_and_stops() {
    let mut dev = system_devices().unwrap();
    let (tx, rx) = channel();
    dev.start(tx).unwrap();
    // Every machine has at least its system volume mounted.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut arrived = Vec::new();
    while Instant::now() < deadline {
        match rx.recv_timeout(Duration::from_millis(300)) {
            Ok(DeviceEvent::Arrived { mount, identity }) => arrived.push((mount, identity)),
            Ok(_) => {}
            Err(_) if !arrived.is_empty() => break,
            Err(_) => {}
        }
    }
    assert!(!arrived.is_empty(), "no volumes listed");
    assert!(
        arrived.iter().any(|(_, id)| id.fs_type.is_some()),
        "{arrived:#?}"
    );
    let started = Instant::now();
    dev.stop();
    assert!(started.elapsed() < Duration::from_secs(2), "stop is prompt");
    // After stop the sender is gone.
    assert!(rx.recv_timeout(Duration::from_secs(5)).is_err());
}

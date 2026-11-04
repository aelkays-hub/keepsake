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

/// Collect events until `done` says so or `secs` pass.
#[cfg(any(windows, target_os = "macos"))]
fn collect(
    rx: &std::sync::mpsc::Receiver<DeviceEvent>,
    secs: u64,
    mut done: impl FnMut(&[DeviceEvent]) -> bool,
) -> Vec<DeviceEvent> {
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut got = Vec::new();
    while Instant::now() < deadline && !done(&got) {
        if let Ok(ev) = rx.recv_timeout(Duration::from_millis(300)) {
            got.push(ev);
        }
    }
    got
}

#[cfg(any(windows, target_os = "macos"))]
fn stop_promptly(
    mut dev: keepsake_platform::SystemDevices,
    rx: &std::sync::mpsc::Receiver<DeviceEvent>,
) {
    let started = Instant::now();
    dev.stop();
    assert!(started.elapsed() < Duration::from_secs(2), "stop is prompt");
    // After stop the sender is gone.
    while rx.recv_timeout(Duration::from_secs(5)).is_ok() {}
    assert!(rx.try_recv().is_err());
}

/// Windows: the system drive is a drive letter, so it is always listed.
#[cfg(windows)]
#[test]
fn the_system_backend_lists_mounted_volumes_and_stops() {
    let mut dev = system_devices().unwrap();
    let (tx, rx) = channel();
    dev.start(tx).unwrap();
    let got = collect(&rx, 10, |g| !g.is_empty());
    let ok = got.iter().any(|e| {
        matches!(e, DeviceEvent::Arrived { identity, .. }
            if identity.fs_type.is_some() && identity.fs_serial.is_some())
    });
    assert!(ok, "{got:#?}");
    stop_promptly(dev, &rx);
}

/// macOS: the boot volume is not in /Volumes as a mount point, so mount a
/// real exFAT disk image (like a card) and watch it arrive and leave.
#[cfg(target_os = "macos")]
#[test]
fn the_system_backend_sees_a_disk_image_mount_and_unmount() {
    use std::process::Command;
    let t = tempfile::tempdir().unwrap();
    let dmg = t.path().join("card.dmg");
    let name = format!("KS{}", std::process::id());
    let ok = Command::new("hdiutil")
        .args([
            "create", "-quiet", "-size", "8m", "-fs", "ExFAT", "-volname", &name,
        ])
        .arg(&dmg)
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        eprintln!("skipping: hdiutil cannot create an exFAT image here");
        return;
    }

    let mut dev = system_devices().unwrap();
    let (tx, rx) = channel();
    dev.start(tx).unwrap();
    // Let the initial listing settle before mounting.
    collect(&rx, 2, |_| false);

    let out = Command::new("hdiutil")
        .args(["attach", "-nobrowse", "-plist"])
        .arg(&dmg)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let is_ours = |m: &Path| {
        m.file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with(&name))
    };
    let got = collect(&rx, 15, |g| {
        g.iter()
            .any(|e| matches!(e, DeviceEvent::Arrived { mount, .. } if is_ours(mount)))
    });
    let (mount, identity) = got
        .iter()
        .find_map(|e| match e {
            DeviceEvent::Arrived { mount, identity } if is_ours(mount) => {
                Some((mount.clone(), identity.clone()))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no arrival for {name}: {got:#?}"));
    assert_eq!(
        identity.label.as_deref(),
        Some(name.as_str()),
        "{identity:?}"
    );
    assert!(
        identity
            .fs_type
            .as_deref()
            .is_some_and(|k| k.to_lowercase().contains("fat")),
        "{identity:?}"
    );
    // Card identity depends on this.
    assert!(identity.fs_serial.is_some(), "{identity:?}");
    assert!(identity.capacity.unwrap_or(0) > 0, "{identity:?}");
    assert_eq!(
        keepsake_platform::identify_volume(&mount).and_then(|i| i.fs_serial),
        identity.fs_serial
    );
    assert_eq!(
        keepsake_platform::volume_root(&mount),
        std::fs::canonicalize(&mount).ok()
    );

    let detached = Command::new("hdiutil")
        .args(["detach", "-force"])
        .arg(&mount)
        .status()
        .is_ok_and(|s| s.success());
    assert!(detached);
    let got = collect(&rx, 15, |g| {
        g.iter()
            .any(|e| matches!(e, DeviceEvent::Removed { mount: m } if m == &mount))
    });
    assert!(
        got.iter()
            .any(|e| matches!(e, DeviceEvent::Removed { mount: m } if m == &mount)),
        "no removal: {got:#?}"
    );
    stop_promptly(dev, &rx);
}

//! Volume arrival and removal.
//!
//! Both OS backends work the same way:
//! - a background thread lists the mounted volumes and reports what changed
//!   since the last listing, as [`DeviceEvent`]s;
//! - OS notifications (`CM_Register_Notification` on Windows,
//!   DiskArbitration on macOS) only wake that thread, which then lists
//!   again a few times while the new volume finishes mounting;
//! - a slow periodic listing catches anything a notification missed (a card
//!   swapped in a reader, a drive letter changed).
//!
//! Listing is cheap: drive letters or `/Volumes`, plus one identity query
//! per volume. Nothing is written to any volume.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use thiserror::Error;

pub use keepsake_core::devices::{DeviceEvent, VolumeIdentity};

/// Errors from device backends.
#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("device notifications are not supported on this system")]
    Unsupported,
    #[error("{0}")]
    Os(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("already started")]
    AlreadyStarted,
}

/// A source of volume arrival and removal events.
pub trait DeviceEvents: Send {
    /// Start sending events to `tx`: first an `Arrived` for every volume
    /// already mounted, then changes as they happen, until [`Self::stop`]
    /// is called, the backend is dropped, or `tx`'s receiver is dropped.
    fn start(&mut self, tx: Sender<DeviceEvent>) -> Result<(), PlatformError>;

    /// Stop sending events. Idempotent.
    fn stop(&mut self);
}

/// The backend for this OS.
pub fn system_devices() -> Result<SystemDevices, PlatformError> {
    if cfg!(any(windows, target_os = "macos")) {
        Ok(SystemDevices::default())
    } else {
        Err(PlatformError::Unsupported)
    }
}

/// The identity of the volume mounted at `mount` (a volume root).
pub fn identify_volume(mount: &Path) -> Option<VolumeIdentity> {
    #[cfg(windows)]
    {
        crate::windows::identity(mount)
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos::identity(mount)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = mount;
        None
    }
}

/// The root of the volume holding `path`, if `path` is itself a volume
/// root (a card), else `None` (a folder on some disk).
pub fn volume_root(path: &Path) -> Option<PathBuf> {
    let canon = std::fs::canonicalize(path).ok()?;
    #[cfg(windows)]
    {
        crate::windows::volume_root_of(&canon)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // A mount point has a different device than its parent (or is `/`).
        let dev = std::fs::metadata(&canon).ok()?.dev();
        match canon.parent() {
            None => Some(canon),
            Some(parent) => {
                let pdev = std::fs::metadata(parent).ok()?.dev();
                (pdev != dev).then_some(canon)
            }
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = canon;
        None
    }
}

/// What a backend lists: mounted volumes and their identity.
pub(crate) type Listing = Vec<(PathBuf, VolumeIdentity)>;

/// Remembers the last listing and reports differences.
#[derive(Default)]
pub(crate) struct Scanner {
    known: HashMap<PathBuf, String>,
}

/// What must stay the same for a mount to be "the same volume".
fn fingerprint(id: &VolumeIdentity) -> String {
    format!(
        "{:?}|{:?}|{:?}|{:?}",
        id.fs_serial, id.fs_type, id.capacity, id.os_volume_id
    )
}

impl Scanner {
    /// Diff `now` against the previous listing. Returns the events, in a
    /// stable order: removals first, then arrivals.
    pub(crate) fn diff(&mut self, now: Listing) -> Vec<DeviceEvent> {
        let mut out = Vec::new();
        let fresh: HashMap<PathBuf, (String, VolumeIdentity)> = now
            .into_iter()
            .map(|(m, id)| (m, (fingerprint(&id), id)))
            .collect();
        let mut gone: Vec<PathBuf> = self
            .known
            .iter()
            .filter(|(m, fp)| fresh.get(*m).map(|(f, _)| f) != Some(*fp))
            .map(|(m, _)| m.clone())
            .collect();
        gone.sort();
        for m in gone {
            self.known.remove(&m);
            out.push(DeviceEvent::Removed { mount: m });
        }
        let mut came: Vec<(PathBuf, String, VolumeIdentity)> = fresh
            .into_iter()
            .filter(|(m, _)| !self.known.contains_key(m))
            .map(|(m, (fp, id))| (m, fp, id))
            .collect();
        came.sort_by(|a, b| a.0.cmp(&b.0));
        for (m, fp, id) in came {
            self.known.insert(m.clone(), fp);
            out.push(DeviceEvent::Arrived {
                mount: m,
                identity: id,
            });
        }
        out
    }
}

/// How long to keep listing quickly after a notification (a volume can
/// take a few seconds to get its drive letter or mount point).
pub(crate) const BURST: u32 = 12;
pub(crate) const BURST_INTERVAL: Duration = Duration::from_millis(500);
/// The safety-net listing interval.
pub(crate) const IDLE_INTERVAL: Duration = Duration::from_secs(3);

/// How often a waiting scanner checks for `stop`.
const STOP_CHECK: Duration = Duration::from_millis(200);

/// The OS's device notifications, from the scanner thread's side.
pub(crate) trait Notifier {
    /// Wait up to `timeout`; true if a device notification arrived.
    fn wait(&mut self, timeout: Duration) -> bool;
}

/// The scanner thread: list, diff, send, wait; until `stop` or the
/// receiver goes away. `enrich` adds the slower identity details (device
/// model and serial) to volumes that just arrived.
pub(crate) fn scan_loop(
    list: impl Fn() -> Listing,
    enrich: impl Fn(&Path, &mut VolumeIdentity),
    notifier: &mut dyn Notifier,
    tx: &Sender<DeviceEvent>,
    stop: &AtomicBool,
) {
    let mut scanner = Scanner::default();
    let mut burst = 0u32;
    while !stop.load(Ordering::SeqCst) {
        for mut ev in scanner.diff(list()) {
            if let DeviceEvent::Arrived { mount, identity } = &mut ev {
                enrich(mount, identity);
            }
            tracing::debug!(?ev, "device event");
            if tx.send(ev).is_err() {
                return;
            }
        }
        let timeout = if burst > 0 {
            burst -= 1;
            BURST_INTERVAL
        } else {
            IDLE_INTERVAL
        };
        // Wait in short steps so `stop` takes effect quickly.
        let mut waited = Duration::ZERO;
        while waited < timeout && !stop.load(Ordering::SeqCst) {
            let step = STOP_CHECK.min(timeout - waited);
            if notifier.wait(step) {
                burst = BURST;
                break;
            }
            waited += step;
        }
    }
}

/// The OS backend: Windows or macOS.
#[derive(Default)]
pub struct SystemDevices {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl DeviceEvents for SystemDevices {
    fn start(&mut self, tx: Sender<DeviceEvent>) -> Result<(), PlatformError> {
        if self.thread.is_some() {
            return Err(PlatformError::AlreadyStarted);
        }
        self.stop.store(false, Ordering::SeqCst);
        let stop = self.stop.clone();
        // The thread reports whether notifications could be registered
        // before it starts listing.
        let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<(), PlatformError>>();
        let handle = std::thread::Builder::new()
            .name("keepsake-devices".into())
            .spawn(move || run_backend(tx, stop, ready_tx))?;
        match ready_rx.recv() {
            Ok(Ok(())) => {
                self.thread = Some(handle);
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => {
                let _ = handle.join();
                Err(PlatformError::Os("device thread exited".into()))
            }
        }
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for SystemDevices {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_backend(
    tx: Sender<DeviceEvent>,
    stop: Arc<AtomicBool>,
    ready: Sender<Result<(), PlatformError>>,
) {
    #[cfg(windows)]
    {
        crate::windows::run(tx, &stop, ready)
    }
    #[cfg(target_os = "macos")]
    {
        crate::macos::run(tx, &stop, ready)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (tx, stop);
        let _ = ready.send(Err(PlatformError::Unsupported));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(serial: &str) -> VolumeIdentity {
        VolumeIdentity {
            fs_serial: Some(serial.into()),
            ..Default::default()
        }
    }

    #[test]
    fn scanner_reports_arrivals_removals_and_swaps() {
        let mut s = Scanner::default();
        let e = PathBuf::from("E:\\");
        let f = PathBuf::from("F:\\");
        let ev = s.diff(vec![(e.clone(), id("A")), (f.clone(), id("B"))]);
        assert_eq!(ev.len(), 2);
        assert!(ev.iter().all(|e| matches!(e, DeviceEvent::Arrived { .. })));
        assert!(s
            .diff(vec![(e.clone(), id("A")), (f.clone(), id("B"))])
            .is_empty());
        // F removed; a different card swapped into E.
        let ev = s.diff(vec![(e.clone(), id("C"))]);
        assert_eq!(
            ev,
            vec![
                DeviceEvent::Removed { mount: e.clone() },
                DeviceEvent::Removed { mount: f.clone() },
                DeviceEvent::Arrived {
                    mount: e.clone(),
                    identity: id("C")
                },
            ]
        );
    }

    struct Ticks(u32);
    impl Notifier for Ticks {
        fn wait(&mut self, _: Duration) -> bool {
            self.0 += 1;
            false
        }
    }

    #[test]
    fn scan_loop_stops_when_the_receiver_goes() {
        let (tx, rx) = std::sync::mpsc::channel();
        drop(rx);
        let stop = AtomicBool::new(false);
        let mut n = Ticks(0);
        scan_loop(
            || vec![(PathBuf::from("E:\\"), id("A"))],
            |_, _| {},
            &mut n,
            &tx,
            &stop,
        );
        assert_eq!(n.0, 0, "returned at the first failed send");
    }

    #[test]
    fn volume_root_of_a_plain_folder_is_none() {
        let t = tempfile::tempdir().unwrap();
        let sub = t.path().join("card-copy");
        std::fs::create_dir_all(&sub).unwrap();
        assert_eq!(volume_root(&sub), None);
    }
}

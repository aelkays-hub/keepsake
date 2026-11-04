//! A scripted [`DeviceEvents`] backend for tests and for driving
//! `keepsake watch` without hardware.

use std::io::BufRead;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use crate::devices::{DeviceEvent, DeviceEvents, PlatformError};

/// Sends a fixed list of events when started, then any pushed later
/// through [`FakeDevices::push`].
#[derive(Default, Clone)]
pub struct FakeDevices {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    script: Vec<DeviceEvent>,
    tx: Option<Sender<DeviceEvent>>,
    /// Hang up after the script, so the receiver sees the end.
    close: bool,
}

impl FakeDevices {
    pub fn new(script: Vec<DeviceEvent>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                script,
                tx: None,
                close: false,
            })),
        }
    }

    /// Hang up after sending the script (the receiver then sees the end of
    /// events, e.g. `keepsake watch --fake-events` exits).
    pub fn closing(self) -> Self {
        if let Ok(mut g) = self.inner.lock() {
            g.close = true;
        }
        self
    }

    /// Read a script: one JSON [`DeviceEvent`] per line (blank lines and
    /// lines starting with `#` are skipped).
    pub fn from_jsonl(path: &Path) -> Result<Self, PlatformError> {
        let f = std::fs::File::open(path)?;
        let mut script = Vec::new();
        for (n, line) in std::io::BufReader::new(f).lines().enumerate() {
            let line = line?;
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') {
                continue;
            }
            let ev: DeviceEvent = serde_json::from_str(t)
                .map_err(|e| PlatformError::Os(format!("{}:{}: {e}", path.display(), n + 1)))?;
            script.push(ev);
        }
        Ok(Self::new(script))
    }

    /// Send an event now (after [`DeviceEvents::start`]); before start it is
    /// added to the script. Returns false if the receiver is gone.
    pub fn push(&self, ev: DeviceEvent) -> bool {
        let Ok(mut g) = self.inner.lock() else {
            return false;
        };
        match &g.tx {
            Some(tx) => tx.send(ev).is_ok(),
            None => {
                g.script.push(ev);
                true
            }
        }
    }
}

impl DeviceEvents for FakeDevices {
    fn start(&mut self, tx: Sender<DeviceEvent>) -> Result<(), PlatformError> {
        let mut g = self
            .inner
            .lock()
            .map_err(|_| PlatformError::Os("fake devices poisoned".into()))?;
        if g.tx.is_some() {
            return Err(PlatformError::AlreadyStarted);
        }
        for ev in g.script.drain(..) {
            if tx.send(ev).is_err() {
                break;
            }
        }
        if !g.close {
            g.tx = Some(tx);
        }
        Ok(())
    }

    fn stop(&mut self) {
        if let Ok(mut g) = self.inner.lock() {
            g.tx = None;
        }
    }
}

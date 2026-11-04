//! What the operating system tells us about volumes coming and going.
//!
//! The OS backends live in `keepsake-platform`; they produce these events.
//! Everything that decides what an event *means* (is it a Sony card, have we
//! seen it, should we import it) lives in [`crate::cards`] and
//! [`crate::watch`], so it can be tested without hardware.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// What the OS reports about a mounted volume.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeIdentity {
    /// File-system serial number (FAT/exFAT volume ID). Set when the card
    /// is formatted, so it identifies one card across readers and slots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fs_serial: Option<String>,
    /// File-system type, e.g. `exFAT`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fs_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// Size of the volume in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<u64>,
    /// The OS's own volume id (Windows volume GUID path, macOS volume UUID).
    /// Stable on one machine; informational only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_volume_id: Option<String>,
    #[serde(default)]
    pub removable: bool,
    /// Serial of the device the volume is on, when reported. In a card
    /// reader this is the reader's serial, shared by every card put in it,
    /// so it is recorded but never used to identify a card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hw_serial: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hw_model: Option<String>,
}

impl VolumeIdentity {
    /// The key a card is remembered by: file-system type, serial and size.
    /// `None` when the OS did not report a serial.
    pub fn card_key(&self) -> Option<String> {
        let serial = self.fs_serial.as_deref()?.trim();
        if serial.is_empty() {
            return None;
        }
        Some(format!(
            "card:{}:{}:{}",
            self.fs_type.as_deref().unwrap_or("?").to_ascii_lowercase(),
            serial.to_ascii_uppercase(),
            self.capacity.unwrap_or(0)
        ))
    }
}

/// A change in the set of mounted volumes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum DeviceEvent {
    /// A volume is mounted and readable at `mount`. Backends also send this
    /// for every volume already mounted when they start.
    Arrived {
        mount: PathBuf,
        #[serde(default)]
        identity: VolumeIdentity,
    },
    /// The volume that was at `mount` is gone.
    Removed { mount: PathBuf },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_key_needs_a_serial_and_normalises_case() {
        let mut id = VolumeIdentity {
            fs_serial: Some("1a2b-3c4d".into()),
            fs_type: Some("exFAT".into()),
            capacity: Some(64_000_000_000),
            ..Default::default()
        };
        assert_eq!(
            id.card_key().as_deref(),
            Some("card:exfat:1A2B-3C4D:64000000000")
        );
        id.fs_serial = Some("  ".into());
        assert_eq!(id.card_key(), None);
        id.fs_serial = None;
        assert_eq!(id.card_key(), None);
    }

    #[test]
    fn events_round_trip_as_json() {
        let e = DeviceEvent::Arrived {
            mount: PathBuf::from("E:\\"),
            identity: VolumeIdentity {
                fs_serial: Some("X".into()),
                removable: true,
                ..Default::default()
            },
        };
        let s = serde_json::to_string(&e).unwrap_or_default();
        assert!(s.contains("\"event\":\"arrived\""), "{s}");
        let back: DeviceEvent = serde_json::from_str(&s).unwrap_or(DeviceEvent::Removed {
            mount: PathBuf::new(),
        });
        assert_eq!(back, e);
        let r: Result<DeviceEvent, _> =
            serde_json::from_str(r#"{"event":"removed","mount":"E:\\"}"#);
        assert!(r.is_ok());
    }
}

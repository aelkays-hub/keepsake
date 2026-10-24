//! OS-level adapters behind traits.
//!
//! Platform-specific implementations live in cfg-gated modules; callers
//! depend only on the traits and functions exposed here.

pub mod devices;
pub mod fake;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

pub use devices::{
    identify_volume, system_devices, volume_root, DeviceEvents, PlatformError, SystemDevices,
};
pub use fake::FakeDevices;
pub use keepsake_core::devices::{DeviceEvent, VolumeIdentity};

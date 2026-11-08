//! OS-level adapters behind traits.
//!
//! Platform-specific implementations live in cfg-gated modules; callers
//! depend only on the traits and functions exposed here.

pub mod devices;
pub mod fake;
pub mod power;
pub mod secrets;

#[cfg(windows)]
mod job;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

pub use devices::{
    identify_volume, system_devices, volume_root, DeviceEvents, PlatformError, SystemDevices,
};
pub use fake::FakeDevices;
pub use keepsake_core::devices::{DeviceEvent, VolumeIdentity};
pub use power::{on_battery, power_source, PowerSource};

/// End Keepsake's helper processes (ExifTool, FFmpeg, …) when the app ends,
/// however it ends; call once at start-up. On Windows the app joins a job
/// object that does this. On macOS this does nothing: the app's exit hook
/// stops ExifTool, but after a crash or Force Quit a `-stay_open` ExifTool
/// keeps running (it polls a closed input forever, it does not end).
pub fn close_helpers_with_app() -> Result<(), PlatformError> {
    #[cfg(windows)]
    {
        job::close_helpers_with_app().map_err(PlatformError::Os)
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

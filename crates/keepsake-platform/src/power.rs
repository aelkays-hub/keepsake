//! Is the computer running on battery? Render workers pause while it is.
//!
//! - Windows: `GetSystemPowerStatus` (`ACLineStatus` 0 = on battery).
//! - macOS: IOKit `IOPSGetProvidingPowerSourceType` ("Battery Power").
//!
//! Desktops without a battery report AC. Anything unexpected is
//! `Unknown`, which does not pause.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerSource {
    Ac,
    Battery,
    Unknown,
}

/// Where power comes from right now.
pub fn power_source() -> PowerSource {
    imp::power_source()
}

/// True only when the OS says the computer runs on battery.
pub fn on_battery() -> bool {
    power_source() == PowerSource::Battery
}

#[cfg(windows)]
mod imp {
    use super::PowerSource;

    /// `SYSTEM_POWER_STATUS` (winbase.h).
    #[repr(C)]
    #[derive(Default)]
    struct SystemPowerStatus {
        ac_line_status: u8,
        battery_flag: u8,
        battery_life_percent: u8,
        system_status_flag: u8,
        battery_life_time: u32,
        battery_full_life_time: u32,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
    }

    pub fn power_source() -> PowerSource {
        let mut s = SystemPowerStatus::default();
        // SAFETY: `s` is a valid, writable SYSTEM_POWER_STATUS.
        if unsafe { GetSystemPowerStatus(&mut s) } == 0 {
            return PowerSource::Unknown;
        }
        match s.ac_line_status {
            0 => PowerSource::Battery,
            1 => PowerSource::Ac,
            _ => PowerSource::Unknown,
        }
    }

    #[cfg(test)]
    #[test]
    fn layout_matches_winbase() {
        assert_eq!(std::mem::size_of::<SystemPowerStatus>(), 12);
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use std::ffi::{c_char, c_void};

    use super::PowerSource;

    type CFTypeRef = *const c_void;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
        fn IOPSGetProvidingPowerSourceType(snapshot: CFTypeRef) -> CFTypeRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CFTypeRef);
        fn CFStringGetCString(s: CFTypeRef, buf: *mut c_char, size: isize, encoding: u32) -> u8;
    }

    pub fn power_source() -> PowerSource {
        // SAFETY: returns an owned snapshot (or null), released below.
        let snap = unsafe { IOPSCopyPowerSourcesInfo() };
        if snap.is_null() {
            return PowerSource::Unknown;
        }
        // SAFETY: `snap` is live; the returned string is not owned ("Get").
        let kind = unsafe { IOPSGetProvidingPowerSourceType(snap) };
        let mut buf = [0 as c_char; 64];
        // SAFETY: `kind` is a CFString when non-null; `buf` holds 64 bytes.
        let ok = !kind.is_null()
            && unsafe { CFStringGetCString(kind, buf.as_mut_ptr(), 64, 0x0800_0100) } != 0;
        let text: String = buf
            .iter()
            .take_while(|c| **c != 0)
            .map(|c| *c as u8 as char)
            .collect();
        // SAFETY: `snap` came from a "Copy" function and is released once.
        unsafe { CFRelease(snap) };
        match (ok, text.as_str()) {
            (true, "Battery Power") => PowerSource::Battery,
            (true, "AC Power") | (true, "UPS Power") => PowerSource::Ac,
            _ => PowerSource::Unknown,
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::PowerSource;

    pub fn power_source() -> PowerSource {
        PowerSource::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_source_is_answered() {
        // CI machines are on AC (or unknown); never panics.
        let p = power_source();
        assert!(matches!(
            p,
            PowerSource::Ac | PowerSource::Battery | PowerSource::Unknown
        ));
        assert_eq!(on_battery(), p == PowerSource::Battery);
    }
}

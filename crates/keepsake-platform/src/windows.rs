//! Windows backend.
//!
//! `CM_Register_Notification` for the volume device interface class wakes
//! the scanner on volume arrival and removal (including a card going into
//! or out of a reader). The scanner lists drive letters and asks each
//! volume for its identity:
//! - `GetVolumeInformationW`: label, file-system serial and type;
//! - `GetVolumeNameForVolumeMountPointW`: the volume GUID path;
//! - `GetDiskFreeSpaceExW`: capacity;
//! - `IOCTL_STORAGE_QUERY_PROPERTY`: device model, serial and bus.
//!
//! Volumes mounted in folders instead of drive letters are not watched.

use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use crate::devices::{scan_loop, DeviceEvent, Listing, Notifier, PlatformError, VolumeIdentity};

type Handle = *mut c_void;
type HCmNotification = *mut c_void;
type CmCallback =
    unsafe extern "system" fn(HCmNotification, *const c_void, i32, *const c_void, u32) -> u32;

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid {
    d1: u32,
    d2: u16,
    d3: u16,
    d4: [u8; 8],
}

/// `GUID_DEVINTERFACE_VOLUME` {53F5630D-B6BF-11D0-94F2-00A0C91EFB8B}.
const GUID_DEVINTERFACE_VOLUME: Guid = Guid {
    d1: 0x53f5_630d,
    d2: 0xb6bf,
    d3: 0x11d0,
    d4: [0x94, 0xf2, 0x00, 0xa0, 0xc9, 0x1e, 0xfb, 0x8b],
};

/// `CM_NOTIFY_FILTER` (cfgmgr32.h).
#[repr(C)]
struct CmNotifyFilter {
    cb_size: u32,
    flags: u32,
    filter_type: i32,
    reserved: u32,
    u: CmNotifyFilterUnion,
}

#[repr(C)]
union CmNotifyFilterUnion {
    class_guid: Guid,
    target: Handle,
    instance_id: [u16; 200], // MAX_DEVICE_ID_LEN
}

const CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE: i32 = 0;
const CR_SUCCESS: u32 = 0;

#[link(name = "cfgmgr32")]
extern "system" {
    fn CM_Register_Notification(
        filter: *const CmNotifyFilter,
        context: *const c_void,
        callback: CmCallback,
        notify: *mut HCmNotification,
    ) -> u32;
    fn CM_Unregister_Notification(notify: HCmNotification) -> u32;
}

const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const SEM_FAILCRITICALERRORS: u32 = 0x0001;
const SEM_NOOPENFILEERRORBOX: u32 = 0x8000;
const FILE_SHARE_READ_WRITE: u32 = 0x1 | 0x2;
const OPEN_EXISTING: u32 = 3;
const IOCTL_STORAGE_QUERY_PROPERTY: u32 = 0x002D_1400;

#[link(name = "kernel32")]
extern "system" {
    fn GetLogicalDrives() -> u32;
    fn GetDriveTypeW(root: *const u16) -> u32;
    fn GetVolumeInformationW(
        root: *const u16,
        name: *mut u16,
        name_len: u32,
        serial: *mut u32,
        max_component: *mut u32,
        flags: *mut u32,
        fs_name: *mut u16,
        fs_name_len: u32,
    ) -> i32;
    fn GetVolumeNameForVolumeMountPointW(mount: *const u16, name: *mut u16, len: u32) -> i32;
    fn GetVolumePathNameW(file: *const u16, out: *mut u16, len: u32) -> i32;
    fn GetDiskFreeSpaceExW(
        dir: *const u16,
        free_to_caller: *mut u64,
        total: *mut u64,
        total_free: *mut u64,
    ) -> i32;
    fn SetThreadErrorMode(new_mode: u32, old_mode: *mut u32) -> i32;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *mut c_void,
        disposition: u32,
        flags: u32,
        template: Handle,
    ) -> Handle;
    fn DeviceIoControl(
        device: Handle,
        code: u32,
        in_buf: *const c_void,
        in_len: u32,
        out_buf: *mut c_void,
        out_len: u32,
        returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn CloseHandle(h: Handle) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn from_wide(buf: &[u16]) -> String {
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// `\\?\E:\x` → `E:\x` (the Win32 calls below want plain paths).
fn plain(p: &Path) -> String {
    let s = p.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        s.into_owned()
    }
}

/// The volume root holding `path`, e.g. `E:\`.
fn root_of(path: &str) -> Option<String> {
    let w = wide(path);
    let mut out = vec![0u16; 1024];
    // SAFETY: `w` is NUL-terminated; `out` has `out.len()` writable u16s.
    let ok = unsafe { GetVolumePathNameW(w.as_ptr(), out.as_mut_ptr(), out.len() as u32) };
    (ok != 0).then(|| from_wide(&out))
}

pub(crate) fn volume_root_of(canon: &Path) -> Option<PathBuf> {
    let p = plain(canon);
    let root = root_of(&p)?;
    let norm = |s: &str| s.trim_end_matches('\\').to_lowercase();
    (norm(&root) == norm(&p)).then(|| PathBuf::from(root))
}

/// Identity from the cheap calls (no device handle opened).
fn basic_identity(root: &str) -> Option<VolumeIdentity> {
    let w = wide(root);
    let mut name = [0u16; 261];
    let mut fs = [0u16; 261];
    let (mut serial, mut max_comp, mut flags) = (0u32, 0u32, 0u32);
    // SAFETY: `w` is NUL-terminated; the buffers and out-pointers are valid
    // for the lengths given. Fails (returns 0) for an empty card reader.
    let ok = unsafe {
        GetVolumeInformationW(
            w.as_ptr(),
            name.as_mut_ptr(),
            name.len() as u32,
            &mut serial,
            &mut max_comp,
            &mut flags,
            fs.as_mut_ptr(),
            fs.len() as u32,
        )
    };
    if ok == 0 {
        return None;
    }
    let mut guid = vec![0u16; 64];
    // SAFETY: as above; `root` ends with a backslash, as the API requires.
    let has_guid = unsafe {
        GetVolumeNameForVolumeMountPointW(w.as_ptr(), guid.as_mut_ptr(), guid.len() as u32)
    };
    let (mut free, mut total, mut total_free) = (0u64, 0u64, 0u64);
    // SAFETY: `w` is NUL-terminated; the out-pointers are valid u64s.
    let has_size =
        unsafe { GetDiskFreeSpaceExW(w.as_ptr(), &mut free, &mut total, &mut total_free) };
    // SAFETY: `w` is NUL-terminated.
    let kind = unsafe { GetDriveTypeW(w.as_ptr()) };
    let label = from_wide(&name);
    Some(VolumeIdentity {
        fs_serial: Some(format!("{:04X}-{:04X}", serial >> 16, serial & 0xFFFF)),
        fs_type: Some(from_wide(&fs)).filter(|s| !s.is_empty()),
        label: Some(label).filter(|s| !s.trim().is_empty()),
        capacity: (has_size != 0).then_some(total),
        os_volume_id: (has_guid != 0).then(|| from_wide(&guid)),
        removable: kind == DRIVE_REMOVABLE,
        hw_serial: None,
        hw_model: None,
    })
}

/// Device model, serial and bus from `IOCTL_STORAGE_QUERY_PROPERTY`.
fn add_device_info(root: &str, id: &mut VolumeIdentity) {
    let letter = root.trim_end_matches('\\');
    let dev = wide(&format!(r"\\.\{letter}"));
    // SAFETY: `dev` is NUL-terminated. Access 0 is allowed for this query
    // and does not need administrator rights; the handle is closed below.
    let h = unsafe {
        CreateFileW(
            dev.as_ptr(),
            0,
            FILE_SHARE_READ_WRITE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if h.is_null() || h as isize == -1 {
        return;
    }
    // STORAGE_PROPERTY_QUERY { StorageDeviceProperty, PropertyStandardQuery, [0] }
    let query: [u32; 3] = [0, 0, 0];
    let mut out = vec![0u32; 256]; // 1 KiB, 4-byte aligned
    let mut got = 0u32;
    // SAFETY: `h` is a valid handle; the buffers are valid for the sizes
    // given; synchronous call (no OVERLAPPED).
    let ok = unsafe {
        DeviceIoControl(
            h,
            IOCTL_STORAGE_QUERY_PROPERTY,
            query.as_ptr().cast(),
            12,
            out.as_mut_ptr().cast(),
            (out.len() * 4) as u32,
            &mut got,
            std::ptr::null_mut(),
        )
    };
    // SAFETY: `h` came from CreateFileW and is closed exactly once.
    unsafe { CloseHandle(h) };
    if ok == 0 {
        return;
    }
    let bytes: Vec<u8> = out.iter().flat_map(|w| w.to_le_bytes()).collect();
    let bytes = &bytes[..(got as usize).min(bytes.len())];
    // STORAGE_DEVICE_DESCRIPTOR: RemovableMedia at 10; Vendor/Product/
    // Revision/Serial offsets at 12/16/20/24; BusType at 28.
    let u32_at = |o: usize| {
        bytes
            .get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .unwrap_or(0)
    };
    let str_at = |o: usize| -> Option<String> {
        let off = u32_at(o) as usize;
        if off == 0 || off >= bytes.len() {
            return None;
        }
        let end = bytes[off..]
            .iter()
            .position(|b| *b == 0)
            .map_or(bytes.len(), |e| off + e);
        let s = String::from_utf8_lossy(&bytes[off..end]).trim().to_owned();
        (!s.is_empty()).then_some(s)
    };
    let model = [str_at(12), str_at(16)]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    if !model.is_empty() {
        id.hw_model = Some(model);
    }
    id.hw_serial = str_at(24);
    let removable_media = bytes.get(10).is_some_and(|b| *b != 0);
    // BusTypeUsb = 7, BusTypeSd = 12, BusTypeMmc = 13.
    let bus = u32_at(28);
    if removable_media || matches!(bus, 7 | 12 | 13) {
        id.removable = true;
    }
}

pub(crate) fn identity(mount: &Path) -> Option<VolumeIdentity> {
    let root = root_of(&plain(mount))?;
    let mut id = basic_identity(&root)?;
    add_device_info(&root, &mut id);
    Some(id)
}

/// Mounted volumes on drive letters (local disks and removable drives).
fn list() -> Listing {
    // SAFETY: no arguments; returns a bit mask.
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let root = format!("{}:\\", char::from(b'A' + i as u8));
        let w = wide(&root);
        // SAFETY: `w` is NUL-terminated.
        let kind = unsafe { GetDriveTypeW(w.as_ptr()) };
        if kind != DRIVE_REMOVABLE && kind != DRIVE_FIXED {
            continue; // network, optical, RAM disks
        }
        if let Some(id) = basic_identity(&root) {
            out.push((PathBuf::from(root), id));
        }
    }
    out
}

/// Device-notification callback: runs on a system thread pool, so it only
/// wakes the scanner.
unsafe extern "system" fn on_notify(
    _h: HCmNotification,
    context: *const c_void,
    _action: i32,
    _data: *const c_void,
    _size: u32,
) -> u32 {
    if !context.is_null() {
        // SAFETY: `context` is the `Sender<()>` boxed in `run`, which stays
        // alive until after `CM_Unregister_Notification` returns (and that
        // call waits for running callbacks).
        let tx = unsafe { &*(context as *const Sender<()>) };
        let _ = tx.send(());
    }
    0 // ERROR_SUCCESS
}

struct Wake(Receiver<()>);

impl Notifier for Wake {
    fn wait(&mut self, timeout: Duration) -> bool {
        match self.0.recv_timeout(timeout) {
            Ok(()) => {
                while self.0.try_recv().is_ok() {}
                true
            }
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => {
                std::thread::sleep(timeout);
                false
            }
        }
    }
}

pub(crate) fn run(
    tx: Sender<DeviceEvent>,
    stop: &AtomicBool,
    ready: Sender<Result<(), PlatformError>>,
) {
    // Never show "insert a disk" dialogs for empty card readers.
    // SAFETY: plain flag call; the old mode is not needed.
    unsafe {
        SetThreadErrorMode(
            SEM_FAILCRITICALERRORS | SEM_NOOPENFILEERRORBOX,
            std::ptr::null_mut(),
        )
    };

    let (wake_tx, wake_rx) = channel::<()>();
    let context = Box::into_raw(Box::new(wake_tx));
    // SAFETY: an all-zero CM_NOTIFY_FILTER is valid (plain integers and a
    // union of plain data); the needed fields are set below.
    let mut filter: CmNotifyFilter = unsafe { std::mem::zeroed() };
    filter.cb_size = std::mem::size_of::<CmNotifyFilter>() as u32;
    filter.filter_type = CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE;
    filter.u.class_guid = GUID_DEVINTERFACE_VOLUME;
    let mut handle: HCmNotification = std::ptr::null_mut();
    // SAFETY: `filter` is a valid, initialised filter; `context` is a live
    // heap pointer freed only after unregistering; `handle` is writable.
    let rc = unsafe {
        CM_Register_Notification(&filter, context as *const c_void, on_notify, &mut handle)
    };
    if rc != CR_SUCCESS {
        // SAFETY: registration failed, so no callback holds `context`.
        drop(unsafe { Box::from_raw(context) });
        let _ = ready.send(Err(PlatformError::Os(format!(
            "CM_Register_Notification failed (CONFIGRET {rc})"
        ))));
        return;
    }
    let _ = ready.send(Ok(()));

    let mut notifier = Wake(wake_rx);
    scan_loop(
        list,
        |mount: &Path, id: &mut VolumeIdentity| add_device_info(&plain(mount), id),
        &mut notifier,
        &tx,
        stop,
    );

    // SAFETY: `handle` came from a successful registration and is
    // unregistered once; this waits for callbacks in flight to finish.
    unsafe { CM_Unregister_Notification(handle) };
    // SAFETY: no callback can run any more, so `context` is ours again.
    drop(unsafe { Box::from_raw(context) });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_has_the_c_layout() {
        // sizeof(CM_NOTIFY_FILTER) is 416 on 64-bit Windows.
        if cfg!(target_pointer_width = "64") {
            assert_eq!(std::mem::size_of::<CmNotifyFilter>(), 416);
        }
        assert_eq!(std::mem::size_of::<Guid>(), 16);
    }

    #[test]
    fn plain_strips_verbatim_prefixes() {
        assert_eq!(plain(Path::new(r"\\?\E:\DCIM")), r"E:\DCIM");
        assert_eq!(plain(Path::new(r"\\?\UNC\srv\share")), r"\\srv\share");
        assert_eq!(plain(Path::new(r"E:\")), r"E:\");
    }

    #[test]
    fn the_system_drive_is_listed_with_an_identity() {
        let list = list();
        let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        let root = PathBuf::from(format!("{sys}\\"));
        let (_, id) = list
            .iter()
            .find(|(m, _)| m == &root)
            .unwrap_or_else(|| panic!("{root:?} not in {list:?}"));
        assert!(id.fs_serial.is_some() && id.fs_type.is_some(), "{id:?}");
        assert!(id.capacity.unwrap_or(0) > 0);
        let full = identity(&root).unwrap();
        assert_eq!(full.fs_serial, id.fs_serial);
        assert_eq!(volume_root_of(&root), Some(root.clone()));
        assert_eq!(volume_root_of(&root.join("Windows")), None);
    }
}

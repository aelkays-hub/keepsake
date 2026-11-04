//! macOS backend.
//!
//! A DiskArbitration session, scheduled on the scanner thread's run loop,
//! reports disks appearing, disappearing and changing (a volume being
//! mounted is a description change); each callback only wakes the scanner.
//! The scanner lists the mount points in `/Volumes` and asks
//! DiskArbitration for each one's description:
//! - `VolumeUUID` (for FAT and exFAT cards, macOS derives it from the
//!   file-system serial), `VolumeKind`, `VolumeName`;
//! - `MediaSize`, `MediaRemovable`/`MediaEjectable`;
//! - `DeviceVendor` and `DeviceModel`.
//!
//! DiskArbitration does not report a hardware serial number.

use std::ffi::{c_char, c_void};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::devices::{scan_loop, DeviceEvent, Listing, Notifier, PlatformError, VolumeIdentity};

type CFTypeRef = *const c_void;
type CFAllocatorRef = *const c_void;
type CFStringRef = *const c_void;
type CFDictionaryRef = *const c_void;
type CFArrayRef = *const c_void;
type CFURLRef = *const c_void;
type CFRunLoopRef = *const c_void;
type CFIndex = isize;
type CFTypeID = usize;
type DASessionRef = *const c_void;
type DADiskRef = *const c_void;

type DiskCallback = extern "C" fn(DADiskRef, *mut c_void);
type DescriptionCallback = extern "C" fn(DADiskRef, CFArrayRef, *mut c_void);

const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const K_CF_NUMBER_SINT64_TYPE: CFIndex = 4;

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopDefaultMode: CFStringRef;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, return_after_source: u8) -> i32;
    fn CFRelease(cf: CFTypeRef);
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    fn CFStringGetTypeID() -> CFTypeID;
    fn CFNumberGetTypeID() -> CFTypeID;
    fn CFBooleanGetTypeID() -> CFTypeID;
    fn CFUUIDGetTypeID() -> CFTypeID;
    fn CFStringGetCString(s: CFStringRef, buf: *mut c_char, size: CFIndex, encoding: u32) -> u8;
    fn CFNumberGetValue(n: CFTypeRef, kind: CFIndex, out: *mut c_void) -> u8;
    fn CFBooleanGetValue(b: CFTypeRef) -> u8;
    fn CFUUIDCreateString(alloc: CFAllocatorRef, uuid: CFTypeRef) -> CFStringRef;
    fn CFDictionaryGetValue(d: CFDictionaryRef, key: *const c_void) -> *const c_void;
    fn CFURLCreateFromFileSystemRepresentation(
        alloc: CFAllocatorRef,
        buf: *const u8,
        len: CFIndex,
        is_dir: u8,
    ) -> CFURLRef;
}

#[link(name = "DiskArbitration", kind = "framework")]
extern "C" {
    static kDADiskDescriptionVolumeNameKey: CFStringRef;
    static kDADiskDescriptionVolumeKindKey: CFStringRef;
    static kDADiskDescriptionVolumeUUIDKey: CFStringRef;
    static kDADiskDescriptionMediaSizeKey: CFStringRef;
    static kDADiskDescriptionMediaRemovableKey: CFStringRef;
    static kDADiskDescriptionMediaEjectableKey: CFStringRef;
    static kDADiskDescriptionDeviceVendorKey: CFStringRef;
    static kDADiskDescriptionDeviceModelKey: CFStringRef;

    fn DASessionCreate(alloc: CFAllocatorRef) -> DASessionRef;
    fn DASessionScheduleWithRunLoop(s: DASessionRef, rl: CFRunLoopRef, mode: CFStringRef);
    fn DASessionUnscheduleFromRunLoop(s: DASessionRef, rl: CFRunLoopRef, mode: CFStringRef);
    fn DARegisterDiskAppearedCallback(
        s: DASessionRef,
        matching: CFDictionaryRef,
        cb: DiskCallback,
        ctx: *mut c_void,
    );
    fn DARegisterDiskDisappearedCallback(
        s: DASessionRef,
        matching: CFDictionaryRef,
        cb: DiskCallback,
        ctx: *mut c_void,
    );
    fn DARegisterDiskDescriptionChangedCallback(
        s: DASessionRef,
        matching: CFDictionaryRef,
        watch: CFArrayRef,
        cb: DescriptionCallback,
        ctx: *mut c_void,
    );
    fn DAUnregisterCallback(s: DASessionRef, cb: *mut c_void, ctx: *mut c_void);
    fn DADiskCreateFromVolumePath(
        alloc: CFAllocatorRef,
        s: DASessionRef,
        path: CFURLRef,
    ) -> DADiskRef;
    fn DADiskCopyDescription(disk: DADiskRef) -> CFDictionaryRef;
}

/// A DiskArbitration session, released on drop.
struct Session(DASessionRef);

impl Session {
    fn new() -> Option<Self> {
        // SAFETY: the default allocator (null) is valid; null result checked.
        let s = unsafe { DASessionCreate(std::ptr::null()) };
        (!s.is_null()).then_some(Self(s))
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: created by DASessionCreate and released once.
        unsafe { CFRelease(self.0) };
    }
}

/// A CF value of type `T` from a dictionary (not owned: do not release).
fn dict_get(d: CFDictionaryRef, key: CFStringRef, type_id: CFTypeID) -> Option<CFTypeRef> {
    // SAFETY: `d` is a live dictionary and `key` a CF string constant.
    let v = unsafe { CFDictionaryGetValue(d, key) };
    // SAFETY: `v` is a live CF object when non-null.
    (!v.is_null() && unsafe { CFGetTypeID(v) } == type_id).then_some(v)
}

fn cf_string(s: CFStringRef) -> Option<String> {
    let mut buf = vec![0 as c_char; 1024];
    // SAFETY: `s` is a live CFString; `buf` holds `buf.len()` bytes.
    let ok = unsafe {
        CFStringGetCString(
            s,
            buf.as_mut_ptr(),
            buf.len() as CFIndex,
            K_CF_STRING_ENCODING_UTF8,
        )
    };
    if ok == 0 {
        return None;
    }
    let bytes: Vec<u8> = buf
        .iter()
        .take_while(|c| **c != 0)
        .map(|c| *c as u8)
        .collect();
    let s = String::from_utf8_lossy(&bytes).trim().to_owned();
    (!s.is_empty()).then_some(s)
}

fn string_of(d: CFDictionaryRef, key: CFStringRef) -> Option<String> {
    // SAFETY: type id functions take no arguments.
    dict_get(d, key, unsafe { CFStringGetTypeID() }).and_then(cf_string)
}

fn bool_of(d: CFDictionaryRef, key: CFStringRef) -> bool {
    // SAFETY: the value is a CFBoolean (type checked by dict_get).
    dict_get(d, key, unsafe { CFBooleanGetTypeID() })
        .is_some_and(|b| unsafe { CFBooleanGetValue(b) } != 0)
}

fn u64_of(d: CFDictionaryRef, key: CFStringRef) -> Option<u64> {
    // SAFETY: type id functions take no arguments.
    let n = dict_get(d, key, unsafe { CFNumberGetTypeID() })?;
    let mut v: i64 = 0;
    // SAFETY: `n` is a CFNumber; `v` is a writable i64 for kCFNumberSInt64Type.
    let ok = unsafe { CFNumberGetValue(n, K_CF_NUMBER_SINT64_TYPE, (&mut v as *mut i64).cast()) };
    (ok != 0).then(|| u64::try_from(v).ok()).flatten()
}

fn uuid_of(d: CFDictionaryRef, key: CFStringRef) -> Option<String> {
    // SAFETY: type id functions take no arguments.
    let u = dict_get(d, key, unsafe { CFUUIDGetTypeID() })?;
    // SAFETY: `u` is a CFUUID; the returned string is owned and released.
    let s = unsafe { CFUUIDCreateString(std::ptr::null(), u) };
    if s.is_null() {
        return None;
    }
    let out = cf_string(s);
    // SAFETY: `s` was created above ("Create" rule) and is released once.
    unsafe { CFRelease(s) };
    out
}

fn identity_with(session: &Session, mount: &Path) -> Option<VolumeIdentity> {
    let bytes = mount.as_os_str().as_bytes();
    // SAFETY: `bytes` is valid for `len` bytes; null allocator is the default.
    let url = unsafe {
        CFURLCreateFromFileSystemRepresentation(
            std::ptr::null(),
            bytes.as_ptr(),
            bytes.len() as CFIndex,
            1,
        )
    };
    if url.is_null() {
        return None;
    }
    // SAFETY: `session.0` and `url` are live; `url` is released once.
    let disk = unsafe { DADiskCreateFromVolumePath(std::ptr::null(), session.0, url) };
    unsafe { CFRelease(url) };
    if disk.is_null() {
        return None;
    }
    // SAFETY: `disk` is live; it is released once after copying.
    let desc = unsafe { DADiskCopyDescription(disk) };
    unsafe { CFRelease(disk) };
    if desc.is_null() {
        return None;
    }
    // SAFETY: the description keys are constants exported by the framework.
    let id = unsafe {
        let vendor = string_of(desc, kDADiskDescriptionDeviceVendorKey);
        let model = string_of(desc, kDADiskDescriptionDeviceModelKey);
        let hw_model = [vendor, model]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        VolumeIdentity {
            fs_serial: uuid_of(desc, kDADiskDescriptionVolumeUUIDKey),
            fs_type: string_of(desc, kDADiskDescriptionVolumeKindKey),
            label: string_of(desc, kDADiskDescriptionVolumeNameKey),
            capacity: u64_of(desc, kDADiskDescriptionMediaSizeKey),
            os_volume_id: None,
            removable: bool_of(desc, kDADiskDescriptionMediaRemovableKey)
                || bool_of(desc, kDADiskDescriptionMediaEjectableKey),
            hw_serial: None,
            hw_model: (!hw_model.is_empty()).then_some(hw_model),
        }
    };
    // SAFETY: `desc` was copied above ("Copy" rule) and is released once.
    unsafe { CFRelease(desc) };
    Some(id)
}

pub(crate) fn identity(mount: &Path) -> Option<VolumeIdentity> {
    identity_with(&Session::new()?, mount)
}

/// Mount points in `/Volumes` (the boot volume's symlink is skipped).
fn list_with(session: &Session) -> Listing {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir("/Volumes") else {
        return out;
    };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() || !ft.is_dir() {
            continue;
        }
        let mount: PathBuf = e.path();
        if crate::devices::volume_root(&mount).is_none() {
            continue; // an empty folder left behind, not a mount point
        }
        if let Some(id) = identity_with(session, &mount) {
            out.push((mount, id));
        }
    }
    out
}

extern "C" fn on_disk(_disk: DADiskRef, ctx: *mut c_void) {
    if !ctx.is_null() {
        // SAFETY: `ctx` is the `AtomicBool` owned by `run`, which outlives
        // the registration; callbacks run on that same thread.
        unsafe { &*(ctx as *const AtomicBool) }.store(true, Ordering::SeqCst);
    }
}

extern "C" fn on_change(disk: DADiskRef, _keys: CFArrayRef, ctx: *mut c_void) {
    on_disk(disk, ctx);
}

struct RunLoopWake<'a> {
    flag: &'a AtomicBool,
}

impl Notifier for RunLoopWake<'_> {
    fn wait(&mut self, timeout: Duration) -> bool {
        if !self.flag.swap(false, Ordering::SeqCst) {
            // SAFETY: runs this thread's run loop, where the session is
            // scheduled; callbacks fire inside this call.
            unsafe { CFRunLoopRunInMode(kCFRunLoopDefaultMode, timeout.as_secs_f64(), 1) };
        }
        self.flag.swap(false, Ordering::SeqCst)
    }
}

pub(crate) fn run(
    tx: Sender<DeviceEvent>,
    stop: &AtomicBool,
    ready: Sender<Result<(), PlatformError>>,
) {
    let Some(session) = Session::new() else {
        let _ = ready.send(Err(PlatformError::Os("DASessionCreate failed".into())));
        return;
    };
    let flag = AtomicBool::new(false);
    let ctx = &flag as *const AtomicBool as *mut c_void;
    // SAFETY: the session is live; a null matching dictionary matches every
    // disk; `ctx` points to `flag`, which lives until after unregistering.
    // The run loop is this thread's own.
    let run_loop = unsafe {
        DARegisterDiskAppearedCallback(session.0, std::ptr::null(), on_disk, ctx);
        DARegisterDiskDisappearedCallback(session.0, std::ptr::null(), on_disk, ctx);
        DARegisterDiskDescriptionChangedCallback(
            session.0,
            std::ptr::null(),
            std::ptr::null(),
            on_change,
            ctx,
        );
        let rl = CFRunLoopGetCurrent();
        DASessionScheduleWithRunLoop(session.0, rl, kCFRunLoopDefaultMode);
        rl
    };
    let _ = ready.send(Ok(()));

    let mut notifier = RunLoopWake { flag: &flag };
    scan_loop(|| list_with(&session), |_, _| {}, &mut notifier, &tx, stop);

    // SAFETY: undo the registrations above on the same thread before
    // `flag` and the session go away.
    unsafe {
        DASessionUnscheduleFromRunLoop(session.0, run_loop, kCFRunLoopDefaultMode);
        // `on_disk` is registered twice (appeared and disappeared).
        let disk_cb = on_disk as DiskCallback as *mut c_void;
        DAUnregisterCallback(session.0, disk_cb, ctx);
        DAUnregisterCallback(session.0, disk_cb, ctx);
        DAUnregisterCallback(
            session.0,
            on_change as DescriptionCallback as *mut c_void,
            ctx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boot_volume_has_an_identity() {
        let id = identity(Path::new("/")).expect("identity of /");
        assert!(id.fs_type.is_some(), "{id:?}");
    }
}

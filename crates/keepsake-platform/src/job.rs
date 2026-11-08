//! Windows: Keepsake's helpers (ExifTool, FFmpeg, ffprobe, the WebView2
//! processes) end when the app does, however it ends.
//!
//! Closing the window ends the process without running destructors, so a
//! `-stay_open` ExifTool used to keep running. Left behind, it held its
//! `.exe` open and the next installer could not replace it. The app now
//! puts itself in a job object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
//! Every process it starts joins the job. When the app's process ends,
//! Windows closes the job's last handle and ends them all.
//!
//! Programs the user opens from Keepsake ("Open in", a folder in Explorer)
//! must outlive it: they are started with `CREATE_BREAKAWAY_FROM_JOB`
//! (`keepsake_core::external`), which the job allows
//! (`JOB_OBJECT_LIMIT_BREAKAWAY_OK`).

use std::ffi::c_void;
use std::sync::OnceLock;

type Handle = *mut c_void;

const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
const JOB_OBJECT_LIMIT_BREAKAWAY_OK: u32 = 0x0000_0800;
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;

#[repr(C)]
#[derive(Default)]
struct BasicLimitInformation {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[repr(C)]
#[derive(Default)]
struct IoCounters {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}

#[repr(C)]
#[derive(Default)]
struct ExtendedLimitInformation {
    basic: BasicLimitInformation,
    io: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, len: u32) -> i32;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    fn GetCurrentProcess() -> Handle;
    fn CloseHandle(h: Handle) -> i32;
}

/// An unnamed job that ends its processes when its last handle closes,
/// and lets a child break away when asked.
pub(crate) fn kill_on_close_job() -> Result<Handle, String> {
    // SAFETY: plain Win32 calls; `info` lives across the call, and its size
    // is passed with it.
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(format!(
                "CreateJobObjectW: {}",
                std::io::Error::last_os_error()
            ));
        }
        let mut info = ExtendedLimitInformation::default();
        info.basic.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
        let ok = SetInformationJobObject(
            job,
            JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
            (&info as *const ExtendedLimitInformation).cast(),
            std::mem::size_of::<ExtendedLimitInformation>() as u32,
        );
        if ok == 0 {
            let e = std::io::Error::last_os_error();
            CloseHandle(job);
            return Err(format!("SetInformationJobObject: {e}"));
        }
        Ok(job)
    }
}

/// Put `process` in `job`.
pub(crate) fn assign(job: Handle, process: Handle) -> Result<(), String> {
    // SAFETY: both are handles the caller owns.
    if unsafe { AssignProcessToJobObject(job, process) } == 0 {
        return Err(format!(
            "AssignProcessToJobObject: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// The job's handle, as a number so it can live in a static. Never closed:
/// it closes when the process ends, and that ends the helpers.
static JOB: OnceLock<Result<usize, String>> = OnceLock::new();

/// Put this process (and so everything it starts from now on) in the job.
/// Once per process; later calls return the first result.
pub(crate) fn close_helpers_with_app() -> Result<(), String> {
    JOB.get_or_init(|| {
        let job = kill_on_close_job()?;
        // SAFETY: the pseudo-handle of this process needs no closing.
        assign(job, unsafe { GetCurrentProcess() })?;
        Ok(job as usize)
    })
    .as_ref()
    .map(|_| ())
    .map_err(Clone::clone)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::io::AsRawHandle;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    fn sleeper() -> std::process::Child {
        // About 60 s: ping waits a second between replies.
        Command::new("ping")
            .args(["-n", "60", "127.0.0.1"])
            .stdout(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[test]
    fn closing_the_job_ends_its_processes() {
        let job = kill_on_close_job().unwrap();
        let mut child = sleeper();
        assign(job, child.as_raw_handle()).unwrap();
        assert!(child.try_wait().unwrap().is_none(), "still running");
        // SAFETY: the handle came from kill_on_close_job and is closed once.
        unsafe { CloseHandle(job) };
        let deadline = Instant::now() + Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline, "the child outlived the job");
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn a_process_outside_the_job_keeps_running() {
        let job = kill_on_close_job().unwrap();
        let mut inside = sleeper();
        let mut outside = sleeper();
        assign(job, inside.as_raw_handle()).unwrap();
        // SAFETY: as above.
        unsafe { CloseHandle(job) };
        let deadline = Instant::now() + Duration::from_secs(10);
        while inside.try_wait().unwrap().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(outside.try_wait().unwrap().is_none(), "outside still runs");
        let _ = outside.kill();
        let _ = outside.wait();
    }
}

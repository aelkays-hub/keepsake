//! Helper processes end when Keepsake ends.
//!
//! ExifTool (`-stay_open`) never exits by itself: on a closed stdin it
//! keeps polling. If Keepsake crashes or is killed, or quits without
//! running its exit hook, the helper would run forever and lock its own
//! file.
//!
//! - Windows: the process joins a Job Object with
//!   `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. Children join it too. The kernel
//!   closes the job's last handle when Keepsake ends, however it ends, and
//!   that ends every process still in the job. The job allows breakaway, so
//!   apps Keepsake opens a file in (`keepsake_core::external`) can leave it
//!   and keep running.
//! - macOS: nothing here. The app's exit hook stops ExifTool; a crash can
//!   still leave it running (see `docs/DECISIONS.md`, ADR-033).

use crate::PlatformError;

/// Make the OS end this process's helper processes when it ends, however
/// it ends. Call it before starting any. Calling it again does nothing.
pub fn end_helpers_with_this_process() -> Result<(), PlatformError> {
    imp::end_helpers_with_this_process()
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::sync::Mutex;

    use crate::PlatformError;

    type Handle = *mut c_void;

    /// `JOBOBJECT_BASIC_LIMIT_INFORMATION` (winnt.h).
    #[repr(C)]
    #[derive(Default)]
    pub(super) struct BasicLimits {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        pub(super) limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    /// `IO_COUNTERS` (winnt.h).
    #[repr(C)]
    #[derive(Default)]
    pub(super) struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    /// `JOBOBJECT_EXTENDED_LIMIT_INFORMATION` (winnt.h).
    #[repr(C)]
    #[derive(Default)]
    pub(super) struct ExtendedLimits {
        pub(super) basic: BasicLimits,
        io: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
    const JOB_OBJECT_LIMIT_BREAKAWAY_OK: u32 = 0x0000_0800;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, len: u32) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
        #[cfg(test)]
        fn IsProcessInJob(process: Handle, job: Handle, result: *mut i32) -> i32;
        fn GetCurrentProcess() -> Handle;
        fn CloseHandle(h: Handle) -> i32;
    }

    fn last_error(what: &str) -> PlatformError {
        PlatformError::Os(format!("{what}: {}", std::io::Error::last_os_error()))
    }

    /// A kill-on-close job. Dropping it ends every process in it.
    pub(super) struct Job(Handle);

    // SAFETY: a job handle may be used and closed from any thread.
    unsafe impl Send for Job {}

    impl Job {
        pub(super) fn new() -> Result<Self, PlatformError> {
            // SAFETY: both arguments may be null (default security, no name).
            let h = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
            if h.is_null() {
                return Err(last_error("CreateJobObjectW"));
            }
            let job = Job(h);
            let mut limits = ExtendedLimits::default();
            limits.basic.limit_flags =
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_BREAKAWAY_OK;
            // SAFETY: `job.0` is a valid job handle; `limits` is a live
            // JOBOBJECT_EXTENDED_LIMIT_INFORMATION of the size given.
            let ok = unsafe {
                SetInformationJobObject(
                    job.0,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    (&limits as *const ExtendedLimits).cast(),
                    std::mem::size_of::<ExtendedLimits>() as u32,
                )
            };
            if ok == 0 {
                return Err(last_error("SetInformationJobObject"));
            }
            Ok(job)
        }

        /// Put `process` (a process handle with `PROCESS_SET_QUOTA` and
        /// `PROCESS_TERMINATE` access) in the job.
        pub(super) fn assign(&self, process: Handle) -> Result<(), PlatformError> {
            // SAFETY: `self.0` is a valid job handle; the caller passes a
            // valid process handle.
            if unsafe { AssignProcessToJobObject(self.0, process) } == 0 {
                return Err(last_error("AssignProcessToJobObject"));
            }
            Ok(())
        }

        /// Whether `process` (a handle with `PROCESS_QUERY_LIMITED_INFORMATION`
        /// access) is in the job.
        #[cfg(test)]
        pub(super) fn contains(&self, process: Handle) -> bool {
            let mut inside = 0;
            // SAFETY: both handles are valid; `inside` is a live BOOL.
            let ok = unsafe { IsProcessInJob(process, self.0, &mut inside) };
            ok != 0 && inside != 0
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: `self.0` came from CreateJobObjectW and is closed
            // exactly once.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// The process's own job, kept open until the process ends (the kernel
    /// then closes it).
    static OWN: Mutex<Option<Job>> = Mutex::new(None);

    pub fn end_helpers_with_this_process() -> Result<(), PlatformError> {
        let mut own = OWN.lock().unwrap_or_else(|p| p.into_inner());
        if own.is_some() {
            return Ok(());
        }
        let job = Job::new()?;
        // SAFETY: the pseudo-handle of the current process is always valid
        // and has all access rights. Since Windows 8 this works even when
        // the process is already in a job (jobs nest).
        job.assign(unsafe { GetCurrentProcess() })?;
        *own = Some(job);
        Ok(())
    }

    /// Whether `process` is in this process's own job (false before
    /// [`end_helpers_with_this_process`]).
    #[cfg(test)]
    pub(super) fn own_job_contains(process: Handle) -> bool {
        let own = OWN.lock().unwrap_or_else(|p| p.into_inner());
        own.as_ref().is_some_and(|j| j.contains(process))
    }
}

#[cfg(not(windows))]
mod imp {
    use crate::PlatformError;

    pub fn end_helpers_with_this_process() -> Result<(), PlatformError> {
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::imp::{own_job_contains, ExtendedLimits, Job};
    use std::io::Read;
    use std::os::windows::io::AsRawHandle;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    #[test]
    fn limits_have_the_c_layout() {
        // sizeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION) is 144 on 64-bit Windows.
        if cfg!(target_pointer_width = "64") {
            assert_eq!(std::mem::size_of::<ExtendedLimits>(), 144);
        }
    }

    /// The child and the process it starts after joining the job both end
    /// when the job closes.
    #[test]
    fn closing_the_job_ends_the_child_and_its_children() {
        let job = Job::new().unwrap();
        // cmd waits a second (so it is in the job before going on), then
        // starts a ping that would run for a minute.
        let mut child = Command::new("cmd")
            .args(["/c", "ping -n 2 127.0.0.1 >nul & ping -n 60 127.0.0.1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        job.assign(child.as_raw_handle()).unwrap();
        // stdout closes only once both cmd and the long ping have ended.
        let mut out = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = out.read_to_end(&mut buf);
            let _ = tx.send(buf);
        });
        // Let the long ping start and print its first reply.
        std::thread::sleep(Duration::from_secs(3));
        assert!(child.try_wait().unwrap().is_none(), "cmd ended early");
        let closed = Instant::now();
        drop(job);
        // (A job kills with exit code 0, so only the timing tells.)
        child.wait().unwrap();
        let printed = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the ping started by cmd is still running");
        assert!(closed.elapsed() < Duration::from_secs(10));
        assert!(!printed.is_empty(), "the long ping never started");
    }

    fn ping() -> Vec<String> {
        vec!["-n".into(), "30".into(), "127.0.0.1".into()]
    }

    /// Helpers join the process's job; an app opened from Keepsake leaves
    /// it, so it keeps running after Keepsake.
    #[test]
    fn helpers_join_the_job_and_opened_apps_leave_it() {
        super::end_helpers_with_this_process().unwrap();
        let mut helper = Command::new("ping")
            .args(ping())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let mut app = keepsake_core::external::spawn_detached("ping", &ping()).unwrap();
        let helper_inside = own_job_contains(helper.as_raw_handle());
        let app_inside = own_job_contains(app.as_raw_handle());
        for c in [&mut helper, &mut app] {
            let _ = c.kill();
            let _ = c.wait();
        }
        assert!(helper_inside, "a helper is outside the job");
        // An outer job (a terminal, a CI runner) may forbid leaving it; the
        // app then starts inside, as before.
        use std::os::windows::process::CommandExt;
        let can_leave = Command::new("cmd")
            .args(["/c", "exit"])
            .creation_flags(0x0100_0000) // CREATE_BREAKAWAY_FROM_JOB
            .status()
            .is_ok();
        if can_leave {
            assert!(!app_inside, "the opened app is still in the job");
        }
    }

    #[test]
    fn joining_twice_is_harmless() {
        super::end_helpers_with_this_process().unwrap();
        super::end_helpers_with_this_process().unwrap();
    }
}

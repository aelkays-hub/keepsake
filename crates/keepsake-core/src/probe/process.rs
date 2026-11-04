//! Run a child process to completion with a deadline.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::error::ProbeError;

/// Output of a finished process.
pub(crate) struct Finished {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub success: bool,
}

fn drain<R: Read + Send + 'static>(r: Option<R>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut r) = r {
            let _ = r.read_to_end(&mut buf);
        }
        buf
    })
}

/// Run `cmd`, killing it if it has not exited after `timeout`.
pub(crate) fn run(
    tool: &'static str,
    mut cmd: Command,
    timeout: Duration,
) -> Result<Finished, ProbeError> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    hide_window(&mut cmd);
    let mut child = cmd
        .spawn()
        .map_err(|source| ProbeError::Spawn { tool, source })?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ProbeError::Timeout {
                tool,
                secs: timeout.as_secs(),
            });
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(Finished {
        stdout: out.join().unwrap_or_default(),
        stderr: err.join().unwrap_or_default(),
        success: status.success(),
    })
}

/// Do not flash a console window for each child on Windows.
pub(crate) fn hide_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

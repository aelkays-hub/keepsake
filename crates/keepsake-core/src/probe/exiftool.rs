//! A long-lived `exiftool -stay_open` process behind a request queue.
//!
//! One worker thread owns the process. Callers on any thread send requests
//! and wait for their reply. A request that exceeds its timeout kills the
//! process; the next request starts a fresh one. A crashed process is
//! restarted the same way, so one bad file never wedges the queue.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use super::error::ProbeError;
use super::process::hide_window;

const TOOL: &str = "exiftool";

/// Arguments applied to every request.
const COMMON_ARGS: &[&str] = &["-charset", "filename=utf8", "-api", "largefilesupport=1"];

/// Tags Keepsake reads. `-n` is not used globally so values such as
/// `SubSecTimeOriginal = "05"` keep their leading zeros; numeric tags are
/// requested with `#` (ExifTool's per-tag "no print conversion").
pub const DEFAULT_TAGS: &[&str] = &[
    "-DateTimeOriginal",
    "-SubSecTimeOriginal",
    "-OffsetTimeOriginal",
    "-OffsetTime",
    "-CreateDate",
    "-MediaCreateDate",
    "-TimeZone",
    "-Make",
    "-Model",
    "-LensModel",
    "-LensID",
    "-SequenceNumber#",
    "-ReleaseMode2#",
    "-ShotNumberSinceVideoOn#",
    "-SerialNumber",
    "-Composite:GPSLatitude#",
    "-Composite:GPSLongitude#",
    "-Composite:GPSAltitude#",
    "-ImageWidth#",
    "-ImageHeight#",
    "-Duration#",
    "-MIMEType",
];

enum Msg {
    Run {
        args: Vec<String>,
        timeout: Duration,
        reply: mpsc::Sender<Result<String, ProbeError>>,
    },
    Shutdown,
}

struct Proc {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<String>,
    errors: mpsc::Receiver<String>,
}

impl Proc {
    fn spawn(exe: &Path) -> Result<Self, ProbeError> {
        let mut cmd = Command::new(exe);
        cmd.args(["-stay_open", "True", "-@", "-", "-common_args"])
            .args(COMMON_ARGS)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        hide_window(&mut cmd);
        let mut child = cmd
            .spawn()
            .map_err(|source| ProbeError::Spawn { tool: TOOL, source })?;
        let stdin = child.stdin.take().ok_or_else(|| ProbeError::Crashed {
            tool: TOOL,
            detail: "no stdin".into(),
        })?;
        let lines = pump(child.stdout.take());
        let errors = pump(child.stderr.take());
        Ok(Self {
            child,
            stdin,
            lines,
            errors,
        })
    }

    fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn stop(mut self) {
        let _ = self.stdin.write_all(b"-stay_open\nFalse\n");
        let _ = self.stdin.flush();
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        self.kill();
    }

    fn stderr_text(&self) -> String {
        let mut s = String::new();
        while let Ok(l) = self.errors.try_recv() {
            s.push_str(&l);
            s.push('\n');
        }
        s
    }
}

/// Forward lines from a pipe into a channel (ends when the pipe closes).
fn pump<R: std::io::Read + Send + 'static>(r: Option<R>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    if let Some(r) = r {
        std::thread::spawn(move || {
            for line in BufReader::new(r).lines() {
                match line {
                    Ok(l) => {
                        if tx.send(l).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }
    rx
}

struct Worker {
    exe: PathBuf,
    proc: Option<Proc>,
    seq: u64,
    restarts: Arc<AtomicU64>,
}

impl Worker {
    fn run(&mut self, args: &[String], timeout: Duration) -> Result<String, ProbeError> {
        // A write failure means the process died between requests: restart once.
        match self.send(args) {
            Ok(seq) => self.read_reply(seq, timeout),
            Err(_) => {
                self.discard();
                let seq = self.send(args)?;
                self.read_reply(seq, timeout)
            }
        }
    }

    fn send(&mut self, args: &[String]) -> Result<u64, ProbeError> {
        if self.proc.is_none() {
            self.proc = Some(Proc::spawn(&self.exe)?);
        }
        self.seq += 1;
        let seq = self.seq;
        let p = self.proc.as_mut().ok_or_else(|| ProbeError::Crashed {
            tool: TOOL,
            detail: "no process".into(),
        })?;
        let mut buf = String::new();
        for a in args {
            buf.push_str(a);
            buf.push('\n');
        }
        buf.push_str(&format!("-execute{seq}\n"));
        p.stdin.write_all(buf.as_bytes())?;
        p.stdin.flush()?;
        Ok(seq)
    }

    fn read_reply(&mut self, seq: u64, timeout: Duration) -> Result<String, ProbeError> {
        let marker = format!("{{ready{seq}}}");
        let deadline = Instant::now() + timeout;
        let mut out = String::new();
        loop {
            let Some(p) = self.proc.as_ref() else {
                return Err(ProbeError::Crashed {
                    tool: TOOL,
                    detail: "no process".into(),
                });
            };
            let left = deadline.saturating_duration_since(Instant::now());
            match p.lines.recv_timeout(left) {
                Ok(line) if line.trim_end() == marker => {
                    let err = p.stderr_text();
                    if out.trim().is_empty() && !err.trim().is_empty() {
                        return Err(ProbeError::Tool {
                            tool: TOOL,
                            detail: err.trim().to_owned(),
                        });
                    }
                    return Ok(out);
                }
                Ok(line) => {
                    out.push_str(&line);
                    out.push('\n');
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    self.discard();
                    return Err(ProbeError::Timeout {
                        tool: TOOL,
                        secs: timeout.as_secs(),
                    });
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let detail = p.stderr_text();
                    self.discard();
                    return Err(ProbeError::Crashed {
                        tool: TOOL,
                        detail: if detail.is_empty() {
                            "output closed".into()
                        } else {
                            detail
                        },
                    });
                }
            }
        }
    }

    /// Kill the current process; the next request starts a new one.
    fn discard(&mut self) {
        if let Some(p) = self.proc.take() {
            p.kill();
            self.restarts.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Handle to the shared ExifTool process. Cheap to share by reference
/// across threads; requests are served one at a time in arrival order.
pub struct ExifTool {
    tx: Mutex<mpsc::Sender<Msg>>,
    worker: Option<JoinHandle<()>>,
    timeout: Duration,
    restarts: Arc<AtomicU64>,
}

impl ExifTool {
    /// Start the worker. The process itself starts on the first request.
    pub fn start(exe: &Path, timeout: Duration) -> Result<Self, ProbeError> {
        let ok = std::fs::metadata(exe).is_ok_and(|m| m.is_file() && m.len() > 0);
        if !ok {
            return Err(ProbeError::ToolMissing {
                tool: TOOL,
                path: exe.to_path_buf(),
            });
        }
        let (tx, rx) = mpsc::channel::<Msg>();
        let restarts = Arc::new(AtomicU64::new(0));
        let mut w = Worker {
            exe: exe.to_path_buf(),
            proc: None,
            seq: 0,
            restarts: Arc::clone(&restarts),
        };
        let worker = std::thread::Builder::new()
            .name("exiftool".into())
            .spawn(move || {
                for msg in rx {
                    match msg {
                        Msg::Run {
                            args,
                            timeout,
                            reply,
                        } => {
                            let _ = reply.send(w.run(&args, timeout));
                        }
                        Msg::Shutdown => break,
                    }
                }
                if let Some(p) = w.proc.take() {
                    p.stop();
                }
            })
            .map_err(|source| ProbeError::Spawn { tool: TOOL, source })?;
        Ok(Self {
            tx: Mutex::new(tx),
            worker: Some(worker),
            timeout,
            restarts,
        })
    }

    /// How many times the process was killed and replaced.
    pub fn restarts(&self) -> u64 {
        self.restarts.load(Ordering::Relaxed)
    }

    /// Run one ExifTool command (arguments without `-execute`) and return stdout.
    pub fn execute(&self, args: &[String]) -> Result<String, ProbeError> {
        self.execute_with_timeout(args, self.timeout)
    }

    pub fn execute_with_timeout(
        &self,
        args: &[String],
        timeout: Duration,
    ) -> Result<String, ProbeError> {
        if let Some(bad) = args.iter().find(|a| a.contains(['\n', '\r'])) {
            return Err(ProbeError::BadPath {
                tool: TOOL,
                path: bad.clone(),
            });
        }
        let (reply, rx) = mpsc::channel();
        {
            let tx = self.tx.lock().map_err(|_| ProbeError::Crashed {
                tool: TOOL,
                detail: "queue poisoned".into(),
            })?;
            tx.send(Msg::Run {
                args: args.to_vec(),
                timeout,
                reply,
            })
            .map_err(|_| ProbeError::Crashed {
                tool: TOOL,
                detail: "worker stopped".into(),
            })?;
        }
        rx.recv().map_err(|_| ProbeError::Crashed {
            tool: TOOL,
            detail: "worker stopped".into(),
        })?
    }

    /// Read [`DEFAULT_TAGS`] from several files in one request. Returns one
    /// JSON object per file, in the order given.
    pub fn read_tags(&self, paths: &[&Path]) -> Result<Vec<Map<String, Value>>, ProbeError> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let mut args: Vec<String> = vec!["-j".into(), "-q".into(), "-q".into()];
        args.extend(DEFAULT_TAGS.iter().map(|s| s.to_string()));
        for p in paths {
            args.push(path_arg(p)?);
        }
        let out = self.execute(&args)?;
        if out.trim().is_empty() {
            return Err(ProbeError::Tool {
                tool: TOOL,
                detail: "no output (unreadable file?)".into(),
            });
        }
        let v: Value =
            serde_json::from_str(&out).map_err(|e| ProbeError::parse("exiftool JSON", e))?;
        let arr = match v {
            Value::Array(a) => a,
            _ => return Err(ProbeError::parse("exiftool JSON", "expected an array")),
        };
        let mut objs: Vec<Map<String, Value>> = arr
            .into_iter()
            .filter_map(|o| match o {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .collect();
        // ExifTool keeps argument order, but match on SourceFile to be safe.
        let mut ordered = Vec::with_capacity(paths.len());
        for p in paths {
            let want = path_arg(p)?.replace('\\', "/");
            let idx = objs.iter().position(|o| {
                o.get("SourceFile")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.replace('\\', "/") == want)
            });
            ordered.push(match idx {
                Some(i) => objs.swap_remove(i),
                None => Map::new(),
            });
        }
        Ok(ordered)
    }
}

impl Drop for ExifTool {
    fn drop(&mut self) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(Msg::Shutdown);
        }
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

/// A path as an ExifTool argument (UTF-8, no Windows verbatim prefix).
pub(crate) fn path_arg(p: &Path) -> Result<String, ProbeError> {
    let s = p.to_str().ok_or_else(|| ProbeError::BadPath {
        tool: TOOL,
        path: p.display().to_string(),
    })?;
    let s = if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        s.to_owned()
    };
    if s.contains(['\n', '\r']) {
        return Err(ProbeError::BadPath {
            tool: TOOL,
            path: s,
        });
    }
    Ok(s)
}

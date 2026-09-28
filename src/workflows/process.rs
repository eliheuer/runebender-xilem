// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded subprocess supervision for native worker adapters.
//!
//! The caller chooses the executable, arguments, environment, and working directory.
//! This module only replaces standard handles and supervises the direct child process.
//! It is not an operating-system sandbox and does not kill a descendant process tree.
//! A child can write more than the configured capture limit to disk between polling intervals;
//! retained bytes and line callbacks stay bounded, and the next poll reports the overflow.

use std::fmt;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A clonable request to stop one process before it produces a terminal result.
#[derive(Clone, Debug, Default)]
pub struct ProcessCancellation(Arc<AtomicBool>);

impl ProcessCancellation {
    /// Request cancellation and return whether this call first set the flag.
    pub fn cancel(&self) -> bool {
        !self.0.swap(true, Ordering::AcqRel)
    }

    /// Return whether cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Wall-clock and retained byte limits for one subprocess.
#[derive(Clone, Copy, Debug)]
pub struct ProcessLimits {
    /// Maximum time spent supervising the child after spawn.
    pub deadline: Duration,
    /// Maximum number of input bytes supplied on standard input.
    pub stdin_bytes: usize,
    /// Maximum retained standard output bytes.
    pub stdout_bytes: usize,
    /// Maximum retained standard error bytes.
    pub stderr_bytes: usize,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            deadline: Duration::from_secs(30 * 60),
            stdin_bytes: 1024 * 1024,
            stdout_bytes: 4 * 1024 * 1024,
            stderr_bytes: 256 * 1024,
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
const MAX_DEADLINE: Duration = Duration::from_secs(24 * 60 * 60);
#[cfg(not(target_arch = "wasm32"))]
const MAX_STREAM_BYTES: usize = 64 * 1024 * 1024;

#[cfg(not(target_arch = "wasm32"))]
impl ProcessLimits {
    fn validate(self, input_len: usize) -> Result<(), String> {
        if self.deadline.is_zero() || self.deadline > MAX_DEADLINE {
            return Err("deadline must be greater than zero and no longer than 24 hours".into());
        }
        for (name, bound) in [
            ("stdin_bytes", self.stdin_bytes),
            ("stdout_bytes", self.stdout_bytes),
            ("stderr_bytes", self.stderr_bytes),
        ] {
            if bound > MAX_STREAM_BYTES {
                return Err(format!("{name} must be no greater than 67108864 bytes"));
            }
        }
        if input_len > self.stdin_bytes {
            return Err(format!(
                "input has {input_len} bytes, exceeding the {} byte stdin limit",
                self.stdin_bytes
            ));
        }
        Ok(())
    }
}

/// Which captured diagnostic stream produced a line or exceeded its limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputStream {
    /// Child standard output.
    Stdout,
    /// Child standard error.
    Stderr,
}

/// Terminal state of one supervised subprocess.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessOutcome {
    /// The direct child exited, whether successfully or unsuccessfully.
    Exited {
        /// Whether the operating system reported successful exit.
        success: bool,
        /// Platform exit code, when available.
        code: Option<i32>,
    },
    /// Cancellation stopped the direct child or prevented its spawn.
    Cancelled {
        /// Whether a child had been spawned.
        started: bool,
    },
    /// The child exceeded its wall-clock deadline.
    DeadlineExceeded,
    /// A capture exceeded its retained byte limit.
    OutputLimitExceeded {
        /// The stream whose limit was exceeded.
        stream: OutputStream,
    },
    /// The command could not be spawned.
    SpawnFailed(String),
    /// Capture setup, polling, or process supervision failed.
    IoFailed(String),
    /// A limit or the supplied input was invalid.
    InvalidRequest(String),
    /// Native subprocesses are unavailable in a browser build.
    BrowserUnavailable,
}

impl fmt::Display for ProcessOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exited { success: true, .. } => formatter.write_str("worker exited successfully"),
            Self::Exited { code, .. } => write!(formatter, "worker exited with status {code:?}"),
            Self::Cancelled { .. } => formatter.write_str("cancelled"),
            Self::DeadlineExceeded => formatter.write_str("worker exceeded its deadline"),
            Self::OutputLimitExceeded {
                stream: OutputStream::Stdout,
            } => formatter.write_str("worker stdout exceeded its byte limit"),
            Self::OutputLimitExceeded {
                stream: OutputStream::Stderr,
            } => formatter.write_str("worker stderr exceeded its byte limit"),
            Self::SpawnFailed(error) => write!(formatter, "could not start worker: {error}"),
            Self::IoFailed(error) => write!(formatter, "worker I/O failed: {error}"),
            Self::InvalidRequest(error) => write!(formatter, "invalid worker request: {error}"),
            Self::BrowserUnavailable => {
                formatter.write_str("native subprocesses are unavailable in the browser")
            }
        }
    }
}

/// Bounded terminal bytes and the reason the process stopped.
#[derive(Clone, Debug)]
pub struct ProcessOutput {
    /// Terminal process result.
    pub outcome: ProcessOutcome,
    /// Captured standard output, truncated to the configured limit on failure.
    pub stdout: Vec<u8>,
    /// Captured standard error, truncated to the configured limit on failure.
    pub stderr: Vec<u8>,
}

impl ProcessOutput {
    fn empty(outcome: ProcessOutcome) -> Self {
        Self {
            outcome,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }
}

/// Run one caller-configured child with file-backed standard handles and bounded line progress.
///
/// Complete lines are sent to `on_line` without their newline.
/// Any final partial line is sent once after the process stops.
/// The direct child is killed and reaped on cancellation, deadline, output overflow, or I/O error.
pub fn run(
    command: &mut Command,
    input: &[u8],
    limits: ProcessLimits,
    cancel: &ProcessCancellation,
    on_line: impl FnMut(OutputStream, &str),
) -> ProcessOutput {
    #[cfg(not(target_arch = "wasm32"))]
    {
        native::run(command, input, limits, cancel, on_line)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (command, input, limits, cancel, on_line);
        ProcessOutput::empty(ProcessOutcome::BrowserUnavailable)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, Read, Write};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Stdio};
    use std::sync::atomic::AtomicU64;
    use std::time::Instant;

    use super::*;

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);
    const POLL_INTERVAL: Duration = Duration::from_millis(10);

    pub(super) fn run(
        command: &mut Command,
        input: &[u8],
        limits: ProcessLimits,
        cancel: &ProcessCancellation,
        mut on_line: impl FnMut(OutputStream, &str),
    ) -> ProcessOutput {
        if cancel.is_cancelled() {
            return ProcessOutput::empty(ProcessOutcome::Cancelled { started: false });
        }
        if let Err(error) = limits.validate(input.len()) {
            return ProcessOutput::empty(ProcessOutcome::InvalidRequest(error));
        }
        let temporary = match TemporaryDirectory::new() {
            Ok(temporary) => temporary,
            Err(error) => return ProcessOutput::empty(ProcessOutcome::IoFailed(error.to_string())),
        };
        let stdin_path = temporary.path.join("stdin");
        let stdout_path = temporary.path.join("stdout");
        let stderr_path = temporary.path.join("stderr");
        if let Err(error) = write_input(&stdin_path, input) {
            return ProcessOutput::empty(ProcessOutcome::IoFailed(error.to_string()));
        }
        let stdin = match File::open(&stdin_path) {
            Ok(stdin) => stdin,
            Err(error) => return ProcessOutput::empty(ProcessOutcome::IoFailed(error.to_string())),
        };
        let (stdout_writer, mut stdout) = match Capture::new(&stdout_path, limits.stdout_bytes) {
            Ok(capture) => capture,
            Err(error) => return ProcessOutput::empty(ProcessOutcome::IoFailed(error.to_string())),
        };
        let (stderr_writer, mut stderr) = match Capture::new(&stderr_path, limits.stderr_bytes) {
            Ok(capture) => capture,
            Err(error) => return ProcessOutput::empty(ProcessOutcome::IoFailed(error.to_string())),
        };
        if cancel.is_cancelled() {
            return ProcessOutput::empty(ProcessOutcome::Cancelled { started: false });
        }
        command
            .stdin(Stdio::from(stdin))
            .stdout(Stdio::from(stdout_writer))
            .stderr(Stdio::from(stderr_writer));
        let child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                return ProcessOutput::empty(ProcessOutcome::SpawnFailed(error.to_string()));
            }
        };
        let mut child = ReapOnDrop(child);
        let started = Instant::now();
        let outcome = loop {
            let stdout_exceeded = match stdout.poll(OutputStream::Stdout, &mut on_line) {
                Ok(exceeded) => exceeded,
                Err(error) => {
                    break stop(&mut child.0, ProcessOutcome::IoFailed(error.to_string()));
                }
            };
            let stderr_exceeded = match stderr.poll(OutputStream::Stderr, &mut on_line) {
                Ok(exceeded) => exceeded,
                Err(error) => {
                    break stop(&mut child.0, ProcessOutcome::IoFailed(error.to_string()));
                }
            };
            if stdout_exceeded {
                break stop(
                    &mut child.0,
                    ProcessOutcome::OutputLimitExceeded {
                        stream: OutputStream::Stdout,
                    },
                );
            }
            if stderr_exceeded {
                break stop(
                    &mut child.0,
                    ProcessOutcome::OutputLimitExceeded {
                        stream: OutputStream::Stderr,
                    },
                );
            }
            if cancel.is_cancelled() {
                break stop(&mut child.0, ProcessOutcome::Cancelled { started: true });
            }
            if started.elapsed() >= limits.deadline {
                break stop(&mut child.0, ProcessOutcome::DeadlineExceeded);
            }
            match child.0.try_wait() {
                Ok(Some(status)) => {
                    let stdout_exceeded = match stdout.poll(OutputStream::Stdout, &mut on_line) {
                        Ok(exceeded) => exceeded,
                        Err(error) => break ProcessOutcome::IoFailed(error.to_string()),
                    };
                    let stderr_exceeded = match stderr.poll(OutputStream::Stderr, &mut on_line) {
                        Ok(exceeded) => exceeded,
                        Err(error) => break ProcessOutcome::IoFailed(error.to_string()),
                    };
                    if stdout_exceeded {
                        break ProcessOutcome::OutputLimitExceeded {
                            stream: OutputStream::Stdout,
                        };
                    }
                    if stderr_exceeded {
                        break ProcessOutcome::OutputLimitExceeded {
                            stream: OutputStream::Stderr,
                        };
                    }
                    if cancel.is_cancelled() {
                        break ProcessOutcome::Cancelled { started: true };
                    }
                    if started.elapsed() >= limits.deadline {
                        break ProcessOutcome::DeadlineExceeded;
                    }
                    break ProcessOutcome::Exited {
                        success: status.success(),
                        code: status.code(),
                    };
                }
                Ok(None) => {}
                Err(error) => {
                    break stop(&mut child.0, ProcessOutcome::IoFailed(error.to_string()));
                }
            }
            std::thread::sleep(POLL_INTERVAL);
        };
        // A child can exit between polls after writing its last bytes.
        // An overflow discovered here must still fail the run.
        let outcome = match stdout.poll(OutputStream::Stdout, &mut on_line) {
            Ok(true) if matches!(&outcome, ProcessOutcome::Exited { .. }) => {
                ProcessOutcome::OutputLimitExceeded {
                    stream: OutputStream::Stdout,
                }
            }
            Ok(_) => outcome,
            Err(error) => ProcessOutcome::IoFailed(error.to_string()),
        };
        let outcome = match stderr.poll(OutputStream::Stderr, &mut on_line) {
            Ok(true) if matches!(&outcome, ProcessOutcome::Exited { .. }) => {
                ProcessOutcome::OutputLimitExceeded {
                    stream: OutputStream::Stderr,
                }
            }
            Ok(_) => outcome,
            Err(error) => ProcessOutcome::IoFailed(error.to_string()),
        };
        stdout.finish(OutputStream::Stdout, &mut on_line);
        stderr.finish(OutputStream::Stderr, &mut on_line);
        let outcome = if matches!(&outcome, ProcessOutcome::Exited { .. }) && cancel.is_cancelled()
        {
            ProcessOutcome::Cancelled { started: true }
        } else if matches!(&outcome, ProcessOutcome::Exited { .. })
            && started.elapsed() >= limits.deadline
        {
            ProcessOutcome::DeadlineExceeded
        } else {
            outcome
        };
        ProcessOutput {
            outcome,
            stdout: stdout.bytes,
            stderr: stderr.bytes,
        }
    }

    // A host progress callback may unwind. Keep direct-child cleanup owned by
    // the stack rather than relying only on each explicit terminal branch.
    struct ReapOnDrop(Child);

    impl Drop for ReapOnDrop {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) && self.0.kill().is_ok() {
                let _ = self.0.wait();
            }
        }
    }

    fn stop(child: &mut Child, outcome: ProcessOutcome) -> ProcessOutcome {
        match child.kill() {
            Ok(()) => match child.wait() {
                Ok(_) => outcome,
                Err(error) => ProcessOutcome::IoFailed(format!("could not reap child: {error}")),
            },
            Err(kill_error) => match child.try_wait() {
                Ok(Some(_)) => outcome,
                Ok(None) => ProcessOutcome::IoFailed(format!("could not stop child: {kill_error}")),
                Err(error) => ProcessOutcome::IoFailed(format!("could not inspect child: {error}")),
            },
        }
    }

    struct Capture {
        reader: File,
        bytes: Vec<u8>,
        next_line: usize,
        scanned: usize,
        limit: usize,
    }

    impl Capture {
        fn new(path: &Path, limit: usize) -> io::Result<(File, Self)> {
            let writer = OpenOptions::new().write(true).create_new(true).open(path)?;
            let reader = File::open(path)?;
            Ok((
                writer,
                Self {
                    reader,
                    bytes: Vec::new(),
                    next_line: 0,
                    scanned: 0,
                    limit,
                },
            ))
        }

        fn poll(
            &mut self,
            stream: OutputStream,
            on_line: &mut impl FnMut(OutputStream, &str),
        ) -> io::Result<bool> {
            let length = self.reader.metadata()?.len();
            let exceeded = length > self.limit as u64;
            let remaining = self.limit - self.bytes.len();
            if remaining > 0 {
                Read::by_ref(&mut self.reader)
                    .take(remaining as u64)
                    .read_to_end(&mut self.bytes)?;
            }
            let new_end = self.bytes.len();
            for index in self.scanned..new_end {
                if self.bytes[index] == b'\n' {
                    let mut line_end = index;
                    if line_end > self.next_line && self.bytes[line_end - 1] == b'\r' {
                        line_end -= 1;
                    }
                    let line = String::from_utf8_lossy(&self.bytes[self.next_line..line_end]);
                    on_line(stream, &line);
                    self.next_line = index + 1;
                }
            }
            self.scanned = new_end;
            Ok(exceeded)
        }

        fn finish(&mut self, stream: OutputStream, on_line: &mut impl FnMut(OutputStream, &str)) {
            if self.next_line < self.bytes.len() {
                let line = String::from_utf8_lossy(&self.bytes[self.next_line..]);
                on_line(stream, &line);
                self.next_line = self.bytes.len();
            }
        }
    }

    fn write_input(path: &Path, input: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(input)?;
        file.sync_all()
    }

    struct TemporaryDirectory {
        path: PathBuf,
    }

    impl TemporaryDirectory {
        fn new() -> io::Result<Self> {
            for _ in 0..32 {
                let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "runebender-process-{}-{sequence}",
                    std::process::id()
                ));
                #[cfg(unix)]
                let created = {
                    use std::os::unix::fs::DirBuilderExt;
                    let mut builder = fs::DirBuilder::new();
                    builder.mode(0o700).create(&path)
                };
                #[cfg(not(unix))]
                let created = fs::create_dir(&path);
                match created {
                    Ok(()) => return Ok(Self { path }),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "could not allocate a private process directory",
            ))
        }
    }

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[cfg(all(test, unix))]
    mod tests {
        use super::*;

        fn shell(script: &str) -> Command {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", script]);
            command
        }

        fn run_shell(script: &str, input: &[u8], limits: ProcessLimits) -> ProcessOutput {
            run(
                &mut shell(script),
                input,
                limits,
                &ProcessCancellation::default(),
                |_, _| {},
            )
        }

        #[test]
        fn pre_cancel_does_not_spawn_or_create_side_effects() {
            let temporary = TemporaryDirectory::new().expect("temporary directory");
            let marker = temporary.path.join("marker");
            let mut command = shell("printf touched > \"$1\"");
            command.args(["sh", marker.to_str().expect("UTF-8 test path")]);
            let cancel = ProcessCancellation::default();
            assert!(cancel.cancel());
            assert!(!cancel.cancel());
            let output = run(
                &mut command,
                b"",
                ProcessLimits::default(),
                &cancel,
                |_, _| {},
            );
            assert_eq!(output.outcome, ProcessOutcome::Cancelled { started: false });
            assert!(!marker.exists());
        }

        #[test]
        fn reports_streamed_lines_partial_lines_and_nonzero_exit() {
            let mut lines = Vec::new();
            let output = run(
                &mut shell("printf 'one\\ntwo'; printf 'error\\n' >&2; exit 7"),
                b"",
                ProcessLimits::default(),
                &ProcessCancellation::default(),
                |stream, line| lines.push((stream, line.to_owned())),
            );
            assert_eq!(
                output.outcome,
                ProcessOutcome::Exited {
                    success: false,
                    code: Some(7)
                }
            );
            assert_eq!(output.stdout, b"one\ntwo");
            assert_eq!(output.stderr, b"error\n");
            assert!(lines.contains(&(OutputStream::Stdout, "one".into())));
            assert!(lines.contains(&(OutputStream::Stdout, "two".into())));
            assert!(lines.contains(&(OutputStream::Stderr, "error".into())));
            assert_eq!(lines.len(), 3);
        }

        #[test]
        fn kills_hung_child_at_deadline() {
            let limits = ProcessLimits {
                deadline: Duration::from_millis(80),
                ..ProcessLimits::default()
            };
            let started = Instant::now();
            let output = run_shell("while :; do :; done", b"", limits);
            assert_eq!(output.outcome, ProcessOutcome::DeadlineExceeded);
            assert!(started.elapsed() < Duration::from_secs(1));
        }

        #[test]
        fn stdout_and_stderr_overflow_are_terminal_failures() {
            for (script, stream) in [
                ("printf '12345'", OutputStream::Stdout),
                ("printf '12345' >&2", OutputStream::Stderr),
            ] {
                let limits = ProcessLimits {
                    stdout_bytes: 4,
                    stderr_bytes: 4,
                    ..ProcessLimits::default()
                };
                let output = run_shell(script, b"", limits);
                assert_eq!(
                    output.outcome,
                    ProcessOutcome::OutputLimitExceeded { stream }
                );
                assert!(output.stdout.len() <= 4);
                assert!(output.stderr.len() <= 4);
            }
        }

        #[test]
        fn file_backed_input_does_not_block_on_unread_stdin() {
            let input = vec![b'x'; 1024 * 1024];
            let started = Instant::now();
            let output = run_shell("printf done", &input, ProcessLimits::default());
            assert_eq!(
                output.outcome,
                ProcessOutcome::Exited {
                    success: true,
                    code: Some(0)
                }
            );
            assert_eq!(output.stdout, b"done");
            assert!(started.elapsed() < Duration::from_secs(2));
        }

        #[test]
        fn descendant_handles_do_not_hold_up_completion() {
            let started = Instant::now();
            let output = run_shell("sleep 2 & printf parent", b"", ProcessLimits::default());
            assert_eq!(
                output.outcome,
                ProcessOutcome::Exited {
                    success: true,
                    code: Some(0)
                }
            );
            assert_eq!(output.stdout, b"parent");
            assert!(started.elapsed() < Duration::from_secs(1));
        }

        #[test]
        fn cancellation_after_terminal_does_not_change_result() {
            let cancel = ProcessCancellation::default();
            let output = run(
                &mut shell("exit 0"),
                b"",
                ProcessLimits::default(),
                &cancel,
                |_, _| {},
            );
            assert_eq!(
                output.outcome,
                ProcessOutcome::Exited {
                    success: true,
                    code: Some(0)
                }
            );
            assert!(cancel.cancel());
            assert_eq!(
                output.outcome,
                ProcessOutcome::Exited {
                    success: true,
                    code: Some(0)
                }
            );
        }

        #[test]
        fn partial_line_callback_can_cancel_immediate_exit() {
            let cancel = ProcessCancellation::default();
            let output = run(
                &mut shell("printf partial"),
                b"",
                ProcessLimits::default(),
                &cancel,
                |stream, line| {
                    assert_eq!(stream, OutputStream::Stdout);
                    assert_eq!(line, "partial");
                    cancel.cancel();
                },
            );
            assert_eq!(output.outcome, ProcessOutcome::Cancelled { started: true });
            assert_eq!(output.stdout, b"partial");
        }

        #[test]
        fn callback_unwind_kills_and_reaps_direct_child() {
            let mut pid = String::new();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run(
                    &mut shell("printf '%s\\n' \"$$\"; exec sleep 10"),
                    b"",
                    ProcessLimits::default(),
                    &ProcessCancellation::default(),
                    |stream, line| {
                        assert_eq!(stream, OutputStream::Stdout);
                        pid = line.to_owned();
                        panic!("fixture progress callback failed");
                    },
                )
            }));
            assert!(result.is_err(), "fixture callback did not unwind");
            assert!(pid.parse::<u32>().is_ok(), "worker did not report its PID");
            let alive = Command::new("/bin/kill")
                .args(["-0", &pid])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .expect("inspect the fixture child");
            assert!(
                !alive.success(),
                "callback unwind left its child alive or unreaped"
            );
        }

        #[test]
        fn validates_all_bounds_before_spawn() {
            for limits in [
                ProcessLimits {
                    deadline: Duration::ZERO,
                    ..ProcessLimits::default()
                },
                ProcessLimits {
                    deadline: MAX_DEADLINE + Duration::from_secs(1),
                    ..ProcessLimits::default()
                },
                ProcessLimits {
                    stdin_bytes: MAX_STREAM_BYTES + 1,
                    ..ProcessLimits::default()
                },
                ProcessLimits {
                    stdout_bytes: MAX_STREAM_BYTES + 1,
                    ..ProcessLimits::default()
                },
                ProcessLimits {
                    stderr_bytes: MAX_STREAM_BYTES + 1,
                    ..ProcessLimits::default()
                },
                ProcessLimits {
                    stdin_bytes: 0,
                    ..ProcessLimits::default()
                },
            ] {
                let output = run_shell("exit 0", b"x", limits);
                assert!(matches!(output.outcome, ProcessOutcome::InvalidRequest(_)));
            }
        }
    }
}

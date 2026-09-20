// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded native subprocess jobs for immutable Python recipe captures.
//!
//! This is a process boundary, not an operating-system sandbox.
//! The selected interpreter and user-authored code retain the user's filesystem privileges.
//! Jobs receive no font paths, live application bindings, sockets, authorization, or credentials
//! from this runner.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use runebender::document::script_recipe::{ScriptRecipeInput, ScriptRecipeResult};

/// Maximum queued jobs accepted by one runner.
pub(crate) const MAX_SCRIPT_QUEUE_CAPACITY: usize = 8;
/// Maximum retained job records accepted by one runner.
pub(crate) const MAX_RETAINED_SCRIPT_JOBS: usize = 16;

const MAX_SCRIPT_BYTES: usize = 256 * 1024;
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_STDOUT_BYTES: usize = 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
const MAX_DEADLINE: Duration = Duration::from_secs(300);

/// Explicit Python runtime and bounded worker configuration.
#[derive(Clone, Debug)]
pub(crate) struct ScriptJobConfig {
    /// Interpreter executable selected by the user or application.
    pub(crate) python_executable: PathBuf,
    /// Pending jobs accepted without blocking the caller.
    pub(crate) queue_capacity: usize,
    /// Queued, running, and terminal records retained until discarded.
    pub(crate) retained_capacity: usize,
    /// Wall-clock limit for each child process.
    pub(crate) deadline: Duration,
}

impl ScriptJobConfig {
    /// Use conservative defaults around one explicit interpreter.
    pub(crate) fn new(python_executable: impl Into<PathBuf>) -> Self {
        Self {
            python_executable: python_executable.into(),
            queue_capacity: 4,
            retained_capacity: 8,
            deadline: Duration::from_secs(10),
        }
    }
}

/// One immutable input and exact script content submitted together.
#[derive(Debug)]
pub(crate) struct ScriptJobRequest {
    /// Validated immutable document capture.
    pub(crate) input: ScriptRecipeInput,
    /// Exact saved or unsaved UTF-8 Python source copied into the fresh working directory.
    pub(crate) script: String,
}

/// Stable identities retained independently of a late result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScriptJobIdentity {
    /// Host-generated recipe job identity.
    pub(crate) job_id: String,
    /// Hash of the immutable recipe input.
    pub(crate) input_hash: String,
    /// SHA-256 of the exact submitted script content.
    pub(crate) script_hash: String,
    /// Exact configured interpreter executable.
    pub(crate) python_executable: PathBuf,
}

/// Opaque identifier for one retained recipe job.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) struct ScriptJobHandle(u64);

impl ScriptJobHandle {
    /// Return the monotonically increasing diagnostic identifier.
    pub(crate) fn get(self) -> u64 {
        self.0
    }
}

/// Current state of one retained recipe job.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScriptJobStatus {
    /// The worker has not started the child process.
    Queued,
    /// The child process is starting or running.
    Running,
    /// A strict validated result is retained.
    Completed,
    /// The job failed without a proposal.
    Failed,
    /// Cancellation prevented a queued job from starting.
    CancelledBeforeStart,
    /// Cancellation killed and reaped the running child.
    CancelledWhileRunning,
}

/// Why a recipe job failed without returning a proposal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScriptJobFailure {
    /// The submitted input, script, or queue configuration is invalid.
    InvalidRequest(String),
    /// The selected interpreter could not be executed.
    PythonUnavailable(String),
    /// Child setup or I/O failed.
    Io(String),
    /// The child exceeded its wall-clock deadline and was killed.
    DeadlineExceeded,
    /// Standard output or standard error exceeded its byte limit.
    OutputLimitExceeded {
        /// Name of the stream that exceeded its limit.
        stream: &'static str,
    },
    /// Python exited unsuccessfully.
    NonZeroExit {
        /// Platform exit code when available.
        code: Option<i32>,
    },
    /// Standard output was not one strict valid result for the capture.
    InvalidResult(String),
    /// Native subprocess execution is unavailable in the browser build.
    BrowserUnavailable,
}

impl fmt::Display for ScriptJobFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(formatter, "invalid script job: {message}"),
            Self::PythonUnavailable(message) => {
                write!(formatter, "selected Python is unavailable: {message}")
            }
            Self::Io(message) => write!(formatter, "script process I/O failed: {message}"),
            Self::DeadlineExceeded => formatter.write_str("script exceeded its deadline"),
            Self::OutputLimitExceeded { stream } => {
                write!(formatter, "script {stream} exceeded its byte limit")
            }
            Self::NonZeroExit { code } => write!(formatter, "Python exited with status {code:?}"),
            Self::InvalidResult(message) => write!(formatter, "invalid script result: {message}"),
            Self::BrowserUnavailable => {
                formatter.write_str("native Python subprocesses are unavailable in the browser")
            }
        }
    }
}

/// Terminal result retained by the queue.
#[derive(Clone, Debug)]
pub(crate) enum ScriptJobOutcome {
    /// One validated immutable proposal plus bounded diagnostic standard error.
    Completed {
        /// Strict typed result.
        result: ScriptRecipeResult,
        /// Human-readable UTF-8-lossy diagnostics, never parsed as a proposal.
        stderr: String,
    },
    /// Failure plus any bounded diagnostics captured before termination.
    Failed {
        /// Structured failure kind.
        failure: ScriptJobFailure,
        /// Human-readable UTF-8-lossy diagnostics.
        stderr: String,
    },
    /// Cancellation with any diagnostics written before the child was reaped.
    Cancelled {
        /// Whether the child process had begun running.
        while_running: bool,
        /// Human-readable UTF-8-lossy diagnostics.
        stderr: String,
    },
}

/// One newly observed terminal recipe result.
#[derive(Clone, Debug)]
pub(crate) struct ScriptJobCompletion {
    /// Queue-local handle.
    pub(crate) handle: ScriptJobHandle,
    /// Immutable submitted identities.
    pub(crate) identity: ScriptJobIdentity,
    /// Terminal outcome.
    pub(crate) outcome: ScriptJobOutcome,
}

/// One coherent retained-job observation.
#[derive(Clone, Debug)]
pub(crate) struct ScriptJobInspection {
    /// Immutable submitted identities.
    pub(crate) identity: ScriptJobIdentity,
    /// Current state.
    pub(crate) status: ScriptJobStatus,
    /// Terminal result when available.
    pub(crate) outcome: Option<ScriptJobOutcome>,
}

/// Why a queue could not be constructed or accept a request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScriptJobSubmitError {
    /// The configuration is outside documented bounds.
    InvalidConfiguration(String),
    /// The request failed bounded validation.
    InvalidRequest(String),
    /// The pending channel is full.
    QueueFull,
    /// The retained record limit is full.
    RetainedJobsFull,
    /// The runner no longer accepts work.
    Stopped,
    /// Native subprocess execution is unavailable on this target.
    BrowserUnavailable,
}

/// Result of requesting cancellation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScriptJobCancelOutcome {
    /// The queued job will never start.
    CancelledBeforeStart,
    /// The running child has been asked to stop and will be killed by the worker.
    CancellationRequested,
    /// The job is already terminal.
    TooLate,
    /// The handle is not retained.
    UnknownHandle,
}

/// Explicit interpreter availability check for status presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScriptRuntimeAvailability {
    /// The configured command ran and identified itself successfully.
    Available(String),
    /// The configured command could not run successfully.
    Unavailable(String),
    /// Browser builds retain recipe types but cannot spawn a native interpreter.
    BrowserUnavailable,
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::BTreeMap;
    use std::fs::{self, File, OpenOptions};
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::path::Path;
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use sha2::{Digest, Sha256};

    use super::*;

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);
    const RUNTIME_CHECK_DEADLINE: Duration = Duration::from_secs(2);
    const MAX_RUNTIME_CHECK_OUTPUT_BYTES: usize = 4 * 1024;

    struct JobRecord {
        identity: ScriptJobIdentity,
        phase: JobPhase,
        cancel: Arc<AtomicBool>,
        reported: bool,
    }

    enum JobPhase {
        Queued(Box<ScriptJobRequest>),
        Running,
        Terminal(ScriptJobOutcome),
    }

    impl JobPhase {
        fn status(&self) -> ScriptJobStatus {
            match self {
                Self::Queued(_) => ScriptJobStatus::Queued,
                Self::Running => ScriptJobStatus::Running,
                Self::Terminal(ScriptJobOutcome::Completed { .. }) => ScriptJobStatus::Completed,
                Self::Terminal(ScriptJobOutcome::Failed { .. }) => ScriptJobStatus::Failed,
                Self::Terminal(ScriptJobOutcome::Cancelled {
                    while_running: false,
                    ..
                }) => ScriptJobStatus::CancelledBeforeStart,
                Self::Terminal(ScriptJobOutcome::Cancelled {
                    while_running: true,
                    ..
                }) => ScriptJobStatus::CancelledWhileRunning,
            }
        }

        fn outcome(&self) -> Option<ScriptJobOutcome> {
            match self {
                Self::Terminal(outcome) => Some(outcome.clone()),
                Self::Queued(_) | Self::Running => None,
            }
        }
    }

    struct QueueState {
        accepting: bool,
        next_handle: u64,
        jobs: BTreeMap<ScriptJobHandle, JobRecord>,
    }

    struct SharedQueue {
        state: Mutex<QueueState>,
    }

    /// One-worker bounded native Python recipe queue.
    pub(crate) struct ScriptJobQueue {
        config: ScriptJobConfig,
        shared: Arc<SharedQueue>,
        sender: Option<SyncSender<ScriptJobHandle>>,
        worker: Option<JoinHandle<()>>,
    }

    impl fmt::Debug for ScriptJobQueue {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("ScriptJobQueue")
                .field("config", &self.config)
                .field(
                    "worker_finished",
                    &self.worker.as_ref().is_none_or(JoinHandle::is_finished),
                )
                .finish_non_exhaustive()
        }
    }

    impl ScriptRuntimeAvailability {
        /// Run the explicitly selected interpreter's version command with a cleared environment.
        pub(crate) fn check(executable: impl Into<PathBuf>) -> Self {
            let executable = executable.into();
            let temporary = match TemporaryDirectory::new() {
                Ok(temporary) => temporary,
                Err(error) => return Self::Unavailable(error.to_string()),
            };
            let stdout_path = temporary.path.join("version.stdout");
            let stderr_path = temporary.path.join("version.stderr");
            let (stdout, mut stdout_reader) = match create_capture(&stdout_path) {
                Ok(capture) => capture,
                Err(error) => return Self::Unavailable(error.to_string()),
            };
            let (stderr, mut stderr_reader) = match create_capture(&stderr_path) {
                Ok(capture) => capture,
                Err(error) => return Self::Unavailable(error.to_string()),
            };
            let mut child = match Command::new(&executable)
                .arg("--version")
                .env_clear()
                .stdin(Stdio::null())
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr))
                .spawn()
            {
                Ok(child) => child,
                Err(error) => return Self::Unavailable(error.to_string()),
            };
            let cancel = AtomicBool::new(false);
            let completion = monitor_child(
                &mut child,
                &cancel,
                RUNTIME_CHECK_DEADLINE,
                &stdout_reader,
                &stderr_reader,
                MAX_RUNTIME_CHECK_OUTPUT_BYTES,
                MAX_RUNTIME_CHECK_OUTPUT_BYTES,
            );
            let (stdout, stdout_exceeded) =
                match read_bounded_capture(&mut stdout_reader, MAX_RUNTIME_CHECK_OUTPUT_BYTES) {
                    Ok(output) => output,
                    Err(error) => return Self::Unavailable(error.to_string()),
                };
            let (stderr, stderr_exceeded) =
                match read_bounded_capture(&mut stderr_reader, MAX_RUNTIME_CHECK_OUTPUT_BYTES) {
                    Ok(output) => output,
                    Err(error) => return Self::Unavailable(error.to_string()),
                };
            match completion {
                ProcessCompletion::Exited(status) if status.success() => {
                    if stdout_exceeded {
                        return Self::Unavailable("version stdout exceeded 4096 bytes".into());
                    }
                    if stderr_exceeded {
                        return Self::Unavailable("version stderr exceeded 4096 bytes".into());
                    }
                    let bytes = if stdout.is_empty() { stderr } else { stdout };
                    Self::Available(
                        String::from_utf8_lossy(&bytes)
                            .trim()
                            .chars()
                            .take(256)
                            .collect(),
                    )
                }
                ProcessCompletion::Exited(status) => {
                    Self::Unavailable(format!("exit status {:?}", status.code()))
                }
                ProcessCompletion::Forced(ForcedStop::Deadline) => {
                    Self::Unavailable("version check exceeded its deadline".into())
                }
                ProcessCompletion::Forced(ForcedStop::StdoutLimit) => {
                    Self::Unavailable("version stdout exceeded 4096 bytes".into())
                }
                ProcessCompletion::Forced(ForcedStop::StderrLimit) => {
                    Self::Unavailable("version stderr exceeded 4096 bytes".into())
                }
                ProcessCompletion::Forced(ForcedStop::Cancelled) => {
                    Self::Unavailable("version check was cancelled".into())
                }
                ProcessCompletion::Io(error) => Self::Unavailable(error),
            }
        }
    }

    impl ScriptJobQueue {
        /// Start one native worker around an explicit interpreter.
        pub(crate) fn new(config: ScriptJobConfig) -> Result<Self, ScriptJobSubmitError> {
            validate_config(&config)?;
            let shared = Arc::new(SharedQueue {
                state: Mutex::new(QueueState {
                    accepting: true,
                    next_handle: 1,
                    jobs: BTreeMap::new(),
                }),
            });
            let (sender, receiver) = mpsc::sync_channel(config.queue_capacity);
            let worker_shared = shared.clone();
            let worker_config = config.clone();
            let worker = std::thread::spawn(move || {
                worker_loop(worker_shared, receiver, worker_config);
            });
            Ok(Self {
                config,
                shared,
                sender: Some(sender),
                worker: Some(worker),
            })
        }

        /// Submit owned input and script content without blocking on Python.
        pub(crate) fn submit(
            &self,
            request: ScriptJobRequest,
        ) -> Result<ScriptJobHandle, ScriptJobSubmitError> {
            request
                .input
                .validate()
                .map_err(|error| ScriptJobSubmitError::InvalidRequest(error.to_string()))?;
            if request.script.is_empty() || request.script.len() > MAX_SCRIPT_BYTES {
                return Err(ScriptJobSubmitError::InvalidRequest(
                    "script must contain 1..=262144 UTF-8 bytes".into(),
                ));
            }
            let input_bytes = serde_json::to_vec(&request.input)
                .map_err(|error| ScriptJobSubmitError::InvalidRequest(error.to_string()))?;
            if input_bytes.len() > MAX_INPUT_BYTES {
                return Err(ScriptJobSubmitError::InvalidRequest(
                    "serialized recipe input exceeds 1048576 bytes".into(),
                ));
            }
            let identity = ScriptJobIdentity {
                job_id: request.input.job_id.clone(),
                input_hash: request.input.input_hash.clone(),
                script_hash: hex_digest(Sha256::digest(request.script.as_bytes())),
                python_executable: self.config.python_executable.clone(),
            };
            let handle = {
                let mut state = lock(&self.shared.state);
                if !state.accepting || self.sender.is_none() {
                    return Err(ScriptJobSubmitError::Stopped);
                }
                if state.jobs.len() >= self.config.retained_capacity {
                    return Err(ScriptJobSubmitError::RetainedJobsFull);
                }
                let handle = ScriptJobHandle(state.next_handle);
                state.next_handle = state.next_handle.checked_add(1).unwrap_or(1);
                state.jobs.insert(
                    handle,
                    JobRecord {
                        identity,
                        phase: JobPhase::Queued(Box::new(request)),
                        cancel: Arc::new(AtomicBool::new(false)),
                        reported: false,
                    },
                );
                handle
            };
            let Some(sender) = &self.sender else {
                lock(&self.shared.state).jobs.remove(&handle);
                return Err(ScriptJobSubmitError::Stopped);
            };
            match sender.try_send(handle) {
                Ok(()) => Ok(handle),
                Err(TrySendError::Full(_)) => {
                    lock(&self.shared.state).jobs.remove(&handle);
                    Err(ScriptJobSubmitError::QueueFull)
                }
                Err(TrySendError::Disconnected(_)) => {
                    lock(&self.shared.state).jobs.remove(&handle);
                    Err(ScriptJobSubmitError::Stopped)
                }
            }
        }

        /// Inspect one retained job without consuming its completion.
        pub(crate) fn inspect(&self, handle: ScriptJobHandle) -> Option<ScriptJobInspection> {
            let state = lock(&self.shared.state);
            let record = state.jobs.get(&handle)?;
            Some(ScriptJobInspection {
                identity: record.identity.clone(),
                status: record.phase.status(),
                outcome: record.phase.outcome(),
            })
        }

        /// Return newly terminal jobs once each without discarding them.
        pub(crate) fn poll_completions(&self) -> Vec<ScriptJobCompletion> {
            let mut state = lock(&self.shared.state);
            state
                .jobs
                .iter_mut()
                .filter_map(|(handle, record)| {
                    let outcome = record.phase.outcome()?;
                    if record.reported {
                        return None;
                    }
                    record.reported = true;
                    Some(ScriptJobCompletion {
                        handle: *handle,
                        identity: record.identity.clone(),
                        outcome,
                    })
                })
                .collect()
        }

        /// Cancel queued work or request that the worker kill and reap a running child.
        pub(crate) fn cancel(&self, handle: ScriptJobHandle) -> ScriptJobCancelOutcome {
            let mut state = lock(&self.shared.state);
            let Some(record) = state.jobs.get_mut(&handle) else {
                return ScriptJobCancelOutcome::UnknownHandle;
            };
            match &record.phase {
                JobPhase::Queued(_) => {
                    record.cancel.store(true, Ordering::Release);
                    record.phase = JobPhase::Terminal(ScriptJobOutcome::Cancelled {
                        while_running: false,
                        stderr: String::new(),
                    });
                    ScriptJobCancelOutcome::CancelledBeforeStart
                }
                JobPhase::Running => {
                    record.cancel.store(true, Ordering::Release);
                    ScriptJobCancelOutcome::CancellationRequested
                }
                JobPhase::Terminal(_) => ScriptJobCancelOutcome::TooLate,
            }
        }

        /// Remove one terminal job and release retained capacity.
        pub(crate) fn discard(&self, handle: ScriptJobHandle) -> bool {
            let mut state = lock(&self.shared.state);
            let Some(record) = state.jobs.get(&handle) else {
                return false;
            };
            if matches!(record.phase, JobPhase::Queued(_) | JobPhase::Running) {
                return false;
            }
            state.jobs.remove(&handle);
            true
        }

        /// Stop submissions, cancel all work, and join the one worker.
        pub(crate) fn shutdown(&mut self) {
            {
                let mut state = lock(&self.shared.state);
                state.accepting = false;
                for record in state.jobs.values_mut() {
                    record.cancel.store(true, Ordering::Release);
                    if matches!(record.phase, JobPhase::Queued(_)) {
                        record.phase = JobPhase::Terminal(ScriptJobOutcome::Cancelled {
                            while_running: false,
                            stderr: String::new(),
                        });
                    }
                }
            }
            self.sender.take();
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    impl Drop for ScriptJobQueue {
        fn drop(&mut self) {
            self.shutdown();
        }
    }

    fn worker_loop(
        shared: Arc<SharedQueue>,
        receiver: Receiver<ScriptJobHandle>,
        config: ScriptJobConfig,
    ) {
        while let Ok(handle) = receiver.recv() {
            let Some((request, cancel)) = take_request(&shared, handle) else {
                continue;
            };
            let outcome = run_process(&config, *request, &cancel);
            let mut state = lock(&shared.state);
            if let Some(record) = state.jobs.get_mut(&handle)
                && matches!(record.phase, JobPhase::Running)
            {
                record.phase = JobPhase::Terminal(outcome);
            }
        }
    }

    fn take_request(
        shared: &SharedQueue,
        handle: ScriptJobHandle,
    ) -> Option<(Box<ScriptJobRequest>, Arc<AtomicBool>)> {
        let mut state = lock(&shared.state);
        let record = state.jobs.get_mut(&handle)?;
        let phase = std::mem::replace(&mut record.phase, JobPhase::Running);
        match phase {
            JobPhase::Queued(request) if !record.cancel.load(Ordering::Acquire) => {
                Some((request, record.cancel.clone()))
            }
            JobPhase::Queued(_) => {
                record.phase = JobPhase::Terminal(ScriptJobOutcome::Cancelled {
                    while_running: false,
                    stderr: String::new(),
                });
                None
            }
            other => {
                record.phase = other;
                None
            }
        }
    }

    fn run_process(
        config: &ScriptJobConfig,
        request: ScriptJobRequest,
        cancel: &AtomicBool,
    ) -> ScriptJobOutcome {
        let input = match serde_json::to_vec(&request.input) {
            Ok(input) => input,
            Err(error) => return failed(ScriptJobFailure::InvalidRequest(error.to_string()), ""),
        };
        let temporary = match TemporaryDirectory::new() {
            Ok(temporary) => temporary,
            Err(error) => return failed(ScriptJobFailure::Io(error.to_string()), ""),
        };
        let script_path = temporary.path.join("recipe.py");
        if let Err(error) = write_script(&script_path, request.script.as_bytes()) {
            return failed(ScriptJobFailure::Io(error.to_string()), "");
        }
        let input_path = temporary.path.join("input.json");
        if let Err(error) = write_file(&input_path, &input) {
            return failed(ScriptJobFailure::Io(error.to_string()), "");
        }
        let stdout_path = temporary.path.join("stdout");
        let stderr_path = temporary.path.join("stderr");
        let stdin = match File::open(&input_path) {
            Ok(stdin) => stdin,
            Err(error) => return failed(ScriptJobFailure::Io(error.to_string()), ""),
        };
        let (stdout, mut stdout_reader) = match create_capture(&stdout_path) {
            Ok(capture) => capture,
            Err(error) => return failed(ScriptJobFailure::Io(error.to_string()), ""),
        };
        let (stderr, mut stderr_reader) = match create_capture(&stderr_path) {
            Ok(capture) => capture,
            Err(error) => return failed(ScriptJobFailure::Io(error.to_string()), ""),
        };

        let mut child = match Command::new(&config.python_executable)
            .args(["-I", "recipe.py"])
            .current_dir(&temporary.path)
            .env_clear()
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONUNBUFFERED", "1")
            .stdin(Stdio::from(stdin))
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                return failed(ScriptJobFailure::PythonUnavailable(error.to_string()), "");
            }
        };

        let completion = monitor_child(
            &mut child,
            cancel,
            config.deadline,
            &stdout_reader,
            &stderr_reader,
            MAX_STDOUT_BYTES,
            MAX_STDERR_BYTES,
        );
        let (stdout, stdout_exceeded) =
            match read_bounded_capture(&mut stdout_reader, MAX_STDOUT_BYTES) {
                Ok(output) => output,
                Err(error) => return failed(ScriptJobFailure::Io(error.to_string()), ""),
            };
        let (stderr_bytes, stderr_exceeded) =
            match read_bounded_capture(&mut stderr_reader, MAX_STDERR_BYTES) {
                Ok(output) => output,
                Err(error) => return failed(ScriptJobFailure::Io(error.to_string()), ""),
            };
        let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
        let status = match completion {
            ProcessCompletion::Exited(status) => {
                if stdout_exceeded {
                    return failed(
                        ScriptJobFailure::OutputLimitExceeded { stream: "stdout" },
                        stderr,
                    );
                }
                if stderr_exceeded {
                    return failed(
                        ScriptJobFailure::OutputLimitExceeded { stream: "stderr" },
                        stderr,
                    );
                }
                status
            }
            ProcessCompletion::Forced(forced) => {
                return match forced {
                    ForcedStop::Cancelled => ScriptJobOutcome::Cancelled {
                        while_running: true,
                        stderr,
                    },
                    ForcedStop::StdoutLimit => failed(
                        ScriptJobFailure::OutputLimitExceeded { stream: "stdout" },
                        stderr,
                    ),
                    ForcedStop::StderrLimit => failed(
                        ScriptJobFailure::OutputLimitExceeded { stream: "stderr" },
                        stderr,
                    ),
                    ForcedStop::Deadline => failed(ScriptJobFailure::DeadlineExceeded, stderr),
                };
            }
            ProcessCompletion::Io(error) => {
                return failed(ScriptJobFailure::Io(error), stderr);
            }
        };
        if !status.success() {
            return failed(
                ScriptJobFailure::NonZeroExit {
                    code: status.code(),
                },
                stderr,
            );
        }
        let result = match serde_json::from_slice::<ScriptRecipeResult>(&stdout) {
            Ok(result) => result,
            Err(error) => {
                return failed(ScriptJobFailure::InvalidResult(error.to_string()), stderr);
            }
        };
        if let Err(error) = result.validate_against(&request.input) {
            return failed(ScriptJobFailure::InvalidResult(error.to_string()), stderr);
        }
        ScriptJobOutcome::Completed { result, stderr }
    }

    #[derive(Debug)]
    enum ForcedStop {
        Cancelled,
        StdoutLimit,
        StderrLimit,
        Deadline,
    }

    enum ProcessCompletion {
        Exited(ExitStatus),
        Forced(ForcedStop),
        Io(String),
    }

    fn monitor_child(
        child: &mut Child,
        cancel: &AtomicBool,
        deadline: Duration,
        stdout: &File,
        stderr: &File,
        stdout_limit: usize,
        stderr_limit: usize,
    ) -> ProcessCompletion {
        let started = Instant::now();
        loop {
            let forced = if cancel.load(Ordering::Acquire) {
                Some(ForcedStop::Cancelled)
            } else if capture_exceeds(stdout, stdout_limit) {
                Some(ForcedStop::StdoutLimit)
            } else if capture_exceeds(stderr, stderr_limit) {
                Some(ForcedStop::StderrLimit)
            } else if started.elapsed() >= deadline {
                Some(ForcedStop::Deadline)
            } else {
                None
            };
            if let Some(forced) = forced {
                return match kill_and_wait(child) {
                    Ok(_) => ProcessCompletion::Forced(forced),
                    Err(error) => ProcessCompletion::Io(format!(
                        "could not stop script process after {forced:?}: {error}"
                    )),
                };
            }
            match child.try_wait() {
                Ok(Some(status)) => return ProcessCompletion::Exited(status),
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    let _ = kill_and_wait(child);
                    return ProcessCompletion::Io(error.to_string());
                }
            }
        }
    }

    fn capture_exceeds(file: &File, limit: usize) -> bool {
        file.metadata()
            .is_ok_and(|metadata| metadata.len() > limit as u64)
    }

    fn read_bounded_capture(file: &mut File, limit: usize) -> std::io::Result<(Vec<u8>, bool)> {
        file.seek(SeekFrom::Start(0))?;
        let mut output = Vec::new();
        Read::by_ref(file)
            .take(limit.saturating_add(1) as u64)
            .read_to_end(&mut output)?;
        let exceeded = output.len() > limit || capture_exceeds(file, limit);
        output.truncate(limit);
        Ok((output, exceeded))
    }

    fn kill_and_wait(child: &mut Child) -> std::io::Result<ExitStatus> {
        match child.kill() {
            Ok(()) => child.wait(),
            Err(kill_error) => match child.try_wait()? {
                Some(status) => Ok(status),
                None => Err(kill_error),
            },
        }
    }

    fn failed(failure: ScriptJobFailure, stderr: impl Into<String>) -> ScriptJobOutcome {
        ScriptJobOutcome::Failed {
            failure,
            stderr: stderr.into(),
        }
    }

    fn validate_config(config: &ScriptJobConfig) -> Result<(), ScriptJobSubmitError> {
        if config.python_executable.as_os_str().is_empty() {
            return Err(ScriptJobSubmitError::InvalidConfiguration(
                "python_executable must not be empty".into(),
            ));
        }
        if config.queue_capacity == 0 || config.queue_capacity > MAX_SCRIPT_QUEUE_CAPACITY {
            return Err(ScriptJobSubmitError::InvalidConfiguration(
                "queue_capacity must be 1..=8".into(),
            ));
        }
        if config.retained_capacity == 0 || config.retained_capacity > MAX_RETAINED_SCRIPT_JOBS {
            return Err(ScriptJobSubmitError::InvalidConfiguration(
                "retained_capacity must be 1..=16".into(),
            ));
        }
        if config.deadline.is_zero() || config.deadline > MAX_DEADLINE {
            return Err(ScriptJobSubmitError::InvalidConfiguration(
                "deadline must be greater than zero and no longer than 300 seconds".into(),
            ));
        }
        Ok(())
    }

    struct TemporaryDirectory {
        path: PathBuf,
    }

    impl TemporaryDirectory {
        fn new() -> std::io::Result<Self> {
            for _ in 0..32 {
                let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
                let path = std::env::temp_dir().join(format!(
                    "runebender-script-job-{}-{sequence}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Self { path }),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error),
                }
            }
            Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "could not allocate a fresh script working directory",
            ))
        }
    }

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_script(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        write_file(path, bytes)
    }

    fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }

    fn create_capture(path: &Path) -> std::io::Result<(File, File)> {
        let writer = OpenOptions::new().write(true).create_new(true).open(path)?;
        let reader = OpenOptions::new().read(true).open(path)?;
        Ok((writer, reader))
    }

    fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        mutex.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn hex_digest(digest: impl AsRef<[u8]>) -> String {
        digest
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use std::collections::BTreeMap;

        use runebender::document::agent_edit::AgentLayerGuard;
        use runebender::document::script_recipe::{
            SCRIPT_RECIPE_SCHEMA_VERSION, ScriptRecipeLayer,
        };

        use super::*;

        fn python() -> Option<PathBuf> {
            let executable = std::env::var_os("PYTHON")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("python3"));
            matches!(
                ScriptRuntimeAvailability::check(&executable),
                ScriptRuntimeAvailability::Available(_)
            )
            .then_some(executable)
        }

        fn recipe_examples() -> Option<PathBuf> {
            let directory = std::env::var_os("RUNEBENDER_RECIPE_EXAMPLES")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/recipes")
                });
            directory
                .join("anchor_recipes.py")
                .is_file()
                .then_some(directory)
        }

        fn input() -> ScriptRecipeInput {
            ScriptRecipeInput::new(
                "job-test".into(),
                0,
                BTreeMap::new(),
                vec![ScriptRecipeLayer {
                    guard: AgentLayerGuard {
                        glyph: "A".into(),
                        glyph_id: "glyph-a".into(),
                        layer: "public.default".into(),
                        expected_revision: "revision-a".into(),
                    },
                    width: 600.0,
                    anchors: Vec::new(),
                }],
            )
            .expect("valid test input")
        }

        fn queue(python: PathBuf, deadline: Duration) -> ScriptJobQueue {
            let mut config = ScriptJobConfig::new(python);
            config.deadline = deadline;
            ScriptJobQueue::new(config).expect("queue")
        }

        fn wait(queue: &ScriptJobQueue, handle: ScriptJobHandle) -> ScriptJobOutcome {
            let started = Instant::now();
            loop {
                if let Some(outcome) = queue.inspect(handle).and_then(|job| job.outcome) {
                    return outcome;
                }
                assert!(started.elapsed() < Duration::from_secs(5), "job stalled");
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn echo_script(suffix: &str) -> String {
            format!(
                "import json, sys\ndata = json.load(sys.stdin)\njson.dump({{'schema_version': {}, 'job_id': data['job_id'], 'input_hash': data['input_hash'], 'report': 'ok', 'reads': [], 'edits': []}}, sys.stdout)\n{suffix}\n",
                SCRIPT_RECIPE_SCHEMA_VERSION
            )
        }

        #[test]
        fn runs_one_strict_result_and_retains_identity() {
            let Some(python) = python() else {
                return;
            };
            let queue = queue(python, Duration::from_secs(2));
            let input = input();
            let input_hash = input.input_hash.clone();
            let handle = queue
                .submit(ScriptJobRequest {
                    input,
                    script: echo_script("print('diagnostic', file=sys.stderr)"),
                })
                .expect("submit");
            let inspection = queue.inspect(handle).expect("retained job");
            assert_eq!(inspection.identity.input_hash, input_hash);
            match wait(&queue, handle) {
                ScriptJobOutcome::Completed { result, stderr } => {
                    assert_eq!(result.report, "ok");
                    assert!(stderr.contains("diagnostic"));
                }
                other => panic!("unexpected outcome: {other:?}"),
            }
        }

        #[test]
        fn corrected_anchor_example_deserializes_and_validates_against_real_hash() {
            let (Some(python), Some(examples)) = (python(), recipe_examples()) else {
                return;
            };
            let input: ScriptRecipeInput = serde_json::from_str(
                &fs::read_to_string(examples.join("fixture-move-input.json"))
                    .expect("read example fixture"),
            )
            .expect("deserialize example fixture through the Rust contract");
            input
                .validate()
                .expect("example fixture has the Rust input hash");
            let script = fs::read_to_string(examples.join("anchor_recipes.py"))
                .expect("read actual anchor example");
            let queue = queue(python, Duration::from_secs(2));
            let handle = queue
                .submit(ScriptJobRequest { input, script })
                .expect("submit actual anchor example");
            match wait(&queue, handle) {
                ScriptJobOutcome::Completed { result, .. } => {
                    assert!(result.report.starts_with("Proposed 3 anchor move(s)"));
                    assert_eq!(result.edits.len(), 2);
                }
                other => panic!("unexpected example outcome: {other:?}"),
            }
        }

        #[test]
        fn reports_exception_extra_frames_and_output_limit() {
            let Some(python) = python() else {
                return;
            };
            let queue = queue(python, Duration::from_secs(2));
            let exception = queue
                .submit(ScriptJobRequest {
                    input: input(),
                    script: "raise RuntimeError('synthetic failure')\n".into(),
                })
                .expect("submit exception");
            assert!(matches!(
                wait(&queue, exception),
                ScriptJobOutcome::Failed {
                    failure: ScriptJobFailure::NonZeroExit { .. },
                    ..
                }
            ));
            assert!(queue.discard(exception));

            let extra = queue
                .submit(ScriptJobRequest {
                    input: input(),
                    script: echo_script("print('{}')"),
                })
                .expect("submit extra frame");
            assert!(matches!(
                wait(&queue, extra),
                ScriptJobOutcome::Failed {
                    failure: ScriptJobFailure::InvalidResult(_),
                    ..
                }
            ));
            assert!(queue.discard(extra));

            let oversized = queue
                .submit(ScriptJobRequest {
                    input: input(),
                    script: "import sys\nsys.stdout.write('x' * 1100000)\n".into(),
                })
                .expect("submit oversized output");
            assert!(matches!(
                wait(&queue, oversized),
                ScriptJobOutcome::Failed {
                    failure: ScriptJobFailure::OutputLimitExceeded { stream: "stdout" },
                    ..
                }
            ));
        }

        #[test]
        fn kills_hanging_and_cancelled_children() {
            let Some(python) = python() else {
                return;
            };
            let timeout_queue = queue(python.clone(), Duration::from_millis(100));
            let timeout = timeout_queue
                .submit(ScriptJobRequest {
                    input: input(),
                    script: "while True:\n    pass\n".into(),
                })
                .expect("submit timeout");
            assert!(matches!(
                wait(&timeout_queue, timeout),
                ScriptJobOutcome::Failed {
                    failure: ScriptJobFailure::DeadlineExceeded,
                    ..
                }
            ));

            let cancel_queue = queue(python, Duration::from_secs(2));
            let cancelled = cancel_queue
                .submit(ScriptJobRequest {
                    input: input(),
                    script: "while True:\n    pass\n".into(),
                })
                .expect("submit cancellation");
            let started = Instant::now();
            while cancel_queue.inspect(cancelled).expect("job").status == ScriptJobStatus::Queued {
                assert!(
                    started.elapsed() < Duration::from_secs(1),
                    "job never started"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            assert_eq!(
                cancel_queue.cancel(cancelled),
                ScriptJobCancelOutcome::CancellationRequested
            );
            assert!(matches!(
                wait(&cancel_queue, cancelled),
                ScriptJobOutcome::Cancelled {
                    while_running: true,
                    ..
                }
            ));
        }

        #[test]
        fn deadline_does_not_wait_for_descendant_standard_handles() {
            let Some(python) = python() else {
                return;
            };
            let queue = queue(python, Duration::from_millis(100));
            let started = Instant::now();
            let handle = queue
                .submit(ScriptJobRequest {
                    input: input(),
                    script: "import subprocess, sys\nsubprocess.Popen([sys.executable, '-c', 'import time; time.sleep(2)'])\nwhile True:\n    pass\n".into(),
                })
                .expect("submit child with descendant");
            assert!(matches!(
                wait(&queue, handle),
                ScriptJobOutcome::Failed {
                    failure: ScriptJobFailure::DeadlineExceeded,
                    ..
                }
            ));
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "deadline waited for a surviving descendant's standard handles"
            );
        }

        #[cfg(unix)]
        #[test]
        fn runtime_availability_rejects_oversized_version_output() {
            use std::os::unix::fs::PermissionsExt;

            let temporary = TemporaryDirectory::new().expect("temporary directory");
            let executable = temporary.path.join("oversized-version");
            write_script(
                &executable,
                b"#!/bin/sh\n/bin/dd if=/dev/zero bs=5000 count=1 2>/dev/null\n",
            )
            .expect("write executable");
            let mut permissions = fs::metadata(&executable).expect("metadata").permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&executable, permissions).expect("make executable");

            assert!(matches!(
                ScriptRuntimeAvailability::check(executable),
                ScriptRuntimeAvailability::Unavailable(message)
                    if message.contains("exceeded 4096 bytes")
            ));
        }

        #[cfg(unix)]
        #[test]
        fn result_capture_uses_the_preopened_file_identity() {
            let Some(python) = python() else {
                return;
            };
            let queue = queue(python, Duration::from_secs(2));
            let handle = queue
                .submit(ScriptJobRequest {
                    input: input(),
                    script: format!(
                        "import json, os, sys\ndata = json.load(sys.stdin)\nos.unlink('stdout')\nwith open('stdout', 'w') as replacement:\n    replacement.write('not the result')\njson.dump({{'schema_version': {}, 'job_id': data['job_id'], 'input_hash': data['input_hash'], 'report': 'original capture', 'reads': [], 'edits': []}}, sys.stdout)\n",
                        SCRIPT_RECIPE_SCHEMA_VERSION
                    ),
                })
                .expect("submit capture replacement");
            match wait(&queue, handle) {
                ScriptJobOutcome::Completed { result, .. } => {
                    assert_eq!(result.report, "original capture");
                }
                other => panic!("unexpected outcome: {other:?}"),
            }
        }

        #[test]
        fn unavailable_interpreter_is_explicit() {
            let queue = queue(
                PathBuf::from("runebender-python-that-does-not-exist"),
                Duration::from_secs(1),
            );
            let handle = queue
                .submit(ScriptJobRequest {
                    input: input(),
                    script: echo_script(""),
                })
                .expect("submit unavailable runtime");
            assert!(matches!(
                wait(&queue, handle),
                ScriptJobOutcome::Failed {
                    failure: ScriptJobFailure::PythonUnavailable(_),
                    ..
                }
            ));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use native::ScriptJobQueue;

#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;

    /// Browser placeholder retaining the shared job API shape without subprocess support.
    #[derive(Debug)]
    pub(crate) struct ScriptJobQueue;

    impl ScriptRuntimeAvailability {
        /// Report the browser platform limit without probing a runtime.
        pub(crate) fn check(_executable: impl Into<PathBuf>) -> Self {
            Self::BrowserUnavailable
        }
    }

    impl ScriptJobQueue {
        /// Reject construction because browsers cannot spawn native Python.
        pub(crate) fn new(_config: ScriptJobConfig) -> Result<Self, ScriptJobSubmitError> {
            Err(ScriptJobSubmitError::BrowserUnavailable)
        }

        /// Reject submission because browsers cannot spawn native Python.
        pub(crate) fn submit(
            &self,
            _request: ScriptJobRequest,
        ) -> Result<ScriptJobHandle, ScriptJobSubmitError> {
            Err(ScriptJobSubmitError::BrowserUnavailable)
        }

        /// Browser queues retain no job records.
        pub(crate) fn inspect(&self, _handle: ScriptJobHandle) -> Option<ScriptJobInspection> {
            None
        }

        /// Browser queues never produce native completions.
        pub(crate) fn poll_completions(&self) -> Vec<ScriptJobCompletion> {
            Vec::new()
        }

        /// Browser queues cannot retain work to cancel.
        pub(crate) fn cancel(&self, _handle: ScriptJobHandle) -> ScriptJobCancelOutcome {
            ScriptJobCancelOutcome::UnknownHandle
        }

        /// Browser queues cannot retain work to discard.
        pub(crate) fn discard(&self, _handle: ScriptJobHandle) -> bool {
            false
        }

        /// Browser queues have no worker to stop.
        pub(crate) fn shutdown(&mut self) {}
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) use browser::ScriptJobQueue;

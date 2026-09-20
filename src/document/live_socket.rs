// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Private Unix socket mailbox. Only the editor thread executes queued font operations.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::{
    fs::DirBuilderExt,
    net::{UnixListener, UnixStream},
};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    mpsc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::{
    agent::ToolCall,
    agent_cancellation::{
        AgentCancellationAdmission, AgentCancellationError, AgentCancellationIdentity,
        AgentCancellationOutcome, AgentCancellationRegistry,
    },
    agent_edit::AgentEditRequest,
};
use serde::Deserialize;
use serde_json::Value;

/// Maximum serialized request or response frame, excluding the trailing newline.
pub const MAX_FRAME_BYTES: u64 = 8 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);
const STOP_POLL: Duration = Duration::from_millis(25);
/// Maximum number of calls waiting for serialized application dispatch.
pub const MAX_PENDING_REQUESTS: usize = 16;
/// Maximum number of simultaneous socket connection handlers.
pub const MAX_LIVE_CONNECTIONS: usize = 32;
/// Maximum number of exact cancellation identities retained for one endpoint lifetime.
pub const CANCELLATION_CAPACITY: usize = 2048;
static NEXT: AtomicU64 = AtomicU64::new(0);

/// Lists endpoint paths in this user's temporary directory, without contacting editors.
/// A crashed editor may leave a stale entry; a call to it fails rather than using disk.
pub fn sessions() -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(std::env::temp_dir())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("runebender-live-")
        })
        .map(|entry| entry.path().join("session.sock"))
        .filter(|path| path.exists())
        .collect();
    paths.sort();
    paths
}

/// A pending operation, with a deadline checked before the editor invokes it.
#[derive(Debug)]
pub struct Pending {
    call: ToolCall,
    epoch: String,
    deadline: Instant,
    reply: mpsc::Sender<Value>,
}

impl Pending {
    /// Executes a still-current request on the caller's thread and sends its result.
    /// Expired requests are dropped without invoking `handle`.
    pub fn respond(mut self, handle: impl FnOnce(&ToolCall) -> Value) {
        if Instant::now() < self.deadline {
            let expected = self.call.arguments.get("expected_document_epoch").cloned();
            // Strict application edit requests retain their required epoch in the typed payload.
            if !matches!(
                self.call.name.as_str(),
                "agent_apply"
                    | "agent_receipt"
                    | "agent_history"
                    | "proof_start"
                    | "proof_status"
                    | "proof_cancel"
                    | "proof_release"
                    | "nodes_discover"
                    | "nodes_snapshot"
                    | "nodes_mutate"
                    | "nodes_run"
                    | "nodes_status"
                    | "nodes_cancel"
                    | "nodes_release"
                    | "nodes_apply"
                    | "nodes_image"
            ) && let Some(args) = self.call.arguments.as_object_mut()
            {
                args.remove("expected_document_epoch");
            }
            let mut result = if expected
                .as_ref()
                .is_some_and(|value| value.as_str() != Some(&self.epoch))
            {
                serde_json::json!({"ok":false,"error":"document epoch mismatch; reconnect and read the intended document", "error_code":"stale_document"})
            } else {
                handle(&self.call)
            };
            result["document_epoch"] = serde_json::json!(self.epoch);
            result["live_schema_version"] = serde_json::json!(1);
            result["server_version"] = serde_json::json!(env!("CARGO_PKG_VERSION"));
            let _ = self.reply.send(result);
        }
    }
}

/// An editor's socket and incoming queue. Dropping it stops accepting connections.
#[derive(Debug)]
pub struct Server {
    path: PathBuf,
    epoch: String,
    receiver: mpsc::Receiver<Pending>,
    cancellations: AgentCancellationRegistry,
    stop: Arc<AtomicBool>,
    active_connections: Arc<AtomicUsize>,
    listener_worker: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    /// Creates a private directory and socket in the system temporary directory.
    /// No source data is written there. The path identifies this document lifetime.
    pub fn start() -> io::Result<Self> {
        let epoch = format!(
            "{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        use sha2::{Digest as _, Sha256};
        // macOS gives Unix sockets a short path budget, including the user temp root.
        // Exclusive directory creation rejects even an unlikely digest collision.
        let suffix = format!("{:x}", Sha256::digest(epoch.as_bytes()));
        let directory = std::env::temp_dir().join(format!("runebender-live-{}", &suffix[..16]));
        std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let path = directory.join("session.sock");
        let listener = match UnixListener::bind(&path) {
            Ok(listener) => listener,
            Err(error) => {
                let _ = std::fs::remove_dir(&directory);
                return Err(error);
            }
        };
        listener.set_nonblocking(true)?;
        let (sender, receiver) = mpsc::sync_channel(MAX_PENDING_REQUESTS);
        let cancellations = AgentCancellationRegistry::new(&epoch, CANCELLATION_CAPACITY)
            .map_err(io::Error::other)?;
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker_epoch = epoch.clone();
        let worker_cancellations = cancellations.clone();
        let active_connections = Arc::new(AtomicUsize::new(0));
        let worker_connections = active_connections.clone();
        let listener_worker = std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                        if worker_connections
                            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |active| {
                                (active < MAX_LIVE_CONNECTIONS).then_some(active + 1)
                            })
                            .is_err()
                        {
                            let _ = write_frame(
                                &mut stream,
                                &serde_json::json!({"ok":false,"error_code":"connection_capacity",
                                    "error":"live connection capacity exhausted"}),
                            );
                            continue;
                        }
                        let sender = sender.clone();
                        let epoch = worker_epoch.clone();
                        let cancellations = worker_cancellations.clone();
                        let stop = stopping.clone();
                        let active = worker_connections.clone();
                        std::thread::spawn(move || {
                            // `serve` writes at most one response frame.
                            // A write error may occur after partial bytes, so never append a second
                            // JSON error frame here.
                            let _ = serve(&mut stream, &sender, &epoch, &cancellations, &stop);
                            active.fetch_sub(1, Ordering::AcqRel);
                        });
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            path,
            epoch,
            receiver,
            cancellations,
            stop,
            active_connections,
            listener_worker: Some(listener_worker),
        })
    }

    /// The explicit endpoint clients pass to `--session`.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Opaque document lifetime used by request guards and response envelopes.
    /// Application receipts and proof jobs must retain this exact identity.
    pub fn document_epoch(&self) -> &str {
        &self.epoch
    }

    /// Shared cancellation state used by the application's pre-commit hook.
    pub fn cancellations(&self) -> AgentCancellationRegistry {
        self.cancellations.clone()
    }

    /// Takes the next request without blocking the UI thread.
    pub fn try_recv(&self) -> Option<Pending> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_file(&self.path);
        if let Some(worker) = self.listener_worker.take() {
            let _ = worker.join();
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.active_connections.load(Ordering::Acquire) != 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

fn read_frame(stream: &mut UnixStream, limit: u64) -> io::Result<String> {
    let mut line = String::new();
    BufReader::new(stream.take(limit + 1)).read_line(&mut line)?;
    if line.len() as u64 > limit || !line.ends_with('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid or oversized frame",
        ));
    }
    Ok(line)
}

fn serve(
    stream: &mut UnixStream,
    sender: &mpsc::SyncSender<Pending>,
    epoch: &str,
    cancellations: &AgentCancellationRegistry,
    stop: &AtomicBool,
) -> io::Result<()> {
    let call: ToolCall = serde_json::from_str(&read_frame(stream, MAX_FRAME_BYTES)?)?;
    if matches!(call.name.as_str(), "agent_reserve" | "agent_release") {
        let reserve = call.name == "agent_reserve";
        let request: AgentEditRequest = match serde_json::from_value(call.arguments) {
            Ok(request) => request,
            Err(error) => {
                return write_envelope(
                    stream,
                    serde_json::json!({"ok":false,"error_code":"invalid_arguments",
                        "error":error.to_string()}),
                    epoch,
                );
            }
        };
        let payload_digest = request.payload_digest();
        let identity = match AgentCancellationIdentity::new(
            request.expected_document_epoch,
            request.actor,
            request.operation_key,
        ) {
            Ok(identity) => identity,
            Err(error) => {
                return write_envelope(
                    stream,
                    serde_json::json!({"ok":false,"error_code":"invalid_arguments",
                        "error":error.to_string()}),
                    epoch,
                );
            }
        };
        let result = if identity.document_epoch() != epoch {
            serde_json::json!({"ok":false,"error_code":"stale_document",
                "error":"document epoch mismatch; reconnect explicitly"})
        } else if reserve {
            match cancellations.admit(&identity, payload_digest) {
                Ok(AgentCancellationAdmission::New) => {
                    serde_json::json!({"ok":true,"reservation_status":"new"})
                }
                Ok(AgentCancellationAdmission::Existing) => {
                    serde_json::json!({"ok":true,"reservation_status":"existing"})
                }
                Err(error) => cancellation_admission_error(error),
            }
        } else {
            match cancellations.release_unqueued(&identity, payload_digest) {
                Ok(()) => serde_json::json!({"ok":true,"reservation_status":"released"}),
                Err(error) => cancellation_admission_error(error),
            }
        };
        return write_envelope(stream, result, epoch);
    }
    if call.name == "agent_cancel" {
        let request: CancellationRequest = match serde_json::from_value(call.arguments) {
            Ok(request) => request,
            Err(error) => {
                return write_envelope(
                    stream,
                    serde_json::json!({"ok":false,"error_code":"invalid_arguments",
                        "error":error.to_string()}),
                    epoch,
                );
            }
        };
        let identity = match AgentCancellationIdentity::new(
            request.expected_document_epoch,
            request.actor,
            request.operation_key,
        ) {
            Ok(identity) => identity,
            Err(error) => {
                return write_envelope(
                    stream,
                    serde_json::json!({"ok":false,"error_code":"invalid_arguments",
                        "error":error.to_string()}),
                    epoch,
                );
            }
        };
        let result = if identity.document_epoch() != epoch {
            serde_json::json!({"ok":false,"error_code":"stale_document",
                "error":"document epoch mismatch; reconnect explicitly"})
        } else {
            cancellation_result(cancellations.cancel(&identity).map_err(io::Error::other)?)
        };
        return write_envelope(stream, result, epoch);
    }

    let cancellation = if call.name == "agent_apply"
        && let Ok(request) = serde_json::from_value::<AgentEditRequest>(call.arguments.clone())
    {
        let payload_digest = request.payload_digest();
        let identity = AgentCancellationIdentity::new(
            request.expected_document_epoch,
            request.actor,
            request.operation_key,
        )
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        if identity.document_epoch() == epoch {
            let admission = match cancellations.admit(&identity, payload_digest) {
                Ok(admission) => admission,
                Err(error) => {
                    return write_envelope(stream, cancellation_admission_error(error), epoch);
                }
            };
            Some((identity, payload_digest, admission))
        } else {
            // Preserve the application dispatch path for its structured stale-document response.
            None
        }
    } else {
        None
    };
    let (reply, receive) = mpsc::channel();
    if let Err(error) = sender.try_send(Pending {
        call,
        epoch: epoch.to_owned(),
        deadline: Instant::now() + TIMEOUT,
        reply,
    }) {
        if let Some((identity, payload_digest, AgentCancellationAdmission::New)) = &cancellation {
            let _ = cancellations.release_unqueued(identity, *payload_digest);
        }
        return Err(io::Error::other(error.to_string()));
    }
    let deadline = Instant::now() + TIMEOUT;
    let result = loop {
        match receive.recv_timeout(STOP_POLL) {
            Ok(result) => break result,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "editor stopped before responding",
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) if stop.load(Ordering::Relaxed) => {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "editor endpoint is shutting down",
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() >= deadline => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "editor did not respond; inspect receipts before retrying",
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    };
    write_frame(stream, &result)
}

fn cancellation_admission_error(error: AgentCancellationError) -> Value {
    match error {
        AgentCancellationError::CapacityExhausted { .. } => {
            serde_json::json!({"ok":false,"error_code":"cancellation_capacity",
                "error":"cancellation registry capacity exhausted; existing operation state remains available"})
        }
        AgentCancellationError::PayloadMismatch => {
            serde_json::json!({"ok":false,"error_code":"payload_mismatch",
                "error":error.to_string()})
        }
        AgentCancellationError::StaleDocument => {
            serde_json::json!({"ok":false,"error_code":"stale_document",
                "error":error.to_string()})
        }
        AgentCancellationError::InvalidCapacity { .. } => {
            serde_json::json!({"ok":false,"error_code":"invalid_arguments",
                "error":error.to_string()})
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancellationRequest {
    expected_document_epoch: String,
    actor: String,
    operation_key: String,
}

fn cancellation_result(outcome: AgentCancellationOutcome) -> Value {
    let (ok, status) = match outcome {
        AgentCancellationOutcome::Prevented => (true, "prevented"),
        AgentCancellationOutcome::AlreadyPrevented => (true, "already_prevented"),
        AgentCancellationOutcome::TooLate => (false, "too_late"),
        AgentCancellationOutcome::Committed => (false, "committed"),
        AgentCancellationOutcome::Completed => (false, "completed"),
        AgentCancellationOutcome::Unknown => (false, "unknown"),
    };
    serde_json::json!({"ok":ok,"cancellation_status":status,"saved":false})
}

fn write_envelope(stream: &mut UnixStream, mut result: Value, epoch: &str) -> io::Result<()> {
    result["document_epoch"] = serde_json::json!(epoch);
    result["live_schema_version"] = serde_json::json!(1);
    result["server_version"] = serde_json::json!(env!("CARGO_PKG_VERSION"));
    write_frame(stream, &result)
}

fn write_frame(stream: &mut UnixStream, value: &Value) -> io::Result<()> {
    let mut frame = serde_json::to_vec(value)?;
    if frame.len() >= OUTPUT_LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "response meets or exceeds the 8 MiB live frame limit",
        ));
    }
    frame.push(b'\n');
    stream.write_all(&frame)
}

/// Sends one bounded call to an explicit editor endpoint. Never falls back to disk.
pub fn call(path: &Path, call: &ToolCall) -> io::Result<Value> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(TIMEOUT + Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = serde_json::to_string(call)?;
    if request.len() as u64 >= MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "request too large",
        ));
    }
    stream.write_all(request.as_bytes())?;
    stream.write_all(b"\n")?;
    Ok(serde_json::from_str(&read_frame(
        &mut stream,
        OUTPUT_LIMIT as u64,
    )?)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_requires_editor_dispatch_and_drop_removes_endpoint() {
        use std::os::unix::fs::PermissionsExt;
        let server = Server::start().unwrap();
        let path = server.path().to_path_buf();
        assert_eq!(
            std::fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let client_path = path.clone();
        let client = std::thread::spawn(move || {
            call(
                &client_path,
                &ToolCall {
                    name: "read_glyph".into(),
                    arguments: serde_json::json!({"glyph": "n"}),
                },
            )
            .unwrap()
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(pending) = server.try_recv() {
                pending.respond(
                    |request| serde_json::json!({"ok": true, "glyph": request.arguments["glyph"]}),
                );
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let result = client.join().unwrap();
        assert_eq!(result["glyph"], "n");
        assert_eq!(result["document_epoch"], server.document_epoch());
        drop(server);
        assert!(!path.exists());
    }

    #[test]
    fn large_response_is_one_complete_bounded_frame() {
        let server = Server::start().unwrap();
        let path = server.path().to_path_buf();
        let client = std::thread::spawn(move || {
            call(
                &path,
                &ToolCall {
                    name: "proof_status".into(),
                    arguments: serde_json::json!({}),
                },
            )
            .unwrap()
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(pending) = server.try_recv() {
                pending.respond(|_| {
                    let image = "x\\\"\n".repeat(1024 * 1024);
                    serde_json::json!({"ok":true,"image":image})
                });
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            client.join().unwrap()["image"].as_str().unwrap().len(),
            4 * 1024 * 1024
        );
    }

    #[test]
    fn every_nodes_tool_retains_its_required_epoch_through_dispatch() {
        for tool in super::super::agent_nodes::tools() {
            let (reply, receive) = mpsc::channel();
            Pending {
                call: ToolCall {
                    name: tool.name.clone(),
                    arguments: serde_json::json!({"expected_document_epoch":"current"}),
                },
                epoch: "current".into(),
                deadline: Instant::now() + TIMEOUT,
                reply,
            }
            .respond(|call| {
                assert_eq!(
                    call.arguments["expected_document_epoch"], "current",
                    "{} must reach its strict application adapter with its epoch",
                    tool.name
                );
                serde_json::json!({"ok":true})
            });
            assert_eq!(receive.recv().unwrap()["ok"], true);
        }
    }

    #[test]
    fn mismatched_epoch_rejects_before_dispatch() {
        let (reply, receive) = mpsc::channel();
        Pending {
            call: ToolCall {
                name: "proposal_install".into(),
                arguments: serde_json::json!({"expected_document_epoch":"old"}),
            },
            epoch: "current".into(),
            deadline: Instant::now() + TIMEOUT,
            reply,
        }
        .respond(|_| panic!("stale document must never execute"));
        let result = receive.recv().unwrap();
        assert_eq!(result["error_code"], "stale_document");
        assert_eq!(result["document_epoch"], "current");
    }

    #[test]
    fn expired_request_never_executes() {
        let (reply, _) = mpsc::channel();
        Pending {
            call: ToolCall {
                name: "propose_edits".into(),
                arguments: Value::Null,
            },
            epoch: "expired".into(),
            deadline: Instant::now() - Duration::from_secs(1),
            reply,
        }
        .respond(|_| panic!("expired mutation must not run"));
    }

    fn apply_call(server: &Server, actor: &str, key: &str) -> ToolCall {
        ToolCall {
            name: "agent_apply".into(),
            arguments: serde_json::json!({
                "expected_document_epoch":server.document_epoch(),
                "actor":actor,
                "operation_key":key,
                "authorization":"user-approved",
                "source":0,
                "history_name":"socket routing fixture",
                "edits":[{
                    "target":{"glyph":"A","glyph_id":"fixture","layer":"public.default",
                        "expected_revision":"fixture"},
                    "operations":[{"op":"set_width","width":500.0}]
                }]
            }),
        }
    }

    fn cancel_call(server: &Server, actor: &str, key: &str) -> ToolCall {
        ToolCall {
            name: "agent_cancel".into(),
            arguments: serde_json::json!({
                "expected_document_epoch":server.document_epoch(),
                "actor":actor,
                "operation_key":key
            }),
        }
    }

    #[test]
    fn cancel_uses_an_independent_connection_while_apply_waits() {
        let server = Server::start().unwrap();
        let path = server.path().to_owned();
        let apply_request = apply_call(&server, "actor", "queued");
        let apply_path = path.clone();
        let apply = std::thread::spawn(move || call(&apply_path, &apply_request).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        let pending = loop {
            if let Some(pending) = server.try_recv() {
                break pending;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        let cancelled = call(&path, &cancel_call(&server, "actor", "queued")).unwrap();
        assert_eq!(cancelled["cancellation_status"], "prevented");
        let identity =
            AgentCancellationIdentity::new(server.document_epoch(), "actor", "queued").unwrap();
        assert_eq!(
            server.cancellations().claim_commit(&identity).unwrap(),
            super::super::agent_cancellation::AgentCommitClaim::Prevented
        );
        pending.respond(|_| serde_json::json!({"ok":false,"status":"cancelled"}));
        assert_eq!(apply.join().unwrap()["status"], "cancelled");
    }

    #[test]
    fn commit_claim_reports_too_late_then_committed_without_cross_actor_effects() {
        let server = Server::start().unwrap();
        let identity =
            AgentCancellationIdentity::new(server.document_epoch(), "actor", "race").unwrap();
        server
            .cancellations()
            .admit(
                &identity,
                super::super::agent_session::AgentPayloadDigest::sha256(b"race"),
            )
            .unwrap();
        assert_eq!(
            server.cancellations().claim_commit(&identity).unwrap(),
            super::super::agent_cancellation::AgentCommitClaim::Claimed
        );
        assert_eq!(
            call(server.path(), &cancel_call(&server, "actor", "race")).unwrap()["cancellation_status"],
            "too_late"
        );
        server
            .cancellations()
            .finish(
                &identity,
                super::super::agent_cancellation::AgentCancellationTerminal::Committed,
            )
            .unwrap();
        assert_eq!(
            call(server.path(), &cancel_call(&server, "actor", "race")).unwrap()["cancellation_status"],
            "committed"
        );
        assert_eq!(
            call(server.path(), &cancel_call(&server, "other", "race")).unwrap()["cancellation_status"],
            "unknown"
        );
    }

    #[test]
    fn dropping_server_interrupts_waiting_connection_and_removes_endpoint() {
        let server = Server::start().unwrap();
        let path = server.path().to_owned();
        let request = apply_call(&server, "actor", "shutdown");
        let client_path = path.clone();
        let client = std::thread::spawn(move || call(&client_path, &request));
        let deadline = Instant::now() + Duration::from_secs(5);
        let _pending = loop {
            if let Some(pending) = server.try_recv() {
                break pending;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        drop(server);
        assert!(client.join().unwrap().is_err());
        assert!(!path.exists());
    }
}

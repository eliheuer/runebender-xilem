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
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::agent::ToolCall;
use serde_json::Value;

const LIMIT: u64 = 8 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);
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
            let expected = self
                .call
                .arguments
                .as_object_mut()
                .and_then(|args| args.remove("expected_document_epoch"));
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
    stop: Arc<AtomicBool>,
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
        let (sender, receiver) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let worker_epoch = epoch.clone();
        std::thread::spawn(move || {
            while !stopping.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                        let result = serve(&mut stream, &sender, &worker_epoch);
                        if let Err(error) = result {
                            let _ = writeln!(
                                stream,
                                "{}",
                                serde_json::json!({"ok": false,
                                "error": error.to_string()})
                            );
                        }
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
            stop,
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

    /// Takes the next request without blocking the UI thread.
    pub fn try_recv(&self) -> Option<Pending> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_file(&self.path);
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

fn read_frame(stream: &mut UnixStream) -> io::Result<String> {
    let mut line = String::new();
    BufReader::new(stream.take(LIMIT + 1)).read_line(&mut line)?;
    if line.len() as u64 > LIMIT || !line.ends_with('\n') {
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
) -> io::Result<()> {
    let call = serde_json::from_str(&read_frame(stream)?)?;
    let (reply, receive) = mpsc::channel();
    sender
        .try_send(Pending {
            call,
            epoch: epoch.to_owned(),
            deadline: Instant::now() + TIMEOUT,
            reply,
        })
        .map_err(|e| io::Error::other(e.to_string()))?;
    let result = receive.recv_timeout(TIMEOUT).map_err(|_| {
        io::Error::new(
            io::ErrorKind::TimedOut,
            "editor did not respond; inspect proposals before retrying",
        )
    })?;
    writeln!(stream, "{result}")
}

/// Sends one bounded call to an explicit editor endpoint. Never falls back to disk.
pub fn call(path: &Path, call: &ToolCall) -> io::Result<Value> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(TIMEOUT + Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = serde_json::to_string(call)?;
    if request.len() as u64 >= LIMIT {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "request too large",
        ));
    }
    writeln!(stream, "{request}")?;
    Ok(serde_json::from_str(&read_frame(&mut stream)?)?)
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
}

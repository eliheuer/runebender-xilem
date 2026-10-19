// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A supervised `llama-server` child for the Chat panel.
//!
//! The editor starts one server per selected GGUF, on a loopback port it
//! picks, with the model's own chat template driving tool calls, and talks
//! to it through [`LocalChatEndpoint`]. The server lives across turns and
//! is killed when the model changes, the panel closes, or the editor
//! quits. No model is downloaded or loaded here without a running server
//! request; this module only runs what is already on disk.

use std::fmt;
use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::local_chat::LocalChatEndpoint;

/// How a server is started for one model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlamaSettings {
    /// Prompt context in tokens.
    pub context: u32,
    /// Whether the model may think before answering. Off is faster.
    pub thinking: bool,
    /// Layers offloaded to the GPU; 99 means all of them.
    pub gpu_layers: u32,
    /// How long to wait for the server to report healthy.
    pub ready_timeout: Duration,
}

impl Default for LlamaSettings {
    fn default() -> Self {
        Self {
            context: 16384,
            thinking: false,
            gpu_layers: 99,
            ready_timeout: Duration::from_secs(180),
        }
    }
}

/// A running server and the endpoint that reaches it.
pub struct LlamaServer {
    child: Child,
    endpoint: LocalChatEndpoint,
    model: PathBuf,
    settings: LlamaSettings,
    log: PathBuf,
}

impl fmt::Debug for LlamaServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LlamaServer")
            .field("pid", &self.child.id())
            .field("endpoint", &self.endpoint.url())
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

/// The `llama-server` binary this editor uses: beside the executable or in
/// the bundle's resources, then `RUNEBENDER_LLAMA_SERVER`, then PATH.
pub fn find_binary() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    };
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        for candidate in [
            dir.join(name),
            dir.join("..").join("Resources").join(name),
            dir.join("..").join("Resources").join("llama").join(name),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if let Some(path) = std::env::var_os("RUNEBENDER_LLAMA_SERVER").filter(|p| !p.is_empty()) {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// The GGUF file a chat model folder holds: `model.gguf` when present,
/// else the first `.gguf` by name.
pub fn gguf_in(folder: &Path) -> Option<PathBuf> {
    let preferred = folder.join("model.gguf");
    if preferred.is_file() {
        return Some(preferred);
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|x| x == "gguf") && path.is_file())
        .collect();
    files.sort();
    files.into_iter().next()
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| format!("no free loopback port: {error}"))?;
    Ok(listener
        .local_addr()
        .map_err(|error| format!("no free loopback port: {error}"))?
        .port())
}

/// One `GET /health`; true on a 200.
fn healthy(address: SocketAddr) -> bool {
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_millis(500)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    if stream
        .write_all(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .is_err()
    {
        return false;
    }
    let mut head = [0_u8; 64];
    let mut read = 0;
    while read < head.len() {
        match stream.read(&mut head[read..]) {
            Ok(0) => break,
            Ok(n) => {
                read += n;
                if head[..read].contains(&b'\n') {
                    break;
                }
            }
            Err(_) => return false,
        }
    }
    let line = String::from_utf8_lossy(&head[..read]);
    line.starts_with("HTTP/1.1 200") || line.starts_with("HTTP/1.0 200")
}

impl LlamaServer {
    /// Start a server for `model` on a free loopback port and wait until it
    /// answers `/health`. The alias is the model name the endpoint sends.
    pub fn start(
        binary: &Path,
        model: &Path,
        alias: &str,
        settings: LlamaSettings,
    ) -> Result<Self, String> {
        let port = free_port()?;
        Self::start_on(binary, model, alias, settings, port)
    }

    /// [`Self::start`] on a chosen port. Tests use this with a stand-in.
    pub fn start_on(
        binary: &Path,
        model: &Path,
        alias: &str,
        settings: LlamaSettings,
        port: u16,
    ) -> Result<Self, String> {
        if !model.is_file() {
            return Err(format!("model file not found: {}", model.display()));
        }
        let endpoint = LocalChatEndpoint::parse(&format!("http://127.0.0.1:{port}/v1"), alias)
            .map_err(|error| error.to_string())?;
        let log = std::env::temp_dir().join(format!(
            "runebender-llama-{}-{port}.log",
            std::process::id()
        ));
        let log_file = std::fs::File::create(&log)
            .map_err(|error| format!("server log {}: {error}", log.display()))?;
        let log_err = log_file
            .try_clone()
            .map_err(|error| format!("server log {}: {error}", log.display()))?;
        let mut command = Command::new(binary);
        command
            .arg("-m")
            .arg(model)
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["--alias", alias, "--jinja", "--no-webui"])
            .args(["-c", &settings.context.to_string()])
            .args(["-ngl", &settings.gpu_layers.to_string()])
            .args(["--reasoning", if settings.thinking { "on" } else { "off" }])
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_err));
        let mut child = command
            .spawn()
            .map_err(|error| format!("start {}: {error}", binary.display()))?;
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let deadline = Instant::now() + settings.ready_timeout;
        loop {
            if healthy(address) {
                break;
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    return Err(format!(
                        "llama-server exited before it was ready ({status}); see {}",
                        log.display()
                    ));
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    return Err(format!("llama-server status: {error}"));
                }
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "llama-server did not become ready within {} s; see {}",
                    settings.ready_timeout.as_secs(),
                    log.display()
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(Self {
            child,
            endpoint,
            model: model.to_path_buf(),
            settings,
            log,
        })
    }

    /// The endpoint the Chat panel sends requests to.
    pub fn endpoint(&self) -> &LocalChatEndpoint {
        &self.endpoint
    }

    /// The GGUF this server loaded.
    pub fn model(&self) -> &Path {
        &self.model
    }

    /// The settings it was started with.
    pub fn settings(&self) -> &LlamaSettings {
        &self.settings
    }

    /// Where the server's own output goes.
    pub fn log(&self) -> &Path {
        &self.log
    }

    /// Whether this server would serve `model` with `settings` as is.
    pub fn serves(&self, model: &Path, settings: &LlamaSettings) -> bool {
        self.model == model && self.settings == *settings
    }

    /// False once the child has exited, for whatever reason.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.log);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!(
            "runebender-llama-test-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    #[cfg(unix)]
    fn script(dir: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let path = dir.join("fake-llama-server");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn fake_model(dir: &Path) -> PathBuf {
        let model = dir.join("model.gguf");
        std::fs::File::create(&model)
            .unwrap()
            .write_all(b"GGUF")
            .unwrap();
        model
    }

    /// A thread answering `/health` with 200 on the given listener until it
    /// is told to stop by a connection to the same port after the flag flips.
    fn health_server(listener: TcpListener) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if flag.load(std::sync::atomic::Ordering::Acquire) {
                    break;
                }
                let Ok(mut stream) = stream else {
                    break;
                };
                let mut buf = [0_u8; 512];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{\"status\":\"ok\"}",
                );
            }
        });
        stop
    }

    #[test]
    fn gguf_in_prefers_model_gguf_then_first_by_name() {
        let dir = scratch("gguf");
        assert_eq!(gguf_in(&dir.0), None);
        std::fs::write(dir.0.join("b.gguf"), b"x").unwrap();
        std::fs::write(dir.0.join("a.gguf"), b"x").unwrap();
        assert_eq!(gguf_in(&dir.0), Some(dir.0.join("a.gguf")));
        std::fs::write(dir.0.join("model.gguf"), b"x").unwrap();
        assert_eq!(gguf_in(&dir.0), Some(dir.0.join("model.gguf")));
    }

    #[cfg(unix)]
    #[test]
    fn start_waits_for_health_passes_flags_and_kills_on_drop() {
        let dir = scratch("start");
        let model = fake_model(&dir.0);
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = health_server(listener);
        let args = dir.0.join("args.txt");
        let pidfile = dir.0.join("pid.txt");
        let binary = script(
            &dir.0,
            &format!(
                "printf '%s\\n' \"$@\" > '{}'\necho $$ > '{}'\nexec sleep 300",
                args.display(),
                pidfile.display()
            ),
        );
        let settings = LlamaSettings {
            context: 4096,
            thinking: false,
            ready_timeout: Duration::from_secs(10),
            ..LlamaSettings::default()
        };
        let mut server =
            LlamaServer::start_on(&binary, &model, "qwen", settings.clone(), port).unwrap();
        assert!(server.is_running());
        assert_eq!(
            server.endpoint().url(),
            format!("http://127.0.0.1:{port}/v1")
        );
        assert_eq!(server.endpoint().model(), "qwen");
        assert!(server.serves(&model, &settings));
        assert!(!server.serves(&model, &LlamaSettings::default()));
        // The stand-in is healthy before the script reaches its writes.
        let started = Instant::now();
        while !(args.is_file() && pidfile.is_file()) && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(50));
        let recorded = std::fs::read_to_string(&args).unwrap();
        let recorded: Vec<&str> = recorded.lines().collect();
        assert!(
            recorded
                .windows(2)
                .any(|w| w == ["-m", model.to_str().unwrap()])
        );
        assert!(
            recorded
                .windows(2)
                .any(|w| w == ["--port", &port.to_string()])
        );
        assert!(recorded.windows(2).any(|w| w == ["-c", "4096"]));
        assert!(recorded.windows(2).any(|w| w == ["--reasoning", "off"]));
        assert!(recorded.contains(&"--jinja") && recorded.contains(&"--no-webui"));
        let pid: i32 = std::fs::read_to_string(&pidfile)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let log = server.log().to_path_buf();
        drop(server);
        stop.store(true, std::sync::atomic::Ordering::Release);
        let _ = TcpStream::connect(("127.0.0.1", port));
        let alive = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .unwrap()
            .success();
        assert!(!alive, "the child must die with the server");
        assert!(!log.exists());
    }

    #[cfg(unix)]
    #[test]
    fn early_exit_and_silence_are_errors_with_the_log_named() {
        let dir = scratch("fail");
        let model = fake_model(&dir.0);
        let exits = script(&dir.0, "echo boom >&2\nexit 3");
        let error =
            LlamaServer::start_on(&exits, &model, "m", LlamaSettings::default(), 1).unwrap_err();
        assert!(error.contains("exited before it was ready"), "{error}");
        let silent = script(&dir.0, "exec sleep 300");
        let settings = LlamaSettings {
            ready_timeout: Duration::from_millis(700),
            ..LlamaSettings::default()
        };
        let port = free_port().unwrap();
        let error = LlamaServer::start_on(&silent, &model, "m", settings, port).unwrap_err();
        assert!(error.contains("did not become ready"), "{error}");
        assert!(
            LlamaServer::start_on(
                &silent,
                &dir.0.join("none.gguf"),
                "m",
                LlamaSettings::default(),
                port
            )
            .unwrap_err()
            .contains("not found")
        );
    }
}

#[cfg(test)]
mod real_server {
    //! Runs only with `RUNEBENDER_REAL_LLAMA=<gguf>` and a `llama-server`
    //! on PATH: starts the real server, asks one question, stops it.
    use super::*;
    use crate::workflows::local_chat::LocalChatLimits;
    use crate::workflows::process::ProcessCancellation;

    #[test]
    #[ignore = "needs llama-server and a GGUF; set RUNEBENDER_REAL_LLAMA"]
    fn real_llama_server_answers_one_question() {
        let Some(gguf) = std::env::var_os("RUNEBENDER_REAL_LLAMA").map(PathBuf::from) else {
            return;
        };
        let binary = find_binary().expect("llama-server on PATH");
        let started = Instant::now();
        let server = LlamaServer::start(&binary, &gguf, "test", LlamaSettings::default()).unwrap();
        let load = started.elapsed();
        let request = serde_json::json!({
            "messages": [{"role": "user", "content": "Reply with the single word: ready"}],
            "max_tokens": 16,
        });
        let answer = server
            .endpoint()
            .complete(
                &request,
                LocalChatLimits::default(),
                &ProcessCancellation::default(),
            )
            .unwrap();
        let text = answer["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default();
        assert!(
            text.to_lowercase().contains("ready"),
            "loaded in {load:?}; answered in {:?}: {answer}",
            started.elapsed() - load
        );
    }
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A supervised `whisper-server` child that turns a spoken request into
//! text for the Chat panel.
//!
//! The server is whisper.cpp's, the sister project of the llama.cpp server
//! the chat runs on, and it is found and run the same way: from the app
//! bundle's `Resources/whisper/`, then `RUNEBENDER_WHISPER_SERVER`, then
//! PATH and Homebrew's folders (`brew install whisper.cpp`). It loads one
//! speech model, a whisper.cpp `ggml-*.bin`, kept as `model.bin` in a
//! model folder whose name starts with `whisper`.
//!
//! Audio reaches it as a 16 kHz mono WAV in one `POST /inference`; the
//! reply is the text. This module does not touch the microphone; the
//! editor records and hands the samples to [`WhisperServer::transcribe`].

use std::fmt;
use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The sample rate whisper models are trained on and the server expects.
pub const SAMPLE_RATE: u32 = 16_000;

/// How long to wait for the server to answer requests after it starts.
const READY_TIMEOUT: Duration = Duration::from_secs(60);

/// How long one transcription may take, end to end.
const TRANSCRIBE_TIMEOUT: Duration = Duration::from_secs(120);

/// The `whisper-server` binary this editor uses, looked up like the chat's
/// `llama-server`: beside the executable or in the bundle's resources,
/// then `RUNEBENDER_WHISPER_SERVER`, then PATH and Homebrew's folders.
pub fn find_binary() -> Option<PathBuf> {
    let name = if cfg!(windows) {
        "whisper-server.exe"
    } else {
        "whisper-server"
    };
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        for candidate in [
            dir.join(name),
            dir.join("..").join("Resources").join(name),
            dir.join("..").join("Resources").join("whisper").join(name),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if let Some(path) = std::env::var_os("RUNEBENDER_WHISPER_SERVER").filter(|p| !p.is_empty()) {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .chain(
            ["/opt/homebrew/bin", "/usr/local/bin"]
                .into_iter()
                .map(PathBuf::from),
        )
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// The speech model to load: `RUNEBENDER_WHISPER_MODEL` when it names a
/// file, else the `model.bin` of the first folder under `roots` whose name
/// starts with `whisper`, by name.
pub fn find_model(roots: &[PathBuf]) -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("RUNEBENDER_WHISPER_MODEL").filter(|p| !p.is_empty()) {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let mut folders: Vec<PathBuf> = roots
        .iter()
        .filter_map(|root| std::fs::read_dir(root).ok())
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("whisper"))
                && path.join("model.bin").is_file()
        })
        .collect();
    folders.sort();
    folders
        .into_iter()
        .next()
        .map(|folder| folder.join("model.bin"))
}

/// Mono 16 kHz samples from what a microphone gave: channels averaged,
/// then resampled by linear interpolation, which is plenty for speech.
pub fn to_mono_16k(samples: &[f32], channels: u16, rate: u32) -> Vec<f32> {
    let channels = usize::from(channels.max(1));
    let mono: Vec<f32> = samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    if rate == SAMPLE_RATE || mono.is_empty() {
        return mono;
    }
    let step = f64::from(rate) / f64::from(SAMPLE_RATE);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a sample count, floored, within the input's length"
    )]
    let count = ((mono.len() - 1) as f64 / step).floor() as usize + 1;
    (0..count)
        .map(|i| {
            let position = i as f64 * step;
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the sample before position, within the input's length"
            )]
            let index = position as usize;
            let next = (index + 1).min(mono.len() - 1);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a fraction in 0..1; f32 is plenty for a blend"
            )]
            let t = (position - index as f64) as f32;
            mono[index] * (1.0 - t) + mono[next] * t
        })
        .collect()
}

/// A 16-bit PCM WAV file holding mono samples at [`SAMPLE_RATE`].
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes());
    out.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1_u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2_u16.to_le_bytes());
    out.extend_from_slice(&16_u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "clamped to the i16 range first"
        )]
        let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// A running server and where it listens.
pub struct WhisperServer {
    child: Child,
    address: SocketAddr,
    model: PathBuf,
    log: PathBuf,
}

impl fmt::Debug for WhisperServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WhisperServer")
            .field("pid", &self.child.id())
            .field("address", &self.address)
            .field("model", &self.model)
            .finish_non_exhaustive()
    }
}

fn free_port() -> Result<u16, String> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .map_err(|error| format!("no free loopback port: {error}"))?;
    Ok(listener
        .local_addr()
        .map_err(|error| format!("no free loopback port: {error}"))?
        .port())
}

/// One request; the whole response once the server closes the connection.
fn exchange(address: SocketAddr, request: &[u8], timeout: Duration) -> Result<Vec<u8>, String> {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(500))
        .map_err(|error| format!("whisper-server: {error}"))?;
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));
    stream
        .write_all(request)
        .map_err(|error| format!("whisper-server: {error}"))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("whisper-server: {error}"))?;
    Ok(response)
}

/// Status code and body of an HTTP/1.1 response.
fn split_response(response: &[u8]) -> Result<(u16, &[u8]), String> {
    let end = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("whisper-server: incomplete response")?;
    let head = String::from_utf8_lossy(&response[..end]);
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or("whisper-server: no status line")?;
    Ok((status, &response[end + 4..]))
}

fn ready(address: SocketAddr) -> bool {
    exchange(
        address,
        b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        Duration::from_secs(2),
    )
    .ok()
    .and_then(|response| split_response(&response).ok().map(|(status, _)| status))
    .is_some_and(|status| status < 500)
}

impl WhisperServer {
    /// Start a server for `model` on a free loopback port and wait until it
    /// answers.
    pub fn start(binary: &Path, model: &Path) -> Result<Self, String> {
        let port = free_port()?;
        Self::start_on(binary, model, port)
    }

    /// [`Self::start`] on a chosen port. Tests use this with a stand-in.
    pub fn start_on(binary: &Path, model: &Path, port: u16) -> Result<Self, String> {
        if !model.is_file() {
            return Err(format!("speech model not found: {}", model.display()));
        }
        let log = std::env::temp_dir().join(format!(
            "runebender-whisper-{}-{port}.log",
            std::process::id()
        ));
        let log_file = std::fs::File::create(&log)
            .map_err(|error| format!("server log {}: {error}", log.display()))?;
        let log_err = log_file
            .try_clone()
            .map_err(|error| format!("server log {}: {error}", log.display()))?;
        let mut child = Command::new(binary)
            .arg("-m")
            .arg(model)
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["--language", "en", "--no-timestamps"])
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_err))
            .spawn()
            .map_err(|error| format!("start {}: {error}", binary.display()))?;
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            if ready(address) {
                break;
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    return Err(format!(
                        "whisper-server exited before it was ready ({status}); see {}",
                        log.display()
                    ));
                }
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    return Err(format!("whisper-server status: {error}"));
                }
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "whisper-server did not become ready within {} s; see {}",
                    READY_TIMEOUT.as_secs(),
                    log.display()
                ));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Ok(Self {
            child,
            address,
            model: model.to_path_buf(),
            log,
        })
    }

    /// The model this server loaded.
    pub fn model(&self) -> &Path {
        &self.model
    }

    /// False once the child has exited, for whatever reason.
    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// The words in `samples`, mono at [`SAMPLE_RATE`]. `prompt` is text
    /// in the expected vocabulary, which steers spelling toward it.
    pub fn transcribe(&self, samples: &[f32], prompt: &str) -> Result<String, String> {
        let boundary = "runebender-voice-7f3a9c";
        let mut body = Vec::new();
        let mut field = |name: &str, value: &str| {
            body.extend_from_slice(
                format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
                )
                .as_bytes(),
            );
        };
        field("response_format", "json");
        field("temperature", "0.0");
        if !prompt.trim().is_empty() {
            field("prompt", prompt.trim());
        }
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"voice.wav\"\r\nContent-Type: audio/wav\r\n\r\n"
            )
            .as_bytes(),
        );
        body.extend_from_slice(&wav(samples));
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let head = format!(
            "POST /inference HTTP/1.1\r\nHost: localhost\r\nContent-Type: multipart/form-data; boundary={boundary}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let mut request = head.into_bytes();
        request.extend_from_slice(&body);
        let response = exchange(self.address, &request, TRANSCRIBE_TIMEOUT)?;
        let (status, body) = split_response(&response)?;
        let text = String::from_utf8_lossy(body);
        if status != 200 {
            return Err(format!("whisper-server answered {status}: {}", text.trim()));
        }
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|error| format!("whisper-server reply: {error}: {}", text.trim()))?;
        if let Some(error) = value.get("error").and_then(serde_json::Value::as_str) {
            return Err(format!("whisper-server: {error}"));
        }
        Ok(value
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "))
    }
}

impl Drop for WhisperServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.log);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_48k_becomes_mono_16k() {
        // Three stereo frames per output sample; a constant signal stays constant.
        let samples: Vec<f32> = (0..480).flat_map(|_| [0.5_f32, 0.25]).collect();
        let mono = to_mono_16k(&samples, 2, 48_000);
        assert_eq!(mono.len(), 160);
        assert!(mono.iter().all(|s| (s - 0.375).abs() < 1e-6), "{mono:?}");
        assert_eq!(to_mono_16k(&[0.1, 0.2], 1, SAMPLE_RATE), vec![0.1, 0.2]);
        assert!(to_mono_16k(&[], 2, 48_000).is_empty());
    }

    #[test]
    fn a_wav_has_the_header_and_clamped_samples() {
        let bytes = wav(&[0.0, 1.0, -2.0]);
        assert_eq!(bytes.len(), 44 + 6);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            16_000
        );
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 6);
        assert_eq!(
            i16::from_le_bytes(bytes[46..48].try_into().unwrap()),
            i16::MAX
        );
        assert_eq!(
            i16::from_le_bytes(bytes[48..50].try_into().unwrap()),
            -i16::MAX
        );
    }

    #[test]
    fn the_model_is_the_first_whisper_folder_with_a_model_bin() {
        let dir =
            std::env::temp_dir().join(format!("runebender-whisper-find-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("whisper-small")).unwrap();
        std::fs::create_dir_all(dir.join("whisper-base")).unwrap();
        std::fs::create_dir_all(dir.join("qwen3.5-4b")).unwrap();
        std::fs::write(dir.join("qwen3.5-4b").join("model.bin"), b"x").unwrap();
        assert_eq!(find_model(std::slice::from_ref(&dir)), None);
        std::fs::write(dir.join("whisper-small").join("model.bin"), b"x").unwrap();
        std::fs::write(dir.join("whisper-base").join("model.bin"), b"x").unwrap();
        assert_eq!(
            find_model(std::slice::from_ref(&dir)),
            Some(dir.join("whisper-base").join("model.bin"))
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_transcription_is_read_from_the_json_text_field() {
        use std::sync::mpsc;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let (sent, received) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                let n = stream.read(&mut chunk).unwrap();
                request.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&request);
                if let Some(end) = text.find("\r\n\r\n") {
                    let length: usize = text
                        .lines()
                        .find_map(|line| line.strip_prefix("Content-Length: "))
                        .unwrap()
                        .parse()
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            sent.send(request).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 35\r\nConnection: close\r\n\r\n{\"text\":\"  Make the zero wider.\\n\"}",
                )
                .unwrap();
        });
        let server = WhisperServer {
            child: Command::new("sleep").arg("30").spawn().unwrap(),
            address,
            model: PathBuf::new(),
            log: std::env::temp_dir().join("runebender-whisper-test-none.log"),
        };
        let text = server
            .transcribe(&[0.0; 160], "glyph, sidebearing")
            .unwrap();
        assert_eq!(text, "Make the zero wider.");
        let request = received.recv().unwrap();
        let request = String::from_utf8_lossy(&request);
        assert!(
            request.starts_with("POST /inference HTTP/1.1\r\n"),
            "{request}"
        );
        assert!(request.contains("name=\"prompt\"\r\n\r\nglyph, sidebearing\r\n"));
        assert!(request.contains("filename=\"voice.wav\""));
        assert!(request.contains("RIFF"));
    }
}

#[cfg(test)]
mod real_server {
    //! Runs only with `RUNEBENDER_REAL_WHISPER=<wav>` (16 kHz mono) and a
    //! `whisper-server` and model on this machine: transcribes the file.
    use super::*;

    #[test]
    #[ignore = "needs whisper-server and a speech model; set RUNEBENDER_REAL_WHISPER"]
    fn real_whisper_server_transcribes_a_file() {
        let Some(path) = std::env::var_os("RUNEBENDER_REAL_WHISPER").map(PathBuf::from) else {
            return;
        };
        let bytes = std::fs::read(&path).unwrap();
        let samples: Vec<f32> = bytes[44..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| f32::from(i16::from_le_bytes(*b)) / f32::from(i16::MAX))
            .collect();
        let binary = find_binary().expect("whisper-server on PATH");
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        let model = find_model(&[home.join("Runebender").join("models")]).expect("a whisper model");
        let started = Instant::now();
        let server = WhisperServer::start(&binary, &model).unwrap();
        let load = started.elapsed();
        let text = server
            .transcribe(&samples, "glyph, sidebearing, eszett")
            .unwrap();
        assert!(
            !text.is_empty(),
            "loaded in {load:?}; transcribed in {:?}: {text:?}",
            started.elapsed() - load
        );
    }
}

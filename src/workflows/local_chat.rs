// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded inference requests to an already running local chat server.
//!
//! The endpoint owns only its address and model name. The external `font-ml serve`
//! process keeps the model resident across requests; this module neither starts it
//! nor stores model runtime state in a font document. Cancellation stops waiting
//! for a response but cannot stop inference already running in that server.

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

use serde_json::Value;

use super::process::ProcessCancellation;

#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, Read, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::net::TcpStream;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

/// A loopback HTTP endpoint for a model held by an external server.
#[derive(Debug, Clone)]
pub struct LocalChatEndpoint {
    address: SocketAddr,
    base_url: String,
    model: String,
}

impl LocalChatEndpoint {
    /// Parse a loopback URL with an explicit port and a nonempty model name.
    ///
    /// Accepted URL paths are the root and `/v1`; the completion path is appended.
    pub fn parse(url: &str, model: &str) -> Result<Self, LocalChatError> {
        let authority_and_path = url.strip_prefix("http://").ok_or_else(|| {
            LocalChatError::InvalidConfig("local chat requires plain HTTP on loopback".into())
        })?;
        if authority_and_path.is_empty()
            || authority_and_path
                .bytes()
                .any(|b| b.is_ascii_whitespace() || matches!(b, b'@' | b'?' | b'#' | b'%' | b'\\'))
        {
            return Err(LocalChatError::InvalidConfig(
                "local chat URL contains unsupported authority or URL components".into(),
            ));
        }
        let (authority, path) = authority_and_path
            .split_once('/')
            .map_or((authority_and_path, ""), |(authority, path)| {
                (authority, path)
            });
        if !matches!(path, "" | "v1") {
            return Err(LocalChatError::InvalidConfig(
                "local chat URL path must be / or /v1".into(),
            ));
        }
        let address = if let Some(port) = authority.strip_prefix("localhost:") {
            let port = parse_port(port)?;
            SocketAddr::from(([127, 0, 0, 1], port))
        } else {
            let address = SocketAddr::from_str(authority).map_err(|_| {
                LocalChatError::InvalidConfig(
                    "local chat URL needs a literal loopback IP and explicit port".into(),
                )
            })?;
            if !matches!(address.ip(), IpAddr::V4(ip) if ip.is_loopback())
                && !matches!(address.ip(), IpAddr::V6(ip) if ip.is_loopback())
            {
                return Err(LocalChatError::InvalidConfig(
                    "local chat address must be loopback".into(),
                ));
            }
            if address.port() == 0 {
                return Err(LocalChatError::InvalidConfig(
                    "local chat port must be nonzero".into(),
                ));
            }
            address
        };
        if model.is_empty()
            || model.len() > 256
            || model.trim() != model
            || model.chars().any(char::is_control)
        {
            return Err(LocalChatError::InvalidConfig(
                "local chat model must be 1 to 256 bytes without surrounding whitespace or controls"
                    .into(),
            ));
        }
        let base_url = format!(
            "http://{authority}{}",
            if path == "v1" { "/v1" } else { "" }
        );
        Ok(Self {
            address,
            base_url,
            model: model.into(),
        })
    }

    /// The model name configured by the host.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// The validated base URL shown in configuration UI.
    pub fn url(&self) -> String {
        self.base_url.clone()
    }

    /// Send one nonstreaming completion request to the resident local model.
    ///
    /// The configured model and `stream: false` override those fields in `request`.
    /// Cancellation discards the result; it does not interrupt server inference.
    pub fn complete(
        &self,
        request: &Value,
        limits: LocalChatLimits,
        cancellation: &ProcessCancellation,
    ) -> Result<Value, LocalChatError> {
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (self.address, request, limits, cancellation);
            Err(LocalChatError::BrowserUnavailable)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.complete_native(request, limits, cancellation)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn complete_native(
        &self,
        request: &Value,
        limits: LocalChatLimits,
        cancellation: &ProcessCancellation,
    ) -> Result<Value, LocalChatError> {
        limits.validate()?;
        let deadline = Instant::now() + limits.deadline;
        check_progress(deadline, cancellation)?;
        let mut request = request.as_object().cloned().ok_or_else(|| {
            LocalChatError::InvalidRequest("chat completion request must be a JSON object".into())
        })?;
        request.insert("model".into(), Value::String(self.model.clone()));
        request.insert("stream".into(), Value::Bool(false));
        let mut body = BoundedBody::new(limits.request_bytes);
        serde_json::to_writer(&mut body, &Value::Object(request)).map_err(|e| {
            LocalChatError::InvalidRequest(format!(
                "request exceeds byte limit or cannot serialize: {e}"
            ))
        })?;
        let body = body.into_inner();
        check_progress(deadline, cancellation)?;

        let mut stream = TcpStream::connect_timeout(
            &self.address,
            remaining(deadline, cancellation)?.min(IO_POLL),
        )
        .map_err(|e| {
            check_progress(deadline, cancellation)
                .err()
                .unwrap_or_else(|| {
                    LocalChatError::Transport(format!("cannot connect to local model: {e}"))
                })
        })?;
        let path = "/v1/chat/completions";
        let host = match self.address.ip() {
            IpAddr::V4(ip) => format!("{ip}:{}", self.address.port()),
            IpAddr::V6(ip) => format!("[{ip}]:{}", self.address.port()),
        };
        let head = format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        write_bounded(&mut stream, head.as_bytes(), deadline, cancellation)?;
        write_bounded(&mut stream, &body, deadline, cancellation)?;
        let response = read_response(&mut stream, limits.response_bytes, deadline, cancellation)?;
        check_progress(deadline, cancellation)?;
        Ok(response)
    }
}

fn parse_port(text: &str) -> Result<u16, LocalChatError> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(LocalChatError::InvalidConfig(
            "local chat URL needs an explicit numeric port".into(),
        ));
    }
    let port = text.parse::<u16>().map_err(|_| {
        LocalChatError::InvalidConfig("local chat port must be in the TCP port range".into())
    })?;
    if port == 0 {
        return Err(LocalChatError::InvalidConfig(
            "local chat port must be nonzero".into(),
        ));
    }
    Ok(port)
}

/// Limits for a single completion request and response.
#[derive(Clone, Copy, Debug)]
pub struct LocalChatLimits {
    /// Maximum elapsed time for the whole request, including local serialization.
    pub deadline: Duration,
    /// Maximum serialized JSON request bytes.
    pub request_bytes: usize,
    /// Maximum JSON response body bytes.
    pub response_bytes: usize,
}

impl Default for LocalChatLimits {
    fn default() -> Self {
        Self {
            deadline: Duration::from_secs(120),
            request_bytes: 1024 * 1024,
            response_bytes: 4 * 1024 * 1024,
        }
    }
}

impl LocalChatLimits {
    #[cfg(not(target_arch = "wasm32"))]
    fn validate(self) -> Result<(), LocalChatError> {
        if self.deadline.is_zero() || self.deadline > Duration::from_secs(24 * 60 * 60) {
            return Err(LocalChatError::InvalidConfig(
                "local chat deadline must be greater than zero and no longer than 24 hours".into(),
            ));
        }
        if self.request_bytes == 0
            || self.response_bytes == 0
            || self.request_bytes > 16 * 1024 * 1024
            || self.response_bytes > 64 * 1024 * 1024
        {
            return Err(LocalChatError::InvalidConfig(
                "local chat byte limits must be positive (request <= 16 MiB; response <= 64 MiB)"
                    .into(),
            ));
        }
        Ok(())
    }
}

/// A configuration, request, transport, or server failure for local inference.
#[derive(Debug)]
pub enum LocalChatError {
    /// The endpoint or limits are invalid.
    InvalidConfig(String),
    /// The request is invalid or exceeds its byte limit.
    InvalidRequest(String),
    /// The response violates the supported HTTP or JSON protocol.
    Protocol(String),
    /// The local socket failed.
    Transport(String),
    /// The local server returned an unsuccessful HTTP status.
    Server {
        /// The HTTP status returned by the server.
        status: u16,
        /// A bounded diagnostic extracted from the server response.
        message: String,
    },
    /// The caller stopped waiting; server inference may continue.
    Cancelled,
    /// The request exceeded its wall clock deadline; server inference may continue.
    DeadlineExceeded,
    /// Local sockets are unavailable in the browser build.
    BrowserUnavailable,
}

impl fmt::Display for LocalChatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => {
                write!(f, "invalid local chat configuration: {message}")
            }
            Self::InvalidRequest(message) => write!(f, "invalid local chat request: {message}"),
            Self::Protocol(message) => write!(f, "invalid local chat response: {message}"),
            Self::Transport(message) => write!(f, "local chat transport failed: {message}"),
            Self::Server { status, message } => {
                write!(f, "local chat server returned HTTP {status}: {message}")
            }
            Self::Cancelled => write!(
                f,
                "local chat request cancelled; inference already running on the server may continue"
            ),
            Self::DeadlineExceeded => write!(
                f,
                "local chat deadline exceeded; inference already running on the server may continue"
            ),
            Self::BrowserUnavailable => write!(f, "local chat sockets are unavailable in browser"),
        }
    }
}

impl std::error::Error for LocalChatError {}

#[cfg(not(target_arch = "wasm32"))]
const IO_POLL: Duration = Duration::from_millis(50);
#[cfg(not(target_arch = "wasm32"))]
const HEADER_BYTES: usize = 16 * 1024;

#[cfg(not(target_arch = "wasm32"))]
fn remaining(
    deadline: Instant,
    cancellation: &ProcessCancellation,
) -> Result<Duration, LocalChatError> {
    if cancellation.is_cancelled() {
        return Err(LocalChatError::Cancelled);
    }
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or(LocalChatError::DeadlineExceeded)
}

#[cfg(not(target_arch = "wasm32"))]
fn check_progress(
    deadline: Instant,
    cancellation: &ProcessCancellation,
) -> Result<(), LocalChatError> {
    remaining(deadline, cancellation).map(|_| ())
}

#[cfg(not(target_arch = "wasm32"))]
fn write_bounded(
    stream: &mut TcpStream,
    mut bytes: &[u8],
    deadline: Instant,
    cancellation: &ProcessCancellation,
) -> Result<(), LocalChatError> {
    while !bytes.is_empty() {
        stream
            .set_write_timeout(Some(remaining(deadline, cancellation)?.min(IO_POLL)))
            .map_err(transport_error)?;
        match stream.write(bytes) {
            Ok(0) => {
                return Err(LocalChatError::Transport(
                    "connection closed while writing".into(),
                ));
            }
            Ok(n) => bytes = &bytes[n..],
            Err(e) if retryable(&e) => {}
            Err(e) => return Err(transport_error(e)),
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn read_bounded(
    stream: &mut TcpStream,
    bytes: &mut [u8],
    deadline: Instant,
    cancellation: &ProcessCancellation,
) -> Result<usize, LocalChatError> {
    loop {
        stream
            .set_read_timeout(Some(remaining(deadline, cancellation)?.min(IO_POLL)))
            .map_err(transport_error)?;
        match stream.read(bytes) {
            Ok(n) => return Ok(n),
            Err(e) if retryable(&e) => {}
            Err(e) => return Err(transport_error(e)),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn retryable(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn transport_error(error: io::Error) -> LocalChatError {
    LocalChatError::Transport(error.to_string())
}

#[cfg(not(target_arch = "wasm32"))]
fn read_response(
    stream: &mut TcpStream,
    body_limit: usize,
    deadline: Instant,
    cancellation: &ProcessCancellation,
) -> Result<Value, LocalChatError> {
    let mut received = Vec::new();
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let n = read_bounded(stream, &mut chunk, deadline, cancellation)?;
        if n == 0 {
            return Err(LocalChatError::Protocol(
                "response ended before headers".into(),
            ));
        }
        received.extend_from_slice(&chunk[..n]);
        if let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") {
            let end = end + 4;
            if end > HEADER_BYTES {
                return Err(LocalChatError::Protocol(
                    "response headers exceed 16 KiB".into(),
                ));
            }
            break end;
        }
        if received.len() > HEADER_BYTES {
            return Err(LocalChatError::Protocol(
                "response headers exceed 16 KiB".into(),
            ));
        }
    };
    let (status, body_len) = parse_headers(&received[..header_end])?;
    if body_len > body_limit {
        return Err(LocalChatError::Protocol(format!(
            "response body of {body_len} bytes exceeds {body_limit} byte limit"
        )));
    }
    let mut body = Vec::with_capacity(body_len);
    let initial = &received[header_end..];
    if initial.len() > body_len {
        return Err(LocalChatError::Protocol(
            "response contains bytes beyond Content-Length".into(),
        ));
    }
    body.extend_from_slice(initial);
    while body.len() < body_len {
        let remaining_len = body_len - body.len();
        let read_len = remaining_len.min(chunk.len());
        let n = read_bounded(stream, &mut chunk[..read_len], deadline, cancellation)?;
        if n == 0 {
            return Err(LocalChatError::Protocol(
                "response body ended before Content-Length".into(),
            ));
        }
        body.extend_from_slice(&chunk[..n]);
    }
    check_progress(deadline, cancellation)?;
    if status != 200 {
        let message = serde_json::from_slice::<Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| String::from_utf8_lossy(&body).into_owned());
        let message: String = message
            .chars()
            .filter(|c| !c.is_control())
            .take(200)
            .collect();
        return Err(LocalChatError::Server { status, message });
    }
    let value: Value = serde_json::from_slice(&body)
        .map_err(|e| LocalChatError::Protocol(format!("response is not JSON: {e}")))?;
    if value.get("choices").and_then(Value::as_array).is_none() {
        return Err(LocalChatError::Protocol(
            "completion response must contain a choices array".into(),
        ));
    }
    Ok(value)
}

#[cfg(not(target_arch = "wasm32"))]
fn parse_headers(bytes: &[u8]) -> Result<(u16, usize), LocalChatError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| LocalChatError::Protocol("response headers are not UTF-8".into()))?;
    let mut lines = text.strip_suffix("\r\n\r\n").unwrap_or(text).split("\r\n");
    let status_line = lines.next().unwrap_or("");
    let mut status_parts = status_line.splitn(3, ' ');
    if status_parts.next() != Some("HTTP/1.1") {
        return Err(LocalChatError::Protocol(
            "response must use HTTP/1.1".into(),
        ));
    }
    let code = status_parts.next().unwrap_or("");
    let reason = status_parts.next().unwrap_or("");
    if code.len() != 3
        || !code.bytes().all(|b| b.is_ascii_digit())
        || reason.is_empty()
        || reason.chars().any(char::is_control)
    {
        return Err(LocalChatError::Protocol(
            "malformed HTTP status line".into(),
        ));
    }
    let status = code
        .parse::<u16>()
        .map_err(|_| LocalChatError::Protocol("invalid HTTP status".into()))?;
    if !(200..=599).contains(&status) {
        return Err(LocalChatError::Protocol("unsupported HTTP status".into()));
    }
    let mut length = None;
    let mut json_content_type = None;
    for line in lines {
        if line.is_empty()
            || line.contains('\r')
            || line.contains('\n')
            || line.starts_with(' ')
            || line.starts_with('\t')
        {
            return Err(LocalChatError::Protocol("malformed response header".into()));
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| LocalChatError::Protocol("malformed response header".into()))?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err(LocalChatError::Protocol(
                "malformed response header name".into(),
            ));
        }
        if value.chars().any(|c| c.is_control() && c != '\t') {
            return Err(LocalChatError::Protocol(
                "control character in response header".into(),
            ));
        }
        let value = value.trim();
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(LocalChatError::Protocol(
                "Transfer-Encoding is unsupported".into(),
            ));
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() || value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
                return Err(LocalChatError::Protocol("ambiguous Content-Length".into()));
            }
            length = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| LocalChatError::Protocol("invalid Content-Length".into()))?,
            );
        }
        if name.eq_ignore_ascii_case("content-type") {
            if json_content_type.is_some() {
                return Err(LocalChatError::Protocol("duplicate Content-Type".into()));
            }
            json_content_type = Some(
                value.eq_ignore_ascii_case("application/json")
                    || value.to_ascii_lowercase().starts_with("application/json;"),
            );
        }
    }
    let length = length.ok_or_else(|| LocalChatError::Protocol("missing Content-Length".into()))?;
    if status == 200 && json_content_type != Some(true) {
        return Err(LocalChatError::Protocol(
            "completion response must be JSON".into(),
        ));
    }
    Ok((status, length))
}

#[cfg(not(target_arch = "wasm32"))]
struct BoundedBody {
    bytes: Vec<u8>,
    limit: usize,
}

#[cfg(not(target_arch = "wasm32"))]
impl BoundedBody {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Write for BoundedBody {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("request byte limit exceeded"));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::net::{SocketAddr, TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::thread;

    use serde_json::json;

    use super::*;

    fn endpoint(address: SocketAddr) -> LocalChatEndpoint {
        LocalChatEndpoint::parse(&format!("http://{address}/v1"), "font-ml").unwrap()
    }

    fn response(status: &str, body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        response
    }

    fn read_request(stream: &mut TcpStream) -> Value {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut received = Vec::new();
        let mut chunk = [0_u8; 1024];
        let end = loop {
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0, "client closed before request headers");
            received.extend_from_slice(&chunk[..n]);
            if let Some(index) = received.windows(4).position(|w| w == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let header = std::str::from_utf8(&received[..end]).unwrap();
        assert!(
            header.starts_with("POST /v1/chat/completions HTTP/1.1\r\n"),
            "unexpected request target: {header}"
        );
        let length: usize = header
            .split("\r\n")
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .unwrap()
            .parse()
            .unwrap();
        while received.len() - end < length {
            let n = stream.read(&mut chunk).unwrap();
            assert!(n > 0, "client closed before request body");
            received.extend_from_slice(&chunk[..n]);
        }
        serde_json::from_slice(&received[end..end + length]).unwrap()
    }

    fn fixture(responses: Vec<Vec<u8>>) -> (SocketAddr, thread::JoinHandle<Vec<Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let task = thread::spawn(move || {
            let mut requests = Vec::new();
            for response in responses {
                let until = Instant::now() + Duration::from_secs(3);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e)
                            if e.kind() == io::ErrorKind::WouldBlock && Instant::now() < until =>
                        {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(e) => panic!("fixture accept failed: {e}"),
                    }
                };
                requests.push(read_request(&mut stream));
                let _ = stream.write_all(&response);
            }
            requests
        });
        (address, task)
    }

    #[test]
    fn repeated_requests_use_one_resident_server() {
        let success = response("200 OK", br#"{"choices":[{"message":{"content":"ok"}}]}"#);
        let (address, server) = fixture(vec![success.clone(), success]);
        let endpoint = endpoint(address);
        for _ in 0..2 {
            let result = endpoint
                .complete(
                    &json!({"messages": [], "model": "caller", "stream": true}),
                    LocalChatLimits::default(),
                    &ProcessCancellation::default(),
                )
                .unwrap();
            assert_eq!(result["choices"][0]["message"]["content"], "ok");
        }
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(
            requests
                .iter()
                .all(|r| r["model"] == "font-ml" && r["stream"] == false)
        );
    }

    #[test]
    fn rejects_unsafe_configuration_and_bounds() {
        for url in [
            "https://127.0.0.1:1234/v1",
            "http://192.0.2.1:1234/v1",
            "http://localhost/v1",
            "http://127.0.0.1:1234/v1/../v1",
            "http://user@127.0.0.1:1234/v1",
            "http://127.0.0.1:1234/v1?x=1",
            "http://127.0.0.1:1234/v1#x",
        ] {
            assert!(
                matches!(
                    LocalChatEndpoint::parse(url, "model"),
                    Err(LocalChatError::InvalidConfig(_))
                ),
                "{url}"
            );
        }
        assert!(LocalChatEndpoint::parse("http://localhost:1234/v1", "model").is_ok());
        assert!(matches!(
            LocalChatEndpoint::parse("http://127.0.0.1:1234/v1", " "),
            Err(LocalChatError::InvalidConfig(_))
        ));
        let endpoint = LocalChatEndpoint::parse("http://127.0.0.1:1234/v1", "model").unwrap();
        let limits = LocalChatLimits {
            request_bytes: 4,
            ..Default::default()
        };
        assert!(matches!(
            endpoint.complete(
                &json!({"messages": []}),
                limits,
                &ProcessCancellation::default()
            ),
            Err(LocalChatError::InvalidRequest(_))
        ));
    }

    #[test]
    fn rejects_server_failure_and_malformed_responses() {
        let cases = [
            (
                response("500 Error", br#"{"error":{"message":"model failed"}}"#),
                "server",
            ),
            (
                b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\n{".to_vec(),
                "protocol",
            ),
            (
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec(),
                "protocol",
            ),
            (
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 9\r\n\r\n{}"
                    .to_vec(),
                "protocol",
            ),
            (response("200 OK", br#"{"wrong":true}"#), "protocol"),
        ];
        for (wire, expected) in cases {
            let (address, server) = fixture(vec![wire]);
            let result = endpoint(address).complete(
                &json!({"messages": []}),
                LocalChatLimits::default(),
                &ProcessCancellation::default(),
            );
            assert!(match expected {
                "server" => matches!(result, Err(LocalChatError::Server { status: 500, .. })),
                _ => matches!(result, Err(LocalChatError::Protocol(_))),
            });
            server.join().unwrap();
        }
    }

    #[test]
    fn rejects_oversized_body_before_reading_it() {
        let wire =
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1000\r\n\r\n"
                .to_vec();
        let (address, server) = fixture(vec![wire]);
        let limits = LocalChatLimits {
            response_bytes: 32,
            ..Default::default()
        };
        let result = endpoint(address).complete(
            &json!({"messages": []}),
            limits,
            &ProcessCancellation::default(),
        );
        assert!(matches!(result, Err(LocalChatError::Protocol(_))));
        server.join().unwrap();
    }

    #[test]
    fn pre_cancelled_request_never_connects() {
        let cancellation = ProcessCancellation::default();
        cancellation.cancel();
        let endpoint = LocalChatEndpoint::parse("http://127.0.0.1:9/v1", "model").unwrap();
        assert!(matches!(
            endpoint.complete(&json!({}), LocalChatLimits::default(), &cancellation),
            Err(LocalChatError::Cancelled)
        ));
    }

    #[test]
    fn cancellation_and_deadline_stop_waiting_for_slow_server() {
        for cancel in [true, false] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = endpoint(listener.local_addr().unwrap());
            let (accepted_tx, accepted_rx) = mpsc::channel();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                read_request(&mut stream);
                accepted_tx.send(()).unwrap();
                thread::sleep(Duration::from_millis(300));
                let _ = stream.write_all(&response("200 OK", br#"{"choices":[]}"#));
            });
            let cancellation = ProcessCancellation::default();
            let worker_cancellation = cancellation.clone();
            let worker = thread::spawn(move || {
                let limits = LocalChatLimits {
                    deadline: if cancel {
                        Duration::from_secs(2)
                    } else {
                        Duration::from_millis(80)
                    },
                    ..Default::default()
                };
                endpoint.complete(&json!({"messages": []}), limits, &worker_cancellation)
            });
            accepted_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            if cancel {
                cancellation.cancel();
            }
            let result = worker.join().unwrap();
            assert!(if cancel {
                matches!(result, Err(LocalChatError::Cancelled))
            } else {
                matches!(result, Err(LocalChatError::DeadlineExceeded))
            });
            server.join().unwrap();
        }
    }
}

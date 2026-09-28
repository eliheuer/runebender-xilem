// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The stdio MCP host for native and live editor tools.

use super::agent_commands::dispatch_call;
use super::font_commands::proof_content;
use super::*;
use runebender::automation::tool_contracts::{self, ToolSurface};

/// The MCP server: JSON-RPC 2.0 over stdio, one message per line,
/// the way the protocol's stdio transport works. Handles what a
/// client needs to list and call tools; everything else answers
/// "method not found". Operation descriptors are shared with CLI discovery;
/// the live host adds connection selection and explicit proof export.
/// Guarded live mutations remain owned by the editor.
pub(super) fn mcp_serve(
    font: Option<&Path>,
    session: Option<&Path>,
    live: bool,
    tool: Option<&Path>,
) -> i32 {
    use std::io::BufRead as _;
    if font.is_some_and(|font| !font.exists()) {
        eprintln!("font not found");
        return exit::USAGE;
    }
    let live_mode = live || session.is_some();
    let connected = std::sync::Arc::new(McpSession {
        #[cfg(unix)]
        endpoint: std::sync::Mutex::new(session.map(Path::to_path_buf)),
        protocol: std::sync::Mutex::new(McpProtocol::V20251125),
    });
    let output = std::sync::Arc::new(std::sync::Mutex::new(std::io::stdout()));
    let inflight = std::sync::Arc::new(std::sync::Mutex::new(std::collections::BTreeMap::<
        String,
        std::sync::Arc<McpInFlight>,
    >::new()));
    let (request_sender, request_worker) = if live_mode {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<McpWork>(MAX_MCP_REQUESTS);
        let worker_font = font.map(Path::to_path_buf);
        let worker_tool = tool.map(Path::to_path_buf);
        let worker_connected = connected.clone();
        let worker_output = output.clone();
        let worker_inflight = inflight.clone();
        let worker = std::thread::spawn(move || {
            while let Ok(work) = receiver.recv() {
                let cancelled = work
                    .state
                    .cancelled
                    .load(std::sync::atomic::Ordering::Acquire);
                let semantic_cancelled = work
                    .state
                    .semantic_cancelled
                    .load(std::sync::atomic::Ordering::Acquire);
                if cancelled && !semantic_cancelled {
                    worker_inflight
                        .lock()
                        .expect("MCP inflight mutex poisoned")
                        .remove(&work.key);
                    continue;
                }
                let response = mcp_response(
                    work.id,
                    &work.method,
                    work.params,
                    worker_font.as_deref(),
                    true,
                    worker_tool.as_deref(),
                    &worker_connected,
                );
                if !work
                    .state
                    .cancelled
                    .load(std::sync::atomic::Ordering::Acquire)
                {
                    write_mcp(&worker_output, response);
                }
                worker_inflight
                    .lock()
                    .expect("MCP inflight mutex poisoned")
                    .remove(&work.key);
            }
        });
        (Some(sender), Some(worker))
    } else {
        (None, None)
    };
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    const MAX_MCP_FRAME: u64 = 8 * 1024 * 1024;
    loop {
        let mut line = String::new();
        match std::io::Read::take(&mut input, MAX_MCP_FRAME + 1).read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        if line.len() as u64 > MAX_MCP_FRAME || !line.ends_with('\n') {
            write_mcp(
                &output,
                json!({"jsonrpc":"2.0","id":null,"error":{
                    "code":-32600,"message":"invalid or oversized MCP frame (limit 8 MiB)"
                }}),
            );
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let message: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                write_mcp(
                    &output,
                    json!({ "jsonrpc": "2.0", "id": null,
                    "error": { "code": -32700, "message": format!("parse error: {e}") } }),
                );
                continue;
            }
        };
        let id = message.get("id").cloned();
        let method = message.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(json!({}));
        if id.is_none() {
            if live_mode && method == "notifications/cancelled" {
                cancel_mcp_request(&params, &inflight, &connected);
            }
            continue;
        }
        let id = id.expect("checked above");
        if !live_mode {
            write_mcp(
                &output,
                mcp_response(id, method, params, font, false, tool, &connected),
            );
            continue;
        }

        // Semantic cancellation must bypass the ordered request worker so it can interrupt the
        // apply currently waiting on the live endpoint.
        if method == "tools/call" && params["name"] == "agent_cancel" {
            if inflight
                .lock()
                .expect("MCP inflight mutex poisoned")
                .contains_key(&mcp_request_key(&id))
            {
                write_mcp(
                    &output,
                    json!({"jsonrpc":"2.0","id":id,"error":{
                        "code":-32600,"message":"duplicate in-progress request id"
                    }}),
                );
                continue;
            }
            write_mcp(
                &output,
                mcp_response(id, method, params, font, true, tool, &connected),
            );
            continue;
        }
        let key = mcp_request_key(&id);
        if inflight
            .lock()
            .expect("MCP inflight mutex poisoned")
            .contains_key(&key)
        {
            write_mcp(
                &output,
                json!({"jsonrpc":"2.0","id":id,"error":{
                    "code":-32600,"message":"duplicate in-progress request id"
                }}),
            );
            continue;
        }
        let cancellation = mcp_cancellation_arguments(method, &params);
        let apply_arguments = mcp_apply_arguments(method, &params).cloned();
        let reservation = apply_arguments
            .as_ref()
            .map(|arguments| live_client_call("agent_reserve", arguments, &connected))
            .unwrap_or_else(|| json!({"ok":false}));
        let semantic_reserved = reservation["ok"] == true;
        let reservation_owned = reservation["reservation_status"] == "new";
        let state = std::sync::Arc::new(McpInFlight {
            cancelled: std::sync::atomic::AtomicBool::new(false),
            semantic_cancelled: std::sync::atomic::AtomicBool::new(false),
            semantic_reserved,
            cancellation,
        });
        {
            let mut requests = inflight.lock().expect("MCP inflight mutex poisoned");
            requests.insert(key.clone(), state.clone());
        }
        let work = McpWork {
            id: id.clone(),
            method: method.to_owned(),
            params,
            key: key.clone(),
            state,
        };
        if request_sender
            .as_ref()
            .expect("live request worker")
            .try_send(work)
            .is_err()
        {
            inflight
                .lock()
                .expect("MCP inflight mutex poisoned")
                .remove(&key);
            if reservation_owned && let Some(arguments) = apply_arguments.as_ref() {
                let _ = live_client_call("agent_release", arguments, &connected);
            }
            write_mcp(
                &output,
                json!({"jsonrpc":"2.0","id":id,"error":{
                    "code":-32000,"message":"live MCP request capacity exhausted"
                }}),
            );
        }
    }
    drop(request_sender);
    if let Some(worker) = request_worker {
        let _ = worker.join();
    }
    exit::OK
}

const MAX_MCP_REQUESTS: usize = 32;

/// Negotiated framing features; newer fields are never sent to older clients.
#[derive(Clone, Copy)]
enum McpProtocol {
    V20241105,
    V20250326,
    V20250618,
    V20251125,
}

impl McpProtocol {
    fn negotiate(requested: Option<&str>) -> Self {
        match requested {
            Some("2024-11-05") => Self::V20241105,
            Some("2025-03-26") => Self::V20250326,
            Some("2025-06-18") => Self::V20250618,
            _ => Self::V20251125,
        }
    }

    fn version(self) -> &'static str {
        match self {
            Self::V20241105 => "2024-11-05",
            Self::V20250326 => "2025-03-26",
            Self::V20250618 => "2025-06-18",
            Self::V20251125 => "2025-11-25",
        }
    }

    fn annotations(self) -> bool {
        !matches!(self, Self::V20241105)
    }

    fn structured_results(self) -> bool {
        matches!(self, Self::V20250618 | Self::V20251125)
    }
}

struct McpSession {
    #[cfg(unix)]
    endpoint: std::sync::Mutex<Option<PathBuf>>,
    protocol: std::sync::Mutex<McpProtocol>,
}

impl McpSession {
    fn protocol(&self) -> McpProtocol {
        *self.protocol.lock().expect("MCP protocol mutex poisoned")
    }
}

struct McpInFlight {
    cancelled: std::sync::atomic::AtomicBool,
    semantic_cancelled: std::sync::atomic::AtomicBool,
    semantic_reserved: bool,
    cancellation: Option<serde_json::Value>,
}

struct McpWork {
    id: serde_json::Value,
    method: String,
    params: serde_json::Value,
    key: String,
    state: std::sync::Arc<McpInFlight>,
}

fn mcp_response(
    id: serde_json::Value,
    method: &str,
    params: serde_json::Value,
    font: Option<&Path>,
    live_mode: bool,
    tool: Option<&Path>,
    connected: &std::sync::Arc<McpSession>,
) -> serde_json::Value {
    let result = match method {
        "initialize" => {
            let protocol = McpProtocol::negotiate(
                params
                    .get("protocolVersion")
                    .and_then(serde_json::Value::as_str),
            );
            *connected
                .protocol
                .lock()
                .expect("MCP protocol mutex poisoned") = protocol;
            Ok(json!({
                "protocolVersion": protocol.version(),
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": "runebender",
                    "version": env!("CARGO_PKG_VERSION"),
                },
                "instructions": if live_mode {
                    format!("{}\n\n{}", runebender::automation::live::INSTRUCTIONS, runebender::automation::script_recipe::AUTHORING_INSTRUCTIONS)
                } else { mcp_instructions(font.expect("font or session")) },
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => {
            let protocol = connected.protocol();
            let surface = if live_mode {
                ToolSurface::Live
            } else {
                ToolSurface::Disk
            };
            let tools = mcp_tools(live_mode)
                .into_iter()
                .map(|tool| {
                    let descriptor = tool_contracts::describe(tool, surface);
                    let mut result = json!({
                        "name": descriptor.tool.name,
                        "description": descriptor.tool.description,
                        "inputSchema": descriptor.tool.parameters,
                    });
                    if protocol.annotations() {
                        result["annotations"] = descriptor.annotations();
                    }
                    if protocol.structured_results()
                        && let Some(schema) = descriptor.output_schema
                    {
                        result["outputSchema"] = schema;
                    }
                    result
                })
                .collect::<Vec<_>>();
            Ok(json!({"tools": tools}))
        }
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let value = if live_mode {
                live_client_call(name, &args, connected)
            } else {
                dispatch_call(name, font, None, &args, tool)
            };
            let ok = value.get("ok").and_then(|v| v.as_bool()).unwrap_or(true);
            let proof = proof_content(value);
            let mut result = json!({
                "content": proof.content,
                "isError": !ok,
            });
            if connected.protocol().structured_results() && proof.metadata.is_object() {
                result["structuredContent"] = proof.metadata;
            }
            Ok(result)
        }
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        other => Err(json!({
            "code": -32601,
            "message": format!("method not found: {other}")
        })),
    };
    match result {
        Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
        Err(error) => json!({"jsonrpc":"2.0","id":id,"error":error}),
    }
}

fn write_mcp(output: &std::sync::Arc<std::sync::Mutex<std::io::Stdout>>, value: serde_json::Value) {
    use std::io::Write as _;
    let mut output = output.lock().expect("MCP output mutex poisoned");
    let _ = writeln!(output, "{value}");
    let _ = output.flush();
}

fn mcp_request_key(id: &serde_json::Value) -> String {
    id.to_string()
}

fn mcp_cancellation_arguments(
    method: &str,
    params: &serde_json::Value,
) -> Option<serde_json::Value> {
    if method != "tools/call" || params["name"] != "agent_apply" {
        return None;
    }
    let arguments = params.get("arguments")?;
    Some(json!({
        "expected_document_epoch":arguments.get("expected_document_epoch")?.as_str()?,
        "actor":arguments.get("actor")?.as_str()?,
        "operation_key":arguments.get("operation_key")?.as_str()?,
    }))
}

fn mcp_apply_arguments<'a>(
    method: &str,
    params: &'a serde_json::Value,
) -> Option<&'a serde_json::Value> {
    (method == "tools/call" && params["name"] == "agent_apply")
        .then(|| params.get("arguments"))
        .flatten()
}

fn cancel_mcp_request(
    params: &serde_json::Value,
    inflight: &std::sync::Arc<
        std::sync::Mutex<std::collections::BTreeMap<String, std::sync::Arc<McpInFlight>>>,
    >,
    connected: &std::sync::Arc<McpSession>,
) {
    let Some(request_id) = params.get("requestId") else {
        return;
    };
    let state = inflight
        .lock()
        .expect("MCP inflight mutex poisoned")
        .get(&mcp_request_key(request_id))
        .cloned();
    let Some(state) = state else {
        return;
    };
    if state.semantic_reserved
        && let Some(arguments) = &state.cancellation
    {
        let result = live_client_call("agent_cancel", arguments, connected);
        if matches!(
            result
                .get("cancellation_status")
                .and_then(serde_json::Value::as_str),
            Some("prevented" | "already_prevented")
        ) {
            state
                .semantic_cancelled
                .store(true, std::sync::atomic::Ordering::Release);
        }
    }
    state
        .cancelled
        .store(true, std::sync::atomic::Ordering::Release);
}

/// Live tools include explicit discovery and connection, so clients need one stable config.
fn mcp_tools(live: bool) -> Vec<agent::Tool> {
    if !live {
        return agent::tools();
    }
    let mut tools = runebender::automation::live::tools();
    if let Some(proof) = tools.iter_mut().find(|tool| tool.name == "proof") {
        proof.description = "Return a PNG proof image and metrics from the live unsaved source. Supply 1 to 256 explicit glyph names; use layer to view a proposal. Use small groups for legible images. Images are required for visual judgment; report if your client does not deliver them.".into();
    }
    tools.extend(tool_contracts::live_host_tools());
    tools
}

/// Changes only this MCP client's chosen endpoint; all font work stays on the editor thread.
fn live_client_call(
    name: &str,
    args: &serde_json::Value,
    connected: &std::sync::Arc<McpSession>,
) -> serde_json::Value {
    #[cfg(not(unix))]
    {
        let _ = (name, args, connected);
        json!({"ok": false, "error":"live editors require Unix"})
    }
    #[cfg(unix)]
    {
        use runebender::automation::live_socket;
        if name == "export_proof" {
            let run = (|| -> Result<serde_json::Value, String> {
                use std::io::Write as _;
                let path = args
                    .get("output")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .ok_or("output path required")?;
                let pdf = match args.get("format").and_then(|v| v.as_str()) {
                    Some("pdf") => true,
                    Some("png") => false,
                    _ => return Err("format must be png or pdf".into()),
                };
                if args.get("text").is_some() == args.get("glyphs").is_some() {
                    return Err("supply exactly one of text or glyphs".into());
                }
                if args.get("text").is_some() && args.get("layer").is_some() {
                    return Err("text proofs use branch foreground; install the proposal into the branch first".into());
                }
                let value = live_client_call(
                    if args.get("text").is_some() {
                        "specimen"
                    } else {
                        "proof"
                    },
                    args,
                    connected,
                );
                if value["ok"] != true {
                    return Ok(value);
                }
                let scene = value.get("scene").ok_or("proof has no scene")?;
                let bytes = runebender::formats::designbot::render(scene, pdf)?;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(|e| e.to_string())?;
                if let Err(e) = file.write_all(&bytes) {
                    drop(file);
                    let _ = std::fs::remove_file(path);
                    return Err(e.to_string());
                }
                Ok(
                    json!({"ok":true,"output":path,"bytes":bytes.len(),"source_id":value["source_id"],"document_epoch":value["document_epoch"],"document_revision":value["document_revision"],"branch":value["branch"]}),
                )
            })();
            return run.unwrap_or_else(|error| json!({"ok":false,"error":error}));
        }
        if name == "editor_sessions" {
            let selected = connected
                .endpoint
                .lock()
                .expect("MCP connection mutex poisoned")
                .clone();
            return json!({"ok":true, "sessions":live_socket::sessions(), "connected":selected});
        }
        if name == "editor_connect" {
            let Some(path) = args
                .get("session")
                .and_then(|v| v.as_str())
                .map(PathBuf::from)
            else {
                return json!({"ok":false, "error":"session path is required"});
            };
            if !live_socket::sessions().contains(&path) {
                return json!({"ok":false, "error":"choose an endpoint returned by editor_sessions"});
            }
            let value = dispatch_call("project_info", None, Some(&path), &json!({}), None);
            if value["ok"] == true {
                *connected
                    .endpoint
                    .lock()
                    .expect("MCP connection mutex poisoned") = Some(path);
            }
            return value;
        }
        let selected = connected
            .endpoint
            .lock()
            .expect("MCP connection mutex poisoned")
            .clone();
        match selected {
            Some(path) => dispatch_call(name, None, Some(&path), args, None),
            None => json!({"ok":false, "error":"call editor_sessions, then editor_connect first"}),
        }
    }
}

/// What a client is told at `initialize`: the rule of the tool list.
fn mcp_instructions(font: &Path) -> String {
    format!(
        "Runebender font editor, working on {}. The tools read the font, render \
         proofs, run local models, and propose changes. No tool edits the font: a \
         proposal is a UFO layer the person installs or discards in the editor, \
         one glyph at a time. Call project_info first and choose an explicit master; read a glyph before you \
         talk about its shape.",
        font.display()
    )
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A real headless Workspace process, agent socket, and ordinary application undo/redo.

#![cfg(unix)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use runebender::document::{agent::ToolCall, live_socket};
use serde_json::{Value, json};

struct Fixture {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    next_id: u64,
}

impl Fixture {
    fn start() -> Self {
        Self::spawn(Command::new(env!("CARGO_BIN_EXE_runebender")).args([
            "agent",
            "fixture",
            "--duration-seconds",
            "15",
        ]))
    }

    fn spawn(command: &mut Command) -> Self {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input,
            output,
            next_id: 0,
        }
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        assert_ne!(
            self.output.read_line(&mut line).unwrap(),
            0,
            "fixture closed stdout"
        );
        serde_json::from_str(&line).unwrap()
    }

    fn control(&mut self, action: &str) -> Value {
        writeln!(self.input, "{}", json!({"action":action})).unwrap();
        self.input.flush().unwrap();
        self.read()
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        writeln!(
            self.input,
            "{}",
            json!({"jsonrpc":"2.0","id":self.next_id,"method":method,"params":params})
        )
        .unwrap();
        self.input.flush().unwrap();
        let response = self.read();
        assert_eq!(
            response["id"], self.next_id,
            "MCP reply must match its request"
        );
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let result = self.rpc("tools/call", json!({"name":name,"arguments":arguments}));
        assert_eq!(result["isError"], false, "{result}");
        serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn application_fixture_refreshes_and_undoes_an_agent_edit() {
    let mut fixture = Fixture::start();
    let ready = fixture.read();
    assert_eq!(ready["ok"], true, "{ready}");
    let path = std::path::PathBuf::from(ready["session"].as_str().unwrap());
    let call = |name: &str, arguments| {
        live_socket::call(
            &path,
            &ToolCall {
                name: name.into(),
                arguments,
            },
        )
        .unwrap()
    };
    let context = call("editor_context", json!({}));
    assert_eq!(context["context"]["glyph"], "A");
    let epoch = context["document_epoch"].clone();
    let read = call(
        "read_glyph",
        json!({"source":0,"glyph":"A","expected_document_epoch":epoch}),
    );
    assert_eq!(read["advance"], 412.0);
    let check_state = |state: Value, width| {
        assert_eq!(state["ok"], true, "{state}");
        assert_eq!(state["canonical_advance"], width);
        assert_eq!(state["cache_advance"], width);
        assert_eq!(state["session_advance"], width);
        assert_eq!(state["source_exists"], false);
    };
    check_state(fixture.control("state"), 412.0);
    let proposal = call(
        "propose_edits",
        json!({
            "source":0,"task":"fixture-width","reason":"application process integration",
            "expected_document_epoch":epoch,
            "edits":[{"glyph":"A","expected_revision":read["revision"],"operations":[{"op":"set_width","width":430.0}]}]
        }),
    );
    assert_eq!(proposal["ok"], true, "{proposal}");
    let installed = call(
        "proposal_install",
        json!({
            "source":0,"task":"fixture-width","keep_structure":true,
            "authorization":"user-approved","expected_document_epoch":epoch
        }),
    );
    assert_eq!(installed["ok"], true, "{installed}");
    check_state(fixture.control("state"), 430.0);
    check_state(fixture.control("undo"), 412.0);
    check_state(fixture.control("redo"), 430.0);
    assert_eq!(fixture.control("shutdown")["stopped"], true);
    assert!(fixture.child.wait().unwrap().success());
    assert!(!path.exists(), "fixture exit removes its endpoint");
}

#[test]
fn mcp_receipt_tools_reconcile_retry_and_real_application_undo() {
    let mut fixture = Fixture::start();
    let ready = fixture.read();
    let endpoint = ready["session"].as_str().unwrap();
    let mut mcp = Fixture::spawn(Command::new(env!("CARGO_BIN_EXE_runebender")).args([
        "mcp",
        "--session",
        endpoint,
    ]));
    let initialized = mcp.rpc("initialize", json!({"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"receipt-conformance","version":"1"}}));
    assert_eq!(initialized["protocolVersion"], "2025-11-25");
    let tools = mcp.rpc("tools/list", json!({}));
    let listed = tools["tools"].as_array().unwrap();
    let apply = listed
        .iter()
        .find(|tool| tool["name"] == "agent_apply")
        .unwrap();
    assert_eq!(apply["annotations"]["readOnlyHint"], false);
    assert_eq!(apply["inputSchema"]["additionalProperties"], false);
    assert!(
        apply["inputSchema"]["required"]
            .as_array()
            .unwrap()
            .contains(&json!("expected_document_epoch"))
    );
    let receipt_tool = listed
        .iter()
        .find(|tool| tool["name"] == "agent_receipt")
        .unwrap();
    assert_eq!(receipt_tool["annotations"]["readOnlyHint"], true);
    let context = mcp.tool("editor_context", json!({}));
    let epoch = context["document_epoch"].clone();
    let read = mcp.tool(
        "read_glyph",
        json!({"source":0,"glyph":"A","expected_document_epoch":epoch}),
    );
    let payload = json!({"expected_document_epoch":epoch,"actor":"mcp-test","operation_key":"one-width-edit","authorization":"user-approved","source":0,"history_name":"MCP width edit","edits":[{"target":{"glyph":"A","glyph_id":read["glyph_id"],"layer":read["layer"],"expected_revision":read["revision"]},"operations":[{"op":"set_width","width":430.0}]}]});
    let applied = mcp.tool("agent_apply", payload.clone());
    assert_eq!(applied["root_changed"], true);
    let state = fixture.control("state");
    for field in ["canonical_advance", "cache_advance", "session_advance"] {
        assert_eq!(state[field], 430.0);
    }
    let repeated = mcp.tool("agent_apply", payload);
    assert_eq!(repeated["replayed"], true);
    assert_eq!(repeated["receipt"], applied["receipt"]);
    assert_eq!(repeated["document_revision"], applied["document_revision"]);
    assert_eq!(fixture.control("undo")["canonical_advance"], 412.0);
    let lookup = json!({"expected_document_epoch":epoch,"actor":"mcp-test","operation_key":"one-width-edit"});
    let receipt = mcp.tool("agent_receipt", lookup.clone());
    assert_eq!(receipt["receipt"], applied["receipt"]);
    assert_eq!(receipt["history_state"], "undone");
    let mut replay = lookup;
    replay["direction"] = json!("redo");
    replay["authorization"] = json!("user-approved");
    assert_eq!(
        mcp.tool("agent_history", replay)["history_state"],
        "applied"
    );
    let state = fixture.control("state");
    assert_eq!(state["session_advance"], 430.0);
    assert_eq!(state["source_exists"], false);
    assert_eq!(fixture.control("shutdown")["stopped"], true);
    assert!(fixture.child.wait().unwrap().success());
}

#[test]
fn file_backed_host_edits_and_undoes_without_rewriting_source() {
    use runebender::document::project::Project;
    use std::path::Path;

    fn files(path: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
        let mut result = Vec::new();
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                result.extend(files(&path));
            } else {
                result.push((path.clone(), std::fs::read(path).unwrap()));
            }
        }
        result.sort_by(|a, b| a.0.cmp(&b.0));
        result
    }

    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "runebender-file-host-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    let font_path = root.join("Trial.ufo");
    let mut project = Project::new_font(font_path.clone());
    project.add_document_glyph("trial", 400.0, None).unwrap();
    project
        .encode_ufo_source(project.source_id(0).unwrap())
        .unwrap()
        .save(&font_path)
        .unwrap();
    let before = files(&font_path);
    let mut host = Fixture::spawn(
        Command::new(env!("CARGO_BIN_EXE_runebender"))
            .args(["agent", "serve", "--font"])
            .arg(&font_path)
            .args(["--glyph", "trial", "--duration-seconds", "15"]),
    );
    let ready = host.read();
    assert_eq!(ready["ok"], true, "{ready}");
    assert_eq!(ready["fixture"], false);
    assert_eq!(ready["font_path"], json!(font_path.canonicalize().unwrap()));
    let endpoint = std::path::PathBuf::from(ready["session"].as_str().unwrap());
    let call = |name: &str, arguments| {
        live_socket::call(
            &endpoint,
            &ToolCall {
                name: name.into(),
                arguments,
            },
        )
        .unwrap()
    };
    let epoch = &ready["document_epoch"];
    let read = call(
        "read_glyph",
        json!({"source":0,"glyph":"trial","expected_document_epoch":epoch}),
    );
    let applied = call(
        "agent_apply",
        json!({"expected_document_epoch":epoch,"actor":"file-host-test","operation_key":"width-001","authorization":"user-approved","source":0,"history_name":"Trial width","edits":[{"target":{"glyph":"trial","glyph_id":read["glyph_id"],"layer":read["layer"],"expected_revision":read["revision"]},"operations":[{"op":"set_width","width":450.0}]}]}),
    );
    assert_eq!(applied["ok"], true, "{applied}");
    assert_eq!(applied["saved"], false);
    for (action, width) in [("state", 450.0), ("undo", 400.0), ("redo", 450.0)] {
        let state = host.control(action);
        for field in ["canonical_advance", "cache_advance", "session_advance"] {
            assert_eq!(state[field], width, "{state}");
        }
    }
    assert_eq!(
        host.control("save")["ok"],
        false,
        "host exposes no save control"
    );
    assert_eq!(host.control("shutdown")["stopped"], true);
    assert!(host.child.wait().unwrap().success());
    assert!(!endpoint.exists(), "host removes its endpoint on shutdown");
    assert_eq!(
        files(&font_path),
        before,
        "all source bytes remain unchanged"
    );
    std::fs::remove_dir_all(root).unwrap();
}

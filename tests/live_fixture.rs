// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A real headless Workspace process, agent socket, and ordinary application undo/redo.

#![cfg(unix)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use runebender::automation::{agent::ToolCall, live_socket};
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

    fn result(&mut self, name: &str, arguments: Value) -> (bool, Value) {
        let result = self.rpc("tools/call", json!({"name":name,"arguments":arguments}));
        let metadata: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(
            result["structuredContent"], metadata,
            "structured tool metadata must match the compatibility text"
        );
        (result["isError"].as_bool().unwrap(), metadata)
    }

    fn tool(&mut self, name: &str, arguments: Value) -> Value {
        let (is_error, metadata) = self.result(name, arguments);
        assert!(!is_error, "{metadata}");
        metadata
    }

    fn rejected_tool(&mut self, name: &str, arguments: Value) -> Value {
        let (is_error, metadata) = self.result(name, arguments);
        assert!(is_error, "{metadata}");
        metadata
    }

    fn assert_contracts(&mut self, names: &[&str]) {
        use runebender::automation::{live, tool_contracts};
        let listed = self.rpc("tools/list", json!({}));
        let tools = live::tools();
        for name in names {
            let tool = tools
                .iter()
                .find(|tool| tool.name == *name)
                .expect("published live tool");
            let descriptor =
                tool_contracts::describe(tool.clone(), tool_contracts::ToolSurface::Live);
            let published = listed["tools"]
                .as_array()
                .unwrap()
                .iter()
                .find(|tool| tool["name"] == descriptor.tool.name)
                .unwrap();
            assert_eq!(
                published["outputSchema"],
                descriptor.output_schema.unwrap(),
                "MCP must publish the shared result contract for {name}"
            );
        }
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
    use runebender::automation::agent_edit::results::{
        AgentApplyResponse, AgentCancelResponse, AgentHistoryResponse, AgentReceiptResponse,
    };

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
    mcp.assert_contracts(&[
        "agent_apply",
        "agent_receipt",
        "agent_history",
        "agent_cancel",
    ]);
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
    let cancel_tool = listed
        .iter()
        .find(|tool| tool["name"] == "agent_cancel")
        .unwrap();
    assert_eq!(cancel_tool["annotations"]["readOnlyHint"], false);
    assert_eq!(cancel_tool["inputSchema"]["additionalProperties"], false);
    let context = mcp.tool("editor_context", json!({}));
    assert_eq!(context["capabilities"]["edit_cancellation"], true);
    assert_eq!(
        context["capabilities"]["cancellation_identity"],
        "document_epoch+actor+operation_key"
    );
    let epoch = context["document_epoch"].clone();
    let read = mcp.tool(
        "read_glyph",
        json!({"source":0,"glyph":"A","expected_document_epoch":epoch}),
    );
    let payload = json!({"expected_document_epoch":epoch,"actor":"mcp-test","operation_key":"one-width-edit","authorization":"user-approved","source":0,"history_name":"MCP width edit","edits":[{"target":{"glyph":"A","glyph_id":read["glyph_id"],"layer":read["layer"],"expected_revision":read["revision"]},"operations":[{"op":"set_width","width":430.0}]}]});
    let applied = mcp.tool("agent_apply", payload.clone());
    serde_json::from_value::<AgentApplyResponse>(applied.clone()).unwrap();
    assert_eq!(applied["root_changed"], true);
    let mut stale = payload.clone();
    stale["operation_key"] = json!("rejected-stale-edit");
    let rejected = mcp.rejected_tool("agent_apply", stale);
    serde_json::from_value::<AgentApplyResponse>(rejected.clone()).unwrap();
    assert_eq!(rejected["receipt"]["outcome"]["status"], "rejected");
    assert!(
        rejected.get("error").is_none(),
        "rejection details belong to the receipt"
    );
    let rejected_receipt = mcp.tool(
        "agent_receipt",
        json!({
            "expected_document_epoch":epoch,"actor":"mcp-test","operation_key":"rejected-stale-edit"
        }),
    );
    serde_json::from_value::<AgentReceiptResponse>(rejected_receipt.clone()).unwrap();
    assert_eq!(rejected_receipt["receipt"], rejected["receipt"]);
    assert_eq!(rejected_receipt["ok"], true);
    for (key, expected) in [
        ("one-width-edit", "committed"),
        ("not-submitted", "unknown"),
    ] {
        let cancellation = mcp.rejected_tool(
            "agent_cancel",
            json!({
                "expected_document_epoch":epoch,"actor":"mcp-test","operation_key":key
            }),
        );
        serde_json::from_value::<AgentCancelResponse>(cancellation.clone()).unwrap();
        assert_eq!(cancellation["cancellation_status"], expected);
        assert!(
            cancellation.get("error").is_none(),
            "cancellation has its own outcome"
        );
    }
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
    serde_json::from_value::<AgentReceiptResponse>(receipt.clone()).unwrap();
    assert_eq!(receipt["receipt"], applied["receipt"]);
    assert_eq!(receipt["history_state"], "undone");
    let mut replay = lookup;
    replay["direction"] = json!("redo");
    replay["authorization"] = json!("user-approved");
    let history = mcp.tool("agent_history", replay);
    serde_json::from_value::<AgentHistoryResponse>(history.clone()).unwrap();
    assert_eq!(history["history_state"], "applied");
    let state = fixture.control("state");
    assert_eq!(state["session_advance"], 430.0);
    assert_eq!(state["source_exists"], false);
    assert_eq!(fixture.control("shutdown")["stopped"], true);
    assert!(fixture.child.wait().unwrap().success());
}

#[test]
fn file_backed_host_edits_and_undoes_without_rewriting_source() {
    use runebender::font::project::Project;
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

#[test]
fn compiled_proof_mcp_delivers_original_png_after_live_edit() {
    use base64::Engine as _;
    use runebender::automation::agent_proof::{
        self, ProofCancellation, ProofOutcomeResult, ProofReleaseResult, ProofStartResult,
        ProofStatusResult,
    };
    use runebender::automation::tool_contracts::{self, ToolSurface};
    use std::time::{Duration, Instant};

    let mut fixture = Fixture::start();
    let ready = fixture.read();
    assert_eq!(ready["ok"], true, "{ready}");
    let mut mcp = Fixture::spawn(Command::new(env!("CARGO_BIN_EXE_runebender")).args([
        "mcp",
        "--session",
        ready["session"].as_str().unwrap(),
    ]));
    mcp.rpc("initialize", json!({"protocolVersion":"2025-11-25"}));
    let listed = mcp.rpc("tools/list", json!({}));
    for tool in agent_proof::tools() {
        let descriptor = tool_contracts::describe(tool, ToolSurface::Live);
        let listed_tool = listed["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|tool| tool["name"] == descriptor.tool.name)
            .unwrap();
        assert_eq!(
            listed_tool["outputSchema"],
            descriptor.output_schema.unwrap(),
            "MCP must publish the shared proof result contract"
        );
    }
    let epoch = &ready["document_epoch"];
    let read = mcp.tool(
        "read_glyph",
        json!({"source":0,"glyph":"A","expected_document_epoch":epoch}),
    );
    let revision = read["document_revision"].clone();
    let request = json!({"expected_document_epoch":epoch,"expected_document_revision":revision,
        "operation_key":"before-spacing-proof","recipe":{"text":"A","normalized_location":[],
        "right_to_left":false,"features":[],"script":null,"language":null}});
    let started = mcp.tool("proof_start", request.clone());
    let start_receipt: ProofStartResult = serde_json::from_value(started.clone()).unwrap();
    assert!(start_receipt.ok);
    assert!(!start_receipt.replayed);
    assert!(!start_receipt.root_changed);
    assert_eq!(start_receipt.captured_document_revision, revision);
    let applied = mcp.tool("agent_apply", json!({"expected_document_epoch":epoch,"actor":"proof-test",
        "operation_key":"width-during-proof","authorization":"user-approved","source":0,
        "history_name":"Edit after capture","edits":[{"target":{"glyph":"A","glyph_id":read["glyph_id"],
        "layer":read["layer"],"expected_revision":read["revision"]},"operations":[{"op":"set_width","width":430.0}]}]}));
    assert_eq!(applied["root_changed"], true);
    let retry = mcp.tool("proof_start", request.clone());
    assert_eq!(retry["replayed"], true);
    assert_eq!(retry["proof_id"], started["proof_id"]);
    assert_eq!(retry["captured_document_revision"], revision);
    let args = json!({"expected_document_epoch":epoch,"proof_id":started["proof_id"],"include_image":true});
    let deadline = Instant::now() + Duration::from_secs(10);
    let (metadata, png_text) = loop {
        let response = mcp.rpc(
            "tools/call",
            json!({"name":"proof_status","arguments":args}),
        );
        assert_eq!(response["isError"], false, "{response}");
        let metadata: Value =
            serde_json::from_str(response["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(response["structuredContent"], metadata);
        let status: ProofStatusResult = serde_json::from_value(metadata.clone()).unwrap();
        assert!(status.ok);
        assert_eq!(status.proof_id, start_receipt.proof_id);
        assert!(!status.root_changed);
        if let ProofOutcomeResult::Completed(ref proof) = status.outcome {
            assert!(
                proof.png_base64.is_none(),
                "MCP extracts images from the typed metadata"
            );
        }
        assert_eq!(metadata["captured_document_revision"], revision);
        assert_eq!(metadata["document_revision"], applied["document_revision"]);
        assert_eq!(metadata["current"], false);
        assert_eq!(metadata["stale"], true);
        assert_ne!(metadata["status"], "failed", "{metadata}");
        if metadata["status"] == "completed" {
            assert!(
                metadata.get("png_base64").is_none(),
                "image must not be repeated inside text"
            );
            assert_eq!(response["content"][1]["type"], "image");
            assert_eq!(response["content"][1]["mimeType"], "image/png");
            break (
                metadata,
                response["content"][1]["data"].as_str().unwrap().to_owned(),
            );
        }
        assert!(
            Instant::now() < deadline,
            "proof did not finish: {metadata}"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let png = base64::engine::general_purpose::STANDARD
        .decode(&png_text)
        .unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let hash = metadata["font_sha256"]
        .as_str()
        .unwrap()
        .strip_prefix("sha256:")
        .unwrap();
    assert_eq!(hash.len(), 64);
    assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert_eq!(metadata["recipe"], request["recipe"]);
    assert_eq!(metadata["glyphs"][0]["glyph_name"], "A");
    assert_eq!(metadata["glyphs"][0]["x_advance"], 412.0);
    let wire = live_socket::call(
        std::path::Path::new(ready["session"].as_str().unwrap()),
        &ToolCall {
            name: "proof_status".into(),
            arguments: args,
        },
    )
    .unwrap();
    assert_eq!(
        wire["png_base64"], png_text,
        "MCP forwards the worker PNG without rendering again"
    );
    assert_eq!(wire["font_sha256"], metadata["font_sha256"]);
    let wire_status: ProofStatusResult = serde_json::from_value(wire).unwrap();
    let ProofOutcomeResult::Completed(proof) = wire_status.outcome else {
        panic!("completed proof must decode as a completed result");
    };
    assert_eq!(proof.png_base64.as_deref(), Some(png_text.as_str()));
    let cancelled = mcp.tool(
        "proof_cancel",
        json!({"expected_document_epoch":epoch,"proof_id":start_receipt.proof_id}),
    );
    let cancelled: ProofStatusResult = serde_json::from_value(cancelled).unwrap();
    assert_eq!(cancelled.cancellation, Some(ProofCancellation::TooLate));
    assert!(matches!(
        cancelled.outcome,
        ProofOutcomeResult::Completed(_)
    ));
    let mut changed_recipe = request.clone();
    changed_recipe["recipe"]["text"] = json!("AA");
    let conflict = mcp.rpc(
        "tools/call",
        json!({"name":"proof_start","arguments":changed_recipe}),
    );
    assert_eq!(conflict["isError"], true);
    let mut stale_request = request.clone();
    stale_request["operation_key"] = json!("stale-new-capture");
    let stale = mcp.rpc(
        "tools/call",
        json!({"name":"proof_start","arguments":stale_request}),
    );
    let error: Value = serde_json::from_str(stale["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(error["error_code"], "stale_revision");
    assert_eq!(stale["structuredContent"], error);
    let mut current_request = request.clone();
    current_request["operation_key"] = json!("after-spacing-proof");
    current_request["expected_document_revision"] = applied["document_revision"].clone();
    let current_start = mcp.tool("proof_start", current_request);
    let current_args =
        json!({"expected_document_epoch":epoch,"proof_id":current_start["proof_id"]});
    loop {
        let current = mcp.tool("proof_status", current_args.clone());
        assert_ne!(current["status"], "failed", "{current}");
        if current["status"] == "completed" {
            assert_eq!(current["current"], true);
            assert_eq!(current["stale"], false);
            assert_eq!(current["glyphs"][0]["x_advance"], 430.0);
            assert_ne!(current["font_sha256"], metadata["font_sha256"]);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let released: ProofReleaseResult =
        serde_json::from_value(mcp.tool("proof_release", current_args)).unwrap();
    assert!(released.ok);
    assert!(released.released);
    assert!(!released.root_changed);
    assert_eq!(released.proof_id, current_start["proof_id"]);
    let handle = json!({"expected_document_epoch":epoch,"proof_id":started["proof_id"]});
    assert_eq!(mcp.tool("proof_release", handle.clone())["released"], true);
    let unknown = mcp.rpc(
        "tools/call",
        json!({"name":"proof_status","arguments":handle}),
    );
    assert_eq!(unknown["isError"], true);
    let state = fixture.control("state");
    assert_eq!(state["canonical_advance"], 430.0);
    assert_eq!(state["source_exists"], false);
    assert_eq!(fixture.control("undo")["canonical_advance"], 412.0);
}

#[test]
fn mcp_graph_results_preserve_captures_images_and_explicit_apply_receipts() {
    use runebender::automation::agent_edit::results::AgentApplyResponse;
    use std::time::{Duration, Instant};

    let mut fixture = Fixture::spawn(Command::new(env!("CARGO_BIN_EXE_runebender")).args([
        "agent",
        "fixture",
        "--duration-seconds",
        "45",
    ]));
    let ready = fixture.read();
    let endpoint = ready["session"].as_str().unwrap();
    let mut mcp = Fixture::spawn(Command::new(env!("CARGO_BIN_EXE_runebender")).args([
        "mcp",
        "--session",
        endpoint,
    ]));
    mcp.rpc("initialize", json!({"protocolVersion":"2025-11-25"}));
    mcp.assert_contracts(&[
        "nodes_discover",
        "nodes_snapshot",
        "nodes_mutate",
        "nodes_run",
        "nodes_status",
        "nodes_cancel",
        "nodes_release",
        "nodes_apply",
        "nodes_image",
    ]);
    let epoch = &ready["document_epoch"];
    let discovered = mcp.tool("nodes_discover", json!({"expected_document_epoch":epoch}));
    let identity = &discovered["identity"];
    let snapshot = mcp.tool(
        "nodes_snapshot",
        json!({
            "expected_document_epoch":epoch,"identity":identity
        }),
    )["snapshot"]
        .clone();
    let read_args = json!({"expected_document_epoch":epoch,"source":0,"glyph":"A"});
    let before = mcp.tool("read_glyph", read_args.clone());
    let code = r#"import json, sys
p=json.load(sys.stdin)
assert p["schema_version"] == 2
edits=[]
for layer in p["layers"]:
    point=layer["contours"][0]["points"][0]
    drawing={"points":[{"x":x,"y":y,"type":"line"} for x,y in [(50,500),(150,500),(150,600),(50,600)]]}
    edits.append({"target":layer["guard"],"operations":[
        {"op":"set_width","width":layer["width"]+100},
        {"op":"set_point","point_id":point["id"],"x":point["x"]+10,"y":point["y"]},
        {"op":"append_contours","contours":[drawing]}]})
json.dump({"schema_version":p["schema_version"],"job_id":p["job_id"],"input_hash":p["input_hash"],"report":"Generate and reshape","reads":[],"edits":edits},sys.stdout)
"#;
    let edits = snapshot["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|node| match node["type"].as_str() {
            Some("live.python") => Some(json!({
                "edit":"set_value","node":node["id"],"field":"code","value":code
            })),
            Some("live.proof") => {
                let mut recipe = node["values"]["recipe"].clone();
                recipe["text"] = json!("AA");
                Some(json!({"edit":"set_value","node":node["id"],"field":"recipe","value":recipe}))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let mutation = json!({"expected_document_epoch":epoch,"request":{
        "guard":{"identity":identity,"revision":snapshot["revision"]},
        "actor":"graph-fixture","operation_key":"configure",
        "mutation":{"mutation":"patch","edits":edits}
    }});
    let mutated = mcp.tool("nodes_mutate", mutation.clone());
    let mutation_retry = mcp.tool("nodes_mutate", mutation);
    assert_eq!(mutation_retry["mutation"]["disposition"], "replayed");
    assert_eq!(
        mutation_retry["mutation"]["receipt"],
        mutated["mutation"]["receipt"]
    );
    let snapshot = &mutated["snapshot"];
    let run_request = json!({"expected_document_epoch":epoch,
        "guard":{"identity":identity,"semantic_revision":snapshot["semantic_revision"],
            "semantic_hash":snapshot["semantic_hash"]},
        "actor":"graph-fixture","operation_key":"compare","source":0,"glyphs":["A"]});
    let started = mcp.tool("nodes_run", run_request.clone());
    let status_args = json!({"expected_document_epoch":epoch,"identity":identity,
        "handle":started["run"]["receipt"]["handle"]});
    let deadline = Instant::now() + Duration::from_secs(30);
    let completed = loop {
        let status = mcp.tool("nodes_status", status_args.clone());
        match status["run"]["status"].as_str() {
            Some("completed") => break status,
            Some("queued" | "running") => {
                assert!(Instant::now() < deadline, "graph did not finish: {status}");
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => panic!("graph failed: {status}"),
        }
    };
    assert_eq!(completed["can_apply"], true);
    assert_eq!(completed["report"], "Generate and reshape");
    assert_eq!(completed["current"], true);
    assert_eq!(fixture.control("state")["canonical_advance"], 412.0);
    assert_eq!(
        mcp.tool("read_glyph", read_args.clone())["contours"],
        before["contours"]
    );
    let retry = mcp.tool("nodes_run", run_request);
    assert_eq!(retry["replayed"], true);
    assert_eq!(retry["run"], started["run"]);

    let mut images = Vec::new();
    for (branch, advance) in [("original", 412.0), ("changed", 512.0)] {
        let mut args = status_args.clone();
        args["branch"] = json!(branch);
        let result = mcp.rpc("tools/call", json!({"name":"nodes_image","arguments":args}));
        assert_eq!(result["isError"], false, "{result}");
        let metadata: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(result["structuredContent"], metadata);
        assert!(
            metadata.get("png_base64").is_none(),
            "MCP transports the image separately"
        );
        assert_eq!(metadata["captured_document_epoch"], *epoch);
        assert_eq!(metadata["glyphs"][0]["x_advance"], advance);
        assert_eq!(result["content"][1]["mimeType"], "image/png");
        let socket = live_socket::call(
            std::path::Path::new(endpoint),
            &ToolCall {
                name: "nodes_image".into(),
                arguments: args,
            },
        )
        .unwrap();
        assert_eq!(socket["png_base64"], result["content"][1]["data"]);
        assert_eq!(socket["font_sha256"], metadata["font_sha256"]);
        images.push(metadata);
    }
    assert_ne!(images[0]["font_sha256"], images[1]["font_sha256"]);
    let cancelled = mcp.tool(
        "nodes_cancel",
        json!({"expected_document_epoch":epoch,"request":{
            "identity":identity,"handle":started["run"]["receipt"]["handle"],
            "actor":"graph-fixture","operation_key":"cancel-completed"
        }}),
    );
    assert_eq!(cancelled["cancellation"]["receipt"]["outcome"], "too_late");
    let apply = json!({"expected_document_epoch":epoch,"identity":identity,
        "handle":started["run"]["receipt"]["handle"],"actor":"graph-fixture",
        "operation_key":"apply","authorization":"user-approved"});
    let applied = mcp.tool("nodes_apply", apply.clone());
    serde_json::from_value::<AgentApplyResponse>(applied.clone()).unwrap();
    assert_eq!(applied["root_changed"], true);
    assert_eq!(fixture.control("state")["canonical_advance"], 512.0);
    let after = mcp.tool("read_glyph", read_args.clone());
    assert_eq!(
        after["contour_count"].as_u64().unwrap(),
        before["contour_count"].as_u64().unwrap() + 1
    );
    assert_eq!(
        after["contours"][0][0]["id"],
        before["contours"][0][0]["id"]
    );
    assert_eq!(
        after["contours"][0][0]["x"].as_f64().unwrap(),
        before["contours"][0][0]["x"].as_f64().unwrap() + 10.0
    );
    let added_id = after["contour_ids"].as_array().unwrap().last().unwrap();
    assert!(
        applied["receipt"]["outcome"]["changed_objects"]
            .as_array()
            .unwrap()
            .iter()
            .any(|object| object["kind"] == "contour" && &object["contour_id"] == added_id),
        "receipt must identify the new contour"
    );
    assert_eq!(fixture.control("undo")["canonical_advance"], 412.0);
    let undone = mcp.tool("read_glyph", read_args.clone());
    assert_eq!(undone["contours"], before["contours"]);
    assert_eq!(undone["contour_ids"], before["contour_ids"]);
    let replay = mcp.tool("nodes_apply", apply);
    serde_json::from_value::<AgentApplyResponse>(replay.clone()).unwrap();
    assert_eq!(replay["receipt"], applied["receipt"]);
    assert_eq!(replay["history_state"], "undone");
    assert_eq!(replay["root_changed"], false);
    assert_eq!(
        mcp.tool("read_glyph", read_args.clone())["contours"],
        before["contours"]
    );
    assert_eq!(fixture.control("redo")["canonical_advance"], 512.0);
    let redone = mcp.tool("read_glyph", read_args);
    assert_eq!(redone["contour_ids"], after["contour_ids"]);
    assert_eq!(redone["contours"], after["contours"]);
    assert_eq!(fixture.control("undo")["canonical_advance"], 412.0);
    let stale = mcp.tool("nodes_status", status_args.clone());
    assert_eq!(stale["stale"], true);
    assert_eq!(stale["can_apply"], false);
    assert_eq!(
        mcp.tool("nodes_release", status_args.clone())["released"],
        true
    );
    assert_eq!(
        mcp.tool("nodes_status", status_args)["run"]["status"],
        "released"
    );
    let state = fixture.control("state");
    assert_eq!(state["canonical_advance"], 412.0);
    assert_eq!(state["source_exists"], false);
}

#[test]
fn mcp_generated_contours_reject_invalid_batches_and_reconcile_retries() {
    use runebender::automation::agent_edit::results::AgentApplyResponse;

    let mut fixture = Fixture::start();
    let ready = fixture.read();
    let endpoint = ready["session"].as_str().unwrap();
    let epoch = &ready["document_epoch"];
    let mut mcp = Fixture::spawn(Command::new(env!("CARGO_BIN_EXE_runebender")).args([
        "mcp",
        "--session",
        endpoint,
    ]));
    mcp.rpc("initialize", json!({"protocolVersion":"2025-11-25"}));
    let read_args = json!({"expected_document_epoch":epoch,"source":0,"glyph":"A"});
    let before = mcp.tool("read_glyph", read_args.clone());
    let triangle = json!({"points":[
        {"x":20.0,"y":20.0,"type":"line"},
        {"x":80.0,"y":20.0,"type":"line"},
        {"x":50.0,"y":80.0,"type":"line"}
    ]});
    let payload = json!({"expected_document_epoch":epoch,"actor":"shape-fixture",
    "operation_key":"append-triangle","authorization":"user-approved","source":0,
    "history_name":"Add generated triangle","edits":[{
        "target":{"glyph":"A","glyph_id":before["glyph_id"],"layer":before["layer"],"expected_revision":before["revision"]},
        "operations":[{"op":"set_width","width":500.0},{"op":"append_contours","contours":[triangle]}]
    }]});
    let state = fixture.control("state");
    let mut malformed = payload.clone();
    malformed["operation_key"] = json!("invalid-contour");
    malformed["edits"][0]["operations"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "op":"append_contours","contours":[{"points":[
                {"x":0,"y":0,"type":"move"},{"x":50,"y":50,"type":"curve"}
            ]}]
        }));
    let rejected = mcp.rejected_tool("agent_apply", malformed.clone());
    serde_json::from_value::<AgentApplyResponse>(rejected.clone()).unwrap();
    assert_eq!(rejected["receipt"]["outcome"]["status"], "rejected");
    assert_eq!(
        mcp.rejected_tool("agent_apply", malformed)["receipt"],
        rejected["receipt"]
    );
    let untouched = mcp.tool("read_glyph", read_args.clone());
    assert_eq!(untouched["contours"], before["contours"]);
    assert_eq!(untouched["advance"], before["advance"]);
    assert_eq!(
        fixture.control("state")["document_revision"],
        state["document_revision"]
    );
    let mut oversized = payload.clone();
    oversized["operation_key"] = json!("oversized-contours");
    oversized["edits"][0]["operations"][1]["contours"] = json!(vec![triangle; 257]);
    assert_eq!(
        mcp.rejected_tool("agent_apply", oversized)["receipt"]["outcome"]["status"],
        "rejected"
    );

    let applied = mcp.tool("agent_apply", payload.clone());
    serde_json::from_value::<AgentApplyResponse>(applied.clone()).unwrap();
    let after = mcp.tool("read_glyph", read_args.clone());
    assert_eq!(
        after["contour_count"].as_u64().unwrap(),
        before["contour_count"].as_u64().unwrap() + 1
    );
    let mut stale = payload.clone();
    stale["operation_key"] = json!("stale-contour");
    assert_eq!(
        mcp.rejected_tool("agent_apply", stale)["receipt"]["outcome"]["status"],
        "rejected"
    );
    assert_eq!(
        mcp.tool("agent_apply", payload.clone())["receipt"],
        applied["receipt"]
    );
    fixture.control("undo");
    let replay = mcp.tool("agent_apply", payload);
    assert_eq!(replay["history_state"], "undone");
    assert_eq!(replay["root_changed"], false);
    let undone = mcp.tool("read_glyph", read_args.clone());
    assert_eq!(undone["contours"], before["contours"]);
    assert_eq!(undone["advance"], before["advance"]);
    fixture.control("redo");
    let redone = mcp.tool("read_glyph", read_args);
    assert_eq!(redone["contour_ids"], after["contour_ids"]);
    assert_eq!(redone["contours"], after["contours"]);
    assert_eq!(fixture.control("state")["source_exists"], false);
}

#[test]
fn mcp_dag_chains_branches_and_keeps_successful_proofs_after_partial_failure() {
    use runebender::font::variable::SourceId;
    use runebender::workflows::nodes_live;
    use std::time::{Duration, Instant};

    let mut fixture = Fixture::spawn(Command::new(env!("CARGO_BIN_EXE_runebender")).args([
        "agent",
        "fixture",
        "--duration-seconds",
        "90",
    ]));
    let ready = fixture.read();
    let mut mcp = Fixture::spawn(Command::new(env!("CARGO_BIN_EXE_runebender")).args([
        "mcp",
        "--session",
        ready["session"].as_str().unwrap(),
    ]));
    mcp.rpc("initialize", json!({"protocolVersion":"2025-11-25"}));
    mcp.assert_contracts(&[
        "nodes_discover",
        "nodes_run",
        "nodes_status",
        "nodes_apply",
        "nodes_image",
    ]);
    let epoch = &ready["document_epoch"];
    let discovered = mcp.tool("nodes_discover", json!({"expected_document_epoch":epoch}));
    assert_eq!(discovered["discovery"]["execution_versions"], json!([1, 2]));
    let identity = &discovered["identity"];
    let snapshot = mcp.tool(
        "nodes_snapshot",
        json!({"expected_document_epoch":epoch,"identity":identity}),
    )["snapshot"]
        .clone();
    let before = mcp.tool(
        "read_glyph",
        json!({"expected_document_epoch":epoch,"source":0,"glyph":"A"}),
    );
    let mut graph = nodes_live::comparison_starter(SourceId(0));
    graph.node_mut(3).unwrap().values.insert("code".into(), json!(r#"import json,sys
p=json.load(sys.stdin)
l=p['layers'][0]
drawing={'points':[{'x':x,'y':y,'type':'line'} for x,y in [(50,500),(150,500),(150,600),(50,600)]]}
json.dump({'schema_version':2,'job_id':p['job_id'],'input_hash':p['input_hash'],'report':'parent','reads':[],'edits':[{'target':l['guard'],'operations':[{'op':'set_width','width':l['width']+100},{'op':'append_contours','contours':[drawing]}]}]},sys.stdout)
"#));
    let child = graph.add("live.python", [700.0, 60.0]);
    graph.node_mut(child).unwrap().values.insert("code".into(), json!(r#"import json,sys
p=json.load(sys.stdin)
l=p['layers'][0]
point=l['contours'][-1]['points'][0]
assert point['x']==50 and point['y']==500
json.dump({'schema_version':2,'job_id':p['job_id'],'input_hash':p['input_hash'],'report':'moved:'+point['id'],'reads':[],'edits':[{'target':l['guard'],'operations':[{'op':'set_width','width':l['width']+17},{'op':'set_point','point_id':point['id'],'x':83,'y':500}]}]},sys.stdout)
"#));
    graph.connect(3, "font", child, "font");
    let child_proof = graph.add("live.proof", [1000.0, 60.0]);
    graph.connect(child, "font", child_proof, "font");
    let sibling = graph.add("live.python", [400.0, 650.0]);
    graph.node_mut(sibling).unwrap().values.insert("code".into(), json!(r#"import json,sys
p=json.load(sys.stdin)
l=p['layers'][0]
assert l['width']==412
json.dump({'schema_version':2,'job_id':p['job_id'],'input_hash':p['input_hash'],'report':'sibling','reads':[],'edits':[{'target':l['guard'],'operations':[{'op':'set_width','width':l['width']+25}]}]},sys.stdout)
"#));
    graph.connect(1, "font", sibling, "font");
    let sibling_proof = graph.add("live.proof", [700.0, 650.0]);
    graph.connect(sibling, "font", sibling_proof, "font");
    let failed = graph.add("live.python", [400.0, 1000.0]);
    graph.node_mut(failed).unwrap().values.insert(
        "code".into(),
        json!("raise RuntimeError('intentional branch failure')"),
    );
    graph.connect(1, "font", failed, "font");
    let blocked = graph.add("live.python", [700.0, 1000.0]);
    graph.node_mut(blocked).unwrap().values.insert(
        "code".into(),
        json!("raise RuntimeError('this dependent must not execute')"),
    );
    graph.connect(failed, "font", blocked, "font");
    let blocked_proof = graph.add("live.proof", [1000.0, 1000.0]);
    graph.connect(blocked, "font", blocked_proof, "font");
    for node in &mut graph.nodes {
        if node.type_name == "live.proof" {
            let mut recipe = nodes_live::default_proof_recipe();
            recipe["text"] = json!(if node.id == child_proof { "AAA" } else { "AA" });
            node.values.insert("recipe".into(), recipe);
        }
    }
    let mut edits = snapshot["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| json!({"edit":"remove_node","node":node["id"]}))
        .collect::<Vec<_>>();
    edits.extend(
        graph
            .nodes
            .iter()
            .map(|node| json!({"edit":"add_node","node":node})),
    );
    edits.extend(
        graph
            .links
            .iter()
            .map(|link| json!({"edit":"connect","link":link})),
    );
    let mutated = mcp.tool("nodes_mutate", json!({"expected_document_epoch":epoch,"request":{
        "guard":{"identity":identity,"revision":snapshot["revision"]},"actor":"dag-fixture","operation_key":"configure",
        "mutation":{"mutation":"patch","edits":edits}
    }}));
    let snapshot = &mutated["snapshot"];
    let request = json!({"expected_document_epoch":epoch,"execution_version":2,
        "guard":{"identity":identity,"semantic_revision":snapshot["semantic_revision"],"semantic_hash":snapshot["semantic_hash"]},
        "actor":"dag-fixture","operation_key":"run","source":0,"glyphs":["A"]});
    let mut legacy = request.clone();
    legacy.as_object_mut().unwrap().remove("execution_version");
    legacy["operation_key"] = json!("legacy-rejects-dag");
    mcp.rejected_tool("nodes_run", legacy);
    let started = mcp.tool("nodes_run", request.clone());
    let status_args = json!({"expected_document_epoch":epoch,"identity":identity,"handle":started["run"]["receipt"]["handle"]});
    // Layout changes during execution preserve semantic identity and the captured input.
    mcp.tool("nodes_mutate", json!({"expected_document_epoch":epoch,"request":{
        "guard":{"identity":identity,"revision":snapshot["revision"]},"actor":"dag-fixture","operation_key":"layout",
        "mutation":{"mutation":"patch","edits":[{"edit":"move_node","node":child,"pos":[710.0,60.0]}]}
    }}));
    let deadline = Instant::now() + Duration::from_secs(60);
    let completed = loop {
        let status = mcp.tool("nodes_status", status_args.clone());
        match status["run"]["status"].as_str() {
            Some("partially_failed") => break status,
            Some("queued" | "running") => {
                assert!(Instant::now() < deadline, "DAG timed out: {status}");
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => panic!("unexpected DAG status: {status}"),
        }
    };
    assert_eq!(completed["current"], true);
    assert_eq!(
        completed["can_apply"], false,
        "ambiguous default selection must not guess a branch"
    );
    let errors = completed["run"]["errors"].as_array().unwrap();
    for node in [failed, blocked, blocked_proof] {
        assert!(
            errors.iter().any(|error| error["node"] == node),
            "missing failed node {node}: {completed}"
        );
    }
    assert!(
        errors
            .iter()
            .find(|error| error["node"] == blocked)
            .unwrap()["code"]
            .as_str()
            .unwrap()
            .contains("dependency")
    );
    assert_eq!(fixture.control("state")["canonical_advance"], 412.0);
    assert_eq!(
        mcp.tool(
            "read_glyph",
            json!({"expected_document_epoch":epoch,"source":0,"glyph":"A"})
        )["contours"],
        before["contours"]
    );
    let retry = mcp.tool("nodes_run", request);
    assert_eq!(retry["run"], started["run"]);
    assert_eq!(retry["replayed"], true);
    let outputs = completed["run"]["outputs"].as_array().unwrap();
    for (proof_node, parent, advance) in [
        (2, 1, 412.0),
        (4, 3, 512.0),
        (child_proof, child, 529.0),
        (sibling_proof, sibling, 437.0),
    ] {
        let mut args = status_args.clone();
        args["node"] = json!(proof_node);
        let proof = mcp.tool("nodes_image", args);
        assert_eq!(proof["glyphs"][0]["x_advance"], advance);
        let expected = if parent == 1 {
            &completed["run"]["identity"]["capture"]["font"]["capture_sha256"]
        } else {
            &outputs
                .iter()
                .find(|output| {
                    output["node"] == parent && output["value"]["kind"] == "font_version"
                })
                .unwrap()["value"]["content_sha256"]
        };
        assert_eq!(&proof["canonical_input_sha256"], expected);
    }
    let mut alias = status_args.clone();
    alias["branch"] = json!("changed");
    mcp.rejected_tool("nodes_image", alias);
    let mut apply = status_args.clone();
    apply["actor"] = json!("dag-fixture");
    apply["operation_key"] = json!("apply-child");
    apply["authorization"] = json!("user-approved");
    mcp.rejected_tool("nodes_apply", apply.clone());
    apply["node"] = json!(child);
    let applied = mcp.tool("nodes_apply", apply.clone());
    assert_eq!(fixture.control("state")["canonical_advance"], 529.0);
    let after = mcp.tool(
        "read_glyph",
        json!({"expected_document_epoch":epoch,"source":0,"glyph":"A"}),
    );
    let last = after["contours"].as_array().unwrap().last().unwrap();
    assert_eq!(last[0]["x"], 83.0);
    let report = outputs
        .iter()
        .find(|output| output["node"] == child && output["value"]["kind"] == "report")
        .unwrap()["value"]["text"]
        .as_str()
        .unwrap();
    assert_eq!(
        report.strip_prefix("moved:").unwrap(),
        last[0]["id"].as_str().unwrap()
    );
    assert_eq!(fixture.control("undo")["canonical_advance"], 412.0);
    let replay = mcp.tool("nodes_apply", apply.clone());
    assert_eq!(replay["receipt"], applied["receipt"]);
    assert_eq!(replay["history_state"], "undone");
    let mut different = apply;
    different["node"] = json!(sibling);
    mcp.rejected_tool("nodes_apply", different);
    assert_eq!(fixture.control("redo")["canonical_advance"], 529.0);
    let redone = mcp.tool(
        "read_glyph",
        json!({"expected_document_epoch":epoch,"source":0,"glyph":"A"}),
    );
    assert_eq!(redone["contours"], after["contours"]);
    assert_eq!(fixture.control("undo")["canonical_advance"], 412.0);
    assert_eq!(mcp.tool("nodes_status", status_args.clone())["stale"], true);
    assert_eq!(
        mcp.tool("nodes_release", status_args.clone())["released"],
        true
    );
    assert_eq!(
        mcp.tool("nodes_status", status_args)["run"]["status"],
        "released"
    );
    assert_eq!(fixture.control("state")["source_exists"], false);
}

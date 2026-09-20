// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The real CLI and MCP adapters read an unsaved, editor-owned project.

#![cfg(unix)]

use runebender::document::{live, live_socket::Server, project::Project};
use serde_json::{Value, json};
use std::io::{BufRead as _, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn generated_live_prompt_matches_available_tools_and_authorized_edits() {
    let output = Command::new(env!("CARGO_BIN_EXE_runebender"))
        .args(["agent", "tools"])
        .env("RUNEBENDER_LIVE_SESSION", "schema-only-no-connection")
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let prompt = value["prompt"].as_str().unwrap();
    assert!(prompt.starts_with(live::INSTRUCTIONS));
    assert!(!prompt.contains("You cannot edit the font"));
    assert!(!prompt.contains("chosen master"));
    assert!(!prompt.contains("call docs first"));
    assert!(prompt.contains("proposal_install"));
}

#[test]
fn cli_and_mcp_share_one_unsaved_authorized_document() {
    let server = Server::start().unwrap();
    let path = server.path().to_path_buf();
    let font_path = std::env::temp_dir().join(format!(
        "runebender-live-never-written-{}-{}.ufo",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("the system clock is after the Unix epoch")
            .as_nanos()
    ));
    let mut project = Project::new_font(font_path.clone());
    project
        .add_document_glyph("live_test", 731.0, None)
        .unwrap();
    let editor = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        for _ in 0..7 {
            loop {
                if let Some(request) = server.try_recv() {
                    request.respond(|request| {
                        live::call(&mut project, &request.name, &request.arguments)
                    });
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    });
    let output = Command::new(env!("CARGO_BIN_EXE_runebender"))
        .args(["agent", "call", "read_glyph", "--session"])
        .arg(&path)
        .args(["--args", r#"{"glyph":"live_test"}"#])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["result"]["advance"], 731.0);
    let revision = value["result"]["revision"].clone();

    let mut mcp = Command::new(env!("CARGO_BIN_EXE_runebender"))
        .args(["mcp", "--live"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = mcp.stdin.take().unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0", "id":1, "method":"tools/list"})
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0", "id":2, "method":"tools/call",
        "params":{"name":"editor_connect", "arguments":{"session":path}}})
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0", "id":3, "method":"tools/call",
        "params":{"name":"read_glyph", "arguments":{"glyph":"live_test"}}})
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0", "id":4, "method":"tools/call",
        "params":{"name":"propose_edits", "arguments":{"task":"spacing",
        "reason":"socket integration test", "edits":[{"glyph":"live_test",
        "expected_revision":revision,"operations":[{"op":"set_width","width":760.0}]}]}}})
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0", "id":5, "method":"tools/call",
        "params":{"name":"proposal_install", "arguments":{"task":"spacing",
        "keep_structure":true}}})
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0", "id":6, "method":"tools/call",
        "params":{"name":"proposal_install", "arguments":{"task":"spacing",
        "keep_structure":true,"authorization":"user-approved"}}})
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0", "id":7, "method":"tools/call",
        "params":{"name":"read_glyph", "arguments":{"glyph":"live_test"}}})
    )
    .unwrap();
    drop(input);
    let output = mcp.wait_with_output().unwrap();
    let replies: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        replies[0]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .all(|tool| tool["name"] != "nodes_run")
    );
    let result: Value =
        serde_json::from_str(replies[2]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(result["advance"], 731.0);
    let proposed: Value =
        serde_json::from_str(replies[3]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(proposed["ok"], true);
    let unauthorized: Value =
        serde_json::from_str(replies[4]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(unauthorized["ok"], false);
    assert!(
        unauthorized["error"]
            .as_str()
            .is_some_and(|error| error.contains("explicit user authorization"))
    );
    let installed: Value =
        serde_json::from_str(replies[5]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(installed["installed"]["installed"], json!(["live_test"]));
    let reread: Value =
        serde_json::from_str(replies[6]["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(reread["advance"], 760.0);
    editor.join().unwrap();
    assert!(
        !font_path.exists(),
        "the live socket must never save its editor-owned document"
    );
}

#[test]
fn mcp_negotiates_known_versions_and_bounds_input() {
    use std::io::BufRead as _;
    for (requested, expected) in [("2024-11-05", "2024-11-05"), ("2099-01-01", "2025-11-25")] {
        let mut mcp = Command::new(env!("CARGO_BIN_EXE_runebender"))
            .args(["mcp", "--live"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = mcp.stdin.take().unwrap();
        let mut output = std::io::BufReader::new(mcp.stdout.take().unwrap());
        writeln!(input, "{}", json!({"jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":requested,"capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).unwrap();
        input.flush().unwrap();
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["result"]["protocolVersion"], expected);
        input.write_all(&vec![b' '; 8 * 1024 * 1024 + 1]).unwrap();
        input.flush().unwrap();
        drop(input);
        let mut line = String::new();
        output.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(reply["error"]["code"], -32600);
        assert!(mcp.wait().unwrap().success());
    }
}

#[test]
fn mcp_reserves_cancellation_before_apply_admission_and_preserves_framing() {
    let server = Server::start().unwrap();
    let endpoint = server.path().to_path_buf();
    let epoch = server.document_epoch().to_owned();
    let mut mcp = Command::new(env!("CARGO_BIN_EXE_runebender"))
        .args(["mcp", "--session"])
        .arg(&endpoint)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = mcp.stdin.take().unwrap();
    let mut output = std::io::BufReader::new(mcp.stdout.take().unwrap());
    let mut read = || {
        let mut line = String::new();
        assert_ne!(output.read_line(&mut line).unwrap(), 0, "MCP stdout closed");
        serde_json::from_str::<Value>(&line).unwrap()
    };
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":"2025-11-25","capabilities":{},
                "clientInfo":{"name":"cancel-test","version":"1"}}})
    )
    .unwrap();
    input.flush().unwrap();
    assert_eq!(read()["id"], 1);

    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"read_glyph","arguments":{"glyph":"blocker"}}})
    )
    .unwrap();
    input.flush().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let blocker = loop {
        if let Some(pending) = server.try_recv() {
            break pending;
        }
        assert!(
            Instant::now() < deadline,
            "blocker did not enter socket mailbox"
        );
        std::thread::sleep(Duration::from_millis(5));
    };

    let identity = json!({
        "expected_document_epoch":epoch,
        "actor":"stdio-cancel",
        "operation_key":"reserved-apply"
    });
    let mut apply_arguments = identity.clone();
    apply_arguments["authorization"] = json!("user-approved");
    apply_arguments["source"] = json!(0);
    apply_arguments["history_name"] = json!("stdio cancellation fixture");
    apply_arguments["edits"] = json!([{
        "target":{"glyph":"A","glyph_id":"fixture","layer":"public.default",
            "expected_revision":"fixture"},
        "operations":[{"op":"set_width","width":500.0}]
    }]);
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call",
            "params":{"name":"agent_apply","arguments":apply_arguments}})
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","method":"notifications/cancelled",
            "params":{"requestId":3,"reason":"test cancellation"}})
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"agent_cancel","arguments":identity}})
    )
    .unwrap();
    input.flush().unwrap();
    let cancel_reply = read();
    assert_eq!(cancel_reply["id"], 4);
    let cancel_result: Value = serde_json::from_str(
        cancel_reply["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(cancel_result["cancellation_status"], "already_prevented");

    blocker.respond(|_| json!({"ok":true,"glyph":"blocker"}));
    assert_eq!(read()["id"], 2);
    let deadline = Instant::now() + Duration::from_secs(5);
    let pending = loop {
        if let Some(pending) = server.try_recv() {
            break pending;
        }
        assert!(
            Instant::now() < deadline,
            "cancelled apply was not delivered to record its terminal receipt"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    pending.respond(|_| json!({"ok":false,"cancellation_status":"prevented"}));
    writeln!(input, "{}", json!({"jsonrpc":"2.0","id":5,"method":"ping"})).unwrap();
    input.flush().unwrap();
    let ping = read();
    assert_eq!(ping["id"], 5, "cancelled request must not emit a reply");
    drop(input);
    assert!(mcp.wait().unwrap().success());
}

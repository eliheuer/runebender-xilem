// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The real CLI and MCP adapters read an unsaved, editor-owned project.

#![cfg(all(unix, feature = "cli"))]

use runebender_core::document::{live, live_socket::Server, project::Project};
use serde_json::{Value, json};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn cli_and_mcp_read_the_same_unsaved_glyph() {
    let server = Server::start().unwrap();
    let path = server.path().to_path_buf();
    let mut project = Project::new_font("not-written.ufo".into());
    project.masters[0].add_glyph("live_test", 731.0).unwrap();
    let editor = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        for _ in 0..3 {
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
    let output = Command::new(env!("CARGO_BIN_EXE_runebender-core"))
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

    let mut mcp = Command::new(env!("CARGO_BIN_EXE_runebender-core"))
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
        json!({"jsonrpc":"2.0", "id":3, "method":"tools/call",
        "params":{"name":"editor_connect", "arguments":{"session":path}}})
    )
    .unwrap();
    writeln!(
        input,
        "{}",
        json!({"jsonrpc":"2.0", "id":2, "method":"tools/call",
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
    editor.join().unwrap();
}

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
}

impl Fixture {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_runebender"))
            .args(["agent", "fixture", "--duration-seconds", "15"])
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

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Offline integration of the real Chat entry point, HTTP adapter and live document mailbox.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use serde_json::json;

use super::*;
use crate::application::font_model::FontModel;
use runebender::font::project::Project;
use runebender::workflows::local_chat::LocalChatEndpoint;

fn request(listener: &TcpListener) -> (TcpStream, Value) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "fixture request deadline");
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("fixture accept: {error}"),
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    reader.read_line(&mut first).unwrap();
    assert_eq!(
        first, "POST /v1/chat/completions HTTP/1.1\r\n",
        "fixture expects the completion endpoint"
    );
    let mut length = None;
    loop {
        let mut line = String::new();
        assert!(
            reader.read_line(&mut line).unwrap() > 0,
            "request ended before its header boundary"
        );
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = Some(value.trim().parse::<usize>().unwrap());
        }
    }
    let length = length.expect("request length");
    assert!(
        length < 1024 * 1024,
        "fixture request exceeds context bound"
    );
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).unwrap();
    (reader.into_inner(), serde_json::from_slice(&bytes).unwrap())
}

fn respond(mut stream: TcpStream, message: Value) {
    let finish = if message.get("tool_calls").is_some() {
        "tool_calls"
    } else {
        "stop"
    };
    let bytes = json!({"choices":[{"index":0,"message":message,"finish_reason":finish}],
        "usage":{"completion_tokens":10}})
    .to_string();
    // A cancelled client can close while this deliberately late fixture responds.
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        bytes.len(),
        bytes
    );
}

fn call(name: &str, arguments: Value) -> Value {
    json!({"role":"assistant","content":null,"tool_calls":[{
        "id":"call_0","type":"function","function":{"name":name,"arguments":arguments.to_string()}
    }]})
}

fn listener() -> (TcpListener, LocalChatEndpoint) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = LocalChatEndpoint::parse(
        &format!("http://{}", listener.local_addr().unwrap()),
        "font-ml",
    )
    .unwrap();
    (listener, endpoint)
}

fn workspace(endpoint: LocalChatEndpoint, name: &str) -> Workspace {
    let path =
        std::env::temp_dir().join(format!("persistent-chat-{name}-{}.ufo", std::process::id()));
    assert!(!path.exists(), "fixture must stay unsaved");
    let mut project = Project::new_font(path);
    project
        .add_document_glyph("A", 412.0, Some(u32::from('A')))
        .unwrap();
    let source = project.source_id(0).unwrap();
    let layer = project.document_source(source).unwrap().default_layer();
    project
        .edit_document_layer("A", &layer, |draft| {
            draft.set_width(412.0)?;
            Ok(())
        })
        .unwrap();
    let mut app = Workspace::from_model(FontModel::from_project(project)).unwrap();
    app.chat.endpoint = Some(Ok(endpoint));
    app.chat.model = None;
    app.nodes.font_ml = None;
    app
}

fn pump(app: &mut Workspace) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut dispatched = Vec::new();
    while app.chat.job.is_some() {
        if let Some(pending) = app.live.as_ref().unwrap().try_recv() {
            pending.respond(|call| {
                dispatched.push(call.name.clone());
                app.call_live(call)
            });
        }
        app.chat_pump();
        assert!(Instant::now() < deadline, "chat completion deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    dispatched
}

#[test]
fn persistent_chat_reads_unsaved_font_proposes_and_reuses_server_across_turns() {
    let (listener, endpoint) = listener();
    let mut app = workspace(endpoint, "reuse");
    let source = app.font.project.source_id(0).unwrap();
    let server = std::thread::spawn(move || {
        let (stream, first) = request(&listener);
        assert_eq!(first["model"], "font-ml");
        assert_eq!(first["stream"], false);
        assert!(first["tools"].as_array().unwrap().iter().all(|tool| {
            !matches!(
                tool["function"]["name"].as_str(),
                Some("proposal_install" | "nodes_run" | "agent_apply")
            )
        }));
        respond(
            stream,
            call("read_glyph", json!({"source":source.0,"glyph":"A"})),
        );
        let (stream, second) = request(&listener);
        let read: Value = serde_json::from_str(
            second["messages"].as_array().unwrap().last().unwrap()["content"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(read["advance"], 412.0);
        respond(
            stream,
            call(
                "propose_edits",
                json!({"source":source.0,
            "expected_document_epoch":"model-cannot-select-another-document",
            "task":"persistent-width","reason":"Review a wider advance",
            "edits":[{"glyph":"A","expected_revision":read["revision"],"operations":[{"op":"set_width","width":500.0}]}]}),
            ),
        );
        let (stream, third) = request(&listener);
        let proposed: Value = serde_json::from_str(
            third["messages"].as_array().unwrap().last().unwrap()["content"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(proposed["ok"], true, "{proposed}");
        respond(
            stream,
            json!({"role":"assistant","content":"The proposal is ready for review."}),
        );
        let (stream, fourth) = request(&listener);
        let messages = fourth["messages"].as_array().unwrap();
        assert!(
            messages
                .iter()
                .any(|m| m["content"] == "The proposal is ready for review.")
        );
        assert_eq!(
            messages.last().unwrap()["content"],
            "Is the foreground unchanged?"
        );
        respond(
            stream,
            json!({"role":"assistant","content":"The foreground was not installed."}),
        );
    });
    app.chat_send("Read A and propose a wider advance.".into());
    assert_eq!(pump(&mut app), ["read_glyph", "propose_edits"]);
    assert!(
        app.chat
            .entries
            .iter()
            .all(|entry| !matches!(entry, ChatEntry::Error(_))),
        "{:?}",
        app.chat.entries
    );
    let foreground = app.call_live(&runebender::automation::agent::ToolCall {
        name: "read_glyph".into(),
        arguments: json!({"source":source.0,"glyph":"A"}),
    });
    assert_eq!(foreground["advance"], 412.0);
    assert!(
        app.ai
            .proposals
            .iter()
            .any(|p| p.task == "persistent-width")
    );
    app.chat_send("Is the foreground unchanged?".into());
    assert!(pump(&mut app).is_empty());
    server.join().unwrap();
    assert!(
        app.chat
            .entries
            .iter()
            .all(|entry| !matches!(entry, ChatEntry::Error(_))),
        "{:?}",
        app.chat.entries
    );
    assert!(
        !app.font.document_source().exists(),
        "chat must never save the font"
    );
}

#[test]
fn cancelling_persistent_chat_before_inference_returns_never_dispatches_its_tool() {
    let (listener, endpoint) = listener();
    let mut app = workspace(endpoint, "cancel");
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (stream, _) = request(&listener);
        ready_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        respond(stream, call("read_glyph", json!({"glyph":"A"})));
    });
    app.chat_send("Read A.".into());
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    app.chat_cancel();
    release_tx.send(()).unwrap();
    assert!(pump(&mut app).is_empty());
    server.join().unwrap();
    assert!(
        app.chat
            .entries
            .iter()
            .any(|entry| matches!(entry,ChatEntry::Error(text) if text.contains("cancel")))
    );
    assert_eq!(
        app.chat.messages.len(),
        1,
        "no late inference conversation published"
    );
}

#[test]
fn replacing_document_after_inference_discards_reply_and_context() {
    let (listener, endpoint) = listener();
    let mut app = workspace(endpoint, "replace");
    let server = std::thread::spawn(move || {
        let (stream, _) = request(&listener);
        respond(
            stream,
            json!({"role":"assistant","content":"Reply from the previous document."}),
        );
    });
    app.chat_send("Describe this font.".into());
    let deadline = Instant::now() + Duration::from_secs(5);
    while app
        .chat
        .job
        .as_ref()
        .unwrap()
        .finished
        .lock()
        .unwrap()
        .is_none()
    {
        assert!(
            Instant::now() < deadline,
            "inference fixture completion deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    server.join().unwrap();
    app.live = Some(runebender::automation::live_socket::Server::start().unwrap());
    app.chat_pump();
    assert!(app.chat.job.is_none());
    assert!(
        app.chat.messages.is_empty(),
        "old context must not enter the new document"
    );
    assert!(!app.chat.entries.iter().any(
        |entry| matches!(entry,ChatEntry::Assistant(text) if text.contains("previous document"))
    ));
    assert!(
        app.chat.entries.iter().any(
            |entry| matches!(entry,ChatEntry::Error(text) if text.contains("document changed"))
        )
    );
}

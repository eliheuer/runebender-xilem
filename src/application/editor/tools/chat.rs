// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Local chat over the editor-owned live document.
//!
//! `font-ml chat` owns inference and asks `runebender` for the same
//! read/propose tools exposed to other agents. This shell owns only process
//! lifetime, streaming presentation, and the private live-session endpoint.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::application::editor::tools::scripts::{ScriptArtifact, python_artifacts};
use crate::application::workspace::Workspace;

/// One visible transcript row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ChatEntry {
    /// What the user typed.
    User(String),
    /// Model prose, with tool-call markup removed.
    Assistant(String),
    /// A complete Python block the user can explicitly open as a draft.
    Script(ScriptArtifact),
    /// One tool invocation and its short result.
    Tool {
        name: String,
        ok: bool,
        note: String,
    },
    /// A process or protocol failure.
    Error(String),
}

/// Shared state for one background chat turn.
#[derive(Debug, Clone, Default)]
pub(crate) struct ChatJob {
    pub(crate) events: Arc<Mutex<Vec<Value>>>,
    pub(crate) child: Arc<Mutex<Option<std::process::Child>>>,
    pub(crate) finished: Arc<Mutex<Option<Result<(), String>>>>,
}

/// Everything retained by the Chat panel.
#[derive(Debug, Default)]
pub(crate) struct ChatState {
    pub(crate) prompt: String,
    pub(crate) model: Option<PathBuf>,
    pub(crate) installed: Vec<(String, PathBuf)>,
    pub(crate) entries: Vec<ChatEntry>,
    pub(crate) messages: Vec<Value>,
    pub(crate) busy: Option<String>,
    pub(crate) job: Option<ChatJob>,
    pub(crate) last_speed: Option<String>,
    /// Complete artifacts observed during the current stream.
    ///
    /// These are previews only until the turn ends; they never alter the
    /// Scripts buffer without an explicit Open action.
    pub(crate) streaming_artifacts: Vec<ScriptArtifact>,
    raw_assistant: String,
}

impl Drop for ChatState {
    fn drop(&mut self) {
        if let Some(job) = &self.job
            && let Some(child) = job
                .child
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .as_mut()
        {
            let _ = child.kill();
        }
    }
}

/// A wake-up from the asynchronous chat pump.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChatProgress;

/// Use this application's CLI, with an explicit override for custom runners.
fn core_binary() -> Option<PathBuf> {
    std::env::var_os("RUNEBENDER_CORE")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::current_exe().ok())
}

impl Workspace {
    /// Rescan local GGUF chat-model folders without loading any weights.
    pub(crate) fn scan_chat_models(&mut self) {
        self.chat.installed =
            runebender::workflows::nodes_run::installed_chat_models(Self::models_dir().as_deref());
        if self
            .chat
            .model
            .as_ref()
            .is_none_or(|selected| !self.chat.installed.iter().any(|(_, path)| path == selected))
        {
            self.chat.model = self
                .chat
                .installed
                .iter()
                .find(|(name, _)| name.contains("4b"))
                .or_else(|| self.chat.installed.first())
                .map(|(_, path)| path.clone());
        }
    }

    /// Forget the visible and model transcripts.
    pub(crate) fn chat_clear(&mut self) {
        if self.chat.job.is_some() {
            self.note = "Cancel the current chat turn before clearing it".into();
            return;
        }
        self.chat.entries.clear();
        self.chat.messages.clear();
        self.chat.last_speed = None;
        self.chat.streaming_artifacts.clear();
        self.chat.raw_assistant.clear();
    }

    /// Start one local-model turn against this editor's live endpoint.
    pub(crate) fn chat_send(&mut self, text: String) {
        if cfg!(target_arch = "wasm32") {
            self.note = "Local AI and workflow execution are available in the desktop app.".into();
            return;
        }
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        if self.chat.job.is_some() {
            self.note = "The model is still answering".into();
            return;
        }
        let Some(model) = self.chat.model.clone() else {
            self.note = "Choose a local chat model first".into();
            return;
        };
        let Some(font_ml) = self.nodes.font_ml.clone() else {
            self.note = "font-ml not found; install it or set RUNEBENDER_FONT_ML".into();
            return;
        };
        #[cfg(unix)]
        let session = self.live.as_ref().map(|server| server.path().to_path_buf());
        #[cfg(not(unix))]
        let session: Option<PathBuf> = None;
        let Some(session) = session else {
            self.note = "Live chat requires the native Unix editor endpoint".into();
            return;
        };
        let Some(core) = core_binary() else {
            self.note = "Could not locate the running Runebender executable".into();
            return;
        };

        self.chat.entries.push(ChatEntry::User(text.clone()));
        self.chat
            .messages
            .push(serde_json::json!({"role":"user", "content":text}));
        self.chat.entries.push(ChatEntry::Assistant(String::new()));
        self.chat.raw_assistant.clear();
        self.chat.busy = Some("Loading the model…".into());
        let conversation = match serde_json::to_string(&self.chat.messages) {
            Ok(value) => value,
            Err(error) => {
                self.chat.busy = None;
                self.chat.entries.push(ChatEntry::Error(error.to_string()));
                return;
            }
        };
        let font = self.font.document_source().to_path_buf();
        let job = ChatJob::default();
        self.chat.job = Some(job.clone());
        std::thread::spawn(move || {
            let result = run_chat(
                &font_ml,
                &model,
                &font,
                &core,
                &session,
                &conversation,
                &job,
            );
            *job.finished
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some(result);
        });
    }

    /// Stop the child process for the current turn.
    pub(crate) fn chat_cancel(&mut self) {
        let Some(job) = self.chat.job.as_ref() else {
            return;
        };
        if let Some(child) = job
            .child
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_mut()
        {
            let _ = child.kill();
        }
        self.chat.busy = Some("Cancelling…".into());
    }

    /// Drain streamed events and finish a completed turn.
    pub(crate) fn chat_pump(&mut self) {
        let Some(job) = self.chat.job.clone() else {
            return;
        };
        let events =
            std::mem::take(&mut *job.events.lock().unwrap_or_else(|error| error.into_inner()));
        for event in events {
            self.chat_event(&event);
        }
        let finished = job
            .finished
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(result) = finished {
            self.chat.busy = None;
            self.chat.job = None;
            if let Err(error) = result {
                self.chat.entries.push(ChatEntry::Error(error));
            }
            let streaming_artifacts = std::mem::take(&mut self.chat.streaming_artifacts);
            self.chat
                .entries
                .extend(streaming_artifacts.into_iter().map(ChatEntry::Script));
            self.chat
                .entries
                .retain(|entry| !matches!(entry, ChatEntry::Assistant(text) if text.is_empty()));
            self.chat.raw_assistant.clear();
            self.refresh_proposals();
        }
    }

    fn chat_event(&mut self, event: &Value) {
        match event.get("event").and_then(Value::as_str).unwrap_or("") {
            "loaded" => {
                let device = event.get("device").and_then(Value::as_str).unwrap_or("cpu");
                self.chat.busy = Some(format!("Thinking on {device}…"));
            }
            "token" => {
                self.chat
                    .raw_assistant
                    .push_str(event.get("text").and_then(Value::as_str).unwrap_or(""));
                if let Some(ChatEntry::Assistant(text)) = self.chat.entries.last_mut() {
                    *text = visible_text(&self.chat.raw_assistant);
                }
                self.chat.streaming_artifacts = python_artifacts(&self.chat.raw_assistant);
            }
            "tool_call" => {
                let name = event.get("name").and_then(Value::as_str).unwrap_or("?");
                self.chat.busy = Some(format!("Running {name}…"));
                self.chat.entries.push(ChatEntry::Tool {
                    name: name.into(),
                    ok: true,
                    note: "…".into(),
                });
            }
            "tool_result" => {
                let name = event.get("name").and_then(Value::as_str).unwrap_or("?");
                let ok = event.get("ok").and_then(Value::as_bool).unwrap_or(false);
                let note = result_note(name, event.get("result").unwrap_or(&Value::Null));
                if let Some(ChatEntry::Tool {
                    ok: current_ok,
                    note: current_note,
                    ..
                }) = self
                    .chat
                    .entries
                    .iter_mut()
                    .rev()
                    .find(|entry| matches!(entry, ChatEntry::Tool { .. }))
                {
                    *current_ok = ok;
                    *current_note = note;
                }
                self.chat.entries.push(ChatEntry::Assistant(String::new()));
                self.chat.raw_assistant.clear();
                self.chat.streaming_artifacts.clear();
                self.chat.busy = Some("Thinking…".into());
            }
            "done" => {
                let tokens = event.get("tokens").and_then(Value::as_u64).unwrap_or(0);
                let speed = event
                    .get("tokens_per_second")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                self.chat.last_speed = Some(format!("{tokens} tokens, {speed:.1} tok/s"));
                if let Some(text) = event.get("text").and_then(Value::as_str) {
                    let artifacts = python_artifacts(text);
                    if let Some(ChatEntry::Assistant(current)) = self.chat.entries.last_mut() {
                        *current = visible_text(text);
                    }
                    self.chat
                        .entries
                        .extend(artifacts.into_iter().map(ChatEntry::Script));
                }
                self.chat.streaming_artifacts.clear();
            }
            "messages" => {
                if let Some(messages) = event.get("messages").and_then(Value::as_array) {
                    self.chat.messages = messages.clone();
                }
            }
            _ => {}
        }
    }
}

fn visible_text(raw: &str) -> String {
    let mut output = String::new();
    let mut rest = raw;
    while let Some(start) = rest.find("<tool_call>") {
        output.push_str(&rest[..start]);
        match rest[start..].find("</tool_call>") {
            Some(end) => rest = &rest[start + end + "</tool_call>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    output.push_str(rest);
    output.trim_start().to_string()
}

fn result_note(name: &str, result: &Value) -> String {
    if let Some(error) = result.get("error").and_then(Value::as_str) {
        return error.into();
    }
    match name {
        "read_glyph" => format!(
            "{}: advance {}, {} contours",
            result.get("glyph").and_then(Value::as_str).unwrap_or(""),
            result.get("advance").and_then(Value::as_f64).unwrap_or(0.0),
            result
                .get("contours")
                .and_then(Value::as_array)
                .map_or(0, Vec::len)
        ),
        "propose_edits" => result
            .get("proposed")
            .and_then(Value::as_array)
            .map_or_else(
                || "proposal request finished".into(),
                |items| format!("proposed {} glyphs for review", items.len()),
            ),
        "proof" | "specimen" => "proof prepared".into(),
        _ => "done".into(),
    }
}

fn run_chat(
    font_ml: &Path,
    model: &Path,
    font: &Path,
    core: &Path,
    session: &Path,
    conversation: &str,
    job: &ChatJob,
) -> Result<(), String> {
    use std::io::{BufRead as _, Write as _};

    let mut child = std::process::Command::new(font_ml)
        .arg("chat")
        .arg("--model")
        .arg(model)
        .arg("--font")
        .arg(font)
        .arg("--core")
        .arg(core)
        .env("RUNEBENDER_LIVE_SESSION", session)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(conversation.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    let stdout = child.stdout.take().ok_or("font-ml chat has no stdout")?;
    let stderr = child.stderr.take();
    *job.child.lock().unwrap_or_else(|error| error.into_inner()) = Some(child);
    let error_reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(mut stderr) = stderr {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
        }
        text
    });
    for line in std::io::BufReader::new(stdout)
        .lines()
        .map_while(Result::ok)
    {
        if let Ok(event) = serde_json::from_str(&line) {
            job.events
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(event);
        }
    }
    let status = {
        let mut child = job.child.lock().unwrap_or_else(|error| error.into_inner());
        child
            .as_mut()
            .ok_or("cancelled")?
            .wait()
            .map_err(|error| error.to_string())?
    };
    let stderr = error_reader.join().unwrap_or_default();
    if status.success() {
        Ok(())
    } else if status.code().is_none() {
        Err("cancelled".into())
    } else {
        Err(stderr
            .lines()
            .last()
            .unwrap_or("font-ml chat failed")
            .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_hides_tool_markup_and_summarizes_results() {
        assert_eq!(
            visible_text("Before <tool_call>{\"name\":\"read_glyph\"}</tool_call> after"),
            "Before  after"
        );
        assert_eq!(
            result_note(
                "read_glyph",
                &serde_json::json!({"glyph":"beh-ar", "advance":507.0, "contours":[{},{}]})
            ),
            "beh-ar: advance 507, 2 contours"
        );
    }

    #[cfg(unix)]
    #[test]
    fn chat_runner_streams_structured_events_and_conversation() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!(
            "runebender-chat-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("create chat fixture directory");
        let runner = root.join("font-ml");
        std::fs::write(
            &runner,
            "#!/bin/sh\ninput=$(cat)\nprintf '%s\\n' '{\"event\":\"loaded\",\"device\":\"cpu\"}' '{\"event\":\"token\",\"text\":\"Hello\"}' \"{\\\"event\\\":\\\"messages\\\",\\\"messages\\\":$input}\"\n",
        )
        .expect("write fake chat runner");
        std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o700))
            .expect("make fake runner executable");
        let job = ChatJob::default();
        run_chat(
            &runner,
            Path::new("model"),
            Path::new("font.ufo"),
            Path::new("runebender"),
            Path::new("live.sock"),
            r#"[{"role":"user","content":"Inspect beh-ar"}]"#,
            &job,
        )
        .expect("fake chat turn succeeds");
        let events = job.events.lock().unwrap_or_else(|error| error.into_inner());
        assert_eq!(events.len(), 3);
        assert_eq!(events[0]["event"], "loaded");
        assert_eq!(events[1]["text"], "Hello");
        assert_eq!(events[2]["messages"][0]["content"], "Inspect beh-ar");
        std::fs::remove_dir_all(root).expect("remove chat fixture directory");
    }
}

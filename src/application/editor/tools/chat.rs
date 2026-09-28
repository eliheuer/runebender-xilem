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

use runebender::workflows::process::{
    OutputStream, ProcessCancellation, ProcessLimits, ProcessOutcome,
};

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
    pub(crate) cancellation: ProcessCancellation,
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
        if let Some(job) = &self.job {
            job.cancellation.cancel();
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
        job.cancellation.cancel();
        self.chat.busy = Some("Cancelling…".into());
    }

    /// Drain streamed events and finish a completed turn.
    pub(crate) fn chat_pump(&mut self) {
        let Some(job) = self.chat.job.clone() else {
            return;
        };
        let events =
            std::mem::take(&mut *job.events.lock().unwrap_or_else(|error| error.into_inner()));
        if !job.cancellation.is_cancelled() {
            for event in events {
                self.chat_event(&event);
            }
        }
        let finished = job
            .finished
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        if let Some(result) = finished {
            self.chat.busy = None;
            self.chat.job = None;
            // Cancellation may arrive after the process exits but before this pump.
            // Do not publish its final messages or artifacts in that interval.
            if job.cancellation.is_cancelled() {
                // A bounded event queue cancels the runner too; retain its
                // specific terminal error instead of silently dropping it.
                self.chat.entries.push(ChatEntry::Error(
                    result.err().unwrap_or_else(|| "cancelled".into()),
                ));
                self.chat.streaming_artifacts.clear();
            } else if let Err(error) = result {
                self.chat.entries.push(ChatEntry::Error(error));
            }
            let streaming_artifacts = std::mem::take(&mut self.chat.streaming_artifacts);
            if !job.cancellation.is_cancelled() {
                self.chat
                    .entries
                    .extend(streaming_artifacts.into_iter().map(ChatEntry::Script));
            }
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
    run_chat_with_limits(
        font_ml,
        model,
        font,
        core,
        session,
        conversation,
        job,
        ProcessLimits::default(),
    )
}

/// Execute one chat turn with overridable limits for offline worker fixtures.
fn run_chat_with_limits(
    font_ml: &Path,
    model: &Path,
    font: &Path,
    core: &Path,
    session: &Path,
    conversation: &str,
    job: &ChatJob,
    limits: ProcessLimits,
) -> Result<(), String> {
    // A wire-byte cap alone still permits millions of tiny JSON values and
    // unbounded Vec<Value> allocation before the UI gets its next pump.
    const MAX_EVENTS: usize = 8192;
    let mut command = std::process::Command::new(font_ml);
    command
        .arg("chat")
        .arg("--model")
        .arg(model)
        .arg("--font")
        .arg(font)
        .arg("--core")
        .arg(core)
        .env("RUNEBENDER_LIVE_SESSION", session);
    let mut accepted_events = 0;
    let mut event_limit_exceeded = false;
    let output = runebender::workflows::process::run(
        &mut command,
        conversation.as_bytes(),
        limits,
        &job.cancellation,
        |stream, line| {
            if stream != OutputStream::Stdout || event_limit_exceeded {
                return;
            }
            if let Ok(event) = serde_json::from_str::<Value>(line) {
                if accepted_events == MAX_EVENTS {
                    event_limit_exceeded = true;
                    job.cancellation.cancel();
                } else {
                    accepted_events += 1;
                    job.events
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(event);
                }
            }
        },
    );
    if event_limit_exceeded {
        return Err(format!(
            "font-ml chat produced more than {MAX_EVENTS} events"
        ));
    }
    if job.cancellation.is_cancelled() {
        return Err("cancelled".into());
    }
    match output.outcome {
        ProcessOutcome::Exited { success: true, .. } => Ok(()),
        ProcessOutcome::Exited { .. } => Err(String::from_utf8_lossy(&output.stderr)
            .lines()
            .last()
            .unwrap_or("font-ml chat failed")
            .into()),
        outcome => Err(outcome.to_string()),
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
            "#!/bin/sh\n[ \"$1\" = chat ] && [ \"$2\" = --model ] && [ \"$3\" = model ] && [ \"$4\" = --font ] && [ \"$5\" = font.ufo ] && [ \"$6\" = --core ] && [ \"$7\" = runebender ] && [ \"$RUNEBENDER_LIVE_SESSION\" = live.sock ] || exit 17\ninput=$(cat)\nprintf '%s\\n' '{\"event\":\"loaded\",\"device\":\"cpu\"}' '{\"event\":\"token\",\"text\":\"Hello\"}' \"{\\\"event\\\":\\\"messages\\\",\\\"messages\\\":$input}\"\n",
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

    #[cfg(unix)]
    #[test]
    fn chat_runner_bounds_hung_and_noisy_workers() {
        use std::os::unix::fs::PermissionsExt as _;
        use std::time::Duration;

        let root = std::env::temp_dir().join(format!(
            "runebender-chat-bounds-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("create chat fixture directory");
        let runner = root.join("font-ml");
        let job = ChatJob::default();
        std::fs::write(&runner, "#!/bin/sh\nexec sleep 10\n").expect("write hung chat runner");
        std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o700))
            .expect("make fake runner executable");
        let limits = ProcessLimits {
            deadline: Duration::from_millis(100),
            ..ProcessLimits::default()
        };
        let error = run_chat_with_limits(
            &runner,
            Path::new("model"),
            Path::new("font.ufo"),
            Path::new("runebender"),
            Path::new("live.sock"),
            "[]",
            &job,
            limits,
        )
        .expect_err("hung chat must stop at its deadline");
        assert!(error.contains("deadline"), "{error}");

        std::fs::write(&runner, "#!/bin/sh\nprintf '12345678901234567890\\n'\n")
            .expect("write noisy chat runner");
        let limits = ProcessLimits {
            stdout_bytes: 8,
            ..ProcessLimits::default()
        };
        let error = run_chat_with_limits(
            &runner,
            Path::new("model"),
            Path::new("font.ufo"),
            Path::new("runebender"),
            Path::new("live.sock"),
            "[]",
            &job,
            limits,
        )
        .expect_err("noisy chat must stop at its output limit");
        assert!(error.contains("stdout"), "{error}");

        std::fs::write(
            &runner,
            "#!/bin/sh\nprintf 'chat failure detail\\n' >&2\nexit 7\n",
        )
        .expect("write failing chat runner");
        let error = run_chat(
            &runner,
            Path::new("model"),
            Path::new("font.ufo"),
            Path::new("runebender"),
            Path::new("live.sock"),
            "[]",
            &job,
        )
        .expect_err("failed chat reports stderr");
        assert_eq!(error, "chat failure detail");

        std::fs::write(
            &runner,
            "#!/bin/sh\ni=0\nwhile [ \"$i\" -lt 8200 ]; do printf '%s\\n' '{\"event\":\"token\",\"text\":\"x\"}'; i=$((i+1)); done\n",
        )
        .expect("write many-event chat runner");
        let crowded = ChatJob::default();
        let error = run_chat(
            &runner,
            Path::new("model"),
            Path::new("font.ufo"),
            Path::new("runebender"),
            Path::new("live.sock"),
            "[]",
            &crowded,
        )
        .expect_err("too many tiny events must fail explicitly");
        assert!(error.contains("more than 8192 events"), "{error}");
        assert_eq!(
            crowded
                .events
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .len(),
            8192
        );
        std::fs::remove_dir_all(root).expect("remove chat fixture directory");
    }

    #[cfg(unix)]
    #[test]
    fn chat_cancellation_before_spawn_is_retained() {
        let job = ChatJob::default();
        job.cancellation.cancel();
        let error = run_chat(
            Path::new("not-a-real-font-ml"),
            Path::new("model"),
            Path::new("font.ufo"),
            Path::new("runebender"),
            Path::new("live.sock"),
            "[]",
            &job,
        )
        .expect_err("cancellation prevents the worker from starting");
        assert_eq!(error, "cancelled");
    }

    #[cfg(unix)]
    #[test]
    fn chat_cancel_after_worker_completion_suppresses_late_messages() {
        let root = std::env::temp_dir().join(format!(
            "runebender-chat-late-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos()
        ));
        let source = root.join("Late.ufo");
        std::fs::create_dir(&root).expect("create chat fixture directory");
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("A"));
        font.save(&source).expect("save chat fixture");
        let mut workspace = Workspace::open(&source).expect("open chat fixture");
        let job = ChatJob::default();
        job.events.lock().unwrap_or_else(|error| error.into_inner()).push(
            serde_json::json!({"event":"messages","messages":[{"role":"assistant","content":"late"}]}),
        );
        *job.finished
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(Ok(()));
        workspace.chat.job = Some(job.clone());
        workspace.chat_cancel();
        workspace.chat_pump();
        assert!(workspace.chat.job.is_none());
        assert!(workspace.chat.messages.is_empty());
        assert_eq!(
            workspace.chat.entries,
            vec![ChatEntry::Error("cancelled".into())]
        );
        std::fs::remove_dir_all(root).expect("remove chat fixture directory");
    }
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Native Chat adapter for an externally managed resident local model.
//!
//! Inference has no access to the project. This host validates complete model
//! replies and forwards a small read/propose tool set to one captured live
//! document lifetime. A cancelled request may finish on the remote server;
//! cancellation only stops later tool dispatch and transcript publication.

use serde_json::{Map, Value, json};
use std::time::{Duration, Instant};

use runebender::automation::{agent, live};
use runebender::workflows::local_chat::{LocalChatEndpoint, LocalChatLimits};
use runebender::workflows::process::ProcessCancellation;

use super::ChatJob;

const MAX_TOOL_CALLS: usize = 6;
const MAX_COMPLETIONS: usize = 7;
const MAX_MESSAGES: usize = 64;
const MAX_CONTEXT_BYTES: usize = 1024 * 1024;
const ALLOWED_TOOLS: &[&str] = &[
    "project_info",
    "font_info",
    "read_glyph",
    "proof",
    "proposal_list",
    "propose_edits",
];

#[derive(Debug)]
struct RequestedTool {
    id: String,
    name: String,
    arguments: Map<String, Value>,
}

#[derive(Debug)]
struct AssistantReply {
    content: Option<String>,
    calls: Vec<RequestedTool>,
}

fn tools() -> Vec<agent::Tool> {
    live::tools()
        .into_iter()
        .filter(|tool| ALLOWED_TOOLS.contains(&tool.name.as_str()))
        .collect()
}

fn system_prompt() -> String {
    // The tool descriptions and schemas come from the shared agent/live
    // contract; the host alone decides which of them the model may execute.
    String::from(
        "You are a font engineering assistant in Runebender. Work only on the currently open \
         editor document through the supplied tools. Read project_info before choosing a source, \
         and read a glyph before stating its measurements or proposing edits. The source is a \
         stable ID from project_info; never guess a source or a glyph revision. You may read, \
         proof, and propose. A proposal is separate from installation; the person installs it. \
         You cannot install, apply, discard, save, run a program, or call another tool. \
         Give short, factual answers. If a tool reports an error, say what failed. \
         Use the supplied function tools when needed; never write tool-call markup in prose.",
    )
}

fn parse_reply(
    response: &Value,
    allowed: &[agent::Tool],
    namespace: &str,
) -> Result<AssistantReply, String> {
    let choices = response
        .get("choices")
        .and_then(Value::as_array)
        .ok_or("chat response has no choices array")?;
    if choices.len() != 1 {
        return Err("chat response must have exactly one choice".into());
    }
    let choice = &choices[0];
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or("chat response has no assistant message")?;
    if message.get("role").and_then(Value::as_str) != Some("assistant") {
        return Err("chat response role is not assistant".into());
    }
    let content = match message.get("content") {
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Null) | None => None,
        _ => return Err("assistant content must be text or null".into()),
    };
    let raw_calls = match message.get("tool_calls") {
        Some(Value::Array(calls)) => calls.as_slice(),
        None | Some(Value::Null) => &[],
        _ => return Err("assistant tool_calls must be an array".into()),
    };
    if raw_calls.len() > MAX_TOOL_CALLS {
        return Err(format!("chat exceeded {MAX_TOOL_CALLS} tool calls"));
    }
    let finish = choice.get("finish_reason").and_then(Value::as_str);
    if raw_calls.is_empty() && finish != Some("stop") {
        return Err("chat response did not finish normally".into());
    }
    if !raw_calls.is_empty() && finish != Some("tool_calls") {
        return Err("chat response has tool calls without tool_calls finish reason".into());
    }
    if raw_calls.is_empty() && content.as_deref().is_none_or(str::is_empty) {
        return Err("assistant response is empty".into());
    }
    let mut calls = Vec::with_capacity(raw_calls.len());
    let mut seen = std::collections::HashSet::new();
    for raw in raw_calls {
        let id = raw
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or("tool call has no ID")?;
        if id.len() > 128 || !seen.insert(id) {
            return Err("tool call ID is invalid or repeated within one completion".into());
        }
        if raw.get("type").and_then(Value::as_str) != Some("function") {
            return Err("unsupported tool call type".into());
        }
        let function = raw
            .get("function")
            .and_then(Value::as_object)
            .ok_or("tool call has no function")?;
        let name = function
            .get("name")
            .and_then(Value::as_str)
            .ok_or("tool call has no function name")?;
        let Some(tool) = allowed.iter().find(|tool| tool.name == name) else {
            return Err(format!("unsupported chat tool: {name}"));
        };
        let arguments = function
            .get("arguments")
            .and_then(Value::as_str)
            .ok_or("tool arguments must be JSON text")?;
        let arguments: Value = serde_json::from_str(arguments)
            .map_err(|error| format!("invalid {name} arguments: {error}"))?;
        let arguments = arguments
            .as_object()
            .ok_or_else(|| format!("{name} arguments must be a JSON object"))?;
        validate_arguments(tool, arguments)?;
        calls.push(RequestedTool {
            id: format!("turn_{namespace}_{id}"),
            name: name.into(),
            arguments: arguments.clone(),
        });
    }
    Ok(AssistantReply { content, calls })
}

fn validate_arguments(tool: &agent::Tool, args: &Map<String, Value>) -> Result<(), String> {
    let schema = &tool.parameters;
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for field in required.iter().filter_map(Value::as_str) {
            if !args.contains_key(field) {
                return Err(format!("{} requires {field}", tool.name));
            }
        }
    }
    // The live dispatcher owns detailed nested validation. Check the top level
    // before dispatching any call from this completion, including later calls.
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (field, value) in args {
            if field == "expected_document_epoch" {
                // The model can supply a value, but the host always replaces it.
                continue;
            }
            let Some(property) = properties.get(field) else {
                return Err(format!("{} does not accept {field}", tool.name));
            };
            let kind = property.get("type").and_then(Value::as_str);
            let valid = match kind {
                Some("string") => value.is_string(),
                Some("integer") => value.as_i64().is_some() || value.as_u64().is_some(),
                Some("number") => value.is_number(),
                Some("array") => value.is_array(),
                Some("object") => value.is_object(),
                Some("boolean") => value.is_boolean(),
                _ => true,
            };
            if !valid {
                return Err(format!("{} has an invalid {field}", tool.name));
            }
        }
    }
    Ok(())
}

fn check_messages(messages: &[Value]) -> Result<(), String> {
    if messages.len() > MAX_MESSAGES {
        return Err("chat context has too many messages; clear the chat to continue".into());
    }
    for message in messages {
        let role = message
            .get("role")
            .and_then(Value::as_str)
            .ok_or("chat context contains a message without a role")?;
        if !matches!(role, "user" | "assistant" | "tool") {
            return Err("chat context contains an unsupported role".into());
        }
    }
    let bytes = serde_json::to_vec(messages).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_CONTEXT_BYTES {
        return Err("chat context is too large; clear the chat to continue".into());
    }
    Ok(())
}

fn remaining(deadline: Instant) -> Result<Duration, String> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|remaining| !remaining.is_zero())
        .ok_or_else(|| "chat turn exceeded its 120-second deadline".into())
}

fn run_loop(
    conversation: &str,
    cancellation: &ProcessCancellation,
    epoch: &str,
    mut infer: impl FnMut(&Value, Duration) -> Result<Value, String>,
    mut call: impl FnMut(&str, &Value) -> Result<Value, String>,
    mut emit: impl FnMut(Value),
) -> Result<(), String> {
    if epoch.is_empty() {
        return Err("live document has no epoch".into());
    }
    let mut messages: Vec<Value> = serde_json::from_str(conversation)
        .map_err(|error| format!("invalid chat context: {error}"))?;
    let history_len = messages.len();
    let deadline = Instant::now() + LocalChatLimits::default().deadline;
    let allowed = tools();
    let api_tools: Vec<Value> = allowed
        .iter()
        .map(|tool| {
            json!({
                "type":"function", "function": {
                    "name":tool.name, "description":tool.description, "parameters":tool.parameters
                }
            })
        })
        .collect();
    let mut total_calls = 0;
    for turn in 0..MAX_COMPLETIONS {
        if cancellation.is_cancelled() {
            return Err("cancelled".into());
        }
        let inference_budget = remaining(deadline)?;
        check_messages(&messages)?;
        let mut request_messages = Vec::with_capacity(messages.len() + 1);
        request_messages.push(json!({"role":"system", "content":system_prompt()}));
        request_messages.extend(messages.iter().cloned());
        let request = json!({
            "messages":request_messages,
            "tools":api_tools.clone(),
            "tool_choice":"auto",
            "stream":false,
        });
        if serde_json::to_vec(&request)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_CONTEXT_BYTES
        {
            return Err("chat request is too large; clear the chat to continue".into());
        }
        let response = infer(&request, inference_budget)?;
        if cancellation.is_cancelled() {
            return Err("cancelled".into());
        }
        remaining(deadline)?;
        let reply = parse_reply(&response, &allowed, &format!("{history_len}_{turn}"))?;
        if reply.calls.is_empty() {
            remaining(deadline)?;
            let text = reply.content.expect("nonempty final content checked");
            messages.push(json!({"role":"assistant", "content":text}));
            check_messages(&messages)?;
            emit(json!({"event":"done", "text":text}));
            emit(json!({"event":"messages", "messages":messages}));
            return Ok(());
        }
        if total_calls + reply.calls.len() > MAX_TOOL_CALLS {
            return Err(format!("chat exceeded {MAX_TOOL_CALLS} tool calls"));
        }
        if turn + 1 == MAX_COMPLETIONS {
            return Err("chat exceeded the completion limit".into());
        }
        let call_messages: Vec<Value> = reply.calls.iter().map(|tool| json!({
            "id":tool.id, "type":"function", "function":{
                "name":tool.name, "arguments":Value::Object(tool.arguments.clone()).to_string()
            }
        })).collect();
        messages
            .push(json!({"role":"assistant", "content":reply.content, "tool_calls":call_messages}));
        for tool in reply.calls {
            if cancellation.is_cancelled() {
                return Err("cancelled".into());
            }
            remaining(deadline)?;
            let mut args = tool.arguments;
            // Always override a model-supplied epoch with this editor's own.
            args.insert("expected_document_epoch".into(), json!(epoch));
            emit(json!({"event":"tool_call", "name":tool.name}));
            let result = match call(&tool.name, &Value::Object(args)) {
                Ok(result) => result,
                Err(error) => {
                    emit(json!({"event":"tool_result", "name":tool.name, "ok":false,
                        "result":{"error":error}}));
                    return Err(error);
                }
            };
            if cancellation.is_cancelled() {
                return Err("cancelled".into());
            }
            // Live socket dispatch can take up to its own 30-second timeout.
            // Once dispatched it cannot be preempted; suppress later work.
            remaining(deadline)?;
            if result.get("document_epoch").and_then(Value::as_str) != Some(epoch) {
                emit(json!({"event":"tool_result", "name":tool.name, "ok":false,
                    "result":{"error":"live tool response has a different document epoch"}}));
                return Err("live tool response has a different document epoch".into());
            }
            let Some(ok) = result.get("ok").and_then(Value::as_bool) else {
                emit(json!({"event":"tool_result", "name":tool.name, "ok":false,
                    "result":{"error":"live tool response has no success status"}}));
                return Err("live tool response has no success status".into());
            };
            emit(json!({"event":"tool_result", "name":tool.name, "ok":ok, "result":result}));
            if !ok {
                let error = result
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("live tool rejected the request");
                return Err(format!("{}: {error}", tool.name));
            }
            messages
                .push(json!({"role":"tool", "tool_call_id":tool.id, "content":result.to_string()}));
            total_calls += 1;
            check_messages(&messages)?;
        }
    }
    Err("chat exceeded the completion limit".into())
}

pub(super) fn run_chat_endpoint(
    endpoint: &LocalChatEndpoint,
    session: &std::path::Path,
    epoch: &str,
    conversation: &str,
    job: &ChatJob,
) -> Result<(), String> {
    run_loop(
        conversation,
        &job.cancellation,
        epoch,
        |request, remaining| {
            endpoint
                .complete(
                    request,
                    LocalChatLimits {
                        deadline: remaining,
                        ..LocalChatLimits::default()
                    },
                    &job.cancellation,
                )
                .map_err(|error| error.to_string())
        },
        |name, args| {
            runebender::automation::live_socket::call(
                session,
                &agent::ToolCall {
                    name: name.into(),
                    arguments: args.clone(),
                },
            )
            .map_err(|error| format!("live {name} call failed: {error}; its outcome is unknown"))
        },
        |event| {
            job.events
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .push(event);
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    fn completion(text: &str) -> Value {
        json!({"choices":[{"message":{"role":"assistant","content":text},
            "finish_reason":"stop"}]})
    }

    fn call_reply(name: &str, args: Value) -> Value {
        json!({"choices":[{"message":{"role":"assistant","content":null,
            "tool_calls":[{"id":"call_0","type":"function","function":{
                "name":name,"arguments":args.to_string()}}]},"finish_reason":"tool_calls"}]})
    }

    #[test]
    fn repeated_turns_keep_context_and_namespace_ids() {
        let replies = RefCell::new(
            vec![
                call_reply("read_glyph", json!({"glyph":"a","source":0})),
                call_reply(
                    "propose_edits",
                    json!({"task":"chat", "reason":"narrow a",
                "edits":[], "expected_document_epoch":"model-chosen"}),
                ),
                completion("A proposal is ready for review."),
            ]
            .into_iter(),
        );
        let seen = RefCell::new(Vec::new());
        let events = RefCell::new(Vec::new());
        let result = run_loop(
            r#"[{"role":"user","content":"Read and propose a change to a"}]"#,
            &ProcessCancellation::default(),
            "host-epoch",
            |request, _| {
                seen.borrow_mut().push(request.clone());
                Ok(replies.borrow_mut().next().expect("a prepared response"))
            },
            |name, args| {
                assert_eq!(args["expected_document_epoch"], "host-epoch");
                Ok(json!({"ok":true,"document_epoch":"host-epoch","name":name}))
            },
            |event| events.borrow_mut().push(event),
        );
        assert!(result.is_ok(), "{result:?}");
        let seen = seen.into_inner();
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[1]["messages"][3]["tool_call_id"], "turn_1_0_call_0");
        assert_eq!(seen[2]["messages"][5]["tool_call_id"], "turn_1_1_call_0");
        let events = events.into_inner();
        assert_eq!(
            events
                .iter()
                .filter(|event| event["event"] == "tool_result")
                .count(),
            2
        );
        assert_eq!(events.last().expect("messages event")["event"], "messages");
    }

    #[test]
    fn invalid_and_unsupported_calls_fail_before_any_dispatch() {
        for reply in [
            call_reply("proposal_install", json!({"task":"chat"})),
            call_reply("nodes_run", json!({"file":"x.nodes.json"})),
            call_reply("read_glyph", json!({"glyph":4})),
            json!({"choices":[{"message":{"role":"user","content":"wrong role"},
                "finish_reason":"stop"}]}),
            json!({"choices":[{"message":{"role":"assistant","content":null,
                "tool_calls":[{"id":"c","type":"function","function":{
                    "name":"read_glyph","arguments":"not-json"}}]},
                "finish_reason":"tool_calls"}]}),
        ] {
            let dispatched = Cell::new(0);
            let result = run_loop(
                r#"[{"role":"user","content":"hello"}]"#,
                &ProcessCancellation::default(),
                "host-epoch",
                |_, _| Ok(reply.clone()),
                |_, _| {
                    dispatched.set(dispatched.get() + 1);
                    Ok(Value::Null)
                },
                |_| {},
            );
            assert!(result.is_err());
            assert_eq!(dispatched.get(), 0);
        }
    }

    #[test]
    fn sixth_call_is_last_allowed_and_failure_stops_without_retry() {
        let completed = Cell::new(0);
        let calls = Cell::new(0);
        let result = run_loop(
            r#"[{"role":"user","content":"inspect"}]"#,
            &ProcessCancellation::default(),
            "host-epoch",
            |_, _| {
                completed.set(completed.get() + 1);
                Ok(call_reply("project_info", json!({})))
            },
            |_, _| {
                calls.set(calls.get() + 1);
                Ok(json!({"ok":true,"document_epoch":"host-epoch"}))
            },
            |_| {},
        );
        assert!(result.unwrap_err().contains("6 tool calls"));
        assert_eq!(completed.get(), 7);
        assert_eq!(calls.get(), 6);

        let calls = Cell::new(0);
        let result = run_loop(
            r#"[{"role":"user","content":"inspect"}]"#,
            &ProcessCancellation::default(),
            "host-epoch",
            |_, _| Ok(call_reply("project_info", json!({}))),
            |_, _| {
                calls.set(calls.get() + 1);
                Err("uncertain live reply".into())
            },
            |_| {},
        );
        assert!(result.unwrap_err().contains("uncertain live reply"));
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn cancellation_before_dispatch_and_after_inference_publishes_nothing() {
        let cancellation = ProcessCancellation::default();
        cancellation.cancel();
        let inference = Cell::new(0);
        let result = run_loop(
            r#"[{"role":"user","content":"inspect"}]"#,
            &cancellation,
            "host-epoch",
            |_, _| {
                inference.set(inference.get() + 1);
                Ok(completion("answer"))
            },
            |_, _| unreachable!(),
            |_| unreachable!(),
        );
        assert_eq!(result.unwrap_err(), "cancelled");
        assert_eq!(inference.get(), 0);

        let cancellation = ProcessCancellation::default();
        let late = cancellation.clone();
        let dispatched = Cell::new(0);
        let published = Cell::new(0);
        let result = run_loop(
            r#"[{"role":"user","content":"inspect"}]"#,
            &cancellation,
            "host-epoch",
            |_, _| {
                late.cancel();
                Ok(call_reply("project_info", json!({})))
            },
            |_, _| {
                dispatched.set(dispatched.get() + 1);
                Ok(Value::Null)
            },
            |_| published.set(published.get() + 1),
        );
        assert_eq!(result.unwrap_err(), "cancelled");
        assert_eq!(dispatched.get(), 0);
        assert_eq!(published.get(), 0);
    }
}

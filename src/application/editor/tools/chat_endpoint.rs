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
    "design_context",
    "project_info",
    "font_info",
    "editor_context",
    "glyph_inventory",
    "analyze_glyph",
    "analyze_font",
    "read_glyph",
    "read_kerning",
    "proof",
    "specimen",
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
        .map(|mut tool| {
            // The chat has no experiment tools, so a branch parameter is only
            // a field for a small model to fill with a guess.
            if let Some(properties) = tool.parameters["properties"].as_object_mut() {
                properties.remove("branch");
                if properties.contains_key("layer") && tool.name != "design_context" {
                    properties["layer"] = json!({"type":"string",
                        "description":"Only to read a proposal layer by its name from proposal_list. Omit for the glyph itself."});
                }
            }
            tool
        })
        .collect()
}

/// How much of the design context the local model gets in its prompt, in
/// characters: about five thousand tokens of a 16k window, leaving room for the
/// tools, the conversation and the answer.
pub(super) const CONTEXT_BUDGET_CHARS: usize = 20_000;

fn system_prompt(context: &str) -> String {
    let mut prompt = base_prompt();
    if !context.trim().is_empty() {
        prompt.push_str(
            "\n\nThe design context below is how this font is drawn and how fonts are kept \
             sound. Reason from it when a request asks for a change: name the structure the \
             request means, measure it, make the smallest edit that does the job as a \
             proposal, and measure again. Where layers disagree the later one wins.\n\n",
        );
        prompt.push_str(context);
    }
    prompt
}

fn base_prompt() -> String {
    // The tool descriptions and schemas come from the shared agent/live
    // contract; the host alone decides which of them the model may execute.
    String::from(
        "You are a font engineering assistant in Runebender. Work only on the currently open \
         editor document through the supplied tools. Read project_info before choosing a source, \
         and measure before judging: analyze_glyph returns a glyph's stem, bar, kinks, grid fit, \
         mark label and compatibility with the other sources, analyze_font does that for a whole \
         source, and read_glyph gives the raw points only when a finding needs them. Quote the \
         numbers the tools return; never compute a stem or a width from points yourself. The \
         source is a stable ID from project_info; never guess a source or a glyph revision. You \
         may read, measure, proof, and propose. A proposal is separate from installation; the person installs it. \
         You cannot install, apply, discard, save, run a program, or call another tool. \
         Answer in one to three sentences with the numbers the tools returned. \
         To make a glyph wider by N units, move its right-hand points with transform_points and add \
         N to the advance with adjust_width; set_width replaces the whole advance and is for a \
         requested absolute width. After propose_edits, report its changes field as the result. \
         If a tool reports an error, correct the call and try again \
         once (glyph_inventory lists the real glyph names); only then say what failed. \
         Use the supplied function tools when needed; never write tool-call markup in prose.",
    )
}

/// The longest plain answer, in characters. The grammar enforces it, so
/// the model has to say the result first and leave the rest for the detail.
const PLAIN_ANSWER_MAX: usize = 280;

/// The prompt for the restating step: the model's own reply, which names
/// tools, IDs and hashes, said again for the designer who asked.
const PLAIN_PROMPT: &str = "You are rewriting a font assistant's reply for the type designer \
    who asked. Give answer: one or two plain sentences that state the result and the numbers \
    that matter, in a designer's words, with no tool names, IDs, hashes or JSON. Give detail: \
    what the answer leaves out, as plain sentences: each measurement, each step taken, and \
    anything the designer should check.";

/// A request that restates `draft`, the model's final reply, as a plain
/// answer and a detail. It carries the conversation but no tools: the
/// server constrains the whole output to the schema, so a tool call is not
/// possible in the same request, which is why this is a step of its own.
fn plain_answer_request(history: &[Value], draft: &str) -> Value {
    let mut messages = vec![json!({"role":"system", "content":PLAIN_PROMPT})];
    // The first message is the tool-using system prompt; the restating
    // prompt stands in for it.
    messages.extend(history.iter().skip(1).cloned());
    messages.push(json!({"role":"assistant", "content":draft}));
    messages.push(json!({"role":"user", "content":"Restate your last reply for the designer."}));
    json!({
        "messages":messages,
        "stream":false,
        "temperature":0.2,
        "response_format":{"type":"json_schema", "json_schema":{"name":"plain_answer", "schema":{
            "type":"object",
            "properties":{
                "answer":{"type":"string", "maxLength":PLAIN_ANSWER_MAX},
                "detail":{"type":"string"}
            },
            "required":["answer", "detail"]
        }}}
    })
}

/// The answer and detail from a restating response, or `None` when the
/// response is not the schema's object or its answer is empty.
fn parse_plain_answer(response: &Value) -> Option<(String, String)> {
    let content = response
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_str()?
        .trim();
    let content = content
        .strip_prefix("```json")
        .or_else(|| content.strip_prefix("```"))
        .and_then(|rest| rest.strip_suffix("```"))
        .unwrap_or(content);
    let value: Value = serde_json::from_str(content.trim()).ok()?;
    let answer = value.get("answer")?.as_str()?.trim().to_owned();
    let detail = value
        .get("detail")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    (!answer.is_empty()).then_some((answer, detail))
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
    if finish == Some("length") {
        return Err("chat response was cut off at the model's context limit".into());
    }
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
    context: &str,
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
    // Earlier turns keep what was asked and answered. Their tool calls and
    // tool results were the work behind an answer, not the conversation, and
    // one glyph's points can be more than the model's window holds.
    messages.retain(|message| {
        let role = message.get("role").and_then(Value::as_str);
        role == Some("user")
            || role == Some("assistant")
                && message.get("tool_calls").is_none_or(Value::is_null)
                && message.get("content").and_then(Value::as_str).is_some()
    });
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
    // The revision of every glyph a tool returned in this run, by name.
    let mut revisions: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for turn in 0..MAX_COMPLETIONS {
        if cancellation.is_cancelled() {
            return Err("cancelled".into());
        }
        let inference_budget = remaining(deadline)?;
        check_messages(&messages)?;
        let mut request_messages = Vec::with_capacity(messages.len() + 1);
        request_messages.push(json!({"role":"system", "content":system_prompt(context)}));
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
            // The model's own reply stays in the history, so a follow-up
            // question has everything it said. The person sees it restated
            // in plain words, with the rest as a detail; if restating fails
            // for any reason, they see the reply itself.
            let plain = remaining(deadline)
                .ok()
                .and_then(|budget| {
                    infer(&plain_answer_request(&request_messages, &text), budget).ok()
                })
                .and_then(|response| parse_plain_answer(&response));
            if cancellation.is_cancelled() {
                return Err("cancelled".into());
            }
            messages.push(json!({"role":"assistant", "content":text}));
            check_messages(&messages)?;
            match plain {
                Some((answer, detail)) => emit(json!({
                    "event":"done", "text":answer, "detail":detail, "draft":text
                })),
                None => emit(json!({"event":"done", "text":text})),
            }
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
            if tool.name == "propose_edits" {
                unique_task_name(&mut args);
                merge_edits_by_glyph(&mut args);
                restore_read_revisions(&mut args, &revisions);
            }
            emit(json!({"event":"tool_call", "name":tool.name, "arguments":args}));
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
            if let (Some(glyph), Some(revision)) = (
                result.get("glyph").and_then(Value::as_str),
                result.get("revision").and_then(Value::as_str),
            ) {
                revisions.insert(glyph.to_owned(), revision.to_owned());
            }
            // A tool that says no, such as a glyph that is not in the font,
            // is an answer the model can act on: it goes back as the tool's
            // result, and the model can retry with another name or say so.
            messages
                .push(json!({"role":"tool", "tool_call_id":tool.id, "content":result.to_string()}));
            total_calls += 1;
            check_messages(&messages)?;
        }
    }
    Err("chat exceeded the completion limit".into())
}

/// A glyph revision is a 76-character hash, and a small model copying one
/// back by hand drops or changes characters, so every proposal it wrote was
/// refused as stale. The guard is about reading before proposing, not about
/// transcription: for each edit whose glyph a tool returned in this run, the
/// revision the tool returned replaces the one the model typed. A glyph the
/// model never read keeps its typed revision and the guard still applies.
fn restore_read_revisions(
    args: &mut Map<String, Value>,
    revisions: &std::collections::HashMap<String, String>,
) {
    let Some(edits) = args.get_mut("edits").and_then(Value::as_array_mut) else {
        return;
    };
    for edit in edits {
        let Some(read) = edit
            .get("glyph")
            .and_then(Value::as_str)
            .and_then(|glyph| revisions.get(glyph))
        else {
            continue;
        };
        if let Some(edit) = edit.as_object_mut() {
            edit.insert("expected_revision".into(), json!(read));
        }
    }
}

/// A proposal task name must be new, and a model asked for the same thing
/// twice gives it the same name twice. A short time stamp makes each one its
/// own; the task the model chose stays readable in front of it.
fn unique_task_name(args: &mut Map<String, Value>) {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() % 100_000);
    let task = args
        .get("task")
        .and_then(Value::as_str)
        .map_or("edit", str::trim)
        .to_owned();
    let task = if task.is_empty() {
        "edit".to_owned()
    } else {
        task
    };
    args.insert("task".into(), json!(format!("{task}-{stamp}")));
}

/// A batch names each glyph once, and a small model asked for two changes to
/// one glyph writes two entries for it instead of one entry with two
/// operations. The entries mean the same thing in order, so they are joined:
/// the first entry keeps its revision and gains the later operations.
fn merge_edits_by_glyph(args: &mut Map<String, Value>) {
    let Some(edits) = args.get_mut("edits").and_then(Value::as_array_mut) else {
        return;
    };
    let mut merged: Vec<Value> = Vec::with_capacity(edits.len());
    for edit in edits.drain(..) {
        let glyph = edit.get("glyph").and_then(Value::as_str).map(str::to_owned);
        let earlier = glyph.as_deref().and_then(|glyph| {
            merged
                .iter_mut()
                .find(|done| done.get("glyph").and_then(Value::as_str) == Some(glyph))
        });
        match (earlier, edit.get("operations").and_then(Value::as_array)) {
            (Some(earlier), Some(operations)) => {
                if let Some(target) = earlier.get_mut("operations").and_then(Value::as_array_mut) {
                    target.extend(operations.iter().cloned());
                }
            }
            _ => merged.push(edit),
        }
    }
    *edits = merged;
}

pub(super) fn run_chat_endpoint(
    endpoint: &LocalChatEndpoint,
    session: &std::path::Path,
    epoch: &str,
    conversation: &str,
    context: &str,
    job: &ChatJob,
) -> Result<(), String> {
    run_loop(
        conversation,
        context,
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
    fn a_revision_the_model_mistyped_is_replaced_by_the_one_it_read() {
        let mut revisions = std::collections::HashMap::new();
        revisions.insert("zero".to_owned(), "glif-sha256:real".to_owned());
        let mut args = json!({"task":"wider","reason":"r","edits":[
            {"glyph":"zero","expected_revision":"glif-sha256:rea","operations":[]},
            {"glyph":"one","expected_revision":"typed","operations":[]}
        ]})
        .as_object()
        .cloned()
        .unwrap();
        restore_read_revisions(&mut args, &revisions);
        assert_eq!(args["edits"][0]["expected_revision"], "glif-sha256:real");
        assert_eq!(args["edits"][1]["expected_revision"], "typed");
    }

    #[test]
    fn two_entries_for_one_glyph_become_one_entry_with_both_operations() {
        let mut args = json!({"task":"wider","reason":"r","edits":[
            {"glyph":"zero","expected_revision":"a","operations":[{"op":"transform_points","dx":64.0}]},
            {"glyph":"one","expected_revision":"b","operations":[{"op":"adjust_width","delta":1.0}]},
            {"glyph":"zero","expected_revision":"a","operations":[{"op":"adjust_width","delta":64.0}]}
        ]})
        .as_object()
        .cloned()
        .unwrap();
        merge_edits_by_glyph(&mut args);
        let edits = args["edits"].as_array().unwrap();
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0]["glyph"], "zero");
        assert_eq!(edits[0]["operations"].as_array().unwrap().len(), 2);
        assert_eq!(edits[0]["operations"][1]["op"], "adjust_width");
        assert_eq!(edits[1]["glyph"], "one");
    }

    #[test]
    fn a_task_name_gets_a_stamp_so_the_same_request_can_run_twice() {
        let mut args = json!({"task":"wider","edits":[]})
            .as_object()
            .cloned()
            .unwrap();
        unique_task_name(&mut args);
        let task = args["task"].as_str().unwrap().to_owned();
        assert!(task.starts_with("wider-"), "{task}");
        assert!(task["wider-".len()..].chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn chat_tools_carry_no_experiment_branch() {
        for tool in tools() {
            let properties = tool.parameters["properties"].as_object().unwrap();
            assert!(!properties.contains_key("branch"), "{}", tool.name);
        }
        assert!(tools().iter().any(|tool| tool.name == "analyze_glyph"));
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
                completion(r#"{"answer":"A proposal is ready.","detail":"It narrows a."}"#),
            ]
            .into_iter(),
        );
        let seen = RefCell::new(Vec::new());
        let events = RefCell::new(Vec::new());
        let result = run_loop(
            r#"[{"role":"user","content":"Read and propose a change to a"}]"#,
            "",
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
        assert_eq!(seen.len(), 4, "three turns and one restating request");
        assert_eq!(seen[1]["messages"][3]["tool_call_id"], "turn_1_0_call_0");
        assert_eq!(seen[2]["messages"][5]["tool_call_id"], "turn_1_1_call_0");
        // The restating request carries the conversation and the reply, a
        // schema, and no tools.
        assert!(seen[3].get("tools").is_none(), "no tools while restating");
        assert_eq!(
            seen[3]["response_format"]["json_schema"]["schema"]["required"],
            json!(["answer", "detail"])
        );
        assert_eq!(
            seen[3]["messages"][6]["content"],
            "A proposal is ready for review."
        );
        let events = events.into_inner();
        let done = events
            .iter()
            .find(|event| event["event"] == "done")
            .expect("a done event");
        assert_eq!(done["text"], "A proposal is ready.");
        assert_eq!(done["detail"], "It narrows a.");
        assert_eq!(done["draft"], "A proposal is ready for review.");
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
    fn a_reply_the_server_cannot_restate_is_shown_as_it_is() {
        let replies = RefCell::new(vec![completion("The stem is 92 units.")].into_iter());
        let events = RefCell::new(Vec::new());
        let result = run_loop(
            r#"[{"role":"user","content":"How wide is the stem?"}]"#,
            "",
            &ProcessCancellation::default(),
            "host-epoch",
            |_, _| {
                replies
                    .borrow_mut()
                    .next()
                    .ok_or_else(|| "server went away".to_owned())
            },
            |_, _| unreachable!("no tool is called"),
            |event| events.borrow_mut().push(event),
        );
        assert!(result.is_ok(), "{result:?}");
        let events = events.into_inner();
        let done = &events[0];
        assert_eq!(done["text"], "The stem is 92 units.");
        assert!(done.get("detail").is_none(), "nothing was restated");
    }

    #[test]
    fn a_plain_answer_is_read_from_the_schema_object_with_or_without_a_fence() {
        let bare = completion(r#"{"answer":"The stem is 92.","detail":"H measured."}"#);
        assert_eq!(
            parse_plain_answer(&bare),
            Some(("The stem is 92.".to_owned(), "H measured.".to_owned()))
        );
        let fenced = completion("```json\n{\"answer\":\"The stem is 92.\",\"detail\":\"\"}\n```");
        assert_eq!(
            parse_plain_answer(&fenced),
            Some(("The stem is 92.".to_owned(), String::new()))
        );
        assert_eq!(parse_plain_answer(&completion("not json")), None);
        assert_eq!(
            parse_plain_answer(&completion(r#"{"answer":"  ","detail":"x"}"#)),
            None
        );
    }

    #[test]
    fn invalid_and_unsupported_calls_fail_before_any_dispatch() {
        for reply in [
            call_reply("proposal_install", json!({"task":"chat"})),
            call_reply("graph_run", json!({"file":"x.graph.json"})),
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
                "",
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
            "",
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
            "",
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
            "",
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
            "",
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

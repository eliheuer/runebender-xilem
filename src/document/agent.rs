// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The harness a language model works the font through.
//!
//! A model never edits the font. It reads, it proofs, it proposes,
//! and a person installs. Everything a model can do is one of the
//! [`tools`] below, and every tool is a command this crate's binary
//! already runs, so the model's reach is exactly the command line's
//! and no wider. Install is not a tool on purpose: the proposal layer
//! is the model's side of the table, the foreground is the person's.
//!
//! The chat runtime lives in `font-ml`, which asks this crate for the
//! prompt and the tool list (`runebender-core agent tools --json`) and
//! runs each call back through it (`runebender-core agent call`). An
//! outside harness such as OMP reads the same definitions, so the two
//! ways of driving the editor cannot drift apart.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// One thing a model may ask for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Tool {
    /// The name the model calls it by.
    pub name: String,
    /// What it does, for the model.
    pub description: String,
    /// JSON Schema of the arguments.
    pub parameters: Value,
}

/// A call the model made.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToolCall {
    /// The tool.
    pub name: String,
    /// Its arguments, as the model wrote them.
    #[serde(default)]
    pub arguments: Value,
}

/// What a tool gave back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ToolResult {
    /// The tool.
    pub name: String,
    /// Whether it ran.
    pub ok: bool,
    /// The JSON it returned, or an error object.
    pub result: Value,
}

fn params(props: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": props,
        "required": required,
    })
}

/// Every tool, in the order a prompt lists them.
pub fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "font_info".into(),
            description: "The open font: family, style, metrics, glyph count, \
                          proposals waiting. Call this first."
                .into(),
            parameters: params(json!({}), &[]),
        },
        Tool {
            name: "read_glyph".into(),
            description: "One glyph's outline: advance, contours as points with \
                          their types, components, anchors."
                .into(),
            parameters: params(
                json!({ "glyph": { "type": "string", "description": "The glyph name." } }),
                &["glyph"],
            ),
        },
        Tool {
            name: "proof".into(),
            description: "Write an SVG proof sheet of glyphs and return per-glyph \
                          metrics: advance, sidebearings, bounds, point counts."
                .into(),
            parameters: params(
                json!({
                    "glyphs": { "type": "array", "items": { "type": "string" },
                                "description": "Glyph names. Empty means every drawn glyph." }
                }),
                &[],
            ),
        },
        Tool {
            name: "propose".into(),
            description: "Run a local model task (for example bolden) over glyphs. \
                          The result lands as a proposal layer the person can \
                          install or discard; nothing in the font changes."
                .into(),
            parameters: params(
                json!({
                    "task": { "type": "string", "description": "The task name, as font-ml lists it." },
                    "model": { "type": "string", "description": "The model directory name." },
                    "glyphs": { "type": "array", "items": { "type": "string" },
                                "description": "Glyph names. Empty means every drawn glyph." }
                }),
                &["task", "model"],
            ),
        },
        Tool {
            name: "nodes_run".into(),
            description: "Run a saved node workflow file over the font. Steps whose \
                          inputs have not changed are skipped."
                .into(),
            parameters: params(
                json!({
                    "file": { "type": "string", "description": "The .nodes.json path." },
                    "glyphs": { "type": "array", "items": { "type": "string" } }
                }),
                &["file"],
            ),
        },
        Tool {
            name: "proposal_list".into(),
            description: "The proposals waiting in the font, by task, with glyph counts.".into(),
            parameters: params(json!({}), &[]),
        },
        Tool {
            name: "proposal_discard".into(),
            description: "Drop a waiting proposal without installing it.".into(),
            parameters: params(
                json!({ "task": { "type": "string", "description": "The task whose proposal to drop." } }),
                &["task"],
            ),
        },
        Tool {
            name: "docs".into(),
            description: "Search the font engineering documentation on this machine \
                          (UFO, designspace, fontc, Runebender) and return the \
                          passages that match."
                .into(),
            parameters: params(
                json!({ "query": { "type": "string", "description": "Words to look for." } }),
                &["query"],
            ),
        },
    ]
}

/// The system prompt, with the tools written into it in the form the
/// model emits them back. Plain JSON in tags, which every chat model
/// can produce and which needs no template support.
pub fn system_prompt(tools: &[Tool]) -> String {
    let mut out = String::from(
        "You are a font engineering assistant inside Runebender, an open-source \
         font editor. You work on the font the person has open, on their machine, \
         and nothing leaves it.\n\n\
         Rules:\n\
         - You cannot edit the font. You read it, proof it, and propose changes \
         with a tool. A proposal is a layer the person installs or discards; say \
         so when you propose.\n\
         - Call font_info before anything else, and read a glyph before you \
         talk about its shape.\n\
         - Be brief and concrete. Give numbers from the tools, not guesses. \
         Glyph names are as the font has them (for example 'a', 'Aacute', \
         'hah-ar').\n\
         - When you do not know, say so.\n\n\
         Tools. To call one, write exactly one block like this and nothing after \
         it, then wait for the result:\n\
         <tool_call>\n{\"name\": \"font_info\", \"arguments\": {}}\n</tool_call>\n\n",
    );
    for t in tools {
        out.push_str(&format!(
            "- {}: {} Arguments: {}\n",
            t.name,
            t.description,
            serde_json::to_string(&t.parameters["properties"]).unwrap_or_default()
        ));
    }
    out
}

/// Every `<tool_call>` block in a reply, in order. A block that is not
/// valid JSON is skipped.
pub fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("<tool_call>") {
        let after = &rest[start + "<tool_call>".len()..];
        let Some(end) = after.find("</tool_call>") else {
            // An unterminated block: the model may have stopped early;
            // try it as it stands.
            if let Ok(c) = serde_json::from_str::<ToolCall>(after.trim()) {
                calls.push(c);
            }
            break;
        };
        if let Ok(c) = serde_json::from_str::<ToolCall>(after[..end].trim()) {
            calls.push(c);
        }
        rest = &after[end + "</tool_call>".len()..];
    }
    calls
}

/// The reply with its tool blocks removed, for showing.
pub fn strip_tool_calls(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<tool_call>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</tool_call>") {
            Some(end) => rest = &rest[start + end + "</tool_call>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// A glyph name list argument as the CLI takes it.
pub fn glyph_list(args: &Value) -> Vec<String> {
    args.get("glyphs")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_is_not_a_tool() {
        let names: Vec<String> = tools().into_iter().map(|t| t.name).collect();
        assert!(!names.iter().any(|n| n.contains("install")));
        assert!(names.contains(&"propose".to_string()));
    }

    #[test]
    fn tool_calls_parse_and_strip() {
        let text =
            "Let me look.\n<tool_call>\n{\"name\": \"font_info\", \"arguments\": {}}\n</tool_call>";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "font_info");
        assert_eq!(strip_tool_calls(text), "Let me look.");
        let open = "<tool_call>{\"name\":\"read_glyph\",\"arguments\":{\"glyph\":\"a\"}}";
        assert_eq!(parse_tool_calls(open)[0].arguments["glyph"], "a");
        assert!(parse_tool_calls("no call").is_empty());
    }

    #[test]
    fn the_prompt_names_every_tool() {
        let t = tools();
        let p = system_prompt(&t);
        for tool in &t {
            assert!(p.contains(&format!("- {}:", tool.name)));
        }
        assert!(p.contains("cannot edit"));
    }
}

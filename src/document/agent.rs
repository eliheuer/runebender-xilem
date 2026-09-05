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
    let mut tools = vec![
        Tool {
            name: "font_info".into(),
            description: "Returns the open font's family and style names, units per \
                          em, ascender, descender, x-height, cap height, the glyph \
                          count, and the proposals waiting. Use it after project_info \
                          with the selected master for questions about that master."
                .into(),
            parameters: params(json!({}), &[]),
        },
        Tool {
            name: "read_glyph".into(),
            description: "Returns one glyph's numbers: advance width, left and right \
                          sidebearings, bounds, point and contour counts, the contours \
                          as points with their types, components, and anchors. Use it \
                          for any question about a glyph's width, spacing, points, \
                          shape, or anchors; the answer is in its result."
                .into(),
            parameters: params(
                json!({ "glyph": { "type": "string", "description": "The glyph name." } }),
                &["glyph"],
            ),
        },
        Tool {
            name: "proof".into(),
            description: "Writes an SVG proof sheet of several glyphs and returns \
                          per-glyph metrics: advance, sidebearings, bounds, point \
                          counts. Use it to compare several glyphs at once or when the \
                          person wants a proof to look at; for one glyph use read_glyph."
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
            description: "Runs a local model task (for example bolden, which predicts \
                          a heavier master) over glyphs with a model from the models \
                          directory. Returns the proposal layer name and per-glyph \
                          rows. The foreground stays unchanged: the result waits as a \
                          proposal layer, and the person installs or discards it in \
                          the editor. Use it when asked to propose, predict, bolden, \
                          or run a model."
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
            description: "Runs a saved node workflow file (.nodes.json) over the font \
                          and returns each node's status and report. Steps whose \
                          inputs have not changed are skipped. Use it only when the \
                          person names a workflow file."
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
            description: "Returns the proposals waiting in the font, by task, with \
                          glyph counts. Use it when asked what is waiting or pending."
                .into(),
            parameters: params(json!({}), &[]),
        },
        Tool {
            name: "proposal_discard".into(),
            description: "Drops a waiting proposal without installing it. Use it only \
                          when the person asks to discard or remove a proposal."
                .into(),
            parameters: params(
                json!({ "task": { "type": "string", "description": "The task whose proposal to drop." } }),
                &["task"],
            ),
        },
        Tool {
            name: "docs".into(),
            description: "Searches the font engineering documentation on this machine \
                          (the UFO and designspace specs, fontc, Runebender) and \
                          returns the passages that match. Use it for any question \
                          about a format, a spec, an attribute, or a term, and quote \
                          what it returns."
                .into(),
            parameters: params(
                json!({ "query": { "type": "string", "description": "Words to look for." } }),
                &["query"],
            ),
        },
    ];
    tools.insert(0, Tool {
        name: "project_info".into(),
        description: "List the project's masters and their indices. Call first; select a master explicitly for every font operation in a family.".into(),
        parameters: params(json!({}), &[]),
    });
    tools.push(Tool {
        name: "propose_edits".into(),
        description: "Create a new proposal from exact glyph edits in font units. Read each foreground glyph first and supply its revision. Validates the entire batch before writing; does not install. Use a unique task name and explain the design intent in reason.".into(),
        parameters: serde_json::to_value(schemars::schema_for!(crate::document::edit_batch::EditBatch)).expect("schema serializes"),
    });
    for tool in &mut tools {
        if !matches!(tool.name.as_str(), "project_info" | "docs") {
            tool.parameters["properties"]["master"] = json!({"type": "integer", "minimum": 0,
                "description": "Master index from project_info. Required for multi-master projects."});
        }
        if matches!(tool.name.as_str(), "read_glyph" | "proof") {
            tool.parameters["properties"]["layer"] = json!({"type": "string",
                "description": "Optional UFO layer name, for example the proposal layer. Omit for foreground."});
        }
    }
    tools
}

/// The system prompt, with the tools written into it in the form the
/// model emits them back. Plain JSON in tags, which every chat model
/// can produce and which needs no template support.
pub fn system_prompt(tools: &[Tool]) -> String {
    let mut out = String::from(
        "You are a font engineering assistant inside Runebender, an open-source \
         font editor. You work on the font the person has open, on their machine, \
         through the configured model provider. Local providers can run offline; remote providers receive the tool context sent by their host.\n\n\
         Rules:\n\
         - You cannot edit the font. You read it, proof it, and propose changes \
         with a tool. A proposal is a layer the person installs or discards in the \
         editor. After a propose call, name the proposal layer and say that the \
         person installs it; never say it was installed and never offer to install.\n\
         - Read before you answer. For any question about a glyph's width, advance, \
         sidebearings, spacing, points, contours, anchors, or shape, call read_glyph \
         on that glyph first and answer only with numbers from its result. Never \
         answer a geometry question from font_info or from memory.\n\
         - For any question about a format, a spec, an attribute, or a term (UFO, \
         glif, designspace, fontc, OpenType), call docs first and quote what it \
         returns.\n\
         - Call project_info first, then font_info with the chosen master. Never guess which master the person means.\n\
         - Be brief and concrete. Give numbers from the tools, not guesses. Glyph \
         names are as the font has them (for example 'a', 'Aacute', 'hah-ar'); \
         a capital letter's glyph name is the letter itself ('H').\n\
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
            serde_json::to_string(&t.parameters).unwrap_or_default()
        ));
    }
    out.push_str(
        "\nExample. Person: How wide is the n? You: <tool_call>\n\
         {\"name\": \"read_glyph\", \"arguments\": {\"glyph\": \"n\"}}\n</tool_call>\n\
         Result: {\"advance\": 596, \"lsb\": 72, \"rsb\": 24, ...}. You: The n is 596 \
         units wide, with a left sidebearing of 72 and a right sidebearing of 24.\n",
    );
    out
}

/// Whether a question is about a glyph's geometry, so the loop can
/// insist on a read before an answer. Word-based and generous on
/// purpose: a false positive costs one tool call, a miss costs a
/// guessed number.
pub fn asks_geometry(question: &str) -> bool {
    let q = question.to_lowercase();
    [
        "wide",
        "width",
        "advance",
        "sidebearing",
        "side bearing",
        "lsb",
        "rsb",
        "spacing",
        "points",
        "contour",
        "anchor",
        "shape",
        "outline",
        "bounds",
        "how tall",
        "height of",
    ]
    .iter()
    .any(|w| q.contains(w))
}

/// Whether a question is about a format or a term, so the loop can
/// fetch documentation before the model answers.
pub fn asks_docs(question: &str) -> bool {
    let q = question.to_lowercase();
    [
        "spec",
        "ufo",
        "glif",
        "designspace",
        "fontc",
        "opentype",
        "attribute",
        "what does",
        "what is a",
        "what is the",
        "mean",
        "documentation",
    ]
    .iter()
    .any(|w| q.contains(w))
}

/// The one-line nudge the loop sends when the model answered a
/// geometry question without reading the glyph.
pub const READ_FIRST_NUDGE: &str = "You answered without reading the glyph. Call read_glyph on it now and answer \
     only from the result.";

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

    #[test]
    fn the_prompt_carries_the_rules_and_the_example() {
        let p = system_prompt(&tools());
        assert!(p.contains("Read before you answer"));
        assert!(p.contains("call docs first"));
        assert!(p.contains("never say it was installed"));
        assert!(p.contains("Example. Person: How wide is the n?"));
        // The example calls the tool the rule names.
        assert!(p.contains("\"name\": \"read_glyph\""));
    }

    #[test]
    fn questions_are_sorted_by_what_they_need() {
        assert!(asks_geometry(
            "How wide is the H, and what are its sidebearings?"
        ));
        assert!(asks_geometry("how many points does the a have"));
        assert!(!asks_geometry(
            "What font is open and how many glyphs does it have?"
        ));
        assert!(asks_docs(
            "What does the UFO spec say about the smooth attribute on a point?"
        ));
        assert!(!asks_docs(
            "Propose a bolder H with the virtua-12m-bolden model"
        ));
    }
}

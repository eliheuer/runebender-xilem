// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Shared transport descriptions of existing automation operations.
//!
//! These contracts describe effects and selected result shapes; they do not grant
//! permission or replace the typed engine and application request validation.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::agent;

/// The host on which a tool is called.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSurface {
    /// A font loaded from disk by a headless command.
    Disk,
    /// An unsaved document owned by an editor process.
    Live,
}

/// Observable effects of a tool call, including effects outside the font.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ToolEffects {
    /// Whether the call may alter canonical font data or its proposal layers.
    pub mutates_document: bool,
    /// Whether the call may alter editor, graph, job, or connection state.
    pub mutates_session: bool,
    /// Whether the call may create, change, or remove files.
    pub writes_files: bool,
    /// Whether the call may execute a local model, script, or workflow program.
    pub runs_local_code: bool,
    /// Whether the call may discard or replace existing data or retained artifacts.
    pub destructive: bool,
    /// Whether exact retries are stable while a document and its retry receipt are retained.
    pub idempotent: bool,
    /// Whether the call can interact with open-ended local programs or output paths.
    pub open_world: bool,
}

impl ToolEffects {
    const fn read() -> Self {
        Self {
            mutates_document: false,
            mutates_session: false,
            writes_files: false,
            runs_local_code: false,
            destructive: false,
            idempotent: true,
            open_world: false,
        }
    }

    const fn document() -> Self {
        Self {
            mutates_document: true,
            idempotent: false,
            ..Self::read()
        }
    }

    const fn session() -> Self {
        Self {
            mutates_session: true,
            idempotent: false,
            ..Self::read()
        }
    }

    const fn file() -> Self {
        Self {
            writes_files: true,
            idempotent: false,
            open_world: true,
            ..Self::read()
        }
    }

    const fn destructive(self) -> Self {
        Self {
            destructive: true,
            ..self
        }
    }

    const fn idempotent(self) -> Self {
        Self {
            idempotent: true,
            ..self
        }
    }

    const fn local_code(self) -> Self {
        Self {
            runs_local_code: true,
            writes_files: true,
            destructive: true,
            open_world: true,
            ..self
        }
    }

    /// True when the operation has no known font, session, or filesystem mutation.
    pub const fn is_read_only(self) -> bool {
        !self.mutates_document
            && !self.mutates_session
            && !self.writes_files
            && !self.runs_local_code
    }
}

/// One existing tool with versioned transport metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// The original argument schema and description, unchanged.
    pub tool: agent::Tool,
    /// Version of this descriptor's metadata contract.
    pub contract_version: u32,
    /// The host whose implementation the effects and result shape describe.
    pub surface: ToolSurface,
    /// Absent for an unrecognized operation; callers should omit effect claims.
    pub effects: Option<ToolEffects>,
    /// A concrete success-or-error result schema, when verified.
    pub output_schema: Option<Value>,
}

impl ToolDescriptor {
    /// MCP tool annotations derived from actual effects, or empty for an unknown operation.
    pub fn annotations(&self) -> Value {
        self.effects.map_or_else(
            || json!({}),
            |effects| {
                json!({
                    "readOnlyHint": effects.is_read_only(),
                    "destructiveHint": effects.destructive,
                    "idempotentHint": effects.idempotent,
                    "openWorldHint": effects.open_world,
                })
            },
        )
    }
}

/// Describe one existing tool without changing its input contract.
pub fn describe(tool: agent::Tool, surface: ToolSurface) -> ToolDescriptor {
    let effects = effects_for(&tool.name, surface);
    let output_schema = match (tool.name.as_str(), surface) {
        ("project_info" | "editor_connect", ToolSurface::Live) => Some(live_project_info_schema()),
        ("project_info", ToolSurface::Disk) => Some(disk_project_info_schema()),
        ("editor_sessions", ToolSurface::Live) => Some(editor_sessions_schema()),
        ("export_proof", ToolSurface::Live) => Some(export_proof_schema()),
        _ => None,
    };
    ToolDescriptor {
        tool,
        contract_version: 1,
        surface,
        effects,
        output_schema,
    }
}

fn effects_for(name: &str, surface: ToolSurface) -> Option<ToolEffects> {
    let effects = match surface {
        ToolSurface::Disk => match name {
            "project_info" | "font_info" | "read_glyph" | "proposal_list" | "docs" => {
                ToolEffects::read()
            }
            "proof" => ToolEffects::file(),
            "propose" => ToolEffects::document().local_code(),
            "nodes_run" => ToolEffects::file().local_code(),
            "propose_edits" => ToolEffects {
                mutates_document: true,
                ..ToolEffects::file()
            },
            "proposal_discard" => ToolEffects {
                mutates_document: true,
                ..ToolEffects::file().destructive()
            },
            _ => return None,
        },
        ToolSurface::Live => match name {
            "project_info" | "font_info" | "read_glyph" | "proof" | "proposal_list"
            | "editor_context" | "glyph_inventory" | "design_context" | "experiment_list"
            | "read_kerning" | "specimen" | "agent_receipt" | "proof_status" | "nodes_discover"
            | "nodes_snapshot" | "nodes_status" | "nodes_image" | "editor_sessions" => {
                ToolEffects::read()
            }
            "editor_connect" | "editor_open_glyph" | "editor_set_text" | "experiment_fork"
            | "proof_cancel" | "agent_cancel" | "nodes_cancel" => ToolEffects::session(),
            "proof_start" => ToolEffects::session().idempotent(),
            "nodes_mutate" => ToolEffects::session().destructive().idempotent(),
            "proof_release" | "nodes_release" => ToolEffects::session().destructive(),
            "nodes_run" => ToolEffects::session().local_code().idempotent(),
            "experiment_kern" => ToolEffects::session().destructive(),
            "propose_edits" => ToolEffects::document(),
            "proposal_discard" => ToolEffects::document().destructive(),
            "proposal_install"
            | "experiment_apply"
            | "experiment_undo_apply"
            | "agent_history"
            | "nodes_apply" => ToolEffects::document().destructive(),
            "agent_apply" => ToolEffects::document().destructive().idempotent(),
            "export_proof" => ToolEffects::file(),
            _ => return None,
        },
    };
    Some(effects)
}

fn error_schema() -> Value {
    json!({
        "type":"object",
        "properties":{"ok":{"const":false},"error":{"type":"string"}},
        "required":["ok","error"],
        "additionalProperties":true
    })
}

fn result_schema(success: Value) -> Value {
    json!({"type":"object","oneOf":[success,error_schema()]})
}

fn disk_project_info_schema() -> Value {
    result_schema(json!({
        "type":"object",
        "properties":{
            "ok":{"const":true},
            "project":{"type":"string"},
            "masters":{"type":"array","items":{
                "type":"object",
                "properties":{"index":{"type":"integer","minimum":0},
                    "name":{"type":"string"},"source":{"type":"string"}},
                "required":["index","name","source"],
                "additionalProperties":true
            }}
        },
        "required":["ok","project","masters"],
        "additionalProperties":true
    }))
}

fn live_project_info_schema() -> Value {
    result_schema(json!({
        "type":"object",
        "properties":{
            "ok":{"const":true},
            "live":{"const":true},
            "project":{"type":["string","null"]},
            "active_source":{"type":["integer","null"],"minimum":0},
            "sources":{"type":"array","items":{
                "type":"object",
                "properties":{
                    "index":{"type":"integer","minimum":0},
                    "id":{"type":"integer","minimum":0},
                    "path":{"type":"string"},
                    "name":{"type":"string"},
                    "location":{"type":"object"}
                },
                "required":["index","id","path","name","location"],
                "additionalProperties":true
            }},
            "document_epoch":{"type":"string"},
            "document_revision":{"type":"integer","minimum":0}
        },
        "required":["ok","live","project","active_source","sources","document_epoch"],
        "additionalProperties":true
    }))
}

fn editor_sessions_schema() -> Value {
    result_schema(json!({
        "type":"object",
        "properties":{
            "ok":{"const":true},
            "sessions":{"type":"array","items":{"type":"string"}},
            "connected":{"type":["string","null"]}
        },
        "required":["ok","sessions","connected"],
        "additionalProperties":true
    }))
}

fn export_proof_schema() -> Value {
    result_schema(json!({
        "type":"object",
        "properties":{
            "ok":{"const":true},
            "output":{"type":"string"},
            "bytes":{"type":"integer","minimum":0},
            "source_id":{"type":"integer","minimum":0},
            "document_epoch":{"type":"string"},
            "document_revision":{"type":"integer","minimum":0},
            "branch":{"type":["string","null"]}
        },
        "required":["ok","output","bytes","source_id","document_epoch",
            "document_revision","branch"],
        "additionalProperties":true
    }))
}

/// Additional tools supplied by the live MCP host rather than the editor engine.
pub fn live_host_tools() -> Vec<agent::Tool> {
    vec![
        agent::Tool {
            name: "export_proof".into(),
            description: "Export an explicit live or branch glyph/text proof using Designbot. Writes a new PNG or PDF file; refuses overwrite. Does not save the font. Supply either glyphs or text, an explicit output path, and format.".into(),
            parameters: json!({"type":"object","properties":{"source":{"type":"integer","minimum":0},"expected_document_epoch":{"type":"string"},"branch":{"type":"string"},"layer":{"type":"string"},"glyphs":{"type":"array","items":{"type":"string"}},"text":{"type":"string"},"output":{"type":"string"},"format":{"enum":["png","pdf"]}},"required":["output","format"],"additionalProperties":false}),
        },
        agent::Tool {
            name: "editor_sessions".into(),
            description: "List local editor endpoint paths. Connect to inspect the project. Never assume a different window is the requested font.".into(),
            parameters: json!({"type":"object", "properties":{}}),
        },
        agent::Tool {
            name: "editor_connect".into(),
            description: "Connect this agent to a listed editor endpoint and return its live project/source information. Opening another font closes the old connection; reconnect explicitly.".into(),
            parameters: json!({"type":"object", "properties":{"session":{"type":"string"}}, "required":["session"]}),
        },
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::automation::live;
    use crate::font::project::Project;

    #[test]
    fn every_published_tool_has_effect_metadata() {
        let disk = agent::tools();
        let live = live::tools();
        let host = live_host_tools();
        for (surface, tools) in [
            (ToolSurface::Disk, &disk),
            (ToolSurface::Live, &live),
            (ToolSurface::Live, &host),
        ] {
            let names: BTreeSet<_> = tools.iter().map(|tool| tool.name.as_str()).collect();
            assert_eq!(names.len(), tools.len());
            for tool in tools {
                let descriptor = describe(tool.clone(), surface);
                assert!(
                    descriptor.effects.is_some(),
                    "unknown {surface:?} tool: {}",
                    tool.name
                );
                assert_eq!(descriptor.tool, *tool);
            }
        }
    }

    #[test]
    fn effects_distinguish_surface_and_destructive_operations() {
        let find = |name: &str, surface| {
            let tools = match surface {
                ToolSurface::Disk => agent::tools(),
                ToolSurface::Live => live::tools().into_iter().chain(live_host_tools()).collect(),
            };
            describe(
                tools.into_iter().find(|tool| tool.name == name).unwrap(),
                surface,
            )
        };
        assert!(
            !find("proof", ToolSurface::Disk)
                .effects
                .unwrap()
                .is_read_only()
        );
        assert!(
            find("proof", ToolSurface::Live)
                .effects
                .unwrap()
                .is_read_only()
        );
        assert!(
            find("nodes_run", ToolSurface::Live)
                .effects
                .unwrap()
                .open_world
        );
        assert!(
            !find("editor_connect", ToolSurface::Live)
                .effects
                .unwrap()
                .is_read_only()
        );
        for name in ["proposal_discard", "proof_release", "nodes_release"] {
            assert!(find(name, ToolSurface::Live).effects.unwrap().destructive);
        }
        assert_eq!(
            find("project_info", ToolSurface::Disk).annotations()["openWorldHint"],
            false
        );
    }

    #[test]
    fn unknown_tool_does_not_claim_safety_or_result_shape() {
        let unknown = describe(
            agent::Tool {
                name: "future_tool".into(),
                description: String::new(),
                parameters: json!({}),
            },
            ToolSurface::Live,
        );
        assert_eq!(unknown.effects, None);
        assert_eq!(unknown.annotations(), json!({}));
        assert_eq!(unknown.output_schema, None);
    }

    #[test]
    fn concrete_result_schemas_have_success_and_error_branches() {
        for (name, surface) in [
            ("project_info", ToolSurface::Disk),
            ("project_info", ToolSurface::Live),
            ("editor_connect", ToolSurface::Live),
            ("editor_sessions", ToolSurface::Live),
            ("export_proof", ToolSurface::Live),
        ] {
            let descriptor = describe(
                agent::Tool {
                    name: name.into(),
                    description: String::new(),
                    parameters: json!({}),
                },
                surface,
            );
            let schema = descriptor.output_schema.unwrap();
            assert_eq!(schema["type"], "object");
            assert_eq!(schema["oneOf"][0]["properties"]["ok"]["const"], true);
            assert_eq!(schema["oneOf"][1]["properties"]["error"]["type"], "string");
            assert_eq!(schema["oneOf"][0]["additionalProperties"], true);
        }
    }

    // Check the currently emitted fields rather than accepting a generic object contract.
    fn assert_fixture_matches_declared_shape(schema: &Value, value: &Value) {
        let branch = if value["ok"] == true { 0 } else { 1 };
        let shape = &schema["oneOf"][branch];
        for name in shape["required"].as_array().unwrap() {
            let name = name.as_str().unwrap();
            assert!(value.get(name).is_some(), "missing required field {name}");
        }
        for (name, expected) in shape["properties"].as_object().unwrap() {
            let Some(actual) = value.get(name) else {
                continue;
            };
            if let Some(constant) = expected.get("const") {
                assert_eq!(actual, constant, "wrong constant for {name}");
            }
            let types = match &expected["type"] {
                Value::String(kind) => vec![kind.as_str()],
                Value::Array(kinds) => kinds.iter().map(|kind| kind.as_str().unwrap()).collect(),
                _ => continue,
            };
            assert!(
                types.iter().any(|kind| match *kind {
                    "string" => actual.is_string(),
                    "integer" => actual.is_i64() || actual.is_u64(),
                    "array" => actual.is_array(),
                    "object" => actual.is_object(),
                    "null" => actual.is_null(),
                    other => panic!("fixture helper does not support {other}"),
                }),
                "wrong type for {name}: {actual}"
            );
        }
    }

    #[test]
    fn result_contracts_fit_current_success_and_error_payloads() {
        let mut project = Project::new_font("never-saved.ufo".into());
        let mut live_info = live::call(&mut project, "project_info", &json!({}));
        assert_eq!(live_info["ok"], true);
        // The socket attaches the endpoint lifetime after the Project adapter responds.
        live_info["document_epoch"] = json!("epoch");
        let cases = [
            (
                "project_info",
                ToolSurface::Disk,
                json!({"ok":true,"project":"/font.designspace","masters":[
                    {"index":0,"name":"Regular","source":"/Regular.ufo"}]}),
            ),
            ("project_info", ToolSurface::Live, live_info.clone()),
            // Connect returns project_info from the selected endpoint.
            ("editor_connect", ToolSurface::Live, live_info),
            (
                "editor_sessions",
                ToolSurface::Live,
                json!({"ok":true,"sessions":["/tmp/editor.sock"],"connected":null}),
            ),
            (
                "export_proof",
                ToolSurface::Live,
                json!({"ok":true,"output":"/tmp/proof.png","bytes":8192,"source_id":0,
                    "document_epoch":"epoch","document_revision":4,"branch":null}),
            ),
        ];
        for (name, surface, success) in cases {
            let schema = describe(
                agent::Tool {
                    name: name.into(),
                    description: String::new(),
                    parameters: json!({}),
                },
                surface,
            )
            .output_schema
            .unwrap();
            assert_fixture_matches_declared_shape(&schema, &success);
            assert_fixture_matches_declared_shape(
                &schema,
                &json!({"ok":false,"error":"stale document","error_code":"stale_document"}),
            );
        }
    }
}

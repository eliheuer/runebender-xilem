// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Agent operations on the editor-owned project, with no filesystem reads or saves.

use serde_json::{Value, json};

use super::{
    agent,
    canonical_metadata::{KerningParticipant, KerningSide},
    edit_batch,
    project::Project,
    proposal,
    variable::SourceId,
};

/// Common live-document guidance for CLI-generated prompts and MCP initialization.
pub const INSTRUCTIONS: &str = "Live unsaved editor documents. Use editor_sessions then editor_connect if not connected. Verify the project and document_epoch, read editor_context, and choose an explicit stable source ID. Read glyphs before proposing; apply only within the user's granted authorization. Do not save font files. When multiple editors are open, choose the project the user requested. A closed endpoint never reconnects automatically. For agent_apply, reuse exactly the same actor, operation_key and complete payload after a lost response, or inspect agent_receipt. Use agent_cancel with that exact identity to prevent a pending apply before commit; cancellation never undoes a committed edit. These in-memory receipts do not cover legacy proposal/experiment mutations or survive document closure; inspect state before retrying those operations.";

/// A text-client prompt containing only the live tool schemas and live authorization rules.
pub fn system_prompt(tools: &[agent::Tool]) -> String {
    let mut prompt = format!(
        "{INSTRUCTIONS}\n\nUse only tools listed below. Read geometry before stating measurements. \
         A proposal is separate from installation; report an edit as installed only after a \
         successful installation result. Use explicit targets and the revision you read. \
         Use proof_start and proof_status for compiled PNG artifacts bound to captured revisions; \
         check current/stale and font_sha256 before comparing results. Legacy proof returns source \
         geometry. A text response alone does not establish that a model received an image.\n\n\
         To call a tool, emit one <tool_call>JSON object with name and arguments</tool_call> \
         block, then wait for its result.\n\n"
    );
    prompt.push_str(super::script_recipe::AUTHORING_INSTRUCTIONS);
    prompt.push_str("\n\n");
    for tool in tools {
        prompt.push_str(&format!(
            "- {}: {} Arguments: {}\n",
            tool.name, tool.description, tool.parameters
        ));
    }
    prompt
}

/// Tools supported by an open editor. Disk workflows are deliberately excluded.
pub fn tools() -> Vec<agent::Tool> {
    let mut result: Vec<_> = agent::tools()
        .into_iter()
        .filter(|tool| {
            matches!(
                tool.name.as_str(),
                "project_info"
                    | "font_info"
                    | "read_glyph"
                    | "proof"
                    | "propose_edits"
                    | "proposal_list"
                    | "proposal_discard"
            )
        })
        .map(|mut tool| {
            if tool.name == "proof" {
                tool.description = "Return an SVG proof and metrics from the live unsaved document, without writing a file. Supply 1 to 256 glyph names; use layer to compare a proposal.".into();
                tool.parameters["required"] = json!(["glyphs"]);
                tool.parameters["properties"]["glyphs"]["minItems"] = json!(1);
                tool.parameters["properties"]["glyphs"]["maxItems"] = json!(256);
            }
            tool.parameters["properties"].as_object_mut().expect("tool properties").remove("master");
            tool.description = tool.description.replace("master", "source");
            if tool.name == "project_info" {
                tool.description = "List the live project's stable source identities and current display indices. Choose a source by id, never by display index.".into();
            }
            tool
        })
        .collect();
    result.push(agent::Tool {
        name: "editor_context".into(),
        description: "Read one coherent application context: active stable source/layer/glyph, selection identities, text, features, axis values and gesture state. Widget-owned caret ranges are currently unavailable. Context never supplies implicit edit targets.".into(),
        parameters: json!({"type":"object", "properties":{}, "additionalProperties":false}),
    });
    result.push(agent::Tool {
        name: "glyph_inventory".into(),
        description: "Find live glyphs by mark label or Unicode scalar before selecting references and targets. Returns names, encoding, empty status and revisions. Green is a reference only when the project says so. Uses the dark theme to interpret legacy mark colors.".into(),
        parameters: json!({"type":"object", "properties": {
            "source":{"type":"integer","minimum":0},
            "mark":{"type":"string"},
            "codepoint":{"type":"integer","minimum":0,"maximum":1114111},
            "offset":{"type":"integer","minimum":0},
            "limit":{"type":"integer","minimum":1,"maximum":256}
        }, "additionalProperties":false}),
    });
    result.push(agent::Tool {
        name: "design_context".into(),
        description: "Read the type-design workflow and documentation entry points before designing. Project DESIGN.md and the user's reference choices determine the style; docs describe technique, not a universal aesthetic.".into(),
        parameters: json!({"type":"object", "properties":{},"additionalProperties":false}),
    });
    result.push(agent::Tool {
        name: "proposal_install".into(),
        description: "Install a reviewed proposal into the unsaved foreground, with one undo step per glyph. Only set authorization=user-approved after the user asks to apply it. Set keep_structure=false explicitly for a redraw; this can break interpolation with other sources. New glyph names must first exist in the editor. Re-proof after installation.".into(),
        parameters: json!({"type":"object", "properties":{
            "source":{"type":"integer","minimum":0},
            "task":{"type":"string"},
            "glyphs":{"type":"array","items":{"type":"string"},"minItems":1},
            "keep_structure":{"type":"boolean"}
        }, "required":["task","keep_structure"],"additionalProperties":false}),
    });
    for (name, description, properties, required) in [
        (
            "experiment_fork",
            "Fork a live source or a named experiment. Session-only; the root is unchanged. Fork a baseline once, then fork that baseline for fair A/B comparisons.",
            json!({"name":{"type":"string"},"parent":{"type":"string"},"reason":{"type":"string"}}),
            json!(["name", "reason"]),
        ),
        (
            "experiment_list",
            "List independent experimental versions and changes from their root baseline.",
            json!({}),
            json!([]),
        ),
        (
            "experiment_apply",
            "Apply explicitly selected experiment glyphs and/or kerning to the root after review. Set authorization=user-approved only after the user asks. Atomic conflict checks; never saves. Use experiment_undo_apply to undo the transaction.",
            json!({"branch":{"type":"string"},"glyphs":{"type":"array","items":{"type":"string"}},"kerning":{"type":"boolean"},"keep_structure":{"type":"boolean"}}),
            json!(["branch", "glyphs", "kerning", "keep_structure"]),
        ),
        (
            "experiment_undo_apply",
            "Undo the last experiment application without overwriting subsequent edits. Set authorization=user-approved only after the user asks.",
            json!({}),
            json!([]),
        ),
        (
            "read_kerning",
            "Read the complete kerning table, group membership and revision for a source or experiment.",
            json!({}),
            json!([]),
        ),
        (
            "experiment_kern",
            "Set or remove explicit kerning pairs in an experiment only. Supply the read_kerning revision and a reason. Does not change root or groups.",
            json!({"branch":{"type":"string"},"expected_revision":{"type":"string"},"reason":{"type":"string"},"pairs":{"type":"array","maxItems":4096,"items":{"type":"object","properties":{"left":{"type":"string"},"right":{"type":"string"},"value":{"type":["number","null"]}},"required":["left","right","value"],"additionalProperties":false}}}),
            json!(["branch", "expected_revision", "reason", "pairs"]),
        ),
    ] {
        result.push(agent::Tool {name:name.into(),description:description.into(),parameters:json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})});
    }
    result.push(agent::Tool {name:"specimen".into(),description:"Designbot scene for a one-page live Latin text proof at 18,24,36,48 pt. Harfrust shaping plus current UFO kerning. Use identical text for A/B experiments. Does not save files.".into(),parameters:json!({"type":"object","properties":{"text":{"type":"string","maxLength":256}},"required":["text"],"additionalProperties":false})});
    for tool in &mut result {
        tool.parameters["properties"]["expected_document_epoch"] = json!({
            "type":"string",
            "description":"Optional endpoint lifetime guard from a previous response; mismatch rejects before dispatch."
        });
        if matches!(
            tool.name.as_str(),
            "proposal_install" | "experiment_apply" | "experiment_undo_apply"
        ) {
            tool.parameters["properties"]["authorization"] = json!({
                "type":"string",
                "enum":["user-approved"],
                "description":"Explicit confirmation that the user requested this foreground mutation."
            });
            tool.parameters["required"]
                .as_array_mut()
                .expect("tool required list")
                .push(json!("authorization"));
        }
        if !matches!(
            tool.name.as_str(),
            "editor_context"
                | "design_context"
                | "project_info"
                | "experiment_list"
                | "experiment_undo_apply"
        ) {
            tool.parameters["properties"]["source"] = json!({"type":"integer","minimum":0});
            if tool.name != "experiment_fork" {
                tool.parameters["properties"]["branch"] = json!({"type":"string","description":"Named experiment; omit to address the root."});
            }
        }
    }
    result.extend(super::agent_edit::tools());
    result.extend(super::agent_nodes::tools());
    result.extend(super::agent_proof::tools());
    result
}

/// Handles a call on the GUI thread. Reads include unsaved changes; proposals mark
/// their source dirty. Explicit proposal installation changes foreground with undo;
/// no tool saves files.
/// Multi-source calls require an explicit stable source identity, independent of UI selection.
pub fn call(project: &mut Project, name: &str, args: &Value) -> Value {
    let mut result = match handle(project, name, args) {
        Ok(value) => {
            if name == "proposal_install"
                && value["root_changed"] == true
                && let Some(names) = value["installed"]["installed"].as_array()
            {
                for name in names.iter().filter_map(Value::as_str) {
                    project.recheck_compat(name);
                }
            }
            value
        }
        Err(error) => json!({"ok": false, "error": error}),
    };
    result["document_revision"] = json!(project.document_revision());
    result["saved"] = json!(false);
    result
}

fn handle(project: &mut Project, name: &str, args: &Value) -> Result<Value, String> {
    let object = args.as_object().ok_or("arguments must be an object")?;
    if object.contains_key("master") {
        return Err(
            "live tools use stable source IDs from project_info; master indices are unsupported"
                .into(),
        );
    }
    if object.contains_key("expected_document_epoch") {
        return Err("document epoch guards require the live socket session boundary".into());
    }
    if name == "editor_context" {
        return Ok(
            json!({"ok":false,"error":"application context is unavailable in this host", "error_code":"unsupported_context"}),
        );
    }
    if matches!(
        name,
        "proposal_install" | "experiment_apply" | "experiment_undo_apply"
    ) && object.get("authorization").and_then(Value::as_str) != Some("user-approved")
    {
        return Err(
            "explicit user authorization required: set authorization to user-approved only after the user asks"
                .into(),
        );
    }
    if name == "project_info" {
        let active_source = project.source_id(project.active).map(|source| source.0);
        return Ok(
            json!({"ok": true, "live": true, "project": project.export_source,
            "active_source": active_source, "sources": project.document_sources().enumerate()
                .map(|(index, source)| json!({"index": index, "id": source.id().0,
                    "path": source.path(), "name": source.name(), "location": source.location()}))
                .collect::<Vec<_>>()}),
        );
    }
    if name == "design_context" {
        return Ok(json!({"ok":true,
            "documentation":["https://runebender.org/docs/type-design.html", "https://runebender.org/docs/mcp.html", "https://runebender.org/llms-full.txt"],
            "workflow":["Read the project DESIGN.md with your file tools and the official type-design guide with your web tools.",
                "Confirm stable source identities, Unicode mapping, mark meanings, reference glyphs and target glyphs. Missing and empty are different.",
                "Read references and targets, then inspect actual proof images. If your client does not deliver images, stop visual judgments and report the limitation.",
                "Draft explicit contours or point edits with foreground revisions. Keep green references unchanged unless asked. For multiple masters preserve compatible point structure or report incompatibility.",
                "Proof the proposal layer with reference glyphs; compare at text and display sizes. Refine by discarding the draft and proposing from current foreground revisions.",
                "Install only when asked, then re-proof. Report unresolved issues and leave saving to the designer.",
                "For PDF review use the client's PDF tools. Tie each finding to a page, glyph or pair and master; distinguish outline weight, sidebearings, and pair kerning. Do not call a PDF fully reviewed if pages or images were unavailable."]}));
    }
    if name == "experiment_list" {
        return Ok(super::experiments::list(project));
    }
    if name == "experiment_undo_apply" {
        let (source, names) = super::experiments::undo_apply(project)?;
        return Ok(
            json!({"ok":true,"source":source.0,"source_id":source.0,"installed":{"installed":names},"root_changed":true}),
        );
    }
    let source = match object.get("source") {
        Some(value) => value
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .map(SourceId)
            .ok_or("source must be a nonnegative stable source identity")?,
        None if project.document_sources().count() == 1 => project
            .document_sources()
            .next()
            .expect("one source exists")
            .id(),
        None => return Err("source is required for a family; call project_info first".into()),
    };
    let source_path = project
        .document_source(source)
        .ok_or("unknown or removed source")?
        .path()
        .to_owned();
    let branch = object
        .get("branch")
        .map(|v| v.as_str().ok_or("branch must be a string"))
        .transpose()?;
    if name == "experiment_fork" {
        let name = object
            .get("name")
            .and_then(Value::as_str)
            .ok_or("name is required")?;
        let reason = object
            .get("reason")
            .and_then(Value::as_str)
            .ok_or("reason is required")?;
        let parent = object
            .get("parent")
            .map(|v| v.as_str().ok_or("parent must be a string"))
            .transpose()?;
        super::experiments::fork(project, source, name, parent, reason)?;
        return Ok(
            json!({"ok":true,"branch":name,"source":source.0,"source_id":source.0,"session_only":true}),
        );
    }
    if name == "experiment_apply" {
        let branch = branch.ok_or("branch is required")?;
        if project
            .experiments
            .versions
            .get(branch)
            .ok_or("unknown branch")?
            .root
            != source
        {
            return Err("branch belongs to another source".into());
        }
        let names: Vec<String> =
            serde_json::from_value(object.get("glyphs").ok_or("glyphs is required")?.clone())
                .map_err(|e| e.to_string())?;
        let kerning = object
            .get("kerning")
            .and_then(Value::as_bool)
            .ok_or("kerning is required")?;
        let keep = object
            .get("keep_structure")
            .and_then(Value::as_bool)
            .ok_or("keep_structure is required")?;
        let installed = super::experiments::apply(project, branch, &names, kerning, keep)?;
        return Ok(
            json!({"ok":true,"source":source.0,"source_id":source.0,"installed":{"installed":installed},"root_changed":true}),
        );
    }
    if let Some(name) = branch
        && project
            .experiments
            .versions
            .get(name)
            .ok_or("unknown experiment")?
            .root
            != source
    {
        return Err("experiment belongs to another source".into());
    }
    let layer = object
        .get("layer")
        .map(|v| v.as_str().ok_or("layer must be a string"))
        .transpose()?;
    let mut result = match name {
        "specimen" => {
            let text = object
                .get("text")
                .and_then(Value::as_str)
                .ok_or("text required")?;
            let metadata = match branch {
                Some(branch) => project
                    .experiments
                    .versions
                    .get(branch)
                    .ok_or("unknown experiment")?
                    .font_metadata(),
                None => project
                    .document_font_metadata(source)
                    .ok_or("unknown source metadata")?,
            };
            let scene = match branch {
                Some(branch) => crate::formats::designbot::specimen_experiment(
                    project,
                    project
                        .experiments
                        .versions
                        .get(branch)
                        .ok_or("unknown experiment")?,
                    text,
                )?,
                None => crate::formats::designbot::specimen_project(project, source, text)?,
            };
            json!({"ok":true,"scene":scene,"text":text,"kerning_revision":super::experiments::kerning_revision(metadata)?})
        }
        "read_kerning" => {
            let metadata = match branch {
                Some(branch) => project
                    .experiments
                    .versions
                    .get(branch)
                    .ok_or("unknown branch")?
                    .font_metadata(),
                None => project
                    .document_font_metadata(source)
                    .ok_or("unknown source metadata")?,
            };
            json!({"ok":true,"revision":super::experiments::kerning_revision(metadata)?,"pairs":metadata.raw_kerning(),"groups":metadata.groups()})
        }
        "experiment_kern" => {
            let branch = branch.ok_or("kerning edits require an experiment branch")?;
            let revision = object
                .get("expected_revision")
                .and_then(Value::as_str)
                .ok_or("expected_revision required")?;
            let version = project
                .experiments
                .versions
                .get(branch)
                .ok_or("unknown experiment")?;
            if revision != super::experiments::kerning_revision(version.font_metadata())? {
                return Err("stale kerning revision".into());
            }
            if object
                .get("reason")
                .and_then(Value::as_str)
                .is_none_or(|s| s.trim().is_empty())
            {
                return Err("reason is required".into());
            }
            let pairs = object
                .get("pairs")
                .and_then(Value::as_array)
                .ok_or("pairs required")?;
            if pairs.is_empty() || pairs.len() > 4096 {
                return Err("supply 1 to 4096 pairs".into());
            }
            let mut metadata = version.font_metadata().clone();
            for pair in pairs {
                let left = pair
                    .get("left")
                    .and_then(Value::as_str)
                    .ok_or("left required")?;
                let right = pair
                    .get("right")
                    .and_then(Value::as_str)
                    .ok_or("right required")?;
                for (key, prefix) in [(left, "public.kern1."), (right, "public.kern2.")] {
                    if !(version.layer(&version.default_address(key)).is_some()
                        || key.starts_with(prefix) && metadata.groups().contains_key(key))
                    {
                        return Err(format!(
                            "unknown glyph or side-specific kerning group: {key}"
                        ));
                    }
                }
                let value = pair.get("value").ok_or("value required")?;
                let value = if value.is_null() {
                    None
                } else {
                    Some(
                        value
                            .as_f64()
                            .filter(|v| v.is_finite() && v.abs() <= 100000.0)
                            .ok_or("invalid kerning value")?,
                    )
                };
                let participant = |raw: &str, side: KerningSide| {
                    if raw.starts_with(side.prefix()) {
                        KerningParticipant::group(side, raw)
                    } else {
                        KerningParticipant::glyph(raw)
                    }
                };
                metadata
                    .set_kerning_pair(
                        participant(left, KerningSide::First).map_err(|e| e.to_string())?,
                        participant(right, KerningSide::Second).map_err(|e| e.to_string())?,
                        value,
                    )
                    .map_err(|error| error.to_string())?;
            }
            let version = project
                .experiments
                .versions
                .get_mut(branch)
                .ok_or("unknown experiment")?;
            if !version.set_font_metadata(metadata) {
                return Err("kerning operations make no change".into());
            }
            json!({"ok":true,"revision":super::experiments::kerning_revision(version.font_metadata())?})
        }
        "font_info" => {
            let proposals = match branch {
                Some(branch) => project
                    .experiments
                    .versions
                    .get(branch)
                    .ok_or("unknown experiment")?
                    .proposals(),
                None => proposal::list_project(project, source),
            };
            let info = project
                .document_font_info(source)
                .ok_or("unknown source font information")?;
            let metrics = info.metrics.resolved();
            let glyphs = match branch {
                Some(branch) => {
                    let experiment = project
                        .experiments
                        .versions
                        .get(branch)
                        .ok_or("unknown experiment")?;
                    experiment
                        .layer_drafts()
                        .map(|(address, _)| address.glyph.as_str())
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .filter(|name| experiment.selected_layer(name, None).is_some())
                        .count()
                }
                None => {
                    let default = project
                        .document_source(source)
                        .ok_or("unknown source")?
                        .default_layer();
                    project
                        .glyph_names()
                        .filter(|name| project.document_layer(name, &default).is_some())
                        .count()
                }
            };
            json!({"ok": true, "family": info.names.family_name,
                "style": info.names.style_name, "units_per_em": metrics.units_per_em,
                "ascender": metrics.ascender, "descender": metrics.descender,
                "x_height": info.metrics.x_height, "cap_height": info.metrics.cap_height,
                "glyphs": glyphs, "proposals": proposals})
        }
        "glyph_inventory" => {
            let theme = crate::ui::theme::load_theme("dark").ok_or("missing built-in theme")?;
            let mark = object
                .get("mark")
                .map(|v| v.as_str().ok_or("mark must be a string"))
                .transpose()?;
            let codepoint = object
                .get("codepoint")
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .and_then(char::from_u32)
                        .ok_or("codepoint must be a Unicode scalar value")
                })
                .transpose()?;
            let integer = |key: &str, default: usize| -> Result<usize, String> {
                object.get(key).map_or(Ok(default), |v| {
                    v.as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .ok_or_else(|| format!("{key} must be a nonnegative integer"))
                })
            };
            let offset = integer("offset", 0)?;
            let limit = integer("limit", 128)?;
            if !(1..=256).contains(&limit) {
                return Err("limit must be between 1 and 256".into());
            }
            let layers: Vec<(String, super::LayerView<'_>)> = match branch {
                Some(branch) => {
                    let experiment = project
                        .experiments
                        .versions
                        .get(branch)
                        .ok_or("unknown experiment")?;
                    let names = experiment
                        .layer_drafts()
                        .map(|(address, _)| address.glyph.clone())
                        .collect::<std::collections::BTreeSet<_>>();
                    names
                        .into_iter()
                        .filter_map(|name| {
                            experiment
                                .selected_layer(&name, None)
                                .map(|(_, layer)| (name, layer))
                        })
                        .collect()
                }
                None => {
                    let default = project
                        .document_source(source)
                        .ok_or("unknown source")?
                        .default_layer();
                    project
                        .glyph_names()
                        .filter_map(|name| {
                            project
                                .document_layer(name, &default)
                                .map(|layer| (name.to_owned(), layer))
                        })
                        .collect()
                }
            };
            let matches = layers
                .into_iter()
                .filter_map(|(name, layer)| {
                    let label = crate::ui::theme::mark_label_for_layer(layer, &theme);
                    if mark.is_some_and(|mark| label.as_deref() != Some(mark))
                        || codepoint.is_some_and(|codepoint| {
                            !layer.codepoints().any(|candidate| candidate == codepoint)
                        })
                    {
                        return None;
                    }
                    Some((name, layer, label))
                })
                .collect::<Vec<_>>();
            let rows: Result<Vec<_>, String> = matches
                .iter()
                .skip(offset)
                .take(limit)
                .map(|(name, layer, label)| {
                    Ok(json!({"glyph":name,
                        "codepoints":layer.codepoints().map(u32::from).collect::<Vec<_>>(),
                        "mark":label,
                        "empty":layer.contours().next().is_none() && layer.components().next().is_none(),
                        "revision":edit_batch::canonical_glyph_revision(*layer)?}))
                })
                .collect();
            json!({"ok":true,"total":matches.len(),"offset":offset,"glyphs":rows?,
                "next_offset": (offset.saturating_add(limit) < matches.len()).then_some(offset.saturating_add(limit))})
        }
        "read_glyph" => {
            let glyph = object
                .get("glyph")
                .and_then(Value::as_str)
                .ok_or("glyph is required")?;
            match branch {
                Some(branch) => crate::analysis::glyph::read_experiment_glyph(
                    project
                        .experiments
                        .versions
                        .get(branch)
                        .ok_or("unknown experiment")?,
                    glyph,
                    layer,
                ),
                None => crate::analysis::glyph::read_project_glyph(project, source, glyph, layer),
            }
        }
        "proof" => {
            let names: Vec<String> = match object.get("glyphs") {
                Some(value) => serde_json::from_value(value.clone()).map_err(|e| e.to_string())?,
                None => Vec::new(),
            };
            if names.is_empty() || names.len() > 256 {
                return Err("live proofs require between 1 and 256 explicit glyph names".into());
            }
            let (proof, scene) = match branch {
                Some(branch) => {
                    let experiment = project
                        .experiments
                        .versions
                        .get(branch)
                        .ok_or("unknown experiment")?;
                    (
                        crate::formats::svg::proof_sheet_experiment(
                            project, experiment, layer, &names, 10,
                        )?,
                        crate::formats::designbot::scene_experiment(
                            project, experiment, layer, &names,
                        )?,
                    )
                }
                None => (
                    crate::formats::svg::proof_sheet_project(project, source, layer, &names, 10)?,
                    crate::formats::designbot::scene_project(project, source, layer, &names)?,
                ),
            };
            json!({"ok": true, "svg_content": proof.svg, "metrics": proof.metrics, "scene":scene})
        }
        "propose_edits" => {
            let mut batch = object.clone();
            batch.remove("source");
            batch.remove("branch");
            let batch: edit_batch::EditBatch =
                serde_json::from_value(Value::Object(batch)).map_err(|e| e.to_string())?;
            let summary = match branch {
                Some(branch) => project
                    .experiments
                    .versions
                    .get_mut(branch)
                    .ok_or("unknown experiment")?
                    .propose(&batch)?,
                None => edit_batch::propose_project(project, source, &batch)?,
            };
            json!({"ok": true, "proposal": summary})
        }
        "proposal_install" => {
            let task = object
                .get("task")
                .and_then(Value::as_str)
                .ok_or("task is required")?;
            let keep = object
                .get("keep_structure")
                .and_then(Value::as_bool)
                .ok_or("keep_structure is required")?;
            let only: Option<Vec<String>> = object
                .get("glyphs")
                .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
                .transpose()?;
            if only.as_ref().is_some_and(Vec::is_empty) {
                return Err("glyphs must not be empty".into());
            }
            let installed = match branch {
                Some(branch) => project
                    .experiments
                    .versions
                    .get_mut(branch)
                    .ok_or("unknown experiment")?
                    .install_proposal(task, only.as_deref(), keep)
                    .map_err(|error| error.to_string())?,
                None => {
                    proposal::install_project(project, source, task, only.as_deref(), keep)
                        .map_err(|error| error.to_string())?
                        .installed
                }
            };
            json!({"ok":true,"installed":installed})
        }
        "proposal_list" => {
            let proposals = match branch {
                Some(branch) => project
                    .experiments
                    .versions
                    .get(branch)
                    .ok_or("unknown experiment")?
                    .proposals(),
                None => proposal::list_project(project, source),
            };
            json!({"ok": true, "proposals": proposals})
        }
        "proposal_discard" => {
            let task = object
                .get("task")
                .and_then(Value::as_str)
                .ok_or("task is required")?;
            let count = match branch {
                Some(branch) => project
                    .experiments
                    .versions
                    .get_mut(branch)
                    .ok_or("unknown experiment")?
                    .discard_proposal(task)
                    .map_err(|error| error.to_string())?,
                None => proposal::discard_project(project, source, task)
                    .map_err(|error| error.to_string())?,
            };
            json!({"ok": true, "discarded": count})
        }
        _ => return Err(format!("unsupported live tool: {name}")),
    };
    result["branch"] = json!(branch);
    result["root_changed"] = json!(branch.is_none() && name == "proposal_install");
    result["live"] = json!(true);
    result["source_id"] = json!(source.0);
    result["source"] = json!(source_path);
    if let Some(branch) = branch
        && matches!(
            name,
            "experiment_kern" | "propose_edits" | "proposal_install" | "proposal_discard"
        )
        && result["ok"] == true
    {
        let v = project
            .experiments
            .versions
            .get_mut(branch)
            .ok_or("unknown branch")?;
        if v.events.len() >= 64 {
            v.events.remove(0);
        }
        v.events
            .push(json!({"tool":name,"reason":object.get("reason"),"task":object.get("task")}));
    }
    if let Some(scene) = result.get("scene") {
        project.experiments.proofs.insert(
            format!("{}:{}", source.0, branch.unwrap_or("root")),
            scene.clone(),
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::history::HistoryDirection;

    #[test]
    fn experiment_specimen_reports_the_snapshot_kerning_revision() {
        let mut project = Project::new_font("never-saved.ufo".into());
        let source = project.source_id(0).unwrap();
        assert_eq!(
            call(
                &mut project,
                "experiment_fork",
                &json!({"source":source.0,"name":"kern-proof","reason":"test"}),
            )["ok"],
            true
        );
        let before = call(
            &mut project,
            "read_kerning",
            &json!({"source":source.0,"branch":"kern-proof"}),
        )["revision"]
            .as_str()
            .unwrap()
            .to_owned();
        let changed = call(
            &mut project,
            "experiment_kern",
            &json!({"source":source.0,"branch":"kern-proof","expected_revision":before,
                "reason":"test","pairs":[{"left":"A","right":"A","value":-42.0}]}),
        );
        assert_eq!(changed["ok"], true, "{changed:#}");
        let revision = changed["revision"].as_str().unwrap();
        let proof = call(
            &mut project,
            "specimen",
            &json!({"source":source.0,"branch":"kern-proof","text":"AA"}),
        );
        assert_eq!(proof["ok"], true, "{proof:#}");
        assert_eq!(proof["kerning_revision"], revision);
        assert_ne!(
            proof["kerning_revision"],
            crate::document::experiments::kerning_revision(
                project.document_font_metadata(source).unwrap(),
            )
            .unwrap(),
            "the experiment proof must not report the unchanged root revision"
        );
    }

    #[test]
    fn unsaved_reads_proposals_install_and_undo_share_one_document() {
        let mut project = Project::new_font("never-saved.ufo".into());
        project
            .add_document_glyph("live_test", 400.0, None)
            .unwrap();
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        project
            .edit_document_layer("live_test", &layer, |draft| {
                draft.set_width(512.0)?;
                Ok(())
            })
            .unwrap();
        let read = call(&mut project, "read_glyph", &json!({"glyph": "live_test"}));
        assert_eq!(read["advance"], 512.0);
        let batch = json!({"task": "spacing", "reason": "more room", "edits": [{
            "glyph": "live_test", "expected_revision": read["revision"],
            "operations": [{"op": "set_width", "width": 560.0}]
        }]});
        assert_eq!(call(&mut project, "propose_edits", &batch)["ok"], true);
        assert_eq!(
            project.document_layer("live_test", &layer).unwrap().width(),
            512.0
        );
        assert_eq!(call(&mut project, "propose_edits", &batch)["ok"], false);
        let installed = call(
            &mut project,
            "proposal_install",
            &json!({"task":"spacing","keep_structure":true,"authorization":"user-approved"}),
        );
        assert_eq!(installed["installed"]["installed"], json!(["live_test"]));
        let address = super::super::variable::GlyphLayerAddress {
            glyph: "live_test".into(),
            layer,
        };
        assert_eq!(
            project
                .document_layer("live_test", &address.layer)
                .unwrap()
                .width(),
            560.0
        );
        project
            .replay_document_layer_history(&address, HistoryDirection::Undo)
            .unwrap();
        assert_eq!(
            project
                .document_layer("live_test", &address.layer)
                .unwrap()
                .width(),
            512.0
        );
    }

    #[test]
    fn drawing_requires_explicit_structure_choice_and_undo_restores_blank() {
        let mut project = Project::new_font("never-saved.ufo".into());
        project.add_document_glyph("draft", 500.0, None).unwrap();
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let revision =
            call(&mut project, "read_glyph", &json!({"glyph":"draft"}))["revision"].clone();
        let result = call(
            &mut project,
            "propose_edits",
            &json!({"task":"drawing","reason":"test drawing","edits":[{
                "glyph":"draft","expected_revision":revision,"operations":[{"op":"set_outline","contours":[{"points":[
                    {"x":50,"y":0,"type":"line"},{"x":250,"y":700,"type":"line"},{"x":450,"y":0,"type":"line"}
                ]}]}]
            }]}),
        );
        assert_eq!(result["ok"], true);
        let unauthorized = call(
            &mut project,
            "proposal_install",
            &json!({"task":"drawing","keep_structure":false}),
        );
        assert_eq!(unauthorized["ok"], false);
        assert!(
            unauthorized["error"]
                .as_str()
                .is_some_and(|error| error.contains("explicit user authorization"))
        );
        assert_eq!(
            project
                .document_layer("draft", &layer)
                .unwrap()
                .contours()
                .count(),
            0,
            "a missing authorization cannot change the foreground"
        );
        let guarded = call(
            &mut project,
            "proposal_install",
            &json!({"task":"drawing","keep_structure":true,"authorization":"user-approved"}),
        );
        assert_eq!(guarded["installed"]["installed"], json!([]));
        let applied = call(
            &mut project,
            "proposal_install",
            &json!({"task":"drawing","keep_structure":false,"authorization":"user-approved"}),
        );
        assert_eq!(applied["installed"]["installed"], json!(["draft"]));
        assert_eq!(
            project
                .document_layer("draft", &layer)
                .unwrap()
                .contours()
                .count(),
            1
        );
        let address = super::super::variable::GlyphLayerAddress {
            glyph: "draft".into(),
            layer: layer.clone(),
        };
        project
            .replay_document_layer_history(&address, HistoryDirection::Undo)
            .unwrap();
        assert_eq!(
            project
                .document_layer("draft", &layer)
                .unwrap()
                .contours()
                .count(),
            0
        );
        project
            .replay_document_layer_history(&address, HistoryDirection::Redo)
            .unwrap();
        assert_eq!(
            project
                .document_layer("draft", &layer)
                .unwrap()
                .contours()
                .count(),
            1
        );
    }

    #[test]
    fn inventory_finds_unicode_and_marks_without_changing_font() {
        let mut project = Project::new_font("never-saved.ufo".into());
        project
            .add_document_glyph("eight", 500.0, Some('8' as u32))
            .unwrap();
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        project
            .edit_document_layer("eight", &layer, |draft| {
                draft.set_mark(
                    Some("green"),
                    Some(crate::document::model::glyph_metadata::MarkColor {
                        red: 0.1,
                        green: 0.8,
                        blue: 0.2,
                        alpha: 1.0,
                    }),
                )?;
                Ok(())
            })
            .unwrap();
        let result = call(
            &mut project,
            "glyph_inventory",
            &json!({"codepoint":56,"mark":"green"}),
        );
        assert_eq!(result["glyphs"][0]["glyph"], "eight");
        assert_eq!(result["glyphs"][0]["empty"], true);
        assert_eq!(
            call(&mut project, "glyph_inventory", &json!({"codepoint":55296}))["ok"],
            false
        );
    }

    #[test]
    fn edits_after_read_reject_the_entire_proposal() {
        let mut project = Project::new_font("never-saved.ufo".into());
        project
            .add_document_glyph("live_test", 400.0, None)
            .unwrap();
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let read = call(&mut project, "read_glyph", &json!({"glyph": "live_test"}));
        project
            .edit_document_layer("live_test", &layer, |draft| {
                draft.set_width(450.0)?;
                Ok(())
            })
            .unwrap();
        let result = call(
            &mut project,
            "propose_edits",
            &json!({"task": "stale",
            "reason": "stale edit", "edits": [{"glyph": "live_test",
            "expected_revision": read["revision"], "operations": [{"op": "set_width", "width": 500.0}]}]}),
        );
        assert_eq!(result["ok"], false);
        assert!(proposal::list_project(&project, source).is_empty());
    }

    #[test]
    fn foreground_mutation_tools_require_explicit_user_authorization() {
        for name in [
            "proposal_install",
            "experiment_apply",
            "experiment_undo_apply",
        ] {
            let tool = tools()
                .into_iter()
                .find(|tool| tool.name == name)
                .expect("foreground tool is discoverable");
            assert!(
                tool.parameters["required"]
                    .as_array()
                    .is_some_and(|required| required.iter().any(|item| item == "authorization"))
            );
            assert_eq!(
                tool.parameters["properties"]["authorization"]["enum"],
                json!(["user-approved"])
            );
        }
    }
}

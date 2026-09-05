// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Agent operations on the editor-owned project, with no filesystem reads or saves.

use serde_json::{Value, json};

use super::{agent, edit_batch, project::Project, proposal};

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
            tool
        })
        .collect();
    result.push(agent::Tool {
        name: "glyph_inventory".into(),
        description: "Find live glyphs by mark label or Unicode scalar before selecting references and targets. Returns names, encoding, empty status and revisions. Green is a reference only when the project says so. Uses the dark theme to interpret legacy mark colors.".into(),
        parameters: json!({"type":"object", "properties": {
            "master":{"type":"integer","minimum":0},
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
        description: "Install a reviewed proposal into the unsaved foreground, with one undo step per glyph. Only call when the user asks to apply it. Set keep_structure=false explicitly for a redraw; this can break interpolation with other masters. New glyph names must first exist in the editor. Re-proof after installation.".into(),
        parameters: json!({"type":"object", "properties":{
            "master":{"type":"integer","minimum":0},
            "task":{"type":"string"},
            "glyphs":{"type":"array","items":{"type":"string"},"minItems":1},
            "keep_structure":{"type":"boolean"}
        }, "required":["task","keep_structure"],"additionalProperties":false}),
    });
    for (name, description, properties, required) in [
        (
            "experiment_fork",
            "Fork a live master or a named experiment. Session-only; the root is unchanged. Fork a baseline once, then fork that baseline for fair A/B comparisons.",
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
            "Apply explicitly selected experiment glyphs and/or kerning to the root after review. Atomic conflict checks; never saves. Use experiment_undo_apply to undo the transaction.",
            json!({"branch":{"type":"string"},"glyphs":{"type":"array","items":{"type":"string"}},"kerning":{"type":"boolean"},"keep_structure":{"type":"boolean"}}),
            json!(["branch", "glyphs", "kerning", "keep_structure"]),
        ),
        (
            "experiment_undo_apply",
            "Undo the last experiment application without overwriting subsequent edits.",
            json!({}),
            json!([]),
        ),
        (
            "read_kerning",
            "Read the complete kerning table, group membership and revision for a master or experiment.",
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
        if !matches!(
            tool.name.as_str(),
            "design_context" | "project_info" | "experiment_list" | "experiment_undo_apply"
        ) {
            tool.parameters["properties"]["master"] = json!({"type":"integer","minimum":0});
            if tool.name != "experiment_fork" {
                tool.parameters["properties"]["branch"] = json!({"type":"string","description":"Named experiment; omit to address the root."});
            }
        }
    }
    result
}

/// Handles a call on the GUI thread. Reads include unsaved changes; proposals mark
/// their master dirty. Explicit proposal installation changes foreground with undo;
/// no tool saves files.
/// Multi-master calls require an explicit master, independent of UI selection.
pub fn call(project: &mut Project, name: &str, args: &Value) -> Value {
    match handle(project, name, args) {
        Ok(value) => value,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

fn handle(project: &mut Project, name: &str, args: &Value) -> Result<Value, String> {
    let object = args.as_object().ok_or("arguments must be an object")?;
    if name == "project_info" {
        return Ok(
            json!({"ok": true, "live": true, "project": project.export_source,
            "active_master": project.active, "masters": project.masters.iter().enumerate()
                .map(|(index, master)| json!({"index": index, "source": master.source_path,
                    "dirty": master.dirty, "name": project.master_names.get(index)}))
                .collect::<Vec<_>>()}),
        );
    }
    if name == "design_context" {
        return Ok(json!({"ok":true,
            "documentation":["https://runebender.org/docs/type-design.html", "https://runebender.org/docs/mcp.html", "https://runebender.org/llms-full.txt"],
            "workflow":["Read the project DESIGN.md with your file tools and the official type-design guide with your web tools.",
                "Confirm master indices, Unicode mapping, mark meanings, reference glyphs and target glyphs. Missing and empty are different.",
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
        let (master, names) = super::experiments::undo_apply(project)?;
        return Ok(
            json!({"ok":true,"master":master,"installed":{"installed":names},"root_changed":true}),
        );
    }
    let index = match object.get("master") {
        Some(value) => value
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or("master must be a nonnegative integer")?,
        None if project.masters.len() == 1 => 0,
        None => return Err("master is required for a family; call project_info first".into()),
    };
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
        super::experiments::fork(project, index, name, parent, reason)?;
        return Ok(json!({"ok":true,"branch":name,"master":index,"session_only":true}));
    }
    if name == "experiment_apply" {
        let branch = branch.ok_or("branch is required")?;
        if project
            .experiments
            .versions
            .get(branch)
            .ok_or("unknown branch")?
            .root
            != index
        {
            return Err("branch belongs to another master".into());
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
            json!({"ok":true,"master":index,"installed":{"installed":installed},"root_changed":true}),
        );
    }
    let master = match branch {
        Some(name) => {
            let v = project
                .experiments
                .versions
                .get_mut(name)
                .ok_or("unknown experiment")?;
            if v.root != index {
                return Err("experiment belongs to another master".into());
            }
            &mut v.master
        }
        None => project.masters.get_mut(index).ok_or("unknown master")?,
    };
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
            json!({"ok":true,"scene":crate::formats::designbot::specimen(master,text)?,"text":text,"kerning_revision":super::experiments::kerning_revision(&master.font)?})
        }
        "read_kerning" => {
            json!({"ok":true,"revision":super::experiments::kerning_revision(&master.font)?,"pairs":master.font.kerning,"groups":master.font.groups})
        }
        "experiment_kern" => {
            if branch.is_none() {
                return Err("kerning edits require an experiment branch".into());
            }
            let revision = object
                .get("expected_revision")
                .and_then(Value::as_str)
                .ok_or("expected_revision required")?;
            if revision != super::experiments::kerning_revision(&master.font)? {
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
            let mut kerning = master.font.kerning.clone();
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
                    if !(master.font.default_layer().contains_glyph(key)
                        || key.starts_with(prefix) && master.font.groups.contains_key(key))
                    {
                        return Err(format!(
                            "unknown glyph or side-specific kerning group: {key}"
                        ));
                    }
                }
                let value = pair.get("value").ok_or("value required")?;
                if value.is_null() {
                    if let Some(row) = kerning.get_mut(left) {
                        row.remove(right);
                    }
                } else {
                    let value = value
                        .as_f64()
                        .filter(|v| v.is_finite() && v.abs() <= 100000.0)
                        .ok_or("invalid kerning value")?;
                    kerning
                        .entry(norad::Name::new(left).map_err(|e| e.to_string())?)
                        .or_default()
                        .insert(norad::Name::new(right).map_err(|e| e.to_string())?, value);
                }
            }
            kerning.retain(|_, row| !row.is_empty());
            master.font.kerning = kerning;
            master.kerning_dirty = true;
            master.dirty = true;
            json!({"ok":true,"revision":super::experiments::kerning_revision(&master.font)?})
        }
        "font_info" => json!({"ok": true, "family": master.font.font_info.family_name,
            "style": master.font.font_info.style_name, "units_per_em": master.units_per_em,
            "ascender": master.ascender, "descender": master.descender,
            "x_height": master.x_height, "cap_height": master.cap_height,
            "glyphs": master.font.default_layer().len(),
            "proposals": proposal::list(&master.font)}),
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
            let matches: Vec<_> = master
                .font
                .default_layer()
                .iter()
                .filter_map(|g| {
                    let label = crate::ui::theme::mark_label_for_glyph(g, &theme);
                    if mark.is_some_and(|m| label.as_deref() != Some(m))
                        || codepoint.is_some_and(|c| !g.codepoints.contains(c))
                    {
                        return None;
                    }
                    Some((g, label))
                })
                .collect();
            let rows: Result<Vec<_>, String> = matches.iter().skip(offset).take(limit).map(|(g, label)| {
                Ok(json!({"glyph":g.name(), "codepoints":g.codepoints.iter().map(u32::from).collect::<Vec<_>>(),
                    "mark":label, "empty":g.contours.is_empty() && g.components.is_empty(),
                    "revision":edit_batch::glyph_revision(g)?}))
            }).collect();
            json!({"ok":true,"total":matches.len(),"offset":offset,"glyphs":rows?,
                "next_offset": (offset.saturating_add(limit) < matches.len()).then_some(offset.saturating_add(limit))})
        }
        "read_glyph" => crate::analysis::glyph::read_glyph(
            &master.font,
            object
                .get("glyph")
                .and_then(Value::as_str)
                .ok_or("glyph is required")?,
            layer,
        ),
        "proof" => {
            let names: Vec<String> = match object.get("glyphs") {
                Some(value) => serde_json::from_value(value.clone()).map_err(|e| e.to_string())?,
                None => Vec::new(),
            };
            if names.is_empty() || names.len() > 256 {
                return Err("live proofs require between 1 and 256 explicit glyph names".into());
            }
            let proof = crate::formats::svg::proof_sheet(master, layer, &names, 10)?;
            json!({"ok": true, "svg_content": proof.svg, "metrics": proof.metrics, "scene":crate::formats::designbot::scene(master, layer, &names)?})
        }
        "propose_edits" => {
            let mut batch = object.clone();
            batch.remove("master");
            batch.remove("branch");
            let batch: edit_batch::EditBatch =
                serde_json::from_value(Value::Object(batch)).map_err(|e| e.to_string())?;
            let summary = edit_batch::propose(&mut master.font, &batch)?;
            master.dirty = true;
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
            let installed = master
                .install_proposal(task, only.as_deref(), keep)
                .map_err(|e| e.to_string())?;
            json!({"ok":true,"installed":installed})
        }
        "proposal_list" => json!({"ok": true, "proposals": proposal::list(&master.font)}),
        "proposal_discard" => {
            let task = object
                .get("task")
                .and_then(Value::as_str)
                .ok_or("task is required")?;
            let count = master.discard_proposal(task).map_err(|e| e.to_string())?;
            json!({"ok": true, "discarded": count})
        }
        _ => return Err(format!("unsupported live tool: {name}")),
    };
    result["branch"] = json!(branch);
    result["root_changed"] = json!(branch.is_none() && name == "proposal_install");
    result["live"] = json!(true);
    result["master"] = json!(index);
    result["source"] = json!(master.source_path);
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
            format!("{index}:{}", branch.unwrap_or("root")),
            scene.clone(),
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsaved_reads_proposals_install_and_undo_share_one_document() {
        let mut project = Project::new_font("never-saved.ufo".into());
        let index = project.masters[0].add_glyph("live_test", 400.0).unwrap();
        project.masters[0].set_advance(index, 512.0);
        let read = call(&mut project, "read_glyph", &json!({"glyph": "live_test"}));
        assert_eq!(read["advance"], 512.0);
        let batch = json!({"task": "spacing", "reason": "more room", "edits": [{
            "glyph": "live_test", "expected_revision": read["revision"],
            "operations": [{"op": "set_width", "width": 560.0}]
        }]});
        assert_eq!(call(&mut project, "propose_edits", &batch)["ok"], true);
        assert_eq!(
            project.masters[0]
                .font
                .get_glyph("live_test")
                .unwrap()
                .width,
            512.0
        );
        assert!(project.masters[0].dirty);
        assert_eq!(call(&mut project, "propose_edits", &batch)["ok"], false);
        let master = &mut project.masters[0];
        assert_eq!(
            master
                .install_proposal("spacing", None, true)
                .unwrap()
                .installed,
            ["live_test"]
        );
        assert_eq!(master.font.get_glyph("live_test").unwrap().width, 560.0);
        assert!(master.undo(index));
        assert_eq!(master.font.get_glyph("live_test").unwrap().width, 512.0);
    }

    #[test]
    fn drawing_requires_explicit_structure_choice_and_undo_restores_blank() {
        let mut project = Project::new_font("never-saved.ufo".into());
        let index = project.masters[0].add_glyph("draft", 500.0).unwrap();
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
        let guarded = call(
            &mut project,
            "proposal_install",
            &json!({"task":"drawing","keep_structure":true}),
        );
        assert_eq!(guarded["installed"]["installed"], json!([]));
        let applied = call(
            &mut project,
            "proposal_install",
            &json!({"task":"drawing","keep_structure":false}),
        );
        assert_eq!(applied["installed"]["installed"], json!(["draft"]));
        assert_eq!(
            project.masters[0]
                .font
                .get_glyph("draft")
                .unwrap()
                .contours
                .len(),
            1
        );
        assert!(project.masters[0].undo(index));
        assert!(
            project.masters[0]
                .font
                .get_glyph("draft")
                .unwrap()
                .contours
                .is_empty()
        );
        assert!(project.masters[0].redo(index));
        assert_eq!(
            project.masters[0]
                .font
                .get_glyph("draft")
                .unwrap()
                .contours
                .len(),
            1
        );
    }

    #[test]
    fn inventory_finds_unicode_and_marks_without_changing_font() {
        let mut project = Project::new_font("never-saved.ufo".into());
        project.masters[0].add_glyph("eight", 500.0);
        let glyph = project.masters[0].font.get_glyph_mut("eight").unwrap();
        glyph.codepoints.insert('8');
        crate::ui::theme::set_glyph_mark(glyph, Some("green"));
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
        let index = project.masters[0].add_glyph("live_test", 400.0).unwrap();
        let read = call(&mut project, "read_glyph", &json!({"glyph": "live_test"}));
        project.masters[0].set_advance(index, 450.0);
        let result = call(
            &mut project,
            "propose_edits",
            &json!({"task": "stale",
            "reason": "stale edit", "edits": [{"glyph": "live_test",
            "expected_revision": read["revision"], "operations": [{"op": "set_width", "width": 500.0}]}]}),
        );
        assert_eq!(result["ok"], false);
        assert!(proposal::list(&project.masters[0].font).is_empty());
    }
}

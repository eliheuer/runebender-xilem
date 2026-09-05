// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Agent operations on the editor-owned project, with no filesystem reads or saves.

use serde_json::{Value, json};

use super::{agent, edit_batch, project::Project, proposal};

/// Tools supported by an open editor. Disk workflows are deliberately excluded.
pub fn tools() -> Vec<agent::Tool> {
    agent::tools()
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
        .collect()
}

/// Handles a call on the GUI thread. Reads include unsaved changes; proposals mark
/// their master dirty but never install foreground edits or save files.
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
    let index = match object.get("master") {
        Some(value) => value
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .ok_or("master must be a nonnegative integer")?,
        None if project.masters.len() == 1 => 0,
        None => return Err("master is required for a family; call project_info first".into()),
    };
    let master = project.masters.get_mut(index).ok_or("unknown master")?;
    let layer = object
        .get("layer")
        .map(|v| v.as_str().ok_or("layer must be a string"))
        .transpose()?;
    let mut result = match name {
        "font_info" => json!({"ok": true, "family": master.font.font_info.family_name,
            "style": master.font.font_info.style_name, "units_per_em": master.units_per_em,
            "ascender": master.ascender, "descender": master.descender,
            "x_height": master.x_height, "cap_height": master.cap_height,
            "glyphs": master.font.default_layer().len(),
            "proposals": proposal::list(&master.font)}),
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
            json!({"ok": true, "svg_content": proof.svg, "metrics": proof.metrics})
        }
        "propose_edits" => {
            let mut batch = object.clone();
            batch.remove("master");
            let batch: edit_batch::EditBatch =
                serde_json::from_value(Value::Object(batch)).map_err(|e| e.to_string())?;
            let summary = edit_batch::propose(&mut master.font, &batch)?;
            master.dirty = true;
            json!({"ok": true, "proposal": summary})
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
    result["live"] = json!(true);
    result["master"] = json!(index);
    result["source"] = json!(master.source_path);
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

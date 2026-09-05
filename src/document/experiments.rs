// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Independent live-master experiments with guarded, selective application to the root.

use super::{
    edit_batch::glyph_revision,
    project::{Master, Project},
};
use norad::{Font, Glyph};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeMap;

/// One in-memory experimental version of a master. Never saved by normal Save.
#[derive(Debug)]
pub struct Experiment {
    /// Root master index.
    pub root: usize,
    /// Optional parent experiment, retained as provenance.
    pub parent: Option<String>,
    /// Brief or model information supplied by the caller.
    pub reason: String,
    /// Recent branch operations, including caller-supplied design intent.
    pub events: Vec<Value>,
    /// Root baseline for conflict detection, preserved through subsequent forks.
    pub base: Font,
    /// Independent working master with its own proposals and glyph history.
    pub master: Master,
}

/// Experiments live for the document session; applications can be explicitly undone.
#[derive(Debug, Default)]
pub struct Experiments {
    /// Named experiments in stable order.
    pub versions: BTreeMap<String, Experiment>,
    /// Last requested proof scene per master/version, shared with the node preview.
    pub proofs: BTreeMap<String, Value>,
    applied: Vec<Applied>,
}

#[derive(Debug)]
struct Applied {
    root: usize,
    glyphs: Vec<(String, Glyph, Glyph)>,
    kerning: Option<(norad::Kerning, norad::Kerning)>,
    kerning_after_revision: Option<String>,
}

/// Revision for the complete kerning table and group membership.
pub fn kerning_revision(font: &Font) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(&font.kerning, &font.groups)).map_err(|e| e.to_string())?;
    Ok(format!("kerning-sha256:{:x}", Sha256::digest(bytes)))
}

/// List versions and the glyphs and kerning changed from their common root baseline.
pub fn list(project: &Project) -> Value {
    json!({"ok":true,"session_only":true,"versions":project.experiments.versions.iter().map(|(name,v)| {
        let changed: Vec<_> = v.master.font.default_layer().iter().filter(|g| v.base.get_glyph(g.name().as_str()) != Some(*g)).map(|g|g.name().to_string()).collect();
        json!({"name":name,"master":v.root,"parent":v.parent,"reason":v.reason,"events":v.events,"changed_glyphs":changed,
            "kerning_changed":v.master.font.kerning != v.base.kerning})
    }).collect::<Vec<_>>()})
}

/// Fork the selected root or an existing experiment without touching its contents.
pub fn fork(
    project: &mut Project,
    root: usize,
    name: &str,
    parent: Option<&str>,
    reason: &str,
) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
    {
        return Err("name must use 1 to 64 ASCII letters, digits, hyphens or underscores".into());
    }
    if project.experiments.versions.len() >= 16 || project.experiments.versions.contains_key(name) {
        return Err("experiment already exists or the 16-version limit was reached".into());
    }
    let source = project.masters.get(root).ok_or("unknown master")?;
    let (font, base) = match parent {
        Some(parent) => {
            let v = project
                .experiments
                .versions
                .get(parent)
                .ok_or("unknown parent")?;
            if v.root != root {
                return Err("parent belongs to another master".into());
            }
            (v.master.font.clone(), v.base.clone())
        }
        None => (source.font.clone(), source.font.clone()),
    };
    project.experiments.versions.insert(
        name.into(),
        Experiment {
            root,
            parent: parent.map(str::to_owned),
            reason: reason.into(),
            events: Vec::new(),
            base,
            master: Master::from_font(font, source.source_path.clone()),
        },
    );
    Ok(())
}

/// Apply explicit existing glyphs and optionally kerning atomically after checking conflicts.
/// Unrelated root edits survive. Glyph changes enter normal undo; the whole application
/// can also be undone with `undo_apply`. Does not save or alter the experiment.
pub fn apply(
    project: &mut Project,
    name: &str,
    names: &[String],
    kerning: bool,
    keep_structure: bool,
) -> Result<Vec<String>, String> {
    let v = project
        .experiments
        .versions
        .get(name)
        .ok_or("unknown experiment")?;
    let root = project
        .masters
        .get_mut(v.root)
        .ok_or("missing root master")?;
    let mut changes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for name in names {
        if !seen.insert(name) {
            return Err("duplicate glyph".into());
        }
        let base = v.base.get_glyph(name).ok_or("glyph absent from base")?;
        let after = v
            .master
            .font
            .get_glyph(name)
            .ok_or("glyph absent from experiment")?;
        let before = root.font.get_glyph(name).ok_or("glyph absent from root")?;
        if after == base {
            continue;
        }
        if glyph_revision(before)? != glyph_revision(base)? {
            return Err(format!("conflict: {name} changed in the root"));
        }
        if keep_structure && !super::proposal::compatible(before, after) {
            return Err(format!("{name}: point structure changed"));
        }
        changes.push((name.clone(), before.clone(), after.clone()));
    }
    let kern = if kerning && v.master.font.kerning != v.base.kerning {
        if kerning_revision(&root.font)? != kerning_revision(&v.base)? {
            return Err("conflict: root kerning or groups changed".into());
        }
        Some((root.font.kerning.clone(), v.master.font.kerning.clone()))
    } else {
        None
    };
    if changes.is_empty() && kern.is_none() {
        return Err("no selected changes to apply".into());
    }
    for (name, _, after) in &changes {
        let index = *root.name_map.get(name).ok_or("missing glyph cache entry")?;
        root.record_undo(index);
        root.font.default_layer_mut().insert_glyph(after.clone());
        root.rebuild_entry(index);
        root.modified_glyphs.insert(name.clone());
        root.dirty = true;
    }
    if let Some((_, after)) = &kern {
        root.font.kerning = after.clone();
        root.kerning_dirty = true;
        root.dirty = true;
    }
    let names = changes.iter().map(|(name, _, _)| name.clone()).collect();
    project.experiments.applied.push(Applied {
        root: v.root,
        glyphs: changes,
        kerning_after_revision: if kern.is_some() {
            Some(kerning_revision(&root.font)?)
        } else {
            None
        },
        kerning: kern,
    });
    Ok(names)
}

/// Undo the most recent experiment application if its affected data is still unchanged.
/// Refuses to overwrite later edits. Leaves unrelated changes alone and does not save.
pub fn undo_apply(project: &mut Project) -> Result<(usize, Vec<String>), String> {
    let last = project
        .experiments
        .applied
        .last()
        .ok_or("no experiment application to undo")?;
    let root = &mut project.masters[last.root];
    for (name, _, after) in &last.glyphs {
        if root.font.get_glyph(name) != Some(after) {
            return Err(format!("cannot undo: {name} changed after application"));
        }
    }
    if let Some(revision) = &last.kerning_after_revision
        && kerning_revision(&root.font)? != *revision
    {
        return Err("cannot undo: kerning changed after application".into());
    }
    let last = project.experiments.applied.pop().unwrap();
    for (name, before, _) in &last.glyphs {
        let index = *root.name_map.get(name).ok_or("missing glyph cache entry")?;
        root.record_undo(index);
        root.font.default_layer_mut().insert_glyph(before.clone());
        root.rebuild_entry(index);
        root.modified_glyphs.insert(name.clone());
        root.dirty = true;
    }
    if let Some((before, _)) = last.kerning {
        root.font.kerning = before;
        root.kerning_dirty = true;
        root.dirty = true;
    }
    Ok((
        last.root,
        last.glyphs.into_iter().map(|(name, _, _)| name).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::live::call;

    #[test]
    fn two_kerning_versions_share_a_baseline_and_apply_undo_preserves_unrelated_edits() {
        let mut p = Project::new_font("synthetic.ufo".into());
        fork(&mut p, 0, "baseline", None, "same source").unwrap();
        fork(&mut p, 0, "a", Some("baseline"), "model A").unwrap();
        fork(&mut p, 0, "b", Some("baseline"), "model B").unwrap();
        for (branch, value) in [("a", -40.0), ("b", -80.0)] {
            let revision =
                call(&mut p, "read_kerning", &json!({"branch":branch}))["revision"].clone();
            assert_eq!(
                call(
                    &mut p,
                    "experiment_kern",
                    &json!({"branch":branch,"expected_revision":revision,"reason":"test","pairs":[{"left":"A","right":"V","value":value}]})
                )["ok"],
                true
            );
        }
        assert!(p.masters[0].font.kerning.is_empty());
        let index = p.masters[0].name_map["B"];
        p.masters[0].set_advance(index, 731.0);
        apply(&mut p, "a", &[], true, true).unwrap();
        assert_eq!(p.masters[0].font.kerning["A"]["V"], -40.0);
        assert_eq!(
            p.experiments.versions["b"].master.font.kerning["A"]["V"],
            -80.0
        );
        assert!(apply(&mut p, "b", &[], true, true).is_err());
        undo_apply(&mut p).unwrap();
        assert!(p.masters[0].font.kerning.is_empty());
        assert_eq!(p.masters[0].font.get_glyph("B").unwrap().width, 731.0);
    }

    #[test]
    fn glyph_conflict_rejects_whole_application_and_success_has_normal_undo() {
        let mut p = Project::new_font("synthetic.ufo".into());
        fork(&mut p, 0, "a", None, "test").unwrap();
        let v = p.experiments.versions.get_mut("a").unwrap();
        v.master.font.get_glyph_mut("A").unwrap().width = 700.0;
        v.master.font.get_glyph_mut("B").unwrap().width = 710.0;
        let original_a = p.masters[0].font.get_glyph("A").unwrap().width;
        p.masters[0].font.get_glyph_mut("B").unwrap().width = 999.0;
        assert!(apply(&mut p, "a", &["A".into(), "B".into()], false, true).is_err());
        assert_eq!(p.masters[0].font.get_glyph("A").unwrap().width, original_a);
        apply(&mut p, "a", &["A".into()], false, true).unwrap();
        let index = p.masters[0].name_map["A"];
        assert!(p.masters[0].undo(index));
        assert_eq!(p.masters[0].font.get_glyph("A").unwrap().width, original_a);
        assert!(undo_apply(&mut p).is_err());
    }

    #[test]
    fn invalid_pair_batch_never_partially_changes_branch() {
        let mut p = Project::new_font("synthetic.ufo".into());
        fork(&mut p, 0, "a", None, "test").unwrap();
        let revision = call(&mut p, "read_kerning", &json!({"branch":"a"}))["revision"].clone();
        let result = call(
            &mut p,
            "experiment_kern",
            &json!({"branch":"a","expected_revision":revision,"reason":"test","pairs":[
            {"left":"A","right":"V","value":-40},{"left":"absent","right":"V","value":-30}]}),
        );
        assert_eq!(result["ok"], false);
        assert!(p.experiments.versions["a"].master.font.kerning.is_empty());
    }
}

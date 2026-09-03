// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Proposals: edits offered to the designer, not yet made.
//!
//! A model, a script, or a tool such as `font-ml` proposes a change by
//! writing glyphs into a UFO layer named `com.runebender.proposal.<task>`,
//! next to the foreground layer. That is the whole contract. The tool
//! does not need this crate: it needs a UFO writer and the layer name.
//! The editor reads the layer, shows it, and the designer installs it
//! or discards it. Install copies each proposed glyph over the
//! foreground glyph as one undo step per glyph, so a proposed master
//! can be taken back one glyph at a time.
//!
//! A proposal glyph carries contours, components, anchors, and the
//! advance width. Everything else on the foreground glyph (unicodes,
//! lib, mark) stays as it was.
//!
//! Some tasks promise to keep point structure: the same contours, the
//! same points, in the same order, so a master stays interpolable
//! with its siblings. [`compatible`] checks that promise, and
//! [`crate::document::project::Master::install_proposal`] refuses a glyph that breaks it when
//! the caller asks for the check.

use std::fmt;

use norad::{Font, Glyph, Layer};
use serde::{Deserialize, Serialize};

use crate::document::font_ops::glyph_signature;

/// Every proposal layer starts with this.
pub const LAYER_PREFIX: &str = "com.runebender.proposal.";

/// The layer a task writes its proposal into.
pub fn layer_name(task: &str) -> String {
    format!("{LAYER_PREFIX}{task}")
}

/// The task a proposal layer belongs to, or None for any other layer.
pub fn task_of_layer(layer: &str) -> Option<&str> {
    layer.strip_prefix(LAYER_PREFIX).filter(|t| !t.is_empty())
}

/// What is wrong with a proposal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProposalError {
    /// The font has no proposal for this task.
    NoProposal {
        /// The task asked for.
        task: String,
    },
    /// The proposal names a glyph the foreground does not have.
    NoSuchGlyph {
        /// The task.
        task: String,
        /// The glyph.
        glyph: String,
    },
    /// The proposal changes a glyph's point structure, and the caller
    /// required it kept.
    Incompatible {
        /// The task.
        task: String,
        /// The glyph.
        glyph: String,
        /// Foreground contour and point counts against proposed.
        detail: String,
    },
    /// The layer name was not accepted by the UFO.
    BadLayerName {
        /// The name refused.
        name: String,
        /// Why.
        reason: String,
    },
}

impl fmt::Display for ProposalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoProposal { task } => write!(f, "no proposal for task {task}"),
            Self::NoSuchGlyph { task, glyph } => {
                write!(
                    f,
                    "proposal {task} names {glyph}, which the font does not have"
                )
            }
            Self::Incompatible {
                task,
                glyph,
                detail,
            } => {
                write!(
                    f,
                    "proposal {task} changes the structure of {glyph}: {detail}"
                )
            }
            Self::BadLayerName { name, reason } => write!(f, "bad layer name {name}: {reason}"),
        }
    }
}

impl std::error::Error for ProposalError {}

/// One proposal as found in a font.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ProposalSummary {
    /// The task that made it.
    pub task: String,
    /// The layer it lives in.
    pub layer: String,
    /// Every glyph it proposes, in layer order.
    pub glyphs: Vec<String>,
    /// Glyphs whose foreground has the same point structure.
    pub compatible: Vec<String>,
    /// Glyphs whose structure differs, with why.
    pub incompatible: Vec<(String, String)>,
    /// Glyphs the foreground does not have.
    pub missing: Vec<String>,
}

/// What an install did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Installed {
    /// The task.
    pub task: String,
    /// Glyphs now in the foreground, one undo step each.
    pub installed: Vec<String>,
    /// Glyphs left in the proposal, with why.
    pub skipped: Vec<(String, String)>,
    /// True when the proposal layer was removed because nothing was
    /// left in it.
    pub layer_removed: bool,
}

/// Whether a proposed glyph keeps the foreground's point structure.
pub fn compatible(foreground: &Glyph, proposed: &Glyph) -> bool {
    glyph_signature(foreground) == glyph_signature(proposed)
}

/// Contour and point counts, for a message.
fn describe(glyph: &Glyph) -> String {
    let points: usize = glyph.contours.iter().map(|c| c.points.len()).sum();
    format!("{}c · {}pt", glyph.contours.len(), points)
}

fn summarize(font: &Font, layer: &Layer) -> Option<ProposalSummary> {
    let task = task_of_layer(layer.name())?.to_string();
    let mut summary = ProposalSummary {
        task,
        layer: layer.name().to_string(),
        glyphs: Vec::new(),
        compatible: Vec::new(),
        incompatible: Vec::new(),
        missing: Vec::new(),
    };
    for proposed in layer.iter() {
        let name = proposed.name().to_string();
        summary.glyphs.push(name.clone());
        match font.default_layer().get_glyph(&name) {
            None => summary.missing.push(name),
            Some(fore) if compatible(fore, proposed) => summary.compatible.push(name),
            Some(fore) => summary.incompatible.push((
                name,
                format!(
                    "foreground {} · proposed {}",
                    describe(fore),
                    describe(proposed)
                ),
            )),
        }
    }
    Some(summary)
}

/// Every proposal in the font, in layer order.
pub fn list(font: &Font) -> Vec<ProposalSummary> {
    font.iter_layers()
        .filter_map(|layer| summarize(font, layer))
        .collect()
}

/// The proposal for one task.
pub fn find(font: &Font, task: &str) -> Result<ProposalSummary, ProposalError> {
    font.layers
        .get(&layer_name(task))
        .and_then(|layer| summarize(font, layer))
        .ok_or_else(|| ProposalError::NoProposal {
            task: task.to_string(),
        })
}

/// Writes glyphs into the task's proposal layer, replacing any glyph
/// of the same name already proposed. This is what a tool calls, or
/// what it imitates with its own UFO writer.
pub fn write(
    font: &mut Font,
    task: &str,
    glyphs: impl IntoIterator<Item = Glyph>,
) -> Result<ProposalSummary, ProposalError> {
    let name = layer_name(task);
    let layer =
        font.layers
            .get_or_create_layer(&name)
            .map_err(|e| ProposalError::BadLayerName {
                name: name.clone(),
                reason: e.to_string(),
            })?;
    for glyph in glyphs {
        layer.insert_glyph(glyph);
    }
    find(font, task)
}

/// Installs a task's proposal into the foreground of `font`: each
/// proposed glyph the foreground has is copied over it and removed
/// from the layer. `only` limits it to those glyphs. With
/// `keep_structure`, a glyph whose point structure differs is skipped
/// and stays proposed. `before` is called with each glyph's name and
/// its foreground as it stands just before it changes, which is where
/// a caller records an undo step.
/// The layer goes when it is empty.
///
/// This is the whole install; `Master::install_proposal` wraps it
/// with the master's undo pile and cache.
pub fn install(
    font: &mut Font,
    task: &str,
    only: Option<&[String]>,
    keep_structure: bool,
    before: &mut dyn FnMut(&str, &Glyph),
) -> Result<Installed, ProposalError> {
    let summary = find(font, task)?;
    let wanted = |name: &str| only.is_none_or(|list| list.iter().any(|n| n == name));
    let layer_name = layer_name(task);
    let mut installed = Vec::new();
    let mut skipped = Vec::new();
    for name in summary.glyphs.iter().filter(|n| wanted(n)) {
        if font.get_glyph(name.as_str()).is_none() {
            skipped.push((name.clone(), "not in the font".to_string()));
            continue;
        }
        let Some(proposed) = font
            .layers
            .get(&layer_name)
            .and_then(|l| l.get_glyph(name.as_str()))
            .cloned()
        else {
            continue;
        };
        if let Some((_, why)) = summary
            .incompatible
            .iter()
            .find(|(n, _)| keep_structure && n == name)
        {
            skipped.push((name.clone(), why.clone()));
            continue;
        }
        if let Some(foreground) = font.get_glyph_mut(name.as_str()) {
            before(name, foreground);
            apply(foreground, &proposed);
        }
        if let Some(layer) = font.layers.get_mut(&layer_name) {
            layer.remove_glyph(name.as_str());
        }
        installed.push(name.clone());
    }
    let layer_removed = font.layers.get(&layer_name).is_some_and(|l| l.is_empty())
        && font.layers.remove(&layer_name).is_some();
    Ok(Installed {
        task: task.to_string(),
        installed,
        skipped,
        layer_removed,
    })
}

/// Removes the task's proposal layer. Returns how many glyphs it held.
pub fn discard(font: &mut Font, task: &str) -> Result<usize, ProposalError> {
    font.layers
        .remove(&layer_name(task))
        .map(|layer| layer.len())
        .ok_or_else(|| ProposalError::NoProposal {
            task: task.to_string(),
        })
}

/// Copies what a proposal carries onto a foreground glyph.
pub(crate) fn apply(foreground: &mut Glyph, proposed: &Glyph) {
    foreground.contours = proposed.contours.clone();
    foreground.components = proposed.components.clone();
    foreground.anchors = proposed.anchors.clone();
    foreground.width = proposed.width;
}

#[cfg(test)]
mod tests {
    use super::*;
    use norad::{Contour, ContourPoint, PointType};

    fn glyph(name: &str, points: &[(f64, f64)], width: f64) -> Glyph {
        let mut g = Glyph::new(name);
        g.width = width;
        g.contours.push(Contour::new(
            points
                .iter()
                .map(|&(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
                .collect(),
            None,
        ));
        g
    }

    fn font() -> Font {
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(glyph(
            "a",
            &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)],
            100.0,
        ));
        font.default_layer_mut()
            .insert_glyph(glyph("b", &[(0.0, 0.0), (10.0, 0.0)], 100.0));
        font
    }

    #[test]
    fn layer_names_round_trip() {
        assert_eq!(layer_name("bolden"), "com.runebender.proposal.bolden");
        assert_eq!(
            task_of_layer("com.runebender.proposal.bolden"),
            Some("bolden")
        );
        assert_eq!(task_of_layer("com.runebender.proposal."), None);
        assert_eq!(task_of_layer("public.background"), None);
    }

    #[test]
    fn a_written_proposal_is_found_and_classified() {
        let mut font = font();
        let summary = write(
            &mut font,
            "bolden",
            [
                glyph("a", &[(0.0, 0.0), (12.0, 0.0), (12.0, 10.0)], 110.0),
                glyph("b", &[(0.0, 0.0)], 100.0),
                glyph("c", &[(0.0, 0.0)], 100.0),
            ],
        )
        .expect("the layer name is fine");
        assert_eq!(summary.task, "bolden");
        assert_eq!(summary.glyphs, ["a", "b", "c"]);
        assert_eq!(summary.compatible, ["a"]);
        assert_eq!(summary.incompatible.len(), 1);
        assert_eq!(summary.incompatible[0].0, "b");
        assert_eq!(summary.missing, ["c"]);
        assert_eq!(list(&font).len(), 1);
        assert_eq!(find(&font, "bolden").expect("present"), summary);
        assert_eq!(
            find(&font, "kern").expect_err("absent"),
            ProposalError::NoProposal {
                task: "kern".into()
            }
        );
    }

    #[test]
    fn discard_removes_the_layer() {
        let mut font = font();
        write(&mut font, "bolden", [glyph("a", &[(0.0, 0.0)], 1.0)]).expect("written");
        assert_eq!(discard(&mut font, "bolden").expect("present"), 1);
        assert!(list(&font).is_empty());
        assert!(discard(&mut font, "bolden").is_err());
    }

    #[test]
    fn errors_serialize_with_a_kind_tag() {
        let e = ProposalError::NoProposal {
            task: "bolden".into(),
        };
        let json = serde_json::to_value(&e).expect("serializes");
        assert_eq!(json["kind"], "no_proposal");
        assert_eq!(json["task"], "bolden");
    }

    #[test]
    fn install_copies_the_proposal_and_reports_each_glyph_first() {
        let mut font = Font::new();
        let mut a = Glyph::new("A");
        a.width = 500.0;
        let mut b = Glyph::new("B");
        b.width = 500.0;
        font.default_layer_mut().insert_glyph(a);
        font.default_layer_mut().insert_glyph(b);
        let mut pa = Glyph::new("A");
        pa.width = 580.0;
        let mut pb = Glyph::new("B");
        pb.width = 580.0;
        write(&mut font, "bolden", vec![pa, pb]).unwrap();
        let mut seen = Vec::new();
        let done = install(
            &mut font,
            "bolden",
            Some(&["A".to_string()]),
            true,
            &mut |name, glyph| {
                seen.push((name.to_string(), glyph.width));
            },
        )
        .unwrap();
        assert_eq!(done.installed, vec!["A".to_string()]);
        assert_eq!(
            seen,
            vec![("A".to_string(), 500.0)],
            "the foreground before the change"
        );
        assert_eq!(font.get_glyph("A").unwrap().width, 580.0);
        assert_eq!(
            font.get_glyph("B").unwrap().width,
            500.0,
            "B stays proposed"
        );
        assert!(!done.layer_removed);
        let rest = install(&mut font, "bolden", None, true, &mut |_, _| {}).unwrap();
        assert_eq!(rest.installed, vec!["B".to_string()]);
        assert!(rest.layer_removed);
    }
}

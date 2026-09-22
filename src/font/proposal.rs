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
//! foreground glyph as one undo step per glyph, so a proposed source
//! can be taken back one glyph at a time.
//!
//! A proposal glyph carries contours, components, anchors, and the
//! advance width. Everything else on the foreground glyph (unicodes,
//! lib, mark) stays as it was.
//!
//! Some tasks promise to keep point structure: the same contours, the
//! same points, in the same order, so a source stays interpolable
//! with its siblings. [`compatible_layers`] checks that promise for canonical
//! layers, and [`install_project`] refuses a glyph that breaks it when the caller
//! asks for the check. Standalone UFO serialization retains the same external layer contract in
//! the explicit format adapter.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt;

#[cfg(test)]
use norad::{Font, Glyph, Layer};
use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::font::font_ops::glyph_signature;
use crate::font::project::{DocumentChange, DocumentEditOutcome, Project};
use crate::font::variable::{GlyphLayerAddress, LayerId, SourceId};
use crate::font::{CanonicalLayerSnapshot, LayerEditDraft, LayerView};

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
    /// A canonical project operation failed without changing the foreground.
    Project {
        /// Why the operation could not be completed.
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
            Self::Project { reason } => f.write_str(reason),
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
#[cfg(test)]
pub fn compatible(foreground: &Glyph, proposed: &Glyph) -> bool {
    glyph_signature(foreground) == glyph_signature(proposed)
}

/// Whether two canonical layers have the same contour and point structure.
///
/// This compares canonical geometry without materializing a UFO glyph.
/// Components and anchors are deliberately outside the interpolation structure check.
pub fn compatible_layers(foreground: LayerView<'_>, proposed: LayerView<'_>) -> bool {
    let signature = |layer: LayerView<'_>| {
        layer
            .contours()
            .map(|contour| {
                contour
                    .points()
                    .map(|point| point.point_type())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    signature(foreground) == signature(proposed)
}

fn project_error(reason: impl Into<String>) -> ProposalError {
    ProposalError::Project {
        reason: reason.into(),
    }
}

fn validate_task(task: &str) -> Result<(), ProposalError> {
    if task.is_empty()
        || !task
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    {
        return Err(project_error(
            "task must contain only ASCII letters, digits, hyphens, or underscores",
        ));
    }
    Ok(())
}

fn proposal_layer(source: SourceId, task: &str) -> LayerId {
    LayerId {
        source,
        name: layer_name(task),
    }
}

fn describe_layer(layer: LayerView<'_>) -> String {
    let contours = layer.contours().count();
    let points = layer
        .contours()
        .map(|contour| contour.points().count())
        .sum::<usize>();
    format!("{contours}c · {points}pt")
}

fn summarize_project(
    project: &Project,
    source: SourceId,
    task: &str,
) -> Result<ProposalSummary, ProposalError> {
    let foreground = project
        .document_source(source)
        .ok_or_else(|| project_error("unknown source"))?
        .default_layer();
    let proposal = proposal_layer(source, task);
    let mut summary = ProposalSummary {
        task: task.to_owned(),
        layer: proposal.name.clone(),
        glyphs: Vec::new(),
        compatible: Vec::new(),
        incompatible: Vec::new(),
        missing: Vec::new(),
    };
    for glyph in project.glyph_names() {
        let Some(proposed) = project.document_layer(glyph, &proposal) else {
            continue;
        };
        summary.glyphs.push(glyph.to_owned());
        match project.document_layer(glyph, &foreground) {
            None => summary.missing.push(glyph.to_owned()),
            Some(current) if compatible_layers(current, proposed) => {
                summary.compatible.push(glyph.to_owned());
            }
            Some(current) => summary.incompatible.push((
                glyph.to_owned(),
                format!(
                    "foreground {} · proposed {}",
                    describe_layer(current),
                    describe_layer(proposed)
                ),
            )),
        }
    }
    if summary.glyphs.is_empty() {
        return Err(ProposalError::NoProposal {
            task: task.to_owned(),
        });
    }
    Ok(summary)
}

/// Every canonical proposal for one stable source, ordered by task name.
pub fn list_project(project: &Project, source: SourceId) -> Vec<ProposalSummary> {
    let mut tasks = BTreeSet::new();
    for glyph in project.glyph_names() {
        let Some(glyph) = project.document_glyph(glyph) else {
            continue;
        };
        for layer in glyph.layer_ids().filter(|layer| layer.source == source) {
            if let Some(task) = task_of_layer(&layer.name) {
                tasks.insert(task.to_owned());
            }
        }
    }
    tasks
        .into_iter()
        .filter_map(|task| summarize_project(project, source, &task).ok())
        .collect()
}

/// Find one canonical proposal by stable source and task.
pub fn find_project(
    project: &Project,
    source: SourceId,
    task: &str,
) -> Result<ProposalSummary, ProposalError> {
    summarize_project(project, source, task)
}

/// Read one proposed canonical layer for preview without changing the foreground.
pub fn preview_project<'a>(
    project: &'a Project,
    source: SourceId,
    task: &str,
    glyph: &str,
) -> Result<LayerView<'a>, ProposalError> {
    project
        .document_layer(glyph, &proposal_layer(source, task))
        .ok_or_else(|| ProposalError::NoProposal {
            task: task.to_owned(),
        })
}

/// Atomically write one canonical composition plan as a guarded proposal layer.
///
/// Every glyph, layer identity, component reference and foreground revision is validated before
/// the complete proposal layer commits in one document revision. The foreground and its histories
/// remain unchanged; composition becomes editable state only after [`install_project`].
pub fn write_composition_project(
    project: &mut Project,
    source: SourceId,
    mut plan: crate::font::compose::CompositionPlan,
) -> Result<crate::font::compose::Report, ProposalError> {
    let task = crate::font::compose::TASK;
    validate_task(task)?;
    if plan.report.proposal.is_some() {
        return Err(project_error("composition plan already records a proposal"));
    }
    if plan.replacements.is_empty() {
        return Err(project_error("composition plan has no replacements"));
    }
    let foreground = project
        .document_source(source)
        .ok_or_else(|| project_error("unknown source"))?
        .default_layer();
    let target = proposal_layer(source, task);
    let report_names = plan
        .report
        .derived
        .iter()
        .filter(|derived| !derived.up_to_date)
        .map(|derived| derived.glyph.as_str())
        .collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut staged = Vec::with_capacity(plan.replacements.len());
    for replacement in &plan.replacements {
        let name = replacement.derived.glyph.as_str();
        if replacement.derived.up_to_date || !report_names.contains(name) || !seen.insert(name) {
            return Err(project_error(format!(
                "{name}: inconsistent or duplicate composition payload"
            )));
        }
        let current = project.document_layer(name, &foreground).ok_or_else(|| {
            ProposalError::NoSuchGlyph {
                task: task.into(),
                glyph: name.into(),
            }
        })?;
        if replacement.codepoints != current.codepoints().collect::<Vec<_>>() {
            return Err(project_error(format!(
                "{name}: foreground encoding changed after composition planning"
            )));
        }
        if replacement.expected_revision.is_empty()
            || crate::font::edit_batch::canonical_glyph_revision(current)
                .ok()
                .as_deref()
                != Some(replacement.expected_revision.as_str())
        {
            return Err(project_error(format!(
                "{name}: stale composition plan; derive again"
            )));
        }
        for (reference, _, _) in &replacement.derived.components {
            if project.document_layer(reference, &foreground).is_none() {
                return Err(project_error(format!(
                    "{name}: component references missing glyph {reference:?}"
                )));
            }
        }
        let draft = super::babelfont::glyph_transactions::composition_proposal_layer(
            current,
            &target,
            replacement.derived.advance,
            &replacement.derived.components,
            &replacement.anchors,
            &replacement.expected_revision,
            "Compose from canonical anchors",
        )
        .map_err(project_error)?;
        staged.push((name.to_owned(), draft));
    }
    if seen.len() != report_names.len() {
        return Err(project_error(
            "composition report and replacement payloads disagree",
        ));
    }

    let transaction = project
        .begin_proposal_transaction(source, &foreground, &target, staged)
        .map_err(project_error)?;
    project
        .commit_proposal_transaction(transaction)
        .map_err(project_error)?;
    let summary = find_project(project, source, task)?;
    plan.report.proposal = Some(summary);
    Ok(plan.report)
}

pub(super) fn install_replacement(
    address: &GlyphLayerAddress,
    proposed: LayerView<'_>,
    before: &CanonicalLayerSnapshot,
) -> CanonicalLayerSnapshot {
    let (layer, preserved) = before.clone().into_parts();
    let replacement = super::babelfont::glyph_transactions::install_proposal_payload(
        LayerEditDraft::new(layer, preserved),
        proposed,
    );
    let (layer, preserved) = replacement.into_parts();
    CanonicalLayerSnapshot::new(address.clone(), layer, preserved)
}

/// Canonical changes produced while installing a proposal.
#[derive(Clone, Debug)]
pub struct ProjectProposalInstall {
    /// Stable addresses whose foreground payload changed.
    pub affected: Vec<GlyphLayerAddress>,
    /// Exact invalidation scope for every changed foreground layer.
    pub changes: Vec<DocumentChange>,
    /// Existing proposal result contract.
    pub installed: Installed,
}

/// Install selected proposal glyphs into one stable source after all revision checks.
///
/// Foreground replacements use Project-owned canonical history. Stale and structurally
/// incompatible glyphs remain in the proposal layer. All candidates are staged before the first
/// foreground mutation, so a validation error cannot partially install a batch.
pub fn install_project(
    project: &mut Project,
    source: SourceId,
    task: &str,
    only: Option<&[String]>,
    keep_structure: bool,
) -> Result<ProjectProposalInstall, ProposalError> {
    let summary = find_project(project, source, task)?;
    let foreground_layer = project
        .document_source(source)
        .ok_or_else(|| project_error("unknown source"))?
        .default_layer();
    let proposal_layer = proposal_layer(source, task);
    let wanted = |name: &str| only.is_none_or(|list| list.iter().any(|item| item == name));
    let incompatible: BTreeMap<_, _> = summary.incompatible.iter().cloned().collect();
    let mut staged = Vec::new();
    let mut skipped = Vec::new();

    for name in summary.glyphs.iter().filter(|name| wanted(name)) {
        let address = GlyphLayerAddress {
            glyph: name.clone(),
            layer: foreground_layer.clone(),
        };
        let proposal_address = GlyphLayerAddress {
            glyph: name.clone(),
            layer: proposal_layer.clone(),
        };
        let Some(foreground) = project.document_layer(name, &foreground_layer) else {
            skipped.push((name.clone(), "not in the font".to_owned()));
            continue;
        };
        let proposed = project
            .document_layer(name, &proposal_layer)
            .ok_or_else(|| project_error("proposal layer disappeared during validation"))?;
        let Some(base) = super::babelfont::glyph_transactions::proposal_base(proposed) else {
            skipped.push((
                name.clone(),
                "unguarded proposal: missing foreground revision; propose again".into(),
            ));
            continue;
        };
        if crate::font::edit_batch::canonical_glyph_revision(foreground)
            .ok()
            .as_deref()
            != Some(base)
        {
            skipped.push((
                name.clone(),
                "stale proposal: foreground changed; propose again".into(),
            ));
            continue;
        }
        if keep_structure && let Some(reason) = incompatible.get(name) {
            skipped.push((name.clone(), reason.clone()));
            continue;
        }
        let before = project
            .capture_document_layer(&address)
            .ok_or_else(|| project_error("foreground disappeared during validation"))?;
        let replacement = install_replacement(&address, proposed, &before);
        staged.push((address, proposal_address, before, replacement));
    }

    let mut installed = Vec::new();
    let mut affected = Vec::new();
    let mut changes = Vec::new();
    for (address, proposal_address, before, replacement) in staged {
        match project
            .commit_document_layer_replacement(&address, &before, replacement)
            .map_err(|error| project_error(error.to_string()))?
        {
            DocumentEditOutcome::Changed { change, .. } => {
                affected.push(address.clone());
                changes.push(change);
            }
            DocumentEditOutcome::Unchanged { .. } => {}
        }
        project
            .remove_glyph_layer(&proposal_address.glyph, &proposal_address.layer)
            .map_err(project_error)?;
        installed.push(address.glyph);
    }
    let layer_removed = if find_project(project, source, task).is_err() {
        project
            .remove_empty_auxiliary_layer(&proposal_layer)
            .map_err(project_error)?
    } else {
        false
    };
    Ok(ProjectProposalInstall {
        affected,
        changes,
        installed: Installed {
            task: task.to_owned(),
            installed,
            skipped,
            layer_removed,
        },
    })
}

/// Remove a canonical proposal without mutating any foreground glyph.
pub fn discard_project(
    project: &mut Project,
    source: SourceId,
    task: &str,
) -> Result<usize, ProposalError> {
    let summary = find_project(project, source, task)?;
    let layer = proposal_layer(source, task);
    for glyph in &summary.glyphs {
        project
            .remove_glyph_layer(glyph, &layer)
            .map_err(project_error)?;
    }
    if !project
        .remove_empty_auxiliary_layer(&layer)
        .map_err(project_error)?
    {
        return Err(project_error(
            "proposal layer container disappeared during discard",
        ));
    }
    Ok(summary.glyphs.len())
}

/// Adopt one external UFO proposal layer into canonical project state.
///
/// The external layer is decoded once at this named boundary. Missing foreground glyphs and an
/// existing task fail before canonical mutation.
pub fn adopt_external_project(
    project: &mut Project,
    source: SourceId,
    external: &Project,
    external_source: SourceId,
    task: &str,
) -> Result<ProposalSummary, ProposalError> {
    validate_task(task)?;
    if find_project(project, source, task).is_ok() {
        return Err(project_error(
            "proposal task already exists; use a new task name",
        ));
    }
    let source_layer = LayerId {
        source: external_source,
        name: layer_name(task),
    };
    if !external
        .document_source_layer_names(external_source)
        .is_some_and(|layers| layers.iter().any(|name| name == &source_layer.name))
    {
        return Err(ProposalError::NoProposal {
            task: task.to_owned(),
        });
    }
    let foreground = project
        .document_source(source)
        .ok_or_else(|| project_error("unknown source"))?
        .default_layer();
    let target = proposal_layer(source, task);
    let mut seen = HashSet::new();
    let mut staged = Vec::new();
    for name in external.glyph_names() {
        let Some(snapshot) = external.capture_document_layer(&GlyphLayerAddress {
            glyph: name.to_owned(),
            layer: source_layer.clone(),
        }) else {
            continue;
        };
        let name = name.to_owned();
        if !seen.insert(name.clone()) || project.document_layer(&name, &foreground).is_none() {
            return Err(ProposalError::NoSuchGlyph {
                task: task.to_owned(),
                glyph: name,
            });
        }
        let (layer, preserved) = snapshot.into_parts();
        let (layer, preserved) = super::babelfont::copy_layer(&layer, &preserved, &target);
        staged.push((name, LayerEditDraft::new(layer, preserved)));
    }
    if staged.is_empty() {
        return Err(ProposalError::NoProposal {
            task: task.to_owned(),
        });
    }
    for (glyph, replacement) in staged {
        project
            .add_glyph_layer(&glyph, &foreground, &target.name)
            .map_err(project_error)?;
        project
            .edit_document_layer(&glyph, &target, |draft| {
                *draft = replacement;
                Ok(())
            })
            .map_err(|error| project_error(error.to_string()))?;
    }
    find_project(project, source, task)
}

/// Clone a font with the named layer overlaid on the foreground for proof rendering.
/// Components resolve against the same overlaid glyphs; missing layer glyphs fall back
/// to foreground. Returns an error for an unknown layer and never changes the source.
#[cfg(test)]
pub fn preview_font(font: &Font, layer: &str) -> Result<Font, String> {
    let proposed = font
        .layers
        .get(layer)
        .ok_or_else(|| format!("no layer named {layer}"))?;
    let mut preview = font.clone();
    for glyph in proposed.iter() {
        preview.default_layer_mut().insert_glyph(glyph.clone());
    }
    Ok(preview)
}

/// Contour and point counts, for a message.
#[cfg(test)]
fn describe(glyph: &Glyph) -> String {
    let points: usize = glyph.contours.iter().map(|c| c.points.len()).sum();
    format!("{}c · {}pt", glyph.contours.len(), points)
}

#[cfg(test)]
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
#[cfg(test)]
pub fn list(font: &Font) -> Vec<ProposalSummary> {
    font.iter_layers()
        .filter_map(|layer| summarize(font, layer))
        .collect()
}

/// The proposal for one task.
#[cfg(test)]
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
#[cfg(test)]
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
/// This standalone-UFO install remains the external format-contract helper.
/// Live documents use [`install_project`] and Project-owned history.
#[cfg(test)]
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
        if let Some(base) = crate::formats::metadata::lib_keys::read_proposal_base(&proposed) {
            let current = font
                .get_glyph(name.as_str())
                .and_then(|glyph| crate::formats::ufo::glyph_revision(glyph).ok());
            if current.as_deref() != Some(base) {
                skipped.push((
                    name.clone(),
                    "stale proposal: foreground changed; propose again".into(),
                ));
                continue;
            }
        }
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
#[cfg(test)]
pub fn discard(font: &mut Font, task: &str) -> Result<usize, ProposalError> {
    font.layers
        .remove(&layer_name(task))
        .map(|layer| layer.len())
        .ok_or_else(|| ProposalError::NoProposal {
            task: task.to_string(),
        })
}

/// Copies what a proposal carries onto a foreground glyph.
#[cfg(test)]
pub(crate) fn apply(foreground: &mut Glyph, proposed: &Glyph) {
    foreground.contours = proposed.contours.clone();
    foreground.components = proposed.components.clone();
    foreground.anchors = proposed.anchors.clone();
    foreground.width = proposed.width;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use norad::{Anchor, Contour, ContourPoint, Name, PointType};

    use crate::font::history::HistoryDirection;
    use crate::font::project::{Project, SourceInput};
    use crate::font::variable::GlyphLayerAddress;

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

    fn composition_project() -> (Project, SourceId) {
        let mut font = Font::new();
        let anchor =
            |name: &str, x, y| Anchor::new(x, y, Some(Name::new(name).unwrap()), None, None);
        let mut base = Glyph::new("A");
        base.width = 700.0;
        base.codepoints.insert('A');
        base.anchors.push(anchor("top", 350.0, 700.0));
        font.default_layer_mut().insert_glyph(base);
        for (name, codepoint) in [("acute", '\u{00B4}'), ("grave", '`')] {
            let mut mark = Glyph::new(name);
            mark.codepoints.insert(codepoint);
            mark.anchors.push(anchor("_top", 150.0, 560.0));
            mark.anchors.push(anchor("top", 150.0, 760.0));
            font.default_layer_mut().insert_glyph(mark);
        }
        for (name, codepoint) in [("Aacute", '\u{00C1}'), ("Agrave", '\u{00C0}')] {
            let mut target = Glyph::new(name);
            target.codepoints.insert(codepoint);
            target.note = Some(format!("retain {name}"));
            font.default_layer_mut().insert_glyph(target);
        }
        let project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("CompositionProposal.ufo"),
        ));
        (project, SourceId(0))
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

    #[test]
    fn canonical_composition_stays_proposed_until_explicit_install() {
        let (mut project, source) = composition_project();
        let foreground = project.document_source(source).unwrap().default_layer();
        let address = GlyphLayerAddress {
            glyph: "Aacute".into(),
            layer: foreground.clone(),
        };
        let revision = project.document_revision();
        let plan =
            crate::font::compose::plan_project(&project, source, Some(&["Aacute".into()])).unwrap();
        let report = write_composition_project(&mut project, source, plan).unwrap();
        let summary = report.proposal.unwrap();
        assert_eq!(summary.glyphs, ["Aacute"]);
        assert_eq!(project.document_revision(), revision + 1);
        assert_eq!(
            project
                .document_layer("Aacute", &foreground)
                .unwrap()
                .components()
                .count(),
            0
        );
        let proposal =
            preview_project(&project, source, crate::font::compose::TASK, "Aacute").unwrap();
        assert_eq!(
            proposal
                .components()
                .map(|component| component.reference())
                .collect::<Vec<_>>(),
            ["A", "acute"]
        );
        assert!(
            super::super::babelfont::glyph_transactions::proposal_base(proposal)
                .unwrap()
                .starts_with("glif-sha256:")
        );

        let installed = install_project(
            &mut project,
            source,
            crate::font::compose::TASK,
            None,
            false,
        )
        .unwrap();
        assert_eq!(installed.installed.installed, ["Aacute"]);
        assert_eq!(
            project
                .document_layer("Aacute", &foreground)
                .unwrap()
                .components()
                .count(),
            2
        );
        assert_eq!(
            project
                .encode_ufo_source(source)
                .unwrap()
                .get_glyph("Aacute")
                .unwrap()
                .note
                .as_deref(),
            Some("retain Aacute")
        );
        project
            .replay_document_layer_history(&address, HistoryDirection::Undo)
            .unwrap();
        assert_eq!(
            project
                .document_layer("Aacute", &foreground)
                .unwrap()
                .components()
                .count(),
            0
        );
    }

    #[test]
    fn invalid_stale_empty_and_conflicting_composition_plans_are_atomic() {
        let (mut project, source) = composition_project();
        let names = ["Aacute".to_owned(), "Agrave".to_owned()];
        let mut invalid =
            crate::font::compose::plan_project(&project, source, Some(&names)).unwrap();
        invalid.replacements[1].expected_revision = "stale".into();
        let before = project.document_snapshot();
        let revision = project.document_revision();
        assert!(write_composition_project(&mut project, source, invalid).is_err());
        assert_eq!(project.document_snapshot(), before);
        assert_eq!(project.document_revision(), revision);
        assert!(find_project(&project, source, crate::font::compose::TASK).is_err());

        let mut empty =
            crate::font::compose::plan_project(&project, source, Some(&["Aacute".into()])).unwrap();
        empty.replacements.clear();
        assert!(write_composition_project(&mut project, source, empty).is_err());
        assert_eq!(project.document_snapshot(), before);

        let plan =
            crate::font::compose::plan_project(&project, source, Some(&["Aacute".into()])).unwrap();
        write_composition_project(&mut project, source, plan.clone()).unwrap();
        let proposed = project.document_snapshot();
        let proposed_revision = project.document_revision();
        assert!(write_composition_project(&mut project, source, plan).is_err());
        assert_eq!(project.document_snapshot(), proposed);
        assert_eq!(project.document_revision(), proposed_revision);
    }

    #[test]
    fn foreground_change_rejects_a_stale_composition_plan_without_history_noise() {
        let (mut project, source) = composition_project();
        let foreground = project.document_source(source).unwrap().default_layer();
        let address = GlyphLayerAddress {
            glyph: "Aacute".into(),
            layer: foreground.clone(),
        };
        let plan =
            crate::font::compose::plan_project(&project, source, Some(&["Aacute".into()])).unwrap();
        project
            .edit_document_layer("Aacute", &foreground, |draft| {
                draft.set_width(1.0)?;
                Ok(())
            })
            .unwrap();
        let before = project.document_snapshot();
        let revision = project.document_revision();
        let history = project.document_layer_history_depth(&address, HistoryDirection::Undo);
        assert!(write_composition_project(&mut project, source, plan).is_err());
        assert_eq!(project.document_snapshot(), before);
        assert_eq!(project.document_revision(), revision);
        assert_eq!(
            project.document_layer_history_depth(&address, HistoryDirection::Undo),
            history
        );
    }
}

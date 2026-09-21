// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Validated, revision-checked glyph edits offered as one proposal batch.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::formats::ufo::{glyph_from_layer, glyph_revision};
#[cfg(test)]
use norad::{Font, Glyph};

use crate::font::babelfont::{LayerEditDraft, LayerPointType, LayerView};
use crate::font::project::Project;
use crate::font::proposal::{self, ProposalSummary};
use crate::font::variable::{LayerId, SourceId};

/// Opaque SHA-256 revision of a canonical layer encoded through the external GLIF contract.
///
/// The UFO value is a transient compatibility codec result, never editable document state.
pub fn canonical_glyph_revision(layer: LayerView<'_>) -> Result<String, String> {
    glyph_revision(&glyph_from_layer(layer))
}

/// A batch starts from the foreground and writes a new, uniquely named proposal.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditBatch {
    /// Unique proposal task name; existing tasks are never overwritten.
    pub task: String,
    /// Human-readable design intent, persisted with every proposed glyph.
    pub reason: String,
    /// Glyphs to edit, each exactly once. Empty batches are rejected.
    pub edits: Vec<GlyphEdit>,
}

/// Ordered edits to one glyph, based on a revision returned by `read_glyph`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GlyphEdit {
    /// Foreground glyph name.
    pub glyph: String,
    /// Revision read before deciding the edit.
    pub expected_revision: String,
    /// Operations applied in order to a private copy.
    pub operations: Vec<Operation>,
}

/// An exact edit in font units. Only `SetOutline` changes contour and point order.
/// Coordinates must be finite. Point indices are zero-based and revision-scoped.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    /// Replace every contour with an explicit drawing in font units. Components are
    /// removed when `clear_components` is true; anchors, encoding, marks and width stay.
    SetOutline {
        /// Complete replacement contours in UFO point order.
        contours: Vec<crate::outline::drawing::DrawingContour>,
        /// Explicitly remove components when replacing a composite with drawn outlines.
        #[serde(default)]
        clear_components: bool,
    },
    /// Set the intended smooth flag on an existing on-curve point.
    SetSmooth {
        /// Contour index.
        contour: usize,
        /// Point index within the contour.
        point: usize,
        /// Intended smoothness; use curve analysis to measure actual continuity.
        smooth: bool,
    },
    /// Change the advance, leaving the outline in place.
    SetWidth {
        /// Nonnegative advance in font units.
        width: f64,
    },
    /// Move one point, including an off-curve control point, to an exact location.
    SetPoint {
        /// Contour index from `read_glyph`.
        contour: usize,
        /// Point index within the contour.
        point: usize,
        /// New x coordinate.
        x: f64,
        /// New y coordinate.
        y: f64,
    },
    /// Translate all outline points, component offsets, and anchors; keep the advance.
    Translate {
        /// Horizontal displacement.
        dx: f64,
        /// Vertical displacement.
        dy: f64,
    },
    /// Move an existing uniquely named anchor, or add it if absent.
    SetAnchor {
        /// Nonempty anchor name, such as `top` or `_top`.
        name: String,
        /// New x coordinate.
        x: f64,
        /// New y coordinate.
        y: f64,
    },
}

fn finite(values: &[f64]) -> Result<(), String> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err("coordinates and widths must be finite".into())
    }
}

pub(crate) fn validate_batch(batch: &EditBatch) -> Result<(), String> {
    if batch.task.is_empty()
        || !batch
            .task
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_".contains(character))
    {
        return Err("task must contain only ASCII letters, digits, hyphens, or underscores".into());
    }
    if batch.reason.trim().is_empty() || batch.edits.is_empty() {
        return Err("reason and edits must not be empty".into());
    }
    let mut seen = HashSet::new();
    for edit in &batch.edits {
        if !seen.insert(&edit.glyph) || edit.operations.is_empty() {
            return Err(format!(
                "{}: duplicate glyph or empty operations",
                edit.glyph
            ));
        }
    }
    Ok(())
}

fn point_at(
    layer: LayerView<'_>,
    contour: usize,
    point: usize,
) -> Result<crate::font::PointId, String> {
    layer
        .contours()
        .nth(contour)
        .and_then(|contour| contour.points().nth(point))
        .map(|point| point.id())
        .ok_or_else(|| format!("no point {contour}:{point}"))
}

fn replace_outline_canonically(
    draft: &mut LayerEditDraft,
    contours: &[crate::outline::drawing::DrawingContour],
    clear_components: bool,
) -> Result<(), String> {
    let contours = crate::formats::ufo::decode_drawing_contours(contours)?;
    draft
        .replace_imported_contours(contours)
        .map_err(|error| error.to_string())?;
    if clear_components {
        let components = draft
            .view()
            .components()
            .map(|component| component.id())
            .collect::<Vec<_>>();
        for component in components {
            draft
                .remove_component(component)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

/// Apply one revision-scoped batch operation to a canonical layer draft.
///
/// Ordinary point, component, anchor and metric changes use stable canonical identities.
/// Complete outline replacement decodes the public UFO drawing payload once, then replaces
/// canonical contours directly.
pub fn apply_canonical_operation(
    draft: &mut LayerEditDraft,
    operation: &Operation,
) -> Result<(), String> {
    match operation {
        Operation::SetOutline {
            contours,
            clear_components,
        } => replace_outline_canonically(draft, contours, *clear_components),
        Operation::SetSmooth {
            contour,
            point,
            smooth,
        } => {
            let view = draft.view();
            let target = view
                .contours()
                .nth(*contour)
                .and_then(|contour| contour.points().nth(*point))
                .ok_or("unknown point")?;
            if target.point_type() == LayerPointType::OffCurve {
                return Err("off-curve point cannot be smooth".into());
            }
            draft
                .set_point_smooth(target.id(), *smooth)
                .map_err(|error| error.to_string())?;
            Ok(())
        }
        Operation::SetWidth { width } => {
            finite(&[*width])?;
            if *width < 0.0 {
                return Err("advance must be nonnegative".into());
            }
            draft.set_width(*width).map_err(|error| error.to_string())?;
            Ok(())
        }
        Operation::SetPoint {
            contour,
            point,
            x,
            y,
        } => {
            finite(&[*x, *y])?;
            let id = point_at(draft.view(), *contour, *point)?;
            draft
                .set_point_position(id, kurbo::Point::new(*x, *y))
                .map_err(|error| error.to_string())?;
            Ok(())
        }
        Operation::Translate { dx, dy } => {
            finite(&[*dx, *dy])?;
            let points = draft
                .view()
                .contours()
                .flat_map(|contour| contour.points())
                .map(|point| (point.id(), point.position()))
                .collect::<Vec<_>>();
            let components = draft
                .view()
                .components()
                .map(|component| (component.id(), component.transform()))
                .collect::<Vec<_>>();
            let anchors = draft
                .view()
                .anchors()
                .map(|anchor| (anchor.id(), anchor.position()))
                .collect::<Vec<_>>();
            for (id, position) in points {
                draft
                    .set_point_position(id, position + kurbo::Vec2::new(*dx, *dy))
                    .map_err(|error| error.to_string())?;
            }
            for (id, transform) in components {
                let mut coefficients = transform.as_coeffs();
                coefficients[4] += dx;
                coefficients[5] += dy;
                finite(&coefficients)?;
                draft
                    .set_component_transform(id, kurbo::Affine::new(coefficients))
                    .map_err(|error| error.to_string())?;
            }
            for (id, position) in anchors {
                draft
                    .set_anchor_position(id, position + kurbo::Vec2::new(*dx, *dy))
                    .map_err(|error| error.to_string())?;
            }
            Ok(())
        }
        Operation::SetAnchor { name, x, y } => {
            finite(&[*x, *y])?;
            if name.is_empty() {
                return Err("anchor name must not be empty".into());
            }
            let matches = draft
                .view()
                .anchors()
                .filter(|anchor| anchor.name() == name)
                .map(|anchor| anchor.id())
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [] => {
                    draft
                        .add_anchor(name.clone(), kurbo::Point::new(*x, *y))
                        .map_err(|error| error.to_string())?;
                }
                [id] => {
                    draft
                        .set_anchor_position(*id, kurbo::Point::new(*x, *y))
                        .map_err(|error| error.to_string())?;
                }
                _ => return Err(format!("anchor {name} is ambiguous")),
            }
            Ok(())
        }
    }
}

pub(super) fn proposal_draft(
    foreground: LayerEditDraft,
    proposal_layer: &LayerId,
    edit: &GlyphEdit,
    reason: &str,
) -> Result<LayerEditDraft, String> {
    if canonical_glyph_revision(foreground.view())? != edit.expected_revision {
        return Err(format!(
            "{}: stale revision; read the glyph again",
            edit.glyph
        ));
    }
    let original = foreground.clone();
    let (foreground_layer, foreground_preserved) = foreground.into_parts();
    let (layer, preserved) = crate::font::babelfont::copy_layer(
        &foreground_layer,
        &foreground_preserved,
        proposal_layer,
    );
    let mut draft = LayerEditDraft::new(layer, preserved);
    for operation in &edit.operations {
        apply_canonical_operation(&mut draft, operation)
            .map_err(|error| format!("{}: {error}", edit.glyph))?;
    }
    if crate::font::babelfont::glyph_transactions::proposal_payload_eq(
        original.view(),
        draft.view(),
    ) {
        return Err(format!("{}: operations make no change", edit.glyph));
    }
    crate::font::babelfont::glyph_transactions::set_proposal_base(
        &mut draft,
        &edit.expected_revision,
        reason,
    );
    Ok(draft)
}

#[cfg(test)]
fn apply(glyph: &mut Glyph, operation: &Operation) -> Result<(), String> {
    match operation {
        Operation::SetOutline {
            contours,
            clear_components,
        } => {
            glyph.contours = crate::formats::ufo::drawing_contours(contours)?;
            if *clear_components {
                glyph.components.clear();
            }
        }
        Operation::SetSmooth {
            contour,
            point,
            smooth,
        } => {
            let target = glyph
                .contours
                .get_mut(*contour)
                .and_then(|c| c.points.get_mut(*point))
                .ok_or("unknown point")?;
            if target.typ == norad::PointType::OffCurve {
                return Err("off-curve point cannot be smooth".into());
            }
            target.smooth = *smooth;
        }
        Operation::SetWidth { width } => {
            finite(&[*width])?;
            if *width < 0.0 {
                return Err("advance must be nonnegative".into());
            }
            glyph.width = *width;
        }
        Operation::SetPoint {
            contour,
            point,
            x,
            y,
        } => {
            finite(&[*x, *y])?;
            let target = glyph
                .contours
                .get_mut(*contour)
                .and_then(|c| c.points.get_mut(*point))
                .ok_or_else(|| format!("no point {contour}:{point}"))?;
            target.x = *x;
            target.y = *y;
        }
        Operation::Translate { dx, dy } => {
            finite(&[*dx, *dy])?;
            for point in glyph.contours.iter_mut().flat_map(|c| &mut c.points) {
                point.x += dx;
                point.y += dy;
                finite(&[point.x, point.y])?;
            }
            for component in &mut glyph.components {
                component.transform.x_offset += dx;
                component.transform.y_offset += dy;
                finite(&[component.transform.x_offset, component.transform.y_offset])?;
            }
            for anchor in &mut glyph.anchors {
                anchor.x += dx;
                anchor.y += dy;
                finite(&[anchor.x, anchor.y])?;
            }
        }
        Operation::SetAnchor { name, x, y } => {
            finite(&[*x, *y])?;
            if name.is_empty() {
                return Err("anchor name must not be empty".into());
            }
            let matches = glyph
                .anchors
                .iter()
                .filter(|a| a.name.as_deref() == Some(name))
                .count();
            if matches > 1 {
                return Err(format!("anchor {name} is ambiguous"));
            }
            if let Some(anchor) = glyph
                .anchors
                .iter_mut()
                .find(|a| a.name.as_deref() == Some(name))
            {
                anchor.x = *x;
                anchor.y = *y;
            } else {
                let name = norad::Name::new(name).map_err(|e| e.to_string())?;
                glyph
                    .anchors
                    .push(norad::Anchor::new(*x, *y, Some(name), None, None));
            }
        }
    }
    Ok(())
}

/// Validate every edit on private glyph copies, then create a proposal layer.
/// Errors leave `font` unchanged. Never edits the foreground or saves files.
/// Existing proposal tasks, duplicate glyphs, stale revisions, and empty edits fail.
#[cfg(test)]
pub fn propose(font: &mut Font, batch: &EditBatch) -> Result<ProposalSummary, String> {
    validate_batch(batch)?;
    if font
        .layers
        .get(&proposal::layer_name(&batch.task))
        .is_some()
    {
        return Err("proposal task already exists; use a new task name".into());
    }
    let mut proposed = Vec::new();
    for edit in &batch.edits {
        let original = font
            .get_glyph(&edit.glyph)
            .ok_or_else(|| format!("no glyph named {}", edit.glyph))?;
        if glyph_revision(original)? != edit.expected_revision {
            return Err(format!(
                "{}: stale revision; read the glyph again",
                edit.glyph
            ));
        }
        let mut glyph = original.clone();
        for operation in &edit.operations {
            apply(&mut glyph, operation).map_err(|e| format!("{}: {e}", edit.glyph))?;
        }
        if glyph == *original {
            return Err(format!("{}: operations make no change", edit.glyph));
        }
        crate::formats::lib_keys::write_proposal_base(
            &mut glyph,
            &edit.expected_revision,
            &batch.reason,
        );
        proposed.push(glyph);
    }
    proposal::write(font, &batch.task, proposed).map_err(|e| e.to_string())
}

/// Validate and create a proposal in one stable source's canonical auxiliary layers.
///
/// The complete batch is staged before any layer is created. Foreground layers are read only;
/// proposal metadata and the GLIF SHA revision remain the explicit external UFO contract.
pub fn propose_project(
    project: &mut Project,
    source: SourceId,
    batch: &EditBatch,
) -> Result<ProposalSummary, String> {
    validate_batch(batch)?;
    if proposal::find_project(project, source, &batch.task).is_ok() {
        return Err("proposal task already exists; use a new task name".into());
    }
    let foreground = project
        .document_source(source)
        .ok_or("unknown source")?
        .default_layer();
    let proposed = LayerId {
        source,
        name: proposal::layer_name(&batch.task),
    };
    let mut staged = Vec::with_capacity(batch.edits.len());
    for edit in &batch.edits {
        let address = crate::font::variable::GlyphLayerAddress {
            glyph: edit.glyph.clone(),
            layer: foreground.clone(),
        };
        let snapshot = project
            .capture_document_layer(&address)
            .ok_or_else(|| format!("no glyph named {}", edit.glyph))?;
        let (layer, preserved) = snapshot.into_parts();
        staged.push((
            edit.glyph.clone(),
            proposal_draft(
                LayerEditDraft::new(layer, preserved),
                &proposed,
                edit,
                &batch.reason,
            )?,
        ));
    }

    for (glyph, replacement) in staged {
        project.add_glyph_layer(&glyph, &foreground, &proposed.name)?;
        project
            .edit_document_layer(&glyph, &proposed, |draft| {
                *draft = replacement;
                Ok(())
            })
            .map_err(|error| error.to_string())?;
    }
    proposal::find_project(project, source, &batch.task).map_err(|error| error.to_string())
}

/// Create a proposal on disk without rewriting foreground GLIFs or font metadata.
/// Validates the complete batch, writes its new layer, then atomically replaces
/// `layercontents.plist`. Rechecks glyph revisions and the layer index before publication.
/// Other applications do not participate in this writer's lock: callers must coordinate
/// external saves. The revision checks do not provide a cross-process filesystem transaction.
#[cfg(test)]
pub fn save_proposal(
    source: &std::path::Path,
    batch: &EditBatch,
) -> Result<ProposalSummary, String> {
    use std::fs;
    let lock_path = source.join(".runebender-proposal.lock");
    let lock = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|e| format!("cannot acquire proposal writer lock: {e}"))?;
    let result = (|| -> Result<ProposalSummary, String> {
        let index_path = source.join("layercontents.plist");
        let index_before = fs::read(&index_path).map_err(|e| e.to_string())?;
        let mut font = Font::load(source).map_err(|e| e.to_string())?;
        let summary = propose(&mut font, batch)?;
        let layer = font
            .layers
            .get(&summary.layer)
            .ok_or("proposal layer missing")?;
        let directory = source.join(layer.path());
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let index_temp = source.join(".runebender-layercontents.plist");
        let publish = (|| -> Result<(), String> {
            let mut contents = plist::Dictionary::new();
            for glyph in layer.iter() {
                let path = layer.get_path(glyph.name()).ok_or("glyph path missing")?;
                fs::write(
                    directory.join(path),
                    glyph.encode_xml().map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                contents.insert(
                    glyph.name().to_string(),
                    path.to_string_lossy().to_string().into(),
                );
            }
            plist::Value::Dictionary(contents)
                .to_file_xml(directory.join("contents.plist"))
                .map_err(|e| e.to_string())?;
            let layers = font
                .iter_layers()
                .map(|l| {
                    plist::Value::Array(vec![
                        l.name().to_string().into(),
                        l.path().to_string_lossy().to_string().into(),
                    ])
                })
                .collect();
            plist::Value::Array(layers)
                .to_file_xml(&index_temp)
                .map_err(|e| e.to_string())?;
            let latest = Font::load(source).map_err(|e| e.to_string())?;
            for edit in &batch.edits {
                let glyph = latest
                    .get_glyph(&edit.glyph)
                    .ok_or("foreground glyph removed")?;
                if glyph_revision(glyph)? != edit.expected_revision {
                    return Err(format!(
                        "{} changed while preparing the proposal",
                        edit.glyph
                    ));
                }
            }
            if fs::read(&index_path).map_err(|e| e.to_string())? != index_before {
                return Err("layer index changed while preparing the proposal".into());
            }
            fs::rename(&index_temp, &index_path).map_err(|e| e.to_string())
        })();
        if publish.is_err() {
            let _ = fs::remove_dir_all(&directory);
            let _ = fs::remove_file(&index_temp);
        }
        publish?;
        Ok(summary)
    })();
    drop(lock);
    let _ = fs::remove_file(lock_path);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::font::history::HistoryDirection;
    use crate::font::variable::GlyphLayerAddress;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "runebender-canonical-proposal-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fixture() -> (Font, EditBatch) {
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(Glyph::new("n"));
        let batch = EditBatch {
            task: "spacing-1".into(),
            reason: "Test spacing".into(),
            edits: vec![GlyphEdit {
                glyph: "n".into(),
                expected_revision: glyph_revision(font.get_glyph("n").unwrap()).unwrap(),
                operations: vec![Operation::SetWidth { width: 600.0 }],
            }],
        };
        (font, batch)
    }

    #[test]
    fn canonical_outline_replacement_preserves_unrelated_state_and_clears_components_explicitly() {
        use crate::outline::drawing::{DrawingContour, DrawingPoint, DrawingPointType};

        let project = Project::new_font("canonical.ufo".into());
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let address = GlyphLayerAddress {
            glyph: "A".into(),
            layer: layer.clone(),
        };
        let mut transaction = project.begin_document_layer_transaction(&address).unwrap();
        let width = transaction.draft().view().width();
        transaction
            .draft_mut()
            .add_component("B".into(), kurbo::Affine::translate((12.0, 34.0)))
            .unwrap();
        transaction
            .draft_mut()
            .add_anchor("top".into(), kurbo::Point::new(100.0, 700.0))
            .unwrap();
        let contour = DrawingContour {
            points: [(0.0, 0.0), (400.0, 0.0), (400.0, 700.0), (0.0, 700.0)]
                .into_iter()
                .map(|(x, y)| DrawingPoint {
                    x,
                    y,
                    kind: DrawingPointType::Line,
                    smooth: false,
                })
                .collect(),
        };
        apply_canonical_operation(
            transaction.draft_mut(),
            &Operation::SetOutline {
                contours: vec![contour.clone()],
                clear_components: false,
            },
        )
        .unwrap();
        let view = transaction.draft().view();
        assert_eq!(view.contours().count(), 1);
        assert_eq!(view.components().count(), 1);
        assert_eq!(view.anchors().count(), 1);
        assert_eq!(view.width(), width);
        assert_eq!(view.codepoints().collect::<Vec<_>>(), ['A']);

        let invalid = DrawingContour {
            points: vec![DrawingPoint {
                x: 0.0,
                y: 0.0,
                kind: DrawingPointType::Line,
                smooth: false,
            }],
        };
        assert!(
            apply_canonical_operation(
                transaction.draft_mut(),
                &Operation::SetOutline {
                    contours: vec![invalid],
                    clear_components: true,
                },
            )
            .is_err()
        );
        assert_eq!(transaction.draft().view().components().count(), 1);

        apply_canonical_operation(
            transaction.draft_mut(),
            &Operation::SetOutline {
                contours: vec![contour],
                clear_components: true,
            },
        )
        .unwrap();
        let view = transaction.draft().view();
        assert_eq!(view.components().count(), 0);
        assert_eq!(view.anchors().count(), 1);
        assert_eq!(view.width(), width);
        assert_eq!(view.codepoints().collect::<Vec<_>>(), ['A']);
    }

    #[test]
    fn canonical_project_proposal_installs_with_guarded_history() {
        let mut project = Project::new_font("canonical.ufo".into());
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let address = GlyphLayerAddress {
            glyph: "A".into(),
            layer: layer.clone(),
        };
        let original = project.document_layer("A", &layer).unwrap().width();
        let revision = canonical_glyph_revision(project.document_layer("A", &layer).unwrap())
            .expect("canonical GLIF revision");
        let batch = EditBatch {
            task: "canonical-spacing".into(),
            reason: "verify canonical proposal flow".into(),
            edits: vec![GlyphEdit {
                glyph: "A".into(),
                expected_revision: revision,
                operations: vec![Operation::SetWidth {
                    width: original + 31.0,
                }],
            }],
        };

        let summary = propose_project(&mut project, source, &batch).unwrap();
        assert_eq!(summary.glyphs, ["A"]);
        assert_eq!(
            project.document_layer("A", &layer).unwrap().width(),
            original
        );
        let installed =
            proposal::install_project(&mut project, source, &batch.task, None, true).unwrap();
        assert_eq!(installed.installed.installed, ["A"]);
        assert_eq!(installed.affected, std::slice::from_ref(&address));
        assert_eq!(installed.changes.len(), 1);
        assert_eq!(
            project.document_layer("A", &layer).unwrap().width(),
            original + 31.0
        );
        assert!(proposal::find_project(&project, source, &batch.task).is_err());
        project
            .replay_document_layer_history(&address, HistoryDirection::Undo)
            .unwrap();
        assert_eq!(
            project.document_layer("A", &layer).unwrap().width(),
            original
        );
    }

    #[test]
    fn last_install_removes_the_proposal_layer_from_save_and_reopen() {
        let scratch = Scratch::new();
        let path = scratch.0.join("Canonical.ufo");
        let mut project = Project::new_font(path.clone());
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let batch = EditBatch {
            task: "saved-cleanly".into(),
            reason: "verify the empty layer container is removed".into(),
            edits: vec![GlyphEdit {
                glyph: "A".into(),
                expected_revision: canonical_glyph_revision(
                    project.document_layer("A", &layer).unwrap(),
                )
                .unwrap(),
                operations: vec![Operation::SetWidth { width: 701.0 }],
            }],
        };
        propose_project(&mut project, source, &batch).unwrap();
        let installed = proposal::install_project(&mut project, source, &batch.task, None, true)
            .unwrap()
            .installed;
        assert!(installed.layer_removed);
        project.save().unwrap();

        let saved = Font::load(&path).unwrap();
        assert!(
            saved
                .layers
                .get(&proposal::layer_name(&batch.task))
                .is_none()
        );
        let reopened = Project::load(&path).unwrap();
        assert!(proposal::list_project(&reopened, reopened.source_id(0).unwrap()).is_empty());
    }

    #[test]
    fn stale_canonical_install_keeps_the_proposal_and_does_not_mutate() {
        let mut project = Project::new_font("canonical.ufo".into());
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let original = project.document_layer("A", &layer).unwrap().width();
        let batch = EditBatch {
            task: "stale-canonical".into(),
            reason: "verify stale guard".into(),
            edits: vec![GlyphEdit {
                glyph: "A".into(),
                expected_revision: canonical_glyph_revision(
                    project.document_layer("A", &layer).unwrap(),
                )
                .unwrap(),
                operations: vec![Operation::SetWidth {
                    width: original + 20.0,
                }],
            }],
        };
        propose_project(&mut project, source, &batch).unwrap();
        project
            .edit_document_layer("A", &layer, |draft| {
                draft.set_width(original + 1.0)?;
                Ok(())
            })
            .unwrap();
        let before = project.document_revision();
        let installed =
            proposal::install_project(&mut project, source, &batch.task, None, true).unwrap();
        assert!(installed.installed.installed.is_empty());
        assert!(installed.installed.skipped[0].1.contains("stale"));
        assert_eq!(project.document_revision(), before);
        assert_eq!(
            project.document_layer("A", &layer).unwrap().width(),
            original + 1.0
        );
        assert!(proposal::find_project(&project, source, &batch.task).is_ok());
    }

    #[test]
    fn invalid_canonical_batch_does_not_publish_earlier_glyphs() {
        let mut project = Project::new_font("canonical.ufo".into());
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let original_a = project.document_layer("A", &layer).unwrap().width();
        let revision = project.document_revision();
        let batch = EditBatch {
            task: "atomic-canonical".into(),
            reason: "verify complete staging".into(),
            edits: vec![
                GlyphEdit {
                    glyph: "A".into(),
                    expected_revision: canonical_glyph_revision(
                        project.document_layer("A", &layer).unwrap(),
                    )
                    .unwrap(),
                    operations: vec![Operation::SetWidth {
                        width: original_a + 20.0,
                    }],
                },
                GlyphEdit {
                    glyph: "B".into(),
                    expected_revision: canonical_glyph_revision(
                        project.document_layer("B", &layer).unwrap(),
                    )
                    .unwrap(),
                    operations: vec![Operation::SetPoint {
                        contour: usize::MAX,
                        point: 0,
                        x: 1.0,
                        y: 2.0,
                    }],
                },
            ],
        };

        assert!(propose_project(&mut project, source, &batch).is_err());
        assert_eq!(project.document_revision(), revision);
        assert_eq!(
            project.document_layer("A", &layer).unwrap().width(),
            original_a
        );
        assert!(proposal::list_project(&project, source).is_empty());
    }

    #[test]
    fn adopted_external_proposals_without_a_revision_fail_closed() {
        let mut project = Project::new_font("canonical.ufo".into());
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let original = project.document_layer("A", &layer).unwrap().width();
        let mut external = Font::new();
        let snapshot = project.encode_ufo_source(source).unwrap();
        let mut proposed = snapshot.get_glyph("A").unwrap().clone();
        proposed.width = original + 90.0;
        proposal::write(&mut external, "external", [proposed]).unwrap();
        let external = Project::from_source(crate::font::project::SourceInput::from_font(
            external,
            PathBuf::from("external.ufo"),
        ));
        let external_source = external.source_id(0).unwrap();

        let before_invalid = project.document_revision();
        assert!(
            proposal::adopt_external_project(
                &mut project,
                source,
                &external,
                external_source,
                "bad task",
            )
            .unwrap_err()
            .to_string()
            .contains("task must contain")
        );
        assert_eq!(project.document_revision(), before_invalid);

        proposal::adopt_external_project(
            &mut project,
            source,
            &external,
            external_source,
            "external",
        )
        .unwrap();
        let before = project.document_revision();
        let installed = proposal::install_project(&mut project, source, "external", None, true)
            .unwrap()
            .installed;
        assert!(installed.installed.is_empty());
        assert!(installed.skipped[0].1.contains("unguarded"));
        assert_eq!(project.document_revision(), before);
        assert_eq!(
            project.document_layer("A", &layer).unwrap().width(),
            original
        );
    }

    #[test]
    fn isolated_versions_hold_and_install_canonical_proposals() {
        let mut project = Project::new_font("canonical.ufo".into());
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let original = project.document_layer("A", &layer).unwrap().width();
        crate::font::experiments::fork(&mut project, source, "branch", None, "test").unwrap();
        let batch = EditBatch {
            task: "branch-spacing".into(),
            reason: "verify isolated proposal".into(),
            edits: vec![GlyphEdit {
                glyph: "A".into(),
                expected_revision: canonical_glyph_revision(
                    project.experiments.versions["branch"]
                        .layer(&project.experiments.versions["branch"].default_address("A"))
                        .unwrap(),
                )
                .unwrap(),
                operations: vec![Operation::SetWidth {
                    width: original + 45.0,
                }],
            }],
        };
        let version = project.experiments.versions.get_mut("branch").unwrap();
        version.propose(&batch).unwrap();
        assert_eq!(version.proposals()[0].glyphs, ["A"]);
        assert_eq!(
            version
                .install_proposal(&batch.task, None, true)
                .unwrap()
                .installed,
            ["A"]
        );
        assert_eq!(
            version
                .layer(&version.default_address("A"))
                .unwrap()
                .width(),
            original + 45.0
        );
        assert_eq!(
            project.document_layer("A", &layer).unwrap().width(),
            original
        );
    }

    #[test]
    fn translation_moves_components_and_anchors_and_point_edits_are_exact() {
        let (mut font, mut batch) = fixture();
        let glyph = font.get_glyph_mut("n").unwrap();
        glyph.contours.push(norad::Contour::new(
            vec![norad::ContourPoint::new(
                10.0,
                20.0,
                norad::PointType::Move,
                false,
                None,
                None,
            )],
            None,
        ));
        glyph.components.push(norad::Component::new(
            norad::Name::new("base").unwrap(),
            norad::AffineTransform::default(),
            None,
        ));
        glyph.anchors.push(norad::Anchor::new(
            5.0,
            6.0,
            Some(norad::Name::new("top").unwrap()),
            None,
            None,
        ));
        batch.edits[0].expected_revision = glyph_revision(glyph).unwrap();
        batch.edits[0].operations = vec![
            Operation::Translate { dx: 12.0, dy: 3.0 },
            Operation::SetPoint {
                contour: 0,
                point: 0,
                x: 23.0,
                y: 24.0,
            },
            Operation::SetAnchor {
                name: "bottom".into(),
                x: 0.0,
                y: -10.0,
            },
        ];
        propose(&mut font, &batch).unwrap();
        let proposed = font
            .layers
            .get(&proposal::layer_name(&batch.task))
            .unwrap()
            .get_glyph("n")
            .unwrap();
        assert_eq!(
            (
                proposed.contours[0].points[0].x,
                proposed.contours[0].points[0].y
            ),
            (23.0, 24.0)
        );
        assert_eq!(
            (
                proposed.components[0].transform.x_offset,
                proposed.components[0].transform.y_offset
            ),
            (12.0, 3.0)
        );
        assert_eq!((proposed.anchors[0].x, proposed.anchors[0].y), (17.0, 9.0));
        assert_eq!(proposed.anchors.len(), 2);
        assert_eq!(proposed.width, 0.0);
        assert_eq!(font.get_glyph("n").unwrap().contours[0].points[0].x, 10.0);
    }

    #[test]
    fn batch_is_atomic_and_requires_fresh_reads() {
        let (mut font, mut batch) = fixture();
        batch.edits[0].operations.push(Operation::SetPoint {
            contour: 99,
            point: 0,
            x: 1.0,
            y: 2.0,
        });
        assert!(propose(&mut font, &batch).is_err());
        assert!(proposal::list(&font).is_empty());
        assert_eq!(font.get_glyph("n").unwrap().width, 0.0);
        batch.edits[0].operations.pop();
        font.get_glyph_mut("n").unwrap().width = 5.0;
        assert!(propose(&mut font, &batch).unwrap_err().contains("stale"));
    }

    #[test]
    fn proposal_is_reviewable_and_stale_install_is_skipped() {
        let (mut font, batch) = fixture();
        propose(&mut font, &batch).unwrap();
        assert_eq!(font.get_glyph("n").unwrap().width, 0.0);
        assert!(propose(&mut font, &batch).is_err());
        font.get_glyph_mut("n").unwrap().width = 20.0;
        let installed =
            proposal::install(&mut font, &batch.task, None, true, &mut |_, _| {}).unwrap();
        assert!(installed.installed.is_empty());
        assert!(installed.skipped[0].1.contains("stale"));
        assert_eq!(font.get_glyph("n").unwrap().width, 20.0);
    }

    #[test]
    fn canonical_install_preserves_object_identity_metadata_and_undo() {
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(Glyph::new("base"));
        let mut glyph = Glyph::new("A");
        glyph.width = 500.125;
        glyph.codepoints.insert('A');
        glyph.note = Some("retain note".into());
        glyph
            .lib
            .insert("future.key".into(), plist::Value::String("exact".into()));
        glyph.contours.push(norad::Contour::new(
            vec![
                norad::ContourPoint::new(0.0, 0.0, norad::PointType::Line, false, None, None),
                norad::ContourPoint::new(100.0, 0.0, norad::PointType::Line, false, None, None),
            ],
            None,
        ));
        glyph.components.push(norad::Component::new(
            norad::Name::new("base").unwrap(),
            norad::AffineTransform::default(),
            None,
        ));
        glyph.anchors.push(norad::Anchor::new(
            50.0,
            100.0,
            Some(norad::Name::new("top").unwrap()),
            None,
            None,
        ));
        font.default_layer_mut().insert_glyph(glyph);
        let mut project = Project::from_source(crate::font::project::SourceInput::from_font(
            font,
            PathBuf::from("IdentityProposal.ufo"),
        ));
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let address = GlyphLayerAddress {
            glyph: "A".into(),
            layer: layer.clone(),
        };
        let before = project.document_layer("A", &layer).unwrap();
        let contour_id = before.contours().next().unwrap().id();
        let point_ids = before
            .contours()
            .next()
            .unwrap()
            .points()
            .map(|point| point.id())
            .collect::<Vec<_>>();
        let component_id = before.components().next().unwrap().id();
        let anchor_id = before.anchors().next().unwrap().id();
        let batch = EditBatch {
            task: "identity".into(),
            reason: "preserve canonical identities".into(),
            edits: vec![GlyphEdit {
                glyph: "A".into(),
                expected_revision: canonical_glyph_revision(before).unwrap(),
                operations: vec![Operation::Translate { dx: 12.0, dy: 3.0 }],
            }],
        };
        propose_project(&mut project, source, &batch).unwrap();
        proposal::install_project(&mut project, source, &batch.task, None, true).unwrap();

        let installed = project.document_layer("A", &layer).unwrap();
        assert_eq!(installed.contours().next().unwrap().id(), contour_id);
        assert_eq!(
            installed
                .contours()
                .next()
                .unwrap()
                .points()
                .map(|point| point.id())
                .collect::<Vec<_>>(),
            point_ids
        );
        assert_eq!(installed.components().next().unwrap().id(), component_id);
        assert_eq!(installed.anchors().next().unwrap().id(), anchor_id);
        assert_eq!(
            installed
                .contours()
                .next()
                .unwrap()
                .points()
                .next()
                .unwrap()
                .position(),
            kurbo::Point::new(12.0, 3.0)
        );
        let projected = project.encode_ufo_source(source).unwrap();
        let projected = projected.get_glyph("A").unwrap();
        assert_eq!(projected.note.as_deref(), Some("retain note"));
        assert_eq!(
            projected.lib.get("future.key"),
            Some(&plist::Value::String("exact".into()))
        );
        assert_eq!(projected.codepoints.iter().collect::<Vec<_>>(), ['A']);

        project
            .replay_document_layer_history(&address, HistoryDirection::Undo)
            .unwrap();
        assert_eq!(
            project
                .document_layer("A", &layer)
                .unwrap()
                .contours()
                .next()
                .unwrap()
                .points()
                .next()
                .unwrap()
                .position(),
            kurbo::Point::ZERO
        );
    }

    #[test]
    fn fresh_install_preserves_metadata_and_invalid_numbers_fail() {
        let (mut font, mut batch) = fixture();
        batch.edits[0].operations[0] = Operation::SetWidth { width: f64::NAN };
        assert!(propose(&mut font, &batch).is_err());
        batch.edits[0].operations[0] = Operation::SetWidth { width: 600.0 };
        propose(&mut font, &batch).unwrap();
        let installed =
            proposal::install(&mut font, &batch.task, None, true, &mut |_, _| {}).unwrap();
        assert_eq!(installed.installed, ["n"]);
        assert_eq!(font.get_glyph("n").unwrap().width, 600.0);
        assert!(font.get_glyph("n").unwrap().lib.is_empty());
    }
}

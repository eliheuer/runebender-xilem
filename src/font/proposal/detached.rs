// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Stage a detached worker's proposal as a guarded canonical edit group.

use std::collections::{BTreeMap, BTreeSet};

use kurbo::Point;

use super::{ProposalSummary, layer_name, validate_task};
use crate::font::babelfont::glyph_transactions::proposal_base;
use crate::font::edit_batch::canonical_glyph_revision;
use crate::font::project::{
    CanonicalDocumentEditTransaction, DocumentEditOperation, DocumentLayerEdit, Project,
};
use crate::font::variable::{GlyphLayerAddress, LayerId, SourceId};
use crate::font::{CanonicalLayerSnapshot, LayerView};
use crate::formats::metadata::lib_keys::PROPOSAL_BASE_KEY;
use crate::formats::ufo::glyph_from_layer;

const MAX_CAPTURED_LAYERS: usize = 64;
const MAX_STAGED_OPERATIONS: usize = 256;

/// Immutable foreground scope exported to one detached proposal worker.
#[derive(Clone, Debug)]
pub struct DetachedProposalCapture {
    source: SourceId,
    task: String,
    root_revision: u64,
    original: BTreeMap<String, (CanonicalLayerSnapshot, String)>,
}

/// A validated candidate whose transaction can be committed as one undo group.
#[derive(Clone, Debug)]
pub struct DetachedProposalCandidate {
    summary: ProposalSummary,
    transaction: CanonicalDocumentEditTransaction,
    snapshots: Vec<CanonicalLayerSnapshot>,
}

impl DetachedProposalCapture {
    /// Capture a bounded foreground scope without changing the document.
    /// An empty glyph list selects every foreground glyph in this source.
    pub fn capture(
        project: &Project,
        source: SourceId,
        task: &str,
        glyphs: &[String],
    ) -> Result<Self, String> {
        validate_task(task).map_err(|error| error.to_string())?;
        let foreground = project
            .document_source(source)
            .ok_or("unknown source")?
            .default_layer();
        let names = if glyphs.is_empty() {
            project
                .glyph_names()
                .filter(|name| project.document_layer(name, &foreground).is_some())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        } else {
            glyphs.to_vec()
        };
        if names.is_empty() || names.len() > MAX_CAPTURED_LAYERS {
            return Err(format!(
                "detached proposal requires 1..={MAX_CAPTURED_LAYERS} foreground glyphs"
            ));
        }
        let mut original = BTreeMap::new();
        for name in names {
            let address = GlyphLayerAddress {
                glyph: name.clone(),
                layer: foreground.clone(),
            };
            let snapshot = project
                .capture_document_layer(&address)
                .ok_or_else(|| format!("{name}: foreground layer is missing"))?;
            let revision = canonical_glyph_revision(snapshot.view())?;
            if original
                .insert(name.clone(), (snapshot, revision))
                .is_some()
            {
                return Err(format!("duplicate glyph {name} in detached proposal scope"));
            }
        }
        Ok(Self {
            source,
            task: task.to_owned(),
            root_revision: project.document_revision(),
            original,
        })
    }

    /// Validate one external proposal and stage its complete supported payload.
    /// The root document remains unchanged until the returned transaction is committed.
    pub fn stage(
        &self,
        project: &Project,
        external: &Project,
        external_source: SourceId,
    ) -> Result<DetachedProposalCandidate, String> {
        if project.document_revision() != self.root_revision {
            return Err("root document revision changed after detached capture".into());
        }
        if project.document_source(self.source).is_none() {
            return Err("captured source no longer exists".into());
        }
        let external_foreground = external
            .document_source(external_source)
            .ok_or("external source does not exist")?
            .default_layer();
        let proposal_layer = LayerId {
            source: external_source,
            name: layer_name(&self.task),
        };
        let mut proposed_names = BTreeSet::new();
        for name in external.glyph_names() {
            if external.document_layer(name, &proposal_layer).is_none() {
                continue;
            }
            if !self.original.contains_key(name) {
                return Err(format!("{name}: proposal glyph is outside captured scope"));
            }
            proposed_names.insert(name.to_owned());
        }
        if proposed_names.is_empty() {
            return Err(format!("no proposal for task {}", self.task));
        }

        let mut reads = Vec::with_capacity(self.original.len());
        for (name, (snapshot, _)) in &self.original {
            if project.capture_document_layer(snapshot.address()).as_ref() != Some(snapshot) {
                return Err(format!(
                    "{name}: root foreground changed after detached capture"
                ));
            }
            reads.push(snapshot.clone());
        }

        let mut edits = Vec::new();
        let mut operation_count = 0;
        for name in &proposed_names {
            let (snapshot, revision) = &self.original[name];
            let external_before = external
                .document_layer(name, &external_foreground)
                .ok_or_else(|| format!("{name}: external foreground is missing"))?;
            if canonical_glyph_revision(external_before)? != *revision {
                return Err(format!(
                    "{name}: external foreground differs from captured root"
                ));
            }
            let proposed = external
                .document_layer(name, &proposal_layer)
                .ok_or_else(|| format!("{name}: proposal layer disappeared"))?;
            if glyph_from_layer(proposed)
                .lib
                .contains_key(PROPOSAL_BASE_KEY)
                && proposal_base(proposed) != Some(revision.as_str())
            {
                return Err(format!(
                    "{name}: proposal base revision is stale or malformed"
                ));
            }
            let operations = supported_operations(name, snapshot, external_before, proposed)?;
            operation_count += operations.len();
            if operation_count > MAX_STAGED_OPERATIONS {
                return Err(format!(
                    "detached proposal exceeds {MAX_STAGED_OPERATIONS} edit operations"
                ));
            }
            if !operations.is_empty() {
                edits.push(DocumentLayerEdit::new(snapshot.clone(), operations));
            }
        }
        if edits.is_empty() {
            return Err("proposal contains no supported foreground changes".into());
        }
        let transaction = project
            .begin_document_edit_transaction(
                self.source,
                format!("Install {} proposal", self.task),
                reads,
                edits,
            )
            .map_err(|error| error.to_string())?;
        let snapshots = project
            .preview_document_edit_transaction(&transaction)
            .map_err(|error| error.to_string())?;
        let names = proposed_names.into_iter().collect::<Vec<_>>();
        Ok(DetachedProposalCandidate {
            summary: ProposalSummary {
                task: self.task.clone(),
                layer: proposal_layer.name,
                glyphs: names.clone(),
                compatible: names,
                incompatible: Vec::new(),
                missing: Vec::new(),
            },
            transaction,
            snapshots,
        })
    }
}

impl DetachedProposalCandidate {
    /// The complete validated proposal scope.
    pub fn summary(&self) -> &ProposalSummary {
        &self.summary
    }

    /// The guarded transaction to commit once the user accepts the candidate.
    pub fn transaction(&self) -> &CanonicalDocumentEditTransaction {
        &self.transaction
    }

    /// Immutable staged foreground layers for preview before Apply.
    pub fn snapshots(&self) -> &[CanonicalLayerSnapshot] {
        &self.snapshots
    }
}

fn supported_operations(
    name: &str,
    original: &CanonicalLayerSnapshot,
    external_before: LayerView<'_>,
    proposed: LayerView<'_>,
) -> Result<Vec<DocumentEditOperation>, String> {
    let root = original.view();
    if root.components().next().is_some()
        || external_before.components().next().is_some()
        || proposed.components().next().is_some()
    {
        return Err(format!(
            "{name}: component proposals need dependency capture"
        ));
    }
    let mut operations = Vec::new();
    let width = proposed.width();
    if !width.is_finite() || width < 0.0 {
        return Err(format!(
            "{name}: proposed width is not finite and nonnegative"
        ));
    }
    if root.width() != width {
        operations.push(DocumentEditOperation::SetWidth(width));
    }
    let root_contours = root.contours().collect::<Vec<_>>();
    let proposed_contours = proposed.contours().collect::<Vec<_>>();
    if root_contours.len() != proposed_contours.len() {
        return Err(format!("{name}: proposal changes contour count"));
    }
    for (before, after) in root_contours.iter().zip(&proposed_contours) {
        if before.is_hyper() || after.is_hyper() {
            return Err(format!(
                "{name}: hyper contours are not supported in detached proposals"
            ));
        }
        if before.is_closed() != after.is_closed() {
            return Err(format!("{name}: proposal changes contour structure"));
        }
        let before_points = before.points().collect::<Vec<_>>();
        let after_points = after.points().collect::<Vec<_>>();
        if before_points.len() != after_points.len() {
            return Err(format!("{name}: proposal changes point count"));
        }
        for (before, after) in before_points.iter().zip(&after_points) {
            if before.point_type() != after.point_type()
                || before.is_smooth() != after.is_smooth()
                || before.name() != after.name()
            {
                return Err(format!("{name}: proposal changes point type or metadata"));
            }
            let position = after.position();
            if !finite(position) {
                return Err(format!("{name}: proposed point position is not finite"));
            }
            if before.position() != position {
                operations.push(DocumentEditOperation::SetPoint {
                    point: before.id(),
                    position,
                });
            }
        }
    }
    let before_anchors = root.anchors().collect::<Vec<_>>();
    let after_anchors = proposed.anchors().collect::<Vec<_>>();
    if before_anchors.len() != after_anchors.len() {
        return Err(format!("{name}: proposal changes anchor count"));
    }
    for (before, after) in before_anchors.iter().zip(&after_anchors) {
        if before.name() != after.name() {
            return Err(format!("{name}: proposal changes anchor metadata"));
        }
        let position = after.position();
        if !finite(position) {
            return Err(format!("{name}: proposed anchor position is not finite"));
        }
        if before.position() != position {
            operations.push(DocumentEditOperation::SetAnchor {
                anchor: before.id(),
                position,
            });
        }
    }

    // The external glyph may add only the proposal-base receipt. Restoring the
    // supported coordinates must yield the exact captured foreground payload.
    let external_glyph = glyph_from_layer(external_before);
    let mut normalized = glyph_from_layer(proposed);
    normalized.width = external_glyph.width;
    for (contour, before) in normalized.contours.iter_mut().zip(&external_glyph.contours) {
        for (point, original) in contour.points.iter_mut().zip(&before.points) {
            point.x = original.x;
            point.y = original.y;
        }
    }
    for (anchor, before) in normalized.anchors.iter_mut().zip(&external_glyph.anchors) {
        anchor.x = before.x;
        anchor.y = before.y;
    }
    normalized.lib.remove(PROPOSAL_BASE_KEY);
    let mut external_glyph = external_glyph;
    external_glyph.lib.remove(PROPOSAL_BASE_KEY);
    if normalized != external_glyph {
        return Err(format!(
            "{name}: proposal changes unsupported glyph metadata"
        ));
    }
    Ok(operations)
}

fn finite(point: Point) -> bool {
    point.x.is_finite() && point.y.is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::history::HistoryDirection;
    use crate::font::project::{
        DocumentEditObjectKind, DocumentEditTransactionOutcome, SourceInput,
    };

    fn glyph(name: &str) -> norad::Glyph {
        let mut glyph = norad::Glyph::new(name);
        glyph.width = 500.0;
        glyph.contours.push(norad::Contour::new(
            vec![
                norad::ContourPoint::new(0.0, 0.0, norad::PointType::Line, false, None, None),
                norad::ContourPoint::new(100.0, 0.0, norad::PointType::Line, false, None, None),
                norad::ContourPoint::new(100.0, 100.0, norad::PointType::Line, false, None, None),
                norad::ContourPoint::new(0.0, 100.0, norad::PointType::Line, false, None, None),
            ],
            None,
        ));
        glyph
    }

    fn font() -> norad::Font {
        let mut font = norad::Font::new();
        font.default_layer_mut().insert_glyph(glyph("A"));
        font.default_layer_mut().insert_glyph(glyph("B"));
        font
    }

    fn project(font: norad::Font) -> Project {
        Project::from_source(SourceInput::from_font(font, "Detached.ufo".into()))
    }

    fn with_proposal(mut source: norad::Font, glyphs: Vec<norad::Glyph>) -> Project {
        let layer = source.layers.new_layer(&layer_name("bolden")).unwrap();
        for glyph in glyphs {
            layer.insert_glyph(glyph);
        }
        project(source)
    }

    #[test]
    fn stages_exact_point_and_width_ids_without_mutating_root_then_undoes_as_one_group() {
        let source = font();
        let mut root = project(source.clone());
        let source_id = SourceId(0);
        let capture =
            DetachedProposalCapture::capture(&root, source_id, "bolden", &["A".into(), "B".into()])
                .unwrap();
        let mut a = source.get_glyph("A").unwrap().clone();
        a.width = 560.0;
        a.contours[0].points[0].x = 20.0;
        let mut b = source.get_glyph("B").unwrap().clone();
        b.width = 580.0;
        let external = with_proposal(source, vec![a, b]);
        let revision = root.document_revision();
        let foreground = root.document_source(source_id).unwrap().default_layer();
        let point_id = root
            .document_layer("A", &foreground)
            .unwrap()
            .contours()
            .next()
            .unwrap()
            .points()
            .next()
            .unwrap()
            .id();
        let candidate = capture.stage(&root, &external, source_id).unwrap();
        assert_eq!(candidate.summary().glyphs, ["A", "B"]);
        assert_eq!(candidate.snapshots().len(), 2);
        assert_eq!(root.document_revision(), revision);
        assert_eq!(
            root.document_layer("A", &foreground).unwrap().width(),
            500.0
        );
        let DocumentEditTransactionOutcome::Changed {
            changed_objects,
            history_group,
            ..
        } = root
            .commit_document_edit_transaction(candidate.transaction().clone())
            .unwrap()
        else {
            panic!("proposal should change foreground");
        };
        assert!(changed_objects.iter().any(|object| {
            object.glyph == "A" && object.object == DocumentEditObjectKind::Point(point_id)
        }));
        assert_eq!(
            root.document_layer("A", &foreground).unwrap().width(),
            560.0
        );
        assert_eq!(
            root.document_layer("B", &foreground).unwrap().width(),
            580.0
        );
        root.replay_document_edit_history_group(history_group, HistoryDirection::Undo)
            .unwrap();
        assert_eq!(
            root.document_layer("A", &foreground).unwrap().width(),
            500.0
        );
        assert_eq!(
            root.document_layer("B", &foreground).unwrap().width(),
            500.0
        );
    }

    #[test]
    fn rejects_stale_root_external_foreground_and_out_of_scope_proposals() {
        let source = font();
        let root = project(source.clone());
        let capture =
            DetachedProposalCapture::capture(&root, SourceId(0), "bolden", &["A".into()]).unwrap();
        let mut a = source.get_glyph("A").unwrap().clone();
        a.width = 560.0;
        let mut b = source.get_glyph("B").unwrap().clone();
        b.width = 570.0;
        let outside = with_proposal(source.clone(), vec![a.clone(), b]);
        assert!(
            capture
                .stage(&root, &outside, SourceId(0))
                .unwrap_err()
                .contains("outside")
        );

        let mut changed_external = source.clone();
        changed_external.get_glyph_mut("A").unwrap().width = 555.0;
        let external = with_proposal(changed_external, vec![a.clone()]);
        assert!(
            capture
                .stage(&root, &external, SourceId(0))
                .unwrap_err()
                .contains("external foreground")
        );

        let external = with_proposal(source, vec![a]);
        let mut stale_root = root;
        let foreground = stale_root
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let address = GlyphLayerAddress {
            glyph: "A".into(),
            layer: foreground,
        };
        let before = stale_root.capture_document_layer(&address).unwrap();
        let transaction = stale_root
            .begin_document_edit_transaction(
                SourceId(0),
                "change root",
                vec![],
                vec![DocumentLayerEdit::new(
                    before,
                    vec![DocumentEditOperation::SetWidth(700.0)],
                )],
            )
            .unwrap();
        stale_root
            .commit_document_edit_transaction(transaction)
            .unwrap();
        assert!(
            capture
                .stage(&stale_root, &external, SourceId(0))
                .unwrap_err()
                .contains("root document revision")
        );
    }

    #[test]
    fn rejects_missing_proposal_wrong_base_and_unsupported_structure_or_metadata() {
        let source = font();
        let root = project(source.clone());
        let capture =
            DetachedProposalCapture::capture(&root, SourceId(0), "bolden", &["A".into()]).unwrap();
        assert!(
            capture
                .stage(&root, &project(source.clone()), SourceId(0))
                .unwrap_err()
                .contains("no proposal")
        );

        let mut a = source.get_glyph("A").unwrap().clone();
        a.width = 560.0;
        let mut record = plist::Dictionary::new();
        record.insert("revision".into(), "wrong".into());
        a.lib.insert(PROPOSAL_BASE_KEY.into(), record.into());
        assert!(
            capture
                .stage(
                    &root,
                    &with_proposal(source.clone(), vec![a.clone()]),
                    SourceId(0)
                )
                .unwrap_err()
                .contains("base revision")
        );

        a.lib.insert(PROPOSAL_BASE_KEY.into(), true.into());
        assert!(
            capture
                .stage(
                    &root,
                    &with_proposal(source.clone(), vec![a.clone()]),
                    SourceId(0)
                )
                .unwrap_err()
                .contains("base revision")
        );

        a.lib.remove(PROPOSAL_BASE_KEY);
        a.contours[0].points.pop();
        assert!(
            capture
                .stage(&root, &with_proposal(source.clone(), vec![a]), SourceId(0))
                .unwrap_err()
                .contains("point count")
        );
        let mut metadata = source.get_glyph("A").unwrap().clone();
        metadata.width = 560.0;
        metadata.note = Some("unsupported".into());
        assert!(
            capture
                .stage(&root, &with_proposal(source, vec![metadata]), SourceId(0))
                .unwrap_err()
                .contains("unsupported glyph metadata")
        );
    }
}

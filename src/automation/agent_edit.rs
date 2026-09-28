// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Strict live edit requests resolved against canonical document identities.
//!
//! The application owns authorization, epoch binding, receipts and UI history.
//! This adapter only validates typed wire input and stages one canonical transaction.

/// Typed transport receipts and complete normal response schemas for guarded edits.
pub mod results;

use kurbo::Point;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::agent::Tool;
use super::agent_session::{AgentOperationRejection, AgentPayloadDigest};
use crate::font::CanonicalLayerSnapshot;
use crate::font::edit_batch::canonical_glyph_revision;
use crate::font::project::{
    CanonicalDocumentEditTransaction, DocumentEditOperation, DocumentLayerEdit, Project,
};
use crate::font::variable::{GlyphLayerAddress, LayerId, SourceId};

/// An explicit layer read, including identity and the revision returned by `read_glyph`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentLayerGuard {
    /// Glyph name at the time of the read.
    pub glyph: String,
    /// Opaque glyph identity; protects against delete-and-recreate under the same name.
    pub glyph_id: String,
    /// Exact layer name within the request's stable source.
    pub layer: String,
    /// Canonical glyph revision returned by the read adapter.
    pub expected_revision: String,
}

/// One guarded operation over existing objects or newly generated contours.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentEditOperation {
    /// Replace the exact horizontal advance.
    SetWidth {
        /// New advance in font units.
        width: f64,
    },
    /// Move one existing contour point.
    SetPoint {
        /// Opaque point identity from the guarded read.
        point_id: String,
        /// New horizontal coordinate.
        x: f64,
        /// New vertical coordinate.
        y: f64,
    },
    /// Append ordinary contours with identities minted by the canonical font engine.
    AppendContours {
        /// Ordered contours in font coordinates; a leading move starts an open contour.
        contours: Vec<crate::outline::drawing::DrawingContour>,
    },
    /// Move one existing anchor.
    SetAnchor {
        /// Opaque anchor identity from the guarded read.
        anchor_id: String,
        /// New horizontal coordinate.
        x: f64,
        /// New vertical coordinate.
        y: f64,
    },
}

/// Guarded operations for one layer.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentLayerEdits {
    /// Layer state used to derive these operations.
    pub target: AgentLayerGuard,
    /// Operations in execution order.
    pub operations: Vec<AgentEditOperation>,
}

/// Complete semantic payload for one receipt-backed live edit.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentEditRequest {
    /// Required endpoint lifetime guard.
    pub expected_document_epoch: String,
    /// Bounded caller label, not an authentication credential.
    pub actor: String,
    /// Actor-local retry identity.
    pub operation_key: String,
    /// Must be `user-approved`, reflecting existing user authorization.
    pub authorization: String,
    /// Explicit stable source ID, never an active-source default.
    pub source: usize,
    /// Name used for the grouped history entry.
    pub history_name: String,
    /// Additional layer reads on which the edit depends.
    #[serde(default)]
    pub reads: Vec<AgentLayerGuard>,
    /// Complete bounded write set.
    pub edits: Vec<AgentLayerEdits>,
}

impl AgentEditRequest {
    /// Hash the normalized complete typed payload, including guards and authorization.
    pub fn payload_digest(&self) -> AgentPayloadDigest {
        AgentPayloadDigest::sha256(&serde_json::to_vec(self).expect("typed request serializes"))
    }

    /// Resolve opaque identities and revisions, then stage without mutating the document.
    pub fn stage(
        &self,
        project: &Project,
    ) -> Result<CanonicalDocumentEditTransaction, AgentOperationRejection> {
        self.stage_with_parent(project, None)
    }

    /// Extend a detached candidate after resolving guards against its exact staged overlay.
    pub fn stage_after(
        &self,
        project: &Project,
        parent: &CanonicalDocumentEditTransaction,
    ) -> Result<CanonicalDocumentEditTransaction, AgentOperationRejection> {
        self.stage_with_parent(project, Some(parent))
    }

    fn stage_with_parent(
        &self,
        project: &Project,
        parent: Option<&CanonicalDocumentEditTransaction>,
    ) -> Result<CanonicalDocumentEditTransaction, AgentOperationRejection> {
        let invalid = |message: &str| AgentOperationRejection::InvalidRequest(message.into());
        if self.edits.is_empty() || self.edits.len() + self.reads.len() > 64 {
            return Err(invalid("supply edits and at most 64 guarded layer entries"));
        }
        if self.edits.iter().any(|edit| edit.operations.is_empty())
            || self
                .edits
                .iter()
                .map(|edit| edit.operations.len())
                .sum::<usize>()
                > 256
        {
            return Err(invalid(
                "supply 1..=256 operations, with at least one per edited layer",
            ));
        }
        let (mut contour_count, mut point_count) = (0_usize, 0_usize);
        for operation in self.edits.iter().flat_map(|edit| &edit.operations) {
            if let AgentEditOperation::AppendContours { contours } = operation {
                contour_count = contour_count.saturating_add(contours.len());
                for contour in contours {
                    point_count = point_count.saturating_add(contour.points.len());
                }
            }
        }
        if contour_count > crate::font::generated::MAX_GENERATED_CONTOURS
            || point_count > crate::font::generated::MAX_GENERATED_POINTS
        {
            return Err(invalid(
                "generated contours exceed the whole-batch geometry limit",
            ));
        }
        let source = SourceId(self.source);
        let reads = self
            .reads
            .iter()
            .map(|guard| guard.resolve(project, source, parent))
            .collect::<Result<Vec<_>, _>>()?;
        let edits = self
            .edits
            .iter()
            .map(|edit| {
                let snapshot = edit.target.resolve(project, source, parent)?;
                let layer = snapshot.view();
                let operations = edit
                    .operations
                    .iter()
                    .map(|operation| match operation {
                        AgentEditOperation::SetWidth { width } => {
                            Ok(DocumentEditOperation::SetWidth(*width))
                        }
                        AgentEditOperation::AppendContours { contours } => {
                            Ok(DocumentEditOperation::AppendContours(
                                contours.iter().map(Into::into).collect(),
                            ))
                        }
                        AgentEditOperation::SetPoint { point_id, x, y } => {
                            let point = layer
                                .contours()
                                .flat_map(|contour| contour.points())
                                .find(|point| point.id().to_wire() == *point_id)
                                .ok_or_else(|| {
                                    invalid("point identity is absent from the guarded layer")
                                })?;
                            Ok(DocumentEditOperation::SetPoint {
                                point: point.id(),
                                position: Point::new(*x, *y),
                            })
                        }
                        AgentEditOperation::SetAnchor { anchor_id, x, y } => {
                            let anchor = layer
                                .anchors()
                                .find(|anchor| anchor.id().to_wire() == *anchor_id)
                                .ok_or_else(|| {
                                    invalid("anchor identity is absent from the guarded layer")
                                })?;
                            Ok(DocumentEditOperation::SetAnchor {
                                anchor: anchor.id(),
                                position: Point::new(*x, *y),
                            })
                        }
                    })
                    .collect::<Result<Vec<_>, AgentOperationRejection>>()?;
                Ok(DocumentLayerEdit::new(snapshot, operations))
            })
            .collect::<Result<Vec<_>, AgentOperationRejection>>()?;
        match parent {
            Some(parent) => project
                .extend_document_edit_transaction(parent, &self.history_name, reads, edits)
                .map_err(Into::into),
            None => project
                .begin_document_edit_transaction(source, &self.history_name, reads, edits)
                .map_err(Into::into),
        }
    }
}

impl AgentLayerGuard {
    fn resolve(
        &self,
        project: &Project,
        source: SourceId,
        parent: Option<&CanonicalDocumentEditTransaction>,
    ) -> Result<CanonicalLayerSnapshot, AgentOperationRejection> {
        let invalid = |message: &str| AgentOperationRejection::InvalidRequest(message.into());
        if [
            &self.glyph,
            &self.glyph_id,
            &self.layer,
            &self.expected_revision,
        ]
        .iter()
        .any(|value| value.is_empty() || value.len() > 256)
        {
            return Err(invalid("guard strings must contain 1..=256 bytes"));
        }
        let glyph = project
            .document_glyph(&self.glyph)
            .ok_or_else(|| invalid("glyph no longer exists"))?;
        if glyph.id().to_wire() != self.glyph_id {
            return Err(invalid(
                "glyph identity changed; read the intended glyph again",
            ));
        }
        let address = GlyphLayerAddress {
            glyph: self.glyph.clone(),
            layer: LayerId {
                source,
                name: self.layer.clone(),
            },
        };
        let snapshot = match parent {
            Some(parent) => project
                .capture_document_edit_layer(parent, &address)
                .map_err(AgentOperationRejection::from)?,
            None => project
                .capture_document_layer(&address)
                .ok_or_else(|| invalid("explicit source/layer does not contain the glyph"))?,
        };
        let revision = canonical_glyph_revision(snapshot.view())
            .map_err(AgentOperationRejection::InvalidRequest)?;
        if revision != self.expected_revision {
            return Err(invalid(
                "guarded layer changed; read it again before a new operation",
            ));
        }
        Ok(snapshot)
    }
}

/// Generated live tool inventory for the strict application edit boundary.
pub fn tools() -> Vec<Tool> {
    let string = json!({"type":"string","minLength":1,"maxLength":256});
    let guard = json!({"type":"object","properties":{"glyph":string,"glyph_id":string,"layer":string,"expected_revision":string},"required":["glyph","glyph_id","layer","expected_revision"],"additionalProperties":false});
    let mut generated_contour = serde_json::to_value(
        schemars::generate::SchemaSettings::default()
            .with(|settings| settings.inline_subschemas = true)
            .into_generator()
            .into_root_schema_for::<crate::outline::drawing::DrawingContour>(),
    )
    .expect("drawing input schema serializes");
    generated_contour
        .as_object_mut()
        .expect("contour schema is an object")
        .remove("$schema");
    generated_contour["properties"]["points"]["minItems"] = json!(2);
    generated_contour["properties"]["points"]["maxItems"] =
        json!(crate::font::generated::MAX_GENERATED_POINTS);
    for coordinate in ["x", "y"] {
        let property =
            &mut generated_contour["properties"]["points"]["items"]["properties"][coordinate];
        property["minimum"] = json!(-1_000_000.0);
        property["maximum"] = json!(1_000_000.0);
    }
    let operation = json!({"oneOf":[
        {"type":"object","properties":{"op":{"const":"append_contours"},"contours":{"type":"array","minItems":1,"maxItems":crate::font::generated::MAX_GENERATED_CONTOURS,"items":generated_contour}},"required":["op","contours"],"additionalProperties":false},
        {"type":"object","properties":{"op":{"const":"set_width"},"width":{"type":"number"}},"required":["op","width"],"additionalProperties":false},
        {"type":"object","properties":{"op":{"const":"set_point"},"point_id":string,"x":{"type":"number"},"y":{"type":"number"}},"required":["op","point_id","x","y"],"additionalProperties":false},
        {"type":"object","properties":{"op":{"const":"set_anchor"},"anchor_id":string,"x":{"type":"number"},"y":{"type":"number"}},"required":["op","anchor_id","x","y"],"additionalProperties":false}
    ]});
    let mut identity =
        json!({"expected_document_epoch":string,"actor":string,"operation_key":string});
    let mut apply = identity.clone();
    apply["authorization"] = json!({"enum":["user-approved"]});
    apply["source"] = json!({"type":"integer","minimum":0});
    apply["history_name"] = string;
    apply["reads"] = json!({"type":"array","maxItems":64,"items":guard});
    apply["edits"] = json!({"type":"array","minItems":1,"maxItems":64,"items":{"type":"object","properties":{"target":guard,"operations":{"type":"array","minItems":1,"maxItems":256,"items":operation}},"required":["target","operations"],"additionalProperties":false}});
    let make = |name: &str, description: &str, properties: Value, required: Value| Tool {
        name: name.into(),
        description: description.into(),
        parameters: json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),
    };
    let mut result = vec![
        make(
            "agent_apply",
            "Apply one authorized guarded batch to the unsaved root with one history group. Append contours are bounded to 256 contours and 4096 points across the complete batch, with engine-owned identities. Read explicit glyph/source/layer identities first. Reuse exactly the same actor, operation_key and payload after a lost response; a different payload under that key rejects. In-memory receipts do not survive document closure.",
            apply,
            json!([
                "expected_document_epoch",
                "actor",
                "operation_key",
                "authorization",
                "source",
                "history_name",
                "edits"
            ]),
        ),
        make(
            "agent_receipt",
            "Look up the immutable original apply receipt and separate current history state. Never reapplies or saves an edit.",
            identity.clone(),
            json!(["expected_document_epoch", "actor", "operation_key"]),
        ),
        make(
            "agent_cancel",
            "Prevent an admitted agent_apply from committing, addressed by its exact document epoch, actor and operation key. Returns prevented, already_prevented, too_late, committed, completed or unknown. Cancellation never undoes a committed edit; use agent_history for that.",
            identity.clone(),
            json!(["expected_document_epoch", "actor", "operation_key"]),
        ),
    ];
    identity["direction"] = json!({"enum":["undo","redo"]});
    identity["authorization"] = json!({"enum":["user-approved"]});
    result.push(make("agent_history", "Undo or redo the group associated with an apply receipt, sharing ordinary editor history. Overlapping later changes reject. After a lost response inspect agent_receipt history_state before retrying; this command is not an idempotent apply.", identity, json!(["expected_document_epoch","actor","operation_key","direction","authorization"])));
    result
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use norad::{Contour, ContourPoint, Font, Glyph, PointType};

    use super::*;
    use crate::automation::script_recipe::{capture, capture_staged};
    use crate::font::project::SourceInput;
    use crate::outline::drawing::{DrawingContour, DrawingPoint, DrawingPointType};

    fn project() -> Project {
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(Glyph::new("A"));
        Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("staged-recipe.ufo"),
        ))
    }

    fn request(
        source: SourceId,
        guard: AgentLayerGuard,
        operation: AgentEditOperation,
    ) -> AgentEditRequest {
        AgentEditRequest {
            expected_document_epoch: "test-epoch".into(),
            actor: "test".into(),
            operation_key: "test-operation".into(),
            authorization: "user-approved".into(),
            source: source.0,
            history_name: "staged recipe".into(),
            reads: Vec::new(),
            edits: vec![AgentLayerEdits {
                target: guard,
                operations: vec![operation],
            }],
        }
    }

    fn rectangle() -> DrawingContour {
        DrawingContour {
            points: [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)]
                .into_iter()
                .map(|(x, y)| DrawingPoint {
                    x,
                    y,
                    kind: DrawingPointType::Line,
                    smooth: false,
                })
                .collect(),
        }
    }

    #[test]
    fn staged_capture_can_address_generated_point_without_mutating_root() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let glyphs = ["A".to_owned()];
        let root = capture(&project, source, &glyphs, "root".into(), BTreeMap::new()).unwrap();
        let parent = request(
            source,
            root.layers[0].guard.clone(),
            AgentEditOperation::AppendContours {
                contours: vec![rectangle()],
            },
        )
        .stage(&project)
        .unwrap();
        let staged = capture_staged(
            &project,
            &parent,
            source,
            &glyphs,
            "staged".into(),
            BTreeMap::new(),
        )
        .unwrap();
        let inserted_id = staged.layers[0].contours[0].points[0].id.clone();
        assert!(root.layers[0].contours.is_empty());
        assert_eq!(
            project
                .document_layer(
                    "A",
                    &project.document_source(source).unwrap().default_layer()
                )
                .unwrap()
                .contours()
                .count(),
            0
        );

        let child = request(
            source,
            staged.layers[0].guard.clone(),
            AgentEditOperation::SetPoint {
                point_id: inserted_id.clone(),
                x: 25.0,
                y: 15.0,
            },
        )
        .stage_after(&project, &parent)
        .unwrap();
        let child_capture = capture_staged(
            &project,
            &child,
            source,
            &glyphs,
            "child".into(),
            BTreeMap::new(),
        )
        .unwrap();
        let moved = &child_capture.layers[0].contours[0].points[0];
        assert_eq!(moved.id, inserted_id);
        assert_eq!((moved.x, moved.y), (25.0, 15.0));
        assert!(
            project
                .document_layer(
                    "A",
                    &project.document_source(source).unwrap().default_layer()
                )
                .unwrap()
                .contours()
                .next()
                .is_none()
        );

        project.commit_document_edit_transaction(child).unwrap();
        let committed = capture(
            &project,
            source,
            &glyphs,
            "committed".into(),
            BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(committed.layers[0].contours[0].points[0].id, inserted_id);
        assert_eq!(committed.layers[0].contours[0].points[0].x, 25.0);
    }

    #[test]
    fn siblings_are_detached_and_stale_guards_reject() {
        let mut project = project();
        let source = project.source_id(0).unwrap();
        let glyphs = ["A".to_owned()];
        let root = capture(&project, source, &glyphs, "root".into(), BTreeMap::new()).unwrap();
        let first = request(
            source,
            root.layers[0].guard.clone(),
            AgentEditOperation::SetWidth { width: 600.0 },
        )
        .stage(&project)
        .unwrap();
        let second = request(
            source,
            root.layers[0].guard.clone(),
            AgentEditOperation::SetWidth { width: 700.0 },
        )
        .stage(&project)
        .unwrap();
        let first_capture = capture_staged(
            &project,
            &first,
            source,
            &glyphs,
            "first".into(),
            BTreeMap::new(),
        )
        .unwrap();
        let second_capture = capture_staged(
            &project,
            &second,
            source,
            &glyphs,
            "second".into(),
            BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(first_capture.layers[0].width, 600.0);
        assert_eq!(second_capture.layers[0].width, 700.0);
        assert_eq!(root.layers[0].width, 0.0);
        assert!(
            request(
                source,
                root.layers[0].guard.clone(),
                AgentEditOperation::SetWidth { width: 800.0 }
            )
            .stage_after(&project, &first)
            .is_err()
        );
        project.commit_document_edit_transaction(first).unwrap();
        assert!(
            capture_staged(
                &project,
                &second,
                source,
                &glyphs,
                "stale".into(),
                BTreeMap::new()
            )
            .is_err()
        );
    }

    #[test]
    fn staged_capture_enforces_whole_input_geometry_budget() {
        let mut font = Font::new();
        let mut glyph = Glyph::new("A");
        glyph.contours.push(Contour::new(
            vec![
                ContourPoint::new(0.0, 0.0, PointType::Line, false, None, None),
                ContourPoint::new(10.0, 10.0, PointType::Line, false, None, None),
            ],
            None,
        ));
        font.default_layer_mut().insert_glyph(glyph);
        let project = Project::from_source(SourceInput::from_font(
            font,
            PathBuf::from("staged-budget.ufo"),
        ));
        let source = project.source_id(0).unwrap();
        let glyphs = ["A".to_owned()];
        let root = capture(&project, source, &glyphs, "root".into(), BTreeMap::new()).unwrap();
        let parent = request(
            source,
            root.layers[0].guard.clone(),
            AgentEditOperation::AppendContours {
                contours: vec![rectangle(); 256],
            },
        )
        .stage(&project)
        .unwrap();
        let error = capture_staged(
            &project,
            &parent,
            source,
            &glyphs,
            "oversize".into(),
            BTreeMap::new(),
        )
        .unwrap_err();
        assert!(error.contains("256 contours"));
    }

    #[test]
    fn generated_input_schema_bounds_geometry_without_accepting_caller_ids() {
        let tools = tools();
        let schema = &tools
            .iter()
            .find(|tool| tool.name == "agent_apply")
            .unwrap()
            .parameters;
        let choices =
            schema["properties"]["edits"]["items"]["properties"]["operations"]["items"]["oneOf"]
                .as_array()
                .unwrap();
        let append = choices
            .iter()
            .find(|choice| choice["properties"]["op"]["const"] == "append_contours")
            .unwrap();
        let contours = &append["properties"]["contours"];
        assert_eq!(contours["maxItems"], 256);
        let points = &contours["items"]["properties"]["points"];
        assert_eq!(points["maxItems"], 4096);
        assert_eq!(points["items"]["additionalProperties"], false);
        assert!(points["items"]["properties"].get("id").is_none());
        assert_eq!(points["items"]["properties"]["x"]["maximum"], 1_000_000.0);
        assert!(
            serde_json::from_value::<AgentEditOperation>(json!({
                "op":"append_contours","contours":[{"points":[
                    {"x":0,"y":0,"type":"line","id":"caller-point"}
                ]}]
            }))
            .is_err()
        );
    }
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Layer draft lifecycle, contour construction, importing, and duplication.

use super::*;
use crate::font::generated::{GeneratedContour, validate_contours};

impl LayerEditDraft {
    pub(in crate::font) fn new(layer: Layer, preserved: LayerPreservation) -> Self {
        Self { layer, preserved }
    }

    pub(in crate::font) fn into_parts(self) -> (Layer, LayerPreservation) {
        (self.layer, self.preserved)
    }

    pub(in crate::font) fn delta_from(
        &self,
        layer: &Layer,
        preserved: &LayerPreservation,
    ) -> LayerDelta {
        let metrics =
            self.preserved.width != preserved.width || self.preserved.height != preserved.height;
        let exact_components = |items: &[PreservedComponent]| {
            items
                .iter()
                .map(|item| (item.id, item.transform))
                .collect::<Vec<_>>()
        };
        let geometry = self.layer.shapes != layer.shapes
            || self.layer.anchors != layer.anchors
            || exact_components(&self.preserved.components)
                != exact_components(&preserved.components);
        let metadata = self.preserved.name != preserved.name
            || self.preserved.codepoints != preserved.codepoints
            || self.preserved.note != preserved.note
            || self.preserved.guidelines != preserved.guidelines
            || self.preserved.image != preserved.image
            || self.preserved.lib != preserved.lib
            || self.preserved.mark_color != preserved.mark_color
            || self.preserved.left_metrics_key != preserved.left_metrics_key
            || self.preserved.right_metrics_key != preserved.right_metrics_key
            || self.preserved.metaballs != preserved.metaballs
            || self.preserved.composition_recipe != preserved.composition_recipe
            || self.preserved.smart_component_axes != preserved.smart_component_axes
            || self.preserved.smart_component_values != preserved.smart_component_values
            || self.preserved.smart_component_pole != preserved.smart_component_pole
            || self.preserved.hoi_intermediates != preserved.hoi_intermediates
            || self.preserved.contours != preserved.contours
            || self
                .preserved
                .components
                .iter()
                .map(|item| (item.id, &item.alignment, &item.metadata))
                .ne(preserved
                    .components
                    .iter()
                    .map(|item| (item.id, &item.alignment, &item.metadata)))
            || self.preserved.anchors != preserved.anchors;
        LayerDelta {
            geometry,
            metrics,
            metadata,
        }
    }

    /// Read the draft using the same canonical view as a committed layer.
    pub fn view(&self) -> LayerView<'_> {
        LayerView::new(&self.layer, &self.preserved)
    }

    /// Replace the Unicode scalar values, retaining order and removing later duplicates.
    pub fn set_codepoints(&mut self, codepoints: impl IntoIterator<Item = char>) -> bool {
        let mut codepoints = codepoints.into_iter().collect::<Vec<_>>();
        let mut seen = HashSet::new();
        codepoints.retain(|codepoint| seen.insert(*codepoint));
        if self.preserved.codepoints == codepoints {
            return false;
        }
        self.preserved.codepoints = codepoints;
        true
    }

    /// Replace the optional source glyph note.
    pub fn set_note(&mut self, note: Option<String>) -> bool {
        if self.preserved.note == note {
            return false;
        }
        self.preserved.note = note;
        true
    }

    /// Insert or replace one exact source glyph library value.
    pub fn set_lib_value(&mut self, key: String, value: plist::Value) -> bool {
        if self.preserved.lib.get(&key) == Some(&value) {
            return false;
        }
        self.preserved.lib.insert(key, value);
        true
    }

    /// Remove every contour while retaining components, anchors and layer metadata.
    pub fn clear_contours(&mut self) -> bool {
        if self.preserved.contours.is_empty() {
            return false;
        }
        self.layer
            .shapes
            .retain(|shape| !matches!(shape, Shape::Path(_)));
        self.preserved.contours.clear();
        true
    }

    /// Append validated ordinary generated contours with fresh stable identities.
    ///
    /// The entire slice is checked before the draft changes. Existing contours, components,
    /// anchors, advances and metadata remain untouched.
    pub fn append_generated_contours(
        &mut self,
        contours: &[GeneratedContour],
    ) -> Result<PastedContours, DocumentEditError> {
        validate_contours(contours)?;
        let mut inserted = PastedContours::default();
        let mut additions = Vec::with_capacity(contours.len());
        for contour in contours {
            let contour_id = ContourId::next();
            inserted.contours.push(contour_id);
            let mut path = babelfont::Path {
                closed: contour.points[0].point_type != LayerPointType::Move,
                ..babelfont::Path::default()
            };
            write_id(&mut path.format_specific, contour_id.0);
            let mut preserved_points = Vec::with_capacity(contour.points.len());
            for point in &contour.points {
                let node_type = match point.point_type {
                    LayerPointType::Move => NodeType::Move,
                    LayerPointType::Line => NodeType::Line,
                    LayerPointType::OffCurve => NodeType::OffCurve,
                    LayerPointType::Curve => NodeType::Curve,
                    LayerPointType::QCurve => NodeType::QCurve,
                };
                let (id, node, preserved) =
                    new_document_point(point.position, node_type, point.smooth);
                inserted.points.push(id);
                path.nodes.push(node);
                preserved_points.push(preserved);
            }
            additions.push((
                Shape::Path(path),
                PreservedContour {
                    id: contour_id,
                    hyper: false,
                    metadata: ObjectMetadata {
                        identifier: None,
                        lib: None,
                    },
                    points: preserved_points,
                },
            ));
        }
        for (shape, preserved) in additions {
            self.layer.shapes.push(shape);
            self.preserved.contours.push(preserved);
        }
        Ok(inserted)
    }

    /// Start a new open contour at `position`.
    ///
    /// Returns the stable contour and initial-point identities.
    pub fn start_contour(
        &mut self,
        position: kurbo::Point,
    ) -> Result<(ContourId, PointId), DocumentEditError> {
        ensure_finite(&[position.x, position.y])?;
        let contour_id = ContourId::next();
        let (point_id, point, preserved_point) =
            new_document_point(position, NodeType::Move, false);
        let mut path = babelfont::Path {
            nodes: vec![point],
            closed: false,
            ..babelfont::Path::default()
        };
        write_id(&mut path.format_specific, contour_id.0);
        self.layer.shapes.push(Shape::Path(path));
        self.preserved.contours.push(PreservedContour {
            id: contour_id,
            hyper: false,
            metadata: ObjectMetadata {
                identifier: None,
                lib: None,
            },
            points: vec![preserved_point],
        });
        Ok((contour_id, point_id))
    }

    /// Append a line or cubic segment to an open contour.
    ///
    /// When `controls` is present, its two points precede a cubic endpoint.
    /// Returned identities are in the same control-then-endpoint order.
    pub fn append_contour_segment(
        &mut self,
        contour: ContourId,
        controls: Option<[kurbo::Point; 2]>,
        endpoint: kurbo::Point,
        smooth: bool,
    ) -> Result<Vec<PointId>, DocumentEditError> {
        let mut coordinates = vec![endpoint.x, endpoint.y];
        if let Some(controls) = controls {
            coordinates.extend(controls.into_iter().flat_map(|point| [point.x, point.y]));
        }
        ensure_finite(&coordinates)?;
        let shape_index = self
            .contour_shape_index(contour)
            .ok_or(DocumentEditError::MissingContour(contour))?;
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        if path.closed
            || path
                .nodes
                .first()
                .is_none_or(|node| node.nodetype != NodeType::Move)
        {
            return Err(DocumentEditError::NotOpenContour(contour));
        }

        let mut additions = Vec::with_capacity(if controls.is_some() { 3 } else { 1 });
        if let Some(controls) = controls {
            for position in controls {
                additions.push(new_document_point(position, NodeType::OffCurve, false));
            }
        }
        additions.push(new_document_point(
            endpoint,
            if controls.is_some() {
                NodeType::Curve
            } else {
                NodeType::Line
            },
            smooth,
        ));
        let ids = additions.iter().map(|(id, _, _)| *id).collect();
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        path.nodes
            .extend(additions.iter().map(|(_, node, _)| node.clone()));
        self.preserved
            .contours
            .iter_mut()
            .find(|candidate| candidate.id == contour)
            .expect("canonical contour preservation")
            .points
            .extend(additions.into_iter().map(|(_, _, preserved)| preserved));
        Ok(ids)
    }

    /// Close an open contour with an optional cubic segment back to its first point.
    ///
    /// Returns the stable identities of newly inserted controls in contour order.
    pub fn close_contour(
        &mut self,
        contour: ContourId,
        controls: Option<[kurbo::Point; 2]>,
    ) -> Result<Vec<PointId>, DocumentEditError> {
        if let Some(controls) = controls {
            ensure_finite(
                &controls
                    .into_iter()
                    .flat_map(|point| [point.x, point.y])
                    .collect::<Vec<_>>(),
            )?;
        }
        let shape_index = self
            .contour_shape_index(contour)
            .ok_or(DocumentEditError::MissingContour(contour))?;
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        if path.closed
            || path
                .nodes
                .first()
                .is_none_or(|node| node.nodetype != NodeType::Move)
        {
            return Err(DocumentEditError::NotOpenContour(contour));
        }

        let additions: Vec<_> = controls
            .into_iter()
            .flatten()
            .map(|position| new_document_point(position, NodeType::OffCurve, false))
            .collect();
        let ids = additions.iter().map(|(id, _, _)| *id).collect();
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        path.closed = true;
        path.nodes[0].nodetype = if additions.is_empty() {
            NodeType::Line
        } else {
            NodeType::Curve
        };
        path.nodes
            .extend(additions.iter().map(|(_, node, _)| node.clone()));
        self.preserved
            .contours
            .iter_mut()
            .find(|candidate| candidate.id == contour)
            .expect("canonical contour preservation")
            .points
            .extend(additions.into_iter().map(|(_, _, preserved)| preserved));
        Ok(ids)
    }

    /// Start a new open editable hyperbezier contour at `position`.
    ///
    /// The typed hyperbezier kind is authoritative and a fresh UFO identifier is retained as its
    /// compatibility-boundary marker. Returns the stable contour and initial-point identities.
    pub fn start_hyper_contour(
        &mut self,
        position: kurbo::Point,
    ) -> Result<(ContourId, PointId), DocumentEditError> {
        ensure_finite(&[position.x, position.y])?;
        let contour_id = ContourId::next();
        let (point_id, point, preserved_point) =
            new_document_point(position, NodeType::Move, false);
        let mut path = babelfont::Path {
            nodes: vec![point],
            closed: false,
            ..babelfont::Path::default()
        };
        write_id(&mut path.format_specific, contour_id.0);
        self.layer.shapes.push(Shape::Path(path));
        self.preserved.contours.push(PreservedContour {
            id: contour_id,
            hyper: true,
            metadata: ObjectMetadata {
                identifier: Some(fresh_hyper_identifier()),
                lib: None,
            },
            points: vec![preserved_point],
        });
        Ok((contour_id, point_id))
    }

    /// Append one smooth or corner on-curve point to an open hyperbezier contour.
    ///
    /// Returns the stable identity assigned to the new point.
    pub fn append_hyper_point(
        &mut self,
        contour: ContourId,
        position: kurbo::Point,
        corner: bool,
    ) -> Result<PointId, DocumentEditError> {
        ensure_finite(&[position.x, position.y])?;
        let shape_index = self
            .contour_shape_index(contour)
            .ok_or(DocumentEditError::MissingContour(contour))?;
        let preserved_index = self
            .preserved
            .contours
            .iter()
            .position(|candidate| candidate.id == contour)
            .expect("canonical contour preservation");
        if !self.preserved.contours[preserved_index].hyper {
            return Err(DocumentEditError::NotHyperContour(contour));
        }
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        if path.closed
            || path
                .nodes
                .first()
                .is_none_or(|node| node.nodetype != NodeType::Move)
        {
            return Err(DocumentEditError::NotOpenContour(contour));
        }
        let (id, node, preserved) = new_document_point(
            position,
            if corner {
                NodeType::Line
            } else {
                NodeType::Curve
            },
            !corner,
        );
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        path.nodes.push(node);
        self.preserved.contours[preserved_index]
            .points
            .push(preserved);
        Ok(id)
    }

    /// Close an editable hyperbezier contour through its starting point.
    ///
    /// The initial move becomes a smooth hyper point without inserting replacement topology.
    pub fn close_hyper_contour(&mut self, contour: ContourId) -> Result<(), DocumentEditError> {
        let shape_index = self
            .contour_shape_index(contour)
            .ok_or(DocumentEditError::MissingContour(contour))?;
        let preserved = self
            .preserved
            .contours
            .iter()
            .find(|candidate| candidate.id == contour)
            .expect("canonical contour preservation");
        if !preserved.hyper {
            return Err(DocumentEditError::NotHyperContour(contour));
        }
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        if path.closed
            || path
                .nodes
                .first()
                .is_none_or(|node| node.nodetype != NodeType::Move)
        {
            return Err(DocumentEditError::NotOpenContour(contour));
        }
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        path.closed = true;
        path.nodes[0].nodetype = NodeType::Curve;
        path.nodes[0].smooth = true;
        Ok(())
    }

    /// Add a closed rectangle or ellipse contour spanning `rect`.
    ///
    /// Returns the stable contour identity and point identities in contour order.
    pub fn add_shape_contour(
        &mut self,
        rect: kurbo::Rect,
        ellipse: bool,
    ) -> Result<(ContourId, Vec<PointId>), DocumentEditError> {
        ensure_finite(&[rect.x0, rect.y0, rect.x1, rect.y1])?;
        let point = |x, y, point_type, smooth| (kurbo::Point::new(x, y), point_type, smooth);
        let points = if ellipse {
            let center = rect.center();
            let (radius_x, radius_y) = (rect.width() / 2.0, rect.height() / 2.0);
            let (control_x, control_y) = (radius_x * 0.552_284_749_8, radius_y * 0.552_284_749_8);
            let round = |value: f64| value.round();
            vec![
                point(
                    round(center.x + radius_x),
                    round(center.y),
                    NodeType::Curve,
                    true,
                ),
                point(
                    round(center.x + radius_x),
                    round(center.y + control_y),
                    NodeType::OffCurve,
                    false,
                ),
                point(
                    round(center.x + control_x),
                    round(center.y + radius_y),
                    NodeType::OffCurve,
                    false,
                ),
                point(
                    round(center.x),
                    round(center.y + radius_y),
                    NodeType::Curve,
                    true,
                ),
                point(
                    round(center.x - control_x),
                    round(center.y + radius_y),
                    NodeType::OffCurve,
                    false,
                ),
                point(
                    round(center.x - radius_x),
                    round(center.y + control_y),
                    NodeType::OffCurve,
                    false,
                ),
                point(
                    round(center.x - radius_x),
                    round(center.y),
                    NodeType::Curve,
                    true,
                ),
                point(
                    round(center.x - radius_x),
                    round(center.y - control_y),
                    NodeType::OffCurve,
                    false,
                ),
                point(
                    round(center.x - control_x),
                    round(center.y - radius_y),
                    NodeType::OffCurve,
                    false,
                ),
                point(
                    round(center.x),
                    round(center.y - radius_y),
                    NodeType::Curve,
                    true,
                ),
                point(
                    round(center.x + control_x),
                    round(center.y - radius_y),
                    NodeType::OffCurve,
                    false,
                ),
                point(
                    round(center.x + radius_x),
                    round(center.y - control_y),
                    NodeType::OffCurve,
                    false,
                ),
            ]
        } else {
            vec![
                point(rect.x0.round(), rect.y0.round(), NodeType::Line, false),
                point(rect.x1.round(), rect.y0.round(), NodeType::Line, false),
                point(rect.x1.round(), rect.y1.round(), NodeType::Line, false),
                point(rect.x0.round(), rect.y1.round(), NodeType::Line, false),
            ]
        };
        ensure_finite(
            &points
                .iter()
                .flat_map(|(position, _, _)| [position.x, position.y])
                .collect::<Vec<_>>(),
        )?;
        let contour_id = ContourId::next();
        let created: Vec<_> = points
            .into_iter()
            .map(|(position, point_type, smooth)| new_document_point(position, point_type, smooth))
            .collect();
        let point_ids = created.iter().map(|(id, _, _)| *id).collect();
        let mut path = babelfont::Path {
            nodes: created.iter().map(|(_, point, _)| point.clone()).collect(),
            closed: true,
            ..babelfont::Path::default()
        };
        write_id(&mut path.format_specific, contour_id.0);
        self.layer.shapes.push(Shape::Path(path));
        self.preserved.contours.push(PreservedContour {
            id: contour_id,
            hyper: false,
            metadata: ObjectMetadata {
                identifier: None,
                lib: None,
            },
            points: created
                .into_iter()
                .map(|(_, _, preserved)| preserved)
                .collect(),
        });
        Ok((contour_id, point_ids))
    }

    /// Append copied canonical contours with fresh document and UFO identities.
    ///
    /// Point names and object libraries survive the paste. Objects carrying an identifier or lib
    /// receive a fresh UFO identifier so it remains unique within the glyph. Returns the new
    /// contour and point identities.
    pub fn paste_contours(
        &mut self,
        copied: &[CopiedContour],
    ) -> Result<PastedContours, DocumentEditError> {
        ensure_finite(
            &copied
                .iter()
                .flat_map(|contour| contour.path.nodes.iter())
                .flat_map(|node| [node.x, node.y])
                .collect::<Vec<_>>(),
        )?;
        let mut result = PastedContours::default();
        let mut additions = Vec::with_capacity(copied.len());
        for copied in copied {
            debug_assert_eq!(
                copied.path.nodes.len(),
                copied.preserved.points.len(),
                "copied nodes and preservation records stay aligned"
            );
            let contour_id = ContourId::next();
            result.contours.push(contour_id);
            let mut path = copied.path.clone();
            write_id(&mut path.format_specific, contour_id.0);
            let points = path
                .nodes
                .iter_mut()
                .zip(&copied.preserved.points)
                .map(|(node, source)| {
                    let point_id = PointId::next();
                    result.points.push(point_id);
                    write_id(&mut node.format_specific, point_id.0);
                    PreservedPoint {
                        id: point_id,
                        name: source.name.clone(),
                        metadata: ObjectMetadata {
                            identifier: (source.metadata.identifier.is_some()
                                || source.metadata.lib.is_some())
                            .then(norad::Identifier::from_uuidv4),
                            lib: source.metadata.lib.clone(),
                        },
                    }
                })
                .collect();
            additions.push((
                Shape::Path(path),
                PreservedContour {
                    id: contour_id,
                    hyper: copied.preserved.hyper,
                    metadata: ObjectMetadata {
                        identifier: if copied.preserved.hyper {
                            Some(fresh_hyper_identifier())
                        } else {
                            (copied.preserved.metadata.identifier.is_some()
                                || copied.preserved.metadata.lib.is_some())
                            .then(norad::Identifier::from_uuidv4)
                        },
                        lib: copied.preserved.metadata.lib.clone(),
                    },
                    points,
                },
            ));
        }
        self.layer
            .shapes
            .extend(additions.iter().map(|(shape, _)| shape.clone()));
        self.preserved
            .contours
            .extend(additions.into_iter().map(|(_, preserved)| preserved));
        Ok(result)
    }

    /// Append contours decoded at an explicit source-format boundary.
    ///
    /// Source names, identifiers and object libraries are retained exactly. The complete payload
    /// is validated before the draft changes.
    pub fn append_imported_contours(
        &mut self,
        imported: ImportedContours,
    ) -> Result<PastedContours, DocumentEditError> {
        self.validate_imported_contours(&imported, true)?;
        let (shapes, preserved, inserted) = imported.into_fresh_parts();
        self.layer.shapes.extend(shapes);
        self.preserved.contours.extend(preserved);
        Ok(inserted)
    }

    /// Replace only the contours from an explicit UFO boundary.
    ///
    /// Components, anchors, advances and all layer metadata remain unchanged.
    /// An exact contour no-op retains the existing stable identities.
    pub fn replace_imported_contours(
        &mut self,
        imported: ImportedContours,
    ) -> Result<bool, DocumentEditError> {
        if imported.matches_layer(&self.layer, &self.preserved) {
            return Ok(false);
        }
        self.validate_imported_contours(&imported, false)?;
        let (shapes, preserved, _) = imported.into_fresh_parts();
        replace_path_shapes_preserving_slots(&mut self.layer.shapes, shapes);
        self.preserved.contours = preserved;
        Ok(true)
    }

    pub(super) fn validate_imported_contours(
        &self,
        imported: &ImportedContours,
        append: bool,
    ) -> Result<(), DocumentEditError> {
        let mut identifiers = HashSet::new();
        for guideline in &self.preserved.guidelines {
            if guideline
                .identifier()
                .is_some_and(|identifier| !identifiers.insert(identifier.as_ref().to_owned()))
            {
                return Err(DocumentEditError::InvalidLayerMetadata);
            }
        }
        let mut insert = |metadata: &ObjectMetadata| {
            metadata
                .identifier
                .as_ref()
                .is_none_or(|identifier| identifiers.insert(identifier.as_ref().to_owned()))
        };
        if append {
            for contour in &self.preserved.contours {
                if !insert(&contour.metadata)
                    || contour.points.iter().any(|point| !insert(&point.metadata))
                {
                    return Err(DocumentEditError::InvalidLayerMetadata);
                }
            }
        }
        for component in &self.preserved.components {
            if !insert(&component.metadata) {
                return Err(DocumentEditError::InvalidLayerMetadata);
            }
        }
        for anchor in &self.preserved.anchors {
            if !insert(&anchor.metadata) {
                return Err(DocumentEditError::InvalidLayerMetadata);
            }
        }
        for contour in &imported.contours {
            if !insert(&contour.preserved.metadata)
                || contour
                    .preserved
                    .points
                    .iter()
                    .any(|point| !insert(&point.metadata))
            {
                return Err(DocumentEditError::InvalidLayerMetadata);
            }
        }
        Ok(())
    }

    /// Duplicate every contour containing a selected point by `offset`.
    ///
    /// An empty selection is a no-op. The duplicate receives the same metadata treatment as a
    /// paste and fresh stable identities. Returns those new identities.
    pub fn duplicate_contours(
        &mut self,
        selected: &[PointId],
        offset: kurbo::Vec2,
    ) -> Result<PastedContours, DocumentEditError> {
        if selected.is_empty() {
            return Ok(PastedContours::default());
        }
        ensure_finite(&[offset.x, offset.y])?;
        let mut copied = self.view().copy_contours(selected)?;
        let coordinates: Vec<_> = copied
            .iter()
            .flat_map(|contour| &contour.path.nodes)
            .flat_map(|node| [node.x + offset.x, node.y + offset.y])
            .collect();
        ensure_finite(&coordinates)?;
        for contour in &mut copied {
            for node in &mut contour.path.nodes {
                node.x += offset.x;
                node.y += offset.y;
            }
        }
        self.paste_contours(&copied)
    }
}

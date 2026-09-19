// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Babelfont geometry with lossless UFO persistence projections.
//!
//! Babelfont owns paths, anchors and components. UFO payloads retain metadata its
//! model cannot express, plus exact advances and affine coefficients. A projection
//! takes geometry from Babelfont, restoring exact numbers when their corresponding
//! Babelfont value is unchanged. Compilation is the only quantizing boundary.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use babelfont::{Anchor, Component, Layer, Node, NodeType, Shape};
use kurbo::ParamCurve;

use super::variable::LayerId;

pub(super) fn layer_key(id: &LayerId) -> String {
    format!("{}:{}", id.source.0, id.name)
}

const OBJECT_ID_KEY: &str = "com.runebender.documentObjectId";
static NEXT_OBJECT_ID: AtomicU64 = AtomicU64::new(1);

macro_rules! object_id {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u64);

        impl $name {
            fn next() -> Self {
                Self(NEXT_OBJECT_ID.fetch_add(1, Ordering::Relaxed))
            }
        }
    };
}

object_id!(
    ContourId,
    "Stable identity of a contour in an open document."
);
object_id!(PointId, "Stable identity of a point in an open document.");
object_id!(
    ComponentId,
    "Stable identity of a component in an open document."
);
object_id!(
    AnchorId,
    "Stable identity of an anchor in an open document."
);

#[derive(Clone, Debug, PartialEq)]
struct PreservedContour {
    id: ContourId,
    metadata: ObjectMetadata,
    points: Vec<PreservedPoint>,
}

#[derive(Clone, Debug, PartialEq)]
struct PreservedPoint {
    id: PointId,
    name: Option<norad::Name>,
    metadata: ObjectMetadata,
}

#[derive(Clone, Debug, PartialEq)]
struct PreservedComponent {
    id: ComponentId,
    transform: norad::AffineTransform,
    metadata: ObjectMetadata,
}

#[derive(Clone, Debug, PartialEq)]
struct PreservedAnchor {
    id: AnchorId,
    color: Option<norad::Color>,
    metadata: ObjectMetadata,
}

#[derive(Clone, Debug, PartialEq)]
struct ObjectMetadata {
    identifier: Option<norad::Identifier>,
    lib: Option<plist::Dictionary>,
}

impl ObjectMetadata {
    fn new(identifier: Option<&norad::Identifier>, lib: Option<&plist::Dictionary>) -> Self {
        Self {
            identifier: identifier.cloned(),
            lib: lib.cloned(),
        }
    }
}

/// Exact UFO values and object metadata that Babelfont cannot represent faithfully.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct LayerPreservation {
    name: String,
    width: f64,
    height: f64,
    codepoints: norad::Codepoints,
    note: Option<String>,
    guidelines: Vec<norad::Guideline>,
    image: Option<norad::Image>,
    lib: plist::Dictionary,
    contours: Vec<PreservedContour>,
    components: Vec<PreservedComponent>,
    anchors: Vec<PreservedAnchor>,
}

/// A point kind in a canonical document layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerPointType {
    /// Start an open contour without drawing a segment.
    Move,
    /// End a straight segment.
    Line,
    /// A control point outside the curve.
    OffCurve,
    /// End a cubic Bézier segment.
    Curve,
    /// End a quadratic Bézier segment.
    QCurve,
}

/// A canonical segment endpoint backed by a stored point or an implied quadratic join.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentSegmentEndpoint {
    /// An explicit on-curve point.
    Point(PointId),
    /// The midpoint between two consecutive quadratic controls.
    Implied {
        /// The first source control.
        first_control: PointId,
        /// The second source control.
        second_control: PointId,
    },
}

impl DocumentSegmentEndpoint {
    pub(crate) fn append_source_ids(self, output: &mut Vec<PointId>) {
        let mut push = |id| {
            if !output.contains(&id) {
                output.push(id);
            }
        };
        match self {
            Self::Point(id) => push(id),
            Self::Implied {
                first_control,
                second_control,
            } => {
                push(first_control);
                push(second_control);
            }
        }
    }
}

/// Stable identities created while inserting a point on an implied quadratic segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuadraticSegmentInsertion {
    /// The inserted on-curve point selected by the editing operation.
    pub point: PointId,
    /// A stored point created to retain an implied start position, when required.
    pub explicitized_start: Option<PointId>,
    /// A stored point created to retain an implied end position, when required.
    pub explicitized_end: Option<PointId>,
}

/// One owned canonical contour carried by copy and paste operations.
#[derive(Clone, Debug)]
pub struct CopiedContour {
    path: babelfont::Path,
    preserved: PreservedContour,
}

/// Stable identities created while pasting or duplicating canonical contours.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PastedContours {
    /// Identities of the newly created contours in insertion order.
    pub contours: Vec<ContourId>,
    /// Identities of every newly created point in contour order.
    pub points: Vec<PointId>,
}

/// Read-only access to one canonical glyph layer.
#[derive(Clone, Copy, Debug)]
pub struct LayerView<'a> {
    layer: &'a Layer,
    preserved: &'a LayerPreservation,
}

/// One contour or component in a canonical layer's paint order.
#[derive(Clone, Copy, Debug)]
pub enum LayerShapeView<'a> {
    /// An ordinary or special outline contour.
    Contour(ContourView<'a>),
    /// A reference to another glyph layer.
    Component(ComponentView<'a>),
}

impl<'a> LayerView<'a> {
    pub(super) fn new(layer: &'a Layer, preserved: &'a LayerPreservation) -> Self {
        Self { layer, preserved }
    }

    /// The exact horizontal advance from the document extension.
    pub fn width(self) -> f64 {
        self.preserved.width
    }

    /// The exact vertical advance from the document extension.
    pub fn height(self) -> f64 {
        self.preserved.height
    }

    /// The source glyph name attached to this layer.
    pub fn glyph_name(self) -> &'a str {
        &self.preserved.name
    }

    /// The optional source note attached to this layer.
    pub fn note(self) -> Option<&'a str> {
        self.preserved.note.as_deref()
    }

    /// Unicode scalar values attached to this glyph layer.
    pub fn codepoints(self) -> impl Iterator<Item = char> + 'a {
        self.preserved.codepoints.iter()
    }

    /// Canonical contours in storage order.
    pub fn contours(self) -> impl DoubleEndedIterator<Item = ContourView<'a>> + 'a {
        self.layer.paths().map(move |path| {
            let id = ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
            let preserved = self
                .preserved
                .contours
                .iter()
                .find(|candidate| candidate.id == id)
                .expect("contour preservation identity");
            ContourView { path, preserved }
        })
    }

    /// Canonical components in storage order.
    pub fn components(self) -> impl DoubleEndedIterator<Item = ComponentView<'a>> + 'a {
        self.layer.components().map(move |component| {
            let id = ComponentId(
                read_id(&component.format_specific).expect("canonical component identity"),
            );
            let preserved = self
                .preserved
                .components
                .iter()
                .find(|candidate| candidate.id == id)
                .expect("component preservation identity");
            ComponentView {
                component,
                preserved,
            }
        })
    }

    /// Canonical anchors in storage order.
    pub fn anchors(self) -> impl DoubleEndedIterator<Item = AnchorView<'a>> + 'a {
        self.layer.anchors.iter().map(move |anchor| {
            let id = AnchorId(read_id(&anchor.format_specific).expect("canonical anchor identity"));
            let preserved = self
                .preserved
                .anchors
                .iter()
                .find(|candidate| candidate.id == id)
                .expect("anchor preservation identity");
            AnchorView { anchor, preserved }
        })
    }

    /// Canonical contours and components in their stored paint order.
    pub fn shapes(self) -> impl DoubleEndedIterator<Item = LayerShapeView<'a>> + 'a {
        self.layer.shapes.iter().map(move |shape| match shape {
            Shape::Path(path) => {
                let id =
                    ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
                let preserved = self
                    .preserved
                    .contours
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .expect("contour preservation identity");
                LayerShapeView::Contour(ContourView { path, preserved })
            }
            Shape::Component(component) => {
                let id = ComponentId(
                    read_id(&component.format_specific).expect("canonical component identity"),
                );
                let preserved = self
                    .preserved
                    .components
                    .iter()
                    .find(|candidate| candidate.id == id)
                    .expect("component preservation identity");
                LayerShapeView::Component(ComponentView {
                    component,
                    preserved,
                })
            }
        })
    }

    /// Copy every contour containing a selected point, or every contour for an empty selection.
    pub fn copy_contours(
        self,
        selected: &[PointId],
    ) -> Result<Vec<CopiedContour>, DocumentEditError> {
        for id in selected {
            if !self.layer.paths().flat_map(|path| &path.nodes).any(|node| {
                read_id(&node.format_specific).is_some_and(|candidate| candidate == id.0)
            }) {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().map(|id| id.0).collect();
        let copy_all = selected.is_empty();
        Ok(self
            .layer
            .paths()
            .filter(|path| {
                copy_all
                    || path.nodes.iter().any(|node| {
                        read_id(&node.format_specific).is_some_and(|id| selected.contains(&id))
                    })
            })
            .map(|path| {
                let contour_id =
                    ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
                let preserved = self
                    .preserved
                    .contours
                    .iter()
                    .find(|candidate| candidate.id == contour_id)
                    .expect("canonical contour preservation");
                CopiedContour {
                    path: path.clone(),
                    preserved: preserved.clone(),
                }
            })
            .collect())
    }

    /// Capture every canonical point position needed for a persistent drag.
    ///
    /// The returned origins include adjacent handles carried by selected on-curve points and
    /// opposite handles that preserve a smooth tangent.
    pub fn point_drag_origins(
        self,
        selected: &[PointId],
        independent: bool,
    ) -> Result<Vec<(PointId, kurbo::Point)>, DocumentEditError> {
        for id in selected {
            if !self.layer.paths().flat_map(|path| &path.nodes).any(|node| {
                read_id(&node.format_specific).is_some_and(|candidate| candidate == id.0)
            }) {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().copied().collect();
        let mut origins = Vec::new();
        for path in self.layer.paths() {
            let ids: Vec<_> = path
                .nodes
                .iter()
                .map(|node| {
                    PointId(read_id(&node.format_specific).expect("canonical point identity"))
                })
                .collect();
            let selected_indices: HashSet<_> = ids
                .iter()
                .enumerate()
                .filter_map(|(index, id)| selected.contains(id).then_some(index))
                .collect();
            if selected_indices.is_empty() {
                continue;
            }
            let states: Vec<_> = path
                .nodes
                .iter()
                .map(|node| crate::outline::point_ops::PointState {
                    position: kurbo::Point::new(node.x, node.y),
                    off_curve: node.nodetype == NodeType::OffCurve,
                    smooth: node.smooth,
                })
                .collect();
            for index in crate::outline::point_ops::affected_indices(
                &states,
                &selected_indices,
                path.closed,
                independent,
            ) {
                origins.push((ids[index], states[index].position));
            }
        }
        Ok(origins)
    }
}

/// Read-only access to one canonical contour.
#[derive(Clone, Copy, Debug)]
pub struct ContourView<'a> {
    path: &'a babelfont::Path,
    preserved: &'a PreservedContour,
}

impl<'a> ContourView<'a> {
    /// Stable identity retained across ordinary edits and reorder.
    pub fn id(self) -> ContourId {
        self.preserved.id
    }

    /// Whether the contour connects its last point to its first point.
    pub fn is_closed(self) -> bool {
        self.path.closed
    }

    /// Canonical points in contour order.
    pub fn points(self) -> impl DoubleEndedIterator<Item = PointView<'a>> + 'a {
        self.path.nodes.iter().map(move |node| {
            let id = PointId(read_id(&node.format_specific).expect("canonical point identity"));
            let preserved = self
                .preserved
                .points
                .iter()
                .find(|candidate| candidate.id == id)
                .expect("point preservation identity");
            PointView { node, preserved }
        })
    }
}

/// Read-only access to one canonical contour point.
#[derive(Clone, Copy, Debug)]
pub struct PointView<'a> {
    node: &'a Node,
    preserved: &'a PreservedPoint,
}

impl<'a> PointView<'a> {
    /// Stable identity retained across ordinary edits and reorder.
    pub fn id(self) -> PointId {
        self.preserved.id
    }

    /// Position in font design coordinates.
    pub fn position(self) -> kurbo::Point {
        kurbo::Point::new(self.node.x, self.node.y)
    }

    /// Segment role of this point.
    pub fn point_type(self) -> LayerPointType {
        match self.node.nodetype {
            NodeType::Move => LayerPointType::Move,
            NodeType::Line => LayerPointType::Line,
            NodeType::OffCurve => LayerPointType::OffCurve,
            NodeType::Curve => LayerPointType::Curve,
            NodeType::QCurve => LayerPointType::QCurve,
        }
    }

    /// Whether the point has smooth tangent continuity.
    pub fn is_smooth(self) -> bool {
        self.node.smooth
    }

    /// Optional source point name retained by the typed extension.
    pub fn name(self) -> Option<&'a str> {
        self.preserved.name.as_ref().map(norad::Name::as_str)
    }
}

/// Read-only access to one canonical component.
#[derive(Clone, Copy, Debug)]
pub struct ComponentView<'a> {
    component: &'a Component,
    preserved: &'a PreservedComponent,
}

impl<'a> ComponentView<'a> {
    /// Stable identity retained across ordinary edits and reorder.
    pub fn id(self) -> ComponentId {
        self.preserved.id
    }

    /// Name of the referenced glyph.
    pub fn reference(self) -> &'a str {
        self.component.reference.as_str()
    }

    /// Exact six-coefficient source transform.
    pub fn transform(self) -> kurbo::Affine {
        affine(self.preserved.transform)
    }
}

/// Read-only access to one canonical anchor.
#[derive(Clone, Copy, Debug)]
pub struct AnchorView<'a> {
    anchor: &'a Anchor,
    preserved: &'a PreservedAnchor,
}

/// An atomic, owned edit draft for one canonical glyph layer.
#[derive(Clone, Debug)]
pub struct LayerEditDraft {
    layer: Layer,
    preserved: LayerPreservation,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct LayerDelta {
    pub(super) geometry: bool,
    pub(super) metrics: bool,
    pub(super) metadata: bool,
}

impl LayerDelta {
    pub(super) fn is_empty(self) -> bool {
        !(self.geometry || self.metrics || self.metadata)
    }
}

impl LayerEditDraft {
    pub(super) fn new(layer: Layer, preserved: LayerPreservation) -> Self {
        Self { layer, preserved }
    }

    pub(super) fn into_parts(self) -> (Layer, LayerPreservation) {
        (self.layer, self.preserved)
    }

    pub(super) fn delta_from(&self, layer: &Layer, preserved: &LayerPreservation) -> LayerDelta {
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
            || self.preserved.contours != preserved.contours
            || self
                .preserved
                .components
                .iter()
                .map(|item| (item.id, &item.metadata))
                .ne(preserved
                    .components
                    .iter()
                    .map(|item| (item.id, &item.metadata)))
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
                    metadata: ObjectMetadata {
                        identifier: (copied.preserved.metadata.identifier.is_some()
                            || copied.preserved.metadata.lib.is_some())
                        .then(norad::Identifier::from_uuidv4),
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

    /// Apply a boolean operation to canonical contours and replace their topology.
    ///
    /// Union combines every contour. Other operations use the first contour as the left operand
    /// and the remaining contours as the right operand. Replacement contours receive fresh stable
    /// identities and empty source metadata. Returns whether replacement succeeded.
    pub fn boolean_contours(
        &mut self,
        operation: linesweeper::BinaryOp,
    ) -> Result<bool, DocumentEditError> {
        let paths: Vec<_> = self
            .view()
            .contours()
            .map(crate::outline::glyph_paths::ordinary_contour_to_bezpath)
            .collect();
        if paths.len() < 2 {
            return Ok(false);
        }
        let (left, right) = if operation == linesweeper::BinaryOp::Union {
            let mut combined = kurbo::BezPath::new();
            for path in &paths {
                combined.extend(path.elements().iter().copied());
            }
            (combined, kurbo::BezPath::new())
        } else {
            let mut paths = paths.into_iter();
            let left = paths
                .next()
                .expect("boolean input has at least two contours");
            let mut right = kurbo::BezPath::new();
            for path in paths {
                right.extend(path.elements().iter().copied());
            }
            (left, right)
        };
        let Ok(result) =
            linesweeper::binary_op(&left, &right, linesweeper::FillRule::NonZero, operation)
        else {
            return Ok(false);
        };
        let paths: Vec<_> = result
            .contours()
            .map(|contour| contour.path.clone())
            .collect();
        self.replace_contours_with_paths(&paths)
    }

    /// Union every canonical contour and replace their topology.
    ///
    /// Replacement contours receive fresh stable identities and empty source metadata. Returns
    /// whether overlap removal succeeded.
    pub fn remove_overlap(&mut self) -> Result<bool, DocumentEditError> {
        let combined = crate::outline::glyph_paths::ordinary_layer_contours_to_bezpath(self.view());
        if combined.is_empty() {
            return Ok(false);
        }
        let Ok(result) = linesweeper::binary_op(
            &combined,
            &kurbo::BezPath::new(),
            linesweeper::FillRule::NonZero,
            linesweeper::BinaryOp::Union,
        ) else {
            return Ok(false);
        };
        let paths: Vec<_> = result
            .contours()
            .map(|contour| contour.path.clone())
            .collect();
        self.replace_contours_with_paths(&paths)
    }

    fn replace_contours_with_paths(
        &mut self,
        paths: &[kurbo::BezPath],
    ) -> Result<bool, DocumentEditError> {
        if paths.is_empty() {
            return Ok(false);
        }
        let smooth_at: HashMap<_, _> = self
            .layer
            .paths()
            .flat_map(|path| &path.nodes)
            .filter(|node| node.nodetype != NodeType::OffCurve)
            .map(|node| {
                (
                    crate::outline::glyph_paths::point_key(node.x, node.y),
                    node.smooth,
                )
            })
            .collect();
        let mut replacements = Vec::with_capacity(paths.len());
        let mut preserved = Vec::with_capacity(paths.len());
        for path in paths {
            let mut path = babelfont::Path::from(path.clone());
            ensure_finite(
                &path
                    .nodes
                    .iter()
                    .flat_map(|node| [node.x, node.y])
                    .collect::<Vec<_>>(),
            )?;
            if path
                .nodes
                .iter()
                .filter(|node| node.nodetype != NodeType::OffCurve)
                .count()
                < 2
            {
                continue;
            }
            let contour_id = ContourId::next();
            write_id(&mut path.format_specific, contour_id.0);
            let points = path
                .nodes
                .iter_mut()
                .map(|node| {
                    if node.nodetype != NodeType::OffCurve {
                        node.smooth = smooth_at
                            .get(&crate::outline::glyph_paths::point_key(node.x, node.y))
                            .copied()
                            .unwrap_or(false);
                    }
                    let point_id = PointId::next();
                    write_id(&mut node.format_specific, point_id.0);
                    PreservedPoint {
                        id: point_id,
                        name: None,
                        metadata: ObjectMetadata {
                            identifier: None,
                            lib: None,
                        },
                    }
                })
                .collect();
            replacements.push(Shape::Path(path));
            preserved.push(PreservedContour {
                id: contour_id,
                metadata: ObjectMetadata {
                    identifier: None,
                    lib: None,
                },
                points,
            });
        }
        if replacements.is_empty() {
            return Ok(false);
        }
        let insert_at = self
            .layer
            .shapes
            .iter()
            .take_while(|shape| !matches!(shape, Shape::Path(_)))
            .filter(|shape| matches!(shape, Shape::Component(_)))
            .count();
        let mut shapes: Vec<_> = self
            .layer
            .shapes
            .iter()
            .filter(|shape| matches!(shape, Shape::Component(_)))
            .cloned()
            .collect();
        shapes.splice(insert_at..insert_at, replacements);
        self.layer.shapes = shapes;
        self.preserved.contours = preserved;
        Ok(true)
    }

    /// Set the exact horizontal advance and refresh Babelfont's derived width.
    ///
    /// Returns whether the value changed.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the exact f64 value remains authoritative in the document extension"
    )]
    pub fn set_width(&mut self, width: f64) -> Result<bool, DocumentEditError> {
        ensure_finite(&[width])?;
        if self.preserved.width == width {
            return Ok(false);
        }
        self.preserved.width = width;
        self.layer.width = width as f32;
        Ok(true)
    }

    /// Set the exact vertical advance.
    ///
    /// Returns whether the value changed.
    pub fn set_height(&mut self, height: f64) -> Result<bool, DocumentEditError> {
        ensure_finite(&[height])?;
        if self.preserved.height == height {
            return Ok(false);
        }
        self.preserved.height = height;
        Ok(true)
    }

    /// Set one point's position by stable identity.
    ///
    /// Returns whether the value changed.
    pub fn set_point_position(
        &mut self,
        id: PointId,
        position: kurbo::Point,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[position.x, position.y])?;
        let node = self
            .node_mut(id)
            .ok_or(DocumentEditError::MissingPoint(id))?;
        if node.x == position.x && node.y == position.y {
            return Ok(false);
        }
        node.x = position.x;
        node.y = position.y;
        Ok(true)
    }

    /// Move selected points with the editor's snapping and smooth-handle rules.
    ///
    /// `originals` supplies every position returned by [`LayerView::point_drag_origins`], allowing
    /// repeated pointer events to apply their total delta without accumulating intermediate
    /// snapping. An empty origins slice performs a one-step nudge. An empty selection is unchanged.
    /// Returns whether any point moved.
    pub fn translate_points(
        &mut self,
        selected: &[PointId],
        originals: &[(PointId, kurbo::Point)],
        delta: kurbo::Vec2,
        independent: bool,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[delta.x, delta.y])?;
        if selected.is_empty() {
            return Ok(false);
        }
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        for (id, position) in originals {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
            ensure_finite(&[position.x, position.y])?;
        }

        let selected: HashSet<_> = selected.iter().copied().collect();
        let originals: HashMap<_, _> = originals.iter().copied().collect();
        let mut replacements = HashMap::new();
        for path in self.layer.paths() {
            let ids: Vec<_> = path
                .nodes
                .iter()
                .map(|node| {
                    PointId(read_id(&node.format_specific).expect("canonical point identity"))
                })
                .collect();
            let selected_indices: HashSet<_> = ids
                .iter()
                .enumerate()
                .filter_map(|(index, id)| selected.contains(id).then_some(index))
                .collect();
            if selected_indices.is_empty() {
                continue;
            }
            let states: Vec<_> = path
                .nodes
                .iter()
                .map(|node| crate::outline::point_ops::PointState {
                    position: kurbo::Point::new(node.x, node.y),
                    off_curve: node.nodetype == NodeType::OffCurve,
                    smooth: node.smooth,
                })
                .collect();
            let path_originals: HashMap<usize, kurbo::Point> = ids
                .iter()
                .enumerate()
                .filter_map(|(index, id)| originals.get(id).copied().map(|point| (index, point)))
                .collect();
            if !originals.is_empty() {
                for index in crate::outline::point_ops::affected_indices(
                    &states,
                    &selected_indices,
                    path.closed,
                    independent,
                ) {
                    if !path_originals.contains_key(&index) {
                        return Err(DocumentEditError::MissingDragOrigin(ids[index]));
                    }
                }
            }
            for (index, position) in crate::outline::point_ops::translated_positions(
                &states,
                &selected_indices,
                &path_originals,
                (delta.x, delta.y),
                path.closed,
                independent,
            ) {
                ensure_finite(&[position.x, position.y])?;
                replacements.insert(ids[index].0, position);
            }
        }
        if replacements.is_empty() {
            return Ok(false);
        }
        for node in self
            .layer
            .shapes
            .iter_mut()
            .filter_map(|shape| match shape {
                Shape::Path(path) => Some(path),
                Shape::Component(_) => None,
            })
            .flat_map(|path| &mut path.nodes)
        {
            let id = read_id(&node.format_specific).expect("canonical point identity");
            if let Some(position) = replacements.get(&id) {
                node.x = position.x;
                node.y = position.y;
            }
        }
        Ok(true)
    }

    /// Transform selected points about the center of their bounding box.
    ///
    /// An empty selection transforms every point.
    /// Returns whether any point moved.
    pub fn transform_points(
        &mut self,
        selected: &[PointId],
        transform: kurbo::Affine,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&transform.as_coeffs())?;
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let targeted = |node: &Node| {
            selected.is_empty()
                || read_id(&node.format_specific)
                    .is_some_and(|id| selected.iter().any(|selected| selected.0 == id))
        };
        let mut min = kurbo::Point::new(f64::INFINITY, f64::INFINITY);
        let mut max = kurbo::Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for node in self.layer.paths().flat_map(|path| &path.nodes) {
            if targeted(node) {
                min.x = min.x.min(node.x);
                min.y = min.y.min(node.y);
                max.x = max.x.max(node.x);
                max.y = max.y.max(node.y);
            }
        }
        if !min.x.is_finite() {
            return Ok(false);
        }
        let center = (min.x * 0.5 + max.x * 0.5, min.y * 0.5 + max.y * 0.5);
        ensure_finite(&[center.0, center.1])?;
        let transform = kurbo::Affine::translate(center)
            * transform
            * kurbo::Affine::translate((-center.0, -center.1));
        ensure_finite(&transform.as_coeffs())?;
        let mut replacements = HashMap::new();
        for node in self.layer.paths().flat_map(|path| &path.nodes) {
            if !targeted(node) {
                continue;
            }
            let position = transform * kurbo::Point::new(node.x, node.y);
            ensure_finite(&[position.x, position.y])?;
            if node.x != position.x || node.y != position.y {
                replacements.insert(
                    read_id(&node.format_specific).expect("canonical point identity"),
                    position,
                );
            }
        }
        if replacements.is_empty() {
            return Ok(false);
        }
        for node in self
            .layer
            .shapes
            .iter_mut()
            .filter_map(|shape| match shape {
                Shape::Path(path) => Some(path),
                Shape::Component(_) => None,
            })
            .flat_map(|path| &mut path.nodes)
        {
            let id = read_id(&node.format_specific).expect("canonical point identity");
            if let Some(position) = replacements.get(&id) {
                node.x = position.x;
                node.y = position.y;
            }
        }
        Ok(true)
    }

    /// Set one point's segment role by stable identity.
    ///
    /// Returns whether the value changed.
    pub fn set_point_type(
        &mut self,
        id: PointId,
        point_type: LayerPointType,
    ) -> Result<bool, DocumentEditError> {
        let (path, index) = self
            .path_and_node_index_mut(id)
            .ok_or(DocumentEditError::MissingPoint(id))?;
        if point_type == LayerPointType::Move && index != 0 {
            return Err(DocumentEditError::NonInitialMove(id));
        }
        let node_type = match point_type {
            LayerPointType::Move => NodeType::Move,
            LayerPointType::Line => NodeType::Line,
            LayerPointType::OffCurve => NodeType::OffCurve,
            LayerPointType::Curve => NodeType::Curve,
            LayerPointType::QCurve => NodeType::QCurve,
        };
        let closed = point_type != LayerPointType::Move;
        let changed =
            path.nodes[index].nodetype != node_type || (index == 0 && path.closed != closed);
        if !changed {
            return Ok(false);
        }
        path.nodes[index].nodetype = node_type;
        if index == 0 {
            path.closed = closed;
        }
        Ok(true)
    }

    /// Set one point's smooth state by stable identity.
    ///
    /// Returns whether the value changed.
    pub fn set_point_smooth(
        &mut self,
        id: PointId,
        smooth: bool,
    ) -> Result<bool, DocumentEditError> {
        let node = self
            .node_mut(id)
            .ok_or(DocumentEditError::MissingPoint(id))?;
        if node.smooth == smooth {
            return Ok(false);
        }
        node.smooth = smooth;
        Ok(true)
    }

    /// Toggle smooth/corner state on selected on-curve points.
    ///
    /// Selected off-curve points are left unchanged.
    /// Returns whether any point changed.
    pub fn toggle_smooth_points(
        &mut self,
        selected: &[PointId],
    ) -> Result<bool, DocumentEditError> {
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().map(|id| id.0).collect();
        let mut changed = false;
        for node in self
            .layer
            .shapes
            .iter_mut()
            .filter_map(|shape| match shape {
                Shape::Path(path) => Some(path),
                Shape::Component(_) => None,
            })
            .flat_map(|path| &mut path.nodes)
        {
            let id = read_id(&node.format_specific).expect("canonical point identity");
            if selected.contains(&id) && node.nodetype != NodeType::OffCurve {
                node.smooth = !node.smooth;
                changed = true;
            }
        }
        Ok(changed)
    }

    /// Delete selected points while preserving surviving canonical identities and metadata.
    ///
    /// Deleting an on-curve point also removes its incoming controls. Deleting a cubic control
    /// removes both controls from that segment. Deleting a quadratic control materializes its
    /// implied endpoints and replaces only its segment with a line. Contours without a surviving
    /// segment are removed. Returns whether any topology changed.
    pub fn delete_points(&mut self, selected: &[PointId]) -> Result<bool, DocumentEditError> {
        let mut staged = self.clone();
        let changed = staged.delete_points_in_place(selected)?;
        if changed {
            *self = staged;
        }
        Ok(changed)
    }

    fn delete_points_in_place(&mut self, selected: &[PointId]) -> Result<bool, DocumentEditError> {
        if selected.is_empty() {
            return Ok(false);
        }
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().map(|id| id.0).collect();
        let mut changed = false;
        let mut shape_index = 0_usize;
        while shape_index < self.layer.shapes.len() {
            let Shape::Path(path) = &self.layer.shapes[shape_index] else {
                shape_index += 1;
                continue;
            };
            let contour_id =
                ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
            let point_ids: Vec<_> = path
                .nodes
                .iter()
                .map(|node| read_id(&node.format_specific).expect("canonical point identity"))
                .collect();
            if !point_ids.iter().any(|id| selected.contains(id)) {
                shape_index += 1;
                continue;
            }
            if point_ids.iter().all(|id| selected.contains(id)) {
                changed = true;
                self.layer.shapes.remove(shape_index);
                self.preserved
                    .contours
                    .retain(|candidate| candidate.id != contour_id);
                continue;
            }
            {
                let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
                    unreachable!("selected contour is a path");
                };
                let preserved = self
                    .preserved
                    .contours
                    .iter_mut()
                    .find(|candidate| candidate.id == contour_id)
                    .expect("canonical contour preservation");
                changed |= materialize_deleted_quadratic_controls(path, preserved, &selected)?;
            }
            let Shape::Path(path) = &self.layer.shapes[shape_index] else {
                unreachable!("selected contour is a path");
            };
            let point_ids: Vec<_> = path
                .nodes
                .iter()
                .map(|node| read_id(&node.format_specific).expect("canonical point identity"))
                .collect();
            if !point_ids.iter().any(|id| selected.contains(id)) {
                shape_index += 1;
                continue;
            }
            changed = true;
            let closed = path.closed;
            let on_indices: Vec<_> = path
                .nodes
                .iter()
                .enumerate()
                .filter_map(|(index, node)| (node.nodetype != NodeType::OffCurve).then_some(index))
                .collect();
            if on_indices.is_empty() {
                self.layer.shapes.remove(shape_index);
                self.preserved
                    .contours
                    .retain(|candidate| candidate.id != contour_id);
                continue;
            }
            struct SegmentRecord {
                on_index: usize,
                controls: Vec<usize>,
            }
            let controls_between = |start: usize, end: usize| {
                let mut controls = Vec::new();
                let mut index = start + 1;
                if index == path.nodes.len() {
                    index = 0;
                }
                while index != end {
                    controls.push(index);
                    index += 1;
                    if index == path.nodes.len() {
                        index = 0;
                    }
                }
                controls
            };
            let mut records = Vec::with_capacity(on_indices.len());
            for (position, on_index) in on_indices.iter().copied().enumerate() {
                let controls = if !closed && position == 0 {
                    Vec::new()
                } else {
                    let previous = if position == 0 {
                        *on_indices.last().expect("on-curve point exists")
                    } else {
                        on_indices[position - 1]
                    };
                    controls_between(previous, on_index)
                };
                records.push(SegmentRecord { on_index, controls });
            }
            records.retain(|record| !selected.contains(&point_ids[record.on_index]));
            for record in &mut records {
                if record
                    .controls
                    .iter()
                    .any(|index| selected.contains(&point_ids[*index]))
                {
                    record.controls.clear();
                }
            }
            if records.is_empty() {
                self.layer.shapes.remove(shape_index);
                self.preserved
                    .contours
                    .retain(|candidate| candidate.id != contour_id);
                continue;
            }
            if !closed {
                records[0].controls.clear();
            }
            let preserved = self
                .preserved
                .contours
                .iter_mut()
                .find(|candidate| candidate.id == contour_id)
                .expect("canonical contour preservation");
            let old_nodes = path.nodes.clone();
            let old_points = preserved.points.clone();
            let mut nodes = Vec::new();
            let mut points = Vec::new();
            let append = |index: usize, nodes: &mut Vec<Node>, points: &mut Vec<PreservedPoint>| {
                nodes.push(old_nodes[index].clone());
                points.push(old_points[index].clone());
            };
            for (position, record) in records.iter().enumerate() {
                if !(closed && position == 0) {
                    for control in &record.controls {
                        append(*control, &mut nodes, &mut points);
                    }
                }
                append(record.on_index, &mut nodes, &mut points);
                let endpoint = nodes.last_mut().expect("on-curve point was appended");
                if !closed && position == 0 {
                    endpoint.nodetype = NodeType::Move;
                } else if record.controls.is_empty() {
                    endpoint.nodetype = NodeType::Line;
                }
            }
            if closed {
                for control in &records[0].controls {
                    append(*control, &mut nodes, &mut points);
                }
                if records[0].controls.is_empty() {
                    nodes[0].nodetype = NodeType::Line;
                }
            }
            let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
                unreachable!("edited contour remains a path");
            };
            path.nodes = nodes;
            preserved.points = points;
            shape_index += 1;
        }
        Ok(changed)
    }

    /// Reverse every contour containing a selected point while retaining object identities.
    ///
    /// An empty selection reverses every nonempty contour. Closed contours retain their first
    /// stored point so two reversals restore the exact canonical storage order. Returns whether
    /// any topology changed.
    pub fn reverse_contours(&mut self, selected: &[PointId]) -> Result<bool, DocumentEditError> {
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().map(|id| id.0).collect();
        let reverse_all = selected.is_empty();
        let mut changed = false;
        for shape in &mut self.layer.shapes {
            let Shape::Path(path) = shape else {
                continue;
            };
            if path.nodes.is_empty()
                || (!reverse_all
                    && !path.nodes.iter().any(|node| {
                        read_id(&node.format_specific).is_some_and(|id| selected.contains(&id))
                    }))
            {
                continue;
            }
            let contour_id =
                ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
            let preserved = self
                .preserved
                .contours
                .iter_mut()
                .find(|candidate| candidate.id == contour_id)
                .expect("canonical contour preservation");
            changed |= reverse_contour(path, preserved);
        }
        Ok(changed)
    }

    /// Make an on-curve point the first stored point of its closed contour.
    ///
    /// The contour and every point retain their stable identities and source metadata. Returns
    /// whether the canonical storage order changed.
    pub fn set_contour_start(&mut self, point: PointId) -> Result<bool, DocumentEditError> {
        let (shape_index, point_index) = self
            .layer
            .shapes
            .iter()
            .enumerate()
            .find_map(|(shape_index, shape)| {
                let Shape::Path(path) = shape else {
                    return None;
                };
                let point_index = path
                    .nodes
                    .iter()
                    .position(|node| read_id(&node.format_specific) == Some(point.0))?;
                Some((shape_index, point_index))
            })
            .ok_or(DocumentEditError::MissingPoint(point))?;
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        if !path.closed
            || point_index == 0
            || path.nodes[point_index].nodetype == NodeType::OffCurve
        {
            return Ok(false);
        }
        let contour_id =
            ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located contour remains a path");
        };
        path.nodes.rotate_left(point_index);
        self.preserved
            .contours
            .iter_mut()
            .find(|candidate| candidate.id == contour_id)
            .expect("canonical contour preservation")
            .points
            .rotate_left(point_index);
        Ok(true)
    }

    /// Open a closed contour at an on-curve point, or close its open contour.
    ///
    /// Closing changes the initial move point to a line. Opening removes the selected endpoint's
    /// incoming controls, rotates that point to the start and changes it to a move. Returns whether
    /// the contour changed.
    pub fn toggle_contour_open(&mut self, point: PointId) -> Result<bool, DocumentEditError> {
        let (shape_index, point_index) = self
            .layer
            .shapes
            .iter()
            .enumerate()
            .find_map(|(shape_index, shape)| {
                let Shape::Path(path) = shape else {
                    return None;
                };
                let point_index = path
                    .nodes
                    .iter()
                    .position(|node| read_id(&node.format_specific) == Some(point.0))?;
                Some((shape_index, point_index))
            })
            .ok_or(DocumentEditError::MissingPoint(point))?;
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        if path.nodes.len() < 2
            || (path.closed && path.nodes[point_index].nodetype == NodeType::OffCurve)
        {
            return Ok(false);
        }
        let incoming_controls = if path.closed {
            let mut count = 0_usize;
            let mut index = if point_index == 0 {
                path.nodes.len() - 1
            } else {
                point_index - 1
            };
            while path.nodes[index].nodetype == NodeType::OffCurve {
                count += 1;
                index = if index == 0 {
                    path.nodes.len() - 1
                } else {
                    index - 1
                };
            }
            if path.nodes.len() - count < 2 {
                return Ok(false);
            }
            count
        } else {
            0
        };
        let contour_id =
            ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located contour remains a path");
        };
        if path.closed {
            path.nodes.rotate_left(point_index);
            let preserved = self
                .preserved
                .contours
                .iter_mut()
                .find(|candidate| candidate.id == contour_id)
                .expect("canonical contour preservation");
            preserved.points.rotate_left(point_index);
            path.nodes.truncate(path.nodes.len() - incoming_controls);
            preserved
                .points
                .truncate(preserved.points.len() - incoming_controls);
            path.nodes[0].nodetype = NodeType::Move;
            path.closed = false;
        } else {
            path.nodes[0].nodetype = NodeType::Line;
            path.closed = true;
        }
        Ok(true)
    }

    /// Shift every contour point and anchor horizontally.
    ///
    /// Component transforms and the advance remain unchanged, matching a left-sidebearing edit.
    /// Returns whether any geometry moved.
    pub fn shift_points_and_anchors_x(&mut self, delta: f64) -> Result<bool, DocumentEditError> {
        ensure_finite(&[delta])?;
        if delta == 0.0 {
            return Ok(false);
        }
        let mut has_geometry = false;
        for node in self.layer.paths().flat_map(|path| &path.nodes) {
            ensure_finite(&[node.x + delta])?;
            has_geometry = true;
        }
        for anchor in &self.layer.anchors {
            ensure_finite(&[anchor.x + delta])?;
            has_geometry = true;
        }
        if !has_geometry {
            return Ok(false);
        }
        for node in self
            .layer
            .shapes
            .iter_mut()
            .filter_map(|shape| match shape {
                Shape::Path(path) => Some(path),
                Shape::Component(_) => None,
            })
            .flat_map(|path| &mut path.nodes)
        {
            node.x += delta;
        }
        for anchor in &mut self.layer.anchors {
            anchor.x += delta;
        }
        Ok(true)
    }

    /// Insert one on-curve point on a direct segment between two stored endpoints.
    ///
    /// Existing controls retain their identities and metadata while moving to their subdivided
    /// positions. Newly required controls and the inserted point receive fresh identities.
    /// Segments ending at implied quadratic points are handled by a later topology operation.
    pub fn insert_point_on_segment(
        &mut self,
        start: PointId,
        end: PointId,
        parameter: f64,
    ) -> Result<PointId, DocumentEditError> {
        ensure_finite(&[parameter])?;
        let parameter = parameter.clamp(0.0, 1.0);
        let locate = |id: PointId| {
            self.layer
                .shapes
                .iter()
                .enumerate()
                .find_map(|(shape_index, shape)| {
                    let Shape::Path(path) = shape else {
                        return None;
                    };
                    path.nodes
                        .iter()
                        .position(|node| read_id(&node.format_specific) == Some(id.0))
                        .map(|node_index| (shape_index, node_index))
                })
        };
        let (shape_index, start_index) =
            locate(start).ok_or(DocumentEditError::MissingPoint(start))?;
        let (end_shape, end_index) = locate(end).ok_or(DocumentEditError::MissingPoint(end))?;
        if shape_index != end_shape {
            return Err(DocumentEditError::NotDirectSegment(start, end));
        }
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        if path.nodes.len() < 2
            || path.nodes[start_index].nodetype == NodeType::OffCurve
            || path.nodes[end_index].nodetype == NodeType::OffCurve
            || (!path.closed && end_index <= start_index)
        {
            return Err(DocumentEditError::NotDirectSegment(start, end));
        }
        let mut control_indices = Vec::new();
        let mut index = (start_index + 1) % path.nodes.len();
        while index != end_index {
            if path.nodes[index].nodetype != NodeType::OffCurve {
                return Err(DocumentEditError::NotDirectSegment(start, end));
            }
            control_indices.push(index);
            index = (index + 1) % path.nodes.len();
            if !path.closed && index == 0 {
                return Err(DocumentEditError::NotDirectSegment(start, end));
            }
        }
        let start_position =
            kurbo::Point::new(path.nodes[start_index].x, path.nodes[start_index].y);
        let end_position = kurbo::Point::new(path.nodes[end_index].x, path.nodes[end_index].y);
        let endpoint_type = path.nodes[end_index].nodetype;
        let snap = |point: kurbo::Point| {
            kurbo::Point::new(
                crate::outline::point_ops::snap_coord(point.x),
                crate::outline::point_ops::snap_coord(point.y),
            )
        };
        enum Split {
            Line(kurbo::Point),
            Quadratic {
                control: usize,
                left_control: kurbo::Point,
                split: kurbo::Point,
                right_control: kurbo::Point,
            },
            Cubic {
                first_control: usize,
                second_control: usize,
                left_first: kurbo::Point,
                left_second: kurbo::Point,
                split: kurbo::Point,
                right_first: kurbo::Point,
                right_second: kurbo::Point,
            },
        }
        let split = match (control_indices.as_slice(), endpoint_type) {
            ([], _) => Split::Line(snap(start_position.lerp(end_position, parameter))),
            ([control], NodeType::Curve | NodeType::QCurve) => {
                let control_position =
                    kurbo::Point::new(path.nodes[*control].x, path.nodes[*control].y);
                let quad = kurbo::QuadBez::new(start_position, control_position, end_position);
                let left = quad.subsegment(0.0..parameter);
                let right = quad.subsegment(parameter..1.0);
                Split::Quadratic {
                    control: *control,
                    left_control: snap(left.p1),
                    split: snap(left.p2),
                    right_control: snap(right.p1),
                }
            }
            ([first, second], NodeType::Curve) => {
                let first_position = kurbo::Point::new(path.nodes[*first].x, path.nodes[*first].y);
                let second_position =
                    kurbo::Point::new(path.nodes[*second].x, path.nodes[*second].y);
                let cubic = kurbo::CubicBez::new(
                    start_position,
                    first_position,
                    second_position,
                    end_position,
                );
                let left = cubic.subsegment(0.0..parameter);
                let right = cubic.subsegment(parameter..1.0);
                Split::Cubic {
                    first_control: *first,
                    second_control: *second,
                    left_first: snap(left.p1),
                    left_second: snap(left.p2),
                    split: snap(left.p3),
                    right_first: snap(right.p1),
                    right_second: snap(right.p2),
                }
            }
            _ => return Err(DocumentEditError::NotDirectSegment(start, end)),
        };
        match &split {
            Split::Line(point) => ensure_finite(&[point.x, point.y])?,
            Split::Quadratic {
                left_control,
                split,
                right_control,
                ..
            } => ensure_finite(&[
                left_control.x,
                left_control.y,
                split.x,
                split.y,
                right_control.x,
                right_control.y,
            ])?,
            Split::Cubic {
                left_first,
                left_second,
                split,
                right_first,
                right_second,
                ..
            } => ensure_finite(&[
                left_first.x,
                left_first.y,
                left_second.x,
                left_second.y,
                split.x,
                split.y,
                right_first.x,
                right_first.y,
                right_second.x,
                right_second.y,
            ])?,
        }
        let contour_id =
            ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        let preserved = self
            .preserved
            .contours
            .iter_mut()
            .find(|candidate| candidate.id == contour_id)
            .expect("canonical contour preservation");
        let inserted = match split {
            Split::Line(position) => {
                let created = new_document_point(position, NodeType::Line, false);
                let insert_index = if end_index == 0 {
                    path.nodes.len()
                } else {
                    end_index
                };
                path.nodes.insert(insert_index, created.1);
                preserved.points.insert(insert_index, created.2);
                created.0
            }
            Split::Quadratic {
                control,
                left_control,
                split,
                right_control,
            } => {
                path.nodes[control].x = left_control.x;
                path.nodes[control].y = left_control.y;
                let split = new_document_point(split, NodeType::QCurve, false);
                let right = new_document_point(right_control, NodeType::OffCurve, false);
                let insert_index = control + 1;
                path.nodes.insert(insert_index, split.1);
                path.nodes.insert(insert_index + 1, right.1);
                preserved.points.insert(insert_index, split.2);
                preserved.points.insert(insert_index + 1, right.2);
                split.0
            }
            Split::Cubic {
                first_control,
                second_control,
                left_first,
                left_second,
                split,
                right_first,
                right_second,
            } => {
                path.nodes[first_control].x = left_first.x;
                path.nodes[first_control].y = left_first.y;
                path.nodes[second_control].x = right_second.x;
                path.nodes[second_control].y = right_second.y;
                let left = new_document_point(left_second, NodeType::OffCurve, false);
                let split = new_document_point(split, NodeType::Curve, false);
                let right = new_document_point(right_first, NodeType::OffCurve, false);
                path.nodes.insert(second_control, left.1);
                path.nodes.insert(second_control + 1, split.1);
                path.nodes.insert(second_control + 2, right.1);
                preserved.points.insert(second_control, left.2);
                preserved.points.insert(second_control + 1, split.2);
                preserved.points.insert(second_control + 2, right.2);
                split.0
            }
        };
        Ok(inserted)
    }

    /// Insert one on-curve point on a quadratic segment with stored or implied endpoints.
    ///
    /// An implied endpoint is materialized as a fresh on-curve point when subdivision would
    /// otherwise move either control that defines it. The source control retains its identity and
    /// metadata. All computed coordinates are validated before mutation.
    pub fn insert_point_on_quadratic_segment(
        &mut self,
        start: DocumentSegmentEndpoint,
        control: PointId,
        end: DocumentSegmentEndpoint,
        parameter: f64,
    ) -> Result<QuadraticSegmentInsertion, DocumentEditError> {
        ensure_finite(&[parameter])?;
        let parameter = parameter.clamp(0.0, 1.0);
        let representative = |endpoint| match endpoint {
            DocumentSegmentEndpoint::Point(id) => id,
            DocumentSegmentEndpoint::Implied { first_control, .. } => first_control,
        };
        let invalid =
            || DocumentEditError::NotDirectSegment(representative(start), representative(end));
        let locate = |id: PointId| {
            self.layer
                .shapes
                .iter()
                .enumerate()
                .find_map(|(shape_index, shape)| {
                    let Shape::Path(path) = shape else {
                        return None;
                    };
                    path.nodes
                        .iter()
                        .position(|node| read_id(&node.format_specific) == Some(id.0))
                        .map(|node_index| (shape_index, node_index))
                })
        };
        let (shape_index, control_index) =
            locate(control).ok_or(DocumentEditError::MissingPoint(control))?;
        let resolve = |endpoint: DocumentSegmentEndpoint| match endpoint {
            DocumentSegmentEndpoint::Point(id) => {
                let (shape, index) = locate(id).ok_or(DocumentEditError::MissingPoint(id))?;
                let Shape::Path(path) = &self.layer.shapes[shape] else {
                    unreachable!("located endpoint is in a path");
                };
                Ok((
                    shape,
                    kurbo::Point::new(path.nodes[index].x, path.nodes[index].y),
                    Some((index, index)),
                ))
            }
            DocumentSegmentEndpoint::Implied {
                first_control,
                second_control,
            } => {
                let (first_shape, first) =
                    locate(first_control).ok_or(DocumentEditError::MissingPoint(first_control))?;
                let (second_shape, second) = locate(second_control)
                    .ok_or(DocumentEditError::MissingPoint(second_control))?;
                if first_shape != second_shape {
                    return Err(invalid());
                }
                let Shape::Path(path) = &self.layer.shapes[first_shape] else {
                    unreachable!("located endpoint is in a path");
                };
                Ok((
                    first_shape,
                    kurbo::Point::new(path.nodes[first].x, path.nodes[first].y).midpoint(
                        kurbo::Point::new(path.nodes[second].x, path.nodes[second].y),
                    ),
                    Some((first, second)),
                ))
            }
        };
        let (start_shape, start_position, start_indices) = resolve(start)?;
        let (end_shape, end_position, end_indices) = resolve(end)?;
        if start_shape != shape_index || end_shape != shape_index {
            return Err(invalid());
        }
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located segment is in a path");
        };
        if path.nodes[control_index].nodetype != NodeType::OffCurve {
            return Err(invalid());
        }
        let next = |index| {
            if index + 1 < path.nodes.len() {
                Some(index + 1)
            } else if path.closed {
                Some(0)
            } else {
                None
            }
        };
        let is_quadratic_pair = |first: usize, second: usize| {
            if path.nodes[first].nodetype != NodeType::OffCurve
                || path.nodes[second].nodetype != NodeType::OffCurve
                || next(first) != Some(second)
            {
                return false;
            }
            let mut index = second;
            for _ in 0..path.nodes.len() {
                let Some(candidate) = next(index) else {
                    return false;
                };
                if path.nodes[candidate].nodetype != NodeType::OffCurve {
                    return path.nodes[candidate].nodetype == NodeType::QCurve;
                }
                index = candidate;
            }
            path.closed
        };
        let start_valid = match start {
            DocumentSegmentEndpoint::Point(_) => {
                let index = start_indices.expect("stored endpoint index").0;
                path.nodes[index].nodetype != NodeType::OffCurve
                    && next(index) == Some(control_index)
            }
            DocumentSegmentEndpoint::Implied { .. } => {
                let (first, second) = start_indices.expect("implied endpoint indices");
                is_quadratic_pair(first, second) && second == control_index
            }
        };
        let end_valid = match end {
            DocumentSegmentEndpoint::Point(_) => {
                let index = end_indices.expect("stored endpoint index").0;
                path.nodes[index].nodetype != NodeType::OffCurve
                    && next(control_index) == Some(index)
                    && matches!(
                        path.nodes[index].nodetype,
                        NodeType::Curve | NodeType::QCurve
                    )
            }
            DocumentSegmentEndpoint::Implied { .. } => {
                let (first, second) = end_indices.expect("implied endpoint indices");
                is_quadratic_pair(first, second) && first == control_index
            }
        };
        if !start_valid || !end_valid {
            return Err(invalid());
        }
        let control_position =
            kurbo::Point::new(path.nodes[control_index].x, path.nodes[control_index].y);
        let quad = kurbo::QuadBez::new(start_position, control_position, end_position);
        let left = quad.subsegment(0.0..parameter);
        let right = quad.subsegment(parameter..1.0);
        let snap = |point: kurbo::Point| {
            kurbo::Point::new(
                crate::outline::point_ops::snap_coord(point.x),
                crate::outline::point_ops::snap_coord(point.y),
            )
        };
        let left_control = snap(left.p1);
        let split_position = snap(left.p2);
        let right_control = snap(right.p1);
        ensure_finite(&[
            start_position.x,
            start_position.y,
            end_position.x,
            end_position.y,
            left_control.x,
            left_control.y,
            split_position.x,
            split_position.y,
            right_control.x,
            right_control.y,
        ])?;
        let contour_id =
            ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located segment is in a path");
        };
        let preserved = self
            .preserved
            .contours
            .iter_mut()
            .find(|candidate| candidate.id == contour_id)
            .expect("canonical contour preservation");

        let mut control_index = control_index;
        let explicitized_start =
            matches!(start, DocumentSegmentEndpoint::Implied { .. }).then(|| {
                let created = new_document_point(start_position, NodeType::QCurve, false);
                path.nodes.insert(control_index, created.1);
                preserved.points.insert(control_index, created.2);
                control_index += 1;
                created.0
            });
        path.nodes[control_index].x = left_control.x;
        path.nodes[control_index].y = left_control.y;
        let split = new_document_point(split_position, NodeType::QCurve, false);
        let right = new_document_point(right_control, NodeType::OffCurve, false);
        let insert_index = control_index + 1;
        path.nodes.insert(insert_index, split.1);
        path.nodes.insert(insert_index + 1, right.1);
        preserved.points.insert(insert_index, split.2);
        preserved.points.insert(insert_index + 1, right.2);
        let explicitized_end = matches!(end, DocumentSegmentEndpoint::Implied { .. }).then(|| {
            let created = new_document_point(end_position, NodeType::QCurve, false);
            path.nodes.insert(insert_index + 2, created.1);
            preserved.points.insert(insert_index + 2, created.2);
            created.0
        });
        Ok(QuadraticSegmentInsertion {
            point: split.0,
            explicitized_start,
            explicitized_end,
        })
    }

    /// Convert one direct on-curve segment to a cubic with snapped thirds handles.
    ///
    /// The endpoints retain their stable identities and source metadata.
    /// Returns the new control-point identities in contour order.
    pub fn convert_line_to_curve(
        &mut self,
        start: PointId,
        end: PointId,
    ) -> Result<[PointId; 2], DocumentEditError> {
        let locate = |id: PointId| {
            self.layer
                .shapes
                .iter()
                .enumerate()
                .find_map(|(shape_index, shape)| {
                    let Shape::Path(path) = shape else {
                        return None;
                    };
                    path.nodes
                        .iter()
                        .position(|node| read_id(&node.format_specific) == Some(id.0))
                        .map(|node_index| (shape_index, node_index))
                })
        };
        let (shape_index, start_index) =
            locate(start).ok_or(DocumentEditError::MissingPoint(start))?;
        let (end_shape, end_index) = locate(end).ok_or(DocumentEditError::MissingPoint(end))?;
        if shape_index != end_shape {
            return Err(DocumentEditError::NotLineSegment(start, end));
        }
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located shape is a path");
        };
        let wraps = path.closed && start_index + 1 == path.nodes.len() && end_index == 0;
        if !(end_index == start_index + 1 || wraps)
            || path.nodes[start_index].nodetype == NodeType::OffCurve
            || path.nodes[end_index].nodetype == NodeType::OffCurve
        {
            return Err(DocumentEditError::NotLineSegment(start, end));
        }
        let start_position =
            kurbo::Point::new(path.nodes[start_index].x, path.nodes[start_index].y);
        let end_position = kurbo::Point::new(path.nodes[end_index].x, path.nodes[end_index].y);
        let snapped = |point: kurbo::Point| {
            kurbo::Point::new(
                crate::outline::point_ops::snap_coord(point.x),
                crate::outline::point_ops::snap_coord(point.y),
            )
        };
        let first_position = snapped(start_position.lerp(end_position, 1.0 / 3.0));
        let second_position = snapped(start_position.lerp(end_position, 2.0 / 3.0));
        ensure_finite(&[
            first_position.x,
            first_position.y,
            second_position.x,
            second_position.y,
        ])?;
        let point_ids = [PointId::next(), PointId::next()];
        let node = |id: PointId, position: kurbo::Point| {
            let mut node = Node {
                x: position.x,
                y: position.y,
                nodetype: NodeType::OffCurve,
                ..Node::default()
            };
            write_id(&mut node.format_specific, id.0);
            node
        };
        let insert_index = if wraps { start_index + 1 } else { end_index };
        let contour_id =
            ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located shape is a path");
        };
        path.nodes
            .insert(insert_index, node(point_ids[0], first_position));
        path.nodes
            .insert(insert_index + 1, node(point_ids[1], second_position));
        let shifted_end = if wraps { end_index } else { end_index + 2 };
        path.nodes[shifted_end].nodetype = NodeType::Curve;
        let preserved = self
            .preserved
            .contours
            .iter_mut()
            .find(|candidate| candidate.id == contour_id)
            .expect("canonical contour preservation");
        for (offset, id) in point_ids.iter().copied().enumerate() {
            preserved.points.insert(
                insert_index + offset,
                PreservedPoint {
                    id,
                    name: None,
                    metadata: ObjectMetadata {
                        identifier: None,
                        lib: None,
                    },
                },
            );
        }
        Ok(point_ids)
    }

    /// Set one component's exact affine transform by stable identity.
    ///
    /// Returns whether the value changed.
    pub fn set_component_transform(
        &mut self,
        id: ComponentId,
        transform: kurbo::Affine,
    ) -> Result<bool, DocumentEditError> {
        let coefficients = transform.as_coeffs();
        ensure_finite(&coefficients)?;
        let preserved = self
            .preserved
            .components
            .iter_mut()
            .find(|candidate| candidate.id == id)
            .ok_or(DocumentEditError::MissingComponent(id))?;
        let exact = norad::AffineTransform {
            x_scale: coefficients[0],
            xy_scale: coefficients[1],
            yx_scale: coefficients[2],
            y_scale: coefficients[3],
            x_offset: coefficients[4],
            y_offset: coefficients[5],
        };
        if preserved.transform == exact {
            return Ok(false);
        }
        let component = self
            .layer
            .shapes
            .iter_mut()
            .find_map(|shape| match shape {
                Shape::Component(component)
                    if read_id(&component.format_specific) == Some(id.0) =>
                {
                    Some(component)
                }
                Shape::Path(_) | Shape::Component(_) => None,
            })
            .expect("preserved component has canonical geometry");
        preserved.transform = exact;
        component.transform = transform.into();
        Ok(true)
    }

    /// Set one anchor's position by stable identity.
    ///
    /// Returns whether the value changed.
    pub fn set_anchor_position(
        &mut self,
        id: AnchorId,
        position: kurbo::Point,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[position.x, position.y])?;
        let anchor = self
            .layer
            .anchors
            .iter_mut()
            .find(|candidate| read_id(&candidate.format_specific) == Some(id.0))
            .ok_or(DocumentEditError::MissingAnchor(id))?;
        if anchor.x == position.x && anchor.y == position.y {
            return Ok(false);
        }
        anchor.x = position.x;
        anchor.y = position.y;
        Ok(true)
    }

    fn node_mut(&mut self, id: PointId) -> Option<&mut Node> {
        self.layer
            .shapes
            .iter_mut()
            .filter_map(|shape| match shape {
                Shape::Path(path) => Some(path),
                Shape::Component(_) => None,
            })
            .flat_map(|path| &mut path.nodes)
            .find(|node| read_id(&node.format_specific) == Some(id.0))
    }

    fn node(&self, id: PointId) -> Option<&Node> {
        self.layer
            .paths()
            .flat_map(|path| &path.nodes)
            .find(|node| read_id(&node.format_specific) == Some(id.0))
    }

    fn path_and_node_index_mut(&mut self, id: PointId) -> Option<(&mut babelfont::Path, usize)> {
        self.layer.shapes.iter_mut().find_map(|shape| {
            let Shape::Path(path) = shape else {
                return None;
            };
            let index = path
                .nodes
                .iter()
                .position(|node| read_id(&node.format_specific) == Some(id.0))?;
            Some((path, index))
        })
    }

    fn contour_shape_index(&self, id: ContourId) -> Option<usize> {
        self.layer.shapes.iter().position(|shape| match shape {
            Shape::Path(path) => read_id(&path.format_specific) == Some(id.0),
            Shape::Component(_) => false,
        })
    }
}

/// Why a canonical document edit could not be applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentEditError {
    /// The requested glyph layer does not exist.
    MissingLayer,
    /// The requested source identity does not exist.
    MissingSource,
    /// The requested point identity does not exist in the layer.
    MissingPoint(PointId),
    /// The requested contour identity does not exist in the layer.
    MissingContour(ContourId),
    /// The requested contour is already closed or lacks an initial move point.
    NotOpenContour(ContourId),
    /// A persistent drag omitted an automatically affected point's start position.
    MissingDragOrigin(PointId),
    /// The requested endpoints do not identify one direct on-curve segment.
    NotLineSegment(PointId, PointId),
    /// The requested endpoints do not identify one directly editable stored-endpoint segment.
    NotDirectSegment(PointId, PointId),
    /// A move point was requested anywhere except the start of an open contour.
    NonInitialMove(PointId),
    /// The requested component identity does not exist in the layer.
    MissingComponent(ComponentId),
    /// The requested anchor identity does not exist in the layer.
    MissingAnchor(AnchorId),
    /// A numeric edit contained NaN or infinity.
    NonFinite,
    /// The edit closure rejected its owned draft.
    Rejected,
}

impl std::fmt::Display for DocumentEditError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingLayer => formatter.write_str("glyph layer does not exist"),
            Self::MissingSource => formatter.write_str("source does not exist"),
            Self::MissingPoint(id) => write!(formatter, "point {id:?} does not exist"),
            Self::MissingContour(id) => write!(formatter, "contour {id:?} does not exist"),
            Self::NotOpenContour(id) => write!(formatter, "contour {id:?} is not open"),
            Self::MissingDragOrigin(id) => {
                write!(formatter, "point {id:?} is missing its drag-start position")
            }
            Self::NotLineSegment(start, end) => {
                write!(
                    formatter,
                    "points {start:?} and {end:?} do not form a line segment"
                )
            }
            Self::NotDirectSegment(start, end) => {
                write!(
                    formatter,
                    "points {start:?} and {end:?} do not form one direct editable segment"
                )
            }
            Self::NonInitialMove(id) => {
                write!(formatter, "point {id:?} cannot be a noninitial move point")
            }
            Self::MissingComponent(id) => write!(formatter, "component {id:?} does not exist"),
            Self::MissingAnchor(id) => write!(formatter, "anchor {id:?} does not exist"),
            Self::NonFinite => {
                formatter.write_str("document coordinates and metrics must be finite")
            }
            Self::Rejected => formatter.write_str("document edit was rejected"),
        }
    }
}

impl std::error::Error for DocumentEditError {}

fn ensure_finite(values: &[f64]) -> Result<(), DocumentEditError> {
    values
        .iter()
        .all(|value| value.is_finite())
        .then_some(())
        .ok_or(DocumentEditError::NonFinite)
}

fn new_document_point(
    position: kurbo::Point,
    point_type: NodeType,
    smooth: bool,
) -> (PointId, Node, PreservedPoint) {
    let id = PointId::next();
    let mut point = Node {
        x: position.x,
        y: position.y,
        nodetype: point_type,
        smooth,
        ..Node::default()
    };
    write_id(&mut point.format_specific, id.0);
    (
        id,
        point,
        PreservedPoint {
            id,
            name: None,
            metadata: ObjectMetadata {
                identifier: None,
                lib: None,
            },
        },
    )
}

fn reverse_contour(path: &mut babelfont::Path, preserved: &mut PreservedContour) -> bool {
    debug_assert_eq!(
        path.nodes.len(),
        preserved.points.len(),
        "canonical nodes and preserved point records stay aligned"
    );
    let original_nodes = path.nodes.clone();
    let first_id = path
        .closed
        .then(|| read_id(&path.nodes[0].format_specific).expect("canonical point identity"));
    path.nodes.reverse();
    preserved.points.reverse();
    if let Some(first_id) = first_id {
        let offset = path
            .nodes
            .iter()
            .position(|node| read_id(&node.format_specific) == Some(first_id))
            .expect("closed contour retained its first point");
        path.nodes.rotate_left(offset);
        preserved.points.rotate_left(offset);
    }

    let on_curve: Vec<_> = path
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| (node.nodetype != NodeType::OffCurve).then_some(index))
        .collect();
    let old_types: Vec<_> = on_curve
        .iter()
        .map(|index| path.nodes[*index].nodetype)
        .collect();
    for (position, index) in on_curve.into_iter().enumerate() {
        path.nodes[index].nodetype = if !path.closed && position == 0 {
            NodeType::Move
        } else {
            old_types[(position + old_types.len() - 1) % old_types.len()]
        };
    }
    path.nodes != original_nodes
}

fn materialize_deleted_quadratic_controls(
    path: &mut babelfont::Path,
    preserved: &mut PreservedContour,
    selected: &HashSet<u64>,
) -> Result<bool, DocumentEditError> {
    let length = path.nodes.len();
    if length < 2 {
        return Ok(false);
    }
    let next = |index| {
        if index + 1 < length {
            Some(index + 1)
        } else if path.closed {
            Some(0)
        } else {
            None
        }
    };
    let all_off_curve = path.closed
        && path
            .nodes
            .iter()
            .all(|node| node.nodetype == NodeType::OffCurve);
    let belongs_to_quadratic_chain = |control: usize| {
        if path.nodes[control].nodetype != NodeType::OffCurve {
            return false;
        }
        if all_off_curve {
            return true;
        }
        let mut index = control;
        for _ in 0..length {
            let Some(candidate) = next(index) else {
                return false;
            };
            if path.nodes[candidate].nodetype != NodeType::OffCurve {
                return path.nodes[candidate].nodetype == NodeType::QCurve;
            }
            index = candidate;
        }
        false
    };
    let selected_controls: HashSet<_> = path
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            let id = read_id(&node.format_specific).expect("canonical point identity");
            (selected.contains(&id) && belongs_to_quadratic_chain(index)).then_some(index)
        })
        .collect();
    if selected_controls.is_empty() {
        return Ok(false);
    }
    let boundary_before: Vec<_> = (0..length)
        .map(|index| {
            let previous = if index == 0 {
                path.closed.then_some(length - 1)
            } else {
                Some(index - 1)
            }?;
            (path.nodes[previous].nodetype == NodeType::OffCurve
                && path.nodes[index].nodetype == NodeType::OffCurve
                && (selected_controls.contains(&previous) || selected_controls.contains(&index)))
            .then(|| {
                kurbo::Point::new(path.nodes[previous].x, path.nodes[previous].y)
                    .midpoint(kurbo::Point::new(path.nodes[index].x, path.nodes[index].y))
            })
        })
        .collect();
    for position in boundary_before.iter().flatten() {
        ensure_finite(&[position.x, position.y])?;
    }

    let old_nodes = path.nodes.clone();
    let old_points = preserved.points.clone();
    let mut nodes = Vec::with_capacity(length + boundary_before.iter().flatten().count());
    let mut points = Vec::with_capacity(nodes.capacity());
    for index in 0..length {
        if let Some(position) = boundary_before[index] {
            let created = new_document_point(position, NodeType::QCurve, false);
            nodes.push(created.1);
            points.push(created.2);
        }
        if !selected_controls.contains(&index) {
            nodes.push(old_nodes[index].clone());
            points.push(old_points[index].clone());
        }
    }
    path.nodes = nodes;
    preserved.points = points;
    Ok(true)
}

impl<'a> AnchorView<'a> {
    /// Stable identity retained across ordinary edits and reorder.
    pub fn id(self) -> AnchorId {
        self.preserved.id
    }

    /// Position in font design coordinates.
    pub fn position(self) -> kurbo::Point {
        kurbo::Point::new(self.anchor.x, self.anchor.y)
    }

    /// Source anchor name.
    pub fn name(self) -> &'a str {
        &self.anchor.name
    }
}

fn write_id(format: &mut babelfont::FormatSpecific, id: u64) {
    format.insert(OBJECT_ID_KEY.into(), id.into());
}

fn read_id(format: &babelfont::FormatSpecific) -> Option<u64> {
    format.get(OBJECT_ID_KEY)?.as_u64()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the UFO projection retains the exact advance"
)]
pub(super) fn layer_from_ufo(
    glyph: &norad::Glyph,
    id: &LayerId,
    default: bool,
) -> (Layer, LayerPreservation) {
    let mut layer = Layer {
        id: Some(layer_key(id)),
        name: Some(id.name.clone()),
        width: glyph.width as f32,
        master: if default {
            babelfont::LayerType::DefaultForMaster(id.source.0.to_string())
        } else {
            babelfont::LayerType::AssociatedWithMaster(id.source.0.to_string())
        },
        ..Layer::default()
    };
    let mut contours = Vec::with_capacity(glyph.contours.len());
    for contour in &glyph.contours {
        let contour_id = ContourId::next();
        let mut points = Vec::with_capacity(contour.points.len());
        let mut path = babelfont::Path {
            closed: contour.is_closed(),
            ..babelfont::Path::default()
        };
        write_id(&mut path.format_specific, contour_id.0);
        path.nodes = contour
            .points
            .iter()
            .map(|point| {
                let point_id = PointId::next();
                points.push(PreservedPoint {
                    id: point_id,
                    name: point.name.clone(),
                    metadata: ObjectMetadata::new(point.identifier(), point.lib()),
                });
                let mut node = Node {
                    x: point.x,
                    y: point.y,
                    nodetype: match point.typ {
                        norad::PointType::Move => NodeType::Move,
                        norad::PointType::Line => NodeType::Line,
                        norad::PointType::OffCurve => NodeType::OffCurve,
                        norad::PointType::Curve => NodeType::Curve,
                        norad::PointType::QCurve => NodeType::QCurve,
                    },
                    smooth: point.smooth,
                    ..Node::default()
                };
                write_id(&mut node.format_specific, point_id.0);
                node
            })
            .collect();
        layer.shapes.push(Shape::Path(path));
        contours.push(PreservedContour {
            id: contour_id,
            metadata: ObjectMetadata::new(contour.identifier(), contour.lib()),
            points,
        });
    }
    let mut components = Vec::with_capacity(glyph.components.len());
    for component in &glyph.components {
        let component_id = ComponentId::next();
        let mut output = Component {
            reference: component.base.as_str().into(),
            transform: affine(component.transform).into(),
            location: std::iter::empty().collect(),
            format_specific: babelfont::FormatSpecific::default(),
        };
        write_id(&mut output.format_specific, component_id.0);
        layer.shapes.push(Shape::Component(output));
        components.push(PreservedComponent {
            id: component_id,
            transform: component.transform,
            metadata: ObjectMetadata::new(component.identifier(), component.lib()),
        });
    }
    let mut anchors = Vec::with_capacity(glyph.anchors.len());
    layer.anchors = glyph
        .anchors
        .iter()
        .map(|anchor| {
            let anchor_id = AnchorId::next();
            anchors.push(PreservedAnchor {
                id: anchor_id,
                color: anchor.color,
                metadata: ObjectMetadata::new(anchor.identifier(), anchor.lib()),
            });
            let mut output = Anchor {
                x: anchor.x,
                y: anchor.y,
                name: anchor
                    .name
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
                ..Anchor::default()
            };
            write_id(&mut output.format_specific, anchor_id.0);
            output
        })
        .collect();
    (
        layer,
        LayerPreservation {
            name: glyph.name().to_string(),
            width: glyph.width,
            height: glyph.height,
            codepoints: glyph.codepoints.clone(),
            note: glyph.note.clone(),
            guidelines: glyph.guidelines.clone(),
            image: glyph.image.clone(),
            lib: glyph.lib.clone(),
            contours,
            components,
            anchors,
        },
    )
}

pub(super) fn copy_layer(
    layer: &Layer,
    preserved: &LayerPreservation,
    id: &LayerId,
) -> (Layer, LayerPreservation) {
    let mut layer = layer.clone();
    let mut preserved = preserved.clone();
    layer.id = Some(layer_key(id));
    layer.name = Some(id.name.clone());
    layer.master = babelfont::LayerType::AssociatedWithMaster(id.source.0.to_string());

    for (path, preserved) in layer
        .shapes
        .iter_mut()
        .filter_map(|shape| match shape {
            Shape::Path(path) => Some(path),
            Shape::Component(_) => None,
        })
        .zip(&mut preserved.contours)
    {
        preserved.id = ContourId::next();
        write_id(&mut path.format_specific, preserved.id.0);
        for (node, preserved) in path.nodes.iter_mut().zip(&mut preserved.points) {
            preserved.id = PointId::next();
            write_id(&mut node.format_specific, preserved.id.0);
        }
    }
    for (component, preserved) in layer
        .shapes
        .iter_mut()
        .filter_map(|shape| match shape {
            Shape::Component(component) => Some(component),
            Shape::Path(_) => None,
        })
        .zip(&mut preserved.components)
    {
        preserved.id = ComponentId::next();
        write_id(&mut component.format_specific, preserved.id.0);
    }
    for (anchor, preserved) in layer.anchors.iter_mut().zip(&mut preserved.anchors) {
        preserved.id = AnchorId::next();
        write_id(&mut anchor.format_specific, preserved.id.0);
    }
    (layer, preserved)
}

pub(super) fn reconcile_layer_from_ufo(
    glyph: &norad::Glyph,
    id: &LayerId,
    default: bool,
    previous_layer: &Layer,
    previous: &LayerPreservation,
) -> (Layer, LayerPreservation) {
    let old = project_layer(previous_layer, previous);
    let (mut layer, mut preservation) = layer_from_ufo(glyph, id, default);

    let mut used_contours = vec![false; old.contours.len()];
    for (index, (contour, path)) in glyph
        .contours
        .iter()
        .zip(layer.shapes.iter_mut().filter_map(|shape| match shape {
            Shape::Path(path) => Some(path),
            Shape::Component(_) => None,
        }))
        .enumerate()
    {
        let old_index = match_index(&old.contours, &used_contours, |candidate| {
            contour.identifier().is_some() && contour.identifier() == candidate.identifier()
        })
        .or_else(|| {
            match_index(&old.contours, &used_contours, |candidate| {
                contour == candidate
            })
        })
        .or_else(|| {
            match_index(&old.contours, &used_contours, |candidate| {
                contour_signature_matches(contour, candidate)
            })
        });
        let Some(old_index) = old_index else {
            continue;
        };
        used_contours[old_index] = true;
        let old_preserved = &previous.contours[old_index];
        let new_preserved = &mut preservation.contours[index];
        new_preserved.id = old_preserved.id;
        write_id(&mut path.format_specific, old_preserved.id.0);

        let mut used_points = vec![false; old.contours[old_index].points.len()];
        for (point_index, (point, node)) in contour.points.iter().zip(&mut path.nodes).enumerate() {
            let old_point =
                match_index(&old.contours[old_index].points, &used_points, |candidate| {
                    point.identifier().is_some() && point.identifier() == candidate.identifier()
                })
                .or_else(|| {
                    match_index(&old.contours[old_index].points, &used_points, |candidate| {
                        point == candidate
                    })
                })
                .or_else(|| {
                    match_index(&old.contours[old_index].points, &used_points, |candidate| {
                        point_metadata_matches(point, candidate)
                    })
                });
            let Some(old_point) = old_point else {
                continue;
            };
            used_points[old_point] = true;
            let id = old_preserved.points[old_point].id;
            preservation.contours[index].points[point_index].id = id;
            write_id(&mut node.format_specific, id.0);
        }
    }

    let mut used_components = vec![false; old.components.len()];
    for (index, (component, shape)) in glyph
        .components
        .iter()
        .zip(layer.shapes.iter_mut().filter_map(|shape| match shape {
            Shape::Component(component) => Some(component),
            Shape::Path(_) => None,
        }))
        .enumerate()
    {
        let old_index = match_index(&old.components, &used_components, |candidate| {
            component.identifier().is_some() && component.identifier() == candidate.identifier()
        })
        .or_else(|| {
            match_index(&old.components, &used_components, |candidate| {
                component == candidate
            })
        })
        .or_else(|| {
            match_index(&old.components, &used_components, |candidate| {
                component_metadata_matches(component, candidate)
            })
        });
        let Some(old_index) = old_index else {
            continue;
        };
        used_components[old_index] = true;
        let id = previous.components[old_index].id;
        preservation.components[index].id = id;
        write_id(&mut shape.format_specific, id.0);
    }

    let mut used_anchors = vec![false; old.anchors.len()];
    for (index, (anchor, projected)) in glyph.anchors.iter().zip(&mut layer.anchors).enumerate() {
        let old_index = match_index(&old.anchors, &used_anchors, |candidate| {
            anchor.identifier().is_some() && anchor.identifier() == candidate.identifier()
        })
        .or_else(|| match_index(&old.anchors, &used_anchors, |candidate| anchor == candidate))
        .or_else(|| {
            match_index(&old.anchors, &used_anchors, |candidate| {
                anchor_metadata_matches(anchor, candidate)
            })
        });
        let Some(old_index) = old_index else {
            continue;
        };
        used_anchors[old_index] = true;
        let id = previous.anchors[old_index].id;
        preservation.anchors[index].id = id;
        write_id(&mut projected.format_specific, id.0);
    }

    (layer, preservation)
}

fn match_index<T>(items: &[T], used: &[bool], predicate: impl Fn(&T) -> bool) -> Option<usize> {
    let mut matches = items
        .iter()
        .enumerate()
        .filter(|(index, item)| !used[*index] && predicate(item))
        .map(|(index, _)| index);
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

fn contour_signature_matches(a: &norad::Contour, b: &norad::Contour) -> bool {
    let object_metadata = a.identifier().is_some() || a.lib().is_some();
    let point_metadata = a
        .points
        .iter()
        .any(|point| point.identifier().is_some() || point.lib().is_some() || point.name.is_some());
    (object_metadata || point_metadata)
        && a.identifier() == b.identifier()
        && a.lib() == b.lib()
        && a.points.len() == b.points.len()
        && a.points.iter().all(|a| {
            b.points
                .iter()
                .filter(|b| {
                    a.identifier() == b.identifier()
                        && a.lib() == b.lib()
                        && a.name == b.name
                        && a.typ == b.typ
                })
                .count()
                == 1
        })
}

fn point_metadata_matches(a: &norad::ContourPoint, b: &norad::ContourPoint) -> bool {
    (a.identifier().is_some() || a.lib().is_some() || a.name.is_some())
        && a.identifier() == b.identifier()
        && a.lib() == b.lib()
        && a.name == b.name
        && a.typ == b.typ
}

fn component_metadata_matches(a: &norad::Component, b: &norad::Component) -> bool {
    a.identifier() == b.identifier() && a.lib() == b.lib() && a.base == b.base
}

fn anchor_metadata_matches(a: &norad::Anchor, b: &norad::Anchor) -> bool {
    (a.identifier().is_some() || a.lib().is_some() || a.name.is_some() || a.color.is_some())
        && a.identifier() == b.identifier()
        && a.lib() == b.lib()
        && a.name == b.name
        && a.color == b.color
}

fn affine(t: norad::AffineTransform) -> kurbo::Affine {
    kurbo::Affine::new([
        t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset,
    ])
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "compare with the original narrowed Babelfont advance"
)]
pub(super) fn project_layer(layer: &Layer, preserved: &LayerPreservation) -> norad::Glyph {
    let mut glyph = norad::Glyph::new(&preserved.name);
    glyph.width = preserved.width;
    glyph.height = preserved.height;
    glyph.codepoints.clone_from(&preserved.codepoints);
    glyph.note.clone_from(&preserved.note);
    glyph.guidelines.clone_from(&preserved.guidelines);
    glyph.image.clone_from(&preserved.image);
    glyph.lib.clone_from(&preserved.lib);
    if layer.width != preserved.width as f32 {
        glyph.width = f64::from(layer.width);
    }
    glyph.contours = layer
        .paths()
        .map(|path| {
            let preserved_contour = read_id(&path.format_specific)
                .and_then(|id| preserved.contours.iter().find(|item| item.id.0 == id));
            let points = path
                .nodes
                .iter()
                .map(|node| {
                    let original = read_id(&node.format_specific).and_then(|id| {
                        let item = preserved_contour?;
                        item.points.iter().find(|point| point.id.0 == id)
                    });
                    let typ = match node.nodetype {
                        NodeType::Move => norad::PointType::Move,
                        NodeType::Line => norad::PointType::Line,
                        NodeType::OffCurve => norad::PointType::OffCurve,
                        NodeType::Curve => norad::PointType::Curve,
                        NodeType::QCurve => norad::PointType::QCurve,
                    };
                    let mut point = norad::ContourPoint::new(
                        node.x,
                        node.y,
                        typ,
                        node.smooth,
                        original.and_then(|item| item.name.clone()),
                        original.and_then(|item| item.metadata.identifier.clone()),
                    );
                    if let Some(lib) = original.and_then(|item| item.metadata.lib.clone()) {
                        point.replace_lib(lib);
                    }
                    point
                })
                .collect();
            let mut contour = norad::Contour::new(
                points,
                preserved_contour.and_then(|item| item.metadata.identifier.clone()),
            );
            if let Some(lib) = preserved_contour.and_then(|item| item.metadata.lib.clone()) {
                contour.replace_lib(lib);
            }
            contour
        })
        .collect();
    glyph.components = layer
        .components()
        .map(|component| {
            let base = norad::Name::new(&component.reference).expect("validated glyph name");
            let original = read_id(&component.format_specific)
                .and_then(|id| preserved.components.iter().find(|item| item.id.0 == id));
            let exact =
                original.map_or_else(norad::AffineTransform::default, |item| item.transform);
            let decomposed: babelfont::DecomposedAffine = affine(exact).into();
            let transform = if decomposed == component.transform {
                exact
            } else {
                let [x_scale, xy_scale, yx_scale, y_scale, x_offset, y_offset] =
                    component.transform.as_affine().as_coeffs();
                norad::AffineTransform {
                    x_scale,
                    xy_scale,
                    yx_scale,
                    y_scale,
                    x_offset,
                    y_offset,
                }
            };
            let mut output = norad::Component::new(
                base,
                transform,
                original.and_then(|item| item.metadata.identifier.clone()),
            );
            if let Some(lib) = original.and_then(|item| item.metadata.lib.clone()) {
                output.replace_lib(lib);
            }
            output
        })
        .collect();
    glyph.anchors = layer
        .anchors
        .iter()
        .map(|anchor| {
            let original = read_id(&anchor.format_specific)
                .and_then(|id| preserved.anchors.iter().find(|item| item.id.0 == id));
            let mut output = norad::Anchor::new(
                anchor.x,
                anchor.y,
                (!anchor.name.is_empty())
                    .then(|| norad::Name::new(&anchor.name).expect("validated anchor name")),
                original.and_then(|item| item.color),
                original.and_then(|item| item.metadata.identifier.clone()),
            );
            if let Some(lib) = original.and_then(|item| item.metadata.lib.clone()) {
                output.replace_lib(lib);
            }
            output
        })
        .collect();
    glyph
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identifier(value: &str) -> norad::Identifier {
        norad::Identifier::new(value).unwrap()
    }

    fn object_lib(key: &str, value: &str) -> plist::Dictionary {
        let mut lib = plist::Dictionary::new();
        lib.insert(key.into(), value.into());
        lib
    }

    #[test]
    fn projection_follows_object_identity_after_reorder_and_insert() {
        let mut glyph = norad::Glyph::new("A");
        let mut first = norad::ContourPoint::new(
            10.0,
            20.0,
            norad::PointType::Line,
            false,
            Some(norad::Name::new("first").unwrap()),
            Some(identifier("point.first")),
        );
        first.replace_lib(object_lib("owner", "first"));
        let second = norad::ContourPoint::new(
            30.0,
            40.0,
            norad::PointType::Line,
            false,
            Some(norad::Name::new("second").unwrap()),
            Some(identifier("point.second")),
        );
        let mut contour =
            norad::Contour::new(vec![first, second], Some(identifier("contour.original")));
        contour.replace_lib(object_lib("contour", "metadata"));
        glyph.contours.push(contour);
        let mut top = norad::Anchor::new(
            10.0,
            20.0,
            Some(norad::Name::new("top").unwrap()),
            None,
            Some(identifier("anchor.top")),
        );
        top.replace_lib(object_lib("anchor", "metadata"));
        glyph.anchors.push(top);
        glyph.anchors.push(norad::Anchor::new(
            30.0,
            40.0,
            Some(norad::Name::new("bottom").unwrap()),
            None,
            Some(identifier("anchor.bottom")),
        ));
        let first_transform = norad::AffineTransform {
            x_scale: 1.000_000_000_000_1,
            xy_scale: 0.125,
            yx_scale: -0.25,
            y_scale: 0.999_999_999_999_9,
            x_offset: 12.345_678_901_234,
            y_offset: -98.765_432_109_876,
        };
        let mut first_component = norad::Component::new(
            norad::Name::new("base.first").unwrap(),
            first_transform,
            Some(identifier("component.first")),
        );
        first_component.replace_lib(object_lib("component", "first"));
        glyph.components.push(first_component);
        glyph.components.push(norad::Component::new(
            norad::Name::new("base.second").unwrap(),
            norad::AffineTransform {
                x_offset: 50.0,
                ..norad::AffineTransform::default()
            },
            Some(identifier("component.second")),
        ));
        let id = LayerId {
            source: super::super::variable::SourceId(0),
            name: "public.default".into(),
        };
        let (mut layer, preserved) = layer_from_ufo(&glyph, &id, true);
        let Shape::Path(path) = &mut layer.shapes[0] else {
            panic!("first shape is a path");
        };
        path.nodes.swap(0, 1);
        path.nodes.insert(
            0,
            Node {
                x: 5.0,
                y: 5.0,
                nodetype: NodeType::Line,
                ..Node::default()
            },
        );
        layer.shapes.swap(1, 2);
        layer.anchors.swap(0, 1);

        let output = project_layer(&layer, &preserved);
        assert_eq!(
            output.contours[0].identifier(),
            Some(&identifier("contour.original"))
        );
        assert_eq!(output.contours[0].points[0].identifier(), None);
        assert_eq!(
            output.contours[0].points[1].identifier(),
            Some(&identifier("point.second"))
        );
        assert_eq!(
            output.contours[0].points[2].identifier(),
            Some(&identifier("point.first"))
        );
        assert_eq!(
            output.contours[0].points[2].lib().unwrap()["owner"],
            "first".into()
        );
        assert_eq!(
            output.anchors[0].identifier(),
            Some(&identifier("anchor.bottom"))
        );
        assert_eq!(
            output.anchors[1].identifier(),
            Some(&identifier("anchor.top"))
        );
        assert_eq!(
            output.anchors[1].lib().unwrap()["anchor"],
            "metadata".into()
        );
        assert_eq!(output.components[0].base.as_str(), "base.second");
        assert_eq!(
            output.components[0].identifier(),
            Some(&identifier("component.second"))
        );
        assert_eq!(output.components[1].base.as_str(), "base.first");
        assert_eq!(output.components[1].transform, first_transform);
        assert_eq!(
            output.components[1].identifier(),
            Some(&identifier("component.first"))
        );
        assert_eq!(
            output.components[1].lib().unwrap()["component"],
            "first".into()
        );
    }

    #[test]
    fn reconciliation_retains_identity_across_legacy_edits_and_reorder() {
        let mut glyph = norad::Glyph::new("A");
        glyph.contours.push(norad::Contour::new(
            vec![
                norad::ContourPoint::new(
                    10.0,
                    20.0,
                    norad::PointType::Line,
                    false,
                    Some(norad::Name::new("first").unwrap()),
                    Some(identifier("point.first")),
                ),
                norad::ContourPoint::new(
                    30.0,
                    40.0,
                    norad::PointType::Line,
                    false,
                    Some(norad::Name::new("second").unwrap()),
                    Some(identifier("point.second")),
                ),
            ],
            Some(identifier("contour.original")),
        ));
        for (name, x) in [("base.first", 10.0), ("base.second", 20.0)] {
            glyph.components.push(norad::Component::new(
                norad::Name::new(name).unwrap(),
                norad::AffineTransform {
                    x_offset: x,
                    ..norad::AffineTransform::default()
                },
                Some(identifier(name)),
            ));
        }
        for (name, x) in [("top", 10.0), ("bottom", 20.0)] {
            glyph.anchors.push(norad::Anchor::new(
                x,
                100.0,
                Some(norad::Name::new(name).unwrap()),
                None,
                Some(identifier(name)),
            ));
        }
        let id = LayerId {
            source: super::super::variable::SourceId(0),
            name: "public.default".into(),
        };
        let (layer, preservation) = layer_from_ufo(&glyph, &id, true);
        let old_point_ids: Vec<_> = layer
            .paths()
            .next()
            .unwrap()
            .nodes
            .iter()
            .map(|node| read_id(&node.format_specific).unwrap())
            .collect();
        let old_component_ids: Vec<_> = layer
            .components()
            .map(|component| read_id(&component.format_specific).unwrap())
            .collect();
        let old_anchor_ids: Vec<_> = layer
            .anchors
            .iter()
            .map(|anchor| read_id(&anchor.format_specific).unwrap())
            .collect();

        let mut edited = project_layer(&layer, &preservation);
        edited.contours[0].points.swap(0, 1);
        edited.contours[0].points[0].x = 333.0;
        edited.components.swap(0, 1);
        edited.components[0].transform.x_offset = 222.0;
        edited.anchors.swap(0, 1);
        edited.anchors[0].x = 111.0;

        let (reconciled, preservation) =
            reconcile_layer_from_ufo(&edited, &id, true, &layer, &preservation);
        let point_ids: Vec<_> = reconciled
            .paths()
            .next()
            .unwrap()
            .nodes
            .iter()
            .map(|node| read_id(&node.format_specific).unwrap())
            .collect();
        let component_ids: Vec<_> = reconciled
            .components()
            .map(|component| read_id(&component.format_specific).unwrap())
            .collect();
        let anchor_ids: Vec<_> = reconciled
            .anchors
            .iter()
            .map(|anchor| read_id(&anchor.format_specific).unwrap())
            .collect();
        assert_eq!(point_ids, [old_point_ids[1], old_point_ids[0]]);
        assert_eq!(component_ids, [old_component_ids[1], old_component_ids[0]]);
        assert_eq!(anchor_ids, [old_anchor_ids[1], old_anchor_ids[0]]);
        assert_eq!(project_layer(&reconciled, &preservation), edited);
    }
}

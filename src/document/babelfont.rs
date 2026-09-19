// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Babelfont geometry with lossless UFO persistence projections.
//!
//! Babelfont owns paths, anchors and components. UFO payloads retain metadata its
//! model cannot express, plus exact advances and affine coefficients. A projection
//! takes geometry from Babelfont, restoring exact numbers when their corresponding
//! Babelfont value is unchanged. Compilation is the only quantizing boundary.

use std::sync::atomic::{AtomicU64, Ordering};

use babelfont::{Anchor, Component, Layer, Node, NodeType, Shape};

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

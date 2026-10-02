// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Read-only views of canonical glyph layers and their shapes.

use super::*;

/// Read-only access to one canonical glyph layer.
#[derive(Clone, Copy, Debug)]
pub struct LayerView<'a> {
    pub(super) layer: &'a Layer,
    pub(super) preserved: &'a LayerPreservation,
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
    pub(in crate::font) fn new(layer: &'a Layer, preserved: &'a LayerPreservation) -> Self {
        Self { layer, preserved }
    }

    pub(in crate::font) fn codec_parts(self) -> (&'a Layer, &'a LayerPreservation) {
        (self.layer, self.preserved)
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

    /// Optional source image attached to this glyph layer.
    pub fn image(self) -> Option<&'a LayerImage> {
        self.preserved.image.as_ref()
    }

    /// Typed public mark color, preserving an invalid source value as an explicit error.
    pub fn mark_color(self) -> Result<Option<MarkColor>, DocumentEditError> {
        parse_mark_color(self.preserved.mark_color.as_ref())
    }

    /// The exact semantic mark label, if the source stores a valid string.
    pub fn mark_label(self) -> Result<Option<&'a str>, DocumentEditError> {
        match self.preserved.lib.get(MARK_LABEL_KEY) {
            None => Ok(None),
            Some(plist::Value::String(label)) if !label.is_empty() => Ok(Some(label)),
            Some(_) => Err(DocumentEditError::InvalidLayerMetadata),
        }
    }

    /// Exact source spelling of one valid left or right metrics formula.
    pub fn metrics_key(self, left: bool) -> Result<Option<&'a str>, DocumentEditError> {
        let value = if left {
            self.preserved.left_metrics_key.as_ref()
        } else {
            self.preserved.right_metrics_key.as_ref()
        };
        match value {
            None => Ok(None),
            Some(plist::Value::String(source)) if parse_metrics_key(source).is_some() => {
                Ok(Some(source))
            }
            Some(_) => Err(DocumentEditError::InvalidLayerMetadata),
        }
    }

    /// Parsed left or right metrics formula.
    pub fn metrics_formula(self, left: bool) -> Result<Option<MetricsFormula>, DocumentEditError> {
        Ok(self.metrics_key(left)?.and_then(parse_metrics_key))
    }

    /// Validated editable metaball data; a missing key is an empty version-one value.
    pub fn metaballs(self) -> Result<Metaballs, DocumentEditError> {
        parse_metaballs(self.preserved.metaballs.as_ref())
    }

    /// The validated neural item of this layer; a missing key is an empty item.
    pub fn neural_item(
        self,
    ) -> Result<crate::font::model::neural_item::NeuralItem, DocumentEditError> {
        use crate::font::model::neural_item::{NEURAL_ITEM_KEY, NeuralItem};
        let Some(value) = self.preserved.lib.get(NEURAL_ITEM_KEY) else {
            return Ok(NeuralItem::default());
        };
        let item: NeuralItem =
            plist::from_value(value).map_err(|_| DocumentEditError::InvalidLayerMetadata)?;
        item.validate()
            .map_err(|_| DocumentEditError::InvalidLayerMetadata)?;
        Ok(item)
    }

    /// Exact source spelling of one valid explicit composition recipe.
    pub fn composition_recipe_source(self) -> Result<Option<&'a str>, DocumentEditError> {
        match self.preserved.composition_recipe.as_ref() {
            None => Ok(None),
            Some(plist::Value::String(source)) => Ok(Some(source)),
            Some(_) => Err(DocumentEditError::InvalidLayerMetadata),
        }
    }

    /// Smart axes declared by this component-source layer.
    pub fn smart_component_axes(self) -> Option<&'a SmartComponentAxes> {
        self.preserved.smart_component_axes.as_ref()
    }

    /// The smart-axis value bound to one stable component identity.
    pub fn smart_component_value(self, component: ComponentId, axis: &str) -> Option<f64> {
        self.preserved
            .smart_component_values
            .as_ref()?
            .value(component, axis)
    }

    /// Pole selection metadata attached to this source layer.
    pub fn smart_component_pole(self) -> Option<&'a SmartComponentPole> {
        self.preserved.smart_component_pole.as_ref()
    }

    /// Typed HOI intermediate points attached to this source layer.
    pub fn hoi_intermediates(self) -> Option<&'a HoiIntermediates> {
        self.preserved.hoi_intermediates.as_ref()
    }

    /// Unicode scalar values attached to this glyph layer.
    pub fn codepoints(self) -> impl Iterator<Item = char> + 'a {
        self.preserved.codepoints.iter().copied()
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
    pub(super) path: &'a babelfont::Path,
    pub(super) preserved: &'a PreservedContour,
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

    /// Whether this contour uses Runebender's editable hyperbezier convention.
    pub fn is_hyper(self) -> bool {
        self.preserved.hyper
    }

    /// Copy this contour with its canonical geometry and exact source metadata.
    pub fn copied(self) -> CopiedContour {
        CopiedContour {
            path: self.path.clone(),
            preserved: self.preserved.clone(),
        }
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
    pub(super) node: &'a Node,
    pub(super) preserved: &'a PreservedPoint,
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
    pub(super) component: &'a Component,
    pub(super) preserved: &'a PreservedComponent,
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

    /// Whether this component is explicitly cut loose from automatic anchor alignment.
    pub fn alignment_disabled(self) -> bool {
        self.preserved.alignment.is_disabled()
    }
}

/// Read-only access to one canonical anchor.
#[derive(Clone, Copy, Debug)]
pub struct AnchorView<'a> {
    pub(super) anchor: &'a Anchor,
    pub(super) preserved: &'a PreservedAnchor,
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

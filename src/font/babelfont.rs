// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Babelfont geometry with lossless UFO persistence projections.
//!
//! Babelfont owns paths, anchors and components. UFO payloads retain metadata its
//! model cannot express, plus exact advances and affine coefficients. A projection
//! takes geometry from Babelfont, restoring exact numbers when their corresponding
//! Babelfont value is unchanged. Compilation is the only quantizing boundary.

mod curve_conversion;
pub(super) mod glyph_transactions;
mod handle_cleanup;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use babelfont::{Anchor, Component, Layer, Node, NodeType, Shape};
use kurbo::ParamCurve;

use super::model::glyph_metadata::{
    COMPOSITION_RECIPE_KEY, ComponentAlignment, LEFT_METRICS_KEY, MARK_COLOR_KEY, MARK_LABEL_KEY,
    METABALLS_KEY, MarkColor, Metaballs, MetricsFormula, RIGHT_METRICS_KEY, parse_metrics_key,
};
use super::model::hoi::HoiIntermediates;
use super::model::smart_components::{
    SmartComponentAxes, SmartComponentPole, SmartComponentValues,
};
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
            /// Opaque session-local wire identity; meaningful only within its document epoch.
            pub fn to_wire(self) -> String {
                self.0.to_string()
            }

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
    hyper: bool,
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
    alignment: ComponentAlignment,
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

/// One source image reference attached to a canonical glyph layer.
///
/// Image bytes remain source resources. This value owns only the UFO filename, optional RGBA
/// tint and exact affine placement without exposing a source-format model to editor state.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerImage {
    file_name: std::path::PathBuf,
    color: Option<[f64; 4]>,
    transform: kurbo::Affine,
}

impl LayerImage {
    /// Create a validated layer-image reference.
    pub fn new(
        file_name: std::path::PathBuf,
        color: Option<[f64; 4]>,
        transform: kurbo::Affine,
    ) -> Result<Self, DocumentEditError> {
        if file_name.as_os_str().is_empty()
            || file_name.is_absolute()
            || file_name
                .parent()
                .is_some_and(|parent| !parent.as_os_str().is_empty())
            || !transform.as_coeffs().iter().all(|value| value.is_finite())
            || color.is_some_and(|channels| {
                !channels
                    .iter()
                    .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
            })
        {
            return Err(DocumentEditError::InvalidLayerMetadata);
        }
        Ok(Self {
            file_name,
            color,
            transform,
        })
    }

    /// Base filename of the source image resource.
    pub fn file_name(&self) -> &std::path::Path {
        &self.file_name
    }

    /// Optional RGBA tint with channels in `0..=1`.
    pub fn color(&self) -> Option<[f64; 4]> {
        self.color
    }

    /// Exact image placement in font coordinates.
    pub fn transform(&self) -> kurbo::Affine {
        self.transform
    }

    fn from_ufo(image: &norad::Image) -> Self {
        let color = image.color.map(|color| {
            let (red, green, blue, alpha) = color.channels();
            [red, green, blue, alpha]
        });
        Self {
            file_name: image.file_name().to_owned(),
            color,
            transform: kurbo::Affine::new([
                image.transform.x_scale,
                image.transform.xy_scale,
                image.transform.yx_scale,
                image.transform.y_scale,
                image.transform.x_offset,
                image.transform.y_offset,
            ]),
        }
    }

    fn to_ufo(&self) -> norad::Image {
        let [x_scale, xy_scale, yx_scale, y_scale, x_offset, y_offset] = self.transform.as_coeffs();
        let color = self.color.map(|[red, green, blue, alpha]| {
            norad::Color::new(red, green, blue, alpha)
                .expect("canonical image colors are validated")
        });
        norad::Image::new(
            self.file_name.clone(),
            color,
            norad::AffineTransform {
                x_scale,
                xy_scale,
                yx_scale,
                y_scale,
                x_offset,
                y_offset,
            },
        )
        .expect("canonical image filenames are validated")
    }
}

/// Exact UFO values and object metadata that Babelfont cannot represent faithfully.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct LayerPreservation {
    name: String,
    width: f64,
    height: f64,
    codepoints: Vec<char>,
    note: Option<String>,
    guidelines: Vec<norad::Guideline>,
    image: Option<LayerImage>,
    lib: plist::Dictionary,
    mark_color: Option<plist::Value>,
    left_metrics_key: Option<plist::Value>,
    right_metrics_key: Option<plist::Value>,
    metaballs: Option<plist::Value>,
    composition_recipe: Option<plist::Value>,
    smart_component_axes: Option<SmartComponentAxes>,
    smart_component_values: Option<SmartComponentValues<ComponentId>>,
    smart_component_pole: Option<SmartComponentPole>,
    hoi_intermediates: Option<HoiIntermediates>,
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

/// Canonical contours decoded once at an explicit source-format boundary.
///
/// This opaque payload contains Babelfont paths plus exact object metadata. It contains no UFO
/// object and can be installed into a layer without a source-format round trip.
#[derive(Clone, Debug)]
pub struct ImportedContours {
    contours: Vec<CopiedContour>,
}

impl ImportedContours {
    /// Number of decoded contours.
    pub fn len(&self) -> usize {
        self.contours.len()
    }

    /// Whether the boundary payload contains no contours.
    pub fn is_empty(&self) -> bool {
        self.contours.is_empty()
    }

    pub(crate) fn from_ufo(contours: &[norad::Contour]) -> Result<Self, DocumentEditError> {
        validate_ufo_contours(contours)?;
        let (shapes, preserved, _) = decode_imported_contours(contours)?;
        let contours = shapes
            .into_iter()
            .zip(preserved)
            .map(|(shape, preserved)| {
                let Shape::Path(path) = shape else {
                    unreachable!("the UFO contour decoder creates only paths")
                };
                CopiedContour { path, preserved }
            })
            .collect();
        Ok(Self { contours })
    }

    fn matches_layer(&self, layer: &Layer, preserved: &LayerPreservation) -> bool {
        let current = layer.paths().collect::<Vec<_>>();
        current.len() == self.contours.len()
            && current
                .into_iter()
                .zip(&self.contours)
                .all(|(path, imported)| {
                    let Some(current_preserved) = preserved
                        .contours
                        .iter()
                        .find(|candidate| read_id(&path.format_specific) == Some(candidate.id.0))
                    else {
                        return false;
                    };
                    path.closed == imported.path.closed
                        && path.nodes.len() == imported.path.nodes.len()
                        && path.nodes.iter().zip(&imported.path.nodes).all(|(a, b)| {
                            a.x == b.x
                                && a.y == b.y
                                && a.nodetype == b.nodetype
                                && a.smooth == b.smooth
                        })
                        && current_preserved.hyper == imported.preserved.hyper
                        && current_preserved.metadata == imported.preserved.metadata
                        && current_preserved.points.len() == imported.preserved.points.len()
                        && current_preserved
                            .points
                            .iter()
                            .zip(&imported.preserved.points)
                            .all(|(a, b)| a.name == b.name && a.metadata == b.metadata)
                })
    }

    fn into_fresh_parts(self) -> (Vec<Shape>, Vec<PreservedContour>, PastedContours) {
        let mut shapes = Vec::with_capacity(self.contours.len());
        let mut preserved = Vec::with_capacity(self.contours.len());
        let mut inserted = PastedContours::default();
        for contour in self.contours {
            debug_assert_eq!(
                contour.path.nodes.len(),
                contour.preserved.points.len(),
                "imported contour geometry and preservation records stay aligned"
            );
            let mut path = contour.path;
            let mut contour_preserved = contour.preserved;
            let contour_id = ContourId::next();
            write_id(&mut path.format_specific, contour_id.0);
            contour_preserved.id = contour_id;
            inserted.contours.push(contour_id);
            for (node, point) in path.nodes.iter_mut().zip(&mut contour_preserved.points) {
                let point_id = PointId::next();
                write_id(&mut node.format_specific, point_id.0);
                point.id = point_id;
                inserted.points.push(point_id);
            }
            shapes.push(Shape::Path(path));
            preserved.push(contour_preserved);
        }
        (shapes, preserved, inserted)
    }
}

/// Opaque owned state of one canonical glyph layer.
///
/// The snapshot contains Babelfont geometry plus every exact-value and source-metadata extension.
/// It does not contain or materialize a UFO glyph.
#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalLayerSnapshot {
    address: super::variable::GlyphLayerAddress,
    layer: Layer,
    preserved: LayerPreservation,
}

impl CanonicalLayerSnapshot {
    pub(super) fn new(
        address: super::variable::GlyphLayerAddress,
        layer: Layer,
        preserved: LayerPreservation,
    ) -> Self {
        Self {
            address,
            layer,
            preserved,
        }
    }

    /// Stable address this layer was captured from.
    pub fn address(&self) -> &super::variable::GlyphLayerAddress {
        &self.address
    }

    /// The exact horizontal advance retained by this snapshot.
    pub(crate) fn width(&self) -> f64 {
        self.preserved.width
    }

    /// Position of one stable point in this snapshot.
    pub(crate) fn point_position(&self, id: PointId) -> Option<kurbo::Point> {
        LayerView::new(&self.layer, &self.preserved)
            .contours()
            .flat_map(ContourView::points)
            .find(|point| point.id() == id)
            .map(PointView::position)
    }

    /// Position of one stable anchor in this snapshot.
    pub(crate) fn anchor_position(&self, id: AnchorId) -> Option<kurbo::Point> {
        LayerView::new(&self.layer, &self.preserved)
            .anchors()
            .find(|anchor| anchor.id() == id)
            .map(AnchorView::position)
    }

    pub(super) fn delta_from(&self, previous: &Self) -> Option<LayerDelta> {
        if self.address != previous.address {
            return None;
        }
        Some(
            LayerEditDraft::new(self.layer.clone(), self.preserved.clone())
                .delta_from(&previous.layer, &previous.preserved),
        )
    }

    /// Rebind this snapshot during one explicitly authorized glyph rename.
    ///
    /// Both the complete current address and unchanged layer identity must match.
    /// Arbitrary cross-layer or stale-address rebinding is rejected without mutation.
    pub(crate) fn rebind_glyph(
        &mut self,
        old: &super::variable::GlyphLayerAddress,
        new: &super::variable::GlyphLayerAddress,
    ) -> bool {
        if &self.address != old || old.layer != new.layer {
            return false;
        }
        self.address = new.clone();
        self.preserved.name.clone_from(&new.glyph);
        true
    }

    pub(super) fn into_parts(self) -> (Layer, LayerPreservation) {
        (self.layer, self.preserved)
    }
}

impl CopiedContour {
    /// Return a transformed copy rounded to integer font units.
    ///
    /// Source metadata remains attached to each object. Returns `None` if the transform produces
    /// a nonfinite coordinate.
    pub fn transformed_rounded(&self, transform: kurbo::Affine) -> Option<Self> {
        let mut copied = self.clone();
        for node in &mut copied.path.nodes {
            let point = transform * kurbo::Point::new(node.x, node.y);
            if !point.x.is_finite() || !point.y.is_finite() {
                return None;
            }
            node.x = point.x.round();
            node.y = point.y.round();
        }
        Some(copied)
    }
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

    pub(super) fn codec_parts(self) -> (&'a Layer, &'a LayerPreservation) {
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

    /// Whether this component is explicitly cut loose from automatic anchor alignment.
    pub fn alignment_disabled(self) -> bool {
        self.preserved.alignment.is_disabled()
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

    fn validate_imported_contours(
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

    /// Remove duplicate zero-length line endpoints while retaining every surviving object.
    ///
    /// Returns the number of removed points.
    pub fn tidy_contours(&mut self) -> usize {
        let mut removed = 0_usize;
        for shape in &mut self.layer.shapes {
            let Shape::Path(path) = shape else {
                continue;
            };
            let contour_id =
                ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
            let preserved = self
                .preserved
                .contours
                .iter_mut()
                .find(|candidate| candidate.id == contour_id)
                .expect("canonical contour preservation");
            let mut index = 1;
            while index < path.nodes.len() {
                let previous = &path.nodes[index - 1];
                let point = &path.nodes[index];
                let duplicate = point.nodetype == NodeType::Line
                    && previous.nodetype != NodeType::OffCurve
                    && (point.x - previous.x).abs() < 0.01
                    && (point.y - previous.y).abs() < 0.01;
                if duplicate {
                    path.nodes.remove(index);
                    preserved.points.remove(index);
                    removed += 1;
                } else {
                    index += 1;
                }
            }
            if path.closed && path.nodes.len() > 2 {
                let first = &path.nodes[0];
                let last = path.nodes.last().expect("closed contour has nodes");
                if last.nodetype == NodeType::Line
                    && first.nodetype != NodeType::OffCurve
                    && (last.x - first.x).abs() < 0.01
                    && (last.y - first.y).abs() < 0.01
                {
                    path.nodes.pop();
                    preserved.points.pop();
                    removed += 1;
                }
            }
        }
        removed
    }

    /// Round every canonical contour point to integer coordinates.
    ///
    /// Stable identities and source metadata remain attached to their points. Returns the number
    /// of points that moved.
    pub fn round_coordinates(&mut self) -> usize {
        let mut moved = 0_usize;
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
            let rounded = (node.x.round(), node.y.round());
            if (node.x, node.y) != rounded {
                node.x = rounded.0;
                node.y = rounded.1;
                moved += 1;
            }
        }
        moved
    }

    /// Rewind canonical contours to counterclockwise outers and clockwise holes.
    ///
    /// Contours and points retain their stable identities and source metadata. Returns the number
    /// of contours reversed.
    pub fn correct_path_directions(&mut self) -> Result<usize, DocumentEditError> {
        use kurbo::Shape as _;

        let layer = self.view();
        let contours: Vec<_> = layer.contours().collect();
        let paths: Vec<_> = contours
            .iter()
            .map(|contour| crate::outline::glyph_paths::ordinary_contour_to_bezpath(*contour))
            .collect();
        let mut selected = Vec::new();
        for (index, contour) in contours.iter().enumerate() {
            let Some(probe) = contour
                .points()
                .find(|point| point.point_type() != LayerPointType::OffCurve)
            else {
                continue;
            };
            let depth = paths
                .iter()
                .enumerate()
                .filter(|(other, path)| *other != index && path.contains(probe.position()))
                .count();
            let area = paths[index].area();
            let wants_counterclockwise = depth % 2 == 0;
            if (wants_counterclockwise && area < 0.0) || (!wants_counterclockwise && area > 0.0) {
                selected.push(probe.id());
            }
        }
        if selected.is_empty() {
            return Ok(0);
        }
        self.reverse_contours(&selected)?;
        Ok(selected.len())
    }

    /// Scale selected cubic handles to a fraction of their tangent-intersection maximum.
    ///
    /// An empty selection fits every cubic segment. Existing point identities and source metadata
    /// remain attached to moved controls. Returns whether any control moved.
    pub fn fit_curve_handles(
        &mut self,
        selected: &[PointId],
        fraction: f64,
    ) -> Result<bool, DocumentEditError> {
        if !(0.01..=1.5).contains(&fraction) {
            return Ok(false);
        }
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().map(|id| id.0).collect();
        let fit_all = selected.is_empty();
        let cross =
            |first: kurbo::Vec2, second: kurbo::Vec2| first.x * second.y - first.y * second.x;
        let mut replacements = HashMap::new();
        for path in self.layer.paths() {
            let count = path.nodes.len();
            if count < 4 {
                continue;
            }
            for end in 0..count {
                if path.nodes[end].nodetype != NodeType::Curve {
                    continue;
                }
                let second_control = (end + count - 1) % count;
                let first_control = (end + count - 2) % count;
                let start = (end + count - 3) % count;
                if path.nodes[first_control].nodetype != NodeType::OffCurve
                    || path.nodes[second_control].nodetype != NodeType::OffCurve
                    || path.nodes[start].nodetype == NodeType::OffCurve
                {
                    continue;
                }
                if !fit_all
                    && ![start, first_control, second_control, end]
                        .iter()
                        .any(|index| {
                            read_id(&path.nodes[*index].format_specific)
                                .is_some_and(|id| selected.contains(&id))
                        })
                {
                    continue;
                }
                let start_point = kurbo::Point::new(path.nodes[start].x, path.nodes[start].y);
                let first_point =
                    kurbo::Point::new(path.nodes[first_control].x, path.nodes[first_control].y);
                let second_point =
                    kurbo::Point::new(path.nodes[second_control].x, path.nodes[second_control].y);
                let end_point = kurbo::Point::new(path.nodes[end].x, path.nodes[end].y);
                let first_direction = first_point - start_point;
                let second_direction = second_point - end_point;
                if first_direction.hypot() < 1e-9 || second_direction.hypot() < 1e-9 {
                    continue;
                }
                let first_direction = first_direction / first_direction.hypot();
                let second_direction = second_direction / second_direction.hypot();
                let denominator = cross(first_direction, second_direction);
                if denominator.abs() < 1e-9 {
                    continue;
                }
                let between = end_point - start_point;
                let first_maximum = cross(between, second_direction) / denominator;
                let second_maximum = cross(between, first_direction) / denominator;
                if first_maximum <= 0.0 || second_maximum <= 0.0 {
                    continue;
                }
                let first = start_point + first_direction * (first_maximum * fraction);
                let second = end_point + second_direction * (second_maximum * fraction);
                let first = kurbo::Point::new(first.x.round(), first.y.round());
                let second = kurbo::Point::new(second.x.round(), second.y.round());
                ensure_finite(&[first.x, first.y, second.x, second.y])?;
                if first != first_point {
                    replacements.insert(
                        read_id(&path.nodes[first_control].format_specific)
                            .expect("canonical point identity"),
                        first,
                    );
                }
                if second != second_point {
                    replacements.insert(
                        read_id(&path.nodes[second_control].format_specific)
                            .expect("canonical point identity"),
                        second,
                    );
                }
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

    /// Insert on-curve points at selected cubic extrema.
    ///
    /// An empty selection considers every ordinary cubic segment. Hyperbezier contours remain on
    /// their editable source path. Returns whether any point was inserted.
    pub fn add_extreme_points(&mut self, selected: &[PointId]) -> Result<bool, DocumentEditError> {
        use kurbo::ParamCurveExtrema as _;

        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().copied().collect();
        let mut staged = self.clone();
        let mut changed = false;
        for _ in 0..300 {
            let candidate = crate::outline::segment_ops::ordinary_layer_segments(staged.view())
                .into_iter()
                .find_map(|segment| {
                    let kurbo::PathSeg::Cubic(cubic) = segment.seg else {
                        return None;
                    };
                    if !selected.is_empty()
                        && !segment.point_ids().iter().any(|id| selected.contains(id))
                    {
                        return None;
                    }
                    let parameter = cubic
                        .extrema()
                        .into_iter()
                        .find(|parameter| (0.02..=0.98).contains(parameter))?;
                    let (
                        DocumentSegmentEndpoint::Point(start),
                        DocumentSegmentEndpoint::Point(end),
                    ) = (segment.start, segment.end)
                    else {
                        return None;
                    };
                    Some((start, end, parameter))
                });
            let Some((start, end, parameter)) = candidate else {
                break;
            };
            staged.insert_point_on_segment(start, end, parameter)?;
            changed = true;
        }
        if changed {
            *self = staged;
        }
        Ok(changed)
    }

    /// Push every canonical contour point along its anisotropic outward normal.
    ///
    /// Point order, roles, stable identities and source metadata remain unchanged. Returns whether
    /// any point moved.
    pub fn embolden(
        &mut self,
        offset: crate::outline::embolden::Offset,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[offset.x, offset.y])?;
        if offset.x == 0.0 && offset.y == 0.0 {
            return Ok(false);
        }
        let mut replacements = HashMap::new();
        for path in self.layer.paths() {
            let positions: Vec<_> = path
                .nodes
                .iter()
                .map(|node| kurbo::Point::new(node.x, node.y))
                .collect();
            for (node, (normal_x, normal_y)) in
                path.nodes
                    .iter()
                    .zip(crate::outline::embolden::outward_normals_for_points(
                        &positions,
                    ))
            {
                let position =
                    kurbo::Point::new(node.x + normal_x * offset.x, node.y + normal_y * offset.y);
                ensure_finite(&[position.x, position.y])?;
                if position != kurbo::Point::new(node.x, node.y) {
                    replacements.insert(
                        read_id(&node.format_specific).expect("canonical point identity"),
                        position,
                    );
                }
            }
        }
        self.apply_point_replacements(&replacements)
    }

    /// Apply model-predicted integer point deltas in outline-reader order.
    ///
    /// The extra closing delta after each contour is consumed to match the model's reader. Point
    /// order, roles, stable identities and source metadata remain unchanged. Returns whether any
    /// point moved.
    pub fn apply_bolden_deltas(
        &mut self,
        deltas: &[(i32, i32)],
        center: (i32, i32),
    ) -> Result<bool, DocumentEditError> {
        let mut next = deltas.iter();
        let mut replacements = HashMap::new();
        for path in self.layer.paths() {
            let count = path.nodes.len();
            let start = path
                .nodes
                .iter()
                .position(|node| node.nodetype != NodeType::OffCurve)
                .unwrap_or(0);
            for step in 0..count {
                let Some((delta_x, delta_y)) = next.next().copied() else {
                    break;
                };
                let index = (start + step) % count;
                let node = &path.nodes[index];
                let position = kurbo::Point::new(
                    node.x + f64::from(delta_x) + f64::from(center.0),
                    node.y + f64::from(delta_y) + f64::from(center.1),
                );
                ensure_finite(&[position.x, position.y])?;
                if position != kurbo::Point::new(node.x, node.y) {
                    replacements.insert(
                        read_id(&node.format_specific).expect("canonical point identity"),
                        position,
                    );
                }
            }
            next.next();
        }
        self.apply_point_replacements(&replacements)
    }

    /// Replace every component with pre-resolved canonical contours.
    ///
    /// Existing contours and anchors retain their identities and exact metadata. Decomposed
    /// contours preserve source names and libraries while receiving fresh document and UFO
    /// identities. Returns whether components were replaced.
    pub fn decompose_components(
        &mut self,
        resolved: &[CopiedContour],
    ) -> Result<bool, DocumentEditError> {
        if self.layer.components().next().is_none() {
            return Ok(false);
        }
        self.paste_contours(resolved)?;
        self.layer
            .shapes
            .retain(|shape| matches!(shape, Shape::Path(_)));
        self.preserved.components.clear();
        Ok(true)
    }

    fn apply_point_replacements(
        &mut self,
        replacements: &HashMap<u64, kurbo::Point>,
    ) -> Result<bool, DocumentEditError> {
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

    fn selected_contour_ids(
        &self,
        selected: &[PointId],
    ) -> Result<HashSet<ContourId>, DocumentEditError> {
        for id in selected {
            if self.node(*id).is_none() {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        if selected.is_empty() {
            return Ok(self.view().contours().map(ContourView::id).collect());
        }
        let selected: HashSet<_> = selected.iter().copied().collect();
        Ok(self
            .view()
            .contours()
            .filter(|contour| contour.points().any(|point| selected.contains(&point.id())))
            .map(ContourView::id)
            .collect())
    }

    /// Replace selected contours with stroked outlines.
    ///
    /// An empty selection targets every contour. Replaced contours receive fresh identities and
    /// empty source metadata; untargeted contours, components and anchors remain unchanged.
    pub fn expand_stroke(
        &mut self,
        selected: &[PointId],
        width: f64,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[width])?;
        if width <= 0.0 {
            return Ok(false);
        }
        let selected = self.selected_contour_ids(selected)?;
        let replacements: HashMap<_, _> = self
            .view()
            .contours()
            .filter(|contour| selected.contains(&contour.id()))
            .filter_map(|contour| {
                let path = crate::outline::path::Path::from_document_contour(contour).to_bezpath();
                let paths = crate::outline::effects::expanded_stroke_paths(&path, width);
                (!paths.is_empty()).then_some((contour.id(), paths))
            })
            .collect();
        self.replace_selected_contours_with_paths(&replacements)
    }

    /// Offset every canonical contour outward or inward.
    ///
    /// All output contours receive fresh identities and empty source metadata. Components and
    /// anchors remain unchanged. A successful empty result removes every contour.
    pub fn offset_contours(&mut self, delta: f64) -> Result<bool, DocumentEditError> {
        ensure_finite(&[delta])?;
        let paths: Vec<_> = self
            .view()
            .contours()
            .map(|contour| crate::outline::path::Path::from_document_contour(contour).to_bezpath())
            .collect();
        let Some(paths) = crate::outline::effects::offset_paths(&paths, delta) else {
            return Ok(false);
        };
        self.replace_contours_with_paths(&paths)
    }

    /// Extrude every canonical contour along an angle.
    ///
    /// All output contours receive fresh identities and empty source metadata. Components and
    /// anchors remain unchanged. A successful empty result removes every contour.
    pub fn extrude_contours(
        &mut self,
        offset: f64,
        angle_degrees: f64,
        keep_front: bool,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[offset, angle_degrees])?;
        let paths: Vec<_> = self
            .view()
            .contours()
            .map(|contour| crate::outline::path::Path::from_document_contour(contour).to_bezpath())
            .collect();
        let Some(paths) =
            crate::outline::effects::extruded_paths(&paths, offset, angle_degrees, keep_front)
        else {
            return Ok(false);
        };
        self.replace_contours_with_paths(&paths)
    }

    /// Flatten and jitter selected canonical contours deterministically.
    ///
    /// An empty selection targets every contour. Replaced contours receive fresh identities and
    /// empty source metadata; untargeted contours, components and anchors remain unchanged.
    pub fn roughen_contours(
        &mut self,
        selected: &[PointId],
        segment_length: f64,
        horizontal: f64,
        vertical: f64,
        seed: u64,
    ) -> Result<bool, DocumentEditError> {
        ensure_finite(&[segment_length, horizontal, vertical])?;
        if segment_length < 1.0 {
            return Ok(false);
        }
        let selected = self.selected_contour_ids(selected)?;
        let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let mut replacements = HashMap::new();
        for contour in self.view().contours() {
            if !selected.contains(&contour.id()) {
                continue;
            }
            let path = crate::outline::path::Path::from_document_contour(contour).to_bezpath();
            if let Some(path) = crate::outline::effects::roughened_path(
                &path,
                segment_length,
                horizontal,
                vertical,
                &mut state,
            ) {
                replacements.insert(contour.id(), vec![path]);
            }
        }
        self.replace_selected_contours_with_paths(&replacements)
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

    /// Permanently subtract contours marked as masks from the other canonical contours.
    ///
    /// Mask indices are decoded only at this explicit UFO-key boundary. Successful replacement
    /// clears the key and assigns fresh identities and empty source metadata to the result.
    pub fn bake_masks(&mut self) -> Result<bool, DocumentEditError> {
        let Some(values) = self
            .preserved
            .lib
            .get(crate::formats::lib_keys::MASKS_KEY)
            .and_then(plist::Value::as_array)
        else {
            return Ok(false);
        };
        let contour_count = self.view().contours().count();
        let masks: HashSet<_> = values
            .iter()
            .filter_map(plist::Value::as_unsigned_integer)
            .filter_map(|value| usize::try_from(value).ok())
            .filter(|index| *index < contour_count)
            .collect();
        if masks.is_empty() || masks.len() == contour_count {
            return Ok(false);
        }
        let mut keep = kurbo::BezPath::new();
        let mut cut = kurbo::BezPath::new();
        for (index, contour) in self.view().contours().enumerate() {
            let path = crate::outline::path::Path::from_document_contour(contour).to_bezpath();
            let destination = if masks.contains(&index) {
                &mut cut
            } else {
                &mut keep
            };
            destination.extend(path.elements().iter().copied());
        }
        let Ok(result) = linesweeper::binary_op(
            &keep,
            &cut,
            linesweeper::FillRule::NonZero,
            linesweeper::BinaryOp::Difference,
        ) else {
            return Ok(false);
        };
        let paths = result
            .contours()
            .map(|contour| contour.path.clone())
            .collect::<Vec<_>>();
        let changed = self.replace_contours_with_paths(&paths)?;
        if changed {
            self.preserved
                .lib
                .remove(crate::formats::lib_keys::MASKS_KEY);
        }
        Ok(changed)
    }

    /// Cut canonical contours along the line from `p0` to `p1`.
    ///
    /// Missed contours retain their stable identities and exact source metadata. Every contour
    /// whose topology changes receives fresh identities and empty source metadata. A sliced
    /// hyperbezier becomes explicit cubic geometry, matching the existing knife behavior.
    /// Returns whether any topology changed.
    pub fn knife_cut(
        &mut self,
        p0: kurbo::Point,
        p1: kurbo::Point,
    ) -> Result<bool, DocumentEditError> {
        let originals: Vec<_> = self
            .view()
            .contours()
            .map(|contour| {
                let path = crate::outline::path::Path::from_document_contour(contour);
                (path.entity_id(), contour.id(), path)
            })
            .collect();
        if originals.is_empty() {
            return Ok(false);
        }
        let input: Vec<_> = originals.iter().map(|(_, _, path)| path.clone()).collect();
        let sliced = crate::outline::knife::slice_paths(&input, kurbo::Line::new(p0, p1));
        if sliced.len() == input.len()
            && sliced.iter().all(|path| {
                originals
                    .iter()
                    .any(|(entity_id, _, _)| *entity_id == path.entity_id())
            })
        {
            return Ok(false);
        }
        self.replace_contours_after_knife(&sliced, &originals)
    }

    /// Convert selected editable hyperbezier contours to explicit cubic topology.
    ///
    /// An empty selection converts every hyperbezier contour. Converted topology receives fresh
    /// identities and empty source metadata, while ordinary and unselected contours remain exact.
    pub fn convert_hyper_to_cubic(
        &mut self,
        selected: &[PointId],
    ) -> Result<bool, DocumentEditError> {
        for id in selected {
            if !self.layer.paths().flat_map(|path| &path.nodes).any(|node| {
                read_id(&node.format_specific).is_some_and(|candidate| candidate == id.0)
            }) {
                return Err(DocumentEditError::MissingPoint(*id));
            }
        }
        let selected: HashSet<_> = selected.iter().copied().collect();
        let convert_all = selected.is_empty();
        let replacement_paths = self
            .view()
            .contours()
            .filter(|contour| {
                contour.is_hyper()
                    && (convert_all || contour.points().any(|point| selected.contains(&point.id())))
            })
            .map(|contour| {
                let path = crate::outline::path::Path::from_document_contour(contour);
                let crate::outline::path::Path::Hyper(hyper) = path else {
                    unreachable!("canonical hyperbezier contour produces a hyper path")
                };
                (
                    contour.id(),
                    crate::outline::path::Path::Cubic(hyper.to_cubic()),
                )
            })
            .collect::<Vec<_>>();
        if replacement_paths.is_empty() {
            return Ok(false);
        }
        let replacements = replacement_paths
            .iter()
            .map(|(id, path)| Ok((*id, Self::replacement_contour_from_outline_path(path)?)))
            .collect::<Result<HashMap<_, _>, DocumentEditError>>()?;
        let mut shapes = Vec::with_capacity(self.layer.shapes.len());
        let mut preserved = Vec::with_capacity(self.preserved.contours.len());
        for shape in &self.layer.shapes {
            let Shape::Path(path) = shape else {
                shapes.push(shape.clone());
                continue;
            };
            let contour_id =
                ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
            if let Some((replacement, metadata)) = replacements.get(&contour_id) {
                shapes.push(replacement.clone());
                preserved.push(metadata.clone());
            } else {
                shapes.push(shape.clone());
                preserved.push(
                    self.preserved
                        .contours
                        .iter()
                        .find(|candidate| candidate.id == contour_id)
                        .expect("canonical contour preservation")
                        .clone(),
                );
            }
        }
        self.layer.shapes = shapes;
        self.preserved.contours = preserved;
        Ok(true)
    }

    fn replacement_contour_from_outline_path(
        path: &crate::outline::path::Path,
    ) -> Result<(Shape, PreservedContour), DocumentEditError> {
        let contour = path.to_contour();
        ensure_finite(
            &contour
                .points
                .iter()
                .flat_map(|point| [point.x, point.y])
                .collect::<Vec<_>>(),
        )?;
        let contour_id = ContourId::next();
        let mut output = babelfont::Path {
            closed: path.is_closed(),
            ..babelfont::Path::default()
        };
        write_id(&mut output.format_specific, contour_id.0);
        let mut points = Vec::with_capacity(contour.points.len());
        output.nodes = contour
            .points
            .iter()
            .map(|point| {
                let point_id = PointId::next();
                points.push(PreservedPoint {
                    id: point_id,
                    name: None,
                    metadata: ObjectMetadata {
                        identifier: None,
                        lib: None,
                    },
                });
                let mut node = Node {
                    x: point.x,
                    y: point.y,
                    nodetype: match point.point_type {
                        crate::outline::path::hyper_model::PointType::Move => NodeType::Move,
                        crate::outline::path::hyper_model::PointType::Line
                        | crate::outline::path::hyper_model::PointType::HyperCorner => {
                            NodeType::Line
                        }
                        crate::outline::path::hyper_model::PointType::OffCurve => {
                            NodeType::OffCurve
                        }
                        crate::outline::path::hyper_model::PointType::Curve
                        | crate::outline::path::hyper_model::PointType::Hyper => NodeType::Curve,
                        crate::outline::path::hyper_model::PointType::QCurve => NodeType::QCurve,
                    },
                    smooth: point.smooth,
                    ..Node::default()
                };
                write_id(&mut node.format_specific, point_id.0);
                node
            })
            .collect();
        Ok((
            Shape::Path(output),
            PreservedContour {
                id: contour_id,
                hyper: false,
                metadata: ObjectMetadata {
                    identifier: None,
                    lib: None,
                },
                points,
            },
        ))
    }

    fn replace_contours_after_knife(
        &mut self,
        paths: &[crate::outline::path::Path],
        originals: &[(
            crate::font::model::entity_id::EntityId,
            ContourId,
            crate::outline::path::Path,
        )],
    ) -> Result<bool, DocumentEditError> {
        let mut replacements = Vec::with_capacity(paths.len());
        let mut preserved = Vec::with_capacity(paths.len());
        for path in paths {
            if let Some((_, contour_id, _)) = originals
                .iter()
                .find(|(entity_id, _, _)| *entity_id == path.entity_id())
            {
                let source_index = self
                    .preserved
                    .contours
                    .iter()
                    .position(|candidate| candidate.id == *contour_id)
                    .expect("knife source contour preservation");
                let source_path = self
                    .layer
                    .shapes
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Path(path) => Some(path),
                        Shape::Component(_) => None,
                    })
                    .find(|path| read_id(&path.format_specific) == Some(contour_id.0))
                    .expect("knife source canonical contour");
                replacements.push(Shape::Path(source_path.clone()));
                preserved.push(self.preserved.contours[source_index].clone());
                continue;
            }

            let (shape, metadata) = Self::replacement_contour_from_outline_path(path)?;
            replacements.push(shape);
            preserved.push(metadata);
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

    fn replacement_contour_from_path(
        path: &kurbo::BezPath,
        smooth_at: &HashMap<(i64, i64), bool>,
    ) -> Result<Option<(Shape, PreservedContour)>, DocumentEditError> {
        let mut path = babelfont::Path::from(path.clone());
        ensure_finite(
            &path
                .nodes
                .iter()
                .flat_map(|node| [node.x, node.y])
                .collect::<Vec<_>>(),
        )?;
        let on_curve_count = path
            .nodes
            .iter()
            .filter(|node| node.nodetype != NodeType::OffCurve)
            .count();
        if on_curve_count < if path.closed { 1 } else { 2 } {
            return Ok(None);
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
        Ok(Some((
            Shape::Path(path),
            PreservedContour {
                id: contour_id,
                hyper: false,
                metadata: ObjectMetadata {
                    identifier: None,
                    lib: None,
                },
                points,
            },
        )))
    }

    fn replace_selected_contours_with_paths(
        &mut self,
        replacements: &HashMap<ContourId, Vec<kurbo::BezPath>>,
    ) -> Result<bool, DocumentEditError> {
        if replacements.is_empty() {
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
        let mut shapes = Vec::new();
        let mut preserved = Vec::new();
        for shape in &self.layer.shapes {
            let Shape::Path(path) = shape else {
                shapes.push(shape.clone());
                continue;
            };
            let contour_id =
                ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
            let Some(paths) = replacements.get(&contour_id) else {
                shapes.push(shape.clone());
                preserved.push(
                    self.preserved
                        .contours
                        .iter()
                        .find(|candidate| candidate.id == contour_id)
                        .expect("canonical contour preservation")
                        .clone(),
                );
                continue;
            };
            for path in paths {
                if let Some((shape, metadata)) =
                    Self::replacement_contour_from_path(path, &smooth_at)?
                {
                    shapes.push(shape);
                    preserved.push(metadata);
                }
            }
        }
        self.layer.shapes = shapes;
        self.preserved.contours = preserved;
        Ok(true)
    }

    fn replace_contours_with_paths(
        &mut self,
        paths: &[kurbo::BezPath],
    ) -> Result<bool, DocumentEditError> {
        let had_contours = self.layer.paths().next().is_some();
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
            if let Some((shape, metadata)) = Self::replacement_contour_from_path(path, &smooth_at)?
            {
                replacements.push(shape);
                preserved.push(metadata);
            }
        }
        let changed = had_contours || !replacements.is_empty();
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
        Ok(changed)
    }

    /// Replace only the contours and exact width from another canonical layer.
    pub(super) fn replace_layer_contours(
        &mut self,
        source: LayerView<'_>,
    ) -> Result<bool, DocumentEditError> {
        let contours_changed = self.replace_layer_contours_only(source)?;
        let width_changed = self.set_width(source.width())?;
        Ok(contours_changed || width_changed)
    }

    /// Replace only the contours from another canonical layer, retaining this layer's width.
    pub(super) fn replace_layer_contours_only(
        &mut self,
        source: LayerView<'_>,
    ) -> Result<bool, DocumentEditError> {
        if source.glyph_name() != self.preserved.name {
            return Err(DocumentEditError::InvalidLayerMetadata);
        }
        let current = self.view();
        let same_contours = current.contours().count() == source.contours().count()
            && current.contours().zip(source.contours()).all(|(a, b)| {
                a.is_closed() == b.is_closed()
                    && a.is_hyper() == b.is_hyper()
                    && same_copied_object_metadata(&a.preserved.metadata, &b.preserved.metadata)
                    && a.points().count() == b.points().count()
                    && a.points().zip(b.points()).all(|(a, b)| {
                        a.position() == b.position()
                            && a.point_type() == b.point_type()
                            && a.is_smooth() == b.is_smooth()
                            && a.name() == b.name()
                            && same_copied_object_metadata(
                                &a.preserved.metadata,
                                &b.preserved.metadata,
                            )
                    })
            });
        if same_contours {
            return Ok(false);
        }
        let copied = source.copy_contours(&[])?;
        let old_shape_count = self.layer.shapes.len();
        let old_contour_count = self.preserved.contours.len();
        self.paste_contours(&copied)?;
        let replacements = self.layer.shapes.split_off(old_shape_count);
        replace_path_shapes_preserving_slots(&mut self.layer.shapes, replacements);
        let replacements = self.preserved.contours.split_off(old_contour_count);
        self.preserved.contours = replacements;
        Ok(true)
    }

    /// Replace only the contours with one checked canonical interpolation result.
    ///
    /// The replacement receives fresh contour and point identities because its topology comes
    /// from other source layers. Components, anchors and every non-contour layer field remain
    /// untouched.
    pub(super) fn replace_interpolated_contours(
        &mut self,
        output: &super::interpolation::InterpolatedLayer,
    ) -> Result<bool, DocumentEditError> {
        if output.glyph_name != self.preserved.name {
            return Err(DocumentEditError::InvalidLayerMetadata);
        }
        let current = self.view();
        let same_contours = current.contours().count() == output.contours().count()
            && current
                .contours()
                .zip(output.contours())
                .all(|(source, replacement)| {
                    source.is_closed() == replacement.closed
                        && source.is_hyper() == replacement.hyper
                        && source.points().count() == replacement.points.len()
                        && source.points().zip(&replacement.points).all(|(a, b)| {
                            a.position() == b.position
                                && a.point_type() == b.point_type
                                && a.is_smooth() == b.smooth
                                && a.name() == b.name.as_deref()
                        })
                });
        if same_contours && self.preserved.width == output.width {
            return Ok(false);
        }
        let mut replacements = Vec::with_capacity(output.contours().count());
        let mut preserved = Vec::with_capacity(output.contours().count());
        for contour in output.contours() {
            let contour_id = ContourId::next();
            let mut path = babelfont::Path {
                closed: contour.closed,
                ..babelfont::Path::default()
            };
            write_id(&mut path.format_specific, contour_id.0);
            let mut points = Vec::with_capacity(contour.points.len());
            for point in &contour.points {
                ensure_finite(&[point.position.x, point.position.y])?;
                let point_id = PointId::next();
                let mut node = Node {
                    x: point.position.x,
                    y: point.position.y,
                    nodetype: match point.point_type {
                        LayerPointType::Move => NodeType::Move,
                        LayerPointType::Line => NodeType::Line,
                        LayerPointType::OffCurve => NodeType::OffCurve,
                        LayerPointType::Curve => NodeType::Curve,
                        LayerPointType::QCurve => NodeType::QCurve,
                    },
                    smooth: point.smooth,
                    ..Node::default()
                };
                write_id(&mut node.format_specific, point_id.0);
                path.nodes.push(node);
                points.push(PreservedPoint {
                    id: point_id,
                    name: point
                        .name
                        .as_deref()
                        .and_then(|name| norad::Name::new(name).ok()),
                    metadata: ObjectMetadata {
                        identifier: None,
                        lib: None,
                    },
                });
            }
            replacements.push(Shape::Path(path));
            preserved.push(PreservedContour {
                id: contour_id,
                hyper: contour.hyper,
                metadata: ObjectMetadata {
                    identifier: contour.hyper.then(fresh_hyper_identifier),
                    lib: None,
                },
                points,
            });
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
        self.set_width(output.width)?;
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

    /// Replace one component's referenced glyph by stable identity.
    pub fn set_component_reference(
        &mut self,
        id: ComponentId,
        reference: &str,
    ) -> Result<bool, DocumentEditError> {
        norad::Name::new(reference).map_err(|_| DocumentEditError::InvalidLayerMetadata)?;
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
            .ok_or(DocumentEditError::MissingComponent(id))?;
        if component.reference.as_str() == reference {
            return Ok(false);
        }
        component.reference = reference.into();
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

    /// Append a component with a fresh stable identity and exact affine transform.
    ///
    /// The Project or application boundary is responsible for validating that `reference` names
    /// a glyph in the current document.
    pub fn add_component(
        &mut self,
        reference: String,
        transform: kurbo::Affine,
    ) -> Result<ComponentId, DocumentEditError> {
        let coefficients = transform.as_coeffs();
        ensure_finite(&coefficients)?;
        let id = ComponentId::next();
        let mut component = Component {
            reference: reference.into(),
            transform: transform.into(),
            location: std::iter::empty().collect(),
            format_specific: babelfont::FormatSpecific::default(),
        };
        write_id(&mut component.format_specific, id.0);
        self.layer.shapes.push(Shape::Component(component));
        self.preserved.components.push(PreservedComponent {
            id,
            transform: norad::AffineTransform {
                x_scale: coefficients[0],
                xy_scale: coefficients[1],
                yx_scale: coefficients[2],
                y_scale: coefficients[3],
                x_offset: coefficients[4],
                y_offset: coefficients[5],
            },
            alignment: ComponentAlignment::default(),
            metadata: ObjectMetadata {
                identifier: None,
                lib: None,
            },
        });
        Ok(id)
    }

    /// Read whether one stable component is cut loose from automatic anchor alignment.
    pub fn component_alignment_disabled(&self, id: ComponentId) -> Result<bool, DocumentEditError> {
        self.preserved
            .components
            .iter()
            .find(|component| component.id == id)
            .map(|component| component.alignment.is_disabled())
            .ok_or(DocumentEditError::MissingComponent(id))
    }

    /// Change automatic anchor alignment for one stable component.
    pub fn set_component_alignment_disabled(
        &mut self,
        id: ComponentId,
        disabled: bool,
    ) -> Result<bool, DocumentEditError> {
        let component = self
            .preserved
            .components
            .iter_mut()
            .find(|component| component.id == id)
            .ok_or(DocumentEditError::MissingComponent(id))?;
        let changed = component.alignment.set_disabled(disabled);
        if changed && component.metadata.identifier.is_none() {
            component.metadata.identifier = Some(norad::Identifier::from_uuidv4());
        }
        Ok(changed)
    }

    /// Set or remove one smart-axis value bound to a stable component identity.
    pub fn set_smart_component_value(
        &mut self,
        id: ComponentId,
        axis: &str,
        value: Option<f64>,
    ) -> Result<bool, DocumentEditError> {
        if !self
            .preserved
            .components
            .iter()
            .any(|component| component.id == id)
        {
            return Err(DocumentEditError::MissingComponent(id));
        }
        let Some(value) = value else {
            return Ok(self
                .preserved
                .smart_component_values
                .as_mut()
                .and_then(|values| values.get_mut(id))
                .is_some_and(|entry| entry.remove_value(axis)));
        };
        let values = self
            .preserved
            .smart_component_values
            .get_or_insert_with(SmartComponentValues::default);
        values.insert_component(id);
        values
            .get_mut(id)
            .expect("inserted smart-component entry")
            .set_value(axis, value)
            .map_err(|_| DocumentEditError::InvalidLayerMetadata)
    }

    /// Remove one component by stable identity.
    pub fn remove_component(&mut self, id: ComponentId) -> Result<bool, DocumentEditError> {
        let shape_index = self
            .layer
            .shapes
            .iter()
            .position(|shape| match shape {
                Shape::Component(component) => read_id(&component.format_specific) == Some(id.0),
                Shape::Path(_) => false,
            })
            .ok_or(DocumentEditError::MissingComponent(id))?;
        let preserved_index = self
            .preserved
            .components
            .iter()
            .position(|component| component.id == id)
            .expect("canonical component has preservation metadata");
        self.layer.shapes.remove(shape_index);
        self.preserved.components.remove(preserved_index);
        if let Some(values) = &mut self.preserved.smart_component_values {
            values.remove_component(id);
        }
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

    /// Append an anchor with a fresh stable identity.
    pub fn add_anchor(
        &mut self,
        name: String,
        position: kurbo::Point,
    ) -> Result<AnchorId, DocumentEditError> {
        ensure_finite(&[position.x, position.y])?;
        let id = AnchorId::next();
        let mut anchor = Anchor {
            x: position.x,
            y: position.y,
            name,
            ..Anchor::default()
        };
        write_id(&mut anchor.format_specific, id.0);
        self.layer.anchors.push(anchor);
        self.preserved.anchors.push(PreservedAnchor {
            id,
            color: None,
            metadata: ObjectMetadata {
                identifier: None,
                lib: None,
            },
        });
        Ok(id)
    }

    /// Remove one anchor by stable identity.
    pub fn remove_anchor(&mut self, id: AnchorId) -> Result<bool, DocumentEditError> {
        let anchor_index = self
            .layer
            .anchors
            .iter()
            .position(|anchor| read_id(&anchor.format_specific) == Some(id.0))
            .ok_or(DocumentEditError::MissingAnchor(id))?;
        let preserved_index = self
            .preserved
            .anchors
            .iter()
            .position(|anchor| anchor.id == id)
            .expect("canonical anchor has preservation metadata");
        self.layer.anchors.remove(anchor_index);
        self.preserved.anchors.remove(preserved_index);
        Ok(true)
    }

    /// Set or remove the source image attached to this layer.
    pub fn set_image(&mut self, image: Option<LayerImage>) -> bool {
        if self.preserved.image == image {
            return false;
        }
        self.preserved.image = image;
        true
    }

    /// Replace or remove the typed public mark color.
    pub fn set_mark_color(&mut self, color: Option<MarkColor>) -> Result<bool, DocumentEditError> {
        if color.is_some_and(|color| !color.is_valid()) {
            return Err(DocumentEditError::InvalidLayerMetadata);
        }
        if parse_mark_color(self.preserved.mark_color.as_ref()).ok() == Some(color) {
            return Ok(false);
        }
        self.preserved.mark_color = color.map(|color| {
            plist::Value::String(format!(
                "{},{},{},{}",
                color.red, color.green, color.blue, color.alpha
            ))
        });
        Ok(true)
    }

    /// Set or clear one semantic glyph mark atomically.
    ///
    /// A mark requires both its stable label and its typed public color.
    /// Clearing removes both UFO keys, while an equivalent edit retains the exact source spelling
    /// of the existing valid color.
    pub fn set_mark(
        &mut self,
        label: Option<&str>,
        color: Option<MarkColor>,
    ) -> Result<bool, DocumentEditError> {
        if label.is_some_and(str::is_empty)
            || color.is_some_and(|color| !color.is_valid())
            || label.is_some() != color.is_some()
        {
            return Err(DocumentEditError::InvalidLayerMetadata);
        }
        let Some((label, color)) = label.zip(color) else {
            let color_changed = self.preserved.mark_color.take().is_some();
            let label_changed = self.preserved.lib.remove(MARK_LABEL_KEY).is_some();
            return Ok(color_changed || label_changed);
        };
        let color_changed = parse_mark_color(self.preserved.mark_color.as_ref()) != Ok(Some(color));
        let label_value = plist::Value::String(label.to_owned());
        let label_changed = self.preserved.lib.get(MARK_LABEL_KEY) != Some(&label_value);
        if color_changed {
            self.preserved.mark_color = Some(plist::Value::String(format!(
                "{},{},{},{}",
                color.red, color.green, color.blue, color.alpha
            )));
        }
        if label_changed {
            self.preserved
                .lib
                .insert(MARK_LABEL_KEY.into(), label_value);
        }
        Ok(color_changed || label_changed)
    }

    /// Replace or remove one exact left or right metrics-key source string.
    pub fn set_metrics_key(
        &mut self,
        left: bool,
        source: Option<String>,
    ) -> Result<bool, DocumentEditError> {
        if source
            .as_deref()
            .is_some_and(|source| parse_metrics_key(source).is_none())
        {
            return Err(DocumentEditError::InvalidLayerMetadata);
        }
        let target = if left {
            &mut self.preserved.left_metrics_key
        } else {
            &mut self.preserved.right_metrics_key
        };
        let replacement = source.map(plist::Value::String);
        if *target == replacement {
            return Ok(false);
        }
        *target = replacement;
        Ok(true)
    }

    /// Replace validated editable metaball data, removing the key for an empty value.
    pub fn set_metaballs(&mut self, metaballs: Metaballs) -> Result<bool, DocumentEditError> {
        metaballs
            .validate()
            .map_err(|_| DocumentEditError::InvalidLayerMetadata)?;
        if parse_metaballs(self.preserved.metaballs.as_ref()).ok() == Some(metaballs.clone()) {
            return Ok(false);
        }
        self.preserved.metaballs = if metaballs.groups.is_empty() {
            None
        } else {
            Some(plist::to_value(&metaballs).map_err(|_| DocumentEditError::InvalidLayerMetadata)?)
        };
        Ok(true)
    }

    /// Convert selected live metaball groups to explicit cubic contours.
    ///
    /// `None` converts every group and an empty slice converts none. All sampling and curve
    /// fitting complete on a staged draft, so an invalid group or empty sampled outline leaves
    /// geometry and live source data unchanged. Generated topology receives fresh identities and
    /// empty source metadata while existing contours, components and anchors remain exact.
    pub fn collapse_metaballs(
        &mut self,
        groups: Option<&[u32]>,
        options: crate::outline::metaballs::OutlineOptions,
    ) -> Result<usize, String> {
        let mut data = self.view().metaballs().map_err(|error| error.to_string())?;
        let selected: HashSet<_> = groups
            .map(|groups| groups.iter().copied().collect())
            .unwrap_or_else(|| data.groups.iter().map(|group| group.id).collect());
        if selected
            .iter()
            .any(|id| !data.groups.iter().any(|group| group.id == *id))
        {
            return Err("unknown metaball group".into());
        }
        if selected.is_empty() {
            return Ok(0);
        }
        let mut generated = Vec::new();
        for group in data
            .groups
            .iter()
            .filter(|group| selected.contains(&group.id))
        {
            let paths = crate::outline::metaballs::cubic_outline(group, options)?;
            if paths.is_empty() {
                return Err("metaball group has no sampled outline; source preserved".into());
            }
            for path in paths {
                let smooth_at = path
                    .segments()
                    .map(|segment| {
                        (
                            crate::outline::glyph_paths::point_key(
                                segment.end().x,
                                segment.end().y,
                            ),
                            true,
                        )
                    })
                    .collect();
                let mut converted = Self::replacement_contour_from_path(&path, &smooth_at)
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "metaball outline did not produce a contour".to_owned())?;
                // Babelfont rotates closed cubics to begin with off-curves on import.
                // Restore img2bez's bottom start, keeping point metadata in the same order.
                if let (Some(start), Shape::Path(contour)) =
                    (path.segments().next().map(|s| s.start()), &mut converted.0)
                    && let Some(index) = contour.nodes.iter().position(|node| {
                        node.nodetype != NodeType::OffCurve
                            && node.x == start.x
                            && node.y == start.y
                    })
                {
                    contour.nodes.rotate_left(index);
                    converted.1.points.rotate_left(index);
                }
                generated.push(converted);
            }
        }
        data.groups.retain(|group| !selected.contains(&group.id));
        let mut staged = self.clone();
        let insert_at = staged
            .layer
            .shapes
            .iter()
            .rposition(|shape| matches!(shape, Shape::Path(_)))
            .map_or_else(
                || {
                    staged
                        .layer
                        .shapes
                        .iter()
                        .position(|shape| matches!(shape, Shape::Component(_)))
                        .unwrap_or(staged.layer.shapes.len())
                },
                |index| index + 1,
            );
        staged.layer.shapes.splice(
            insert_at..insert_at,
            generated.iter().map(|item| item.0.clone()),
        );
        staged
            .preserved
            .contours
            .extend(generated.into_iter().map(|item| item.1));
        staged
            .set_metaballs(data)
            .map_err(|error| error.to_string())?;
        *self = staged;
        Ok(selected.len())
    }

    /// Replace typed HOI intermediate points without materializing a UFO glyph.
    pub fn set_hoi_intermediates(&mut self, points: HoiIntermediates) -> bool {
        let replacement = (!points.is_empty()).then_some(points);
        if self.preserved.hoi_intermediates == replacement {
            return false;
        }
        self.preserved.hoi_intermediates = replacement;
        true
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
    /// The number of source-scoped values does not match the document source count.
    SourceCountMismatch,
    /// Canonical font information failed validation.
    InvalidFontInfo,
    /// Layer metadata is malformed, nonfinite or uses an unsupported schema.
    InvalidLayerMetadata,
    /// The requested point identity does not exist in the layer.
    MissingPoint(PointId),
    /// The requested contour identity does not exist in the layer.
    MissingContour(ContourId),
    /// The requested contour is already closed or lacks an initial move point.
    NotOpenContour(ContourId),
    /// The requested contour does not carry the editable hyperbezier kind.
    NotHyperContour(ContourId),
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
            Self::SourceCountMismatch => {
                formatter.write_str("source value count does not match the document")
            }
            Self::InvalidFontInfo => formatter.write_str("font information is invalid"),
            Self::InvalidLayerMetadata => formatter.write_str("glyph-layer metadata is invalid"),
            Self::MissingPoint(id) => write!(formatter, "point {id:?} does not exist"),
            Self::MissingContour(id) => write!(formatter, "contour {id:?} does not exist"),
            Self::NotOpenContour(id) => write!(formatter, "contour {id:?} is not open"),
            Self::NotHyperContour(id) => {
                write!(formatter, "contour {id:?} is not an editable hyperbezier")
            }
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

fn decode_imported_contours(
    contours: &[norad::Contour],
) -> Result<(Vec<Shape>, Vec<PreservedContour>, PastedContours), DocumentEditError> {
    ensure_finite(
        &contours
            .iter()
            .flat_map(|contour| &contour.points)
            .flat_map(|point| [point.x, point.y])
            .collect::<Vec<_>>(),
    )?;
    let mut result = PastedContours::default();
    let mut shapes = Vec::with_capacity(contours.len());
    let mut preserved = Vec::with_capacity(contours.len());
    for contour in contours {
        let contour_id = ContourId::next();
        result.contours.push(contour_id);
        let mut path = babelfont::Path {
            closed: contour.is_closed(),
            ..babelfont::Path::default()
        };
        write_id(&mut path.format_specific, contour_id.0);
        let mut points = Vec::with_capacity(contour.points.len());
        for point in &contour.points {
            let point_id = PointId::next();
            result.points.push(point_id);
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
            path.nodes.push(node);
            points.push(PreservedPoint {
                id: point_id,
                name: point.name.clone(),
                metadata: ObjectMetadata::new(point.identifier(), point.lib()),
            });
        }
        shapes.push(Shape::Path(path));
        preserved.push(PreservedContour {
            id: contour_id,
            hyper: ufo_contour_is_hyper(contour),
            metadata: ObjectMetadata::new(contour.identifier(), contour.lib()),
            points,
        });
    }
    Ok((shapes, preserved, result))
}

fn validate_ufo_contours(contours: &[norad::Contour]) -> Result<(), DocumentEditError> {
    ensure_finite(
        &contours
            .iter()
            .flat_map(|contour| &contour.points)
            .flat_map(|point| [point.x, point.y])
            .collect::<Vec<_>>(),
    )?;
    if contours.iter().any(|contour| {
        contour.lib().is_some() && contour.identifier().is_none()
            || contour
                .points
                .iter()
                .any(|point| point.lib().is_some() && point.identifier().is_none())
    }) {
        return Err(DocumentEditError::InvalidLayerMetadata);
    }
    let mut glyph = norad::Glyph::new("boundary");
    glyph.contours = contours.to_vec();
    let encoded = glyph
        .encode_xml()
        .map_err(|_| DocumentEditError::InvalidLayerMetadata)?;
    norad::Glyph::parse_raw(&encoded)
        .map(|_| ())
        .map_err(|_| DocumentEditError::InvalidLayerMetadata)
}

fn replace_path_shapes_preserving_slots(shapes: &mut Vec<Shape>, replacements: Vec<Shape>) {
    let mut replacements = replacements.into_iter();
    let mut output = Vec::with_capacity(shapes.len());
    let mut last_path_end = None;
    for shape in shapes.drain(..) {
        if matches!(shape, Shape::Path(_)) {
            if let Some(replacement) = replacements.next() {
                output.push(replacement);
                last_path_end = Some(output.len());
            }
        } else {
            output.push(shape);
        }
    }
    let remaining = replacements.collect::<Vec<_>>();
    let insert_at = last_path_end.unwrap_or(output.len());
    output.splice(insert_at..insert_at, remaining);
    *shapes = output;
}

fn same_copied_object_metadata(a: &ObjectMetadata, b: &ObjectMetadata) -> bool {
    a.identifier.is_some() == b.identifier.is_some() && a.lib == b.lib
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

fn parse_mark_color(value: Option<&plist::Value>) -> Result<Option<MarkColor>, DocumentEditError> {
    match value {
        None => Ok(None),
        Some(plist::Value::String(source)) if source.trim().is_empty() => Ok(None),
        Some(plist::Value::String(source)) => MarkColor::parse(source)
            .map(Some)
            .ok_or(DocumentEditError::InvalidLayerMetadata),
        Some(_) => Err(DocumentEditError::InvalidLayerMetadata),
    }
}

fn parse_metaballs(value: Option<&plist::Value>) -> Result<Metaballs, DocumentEditError> {
    let Some(value) = value else {
        return Ok(Metaballs::default());
    };
    let metaballs: Metaballs =
        plist::from_value(value).map_err(|_| DocumentEditError::InvalidLayerMetadata)?;
    metaballs
        .validate()
        .map_err(|_| DocumentEditError::InvalidLayerMetadata)?;
    Ok(metaballs)
}

fn fresh_hyper_identifier() -> norad::Identifier {
    let unique = norad::Identifier::from_uuidv4();
    let identifier = format!("hyperbezier-{}", unique.as_ref());
    norad::Identifier::new(&identifier).expect("generated hyperbezier identifier is valid")
}

fn fresh_object_identifier() -> norad::Identifier {
    norad::Identifier::from_uuidv4()
}

fn ufo_contour_is_hyper(contour: &norad::Contour) -> bool {
    contour
        .identifier()
        .is_some_and(|identifier| identifier.as_ref().contains("hyper"))
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
            hyper: ufo_contour_is_hyper(contour),
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
        let mut lib = component.lib().cloned().unwrap_or_default();
        let alignment = ComponentAlignment::take_from_lib(&mut lib);
        components.push(PreservedComponent {
            id: component_id,
            transform: component.transform,
            alignment,
            metadata: ObjectMetadata {
                identifier: component.identifier().cloned(),
                lib: (!lib.is_empty()).then_some(lib),
            },
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
    let mut lib = glyph.lib.clone();
    let mark_color = lib.remove(MARK_COLOR_KEY);
    let left_metrics_key = lib.remove(LEFT_METRICS_KEY);
    let right_metrics_key = lib.remove(RIGHT_METRICS_KEY);
    let metaballs = lib.remove(METABALLS_KEY);
    let composition_recipe = lib.remove(COMPOSITION_RECIPE_KEY);
    let component_order = components
        .iter()
        .map(|component| component.id)
        .collect::<Vec<_>>();
    let smart_component_axes = SmartComponentAxes::take_from_lib(&mut lib)
        .expect("UFO smart-component axes must satisfy the canonical metadata contract");
    let smart_component_values = SmartComponentValues::take_from_lib(&mut lib, &component_order)
        .expect("UFO smart-component values must satisfy the canonical metadata contract");
    let smart_component_pole = SmartComponentPole::take_from_lib(&mut lib)
        .expect("UFO smart-component poles must satisfy the canonical metadata contract");
    let hoi_intermediates = HoiIntermediates::take_from_lib(&mut lib);
    (
        layer,
        LayerPreservation {
            name: glyph.name().to_string(),
            width: glyph.width,
            height: glyph.height,
            codepoints: glyph.codepoints.iter().collect(),
            note: glyph.note.clone(),
            guidelines: glyph.guidelines.clone(),
            image: glyph.image.as_ref().map(LayerImage::from_ufo),
            lib,
            mark_color,
            left_metrics_key,
            right_metrics_key,
            metaballs,
            composition_recipe,
            smart_component_axes,
            smart_component_values,
            smart_component_pole,
            hoi_intermediates,
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
    let old_component_order = preserved
        .components
        .iter()
        .map(|component| component.id)
        .collect::<Vec<_>>();
    let smart_component_values = preserved.smart_component_values.as_ref().map(|values| {
        values
            .to_plist(&old_component_order)
            .expect("canonical smart-component values retain their source components")
    });
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
    if let Some(values) = smart_component_values {
        let new_component_order = preserved
            .components
            .iter()
            .map(|component| component.id)
            .collect::<Vec<_>>();
        preserved.smart_component_values = Some(
            SmartComponentValues::from_plist(&values, &new_component_order)
                .expect("copied smart-component values bind to copied components"),
        );
    }
    (layer, preserved)
}

pub(super) fn copy_contours_only(
    layer: &Layer,
    preserved: &LayerPreservation,
    id: &LayerId,
) -> (Layer, LayerPreservation) {
    let (mut layer, mut preserved) = copy_layer(layer, preserved, id);
    layer.shapes.retain(|shape| matches!(shape, Shape::Path(_)));
    layer.anchors.clear();
    preserved.height = 0.0;
    preserved.codepoints.clear();
    preserved.note = None;
    preserved.guidelines.clear();
    preserved.image = None;
    preserved.lib.clear();
    preserved.mark_color = None;
    preserved.left_metrics_key = None;
    preserved.right_metrics_key = None;
    preserved.metaballs = None;
    preserved.composition_recipe = None;
    preserved.smart_component_axes = None;
    preserved.smart_component_values = None;
    preserved.smart_component_pole = None;
    preserved.hoi_intermediates = None;
    preserved.components.clear();
    preserved.anchors.clear();
    (layer, preserved)
}

fn affine(t: norad::AffineTransform) -> kurbo::Affine {
    kurbo::Affine::new([
        t.x_scale, t.xy_scale, t.yx_scale, t.y_scale, t.x_offset, t.y_offset,
    ])
}

fn project_contours(layer: &Layer, preserved: &LayerPreservation) -> Vec<norad::Contour> {
    layer
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
        .collect()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "compare with the original narrowed Babelfont advance"
)]
pub(super) fn project_layer(layer: &Layer, preserved: &LayerPreservation) -> norad::Glyph {
    let mut glyph = norad::Glyph::new(&preserved.name);
    glyph.width = preserved.width;
    glyph.height = preserved.height;
    glyph.codepoints = norad::Codepoints::new(preserved.codepoints.iter().copied());
    glyph.note.clone_from(&preserved.note);
    glyph.guidelines.clone_from(&preserved.guidelines);
    glyph.image = preserved.image.as_ref().map(LayerImage::to_ufo);
    glyph.lib.clone_from(&preserved.lib);
    for (key, value) in [
        (MARK_COLOR_KEY, &preserved.mark_color),
        (LEFT_METRICS_KEY, &preserved.left_metrics_key),
        (RIGHT_METRICS_KEY, &preserved.right_metrics_key),
        (METABALLS_KEY, &preserved.metaballs),
        (COMPOSITION_RECIPE_KEY, &preserved.composition_recipe),
    ] {
        if let Some(value) = value {
            glyph.lib.insert(key.into(), value.clone());
        }
    }
    if let Some(axes) = &preserved.smart_component_axes {
        axes.write_to_lib(&mut glyph.lib);
    }
    let component_order = layer
        .components()
        .map(|component| {
            ComponentId(read_id(&component.format_specific).expect("canonical component identity"))
        })
        .collect::<Vec<_>>();
    if let Some(values) = &preserved.smart_component_values {
        values
            .write_to_lib(&mut glyph.lib, &component_order)
            .expect("canonical smart-component values retain current component identities");
    }
    if let Some(pole) = &preserved.smart_component_pole {
        pole.write_to_lib(&mut glyph.lib);
    }
    if let Some(points) = &preserved.hoi_intermediates {
        points.write_to_lib(&mut glyph.lib);
    }
    if layer.width != preserved.width as f32 {
        glyph.width = f64::from(layer.width);
    }
    glyph.contours = project_contours(layer, preserved);
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
            if let Some(original) = original {
                let mut lib = output.lib().cloned().unwrap_or_default();
                original.alignment.write_to_lib(&mut lib);
                if lib.is_empty() {
                    output.take_lib();
                } else {
                    output.replace_lib(lib);
                }
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

    #[test]
    fn layer_snapshot_rebind_requires_the_exact_old_address_and_layer() {
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.125;
        glyph.note = Some("retain me".into());
        let layer_id = LayerId {
            source: super::super::variable::SourceId(7),
            name: "public.default".into(),
        };
        let old = super::super::variable::GlyphLayerAddress {
            glyph: "A".into(),
            layer: layer_id.clone(),
        };
        let (layer, preserved) = layer_from_ufo(&glyph, &layer_id, true);
        let mut snapshot = CanonicalLayerSnapshot::new(old.clone(), layer, preserved);
        let original = snapshot.clone();

        let stale = super::super::variable::GlyphLayerAddress {
            glyph: "B".into(),
            layer: layer_id.clone(),
        };
        let renamed = super::super::variable::GlyphLayerAddress {
            glyph: "A.alt".into(),
            layer: layer_id.clone(),
        };
        assert!(!snapshot.rebind_glyph(&stale, &renamed));
        assert_eq!(snapshot, original);

        let wrong_layer = super::super::variable::GlyphLayerAddress {
            glyph: "A.alt".into(),
            layer: LayerId {
                source: layer_id.source,
                name: "background".into(),
            },
        };
        assert!(!snapshot.rebind_glyph(&old, &wrong_layer));
        assert_eq!(snapshot, original);

        assert!(snapshot.rebind_glyph(&old, &renamed));
        assert_eq!(snapshot.address(), &renamed);
        let (layer, preserved) = snapshot.into_parts();
        let projected = project_layer(&layer, &preserved);
        assert_eq!(projected.name().as_str(), "A.alt");
        assert_eq!(projected.width, 500.125);
        assert_eq!(projected.note.as_deref(), Some("retain me"));
    }

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
    fn smart_component_values_follow_component_identity() {
        use super::super::model::smart_components::{
            SMART_COMPONENT_AXES_KEY, SMART_COMPONENT_POLE_KEY, SMART_COMPONENT_VALUES_KEY,
        };

        let mut glyph = norad::Glyph::new("smart-user");
        for name in ["part.first", "part.second"] {
            glyph.components.push(norad::Component::new(
                norad::Name::new(name).unwrap(),
                norad::AffineTransform::default(),
                None,
            ));
        }
        let axis = [
            ("name".to_string(), plist::Value::String("Width".into())),
            (
                "bottomValue".to_string(),
                plist::Value::Integer(0_i64.into()),
            ),
            ("topValue".to_string(), plist::Value::Real(100.0)),
        ]
        .into_iter()
        .collect();
        glyph.lib.insert(
            SMART_COMPONENT_AXES_KEY.into(),
            plist::Value::Array(vec![plist::Value::Dictionary(axis)]),
        );
        glyph.lib.insert(
            SMART_COMPONENT_VALUES_KEY.into(),
            plist::Value::Array(vec![
                plist::Value::Dictionary(
                    [("Width".to_string(), plist::Value::Integer(25_i64.into()))]
                        .into_iter()
                        .collect(),
                ),
                plist::Value::Dictionary(
                    [("Width".to_string(), plist::Value::Real(75.0))]
                        .into_iter()
                        .collect(),
                ),
            ]),
        );
        glyph.lib.insert(
            SMART_COMPONENT_POLE_KEY.into(),
            plist::Value::Dictionary(
                [("Width".to_string(), plist::Value::Integer(2_i64.into()))]
                    .into_iter()
                    .collect(),
            ),
        );
        let id = LayerId {
            source: super::super::variable::SourceId(0),
            name: "public.default".into(),
        };
        let (mut layer, preserved) = layer_from_ufo(&glyph, &id, true);
        assert!(!preserved.lib.contains_key(SMART_COMPONENT_AXES_KEY));
        assert!(!preserved.lib.contains_key(SMART_COMPONENT_VALUES_KEY));
        assert!(!preserved.lib.contains_key(SMART_COMPONENT_POLE_KEY));
        let view = LayerView::new(&layer, &preserved);
        let components = view.components().map(ComponentView::id).collect::<Vec<_>>();
        assert_eq!(
            view.smart_component_value(components[0], "Width"),
            Some(25.0)
        );
        assert_eq!(
            view.smart_component_value(components[1], "Width"),
            Some(75.0)
        );
        assert!(view.smart_component_pole().unwrap().is_top("Width"));

        layer.shapes.swap(0, 1);
        let output = project_layer(&layer, &preserved);
        assert_eq!(
            output.lib[SMART_COMPONENT_VALUES_KEY],
            plist::Value::Array(vec![
                plist::Value::Dictionary(
                    [("Width".to_string(), plist::Value::Real(75.0))]
                        .into_iter()
                        .collect(),
                ),
                plist::Value::Dictionary(
                    [("Width".to_string(), plist::Value::Integer(25_i64.into()))]
                        .into_iter()
                        .collect(),
                ),
            ])
        );
        assert_eq!(
            output.lib[SMART_COMPONENT_AXES_KEY],
            glyph.lib[SMART_COMPONENT_AXES_KEY]
        );
        assert_eq!(
            output.lib[SMART_COMPONENT_POLE_KEY],
            glyph.lib[SMART_COMPONENT_POLE_KEY]
        );
    }

    #[test]
    fn component_alignment_edit_assigns_one_stable_boundary_identifier() {
        let mut glyph = norad::Glyph::new("component-user");
        glyph.components.push(norad::Component::new(
            norad::Name::new("base").unwrap(),
            norad::AffineTransform::default(),
            None,
        ));
        let layer_id = LayerId {
            source: super::super::variable::SourceId(0),
            name: "public.default".into(),
        };
        let (layer, preserved) = layer_from_ufo(&glyph, &layer_id, true);
        let component = LayerView::new(&layer, &preserved)
            .components()
            .next()
            .unwrap()
            .id();
        let mut draft = LayerEditDraft::new(layer, preserved);
        assert!(
            draft
                .set_component_alignment_disabled(component, true)
                .unwrap()
        );
        let (layer, preserved) = draft.into_parts();
        let first = project_layer(&layer, &preserved);
        let second = project_layer(&layer, &preserved);
        let identifier = first.components[0]
            .identifier()
            .expect("alignment metadata receives a stable identifier")
            .clone();
        assert_eq!(second.components[0].identifier(), Some(&identifier));

        let mut enabled = LayerEditDraft::new(layer, preserved);
        assert!(
            enabled
                .set_component_alignment_disabled(component, false)
                .unwrap()
        );
        let (enabled_layer, enabled_preserved) = enabled.into_parts();
        let enabled = project_layer(&enabled_layer, &enabled_preserved);
        assert_eq!(enabled.components[0].identifier(), Some(&identifier));
    }
}

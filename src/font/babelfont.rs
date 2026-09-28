// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Babelfont geometry with lossless UFO persistence projections.
//!
//! Babelfont owns paths, anchors and components. UFO payloads retain metadata its
//! model cannot express, plus exact advances and affine coefficients. A projection
//! takes geometry from Babelfont, restoring exact numbers when their corresponding
//! Babelfont value is unchanged. Compilation is the only quantizing boundary.
//!
//! `views` exposes read-only geometry, while the `edit_*` modules implement layer edits.
//! `ufo_projection` owns the import/export boundary, and the remaining modules handle
//! specialized curve operations and glyph transactions.

mod curve_conversion;
mod edit_components;
mod edit_contours;
mod edit_effects;
mod edit_points;
mod edit_replacements;
mod edit_segments;
pub(super) mod glyph_transactions;
mod handle_cleanup;
#[cfg(test)]
mod tests;
mod ufo_projection;
mod views;

pub use views::{AnchorView, ComponentView, ContourView, LayerShapeView, LayerView, PointView};

use ufo_projection::affine;
pub(super) use ufo_projection::{copy_contours_only, copy_layer, layer_from_ufo, project_layer};

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

#[derive(Clone, Copy, PartialEq, Eq)]
enum MetaballCollapseKind {
    Cubic,
    Hyperbezier,
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

    /// Read immutable canonical geometry and stable object identities from this snapshot.
    pub fn view(&self) -> LayerView<'_> {
        LayerView::new(&self.layer, &self.preserved)
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

    /// Contour and point identities retained by this exact canonical snapshot.
    pub(crate) fn contour_and_point_ids(&self) -> (Vec<ContourId>, Vec<PointId>) {
        let mut contours = Vec::new();
        let mut points = Vec::new();
        for contour in LayerView::new(&self.layer, &self.preserved).contours() {
            contours.push(contour.id());
            points.extend(contour.points().map(PointView::id));
        }
        (contours, points)
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
    /// Generated contour geometry has an invalid bound or segment topology.
    InvalidGeneratedContour(&'static str),
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
            Self::InvalidGeneratedContour(reason) => {
                write!(formatter, "generated contour: {reason}")
            }
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

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Direct curve conversion for canonical document layers.

use std::collections::HashSet;

use babelfont::{Node, NodeType, Shape};
use kurbo::{CubicBez, PathEl, Point};

use super::{
    ContourId, DocumentEditError, LayerEditDraft, PointId, PreservedContour, PreservedPoint,
    new_document_point, read_id,
};

const MAX_QUADRATICS_PER_CUBIC: usize = 1_024;

#[derive(Clone)]
struct StoredPoint {
    node: Node,
    preserved: PreservedPoint,
}

impl StoredPoint {
    fn position(&self) -> Point {
        Point::new(self.node.x, self.node.y)
    }
}

#[derive(Clone, Copy)]
enum Conversion {
    QuadraticToCubic,
    CubicToQuadratic { tolerance: f64 },
}

impl LayerEditDraft {
    /// Replace every canonical quadratic segment with an exact cubic segment.
    ///
    /// Existing explicit endpoints retain their identities and source metadata. Quadratic
    /// controls are deliberately retired because one source handle becomes two cubic handles;
    /// implied endpoints and every replacement handle receive fresh document identities.
    pub fn convert_quadratics_to_cubics(&mut self) -> Result<bool, DocumentEditError> {
        self.convert_ordinary_curves(Conversion::QuadraticToCubic)
    }

    /// Approximate every canonical cubic segment with quadratic segments within `tolerance`.
    ///
    /// The tolerance must be finite and greater than zero. Approximation is capped at 1,024
    /// quadratic segments per source cubic. Exceeding that limit rejects the complete draft
    /// operation without changing it.
    pub fn convert_cubics_to_quadratics(
        &mut self,
        tolerance: f64,
    ) -> Result<bool, DocumentEditError> {
        if !tolerance.is_finite() {
            return Err(DocumentEditError::NonFinite);
        }
        if tolerance <= 0.0 {
            return Err(DocumentEditError::Rejected);
        }
        self.convert_ordinary_curves(Conversion::CubicToQuadratic { tolerance })
    }

    /// Convert selected editable hyperbezier contours to explicit cubic geometry.
    ///
    /// Empty point and contour selections convert every hyperbezier contour. Otherwise, the
    /// union of contours named directly and contours containing selected points is converted.
    /// Every supplied identity is validated before conversion, including identities belonging to
    /// ordinary contours. Existing hyper on-curve points keep their identities and source
    /// metadata; solved handles receive fresh identities. The hyperbezier marker identifier is
    /// retired, while the contour identity and lib remain attached to the converted contour.
    pub fn convert_hyperbeziers_to_cubics(
        &mut self,
        selected_points: &[PointId],
        selected_contours: &[ContourId],
    ) -> Result<bool, DocumentEditError> {
        let point_ids: HashSet<_> = self
            .layer
            .paths()
            .flat_map(|path| &path.nodes)
            .map(|node| PointId(read_id(&node.format_specific).expect("canonical point identity")))
            .collect();
        for point in selected_points {
            if !point_ids.contains(point) {
                return Err(DocumentEditError::MissingPoint(*point));
            }
        }

        let contour_ids: HashSet<_> = self
            .layer
            .paths()
            .map(|path| {
                ContourId(read_id(&path.format_specific).expect("canonical contour identity"))
            })
            .collect();
        for contour in selected_contours {
            if !contour_ids.contains(contour) {
                return Err(DocumentEditError::MissingContour(*contour));
            }
        }

        let selected_points: HashSet<_> = selected_points.iter().copied().collect();
        let selected_contours: HashSet<_> = selected_contours.iter().copied().collect();
        let convert_all = selected_points.is_empty() && selected_contours.is_empty();
        let mut staged = self.clone();
        let mut changed = false;

        let targets: Vec<_> = staged
            .view()
            .contours()
            .filter(|contour| {
                contour.is_hyper()
                    && (convert_all
                        || selected_contours.contains(&contour.id())
                        || contour
                            .points()
                            .any(|point| selected_points.contains(&point.id())))
            })
            .map(|contour| contour.id())
            .collect();

        for contour_id in targets {
            staged.convert_one_hyperbezier(contour_id)?;
            changed = true;
        }
        if changed {
            *self = staged;
        }
        Ok(changed)
    }

    fn convert_ordinary_curves(
        &mut self,
        conversion: Conversion,
    ) -> Result<bool, DocumentEditError> {
        let mut staged = self.clone();
        let mut changed = false;
        for shape_index in 0..staged.layer.shapes.len() {
            let Shape::Path(path) = &staged.layer.shapes[shape_index] else {
                continue;
            };
            let contour_id =
                ContourId(read_id(&path.format_specific).expect("canonical contour identity"));
            let preserved_index = staged
                .preserved
                .contours
                .iter()
                .position(|contour| contour.id == contour_id)
                .expect("canonical contour preservation");
            if staged.preserved.contours[preserved_index].hyper {
                continue;
            }
            let (path, preserved) = match &mut staged.layer.shapes[shape_index] {
                Shape::Path(path) => (path, &mut staged.preserved.contours[preserved_index]),
                Shape::Component(_) => unreachable!("shape kind was checked above"),
            };
            changed |= convert_ordinary_contour(path, preserved, conversion)?;
        }
        if changed {
            *self = staged;
        }
        Ok(changed)
    }

    fn convert_one_hyperbezier(&mut self, contour_id: ContourId) -> Result<(), DocumentEditError> {
        let contour = self
            .view()
            .contours()
            .find(|contour| contour.id() == contour_id)
            .ok_or(DocumentEditError::MissingContour(contour_id))?;
        let crate::outline::path::Path::Hyper(hyper) =
            crate::outline::path::Path::from_document_contour(contour)
        else {
            return Err(DocumentEditError::Rejected);
        };
        let solved = hyper.to_bezpath();

        let shape_index = self
            .layer
            .shapes
            .iter()
            .position(|shape| match shape {
                Shape::Path(path) => read_id(&path.format_specific) == Some(contour_id.0),
                Shape::Component(_) => false,
            })
            .expect("canonical contour shape");
        let preserved_index = self
            .preserved
            .contours
            .iter()
            .position(|contour| contour.id == contour_id)
            .expect("canonical contour preservation");
        let Shape::Path(path) = &self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        let preserved = &self.preserved.contours[preserved_index];
        let source = stored_points(path, preserved);
        let on_curves: Vec<_> = source
            .into_iter()
            .filter(|point| point.node.nodetype != NodeType::OffCurve)
            .collect();
        let converted = solved_hyper_nodes(&solved, &on_curves, path.closed)?;

        let Shape::Path(path) = &mut self.layer.shapes[shape_index] else {
            unreachable!("located contour is a path");
        };
        path.nodes = converted.iter().map(|point| point.node.clone()).collect();
        let preserved = &mut self.preserved.contours[preserved_index];
        preserved.points = converted.into_iter().map(|point| point.preserved).collect();
        preserved.hyper = false;
        preserved.metadata.identifier = preserved
            .metadata
            .lib
            .is_some()
            .then(norad::Identifier::from_uuidv4);
        Ok(())
    }
}

fn convert_ordinary_contour(
    path: &mut babelfont::Path,
    preserved: &mut PreservedContour,
    conversion: Conversion,
) -> Result<bool, DocumentEditError> {
    debug_assert_eq!(
        path.nodes.len(),
        preserved.points.len(),
        "canonical nodes and preserved point records stay aligned"
    );
    if path.nodes.is_empty() {
        return Ok(false);
    }
    let original_first =
        PointId(read_id(&path.nodes[0].format_specific).expect("canonical point identity"));
    let points = stored_points(path, preserved);
    let Some(start_index) = points
        .iter()
        .position(|point| point.node.nodetype != NodeType::OffCurve)
    else {
        return match conversion {
            Conversion::QuadraticToCubic if path.closed => {
                let converted = all_offcurve_quadratic_to_cubic(&points)?;
                path.nodes = converted.iter().map(|point| point.node.clone()).collect();
                preserved.points = converted.into_iter().map(|point| point.preserved).collect();
                Ok(true)
            }
            _ => Ok(false),
        };
    };
    if !path.closed && start_index != 0 {
        return Ok(false);
    }

    let rotated: Vec<_> = points[start_index..]
        .iter()
        .chain(&points[..start_index])
        .cloned()
        .collect();
    let mut output = vec![rotated[0].clone()];
    let mut start = rotated[0].position();
    let mut controls = Vec::new();
    let indices: Vec<_> = if path.closed {
        (1..=rotated.len())
            .map(|index| index % rotated.len())
            .collect()
    } else {
        (1..rotated.len()).collect()
    };
    let mut changed = false;
    for index in indices {
        let point = rotated[index].clone();
        if point.node.nodetype == NodeType::OffCurve {
            controls.push(point);
            continue;
        }
        let closing = path.closed && index == 0;
        let end = point.position();
        match conversion {
            Conversion::QuadraticToCubic
                if point.node.nodetype == NodeType::QCurve && !controls.is_empty() =>
            {
                append_quadratics_as_cubics(&mut output, start, &controls, point, closing)?;
                changed = true;
            }
            Conversion::CubicToQuadratic { tolerance }
                if point.node.nodetype == NodeType::Curve && controls.len() == 2 =>
            {
                append_cubic_as_quadratics(
                    &mut output,
                    start,
                    &controls,
                    point,
                    closing,
                    tolerance,
                )?;
                changed = true;
            }
            _ => append_unchanged_segment(&mut output, &controls, point, closing),
        }
        controls.clear();
        start = end;
    }
    if !changed {
        return Ok(false);
    }
    if path.closed {
        rotate_to_surviving_start(&mut output, original_first);
    }
    path.nodes = output.iter().map(|point| point.node.clone()).collect();
    preserved.points = output.into_iter().map(|point| point.preserved).collect();
    Ok(true)
}

fn stored_points(path: &babelfont::Path, preserved: &PreservedContour) -> Vec<StoredPoint> {
    path.nodes
        .iter()
        .map(|node| {
            let id = PointId(read_id(&node.format_specific).expect("canonical point identity"));
            StoredPoint {
                node: node.clone(),
                preserved: preserved
                    .points
                    .iter()
                    .find(|point| point.id == id)
                    .expect("canonical point preservation")
                    .clone(),
            }
        })
        .collect()
}

fn fresh_point(position: Point, point_type: NodeType, smooth: bool) -> StoredPoint {
    let (_, node, preserved) = new_document_point(position, point_type, smooth);
    StoredPoint { node, preserved }
}

fn append_unchanged_segment(
    output: &mut Vec<StoredPoint>,
    controls: &[StoredPoint],
    endpoint: StoredPoint,
    closing: bool,
) {
    output.extend(controls.iter().cloned());
    if closing {
        output[0].node.nodetype = endpoint.node.nodetype;
        output[0].node.smooth = endpoint.node.smooth;
    } else {
        output.push(endpoint);
    }
}

fn append_quadratics_as_cubics(
    output: &mut Vec<StoredPoint>,
    mut start: Point,
    controls: &[StoredPoint],
    endpoint: StoredPoint,
    closing: bool,
) -> Result<(), DocumentEditError> {
    for (index, control) in controls.iter().enumerate() {
        let last = index + 1 == controls.len();
        let end = if last {
            endpoint.position()
        } else {
            control.position().midpoint(controls[index + 1].position())
        };
        let first = start + (control.position() - start) * (2.0 / 3.0);
        let second = end + (control.position() - end) * (2.0 / 3.0);
        ensure_points_finite(&[first, second, end])?;
        output.push(fresh_point(first, NodeType::OffCurve, false));
        output.push(fresh_point(second, NodeType::OffCurve, false));
        if last {
            if closing {
                output[0].node.nodetype = NodeType::Curve;
                output[0].node.smooth = endpoint.node.smooth;
            } else {
                let mut endpoint = endpoint.clone();
                endpoint.node.nodetype = NodeType::Curve;
                output.push(endpoint);
            }
        } else {
            output.push(fresh_point(end, NodeType::Curve, false));
        }
        start = end;
    }
    Ok(())
}

fn append_cubic_as_quadratics(
    output: &mut Vec<StoredPoint>,
    start: Point,
    controls: &[StoredPoint],
    endpoint: StoredPoint,
    closing: bool,
    tolerance: f64,
) -> Result<(), DocumentEditError> {
    let cubic = CubicBez::new(
        start,
        controls[0].position(),
        controls[1].position(),
        endpoint.position(),
    );
    let quadratics: Vec<_> = cubic
        .to_quads(tolerance)
        .take(MAX_QUADRATICS_PER_CUBIC + 1)
        .map(|(_, _, quadratic)| quadratic)
        .collect();
    if quadratics.is_empty() || quadratics.len() > MAX_QUADRATICS_PER_CUBIC {
        return Err(DocumentEditError::Rejected);
    }
    for (index, quadratic) in quadratics.iter().enumerate() {
        ensure_points_finite(&[quadratic.p0, quadratic.p1, quadratic.p2])?;
        output.push(fresh_point(quadratic.p1, NodeType::OffCurve, false));
        let last = index + 1 == quadratics.len();
        if last {
            if closing {
                output[0].node.nodetype = NodeType::QCurve;
                output[0].node.smooth = endpoint.node.smooth;
            } else {
                let mut endpoint = endpoint.clone();
                endpoint.node.nodetype = NodeType::QCurve;
                output.push(endpoint);
            }
        } else {
            output.push(fresh_point(quadratic.p2, NodeType::QCurve, true));
        }
    }
    Ok(())
}

fn all_offcurve_quadratic_to_cubic(
    points: &[StoredPoint],
) -> Result<Vec<StoredPoint>, DocumentEditError> {
    let mut start = points[points.len() - 1]
        .position()
        .midpoint(points[0].position());
    let mut output = vec![fresh_point(start, NodeType::Curve, false)];
    for (index, control) in points.iter().enumerate() {
        let next = &points[(index + 1) % points.len()];
        let end = control.position().midpoint(next.position());
        let first = start + (control.position() - start) * (2.0 / 3.0);
        let second = end + (control.position() - end) * (2.0 / 3.0);
        ensure_points_finite(&[first, second, end])?;
        output.push(fresh_point(first, NodeType::OffCurve, false));
        output.push(fresh_point(second, NodeType::OffCurve, false));
        if index + 1 != points.len() {
            output.push(fresh_point(end, NodeType::Curve, false));
        }
        start = end;
    }
    Ok(output)
}

fn rotate_to_surviving_start(points: &mut [StoredPoint], original: PointId) {
    if let Some(index) = points
        .iter()
        .position(|point| point.preserved.id == original)
    {
        points.rotate_left(index);
    }
}

fn solved_hyper_nodes(
    solved: &kurbo::BezPath,
    on_curves: &[StoredPoint],
    closed: bool,
) -> Result<Vec<StoredPoint>, DocumentEditError> {
    if on_curves.is_empty() {
        return solved
            .elements()
            .is_empty()
            .then_some(Vec::new())
            .ok_or(DocumentEditError::Rejected);
    }
    let start_index = usize::from(closed && on_curves.len() > 1) % on_curves.len();
    let mut source_index = start_index;
    let mut output = Vec::new();
    let mut current = None;
    let mut matched_on_curves = 1_usize;
    let mut segments = 0_usize;
    for element in solved.elements() {
        match *element {
            PathEl::MoveTo(position) => {
                if !output.is_empty() || position.distance(on_curves[start_index].position()) > 1e-9
                {
                    return Err(DocumentEditError::Rejected);
                }
                ensure_points_finite(&[position])?;
                let mut point = on_curves[start_index].clone();
                point.node.nodetype = if closed {
                    NodeType::Curve
                } else {
                    NodeType::Move
                };
                output.push(point);
                current = Some(position);
            }
            PathEl::LineTo(end) => {
                append_solved_hyper_segment(
                    &mut output,
                    on_curves,
                    start_index,
                    &mut source_index,
                    &mut matched_on_curves,
                    closed,
                    end,
                    NodeType::Line,
                )?;
                segments += 1;
                current = Some(end);
            }
            PathEl::CurveTo(first, second, end) => {
                ensure_points_finite(&[first, second, end])?;
                output.push(fresh_point(first, NodeType::OffCurve, false));
                output.push(fresh_point(second, NodeType::OffCurve, false));
                append_solved_hyper_segment(
                    &mut output,
                    on_curves,
                    start_index,
                    &mut source_index,
                    &mut matched_on_curves,
                    closed,
                    end,
                    NodeType::Curve,
                )?;
                segments += 1;
                current = Some(end);
            }
            PathEl::QuadTo(control, end) => {
                let start = current.ok_or(DocumentEditError::Rejected)?;
                let first = start + (control - start) * (2.0 / 3.0);
                let second = end + (control - end) * (2.0 / 3.0);
                ensure_points_finite(&[first, second, end])?;
                output.push(fresh_point(first, NodeType::OffCurve, false));
                output.push(fresh_point(second, NodeType::OffCurve, false));
                append_solved_hyper_segment(
                    &mut output,
                    on_curves,
                    start_index,
                    &mut source_index,
                    &mut matched_on_curves,
                    closed,
                    end,
                    NodeType::Curve,
                )?;
                segments += 1;
                current = Some(end);
            }
            PathEl::ClosePath => {}
        }
    }
    let expected_matches = if closed {
        on_curves.len() + 1
    } else {
        on_curves.len()
    };
    if output.is_empty() || segments == 0 || matched_on_curves != expected_matches {
        return Err(DocumentEditError::Rejected);
    }
    if closed {
        rotate_to_surviving_start(&mut output, on_curves[0].preserved.id);
    }
    Ok(output)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the solver segment carries explicit mapping state"
)]
fn append_solved_hyper_segment(
    output: &mut Vec<StoredPoint>,
    on_curves: &[StoredPoint],
    start_index: usize,
    source_index: &mut usize,
    matched_on_curves: &mut usize,
    closed: bool,
    end: Point,
    point_type: NodeType,
) -> Result<(), DocumentEditError> {
    ensure_points_finite(&[end])?;
    let next_index = (*source_index + 1) % on_curves.len();
    let expected = &on_curves[next_index];
    if end.distance(expected.position()) <= 1e-9 {
        *source_index = next_index;
        *matched_on_curves += 1;
        let closing = closed && next_index == start_index;
        if closing {
            output[0].node.nodetype = point_type;
            output[0].node.smooth = point_type == NodeType::Curve;
        } else {
            let mut endpoint = expected.clone();
            endpoint.node.nodetype = point_type;
            endpoint.node.smooth = point_type == NodeType::Curve;
            output.push(endpoint);
        }
    } else {
        output.push(fresh_point(end, point_type, point_type == NodeType::Curve));
    }
    Ok(())
}

fn ensure_points_finite(points: &[Point]) -> Result<(), DocumentEditError> {
    points
        .iter()
        .all(|point| point.x.is_finite() && point.y.is_finite())
        .then_some(())
        .ok_or(DocumentEditError::NonFinite)
}

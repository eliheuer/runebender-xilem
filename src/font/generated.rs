// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded, toolkit-independent contour geometry supplied by shape generators.
//!
//! These values contain no canonical identities or source-format metadata.
//! The font engine validates them before minting IDs in a layer draft.

use kurbo::Point;

use super::{DocumentEditError, LayerPointType};

/// Maximum generated contours in one guarded transaction.
pub const MAX_GENERATED_CONTOURS: usize = 256;
/// Maximum generated points in one guarded transaction.
pub const MAX_GENERATED_POINTS: usize = 4_096;
const MAX_ABS_COORDINATE: f64 = 1_000_000.0;

/// One generator-owned point with a segment role and smoothness flag.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedPoint {
    /// Position in font design coordinates.
    pub position: Point,
    /// On-curve role or off-curve control role.
    pub point_type: LayerPointType,
    /// Whether tangent continuity is requested at this point.
    pub smooth: bool,
}

/// One ordinary contour in storage order.
///
/// An initial Move denotes an open contour; a closed contour contains no Move.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedContour {
    /// Points in the order stored in the canonical layer.
    pub points: Vec<GeneratedPoint>,
}

fn valid_segment(endpoint: LayerPointType, controls: usize) -> bool {
    match endpoint {
        LayerPointType::Line => controls == 0,
        LayerPointType::Curve => controls == 2,
        LayerPointType::QCurve => controls >= 1,
        LayerPointType::Move | LayerPointType::OffCurve => false,
    }
}

fn validate_one(contour: &GeneratedContour) -> Result<(), DocumentEditError> {
    let points = &contour.points;
    if points.is_empty() {
        return Err(DocumentEditError::InvalidGeneratedContour(
            "contour has no points",
        ));
    }
    if points
        .iter()
        .any(|point| !point.position.x.is_finite() || !point.position.y.is_finite())
    {
        return Err(DocumentEditError::NonFinite);
    }
    if points
        .iter()
        .any(|point| point.point_type == LayerPointType::OffCurve && point.smooth)
    {
        return Err(DocumentEditError::InvalidGeneratedContour(
            "off-curve controls cannot be smooth",
        ));
    }
    if points
        .iter()
        .any(|point| point.position.x.abs().max(point.position.y.abs()) > MAX_ABS_COORDINATE)
    {
        return Err(DocumentEditError::InvalidGeneratedContour(
            "generated coordinates exceed one million font units",
        ));
    }
    let open = points[0].point_type == LayerPointType::Move;
    if open {
        if points.len() < 2 {
            return Err(DocumentEditError::InvalidGeneratedContour(
                "open contour needs a segment",
            ));
        }
        let mut controls = 0_usize;
        for point in &points[1..] {
            if point.point_type == LayerPointType::OffCurve {
                controls += 1;
            } else if valid_segment(point.point_type, controls) {
                controls = 0;
            } else {
                return Err(DocumentEditError::InvalidGeneratedContour(
                    "open contour has invalid segment topology",
                ));
            }
        }
        if controls != 0 {
            return Err(DocumentEditError::InvalidGeneratedContour(
                "open contour ends with controls",
            ));
        }
        return Ok(());
    }
    if points
        .iter()
        .any(|point| point.point_type == LayerPointType::Move)
    {
        return Err(DocumentEditError::InvalidGeneratedContour(
            "closed contour contains a Move point",
        ));
    }
    let Some(first_on_curve) = points
        .iter()
        .position(|point| point.point_type != LayerPointType::OffCurve)
    else {
        return (points.len() >= 2).then_some(()).ok_or(
            DocumentEditError::InvalidGeneratedContour("closed quadratic needs two controls"),
        );
    };
    if points.len() < 2 {
        return Err(DocumentEditError::InvalidGeneratedContour(
            "closed contour needs two points",
        ));
    }
    let mut controls = 0_usize;
    for offset in 1..=points.len() {
        let point = &points[(first_on_curve + offset) % points.len()];
        if point.point_type == LayerPointType::OffCurve {
            controls += 1;
        } else if valid_segment(point.point_type, controls) {
            controls = 0;
        } else {
            return Err(DocumentEditError::InvalidGeneratedContour(
                "closed contour has invalid segment topology",
            ));
        }
    }
    Ok(())
}

/// Validate bounded contour geometry before any canonical draft is changed.
///
/// Quadratic chains may use implied on-curve joins, including a closed contour
/// consisting entirely of off-curve controls.
pub fn validate_contours(contours: &[GeneratedContour]) -> Result<(), DocumentEditError> {
    if contours.is_empty() || contours.len() > MAX_GENERATED_CONTOURS {
        return Err(DocumentEditError::InvalidGeneratedContour(
            "generated contour count is outside 1..=256",
        ));
    }
    let point_count = contours.iter().fold(0_usize, |count, contour| {
        count.saturating_add(contour.points.len())
    });
    if point_count > MAX_GENERATED_POINTS {
        return Err(DocumentEditError::InvalidGeneratedContour(
            "generated point count exceeds 4096",
        ));
    }
    for contour in contours {
        validate_one(contour)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, kind: LayerPointType) -> GeneratedPoint {
        GeneratedPoint {
            position: Point::new(x, 0.0),
            point_type: kind,
            smooth: false,
        }
    }

    fn contour(kinds: &[LayerPointType]) -> GeneratedContour {
        GeneratedContour {
            points: kinds
                .iter()
                .enumerate()
                .map(|(index, kind)| point(index as f64, *kind))
                .collect(),
        }
    }

    #[test]
    fn accepts_open_cubic_and_closed_quadratic_topologies() {
        use LayerPointType::{Curve, Line, Move, OffCurve, QCurve};
        assert!(validate_contours(&[contour(&[Move, Line])]).is_ok());
        assert!(validate_contours(&[contour(&[Move, OffCurve, OffCurve, Curve])]).is_ok());
        assert!(validate_contours(&[contour(&[Line, OffCurve, QCurve])]).is_ok());
        assert!(validate_contours(&[contour(&[OffCurve, OffCurve, OffCurve])]).is_ok());
    }

    #[test]
    fn rejects_malformed_nonfinite_or_oversized_geometry() {
        use LayerPointType::{Curve, Line, Move, OffCurve, QCurve};
        for kinds in [
            vec![Move],
            vec![Move, OffCurve],
            vec![Move, OffCurve, Curve],
            vec![Line, Move],
            vec![Line, OffCurve, Curve],
            vec![Line, QCurve],
            vec![OffCurve],
        ] {
            assert!(validate_contours(&[contour(&kinds)]).is_err(), "{kinds:?}");
        }
        let mut nonfinite = contour(&[Move, Line]);
        nonfinite.points[1].position.x = f64::NAN;
        assert_eq!(
            validate_contours(&[nonfinite]),
            Err(DocumentEditError::NonFinite)
        );
        let mut far = contour(&[Move, Line]);
        far.points[1].position.x = 1_000_000.1;
        assert!(validate_contours(&[far]).is_err());
        let mut smooth_control = contour(&[Move, OffCurve, OffCurve, Curve]);
        smooth_control.points[1].smooth = true;
        assert!(validate_contours(&[smooth_control]).is_err());
        assert!(validate_contours(&[]).is_err());
        let many = vec![contour(&[Move, Line]); MAX_GENERATED_CONTOURS + 1];
        assert!(validate_contours(&many).is_err());
        let huge = GeneratedContour {
            points: vec![point(0.0, OffCurve); MAX_GENERATED_POINTS + 1],
        };
        assert!(validate_contours(&[huge]).is_err());
    }
}

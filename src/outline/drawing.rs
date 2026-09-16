// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Explicit UFO contour descriptions for validated agent drawings in font coordinates.

use serde::{Deserialize, Serialize};

/// One contour in UFO point order. A leading move denotes an open contour.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingContour {
    /// Ordered on-curve and off-curve points; coordinates use font units, y upwards.
    pub points: Vec<DrawingPoint>,
}

/// A point in a drawing. A segment's type is stored on its ending on-curve point.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DrawingPoint {
    /// Horizontal coordinate in font units.
    pub x: f64,
    /// Vertical coordinate in font units.
    pub y: f64,
    /// UFO point type.
    #[serde(rename = "type")]
    pub kind: DrawingPointType,
    /// Whether this on-curve point is intended to be smooth; not a continuity guarantee.
    #[serde(default)]
    pub smooth: bool,
}

/// Point types accepted by the drawing interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DrawingPointType {
    /// First point of an open contour.
    Move,
    /// End of a straight segment.
    Line,
    /// End of a cubic segment, preceded by exactly two off-curve points.
    Curve,
    /// End of a quadratic segment, preceded by one or more off-curve points.
    Qcurve,
    /// A control point.
    Offcurve,
}

/// Converts explicit contours after checking coordinates and segment grammar.
/// Empty outlines are allowed for deliberate clearing. Limits are 256 contours and
/// 16,384 points per glyph. Does not alter winding, smoothness, or point order.
pub fn contours(input: &[DrawingContour]) -> Result<Vec<norad::Contour>, String> {
    if input.len() > 256 || input.iter().map(|c| c.points.len()).sum::<usize>() > 16_384 {
        return Err("drawing exceeds contour or point limit".into());
    }
    input
        .iter()
        .enumerate()
        .map(|(ci, contour)| {
            let points = &contour.points;
            if points.len() < 2 {
                return Err(format!("contour {ci}: at least two points required"));
            }
            let open = points[0].kind == DrawingPointType::Move;
            if open
                && points
                    .last()
                    .is_some_and(|p| p.kind == DrawingPointType::Offcurve)
            {
                return Err(format!("contour {ci}: unfinished open segment"));
            }
            let mut on_curves = 0;
            for (pi, point) in points.iter().enumerate() {
                if !point.x.is_finite()
                    || !point.y.is_finite()
                    || point.x.abs().max(point.y.abs()) > 1_000_000.0
                {
                    return Err(format!("contour {ci}, point {pi}: invalid coordinate"));
                }
                if point.kind == DrawingPointType::Offcurve {
                    if point.smooth {
                        return Err("off-curve points cannot be marked smooth".into());
                    }
                    continue;
                }
                on_curves += 1;
                if point.kind == DrawingPointType::Move {
                    if pi != 0 {
                        return Err("move is allowed only at the start of an open contour".into());
                    }
                    continue;
                }
                let controls = (1..points.len())
                    .take_while(|offset| {
                        if open && *offset > pi {
                            return false;
                        }
                        points[(pi + points.len() - offset) % points.len()].kind
                            == DrawingPointType::Offcurve
                    })
                    .count();
                let valid = match point.kind {
                    DrawingPointType::Line => controls == 0,
                    DrawingPointType::Curve => controls == 2,
                    DrawingPointType::Qcurve => controls >= 1,
                    _ => false,
                };
                if !valid {
                    return Err(format!(
                        "contour {ci}, point {pi}: wrong number of control points"
                    ));
                }
            }
            if on_curves == 0 {
                return Err("use explicit on-curve points for quadratic contours".into());
            }
            Ok(norad::Contour::new(
                points
                    .iter()
                    .map(|p| {
                        norad::ContourPoint::new(
                            p.x,
                            p.y,
                            match p.kind {
                                DrawingPointType::Move => norad::PointType::Move,
                                DrawingPointType::Line => norad::PointType::Line,
                                DrawingPointType::Curve => norad::PointType::Curve,
                                DrawingPointType::Qcurve => norad::PointType::QCurve,
                                DrawingPointType::Offcurve => norad::PointType::OffCurve,
                            },
                            p.smooth,
                            None,
                            None,
                        )
                    })
                    .collect(),
                None,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cubic_grammar_rejects_incomplete_segments() {
        let drawing = |types: &[&str]| -> Vec<DrawingContour> {
            serde_json::from_value(json!([{"points": types.iter().map(|kind|
                json!({"x":0,"y":0,"type":kind})).collect::<Vec<_>>()}]))
            .unwrap()
        };
        assert!(contours(&drawing(&["move", "offcurve", "offcurve", "curve"])).is_ok());
        assert!(contours(&drawing(&["move", "offcurve", "curve"])).is_err());
        assert!(contours(&drawing(&["move", "line", "offcurve"])).is_err());
        assert!(contours(&drawing(&["line", "move"])).is_err());
        assert!(contours(&drawing(&["curve", "offcurve", "offcurve"])).is_ok());
        assert!(contours(&drawing(&["offcurve", "offcurve"])).is_err());
    }
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Explicit transient UFO codec values.

use sha2::{Digest as _, Sha256};

use crate::font::project::Project;
use crate::font::variable::{LayerId, SourceId};
use crate::font::{ImportedContours, LayerView};

/// Convert one closed path into a detached UFO contour at a format boundary.
pub(crate) fn bezpath_to_contour(
    path: &kurbo::BezPath,
    smooth_at: &std::collections::HashMap<(i64, i64), bool>,
) -> Option<norad::Contour> {
    use kurbo::PathEl;

    let mut points = Vec::new();
    let mut start = None;
    let smooth = |x: f64, y: f64| {
        smooth_at
            .get(&crate::outline::glyph_paths::point_key(x, y))
            .copied()
            .unwrap_or(false)
    };
    let on = |x: f64, y: f64, curve: bool, smooth: bool| {
        norad::ContourPoint::new(
            x.round(),
            y.round(),
            if curve {
                norad::PointType::Curve
            } else {
                norad::PointType::Line
            },
            smooth,
            None,
            None,
        )
    };
    let off = |point: kurbo::Point| {
        norad::ContourPoint::new(
            point.x.round(),
            point.y.round(),
            norad::PointType::OffCurve,
            false,
            None,
            None,
        )
    };
    for element in path.elements() {
        match element {
            PathEl::MoveTo(point) => start = Some(*point),
            PathEl::LineTo(point) => {
                points.push(on(point.x, point.y, false, smooth(point.x, point.y)));
            }
            PathEl::CurveTo(first, second, point) => {
                points.push(off(*first));
                points.push(off(*second));
                points.push(on(point.x, point.y, true, smooth(point.x, point.y)));
            }
            PathEl::QuadTo(control, point) => {
                let segment_start = points
                    .iter()
                    .rev()
                    .find(|candidate: &&norad::ContourPoint| {
                        candidate.typ != norad::PointType::OffCurve
                    })
                    .map(|candidate| kurbo::Point::new(candidate.x, candidate.y))
                    .or(start)?;
                let first =
                    segment_start + (control.to_vec2() - segment_start.to_vec2()) * (2.0 / 3.0);
                let second = *point + (control.to_vec2() - point.to_vec2()) * (2.0 / 3.0);
                points.push(off(first));
                points.push(off(second));
                points.push(on(point.x, point.y, true, smooth(point.x, point.y)));
            }
            PathEl::ClosePath => {}
        }
    }
    let start = start?;
    let last_on = points
        .iter()
        .rposition(|point| point.typ != norad::PointType::OffCurve)?;
    let point = &points[last_on];
    if (point.x - start.x.round()).abs() < 0.51 && (point.y - start.y.round()).abs() < 0.51 {
        let tail: Vec<_> = points.drain(last_on..).collect();
        let (controls, on_point) = tail.split_at(tail.len() - 1);
        let mut rotated = vec![on_point[0].clone()];
        rotated.extend(points);
        rotated.extend(controls.iter().cloned());
        points = rotated;
    } else {
        points.insert(0, on(start.x, start.y, false, smooth(start.x, start.y)));
    }
    (points
        .iter()
        .filter(|point| point.typ != norad::PointType::OffCurve)
        .count()
        >= 2)
        .then(|| norad::Contour::new(points, None))
}

/// Opaque SHA-256 revision of one serialized GLIF value.
pub fn glyph_revision(glyph: &norad::Glyph) -> Result<String, String> {
    let bytes = glyph.encode_xml().map_err(|error| error.to_string())?;
    Ok(format!("glif-sha256:{:x}", Sha256::digest(bytes)))
}

/// Decode UFO contours once into canonical Babelfont paths plus exact object metadata.
pub fn decode_contours(contours: &[norad::Contour]) -> Result<ImportedContours, String> {
    ImportedContours::from_ufo(contours).map_err(|error| error.to_string())
}

/// Decode the public drawing schema into canonical contours at the UFO wire boundary.
pub fn decode_drawing_contours(
    input: &[crate::outline::drawing::DrawingContour],
) -> Result<ImportedContours, String> {
    decode_contours(&drawing_contours(input)?)
}

pub(crate) fn drawing_contours(
    input: &[crate::outline::drawing::DrawingContour],
) -> Result<Vec<norad::Contour>, String> {
    use crate::outline::drawing::DrawingPointType;

    crate::outline::drawing::validate(input)?;
    Ok(input
        .iter()
        .map(|contour| {
            norad::Contour::new(
                contour
                    .points
                    .iter()
                    .map(|point| {
                        norad::ContourPoint::new(
                            point.x,
                            point.y,
                            match point.kind {
                                DrawingPointType::Move => norad::PointType::Move,
                                DrawingPointType::Line => norad::PointType::Line,
                                DrawingPointType::Curve => norad::PointType::Curve,
                                DrawingPointType::Qcurve => norad::PointType::QCurve,
                                DrawingPointType::Offcurve => norad::PointType::OffCurve,
                            },
                            point.smooth,
                            None,
                            None,
                        )
                    })
                    .collect(),
                None,
            )
        })
        .collect())
}

/// Materialize one canonical layer as a detached UFO glyph.
///
/// This is a read-only format boundary for fixtures and external codecs.
/// The returned value is not editable document state and cannot be reconciled into a Project.
pub fn glyph_from_layer(layer: LayerView<'_>) -> norad::Glyph {
    crate::font::ufo_codec::encode_layer_view(layer)
}

impl Project {
    /// Materialize one canonical layer as a detached UFO codec value.
    pub fn encode_ufo_layer(&self, name: &str, layer: &LayerId) -> Option<norad::Glyph> {
        crate::font::ufo_codec::encode_layer(self.codec_data(), name, layer)
    }

    /// Materialize one canonical source as a detached UFO codec value.
    pub fn encode_ufo_source(&self, source: SourceId) -> Option<norad::Font> {
        crate::font::ufo_codec::encode_source(self.codec_data(), source)
    }

    /// Materialize canonical interpolation at an arbitrary normalized location.
    pub fn try_encode_interpolated_ufo_at(
        &self,
        glyph_name: &str,
        location: &crate::font::var_model::Location,
    ) -> Result<norad::Glyph, String> {
        let (interpolated, base) = self.interpolation_codec_parts(glyph_name, location)?;
        crate::font::ufo_codec::encode_interpolated(&interpolated, base)
    }

    /// Materialize canonical interpolation, suppressing an explicit interpolation error.
    pub fn encode_interpolated_ufo_at(
        &self,
        glyph_name: &str,
        location: &crate::font::var_model::Location,
    ) -> Option<norad::Glyph> {
        self.try_encode_interpolated_ufo_at(glyph_name, location)
            .ok()
    }

    /// Materialize interpolation at the current non-default preview location.
    pub fn encode_current_interpolated_ufo(&self, glyph_name: &str) -> Option<norad::Glyph> {
        if self.location.values().all(|value| value.abs() < 1e-9) {
            return None;
        }
        self.encode_interpolated_ufo_at(glyph_name, &self.location)
    }
}

impl crate::font::experiments::Experiment {
    /// Materialize this isolated version at an explicit UFO export or proof boundary.
    pub fn encode_ufo_source(&self, project: &Project) -> Result<norad::Font, String> {
        let mut font = project
            .encode_ufo_source(self.root)
            .ok_or("the experiment's source is no longer loaded")?;
        for (address, draft) in self.layer_drafts() {
            let layer = font
                .layers
                .get_or_create_layer(&address.layer.name)
                .map_err(|error| error.to_string())?;
            layer.insert_glyph(glyph_from_layer(draft.view()));
        }
        crate::font::ufo_codec::encode_font_metadata(&mut font, self.font_metadata())
            .map_err(|error| error.to_string())?;
        Ok(font)
    }
}

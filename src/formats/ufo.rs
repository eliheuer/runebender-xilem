// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Explicit transient UFO codec values.

use crate::document::project::Project;
use crate::document::variable::{LayerId, SourceId};
use crate::document::{ImportedContours, LayerView};

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
    crate::document::ufo_codec::encode_layer_view(layer)
}

impl Project {
    /// Materialize one canonical layer as a detached UFO codec value.
    pub fn encode_ufo_layer(&self, name: &str, layer: &LayerId) -> Option<norad::Glyph> {
        crate::document::ufo_codec::encode_layer(self.codec_data(), name, layer)
    }

    /// Materialize one canonical source as a detached UFO codec value.
    pub fn encode_ufo_source(&self, source: SourceId) -> Option<norad::Font> {
        crate::document::ufo_codec::encode_source(self.codec_data(), source)
    }

    /// Materialize canonical interpolation at an arbitrary normalized location.
    pub fn try_encode_interpolated_ufo_at(
        &self,
        glyph_name: &str,
        location: &crate::document::var_model::Location,
    ) -> Result<norad::Glyph, String> {
        let (interpolated, base) = self.interpolation_codec_parts(glyph_name, location)?;
        crate::document::ufo_codec::encode_interpolated(&interpolated, base)
    }

    /// Materialize canonical interpolation, suppressing an explicit interpolation error.
    pub fn encode_interpolated_ufo_at(
        &self,
        glyph_name: &str,
        location: &crate::document::var_model::Location,
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

impl crate::document::experiments::Experiment {
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
        crate::document::font_ops::write_canonical_metadata_to_ufo(&mut font, self.font_metadata())
            .map_err(|error| error.to_string())?;
        Ok(font)
    }
}

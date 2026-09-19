// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical single-source document construction.

use std::collections::BTreeSet;

use super::{LayerId, SourceId, SourceMetadata, VariableData, VariableGlyph};
use crate::document::model::glyph_metadata::CanonicalSourceGlyphMetadata;
use crate::document::new_font::NewFontSpecification;

const DEFAULT_LAYER_NAME: &str = "public.default";

impl VariableData {
    /// Construct a new canonical source before creating its UFO compatibility template.
    pub(in crate::document) fn from_new_font(
        specification: NewFontSpecification,
    ) -> Result<Self, String> {
        specification
            .font_info
            .validate()
            .map_err(|error| error.to_string())?;
        let mut names = BTreeSet::new();
        for glyph in &specification.glyphs {
            crate::document::canonical_metadata::validate_name(&glyph.name)
                .map_err(|error| error.to_string())?;
            if !glyph.width.is_finite() || glyph.width < 0.0 {
                return Err(format!("{} has an invalid advance", glyph.name));
            }
            if !names.insert(glyph.name.as_str()) {
                return Err(format!("duplicate template glyph {:?}", glyph.name));
            }
        }

        let source = SourceId(0);
        let layer_id = LayerId {
            source,
            name: DEFAULT_LAYER_NAME.into(),
        };
        let mut data = Self::default();
        data.source_ids.push(source);
        data.next_source = 1;
        data.source_metadata.insert(
            source,
            SourceMetadata {
                feature_text: String::new(),
                font_metadata: crate::document::canonical_metadata::CanonicalFontMetadata::default(
                ),
                font_info: specification.font_info,
            },
        );
        for glyph in specification.glyphs {
            let (layer, preserved) = crate::document::babelfont::glyph_transactions::empty_layer(
                &glyph.name,
                &layer_id,
                true,
                glyph.width,
                glyph.codepoint,
            );
            let mut variable = VariableGlyph::default();
            variable.layers.insert(layer_id.clone(), preserved);
            variable
                .source_metadata
                .insert(source, CanonicalSourceGlyphMetadata::default());
            let mut geometry = babelfont::Glyph::new(&glyph.name);
            geometry.layers.push(layer);
            geometry.codepoints = glyph.codepoint.into_iter().map(u32::from).collect();
            data.glyphs.insert(glyph.name, variable);
            data.font.glyphs.0.push(geometry);
        }

        // Source-format preservation is created only after canonical state.
        let format = crate::document::source_format::SourceFormatData::default();
        debug_assert_eq!(
            format.default_layer_name(),
            DEFAULT_LAYER_NAME,
            "blank source-format data must use the canonical default layer name"
        );
        data.source_formats.insert(source, format);
        Ok(data)
    }

    /// Decode one validated UFO boundary into canonical ownership without a Master intermediary.
    pub(in crate::document) fn from_ufo_boundary(font: &norad::Font) -> Result<Self, String> {
        Self::from_ufo_boundaries([font])
    }

    /// Decode validated UFO sources into one canonical document before making projections.
    pub(in crate::document) fn from_ufo_boundaries<'a>(
        fonts: impl IntoIterator<Item = &'a norad::Font>,
    ) -> Result<Self, String> {
        let mut data = Self::default();
        for font in fonts {
            validate_ufo_boundary(font)?;
            let source = SourceId(data.source_ids.len());
            data.source_ids.push(source);
            data.update_source(source, font);
        }
        if data.source_ids.is_empty() {
            return Err("canonical import needs at least one UFO source".into());
        }
        data.next_source = data.source_ids.len();
        Ok(data)
    }
}

fn validate_ufo_boundary(font: &norad::Font) -> Result<(), String> {
    crate::document::font_ops::canonical_metadata_from_ufo(font)
        .map_err(|error| error.to_string())?;
    crate::document::model::font_info::CanonicalFontInfo::from_ufo(&font.font_info)
        .map_err(|error| error.to_string())?;
    for name in font
        .default_layer()
        .iter()
        .map(|glyph| glyph.name().as_str())
    {
        crate::document::model::glyph_metadata::canonical_glyph_metadata_from_ufo(font, name)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical single-source document construction.

use std::collections::BTreeSet;

use super::{LayerId, SourceId, SourceMetadata, VariableData, VariableGlyph};
use crate::font::model::glyph_metadata::CanonicalSourceGlyphMetadata;
use crate::font::new_font::NewFontSpecification;

const DEFAULT_LAYER_NAME: &str = "public.default";

impl VariableData {
    /// Construct a new canonical source before creating its UFO compatibility template.
    pub(in crate::font) fn from_new_font(
        specification: NewFontSpecification,
    ) -> Result<Self, String> {
        specification
            .font_info
            .validate()
            .map_err(|error| error.to_string())?;
        let mut names = BTreeSet::new();
        for glyph in &specification.glyphs {
            crate::font::canonical_metadata::validate_name(&glyph.name)
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
                font_metadata: crate::font::canonical_metadata::CanonicalFontMetadata::default(),
                font_info: specification.font_info,
            },
        );
        for glyph in specification.glyphs {
            let (layer, preserved) = crate::font::babelfont::glyph_transactions::empty_layer(
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
        let format = crate::font::persistence::source_format::SourceFormatData::default();
        debug_assert_eq!(
            format.default_layer_name(),
            DEFAULT_LAYER_NAME,
            "blank source-format data must use the canonical default layer name"
        );
        data.source_formats.insert(source, format);
        Ok(data)
    }
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Transient UFO decoding and projection for canonical documents.
//!
//! This is the only whole-font bridge between Norad's UFO object model and the live document.
//! Values cross it once during import or are freshly materialized for serialization; none are
//! retained as editable state and there is no projection-to-document reconciliation path.

use super::variable::{LayerId, SourceId, SourceMetadata, VariableData};

pub(super) fn decode_source(font: &norad::Font) -> Result<VariableData, String> {
    decode_sources([font])
}

pub(super) fn decode_sources<'a>(
    fonts: impl IntoIterator<Item = &'a norad::Font>,
) -> Result<VariableData, String> {
    let mut data = VariableData::default();
    for font in fonts {
        validate_source(font)?;
        let source = SourceId(data.source_ids.len());
        data.source_ids.push(source);
        data.source_metadata.insert(
            source,
            SourceMetadata {
                feature_text: font.features.clone(),
                font_metadata: super::font_ops::canonical_metadata_from_ufo(font)
                    .map_err(|error| error.to_string())?,
                font_info: super::model::font_info::CanonicalFontInfo::from_ufo(&font.font_info)
                    .map_err(|error| error.to_string())?,
            },
        );
        data.source_formats.insert(
            source,
            super::source_format::SourceFormatData::from_ufo(font),
        );

        let default_layer_name = font.default_layer().name().as_str();
        for layer in font.layers.iter() {
            let id = LayerId {
                source,
                name: layer.name().to_string(),
            };
            for payload in layer.iter() {
                let name = payload.name().as_str();
                let default = layer.name().as_str() == default_layer_name;
                let (geometry, preserved) = super::babelfont::layer_from_ufo(payload, &id, default);
                let glyph = data.glyphs.entry(name.to_owned()).or_default();
                glyph.layers.insert(id.clone(), preserved);
                if default {
                    glyph.source_metadata.insert(
                        source,
                        super::model::glyph_metadata::canonical_glyph_metadata_from_ufo(font, name)
                            .map_err(|error| error.to_string())?
                            .source()
                            .clone(),
                    );
                }
                if data.font.glyphs.get(name).is_none() {
                    data.font.glyphs.0.push(babelfont::Glyph::new(name));
                }
                let target = data.font.glyphs.get_mut(name).expect("inserted glyph");
                target.layers.push(geometry);
                target.codepoints = payload.codepoints.iter().map(u32::from).collect();
            }
        }
        data.revision = data.revision.wrapping_add(1);
    }
    if data.source_ids.is_empty() {
        return Err("canonical import needs at least one UFO source".into());
    }
    data.next_source = data.source_ids.len();
    Ok(data)
}

fn validate_source(font: &norad::Font) -> Result<(), String> {
    super::font_ops::canonical_metadata_from_ufo(font).map_err(|error| error.to_string())?;
    super::model::font_info::CanonicalFontInfo::from_ufo(&font.font_info)
        .map_err(|error| error.to_string())?;
    for name in font
        .default_layer()
        .iter()
        .map(|glyph| glyph.name().as_str())
    {
        super::model::glyph_metadata::canonical_glyph_metadata_from_ufo(font, name)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn encode_source(data: &VariableData, source: SourceId) -> Option<norad::Font> {
    let mut font = data.source_formats.get(&source)?.to_ufo_template();
    font.features
        .clone_from(&data.source_metadata.get(&source)?.feature_text);
    super::font_ops::write_canonical_metadata_to_ufo(
        &mut font,
        &data.source_metadata.get(&source)?.font_metadata,
    )
    .expect("canonical source metadata must remain writable as UFO");
    data.source_metadata
        .get(&source)?
        .font_info
        .write_to_ufo(&mut font.font_info)
        .expect("canonical font info must remain writable as UFO");
    for (name, glyph) in &data.glyphs {
        for (id, preserved) in &glyph.layers {
            if id.source == source {
                font.layers
                    .get_mut(&id.name)
                    .expect("every stored layer has persistence metadata")
                    .insert_glyph(super::babelfont::project_layer(
                        data.font
                            .glyphs
                            .get(name)?
                            .get_layer(&super::babelfont::layer_key(id))?,
                        preserved,
                    ));
            }
        }
    }
    for (name, glyph) in &data.glyphs {
        let Some(metadata) = glyph.source_metadata.get(&source) else {
            continue;
        };
        let Some(payload) = font.get_glyph(name) else {
            continue;
        };
        let boundary = super::model::glyph_metadata::CanonicalGlyphMetadata::new(
            payload.codepoints.iter(),
            payload.note.clone(),
            metadata.exported(),
            metadata.category().cloned(),
        );
        super::model::glyph_metadata::write_canonical_glyph_metadata_to_ufo(
            &mut font, name, &boundary,
        )
        .expect("canonical glyph metadata must remain writable as UFO");
    }
    Some(font)
}

pub(crate) fn encode_layer(data: &VariableData, name: &str, id: &LayerId) -> Option<norad::Glyph> {
    let preserved = data.glyphs.get(name)?.layers.get(id)?;
    let layer = data
        .font
        .glyphs
        .get(name)?
        .get_layer(&super::babelfont::layer_key(id))?;
    Some(super::babelfont::project_layer(layer, preserved))
}

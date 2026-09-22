// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Transient UFO decoding and projection for canonical documents.
//!
//! This is the only whole-font bridge between Norad's UFO object model and the live document.
//! Values cross it once during import or are freshly materialized for serialization; none are
//! retained as editable state and there is no projection-to-document reconciliation path.

use super::super::{
    LayerView, babelfont as font_babelfont, font_ops, interpolation, model,
    variable::{LayerId, SourceId, SourceMetadata, VariableData},
};

pub(crate) fn decode_font_metadata(
    font: &norad::Font,
) -> Result<font_ops::CanonicalFontMetadata, font_ops::CanonicalMetadataError> {
    let groups = font
        .groups
        .iter()
        .map(|(name, members)| {
            (
                name.to_string(),
                members.iter().map(ToString::to_string).collect(),
            )
        })
        .collect();
    let kerning = font
        .kerning
        .iter()
        .map(|(left, row)| {
            (
                left.to_string(),
                row.iter()
                    .map(|(right, value)| (right.to_string(), *value))
                    .collect(),
            )
        })
        .collect();
    font_ops::CanonicalFontMetadata::from_raw(groups, kerning)
}

pub(crate) fn encode_font_metadata(
    font: &mut norad::Font,
    metadata: &font_ops::CanonicalFontMetadata,
) -> Result<bool, font_ops::CanonicalMetadataError> {
    let mut groups = norad::Groups::default();
    for (name, members) in metadata.groups() {
        let name = norad::Name::new(name)
            .map_err(|_| font_ops::CanonicalMetadataError::InvalidName(name.clone()))?;
        let members = members
            .iter()
            .map(|member| {
                norad::Name::new(member)
                    .map_err(|_| font_ops::CanonicalMetadataError::InvalidName(member.clone()))
            })
            .collect::<Result<_, _>>()?;
        groups.insert(name, members);
    }
    let mut kerning = norad::Kerning::default();
    for (left, row) in metadata.raw_kerning() {
        let left_name = norad::Name::new(&left)
            .map_err(|_| font_ops::CanonicalMetadataError::InvalidName(left.clone()))?;
        let mut output_row = std::collections::BTreeMap::new();
        for (right, value) in row {
            let right_name = norad::Name::new(&right)
                .map_err(|_| font_ops::CanonicalMetadataError::InvalidName(right.clone()))?;
            output_row.insert(right_name, value);
        }
        kerning.insert(left_name, output_row);
    }
    if font.groups == groups && font.kerning == kerning {
        return Ok(false);
    }
    font.groups = groups;
    font.kerning = kerning;
    Ok(true)
}

pub(in crate::font) fn decode_source(font: &norad::Font) -> Result<VariableData, String> {
    decode_sources([font])
}

pub(in crate::font) fn decode_sources<'a>(
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
                font_metadata: decode_font_metadata(font).map_err(|error| error.to_string())?,
                font_info: model::font_info::CanonicalFontInfo::from_ufo(&font.font_info)
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
                let (geometry, preserved) = font_babelfont::layer_from_ufo(payload, &id, default);
                let glyph = data.glyphs.entry(name.to_owned()).or_default();
                glyph.layers.insert(id.clone(), preserved);
                if default {
                    glyph.source_metadata.insert(
                        source,
                        model::glyph_metadata::canonical_glyph_metadata_from_ufo(font, name)
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
    decode_font_metadata(font).map_err(|error| error.to_string())?;
    model::font_info::CanonicalFontInfo::from_ufo(&font.font_info)
        .map_err(|error| error.to_string())?;
    for name in font
        .default_layer()
        .iter()
        .map(|glyph| glyph.name().as_str())
    {
        model::glyph_metadata::canonical_glyph_metadata_from_ufo(font, name)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn encode_source(data: &VariableData, source: SourceId) -> Option<norad::Font> {
    let mut font = data.source_formats.get(&source)?.to_ufo_template();
    font.features
        .clone_from(&data.source_metadata.get(&source)?.feature_text);
    encode_font_metadata(&mut font, &data.source_metadata.get(&source)?.font_metadata)
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
                    .insert_glyph(font_babelfont::project_layer(
                        data.font
                            .glyphs
                            .get(name)?
                            .get_layer(&font_babelfont::layer_key(id))?,
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
        let boundary = model::glyph_metadata::CanonicalGlyphMetadata::new(
            payload.codepoints.iter(),
            payload.note.clone(),
            metadata.exported(),
            metadata.category().cloned(),
        );
        model::glyph_metadata::write_canonical_glyph_metadata_to_ufo(&mut font, name, &boundary)
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
        .get_layer(&font_babelfont::layer_key(id))?;
    Some(font_babelfont::project_layer(layer, preserved))
}

pub(crate) fn encode_layer_view(layer: LayerView<'_>) -> norad::Glyph {
    let (geometry, preserved) = layer.codec_parts();
    font_babelfont::project_layer(geometry, preserved)
}

pub(crate) fn encode_interpolated(
    output: &interpolation::InterpolatedLayer,
    base: LayerView<'_>,
) -> Result<norad::Glyph, String> {
    let mut glyph = encode_layer_view(base);
    if output.glyph_name != glyph.name().as_str()
        || output
            .codepoints
            .iter()
            .copied()
            .collect::<norad::Codepoints>()
            != glyph.codepoints
        || output.note != glyph.note
    {
        return Err("canonical interpolation changed default-layer metadata".into());
    }
    glyph.width = output.width;
    glyph.height = output.height;
    for ((contour, output), source) in glyph
        .contours
        .iter_mut()
        .zip(output.contours())
        .zip(base.contours())
    {
        if output.id != source.id()
            || output.closed != source.is_closed()
            || output.hyper != source.is_hyper()
        {
            return Err("canonical interpolation changed default contour structure".into());
        }
        for ((point, output), source) in contour
            .points
            .iter_mut()
            .zip(&output.points)
            .zip(source.points())
        {
            if output.id != source.id()
                || output.point_type != source.point_type()
                || output.smooth != source.is_smooth()
                || output.name.as_deref() != source.name()
            {
                return Err("canonical interpolation changed default point structure".into());
            }
            point.x = output.position.x;
            point.y = output.position.y;
        }
    }
    for ((anchor, output), source) in glyph
        .anchors
        .iter_mut()
        .zip(&output.anchors)
        .zip(base.anchors())
    {
        if output.id != source.id() || output.name != source.name() {
            return Err("canonical interpolation changed default anchor structure".into());
        }
        anchor.x = output.position.x;
        anchor.y = output.position.y;
    }
    for ((component, output), source) in glyph
        .components
        .iter_mut()
        .zip(output.components())
        .zip(base.components())
    {
        if output.id != source.id() || output.reference != source.reference() {
            return Err("canonical interpolation changed default component structure".into());
        }
        let [x_scale, xy_scale, yx_scale, y_scale, x_offset, y_offset] =
            output.transform.as_coeffs();
        component.transform = norad::AffineTransform {
            x_scale,
            xy_scale,
            yx_scale,
            y_scale,
            x_offset,
            y_offset,
        };
    }
    Ok(glyph)
}

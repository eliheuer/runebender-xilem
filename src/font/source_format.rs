// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Source-format data that is not canonical editable font state.
//!
//! Canonical glyph geometry, metadata, features, groups and kerning live in the document model.
//! This record retains only glyph-free UFO structure and opaque resources needed to reconstruct a
//! transient Norad value at a persistence boundary.

use std::path::{Path, PathBuf};

/// Glyph-free UFO structure and opaque resources retained for exact persistence.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SourceFormatData {
    meta: norad::MetaInfo,
    font_info: norad::FontInfo,
    layers: norad::LayerContents,
    lib: norad::Plist,
    data: norad::datastore::DataStore,
    images: norad::datastore::ImageStore,
}

impl Default for SourceFormatData {
    fn default() -> Self {
        Self::from_ufo(&norad::Font::new())
    }
}

impl SourceFormatData {
    /// Capture only the source-format fields not owned by the canonical document.
    pub(super) fn from_ufo(font: &norad::Font) -> Self {
        let mut layers = font.layers.clone();
        for layer in layers.iter_mut() {
            layer.clear();
        }
        let mut font_info = font.font_info.clone();
        super::model::font_info::clear_canonical_font_info_fields(&mut font_info);
        Self {
            meta: font.meta.clone(),
            font_info,
            layers,
            lib: font.lib.clone(),
            data: font.data.clone(),
            images: font.images.clone(),
        }
    }

    /// Materialize a glyph-free UFO codec value for staged export.
    pub(super) fn to_ufo_template(&self) -> norad::Font {
        let mut font = norad::Font::new();
        font.meta.clone_from(&self.meta);
        font.font_info.clone_from(&self.font_info);
        font.layers.clone_from(&self.layers);
        font.lib.clone_from(&self.lib);
        font.data.clone_from(&self.data);
        font.images.clone_from(&self.images);
        font
    }

    pub(super) fn default_layer_name(&self) -> &str {
        self.layers.default_layer().name().as_str()
    }

    pub(super) fn contains_layer(&self, name: &str) -> bool {
        self.layers.get(name).is_some()
    }

    pub(super) fn layer_names(&self) -> impl Iterator<Item = &str> {
        self.layers.names().map(norad::Name::as_str)
    }

    pub(super) fn ensure_layer(&mut self, name: &str) -> Result<(), String> {
        self.layers
            .get_or_create_layer(name)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub(super) fn retain_default_layer(&mut self) {
        self.layers.retain(|_| false);
    }

    pub(super) fn remove_empty_layer(&mut self, name: &str) -> bool {
        if self.default_layer_name() == name
            || self.layers.get(name).is_none_or(|layer| !layer.is_empty())
        {
            return false;
        }
        self.layers.remove(name).is_some()
    }

    pub(super) fn image_bytes(&self, path: &Path) -> Result<Option<std::sync::Arc<[u8]>>, String> {
        self.images
            .get(path)
            .transpose()
            .map_err(|error| error.to_string())
    }

    pub(super) fn install_image(&mut self, path: PathBuf, bytes: Vec<u8>) -> Result<(), String> {
        self.images
            .insert(path, bytes)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use norad::{Color, Font, Glyph};

    use super::SourceFormatData;

    #[test]
    fn record_is_glyph_free_and_preserves_exact_format_structure() {
        let mut font = Font::new();
        font.meta.creator = Some("com.runebender.test".into());
        font.font_info.family_name = Some("Canonical owner".into());
        font.lib.insert("com.example.font".into(), "opaque".into());
        font.default_layer_mut().color = Some(Color::new(0.1, 0.2, 0.3, 0.4).unwrap());
        font.default_layer_mut()
            .lib
            .insert("com.example.default".into(), 1.into());
        font.default_layer_mut().insert_glyph(Glyph::new("A"));
        let background = font.layers.new_layer("public.background").unwrap();
        background.color = Some(Color::new(0.4, 0.3, 0.2, 0.1).unwrap());
        background
            .lib
            .insert("com.example.background".into(), 2.into());
        background.insert_glyph(Glyph::new("B"));
        font.features = "feature liga { sub A A by A; } liga;".into();
        font.data
            .insert(PathBuf::from("com.example/payload.bin"), vec![1, 2, 3])
            .unwrap();
        let image = include_bytes!("../../tests/fixtures/variable/reference.png").to_vec();
        font.images
            .insert(PathBuf::from("reference.png"), image.clone())
            .unwrap();

        let expected_layers = font
            .layers
            .iter()
            .map(|layer| {
                (
                    layer.name().to_string(),
                    layer.path().to_path_buf(),
                    layer.color,
                    layer.lib.clone(),
                )
            })
            .collect::<Vec<_>>();
        let format = SourceFormatData::from_ufo(&font);
        let template = format.to_ufo_template();

        assert!(template.layers.iter().all(norad::Layer::is_empty));
        assert_eq!(
            template
                .layers
                .iter()
                .map(|layer| {
                    (
                        layer.name().to_string(),
                        layer.path().to_path_buf(),
                        layer.color,
                        layer.lib.clone(),
                    )
                })
                .collect::<Vec<_>>(),
            expected_layers
        );
        assert_eq!(template.meta, font.meta);
        assert_eq!(template.lib, font.lib);
        assert!(template.font_info.family_name.is_none());
        assert!(template.features.is_empty());
        assert_eq!(
            template
                .data
                .get(Path::new("com.example/payload.bin"))
                .unwrap()
                .unwrap()
                .as_ref(),
            [1, 2, 3]
        );
        assert_eq!(
            template
                .images
                .get(Path::new("reference.png"))
                .unwrap()
                .unwrap()
                .as_ref(),
            image
        );
    }
}

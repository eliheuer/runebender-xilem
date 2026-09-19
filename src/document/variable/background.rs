// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Atomic canonical background-layer staging.

use super::{CanonicalSourceStructureSnapshot, LayerId, SourceId, VariableData};
use crate::document::babelfont::{LayerEditDraft, LayerView, copy_contours_only, layer_key};

const BACKGROUND_NAMES: [&str; 2] = ["public.background", "background"];

impl VariableData {
    pub(in crate::document) fn background_layer_id(&self, source: SourceId) -> Option<LayerId> {
        let format = self.source_formats.get(&source)?;
        let name = BACKGROUND_NAMES
            .into_iter()
            .find(|name| format.contains_layer(name))
            .unwrap_or(BACKGROUND_NAMES[0]);
        Some(LayerId {
            source,
            name: name.into(),
        })
    }
}

impl CanonicalSourceStructureSnapshot {
    pub(in crate::document) fn background_layer_id(
        &self,
        source: SourceId,
    ) -> Result<LayerId, String> {
        let format = self.source_formats.get(&source).ok_or("unknown source")?;
        let name = BACKGROUND_NAMES
            .into_iter()
            .find(|name| format.contains_layer(name))
            .unwrap_or(BACKGROUND_NAMES[0]);
        Ok(LayerId {
            source,
            name: name.into(),
        })
    }

    pub(in crate::document) fn copy_layer_to_background(
        &mut self,
        glyph: &str,
        foreground: &LayerId,
    ) -> Result<(LayerId, bool), String> {
        let background = self.background_layer_id(foreground.source)?;
        if foreground == &background {
            return Err("the background cannot copy itself".into());
        }
        let (source_layer, source_preserved) = self
            .layer_parts(glyph, foreground)
            .ok_or("missing foreground glyph layer")?;
        if let Some((target_layer, target_preserved)) = self.layer_parts(glyph, &background) {
            let source = LayerView::new(&source_layer, &source_preserved);
            let mut draft = LayerEditDraft::new(target_layer, target_preserved);
            if !draft
                .replace_layer_contours(source)
                .map_err(|error| error.to_string())?
            {
                return Ok((background, false));
            }
            self.install_layer(glyph, &background, draft.into_parts())?;
            return Ok((background, true));
        }

        self.source_formats
            .get_mut(&foreground.source)
            .expect("validated source")
            .ensure_layer(&background.name)?;
        let copied = copy_contours_only(&source_layer, &source_preserved, &background);
        self.install_layer(glyph, &background, copied)?;
        Ok((background, true))
    }

    pub(in crate::document) fn swap_layer_with_background(
        &mut self,
        glyph: &str,
        foreground: &LayerId,
    ) -> Result<(LayerId, bool), String> {
        let background = self.background_layer_id(foreground.source)?;
        if foreground == &background {
            return Err("the background cannot swap with itself".into());
        }
        let (foreground_layer, foreground_preserved) = self
            .layer_parts(glyph, foreground)
            .ok_or("missing foreground glyph layer")?;
        let (background_layer, background_preserved) = self
            .layer_parts(glyph, &background)
            .ok_or("missing background glyph layer")?;
        let foreground_view = LayerView::new(&foreground_layer, &foreground_preserved);
        let background_view = LayerView::new(&background_layer, &background_preserved);
        let mut foreground_draft =
            LayerEditDraft::new(foreground_layer.clone(), foreground_preserved.clone());
        let mut background_draft =
            LayerEditDraft::new(background_layer.clone(), background_preserved.clone());
        let foreground_changed = foreground_draft
            .replace_layer_contours_only(background_view)
            .map_err(|error| error.to_string())?;
        let background_contours_changed = background_draft
            .replace_layer_contours_only(foreground_view)
            .map_err(|error| error.to_string())?;
        let background_width_changed = background_draft
            .set_width(foreground_view.width())
            .map_err(|error| error.to_string())?;
        let background_changed = background_contours_changed || background_width_changed;
        if !foreground_changed && !background_changed {
            return Ok((background, false));
        }
        self.install_layer(glyph, foreground, foreground_draft.into_parts())?;
        self.install_layer(glyph, &background, background_draft.into_parts())?;
        Ok((background, true))
    }

    pub(in crate::document) fn clear_background_layer(
        &mut self,
        glyph: &str,
        source: SourceId,
    ) -> Result<(LayerId, bool), String> {
        let background = self.background_layer_id(source)?;
        let Some(variable_glyph) = self.glyphs.get_mut(glyph) else {
            return Ok((background, false));
        };
        if variable_glyph.layers.remove(&background).is_none() {
            return Ok((background, false));
        }
        let geometry = self
            .glyph_geometry
            .get_mut(glyph)
            .ok_or("background preservation has no geometry")?;
        let key = layer_key(&background);
        geometry
            .layers
            .retain(|layer| layer.id.as_deref() != Some(key.as_str()));
        Ok((background, true))
    }

    fn layer_parts(
        &self,
        glyph: &str,
        layer: &LayerId,
    ) -> Option<(babelfont::Layer, super::super::babelfont::LayerPreservation)> {
        let preserved = self.glyphs.get(glyph)?.layers.get(layer)?.clone();
        let geometry = self
            .glyph_geometry
            .get(glyph)?
            .get_layer(&layer_key(layer))?
            .clone();
        Some((geometry, preserved))
    }

    fn install_layer(
        &mut self,
        glyph: &str,
        layer: &LayerId,
        parts: (babelfont::Layer, super::super::babelfont::LayerPreservation),
    ) -> Result<(), String> {
        let (geometry, preserved) = parts;
        let variable_glyph = self.glyphs.get_mut(glyph).ok_or("missing glyph")?;
        variable_glyph.layers.insert(layer.clone(), preserved);
        let glyph_geometry = self
            .glyph_geometry
            .get_mut(glyph)
            .ok_or("missing glyph geometry")?;
        let key = layer_key(layer);
        if let Some(existing) = glyph_geometry.get_layer_mut(&key) {
            *existing = geometry;
        } else {
            glyph_geometry.layers.push(geometry);
        }
        Ok(())
    }
}

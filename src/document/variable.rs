// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Glyph-first storage and scoped compatibility edits.
//!
//! A source is a location and a persistence destination, not a separate font.
//! Each variable glyph owns its source and auxiliary layers. UFO projections are
//! disposable views for existing outline tools; guards reconcile all their edits,
//! including history replay and metadata changes, before another Project operation.
//! Exact UFO payloads remain here until the Babelfont adapter can represent them
//! without narrowing numbers or dropping metadata.

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};

use super::project::Master;

/// Stable source identity within an open project.
///
/// Source order is fixed for the lifetime of the project; reload creates a new project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(pub usize);

/// A layer identity independent of glyph name or current editor selection.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LayerId {
    /// Source supplying this layer's metadata and persistence destination.
    pub source: SourceId,
    /// Original layer name, including background and glyph-specific layers.
    pub name: String,
}

/// A layer's participation in one glyph's variation model.
/// Auxiliary layers remain editable without becoming interpolation sources.
#[derive(Clone, Debug)]
pub struct GlyphSource {
    /// Stable address of the layer supplying this glyph at the location.
    pub layer: LayerId,
    /// Normalized design coordinates by axis name, with absent axes at zero.
    pub location: super::var_model::Location,
}

/// One glyph across all sources, including sparse and auxiliary layers.
#[derive(Clone, Debug, Default)]
pub struct VariableGlyph {
    layers: BTreeMap<LayerId, norad::Glyph>,
}

impl VariableGlyph {
    /// All of this glyph's layers, with stable addresses.
    pub fn layers(&self) -> impl Iterator<Item = (&LayerId, &norad::Glyph)> {
        self.layers.iter()
    }

    /// The exact editable payload at a layer address.
    pub fn layer(&self, id: &LayerId) -> Option<&norad::Glyph> {
        self.layers.get(id)
    }
}

/// Canonical glyph ownership plus glyph-free UFO persistence metadata.
#[derive(Debug, Default)]
pub(super) struct VariableData {
    pub(super) glyphs: BTreeMap<String, VariableGlyph>,
    pub(super) histories: BTreeMap<LayerId, super::history::EditHistory>,
    templates: Vec<norad::Font>,
}

impl VariableData {
    pub(super) fn from_sources(sources: &[Master]) -> Self {
        let mut data = Self::default();
        for (index, source) in sources.iter().enumerate() {
            data.update_source(SourceId(index), &source.font);
        }
        data
    }

    fn update_source(&mut self, source: SourceId, font: &norad::Font) -> bool {
        let mut changed = false;
        // Remove deleted layers/glyphs without disturbing any other source.
        for (name, glyph) in &mut self.glyphs {
            let before = glyph.layers.len();
            glyph.layers.retain(|id, _| {
                id.source != source
                    || font
                        .layers
                        .get(&id.name)
                        .is_some_and(|layer| layer.get_glyph(name).is_some())
            });
            changed |= before != glyph.layers.len();
        }
        self.glyphs.retain(|_, glyph| !glyph.layers.is_empty());
        for layer in font.layers.iter() {
            let id = LayerId {
                source,
                name: layer.name().to_string(),
            };
            for payload in layer.iter() {
                let glyph = self.glyphs.entry(payload.name().to_string()).or_default();
                if glyph.layers.get(&id) != Some(payload) {
                    glyph.layers.insert(id.clone(), payload.clone());
                    changed = true;
                }
            }
        }
        // A template contains no glyphs. Preserve layer ordering, paths, color,
        // libs, images, data, feature text, groups, and all font-info fields.
        let previous = self.templates.get(source.0);
        let mut template = previous.cloned().unwrap_or_default();
        let same_layers = template.layers.len() == font.layers.len()
            && template
                .layers
                .iter()
                .zip(font.layers.iter())
                .all(|(a, b)| a.name() == b.name() && a.path() == b.path());
        if !same_layers {
            template.layers = font.layers.clone();
            for layer in template.layers.iter_mut() {
                layer.clear();
            }
        }
        for (target, original) in template.layers.iter_mut().zip(font.layers.iter()) {
            target.lib.clone_from(&original.lib);
            target.color = original.color;
        }
        template.meta.clone_from(&font.meta);
        template.font_info.clone_from(&font.font_info);
        template.lib.clone_from(&font.lib);
        template.groups.clone_from(&font.groups);
        template.kerning.clone_from(&font.kerning);
        template.features.clone_from(&font.features);
        template.data.clone_from(&font.data);
        template.images.clone_from(&font.images);
        changed |= previous != Some(&template);
        if let Some(slot) = self.templates.get_mut(source.0) {
            *slot = template;
        } else {
            assert_eq!(source.0, self.templates.len(), "sources append in order");
            self.templates.push(template);
        }
        changed
    }

    pub(super) fn source_font(&self, source: SourceId) -> Option<norad::Font> {
        let mut font = self.templates.get(source.0)?.clone();
        for glyph in self.glyphs.values() {
            for (id, payload) in &glyph.layers {
                if id.source == source {
                    font.layers
                        .get_mut(&id.name)
                        .expect("every stored layer has persistence metadata")
                        .insert_glyph(payload.clone());
                }
            }
        }
        Some(font)
    }
}

/// A scoped edit of a source projection; dropping it commits to the variable project.
#[derive(Debug)]
pub struct SourceEdit<'a> {
    pub(super) source: &'a mut Master,
    pub(super) data: &'a mut VariableData,
    pub(super) id: SourceId,
}

impl<'a> SourceEdit<'a> {
    /// Narrow a compatibility edit to its UFO payload.
    pub fn into_font(self) -> SourceFontEdit<'a> {
        SourceFontEdit(self)
    }
}

impl Deref for SourceEdit<'_> {
    type Target = Master;

    fn deref(&self) -> &Self::Target {
        self.source
    }
}

impl DerefMut for SourceEdit<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.source
    }
}

impl Drop for SourceEdit<'_> {
    fn drop(&mut self) {
        self.source.dirty |= self.data.update_source(self.id, &self.source.font);
    }
}

/// Scoped UFO compatibility access used by existing editing algorithms.
#[derive(Debug)]
pub struct SourceFontEdit<'a>(SourceEdit<'a>);

impl Deref for SourceFontEdit<'_> {
    type Target = norad::Font;

    fn deref(&self) -> &Self::Target {
        &self.0.font
    }
}

impl DerefMut for SourceFontEdit<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0.dirty = true;
        &mut self.0.font
    }
}

/// A batch of source edits committed together when the scope ends.
#[derive(Debug)]
pub struct SourcesEdit<'a> {
    pub(super) sources: &'a mut [Master],
    pub(super) data: &'a mut VariableData,
}

impl Deref for SourcesEdit<'_> {
    type Target = [Master];

    fn deref(&self) -> &Self::Target {
        self.sources
    }
}

impl DerefMut for SourcesEdit<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.sources
    }
}

impl Drop for SourcesEdit<'_> {
    fn drop(&mut self) {
        for (index, source) in self.sources.iter_mut().enumerate() {
            source.dirty |= self.data.update_source(SourceId(index), &source.font);
        }
    }
}

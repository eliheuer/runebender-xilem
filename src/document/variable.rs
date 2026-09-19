// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Glyph-first storage and scoped compatibility edits.
//!
//! A source is a location and a persistence destination, not a separate font.
//! Each variable glyph owns its source and auxiliary layers. UFO projections are
//! disposable views for existing outline tools; guards reconcile all their edits,
//! including history replay and metadata changes, before another Project operation.
//! Babelfont owns live glyph geometry. Exact UFO projections also retain fields
//! and precision outside Babelfont's schema; saving materializes its geometry
//! through the preserving adapter rather than its lossy UFO converter.

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};

use super::project::Master;

/// Stable source identity within an open project.
///
/// Reordering or removing other sources does not change this identity.
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

/// Stable address of one glyph layer in the canonical document.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlyphLayerAddress {
    /// Current glyph name in the document index.
    pub glyph: String,
    /// Stable source and layer address for that glyph.
    pub layer: LayerId,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct SourceMetadata {
    feature_text: String,
    font_metadata: super::canonical_metadata::CanonicalFontMetadata,
}

/// Owned edit draft for source-wide metadata with canonical ownership.
#[derive(Clone, Debug)]
pub struct SourceMetadataEditDraft {
    metadata: SourceMetadata,
}

impl SourceMetadataEditDraft {
    /// Current OpenType feature text for this source.
    pub fn feature_text(&self) -> &str {
        &self.metadata.feature_text
    }

    /// Current canonical group and kerning metadata for this source.
    pub fn font_metadata(&self) -> &super::canonical_metadata::CanonicalFontMetadata {
        &self.metadata.font_metadata
    }

    /// Set the source's OpenType feature text.
    ///
    /// Returns whether the value changed.
    pub fn set_feature_text(&mut self, text: String) -> bool {
        if self.metadata.feature_text == text {
            return false;
        }
        self.metadata.feature_text = text;
        true
    }

    /// Replace the source's canonical group and kerning metadata.
    ///
    /// Returns whether the value changed.
    pub fn set_font_metadata(
        &mut self,
        metadata: super::canonical_metadata::CanonicalFontMetadata,
    ) -> bool {
        if self.metadata.font_metadata == metadata {
            return false;
        }
        self.metadata.font_metadata = metadata;
        true
    }
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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VariableGlyph {
    layers: BTreeMap<LayerId, super::babelfont::LayerPreservation>,
}

/// Cloneable canonical editing state without UFO format templates or Master projections.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentSnapshot {
    glyph_geometry: babelfont::GlyphList,
    glyphs: BTreeMap<String, VariableGlyph>,
    source_metadata: BTreeMap<SourceId, SourceMetadata>,
    source_ids: Vec<SourceId>,
}

impl DocumentSnapshot {
    /// Stable source identities in current display order.
    pub fn source_ids(&self) -> &[SourceId] {
        &self.source_ids
    }

    /// Every glyph name in canonical document order.
    pub fn glyph_names(&self) -> impl Iterator<Item = &str> {
        self.glyph_geometry
            .0
            .iter()
            .map(|glyph| glyph.name.as_str())
    }

    /// Read one snapshotted canonical layer without constructing UFO values.
    pub fn layer(&self, name: &str, id: &LayerId) -> Option<super::LayerView<'_>> {
        let preserved = self.glyphs.get(name)?.layers.get(id)?;
        let layer = self
            .glyph_geometry
            .get(name)?
            .get_layer(&super::babelfont::layer_key(id))?;
        Some(super::LayerView::new(layer, preserved))
    }

    /// Read snapshotted OpenType feature text for one source.
    pub fn feature_text(&self, source: SourceId) -> Option<&str> {
        Some(&self.source_metadata.get(&source)?.feature_text)
    }

    /// Read snapshotted canonical group and kerning metadata for one source.
    pub fn font_metadata(
        &self,
        source: SourceId,
    ) -> Option<&super::canonical_metadata::CanonicalFontMetadata> {
        Some(&self.source_metadata.get(&source)?.font_metadata)
    }
}

impl VariableGlyph {
    /// Stable addresses of all of this glyph's layers.
    pub fn layer_ids(&self) -> impl Iterator<Item = &LayerId> {
        self.layers.keys()
    }

    /// Whether this glyph has a layer at the stable address.
    pub fn has_layer(&self, id: &LayerId) -> bool {
        self.layers.contains_key(id)
    }
}

/// Read-only access to one glyph and all of its canonical layers.
#[derive(Clone, Copy, Debug)]
pub struct GlyphView<'a> {
    name: &'a str,
    glyph: &'a VariableGlyph,
    data: &'a VariableData,
}

impl<'a> GlyphView<'a> {
    /// Current glyph name in the document index.
    pub fn name(self) -> &'a str {
        self.name
    }

    /// Stable addresses of every source and auxiliary layer for this glyph.
    pub fn layer_ids(self) -> impl Iterator<Item = &'a LayerId> + 'a {
        self.glyph.layers.keys()
    }

    /// Read one canonical layer without materializing a UFO glyph.
    pub fn layer(self, id: &LayerId) -> Option<super::babelfont::LayerView<'a>> {
        self.data.layer_view(self.name, id)
    }
}

/// Canonical glyph ownership plus glyph-free UFO persistence metadata.
#[derive(Debug, Default)]
pub(super) struct VariableData {
    pub(super) font: babelfont::Font,
    pub(super) revision: u64,
    pub(super) compiled: std::sync::Mutex<super::compile::CompileCache>,
    pub(super) glyphs: BTreeMap<String, VariableGlyph>,
    pub(super) histories: BTreeMap<LayerId, super::history::EditHistory>,
    templates: BTreeMap<SourceId, norad::Font>,
    source_metadata: BTreeMap<SourceId, SourceMetadata>,
    pub(super) source_ids: Vec<SourceId>,
    pub(super) next_source: usize,
}

impl Clone for VariableData {
    fn clone(&self) -> Self {
        Self {
            font: self.font.clone(),
            revision: self.revision,
            compiled: std::sync::Mutex::default(),
            glyphs: self.glyphs.clone(),
            histories: self.histories.clone(),
            templates: self.templates.clone(),
            source_metadata: self.source_metadata.clone(),
            source_ids: self.source_ids.clone(),
            next_source: self.next_source,
        }
    }
}

impl VariableData {
    pub(super) fn snapshot(&self) -> DocumentSnapshot {
        DocumentSnapshot {
            glyph_geometry: self.font.glyphs.clone(),
            glyphs: self.glyphs.clone(),
            source_metadata: self.source_metadata.clone(),
            source_ids: self.source_ids.clone(),
        }
    }

    pub(super) fn glyph_view(&self, name: &str) -> Option<GlyphView<'_>> {
        Some(GlyphView {
            name: self.glyphs.get_key_value(name)?.0,
            glyph: self.glyphs.get(name)?,
            data: self,
        })
    }

    pub(super) fn layer_view(
        &self,
        name: &str,
        id: &LayerId,
    ) -> Option<super::babelfont::LayerView<'_>> {
        let preserved = self.glyphs.get(name)?.layers.get(id)?;
        let layer = self
            .font
            .glyphs
            .get(name)?
            .get_layer(&super::babelfont::layer_key(id))?;
        Some(super::babelfont::LayerView::new(layer, preserved))
    }

    pub(super) fn layer_edit_draft(
        &self,
        name: &str,
        id: &LayerId,
    ) -> Option<super::LayerEditDraft> {
        let preserved = self.glyphs.get(name)?.layers.get(id)?.clone();
        let layer = self
            .font
            .glyphs
            .get(name)?
            .get_layer(&super::babelfont::layer_key(id))?
            .clone();
        Some(super::LayerEditDraft::new(layer, preserved))
    }

    pub(super) fn layer_snapshot(
        &self,
        address: &GlyphLayerAddress,
    ) -> Option<super::CanonicalLayerSnapshot> {
        let preserved = self
            .glyphs
            .get(&address.glyph)?
            .layers
            .get(&address.layer)?
            .clone();
        let layer = self
            .font
            .glyphs
            .get(&address.glyph)?
            .get_layer(&super::babelfont::layer_key(&address.layer))?
            .clone();
        Some(super::CanonicalLayerSnapshot::new(
            address.clone(),
            layer,
            preserved,
        ))
    }

    pub(super) fn feature_text(&self, source: SourceId) -> Option<&str> {
        Some(&self.source_metadata.get(&source)?.feature_text)
    }

    pub(super) fn font_metadata(
        &self,
        source: SourceId,
    ) -> Option<&super::canonical_metadata::CanonicalFontMetadata> {
        Some(&self.source_metadata.get(&source)?.font_metadata)
    }

    pub(super) fn source_metadata_edit_draft(
        &self,
        source: SourceId,
    ) -> Option<SourceMetadataEditDraft> {
        Some(SourceMetadataEditDraft {
            metadata: self.source_metadata.get(&source)?.clone(),
        })
    }

    pub(super) fn commit_source_metadata_edit(
        &mut self,
        source: SourceId,
        draft: SourceMetadataEditDraft,
    ) -> bool {
        let Some(metadata) = self.source_metadata.get_mut(&source) else {
            return false;
        };
        if *metadata == draft.metadata {
            return false;
        }
        *metadata = draft.metadata;
        self.revision = self.revision.wrapping_add(1);
        true
    }

    pub(super) fn commit_layer_edit(
        &mut self,
        name: &str,
        id: &LayerId,
        draft: super::LayerEditDraft,
    ) -> Option<super::babelfont::LayerDelta> {
        let key = super::babelfont::layer_key(id);
        let preserved = self
            .glyphs
            .get_mut(name)
            .and_then(|glyph| glyph.layers.get_mut(id))?;
        let layer = self
            .font
            .glyphs
            .get_mut(name)
            .and_then(|glyph| glyph.get_layer_mut(&key))?;
        let delta = draft.delta_from(layer, preserved);
        if delta.is_empty() {
            return None;
        }
        let (new_layer, new_preserved) = draft.into_parts();
        *layer = new_layer;
        *preserved = new_preserved;
        self.revision = self.revision.wrapping_add(1);
        Some(delta)
    }

    pub(super) fn copy_layer(&mut self, name: &str, from: &LayerId, to: &LayerId) -> bool {
        if self
            .glyphs
            .get(name)
            .is_some_and(|glyph| glyph.layers.contains_key(to))
        {
            return false;
        }
        let Some((layer, preserved)) = self
            .glyphs
            .get(name)
            .and_then(|glyph| glyph.layers.get(from))
            .zip(
                self.font
                    .glyphs
                    .get(name)
                    .and_then(|glyph| glyph.get_layer(&super::babelfont::layer_key(from))),
            )
            .map(|(preserved, layer)| super::babelfont::copy_layer(layer, preserved, to))
        else {
            return false;
        };
        self.glyphs
            .get_mut(name)
            .expect("source layer retains its glyph")
            .layers
            .insert(to.clone(), preserved);
        self.font
            .glyphs
            .get_mut(name)
            .expect("source layer retains its geometry")
            .layers
            .push(layer);
        true
    }

    pub(super) fn remove_layer(&mut self, name: &str, id: &LayerId) -> bool {
        let key = super::babelfont::layer_key(id);
        let Some(glyph) = self.glyphs.get(name) else {
            return false;
        };
        if !glyph.layers.contains_key(id)
            || self
                .font
                .glyphs
                .get(name)
                .and_then(|glyph| glyph.get_layer(&key))
                .is_none()
        {
            return false;
        }
        let glyph = self.glyphs.get_mut(name).expect("validated glyph");
        glyph.layers.remove(id);
        let geometry = self
            .font
            .glyphs
            .get_mut(name)
            .expect("preserved layer retains its geometry");
        geometry
            .layers
            .retain(|layer| layer.id.as_deref() != Some(key.as_str()));
        if glyph.layers.is_empty() {
            self.glyphs.remove(name);
            self.font.glyphs.0.retain(|glyph| glyph.name != name);
        }
        true
    }

    pub(super) fn dependent_component_layers(&self, name: &str) -> Vec<GlyphLayerAddress> {
        self.glyphs
            .iter()
            .flat_map(|(glyph_name, glyph)| {
                glyph.layers.keys().filter_map(move |id| {
                    let layer = self
                        .font
                        .glyphs
                        .get(glyph_name)?
                        .get_layer(&super::babelfont::layer_key(id))?;
                    layer
                        .components()
                        .any(|component| component.reference.as_str() == name)
                        .then(|| GlyphLayerAddress {
                            glyph: glyph_name.clone(),
                            layer: id.clone(),
                        })
                })
            })
            .collect()
    }

    pub(super) fn from_sources(sources: &[Master]) -> Self {
        let mut data = Self::default();
        for (index, source) in sources.iter().enumerate() {
            data.source_ids.push(SourceId(index));
            data.update_source(SourceId(index), &source.font);
        }
        data.next_source = sources.len();
        data
    }

    pub(super) fn synchronize(&mut self, sources: &[Master]) {
        self.templates.retain(|id, _| self.source_ids.contains(id));
        self.source_metadata
            .retain(|id, _| self.source_ids.contains(id));
        self.histories
            .retain(|id, _| self.source_ids.contains(&id.source));
        for glyph in self.glyphs.values_mut() {
            glyph
                .layers
                .retain(|id, _| self.source_ids.contains(&id.source));
        }
        for (index, source) in sources.iter().enumerate() {
            self.update_source(self.source_ids[index], &source.font);
        }
        self.revision = self.revision.wrapping_add(1);
    }

    fn update_source(&mut self, source: SourceId, font: &norad::Font) -> bool {
        let mut changed = false;
        let metadata = SourceMetadata {
            feature_text: font.features.clone(),
            font_metadata: super::font_ops::canonical_metadata_from_ufo(font)
                .expect("Norad source metadata must satisfy the canonical metadata contract"),
        };
        changed |= self.source_metadata.get(&source) != Some(&metadata);
        self.source_metadata.insert(source, metadata);
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
                let name = payload.name().as_str();
                let key = super::babelfont::layer_key(&id);
                let unchanged = self
                    .glyphs
                    .get(name)
                    .and_then(|glyph| glyph.layers.get(&id))
                    .zip(
                        self.font
                            .glyphs
                            .get(name)
                            .and_then(|glyph| glyph.get_layer(&key)),
                    )
                    .is_some_and(|(preserved, layer)| {
                        super::babelfont::project_layer(layer, preserved) == *payload
                    });
                if !unchanged {
                    let default = layer.name() == font.default_layer().name();
                    let converted = self
                        .glyphs
                        .get(name)
                        .and_then(|glyph| glyph.layers.get(&id))
                        .zip(
                            self.font
                                .glyphs
                                .get(name)
                                .and_then(|glyph| glyph.get_layer(&key)),
                        )
                        .map_or_else(
                            || super::babelfont::layer_from_ufo(payload, &id, default),
                            |(preserved, previous)| {
                                super::babelfont::reconcile_layer_from_ufo(
                                    payload, &id, default, previous, preserved,
                                )
                            },
                        );
                    let (layer, preserved) = converted;
                    let glyph = self.glyphs.entry(payload.name().to_string()).or_default();
                    glyph.layers.insert(id.clone(), preserved);
                    if self.font.glyphs.get(name).is_none() {
                        self.font.glyphs.0.push(babelfont::Glyph::new(name));
                    }
                    let target = self.font.glyphs.get_mut(name).expect("inserted glyph");
                    if let Some(existing) =
                        target.get_layer_mut(layer.id.as_deref().expect("layer identity"))
                    {
                        *existing = layer;
                    } else {
                        target.layers.push(layer);
                    }
                    target.codepoints = payload.codepoints.iter().map(u32::from).collect();
                    changed = true;
                }
            }
        }
        self.font.glyphs.0.retain_mut(|glyph| {
            let Some(projected) = self.glyphs.get(glyph.name.as_str()) else {
                return false;
            };
            glyph.layers.retain(|layer| {
                projected
                    .layers
                    .keys()
                    .any(|id| layer.id.as_deref() == Some(super::babelfont::layer_key(id).as_str()))
            });
            !glyph.layers.is_empty()
        });
        // A template contains no glyphs or canonically owned source metadata.
        // Preserve layer ordering, paths, color, libs, images, data and all font-info fields.
        let previous = self.templates.get(&source);
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
        template.groups.clear();
        template.kerning.clear();
        template.features.clear();
        template.data.clone_from(&font.data);
        template.images.clone_from(&font.images);
        changed |= previous != Some(&template);
        self.templates.insert(source, template);
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }

    pub(super) fn source_font(&self, source: SourceId) -> Option<norad::Font> {
        let mut font = self.templates.get(&source)?.clone();
        font.features
            .clone_from(&self.source_metadata.get(&source)?.feature_text);
        super::font_ops::write_canonical_metadata_to_ufo(
            &mut font,
            &self.source_metadata.get(&source)?.font_metadata,
        )
        .expect("canonical source metadata must remain writable as UFO");
        for (name, glyph) in &self.glyphs {
            for (id, preserved) in &glyph.layers {
                if id.source == source {
                    font.layers
                        .get_mut(&id.name)
                        .expect("every stored layer has persistence metadata")
                        .insert_glyph(super::babelfont::project_layer(
                            self.font
                                .glyphs
                                .get(name)?
                                .get_layer(&super::babelfont::layer_key(id))?,
                            preserved,
                        ));
                }
            }
        }
        Some(font)
    }

    pub(super) fn project_layer(&self, name: &str, id: &LayerId) -> Option<norad::Glyph> {
        let preserved = self.glyphs.get(name)?.layers.get(id)?;
        let layer = self
            .font
            .glyphs
            .get(name)?
            .get_layer(&super::babelfont::layer_key(id))?;
        Some(super::babelfont::project_layer(layer, preserved))
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
            let id = self.data.source_ids[index];
            source.dirty |= self.data.update_source(id, &source.font);
        }
    }
}

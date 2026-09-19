// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Stable glyph identity and canonical whole-glyph storage transactions.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use babelfont::Glyph;

use super::{CanonicalSourceStructureSnapshot, GlyphLayerAddress, VariableGlyph};

/// Stable identity of one logical glyph in an open document.
///
/// The name and display order may change without changing this identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GlyphId(u64);

static NEXT_GLYPH_ID: AtomicU64 = AtomicU64::new(1);

impl GlyphId {
    fn next() -> Self {
        Self(NEXT_GLYPH_ID.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for GlyphId {
    fn default() -> Self {
        Self::next()
    }
}

impl super::GlyphView<'_> {
    /// Stable identity retained across glyph rename and display-order changes.
    pub fn id(self) -> GlyphId {
        self.glyph.id
    }
}

impl CanonicalSourceStructureSnapshot {
    pub(in crate::document) fn font_metadata(
        &self,
        source: super::SourceId,
    ) -> Option<&crate::document::canonical_metadata::CanonicalFontMetadata> {
        Some(&self.source_metadata.get(&source)?.font_metadata)
    }

    pub(in crate::document) fn add_empty_glyph(
        &mut self,
        name: &str,
        width: f64,
        codepoint: Option<char>,
    ) -> Result<Vec<GlyphLayerAddress>, String> {
        crate::document::canonical_metadata::validate_name(name)
            .map_err(|error| error.to_string())?;
        if !width.is_finite() {
            return Err("glyph advance must be finite".into());
        }
        let present = self.glyphs.contains_key(name);
        if present != self.glyph_geometry.get(name).is_some() {
            return Err(format!("inconsistent canonical glyph {name:?}"));
        }
        if !present {
            self.glyphs
                .insert(name.to_owned(), VariableGlyph::default());
            self.glyph_geometry.0.push(Glyph::new(name));
        }
        let default_layers = self
            .source_ids
            .iter()
            .map(|source| Ok((*source, self.default_layer(*source)?)))
            .collect::<Result<Vec<_>, String>>()?;
        let mut addresses = Vec::with_capacity(self.source_ids.len());
        for (source, id) in default_layers {
            let glyph = self.glyphs.get_mut(name).expect("inserted glyph");
            if glyph.layers.contains_key(&id) {
                continue;
            }
            let (layer, preserved) = crate::document::babelfont::glyph_transactions::empty_layer(
                name, &id, true, width, codepoint,
            );
            glyph.layers.insert(id.clone(), preserved);
            glyph.source_metadata.insert(
                source,
                crate::document::model::glyph_metadata::CanonicalSourceGlyphMetadata::default(),
            );
            let geometry = self
                .glyph_geometry
                .get_mut(name)
                .expect("inserted glyph geometry");
            geometry.layers.push(layer);
            if geometry.codepoints.is_empty() {
                geometry.codepoints = codepoint.into_iter().map(u32::from).collect();
            }
            addresses.push(GlyphLayerAddress {
                glyph: name.to_owned(),
                layer: id,
            });
        }
        Ok(addresses)
    }

    pub(in crate::document) fn duplicate_glyph(
        &mut self,
        source: &str,
        name: &str,
    ) -> Result<Vec<GlyphLayerAddress>, String> {
        validate_new_glyph(self, name)?;
        let source_glyph = self
            .glyphs
            .get(source)
            .ok_or_else(|| format!("missing glyph {source:?}"))?
            .clone();
        let source_geometry = self
            .glyph_geometry
            .get(source)
            .ok_or_else(|| format!("missing glyph geometry for {source:?}"))?
            .clone();
        let mut glyph = VariableGlyph {
            id: GlyphId::default(),
            layers: BTreeMap::default(),
            source_metadata: source_glyph.source_metadata,
        };
        let mut geometry = Glyph::new(name);
        let mut addresses = Vec::with_capacity(source_glyph.layers.len());
        for source_layer in &source_geometry.layers {
            let (id, preserved) = source_glyph
                .layers
                .iter()
                .find(|(id, _)| {
                    source_layer.id.as_deref()
                        == Some(crate::document::babelfont::layer_key(id).as_str())
                })
                .ok_or("glyph layer geometry has no preservation payload")?;
            let (layer, preserved) =
                crate::document::babelfont::glyph_transactions::duplicate_layer(
                    source_layer,
                    preserved,
                    id,
                    name,
                );
            glyph.layers.insert(id.clone(), preserved);
            geometry.layers.push(layer);
            addresses.push(GlyphLayerAddress {
                glyph: name.to_owned(),
                layer: id.clone(),
            });
        }
        self.glyphs.insert(name.to_owned(), glyph);
        self.glyph_geometry.0.push(geometry);
        Ok(addresses)
    }

    pub(in crate::document) fn remove_glyph(
        &mut self,
        name: &str,
    ) -> Result<Vec<GlyphLayerAddress>, String> {
        let glyph = self
            .glyphs
            .remove(name)
            .ok_or_else(|| format!("missing glyph {name:?}"))?;
        let addresses = glyph
            .layers
            .keys()
            .cloned()
            .map(|layer| GlyphLayerAddress {
                glyph: name.to_owned(),
                layer,
            })
            .collect();
        self.glyph_geometry.0.retain(|glyph| glyph.name != name);
        for metadata in self.source_metadata.values_mut() {
            metadata
                .font_metadata
                .remove_glyph_references(name)
                .map_err(|error| error.to_string())?;
        }
        Ok(addresses)
    }

    pub(in crate::document) fn rename_glyph(
        &mut self,
        old: &str,
        new: &str,
    ) -> Result<Vec<GlyphLayerAddress>, String> {
        validate_new_glyph(self, new)?;
        let mut glyph = self
            .glyphs
            .remove(old)
            .ok_or_else(|| format!("missing glyph {old:?}"))?;

        let mut metadata = self.source_metadata.clone();
        for source in metadata.values_mut() {
            source
                .font_metadata
                .rename_glyph_references(old, new)
                .map_err(|error| error.to_string())?;
        }

        for preserved in glyph.layers.values_mut() {
            crate::document::babelfont::glyph_transactions::rename_layer(preserved, new);
        }
        for geometry in &mut self.glyph_geometry.0 {
            if geometry.name == old {
                continue;
            }
            for layer in &mut geometry.layers {
                let Some((_, preserved)) =
                    self.glyphs
                        .get_mut(geometry.name.as_str())
                        .and_then(|glyph| {
                            glyph.layers.iter_mut().find(|(id, _)| {
                                layer.id.as_deref()
                                    == Some(crate::document::babelfont::layer_key(id).as_str())
                            })
                        })
                else {
                    continue;
                };
                crate::document::babelfont::glyph_transactions::rename_references(
                    layer, preserved, old, new,
                );
            }
        }
        let geometry = self
            .glyph_geometry
            .get_mut(old)
            .ok_or_else(|| format!("missing glyph geometry for {old:?}"))?;
        geometry.name = new.into();
        for layer in &mut geometry.layers {
            let Some((_, preserved)) = glyph.layers.iter_mut().find(|(id, _)| {
                layer.id.as_deref() == Some(crate::document::babelfont::layer_key(id).as_str())
            }) else {
                continue;
            };
            crate::document::babelfont::glyph_transactions::rename_references(
                layer, preserved, old, new,
            );
        }
        self.source_metadata = metadata;
        let addresses = glyph
            .layers
            .keys()
            .cloned()
            .map(|layer| GlyphLayerAddress {
                glyph: new.to_owned(),
                layer,
            })
            .collect();
        self.glyphs.insert(new.to_owned(), glyph);
        Ok(addresses)
    }

    fn default_layer(&self, source: super::SourceId) -> Result<super::LayerId, String> {
        let name = self
            .templates
            .get(&source)
            .ok_or_else(|| format!("missing source template {}", source.0))?
            .default_layer()
            .name()
            .to_string();
        Ok(super::LayerId { source, name })
    }
}

fn validate_new_glyph(
    snapshot: &CanonicalSourceStructureSnapshot,
    name: &str,
) -> Result<(), String> {
    crate::document::canonical_metadata::validate_name(name).map_err(|error| error.to_string())?;
    if snapshot.glyphs.contains_key(name) || snapshot.glyph_geometry.get(name).is_some() {
        return Err(format!("glyph {name:?} already exists"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use norad::{Font, Glyph};

    use super::super::*;

    #[test]
    fn imported_identity_is_stable_across_snapshots_and_source_updates() {
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(Glyph::new("A"));
        let source = Master::from_font(font.clone(), PathBuf::from("Regular.ufo"));
        let mut data = VariableData::from_sources(&[source]);
        let id = data.glyph_view("A").unwrap().id();

        assert_eq!(data.snapshot().glyphs["A"].id, id);
        assert!(!data.update_source(SourceId(0), &font));
        assert_eq!(data.glyph_view("A").unwrap().id(), id);
    }

    #[test]
    fn separate_imported_glyphs_have_separate_identities() {
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(Glyph::new("A"));
        font.default_layer_mut().insert_glyph(Glyph::new("B"));
        let source = Master::from_font(font, PathBuf::from("Regular.ufo"));
        let data = VariableData::from_sources(&[source]);

        assert_ne!(
            data.glyph_view("A").unwrap().id(),
            data.glyph_view("B").unwrap().id()
        );
    }
}

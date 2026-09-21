// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Atomic canonical storage for complete proposal-layer batches.

use std::collections::HashSet;

use super::{CanonicalSourceStructureSnapshot, GlyphLayerAddress, LayerId, SourceId};
use crate::font::LayerEditDraft;

impl CanonicalSourceStructureSnapshot {
    pub(in crate::font) fn stage_proposal_layers(
        &mut self,
        source: SourceId,
        foreground: &LayerId,
        target: &LayerId,
        staged: Vec<(String, LayerEditDraft)>,
    ) -> Result<Vec<GlyphLayerAddress>, String> {
        if foreground.source != source || target.source != source {
            return Err("proposal layers must belong to the selected source".into());
        }
        let format = self.source_formats.get(&source).ok_or("unknown source")?;
        if format.default_layer_name() == target.name {
            return Err("a proposal cannot replace the default layer".into());
        }
        if format.contains_layer(&target.name)
            || self
                .glyphs
                .values()
                .any(|glyph| glyph.layers.contains_key(target))
        {
            return Err("proposal task already exists; use a new task name".into());
        }
        if staged.is_empty() {
            return Err("composition plan has no replacements".into());
        }

        let mut seen = HashSet::new();
        for (name, draft) in &staged {
            let address = GlyphLayerAddress {
                glyph: name.clone(),
                layer: target.clone(),
            };
            if !seen.insert(name.as_str()) {
                return Err(format!("duplicate proposal glyph {name:?}"));
            }
            let Some(glyph) = self.glyphs.get(name) else {
                return Err(format!("no glyph named {name}"));
            };
            if !glyph.layers.contains_key(foreground) {
                return Err(format!("{name}: foreground layer disappeared"));
            }
            if !crate::font::babelfont::glyph_transactions::draft_matches_address(draft, &address) {
                return Err(format!(
                    "{name}: proposal draft has the wrong layer identity"
                ));
            }
        }

        self.source_formats
            .get_mut(&source)
            .expect("validated source")
            .ensure_layer(&target.name)?;
        let mut affected = Vec::with_capacity(staged.len());
        for (name, draft) in staged {
            let address = GlyphLayerAddress {
                glyph: name.clone(),
                layer: target.clone(),
            };
            let (layer, preserved) = draft.into_parts();
            self.glyphs
                .get_mut(&name)
                .expect("validated glyph")
                .layers
                .insert(target.clone(), preserved);
            self.glyph_geometry
                .get_mut(&name)
                .expect("validated glyph geometry")
                .layers
                .push(layer);
            affected.push(address);
        }
        Ok(affected)
    }
}

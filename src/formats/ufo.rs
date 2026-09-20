// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Explicit transient UFO codec values.

use crate::document::LayerView;
use crate::document::project::Project;
use crate::document::variable::{LayerId, SourceId};

/// Materialize one canonical layer as a detached UFO glyph.
///
/// This is a read-only format boundary for fixtures and external codecs.
/// The returned value is not editable document state and cannot be reconciled into a Project.
pub fn glyph_from_layer(layer: LayerView<'_>) -> norad::Glyph {
    layer.project()
}

impl Project {
    /// Materialize one canonical layer as a detached UFO codec value.
    pub fn encode_ufo_layer(&self, name: &str, layer: &LayerId) -> Option<norad::Glyph> {
        crate::document::ufo_codec::encode_layer(self.codec_data(), name, layer)
    }

    /// Materialize one canonical source as a detached UFO codec value.
    pub fn encode_ufo_source(&self, source: SourceId) -> Option<norad::Font> {
        crate::document::ufo_codec::encode_source(self.codec_data(), source)
    }
}

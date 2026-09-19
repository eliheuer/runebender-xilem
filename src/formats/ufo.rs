// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Explicit transient UFO codec values.

use crate::document::LayerView;

/// Materialize one canonical layer as a detached UFO glyph.
///
/// This is a read-only format boundary for fixtures and external codecs.
/// The returned value is not editable document state and cannot be reconciled into a Project.
pub fn glyph_from_layer(layer: LayerView<'_>) -> norad::Glyph {
    layer.project()
}

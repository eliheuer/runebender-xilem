// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Kerning lookup with group fallback, glyph metadata, and entity ids.

pub mod entity_id;
pub mod glyph_metadata;
pub mod kerning;

pub use entity_id::EntityId;
pub use glyph_metadata::GlyphMetadata;

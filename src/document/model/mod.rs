// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Font and glyph metadata, kerning lookup with group fallback, and entity ids.

pub mod designspace;
pub mod entity_id;
pub mod font_info;
pub mod glyph_metadata;
pub mod kerning;
pub mod smart_components;

pub use entity_id::EntityId;
pub use glyph_metadata::GlyphMetadata;

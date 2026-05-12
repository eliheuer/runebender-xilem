// Subset of runebender-xilem's `model/` that's free of kurbo
// geometry types — workspace and glyph_renderer stay in each
// consumer until they can share a kurbo version.

pub mod entity_id;
pub mod kerning;

pub use entity_id::EntityId;

// runebender-core — platform-independent types shared between
// runebender-xilem (native, kurbo 0.12 via masonry 0.4) and
// runebender-comfy (WASM, kurbo 0.13 via vello+peniko).
//
// Only the modules with NO kurbo geometry types in their public API
// live here. Anything that takes/returns `kurbo::Point`,
// `kurbo::Affine`, `kurbo::BezPath`, etc. stays duplicated in each
// project until the two consumers can be aligned on one kurbo
// version. See the project SECURITY.md and runebender-comfy README
// for the broader plan.

pub mod category;
pub mod composites;
pub mod curve;
pub mod glyph_ops;
pub mod glyph_paths;
pub mod font_memory;
pub mod glyphs_import;
pub mod var_model;
pub mod editing;
pub mod mark_color;
pub mod measure;
pub mod model;
pub mod new_font;
pub mod optical;
pub mod image_trace;
pub mod knife;
pub mod path;
pub mod point_ops;
pub mod segment_ops;
pub mod sidebar;
pub mod shape;
pub mod shaping;
pub mod text;
pub mod theme;
pub mod theme_oklch;

pub use category::GlyphCategory;
pub use mark_color::MarkColor;
pub use model::GlyphMetadata;

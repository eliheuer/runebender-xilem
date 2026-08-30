// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The editing engine behind the Runebender font editor, with no
//! interface attached.
//!
//! One rule decides what belongs here: if an operation changes a font,
//! or reads one to answer a question, it lives in this crate. The
//! front-ends (runebender-gpui, runebender-xilem) own the window, the
//! input, and the drawing, and call this crate for everything else.
//! The `runebender` binary in `src/bin` exposes the same operations
//! on the command line.
//!
//! The in-memory font is `norad::Font`. Every function here takes
//! norad types, or kurbo geometry, and returns the same. The modules
//! group by what they do to a font:
//!
//! - Outline editing: `glyph_ops`, `point_ops`, `segment_ops`,
//!   `knife`, `shape`, `cleanup`, `effects`, `convert`, `embolden`.
//! - Reading a font: `measure`, `optical`, `spacing`, `curve`,
//!   `category`, `search`.
//! - Lib keys and formats: `lib_keys`, `metrics_keys`, `color_font`,
//!   `mark_color`, `svg`, `binary_import`, `glyphs_import`,
//!   `image_trace`.
//! - Families and text: `var_model`, `composites`, `model`,
//!   `shaping`, `text`.
//! - Editor state that is not toolkit specific: `editing`,
//!   `font_memory`, `sidebar`, `theme`, `theme_oklch`.

pub mod binary_import;
pub mod category;
pub mod cleanup;
pub mod color_font;
pub mod composites;
pub mod convert;
pub mod curve;
pub mod editing;
pub mod effects;
pub mod embolden;
pub mod font_memory;
pub mod glyph_ops;
pub mod glyph_paths;
pub mod glyphs_import;
pub mod image_trace;
pub mod knife;
pub mod lib_keys;
pub mod mark_color;
pub mod measure;
pub mod metrics_keys;
pub mod model;
pub mod new_font;
pub mod optical;
pub mod path;
pub mod point_ops;
pub mod search;
pub mod segment_ops;
pub mod shape;
pub mod shaping;
pub mod sidebar;
pub mod spacing;
pub mod svg;
pub mod text;
pub mod theme;
pub mod theme_oklch;
pub mod var_model;

pub use category::GlyphCategory;
pub use mark_color::MarkColor;
pub use model::GlyphMetadata;

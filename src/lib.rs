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
//! norad types, or kurbo geometry, and returns the same. The
//! directories group the modules by what they do to a font:
//!
//! - [`outline`]: what changes a shape. Point and segment edits, the
//!   knife, cleanup, effects, conversion, emboldening, and the
//!   segment maths in `outline::path`.
//! - [`analysis`]: what reads a font. Measurement, optical weight,
//!   spacing, curvature, categories, search.
//! - [`formats`]: lib keys, and every format besides UFO.
//! - [`document`]: the open font and its family. `Master`, `Project`,
//!   interpolation, composites, in-memory fonts.
//! - [`text`]: shaping, joining rules, and the text buffer.
//! - [`ui`]: what every front-end shares that is not font data.
//!   Themes, the sidebar's filter data, selection and undo.

pub mod analysis;
pub mod document;
pub mod formats;
pub mod outline;
#[cfg(test)]
mod testing;
pub mod text;
pub mod ui;

pub use analysis::category::GlyphCategory;
pub use document::model::GlyphMetadata;
pub use formats::mark_color::MarkColor;

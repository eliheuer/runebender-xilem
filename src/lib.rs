// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender's font engine.
//!
//! The `runebender` package contains both this library target and the
//! Xilem application. The executable uses these modules for its editor
//! and for headless subcommands.
//!
//! The in-memory font is `norad::Font`. Every function here takes
//! norad types, or kurbo geometry, and returns the same. The
//! directories group the modules by what they do to a font:
//!
//! - [`outline`]: what changes a shape. Point and segment edits, the
//!   knife, cleanup, effects, conversion, emboldening, and the
//!   segment maths in `outline::path`.
//! - [`analysis`]: what reads a font. Measurement, curvature,
//!   categories, search.
//! - [`formats`]: lib keys, and every format besides UFO.
//! - [`document`]: the open font and its family. `Master`, `Project`,
//!   interpolation, composites, in-memory fonts.
//! - [`text`]: shaping, joining rules, and the text buffer.
//! - [`ui`]: toolkit-independent editor data: themes, sidebar filters,
//!   selection, undo, and viewport state.

// LINEBENDER LINT SET - lib.rs - v4
// See https://linebender.org/wiki/canonical-lints/
#![warn(clippy::print_stdout, clippy::print_stderr)]
#![cfg_attr(target_pointer_width = "64", warn(clippy::trivially_copy_pass_by_ref))]
// END LINEBENDER LINT SET

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

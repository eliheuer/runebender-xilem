// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Reusable font-editing code for Runebender.
//!
//! The graphical editor and headless commands share this library.
//! UFO and Designspace are Runebender's first-class formats.
//! Importers support most other common font formats.
//! A [`document::project::Project`] stores canonical Babelfont glyph layers plus exact-value and
//! metadata extensions, grouping them with one or more source records and their designspace data.
//! Text preview builds a temporary, outline-free OpenType font for shaping.
//!
//! - [`analysis`] computes measurements, curvature, categories, and search results.
//!   It borrows the in-memory source model and does not change it.
//! - [`document`] owns canonical glyph layers, source records and designspace metadata.
//!   It also handles interpolation, edit history, proposed changes, and node workflows.
//! - [`formats`] interprets UFO lib keys and reads or writes data at the document boundary.
//!   Its converters cover Glyphs sources, OpenType binaries, SVG, and traced images.
//! - [`outline`] converts UFO contours to editable paths and performs geometric operations.
//!   Edited paths are written back to a `norad::Glyph`.
//! - [`text`] builds layout from the source's glyphs, metrics, anchors, and OpenType feature code.
//!   It also owns the editable text buffer used by the Text tool.
//! - [`ui`] contains selection, undo, viewport, theme, sidebar, and node-layout data.
//!   Front-ends share these types without making the library depend on their GUI toolkits.

// LINEBENDER LINT SET - lib.rs - v4
// See https://linebender.org/wiki/canonical-lints/
#![cfg_attr(not(test), warn(unused_crate_dependencies))]
#![warn(clippy::print_stdout, clippy::print_stderr)]
#![cfg_attr(target_pointer_width = "64", warn(clippy::trivially_copy_pass_by_ref))]
#![cfg_attr(docsrs, feature(doc_cfg))]
// END LINEBENDER LINT SET

// Cargo enables package features for both targets. These anonymous imports stop the library's
// dependency lint from flagging dependencies used only by the application executable.
#[cfg(all(feature = "application", target_os = "macos"))]
use muda as _;
#[cfg(feature = "application")]
use {
    base64 as _, clap as _, copypasta as _, image as _, imaging_vello_cpu as _, masonry as _,
    notify as _, regex as _, rfd as _, tokio as _, winit as _, xilem as _,
};

// These modules form the public, domain-oriented font-engine API.
pub mod analysis;
pub mod document;
pub mod formats;
pub mod outline;
// Test fixtures stay private so downstream crates cannot depend on them.
#[cfg(test)]
mod testing;
pub mod text;
pub mod ui;

// Common data types are available at the crate root; other APIs stay under their domain module.
pub use analysis::category::GlyphCategory;
pub use document::model::GlyphMetadata;
pub use formats::mark_color::MarkColor;

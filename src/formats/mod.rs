// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Lib keys and file formats.
//!
//! `metadata` owns codecs for persisted keys and values.
//! The remaining modules read or write complete formats such as UFO, SVG, compiled fonts,
//! `.glyphs`, and traced images.

pub mod babelfont_import;
pub mod binary_import;
pub mod color_font;
pub mod glyphs_import;
pub mod image_trace;
pub mod metadata;
pub mod proposal_ufo;
pub mod svg;
pub mod ufo;

pub mod designbot;
pub mod designspace;

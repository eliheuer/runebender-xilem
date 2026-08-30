// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The open font and its family.
//!
//! `project` holds a `Master` and a `Project`; `var_model` interpolates
//! across a designspace; `composites` places components; `font_memory`
//! and `new_font` build fonts without a filesystem. `model` keeps the
//! kerning lookup, glyph metadata, and entity ids.

pub mod composites;
pub mod font_memory;
pub mod font_ops;
pub mod model;
pub mod new_font;
pub mod project;
pub mod var_model;

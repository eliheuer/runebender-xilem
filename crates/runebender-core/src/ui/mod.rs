// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! What every front-end shares that is not font data.
//!
//! The colour type, the theme resolver, the glyph grid's filter
//! data, the toolkit-free editing state (selection, undo, the
//! viewport), and the nodes canvas as geometry.

pub mod color;
pub mod editing;
pub mod nodes;
pub mod sidebar;
pub mod theme;

// Copyright 2024 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Sort system for text editing in font editors.
//!
//! A "sort" is a virtual representation of a physical typesetting sort - a block
//! with a typographic character that can be lined up with others to form text.
//! This module provides:
//! - Sort data structures representing individual glyphs or line breaks
//! - Gap buffer-based text buffer for efficient editing
//! - Cursor management and text positioning

pub mod buffer;
pub mod cursor;
pub mod data;

pub use buffer::SortBuffer;
pub use cursor::TextCursor;
pub use data::{LayoutMode, Sort, SortKind};

// Copyright 2024 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]
#![allow(clippy::upper_case_acronyms)]

//! Core data structures for sorts.

use kurbo::Point;

/// Layout mode for text flow.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LayoutMode {
    /// Left-to-right text (Latin scripts, etc.)
    #[default]
    LTR,
    /// Right-to-left text (Arabic, Hebrew, etc.)
    RTL,
    /// Individual positioning (no automatic flow)
    Freeform,
}

/// Kind of sort - either a glyph or a line break.
#[derive(Clone, Debug, PartialEq)]
pub enum SortKind {
    /// A glyph with typographic data.
    Glyph {
        /// Glyph name from the font (e.g., "a", "A", "exclam")
        name: String,
        /// Unicode codepoint if available
        codepoint: Option<char>,
        /// Horizontal advance width
        advance_width: f64,
    },
    /// A line break (newline)
    LineBreak,
}

/// A virtual sort representing a glyph or line break in the text buffer.
///
/// Each sort has:
/// - A kind (glyph with metrics, or line break)
/// - Active state (editable with control points vs. preview only)
/// - Layout mode (LTR/RTL/Freeform)
/// - Position in 2D space
#[derive(Clone, Debug, PartialEq)]
pub struct Sort {
    /// The kind of sort (glyph or line break)
    pub kind: SortKind,
    /// Whether this sort is active (editable)
    /// Only one sort should be active at a time
    pub is_active: bool,
    /// Layout mode for this sort
    pub layout_mode: LayoutMode,
    /// Position in design space (calculated during layout)
    pub position: Point,
}

impl Sort {
    /// Create a new glyph sort.
    pub fn new_glyph(
        name: String,
        codepoint: Option<char>,
        advance_width: f64,
        is_active: bool,
    ) -> Self {
        Self {
            kind: SortKind::Glyph {
                name,
                codepoint,
                advance_width,
            },
            is_active,
            layout_mode: LayoutMode::default(),
            position: Point::ZERO,
        }
    }

    /// Create a new line break sort.
    pub fn new_line_break() -> Self {
        Self {
            kind: SortKind::LineBreak,
            is_active: false,
            layout_mode: LayoutMode::default(),
            position: Point::ZERO,
        }
    }

    /// Get the advance width if this is a glyph sort.
    pub fn advance_width(&self) -> Option<f64> {
        match &self.kind {
            SortKind::Glyph { advance_width, .. } => Some(*advance_width),
            SortKind::LineBreak => None,
        }
    }

    /// Get the glyph name if this is a glyph sort.
    pub fn glyph_name(&self) -> Option<&str> {
        match &self.kind {
            SortKind::Glyph { name, .. } => Some(name),
            SortKind::LineBreak => None,
        }
    }

    /// Check if this is a line break.
    pub fn is_line_break(&self) -> bool {
        matches!(self.kind, SortKind::LineBreak)
    }
}

impl Default for Sort {
    fn default() -> Self {
        Self {
            kind: SortKind::Glyph {
                name: String::from("space"),
                codepoint: Some(' '),
                advance_width: 250.0,
            },
            is_active: false,
            layout_mode: LayoutMode::default(),
            position: Point::ZERO,
        }
    }
}

// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Text cursor rendering and position calculation

use kurbo::{Affine, Line, Point, Stroke};
use masonry::vello::Scene;
use masonry::vello::peniko::{Brush, Color};

use super::buffer::SortBuffer;
use super::data::SortKind;

/// Text cursor with blinking animation
#[derive(Debug, Clone)]
pub struct TextCursor {
    /// Blink timer in seconds
    blink_timer: f64,
    /// Is cursor currently visible?
    visible: bool,
}

impl Default for TextCursor {
    fn default() -> Self {
        Self::new()
    }
}

impl TextCursor {
    /// Create a new text cursor
    pub fn new() -> Self {
        TextCursor {
            blink_timer: 0.0,
            visible: true,
        }
    }

    /// Update cursor blink animation
    pub fn update(&mut self, delta_time: f64) {
        self.blink_timer += delta_time;
        if self.blink_timer >= 0.5 {
            // Blink every 500ms
            self.visible = !self.visible;
            self.blink_timer = 0.0;
        }
    }

    /// Reset cursor to visible state (called when cursor moves)
    pub fn reset(&mut self) {
        self.blink_timer = 0.0;
        self.visible = true;
    }

    /// Calculate cursor position from sort buffer
    ///
    /// Returns the (x, y) position in design space where the cursor should be rendered.
    /// This accumulates advance widths up to the cursor position.
    pub fn calculate_position(
        &self,
        buffer: &SortBuffer,
        line_height: f64,
    ) -> Point {
        let mut x = 0.0;
        let mut y = 0.0;
        let cursor_pos = buffer.cursor();

        // Accumulate advance widths up to cursor position
        for (i, sort) in buffer.iter().enumerate() {
            if i >= cursor_pos {
                break;
            }

            match &sort.kind {
                SortKind::Glyph { advance_width, .. } => {
                    x += advance_width;
                }
                SortKind::LineBreak => {
                    x = 0.0;
                    y -= line_height;
                }
            }
        }

        Point::new(x, y)
    }

    /// Render the cursor as a vertical line
    pub fn render(&self, scene: &mut Scene, position: Point, height: f64, transform: &Affine) {
        if !self.visible {
            return;
        }

        // Draw vertical line from baseline - 80% of height to baseline + 20% of height
        let cursor_line = Line::new(
            Point::new(position.x, position.y - height * 0.8),
            Point::new(position.x, position.y + height * 0.2),
        );

        // Cursor color - bright blue
        let cursor_color = Color::from_rgb8(0x00, 0x7A, 0xFF);

        scene.stroke(
            &Stroke::new(2.0),
            *transform,
            &Brush::Solid(cursor_color),
            None,
            &cursor_line,
        );
    }

    /// Check if cursor is visible (for animation)
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sort::data::{LayoutMode, Sort};

    #[test]
    fn test_cursor_initial_state() {
        let cursor = TextCursor::new();
        assert!(cursor.is_visible());
        assert_eq!(cursor.blink_timer, 0.0);
    }

    #[test]
    fn test_cursor_blink() {
        let mut cursor = TextCursor::new();
        assert!(cursor.is_visible());

        // Update with 0.5 seconds - should toggle visibility
        cursor.update(0.5);
        assert!(!cursor.is_visible());

        // Update with another 0.5 seconds - should toggle back
        cursor.update(0.5);
        assert!(cursor.is_visible());
    }

    #[test]
    fn test_cursor_reset() {
        let mut cursor = TextCursor::new();
        cursor.update(0.5); // Make invisible
        assert!(!cursor.is_visible());

        cursor.reset();
        assert!(cursor.is_visible());
        assert_eq!(cursor.blink_timer, 0.0);
    }

    #[test]
    fn test_cursor_position_empty_buffer() {
        let cursor = TextCursor::new();
        let buffer = SortBuffer::new();

        let pos = cursor.calculate_position(&buffer, 1000.0);
        assert_eq!(pos, Point::ZERO);
    }

    #[test]
    fn test_cursor_position_with_glyphs() {
        let cursor = TextCursor::new();
        let mut buffer = SortBuffer::new();

        // Insert two glyphs
        buffer.insert(Sort {
            kind: SortKind::Glyph {
                name: "A".to_string(),
                codepoint: Some('A'),
                advance_width: 500.0,
            },
            is_active: false,
            layout_mode: LayoutMode::LTR,
            position: Point::ZERO,
        });

        buffer.insert(Sort {
            kind: SortKind::Glyph {
                name: "B".to_string(),
                codepoint: Some('B'),
                advance_width: 300.0,
            },
            is_active: false,
            layout_mode: LayoutMode::LTR,
            position: Point::ZERO,
        });

        // Cursor should be after both glyphs
        let pos = cursor.calculate_position(&buffer, 1000.0);
        assert_eq!(pos.x, 800.0); // 500 + 300
        assert_eq!(pos.y, 0.0);
    }

    #[test]
    fn test_cursor_position_with_line_break() {
        let cursor = TextCursor::new();
        let mut buffer = SortBuffer::new();

        // Insert glyph, line break, another glyph
        buffer.insert(Sort {
            kind: SortKind::Glyph {
                name: "A".to_string(),
                codepoint: Some('A'),
                advance_width: 500.0,
            },
            is_active: false,
            layout_mode: LayoutMode::LTR,
            position: Point::ZERO,
        });

        buffer.insert(Sort {
            kind: SortKind::LineBreak,
            is_active: false,
            layout_mode: LayoutMode::LTR,
            position: Point::ZERO,
        });

        buffer.insert(Sort {
            kind: SortKind::Glyph {
                name: "B".to_string(),
                codepoint: Some('B'),
                advance_width: 300.0,
            },
            is_active: false,
            layout_mode: LayoutMode::LTR,
            position: Point::ZERO,
        });

        // Cursor should be on second line after B
        let line_height = 1000.0;
        let pos = cursor.calculate_position(&buffer, line_height);
        assert_eq!(pos.x, 300.0); // Just B's advance width
        assert_eq!(pos.y, -line_height); // One line down
    }
}

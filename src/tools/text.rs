// Copyright 2024 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Text editing tool (Phase 4)
//!
//! This tool activates text editing mode, allowing the user to edit multiple
//! glyphs in a text buffer.

use crate::edit_session::EditSession;
use crate::edit_types::EditType;
use crate::mouse::{Drag, MouseDelegate, MouseEvent};
use crate::tools::{Tool, ToolId};
use kurbo::Affine;
use masonry::vello::Scene;

/// Text editing tool
///
/// When activated, this tool enters text editing mode where the user can:
/// - Type characters to insert sorts
/// - Use arrow keys to move cursor
/// - Delete with backspace
/// - Double-click on a sort to activate it for editing
/// - See multiple glyphs laid out horizontally
#[derive(Debug, Clone)]
pub struct TextTool {
    /// Track last click time and position for double-click detection
    last_click_time: f64,
    last_click_pos: kurbo::Point,
}

impl Default for TextTool {
    fn default() -> Self {
        Self {
            last_click_time: 0.0,
            last_click_pos: kurbo::Point::ZERO,
        }
    }
}

impl Tool for TextTool {
    fn id(&self) -> ToolId {
        ToolId::Text
    }

    fn paint(
        &mut self,
        _scene: &mut Scene,
        _session: &EditSession,
        _transform: &Affine,
    ) {
        // Text tool doesn't paint overlays
        // (cursor will be rendered by editor_canvas in Phase 6)
    }

    fn edit_type(&self) -> Option<EditType> {
        None // Text operations will use EditType::Normal
    }
}

impl MouseDelegate for TextTool {
    type Data = EditSession;

    fn left_down(&mut self, _event: MouseEvent, session: &mut Self::Data) {
        // When activating text tool, enter text mode if buffer exists
        if session.has_text_buffer() {
            session.enter_text_mode();
        }
    }

    fn left_up(&mut self, _event: MouseEvent, _data: &mut Self::Data) {
        // No-op
    }

    fn left_click(&mut self, event: MouseEvent, session: &mut Self::Data) {
        // Phase 7: Double-click on a sort to activate it for editing
        // (matching Glyphs app UX)

        use std::time::{SystemTime, UNIX_EPOCH};

        // Get current time in seconds
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        // Check if this is a double-click
        // Double-click threshold: 500ms and within 10 pixels
        let time_delta = now - self.last_click_time;
        let pos_delta = (event.pos - self.last_click_pos).hypot();

        let is_double_click = time_delta < 0.5 && pos_delta < 10.0;

        if is_double_click {
            // Transform click position from screen to design space
            let design_pos = session.viewport.affine().inverse() * event.pos;

            if session.activate_sort_at_position(design_pos) {
                tracing::info!("Double-clicked to activate sort at {:?}", design_pos);
            }

            // Reset click tracking after double-click
            self.last_click_time = 0.0;
        } else {
            // Record this click for potential double-click
            self.last_click_time = now;
            self.last_click_pos = event.pos;
        }
    }

    fn mouse_moved(&mut self, _event: MouseEvent, _data: &mut Self::Data) {
        // No-op
    }

    fn left_drag_began(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        _data: &mut Self::Data,
    ) {
        // No-op
    }

    fn left_drag_changed(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        _data: &mut Self::Data,
    ) {
        // No-op
    }

    fn left_drag_ended(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        _data: &mut Self::Data,
    ) {
        // No-op
    }

    fn cancel(&mut self, _data: &mut Self::Data) {
        // No-op
    }
}

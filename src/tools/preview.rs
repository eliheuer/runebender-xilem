// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Preview tool - hand tool for panning the canvas

use crate::edit_session::EditSession;
use crate::edit_types::EditType;
use crate::mouse::{Drag, MouseDelegate, MouseEvent};
use crate::tools::{Tool, ToolId};
use kurbo::Vec2;
use tracing;

// ===== PreviewTool Struct =====

/// Preview/Hand tool for panning the viewport
#[derive(Debug, Clone, Default)]
pub struct PreviewTool {
    state: State,
}

// ===== Internal State =====

/// Internal state for the preview tool
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum State {
    /// Ready to start dragging
    #[default]
    Ready,
    /// Currently dragging/panning
    Dragging { start_offset: Vec2 },
}

// ===== Tool Implementation =====

#[allow(dead_code)]
impl Tool for PreviewTool {
    fn id(&self) -> ToolId {
        ToolId::Preview
    }

    fn edit_type(&self) -> Option<EditType> {
        // Panning doesn't modify the glyph, so no edit type
        None
    }
}

// ===== MouseDelegate Implementation =====

#[allow(dead_code)]
impl MouseDelegate for PreviewTool {
    type Data = EditSession;

    fn left_down(
        &mut self,
        _event: MouseEvent,
        session: &mut EditSession,
    ) {
        tracing::debug!(
            "[PreviewTool] left_down - capturing offset: {:?}",
            session.viewport.offset
        );
        // Capture the current viewport offset when we start dragging
        self.state = State::Dragging {
            start_offset: session.viewport.offset,
        };
    }

    fn left_up(
        &mut self,
        _event: MouseEvent,
        _session: &mut EditSession,
    ) {
        tracing::debug!("[PreviewTool] left_up - returning to ready");
        // Return to ready state
        self.state = State::Ready;
    }

    fn left_drag_changed(
        &mut self,
        _event: MouseEvent,
        drag: Drag,
        session: &mut EditSession,
    ) {
        tracing::debug!(
            "[PreviewTool] left_drag_changed - state: {:?}, \
             drag.start: {:?}, drag.current: {:?}",
            self.state,
            drag.start,
            drag.current
        );

        let State::Dragging { start_offset } = self.state else {
            return;
        };

        // Calculate the delta in screen space
        let delta = drag.current - drag.start;
        tracing::debug!(
            "[PreviewTool] delta: {:?}, start_offset: {:?}",
            delta,
            start_offset
        );

        // Update viewport offset
        // Note: We add the delta directly since screen space panning
        // should move the viewport in the same direction as the mouse
        session.viewport.offset = start_offset + delta;
        tracing::debug!(
            "[PreviewTool] new offset: {:?}",
            session.viewport.offset
        );
    }

    fn cancel(&mut self, session: &mut EditSession) {
        // If we were dragging, restore the original offset
        if let State::Dragging { start_offset } = self.state {
            session.viewport.offset = start_offset;
        }
        self.state = State::Ready;
    }
}

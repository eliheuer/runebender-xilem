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
/// - See multiple glyphs laid out horizontally
#[derive(Default, Debug, Clone)]
pub struct TextTool;

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

    fn left_click(&mut self, _event: MouseEvent, _data: &mut Self::Data) {
        // No-op
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

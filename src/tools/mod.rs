// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Tool system for glyph editing

use crate::edit_session::EditSession;
use crate::edit_types::EditType;
use crate::mouse::{Drag, MouseDelegate, MouseEvent};
use kurbo::Affine;
use masonry::vello::Scene;

// ===== Tool Identifier =====

/// Tool identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolId {
    /// Select and move points
    Select,
    /// Draw new paths
    Pen,
    /// Preview mode (view only)
    Preview,
}

// ===== Tool Trait =====

/// A tool for editing glyphs
pub trait Tool: MouseDelegate<Data = EditSession> {
    /// Get the tool identifier
    fn id(&self) -> ToolId;

    /// Paint tool-specific overlays
    fn paint(
        &mut self,
        _scene: &mut Scene,
        _session: &EditSession,
        _transform: &Affine,
    ) {
    }

    /// Get the edit type for the current operation (for undo grouping)
    fn edit_type(&self) -> Option<EditType> {
        None
    }
}

// ===== ToolBox Enum =====

/// Enum wrapping all tool types
#[derive(Debug, Clone)]
pub enum ToolBox {
    Select(select::SelectTool),
    Pen(pen::PenTool),
    Preview(preview::PreviewTool),
}

// ===== ToolBox Implementation =====

impl ToolBox {
    /// Create a tool by ID
    pub fn for_id(id: ToolId) -> Self {
        match id {
            ToolId::Select => {
                ToolBox::Select(select::SelectTool::default())
            }
            ToolId::Pen => ToolBox::Pen(pen::PenTool::default()),
            ToolId::Preview => {
                ToolBox::Preview(preview::PreviewTool::default())
            }
        }
    }

    /// Get the tool ID
    pub fn id(&self) -> ToolId {
        match self {
            ToolBox::Select(tool) => tool.id(),
            ToolBox::Pen(tool) => tool.id(),
            ToolBox::Preview(tool) => tool.id(),
        }
    }

    /// Paint tool overlays
    pub fn paint(
        &mut self,
        scene: &mut Scene,
        session: &EditSession,
        transform: &Affine,
    ) {
        match self {
            ToolBox::Select(tool) => {
                tool.paint(scene, session, transform);
            }
            ToolBox::Pen(tool) => {
                tool.paint(scene, session, transform);
            }
            ToolBox::Preview(_) => {
                // Preview tool has no overlays
            }
        }
    }

    /// Get edit type
    pub fn edit_type(&self) -> Option<EditType> {
        match self {
            ToolBox::Select(tool) => tool.edit_type(),
            ToolBox::Pen(tool) => tool.edit_type(),
            ToolBox::Preview(tool) => tool.edit_type(),
        }
    }

    /// Handle mouse down
    pub fn mouse_down(
        &mut self,
        event: MouseEvent,
        session: &mut EditSession,
    ) {
        match self {
            ToolBox::Select(tool) => tool.left_down(event, session),
            ToolBox::Pen(tool) => tool.left_down(event, session),
            ToolBox::Preview(tool) => tool.left_down(event, session),
        }
    }

    /// Handle mouse up
    pub fn mouse_up(
        &mut self,
        event: MouseEvent,
        session: &mut EditSession,
    ) {
        match self {
            ToolBox::Select(tool) => tool.left_up(event, session),
            ToolBox::Pen(tool) => tool.left_up(event, session),
            ToolBox::Preview(tool) => tool.left_up(event, session),
        }
    }

    /// Handle mouse moved
    ///
    /// Called indirectly through MouseDelegate trait implementation
    #[allow(dead_code)]
    pub fn mouse_moved(
        &mut self,
        event: MouseEvent,
        session: &mut EditSession,
    ) {
        match self {
            ToolBox::Select(tool) => tool.mouse_moved(event, session),
            ToolBox::Pen(tool) => tool.mouse_moved(event, session),
            ToolBox::Preview(tool) => tool.mouse_moved(event, session),
        }
    }

    /// Handle drag began
    pub fn drag_began(
        &mut self,
        event: MouseEvent,
        drag: Drag,
        session: &mut EditSession,
    ) {
        match self {
            ToolBox::Select(tool) => {
                tool.left_drag_began(event, drag, session);
            }
            ToolBox::Pen(tool) => {
                tool.left_drag_began(event, drag, session);
            }
            ToolBox::Preview(tool) => {
                tool.left_drag_began(event, drag, session);
            }
        }
    }

    /// Handle drag changed
    pub fn drag_changed(
        &mut self,
        event: MouseEvent,
        drag: Drag,
        session: &mut EditSession,
    ) {
        match self {
            ToolBox::Select(tool) => {
                tool.left_drag_changed(event, drag, session);
            }
            ToolBox::Pen(tool) => {
                tool.left_drag_changed(event, drag, session);
            }
            ToolBox::Preview(tool) => {
                tool.left_drag_changed(event, drag, session);
            }
        }
    }

    /// Handle drag ended
    pub fn drag_ended(
        &mut self,
        event: MouseEvent,
        drag: Drag,
        session: &mut EditSession,
    ) {
        match self {
            ToolBox::Select(tool) => {
                tool.left_drag_ended(event, drag, session);
            }
            ToolBox::Pen(tool) => {
                tool.left_drag_ended(event, drag, session);
            }
            ToolBox::Preview(tool) => {
                tool.left_drag_ended(event, drag, session);
            }
        }
    }

    /// Cancel current operation
    ///
    /// Called indirectly through MouseDelegate trait implementation
    #[allow(dead_code)]
    pub fn cancel(&mut self, session: &mut EditSession) {
        match self {
            ToolBox::Select(tool) => tool.cancel(session),
            ToolBox::Pen(tool) => tool.cancel(session),
            ToolBox::Preview(tool) => tool.cancel(session),
        }
    }
}

// ===== MouseDelegate Implementation =====

/// Implement MouseDelegate for ToolBox so it can be used with the Mouse
/// state machine
///
/// These methods are required by the trait contract even if not directly
/// called - they're invoked through the Mouse state machine.
#[allow(dead_code)]
impl MouseDelegate for ToolBox {
    type Data = EditSession;

    fn left_down(
        &mut self,
        event: MouseEvent,
        data: &mut EditSession,
    ) {
        self.mouse_down(event, data);
    }

    fn left_up(
        &mut self,
        event: MouseEvent,
        data: &mut EditSession,
    ) {
        self.mouse_up(event, data);
    }

    fn left_click(
        &mut self,
        event: MouseEvent,
        data: &mut EditSession,
    ) {
        match self {
            ToolBox::Select(tool) => tool.left_click(event, data),
            ToolBox::Pen(tool) => tool.left_click(event, data),
            ToolBox::Preview(tool) => tool.left_click(event, data),
        }
    }

    fn mouse_moved(
        &mut self,
        event: MouseEvent,
        data: &mut EditSession,
    ) {
        match self {
            ToolBox::Select(tool) => tool.mouse_moved(event, data),
            ToolBox::Pen(tool) => tool.mouse_moved(event, data),
            ToolBox::Preview(tool) => tool.mouse_moved(event, data),
        }
    }

    fn left_drag_began(
        &mut self,
        event: MouseEvent,
        drag: Drag,
        data: &mut EditSession,
    ) {
        self.drag_began(event, drag, data);
    }

    fn left_drag_changed(
        &mut self,
        event: MouseEvent,
        drag: Drag,
        data: &mut EditSession,
    ) {
        self.drag_changed(event, drag, data);
    }

    fn left_drag_ended(
        &mut self,
        event: MouseEvent,
        drag: Drag,
        data: &mut EditSession,
    ) {
        self.drag_ended(event, drag, data);
    }

    fn cancel(&mut self, data: &mut EditSession) {
        match self {
            ToolBox::Select(tool) => tool.cancel(data),
            ToolBox::Pen(tool) => tool.cancel(data),
            ToolBox::Preview(tool) => tool.cancel(data),
        }
    }
}

// ===== Tool Modules =====

pub mod pen;
pub mod preview;
pub mod select;

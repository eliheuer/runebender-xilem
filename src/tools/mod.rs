// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Tool system for glyph editing

use crate::editing::{Drag, EditSession, EditType, MouseDelegate, MouseEvent};
use kurbo::Affine;
use masonry::vello::Scene;

// ===== Tool Identifier =====

/// Tool identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolId {
    /// Select and move points
    Select,
    /// Draw new paths (cubic bezier)
    Pen,
    /// Draw new paths (hyperbezier)
    HyperPen,
    /// Preview mode (view only)
    Preview,
    /// Knife tool for cutting paths
    Knife,
    /// Measure tool for distances and angles
    Measure,
    /// Shapes tool for geometric primitives
    Shapes,
    /// Text editing mode (Phase 4)
    Text,
}

// ===== Tool Trait =====

/// A tool for editing glyphs
pub trait Tool: MouseDelegate<Data = EditSession> {
    /// Get the tool identifier
    fn id(&self) -> ToolId;

    /// Paint tool-specific overlays
    fn paint(&mut self, _scene: &mut Scene, _session: &EditSession, _transform: &Affine) {}

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
    HyperPen(hyper_pen::HyperPenTool),
    Preview(preview::PreviewTool),
    Knife(knife::KnifeTool),
    Measure(measure::MeasureTool),
    Shapes(shapes::ShapesTool),
    Text(text::TextTool),
}

// ===== ToolBox Implementation =====

impl ToolBox {
    /// Create a tool by ID
    pub fn for_id(id: ToolId) -> Self {
        match id {
            ToolId::Select => ToolBox::Select(select::SelectTool::default()),
            ToolId::Pen => ToolBox::Pen(pen::PenTool::default()),
            ToolId::HyperPen => ToolBox::HyperPen(hyper_pen::HyperPenTool::default()),
            ToolId::Preview => ToolBox::Preview(preview::PreviewTool::default()),
            ToolId::Knife => ToolBox::Knife(knife::KnifeTool::default()),
            ToolId::Measure => ToolBox::Measure(measure::MeasureTool::default()),
            ToolId::Shapes => ToolBox::Shapes(shapes::ShapesTool::default()),
            ToolId::Text => ToolBox::Text(text::TextTool::default()),
        }
    }

    /// Get the tool ID
    pub fn id(&self) -> ToolId {
        match self {
            ToolBox::Select(tool) => tool.id(),
            ToolBox::Pen(tool) => tool.id(),
            ToolBox::HyperPen(tool) => tool.id(),
            ToolBox::Preview(tool) => tool.id(),
            ToolBox::Knife(tool) => tool.id(),
            ToolBox::Measure(tool) => tool.id(),
            ToolBox::Shapes(tool) => tool.id(),
            ToolBox::Text(tool) => tool.id(),
        }
    }

    /// Paint tool overlays
    pub fn paint(&mut self, scene: &mut Scene, session: &EditSession, transform: &Affine) {
        match self {
            ToolBox::Select(tool) => {
                tool.paint(scene, session, transform);
            }
            ToolBox::Pen(tool) => {
                tool.paint(scene, session, transform);
            }
            ToolBox::HyperPen(tool) => {
                tool.paint(scene, session, transform);
            }
            ToolBox::Preview(_) => {
                // Preview tool has no overlays
            }
            ToolBox::Knife(tool) => {
                tool.paint(scene, session, transform);
            }
            ToolBox::Measure(tool) => {
                tool.paint(scene, session, transform);
            }
            ToolBox::Shapes(tool) => {
                tool.paint(scene, session, transform);
            }
            ToolBox::Text(tool) => {
                tool.paint(scene, session, transform);
            }
        }
    }

    /// Get edit type
    pub fn edit_type(&self) -> Option<EditType> {
        match self {
            ToolBox::Select(tool) => tool.edit_type(),
            ToolBox::Pen(tool) => tool.edit_type(),
            ToolBox::HyperPen(tool) => tool.edit_type(),
            ToolBox::Preview(tool) => tool.edit_type(),
            ToolBox::Knife(tool) => tool.edit_type(),
            ToolBox::Measure(tool) => tool.edit_type(),
            ToolBox::Shapes(tool) => tool.edit_type(),
            ToolBox::Text(tool) => tool.edit_type(),
        }
    }

    /// Handle mouse down
    pub fn mouse_down(&mut self, event: MouseEvent, session: &mut EditSession) {
        match self {
            ToolBox::Select(tool) => tool.left_down(event, session),
            ToolBox::Pen(tool) => tool.left_down(event, session),
            ToolBox::HyperPen(tool) => tool.left_down(event, session),
            ToolBox::Preview(tool) => tool.left_down(event, session),
            ToolBox::Knife(tool) => tool.left_down(event, session),
            ToolBox::Measure(tool) => tool.left_down(event, session),
            ToolBox::Shapes(tool) => tool.left_down(event, session),
            ToolBox::Text(tool) => tool.left_down(event, session),
        }
    }

    /// Handle mouse up
    pub fn mouse_up(&mut self, event: MouseEvent, session: &mut EditSession) {
        match self {
            ToolBox::Select(tool) => tool.left_up(event, session),
            ToolBox::Pen(tool) => tool.left_up(event, session),
            ToolBox::HyperPen(tool) => tool.left_up(event, session),
            ToolBox::Preview(tool) => tool.left_up(event, session),
            ToolBox::Knife(tool) => tool.left_up(event, session),
            ToolBox::Measure(tool) => tool.left_up(event, session),
            ToolBox::Shapes(tool) => tool.left_up(event, session),
            ToolBox::Text(tool) => tool.left_up(event, session),
        }
    }

    /// Handle mouse moved
    ///
    /// Called indirectly through MouseDelegate trait implementation
    #[allow(dead_code)]
    pub fn mouse_moved(&mut self, event: MouseEvent, session: &mut EditSession) {
        match self {
            ToolBox::Select(tool) => tool.mouse_moved(event, session),
            ToolBox::Pen(tool) => tool.mouse_moved(event, session),
            ToolBox::HyperPen(tool) => tool.mouse_moved(event, session),
            ToolBox::Preview(tool) => tool.mouse_moved(event, session),
            ToolBox::Knife(tool) => tool.mouse_moved(event, session),
            ToolBox::Measure(tool) => tool.mouse_moved(event, session),
            ToolBox::Shapes(tool) => tool.mouse_moved(event, session),
            ToolBox::Text(tool) => tool.mouse_moved(event, session),
        }
    }

    /// Handle drag began
    pub fn drag_began(&mut self, event: MouseEvent, drag: Drag, session: &mut EditSession) {
        match self {
            ToolBox::Select(tool) => {
                tool.left_drag_began(event, drag, session);
            }
            ToolBox::Pen(tool) => {
                tool.left_drag_began(event, drag, session);
            }
            ToolBox::HyperPen(tool) => {
                tool.left_drag_began(event, drag, session);
            }
            ToolBox::Preview(tool) => {
                tool.left_drag_began(event, drag, session);
            }
            ToolBox::Knife(tool) => {
                tool.left_drag_began(event, drag, session);
            }
            ToolBox::Measure(tool) => {
                tool.left_drag_began(event, drag, session);
            }
            ToolBox::Shapes(tool) => {
                tool.left_drag_began(event, drag, session);
            }
            ToolBox::Text(tool) => {
                tool.left_drag_began(event, drag, session);
            }
        }
    }

    /// Handle drag changed
    pub fn drag_changed(&mut self, event: MouseEvent, drag: Drag, session: &mut EditSession) {
        match self {
            ToolBox::Select(tool) => {
                tool.left_drag_changed(event, drag, session);
            }
            ToolBox::Pen(tool) => {
                tool.left_drag_changed(event, drag, session);
            }
            ToolBox::HyperPen(tool) => {
                tool.left_drag_changed(event, drag, session);
            }
            ToolBox::Preview(tool) => {
                tool.left_drag_changed(event, drag, session);
            }
            ToolBox::Knife(tool) => {
                tool.left_drag_changed(event, drag, session);
            }
            ToolBox::Measure(tool) => {
                tool.left_drag_changed(event, drag, session);
            }
            ToolBox::Shapes(tool) => {
                tool.left_drag_changed(event, drag, session);
            }
            ToolBox::Text(tool) => {
                tool.left_drag_changed(event, drag, session);
            }
        }
    }

    /// Handle drag ended
    pub fn drag_ended(&mut self, event: MouseEvent, drag: Drag, session: &mut EditSession) {
        match self {
            ToolBox::Select(tool) => {
                tool.left_drag_ended(event, drag, session);
            }
            ToolBox::Pen(tool) => {
                tool.left_drag_ended(event, drag, session);
            }
            ToolBox::HyperPen(tool) => {
                tool.left_drag_ended(event, drag, session);
            }
            ToolBox::Preview(tool) => {
                tool.left_drag_ended(event, drag, session);
            }
            ToolBox::Knife(tool) => {
                tool.left_drag_ended(event, drag, session);
            }
            ToolBox::Measure(tool) => {
                tool.left_drag_ended(event, drag, session);
            }
            ToolBox::Shapes(tool) => {
                tool.left_drag_ended(event, drag, session);
            }
            ToolBox::Text(tool) => {
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
            ToolBox::HyperPen(tool) => tool.cancel(session),
            ToolBox::Preview(tool) => tool.cancel(session),
            ToolBox::Knife(tool) => tool.cancel(session),
            ToolBox::Measure(tool) => tool.cancel(session),
            ToolBox::Shapes(tool) => tool.cancel(session),
            ToolBox::Text(tool) => tool.cancel(session),
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

    fn left_down(&mut self, event: MouseEvent, data: &mut EditSession) {
        self.mouse_down(event, data);
    }

    fn left_up(&mut self, event: MouseEvent, data: &mut EditSession) {
        self.mouse_up(event, data);
    }

    fn left_click(&mut self, event: MouseEvent, data: &mut EditSession) {
        match self {
            ToolBox::Select(tool) => tool.left_click(event, data),
            ToolBox::Pen(tool) => tool.left_click(event, data),
            ToolBox::HyperPen(tool) => tool.left_click(event, data),
            ToolBox::Preview(tool) => tool.left_click(event, data),
            ToolBox::Knife(tool) => tool.left_click(event, data),
            ToolBox::Measure(tool) => tool.left_click(event, data),
            ToolBox::Shapes(tool) => tool.left_click(event, data),
            ToolBox::Text(tool) => tool.left_click(event, data),
        }
    }

    fn mouse_moved(&mut self, event: MouseEvent, data: &mut EditSession) {
        match self {
            ToolBox::Select(tool) => tool.mouse_moved(event, data),
            ToolBox::Pen(tool) => tool.mouse_moved(event, data),
            ToolBox::HyperPen(tool) => tool.mouse_moved(event, data),
            ToolBox::Preview(tool) => tool.mouse_moved(event, data),
            ToolBox::Knife(tool) => tool.mouse_moved(event, data),
            ToolBox::Measure(tool) => tool.mouse_moved(event, data),
            ToolBox::Shapes(tool) => tool.mouse_moved(event, data),
            ToolBox::Text(tool) => tool.mouse_moved(event, data),
        }
    }

    fn left_drag_began(&mut self, event: MouseEvent, drag: Drag, data: &mut EditSession) {
        self.drag_began(event, drag, data);
    }

    fn left_drag_changed(&mut self, event: MouseEvent, drag: Drag, data: &mut EditSession) {
        self.drag_changed(event, drag, data);
    }

    fn left_drag_ended(&mut self, event: MouseEvent, drag: Drag, data: &mut EditSession) {
        self.drag_ended(event, drag, data);
    }

    fn cancel(&mut self, data: &mut EditSession) {
        match self {
            ToolBox::Select(tool) => tool.cancel(data),
            ToolBox::Pen(tool) => tool.cancel(data),
            ToolBox::HyperPen(tool) => tool.cancel(data),
            ToolBox::Preview(tool) => tool.cancel(data),
            ToolBox::Knife(tool) => tool.cancel(data),
            ToolBox::Measure(tool) => tool.cancel(data),
            ToolBox::Shapes(tool) => tool.cancel(data),
            ToolBox::Text(tool) => tool.cancel(data),
        }
    }
}

// ===== Tool Modules =====

pub mod hyper_pen;
pub mod knife;
pub mod measure;
pub mod pen;
pub mod preview;
pub mod select;
pub mod shapes;
pub mod text;

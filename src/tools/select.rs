// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Select tool for selecting and moving points

use crate::edit_session::EditSession;
use crate::edit_types::EditType;
use crate::mouse::{Drag, MouseDelegate, MouseEvent};
use crate::selection::Selection;
use crate::tools::{Tool, ToolId};
use kurbo::Affine;
use kurbo::Point;
use kurbo::Rect;
use kurbo::Vec2;
use masonry::vello::Scene;
use tracing;

// ===== SelectTool Struct =====

/// The select tool - used for selecting and moving points
#[derive(Debug, Clone, Default)]
pub struct SelectTool {
    /// Current tool state
    state: State,
}

// ===== Internal State =====

/// Internal state for the select tool
#[derive(Debug, Clone, Default)]
enum State {
    /// Ready to start an interaction
    #[default]
    Ready,
    /// Dragging selected points
    DraggingPoints {
        /// Last mouse position in design space
        last_pos: Point,
    },
    /// Marquee selection (dragging out a rectangle)
    MarqueeSelect {
        /// Selection before this marquee started (for shift+toggle mode)
        previous_selection: Selection,
        /// The selection rectangle in screen space
        rect: Rect,
        /// Whether shift is held (toggle mode)
        toggle: bool,
    },
}

// ===== Tool Implementation =====

#[allow(dead_code)]
impl Tool for SelectTool {
    fn id(&self) -> ToolId {
        ToolId::Select
    }

    fn paint(
        &mut self,
        scene: &mut Scene,
        _session: &EditSession,
        _transform: &Affine,
    ) {
        // Draw selection rectangle if in marquee mode
        let State::MarqueeSelect { rect, .. } = &self.state else {
            return;
        };

        use masonry::util::fill_color;
        use masonry::vello::peniko::Brush;

        // Fill the selection rectangle with semi-transparent orange
        fill_color(scene, rect, crate::theme::selection::RECT_FILL);

        // Stroke the selection rectangle with dashed bright orange
        // Create a dashed stroke pattern: 4px dash, 4px gap
        let stroke = kurbo::Stroke::new(1.5).with_dashes(0.0, [4.0, 4.0]);
        let brush = Brush::Solid(crate::theme::selection::RECT_STROKE);
        scene.stroke(
            &stroke,
            Affine::IDENTITY,
            &brush,
            None,
            rect,
        );
    }

    fn edit_type(&self) -> Option<EditType> {
        match &self.state {
            State::DraggingPoints { .. } => Some(EditType::Drag),
            _ => None,
        }
    }
}

// ===== MouseDelegate Implementation =====

#[allow(dead_code)]
impl MouseDelegate for SelectTool {
    type Data = EditSession;

    fn left_down(
        &mut self,
        event: MouseEvent,
        data: &mut EditSession,
    ) {
        tracing::debug!(
            "SelectTool::left_down pos={:?} shift={}",
            event.pos,
            event.mods.shift
        );

        // Hit test for a point at the cursor - selection happens HERE,
        // on mouse down
        if let Some(hit) = data.hit_test_point(event.pos, None) {
            tracing::debug!(
                "Hit point: {:?} distance={}",
                hit.entity,
                hit.distance
            );
            self.handle_point_selection(data, hit.entity, event.mods.shift);
        } else if !event.mods.shift {
            // Clicked on empty space without shift - clear selection
            data.selection = Selection::new();
            data.update_coord_selection();
        }
    }

    fn left_up(
        &mut self,
        _event: MouseEvent,
        _data: &mut EditSession,
    ) {
        // Selection already happened in left_down, nothing to do here
    }

    fn left_click(
        &mut self,
        _event: MouseEvent,
        _data: &mut EditSession,
    ) {
        // Click is now handled entirely by left_down
        // This method is called after left_up if no drag occurred
        // But we don't need to do anything here since selection already
        // happened
    }

    fn left_drag_began(
        &mut self,
        event: MouseEvent,
        drag: Drag,
        data: &mut EditSession,
    ) {
        // Check if we're starting the drag on a selected point
        if self.start_dragging_points(event, data) {
            return;
        }

        // Start marquee selection
        self.start_marquee_selection(event, drag, data);
    }

    fn left_drag_changed(
        &mut self,
        event: MouseEvent,
        drag: Drag,
        data: &mut EditSession,
    ) {
        match &mut self.state {
            State::DraggingPoints { last_pos } => {
                handle_dragging_points(event, data, last_pos);
            }
            State::MarqueeSelect {
                previous_selection,
                rect,
                toggle,
            } => {
                handle_marquee_selection(
                    drag,
                    data,
                    previous_selection,
                    rect,
                    *toggle,
                );
            }
            State::Ready => {}
        }
    }

    fn left_drag_ended(
        &mut self,
        _event: MouseEvent,
        _drag: Drag,
        data: &mut EditSession,
    ) {
        match &self.state {
            State::DraggingPoints { .. } => {
                tracing::debug!("Select tool: finished dragging points");
            }
            State::MarqueeSelect { .. } => {
                tracing::debug!(
                    "Select tool: finished marquee selection, \
                     selected {} points",
                    data.selection.len()
                );
                // Update coordinate selection after marquee
                data.update_coord_selection();
            }
            State::Ready => {}
        }

        // Return to ready state
        self.state = State::Ready;
    }

    fn cancel(&mut self, data: &mut EditSession) {
        // If we were in marquee mode, restore the previous selection
        if let State::MarqueeSelect {
            previous_selection, ..
        } = &self.state
        {
            data.selection = previous_selection.clone();
            data.update_coord_selection();
        }

        self.state = State::Ready;
        tracing::debug!("Select tool: cancelled");
    }
}

// ===== Helper Methods =====

impl SelectTool {
    /// Handle point selection (click on a point)
    fn handle_point_selection(
        &self,
        data: &mut EditSession,
        entity: crate::entity_id::EntityId,
        shift: bool,
    ) {
        if shift {
            tracing::debug!("Multi-select mode");
            // Shift+click: toggle selection
            let mut new_selection = data.selection.clone();
            if data.selection.contains(&entity) {
                // Deselect if already selected
                new_selection.remove(&entity);
            } else {
                // Add to selection
                new_selection.insert(entity);
            }
            data.selection = new_selection;
            data.update_coord_selection();
        } else {
            // Normal click: replace selection with just this point
            // UNLESS the point is already selected (then keep current
            // selection for dragging)
            if !data.selection.contains(&entity) {
                let mut new_selection = Selection::new();
                new_selection.insert(entity);
                data.selection = new_selection;
                data.update_coord_selection();
            }
        }
    }

    /// Start dragging selected points
    ///
    /// Returns true if we started dragging points, false otherwise
    fn start_dragging_points(
        &mut self,
        event: MouseEvent,
        data: &mut EditSession,
    ) -> bool {
        // Check if we have any selected points
        // (They were already selected in left_down)
        if data.selection.is_empty() {
            return false;
        }

        // Check if we're starting the drag on a selected point
        let Some(hit) = data.hit_test_point(event.pos, None) else {
            return false;
        };

        if !data.selection.contains(&hit.entity) {
            return false;
        }

        // We're dragging a selected point
        let design_pos = data.viewport.screen_to_design(event.pos);
        self.state = State::DraggingPoints {
            last_pos: design_pos,
        };
        tracing::debug!(
            "Select tool: started dragging {} selected point(s)",
            data.selection.len()
        );
        true
    }

    /// Start marquee selection
    fn start_marquee_selection(
        &mut self,
        event: MouseEvent,
        drag: Drag,
        data: &mut EditSession,
    ) {
        // Store the previous selection for toggle mode
        let previous_selection = data.selection.clone();
        let rect = Rect::from_points(drag.start, drag.current);

        tracing::debug!(
            "Select tool: started marquee selection, toggle={}",
            event.mods.shift
        );
        self.state = State::MarqueeSelect {
            previous_selection,
            rect,
            toggle: event.mods.shift,
        };
    }

}

// ===== Drag Handling Helpers =====

/// Handle dragging points (during drag)
fn handle_dragging_points(
    event: MouseEvent,
    data: &mut EditSession,
    last_pos: &mut Point,
) {
    // Convert current mouse position to design space
    let current_pos = data.viewport.screen_to_design(event.pos);

    // Calculate delta in design space
    let delta = Vec2::new(
        current_pos.x - last_pos.x,
        current_pos.y - last_pos.y,
    );

    // Move selected points
    data.move_selection(delta);

    // Update last position
    *last_pos = current_pos;
}

/// Handle marquee selection (during drag)
fn handle_marquee_selection(
    drag: Drag,
    data: &mut EditSession,
    previous_selection: &Selection,
    rect: &mut Rect,
    toggle: bool,
) {
    // Update the selection rectangle
    *rect = Rect::from_points(drag.start, drag.current);

    // Update selection based on points in rectangle
    update_selection_for_marquee(data, previous_selection, *rect, toggle);
}

// ===== Marquee Selection Helper =====

/// Update selection based on points in the marquee rectangle
///
/// This filters all points to find those within the rectangle (in screen
/// space), and applies toggle logic if shift is held.
fn update_selection_for_marquee(
    data: &mut EditSession,
    previous_selection: &Selection,
    rect: Rect,
    toggle: bool,
) {
    use crate::path::Path;

    // Collect all points that are within the selection rectangle
    let mut new_selection = Selection::new();

    for path in data.paths.iter() {
        match path {
            Path::Cubic(cubic) => {
                for pt in cubic.points.iter() {
                    // Convert point to screen space for hit testing
                    let screen_pos = data.viewport.to_screen(pt.point);

                    // Check if point is inside the rectangle
                    if rect.contains(screen_pos) {
                        new_selection.insert(pt.id);
                    }
                }
            }
            Path::Quadratic(quadratic) => {
                for pt in quadratic.points.iter() {
                    // Convert point to screen space for hit testing
                    let screen_pos = data.viewport.to_screen(pt.point);

                    // Check if point is inside the rectangle
                    if rect.contains(screen_pos) {
                        new_selection.insert(pt.id);
                    }
                }
            }
        }
    }

    // Apply toggle logic if shift is held
    if toggle {
        // Symmetric difference: (previous ∪ new) - (previous ∩ new)
        // This toggles: adds new points, removes previously selected
        // points that are also in new
        let mut result = Selection::new();

        // Add points that are in previous but not in new
        for id in previous_selection.iter() {
            if !new_selection.contains(id) {
                result.insert(*id);
            }
        }

        // Add points that are in new but not in previous
        for id in new_selection.iter() {
            if !previous_selection.contains(id) {
                result.insert(*id);
            }
        }

        data.selection = result;
    } else {
        // Normal mode: replace selection with points in rectangle
        data.selection = new_selection;
    }
}

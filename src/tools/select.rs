// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Select tool for selecting and moving points

use crate::editing::{Drag, EditSession, EditType, MouseDelegate, MouseEvent, Selection};
use crate::path::Segment;
use crate::tools::{Tool, ToolId};
use crate::theme;
use kurbo::Affine;
use kurbo::Point;
use kurbo::Rect;
use kurbo::Vec2;
use masonry::vello::Scene;
use masonry::vello::peniko::Brush;
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
    /// Dragging a selected component
    DraggingComponent {
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

    fn paint(&mut self, scene: &mut Scene, session: &EditSession, _transform: &Affine) {
        // Draw hovered segment highlight (option-click feedback)
        if let Some(ref seg_info) = session.hovered_segment {
            let stroke = kurbo::Stroke::new(theme::segment::HOVER_WIDTH);
            let brush = Brush::Solid(theme::segment::HOVER);
            let offset_x = session.active_sort_x_offset;

            // Convert segment to screen-space BezPath for drawing
            match &seg_info.segment {
                Segment::Line(line) => {
                    let p0 = session.viewport.to_screen(
                        Point::new(line.p0.x + offset_x, line.p0.y),
                    );
                    let p1 = session.viewport.to_screen(
                        Point::new(line.p1.x + offset_x, line.p1.y),
                    );
                    let screen_line = kurbo::Line::new(p0, p1);
                    scene.stroke(
                        &stroke, Affine::IDENTITY, &brush,
                        None, &screen_line,
                    );
                }
                Segment::Cubic(cubic) => {
                    let p0 = session.viewport.to_screen(
                        Point::new(cubic.p0.x + offset_x, cubic.p0.y),
                    );
                    let p1 = session.viewport.to_screen(
                        Point::new(cubic.p1.x + offset_x, cubic.p1.y),
                    );
                    let p2 = session.viewport.to_screen(
                        Point::new(cubic.p2.x + offset_x, cubic.p2.y),
                    );
                    let p3 = session.viewport.to_screen(
                        Point::new(cubic.p3.x + offset_x, cubic.p3.y),
                    );
                    let mut path = kurbo::BezPath::new();
                    path.move_to(p0);
                    path.curve_to(p1, p2, p3);
                    scene.stroke(
                        &stroke, Affine::IDENTITY, &brush,
                        None, &path,
                    );
                }
                Segment::Quadratic(quad) => {
                    let p0 = session.viewport.to_screen(
                        Point::new(quad.p0.x + offset_x, quad.p0.y),
                    );
                    let p1 = session.viewport.to_screen(
                        Point::new(quad.p1.x + offset_x, quad.p1.y),
                    );
                    let p2 = session.viewport.to_screen(
                        Point::new(quad.p2.x + offset_x, quad.p2.y),
                    );
                    let mut path = kurbo::BezPath::new();
                    path.move_to(p0);
                    path.quad_to(p1, p2);
                    scene.stroke(
                        &stroke, Affine::IDENTITY, &brush,
                        None, &path,
                    );
                }
            }
        }

        // Draw selection rectangle if in marquee mode
        let State::MarqueeSelect { rect, .. } = &self.state else {
            return;
        };

        use masonry::util::fill_color;

        // Fill the selection rectangle with semi-transparent orange
        fill_color(scene, rect, theme::selection::RECT_FILL);

        // Stroke the selection rectangle with dashed bright orange
        let stroke = kurbo::Stroke::new(1.5).with_dashes(0.0, [4.0, 4.0]);
        let brush = Brush::Solid(theme::selection::RECT_STROKE);
        scene.stroke(&stroke, Affine::IDENTITY, &brush, None, rect);
    }

    fn edit_type(&self) -> Option<EditType> {
        match &self.state {
            State::DraggingPoints { .. } => Some(EditType::Drag),
            State::DraggingComponent { .. } => Some(EditType::Drag),
            _ => None,
        }
    }
}

// ===== MouseDelegate Implementation =====

#[allow(dead_code)]
impl MouseDelegate for SelectTool {
    type Data = EditSession;

    fn left_down(&mut self, event: MouseEvent, data: &mut EditSession) {
        tracing::debug!(
            "SelectTool::left_down pos={:?} shift={}",
            event.pos,
            event.mods.shift
        );

        // Hit test for a point at the cursor - selection happens HERE,
        // on mouse down
        if let Some(hit) = data.hit_test_point(event.pos, None) {
            tracing::debug!("Hit point: {:?} distance={}", hit.entity, hit.distance);
            // Clear component selection when selecting a point
            data.clear_component_selection();
            self.handle_point_selection(data, hit.entity, event.mods.shift);
        } else if let Some(component_id) = data.hit_test_component(event.pos) {
            // Hit a component - select it
            tracing::debug!("Hit component: {:?}", component_id);
            data.select_component(component_id);
            data.update_coord_selection();
        } else if !event.mods.shift {
            // Clicked on empty space without shift - clear selection
            data.selection = Selection::new();
            data.clear_component_selection();
            data.update_coord_selection();
        }
    }

    fn left_up(&mut self, _event: MouseEvent, _data: &mut EditSession) {
        // Selection already happened in left_down, nothing to do here
    }

    fn left_click(&mut self, _event: MouseEvent, _data: &mut EditSession) {
        // Click is now handled entirely by left_down
        // This method is called after left_up if no drag occurred
        // But we don't need to do anything here since selection already
        // happened
    }

    fn left_drag_began(&mut self, event: MouseEvent, drag: Drag, data: &mut EditSession) {
        // Check if we're starting the drag on a selected point
        if self.start_dragging_points(event, data) {
            return;
        }

        // Check if we're starting the drag on a selected component
        if self.start_dragging_component(event, data) {
            return;
        }

        // Start marquee selection
        self.start_marquee_selection(event, drag, data);
    }

    fn left_drag_changed(&mut self, event: MouseEvent, drag: Drag, data: &mut EditSession) {
        match &mut self.state {
            State::DraggingPoints { last_pos } => {
                handle_dragging_points(event, data, last_pos);
            }
            State::DraggingComponent { last_pos } => {
                handle_dragging_component(event, data, last_pos);
            }
            State::MarqueeSelect {
                previous_selection,
                rect,
                toggle,
            } => {
                handle_marquee_selection(drag, data, previous_selection, rect, *toggle);
            }
            State::Ready => {}
        }
    }

    fn left_drag_ended(&mut self, _event: MouseEvent, _drag: Drag, data: &mut EditSession) {
        match &self.state {
            State::DraggingPoints { .. } => {
                tracing::debug!("Select tool: finished dragging points");
            }
            State::DraggingComponent { .. } => {
                tracing::debug!("Select tool: finished dragging component");
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

    fn mouse_moved(&mut self, event: MouseEvent, data: &mut EditSession) {
        // When option/alt is held, highlight the segment under the
        // cursor to show it can be converted (line → curve)
        if event.mods.alt {
            if let Some((seg_info, _t)) =
                data.hit_test_segments(event.pos, 10.0)
            {
                data.hovered_segment = Some(seg_info);
                return;
            }
        }
        // Clear hover when alt is released or cursor moves away
        data.hovered_segment = None;
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
        entity: crate::model::EntityId,
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
    fn start_dragging_points(&mut self, event: MouseEvent, data: &mut EditSession) -> bool {
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

    /// Start dragging a selected component
    ///
    /// Returns true if we started dragging a component, false otherwise
    fn start_dragging_component(&mut self, event: MouseEvent, data: &mut EditSession) -> bool {
        // Check if we have a selected component
        if data.selected_component.is_none() {
            return false;
        }

        // Check if we're starting the drag on the selected component
        let Some(hit_component) = data.hit_test_component(event.pos) else {
            return false;
        };

        if data.selected_component != Some(hit_component) {
            return false;
        }

        // We're dragging the selected component
        let design_pos = data.viewport.screen_to_design(event.pos);
        self.state = State::DraggingComponent {
            last_pos: design_pos,
        };
        tracing::debug!("Select tool: started dragging component");
        true
    }

    /// Start marquee selection
    fn start_marquee_selection(&mut self, event: MouseEvent, drag: Drag, data: &mut EditSession) {
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
///
/// Moves selected points by the mouse delta, then snaps the first
/// selected on-curve point to the design grid. The snap correction
/// is folded into `last_pos` so that the fractional remainder
/// carries over to the next frame, giving smooth grid-locked dragging.
fn handle_dragging_points(event: MouseEvent, data: &mut EditSession, last_pos: &mut Point) {
    // Convert current mouse position to design space
    let current_pos = data.viewport.screen_to_design(event.pos);

    // Calculate delta in design space
    let delta = Vec2::new(current_pos.x - last_pos.x, current_pos.y - last_pos.y);

    // Option/Alt: move on-curve points independently of their
    // adjacent off-curve handles (Glyphs-style behavior)
    if event.mods.alt {
        data.move_selection_independent(delta);
    } else {
        data.move_selection(delta);
    }

    // Find the first selected on-curve point and snap it to grid.
    // Apply the same snap correction to ALL selected points so they
    // move together.
    let snap_correction = find_snap_correction(data);
    if snap_correction.x.abs() > 1e-9 || snap_correction.y.abs() > 1e-9 {
        if event.mods.alt {
            data.move_selection_independent(snap_correction);
        } else {
            data.move_selection(snap_correction);
        }
    }

    // Update last_pos: advance by the raw delta PLUS the snap
    // correction. This ensures the un-snapped remainder accumulates
    // correctly across frames.
    *last_pos = Point::new(
        last_pos.x + delta.x + snap_correction.x,
        last_pos.y + delta.y + snap_correction.y,
    );
}

/// Find the snap correction for the first selected point.
///
/// On-curve points always snap to grid. Off-curve points also snap
/// to grid when manually dragged; the smooth-point collinearity
/// enforcement (which runs inside `move_selection`) only adjusts
/// the *opposite* handle, so the dragged off-curve stays snapped.
fn find_snap_correction(data: &EditSession) -> Vec2 {
    use crate::editing::session::snap_point_to_grid;
    use crate::path::Path;

    let mut first_selected: Option<kurbo::Point> = None;

    for path in data.paths.iter() {
        let points = match path {
            Path::Cubic(c) => c.points(),
            Path::Quadratic(q) => q.points(),
            Path::Hyper(h) => h.points(),
        };
        for pt in points.iter() {
            if !data.selection.contains(&pt.id) {
                continue;
            }
            // Prefer on-curve as snap reference
            if pt.is_on_curve() {
                let snapped = snap_point_to_grid(pt.point);
                return Vec2::new(
                    snapped.x - pt.point.x,
                    snapped.y - pt.point.y,
                );
            }
            if first_selected.is_none() {
                first_selected = Some(pt.point);
            }
        }
    }

    // Fall back to off-curve point
    if let Some(pos) = first_selected {
        let snapped = snap_point_to_grid(pos);
        return Vec2::new(snapped.x - pos.x, snapped.y - pos.y);
    }

    Vec2::ZERO
}

/// Handle dragging component (during drag)
fn handle_dragging_component(event: MouseEvent, data: &mut EditSession, last_pos: &mut Point) {
    // Convert current mouse position to design space
    let current_pos = data.viewport.screen_to_design(event.pos);

    // Calculate delta in design space
    let delta = Vec2::new(current_pos.x - last_pos.x, current_pos.y - last_pos.y);

    // Move the selected component
    data.move_selected_component(delta);

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
    use kurbo::Point;

    // Collect all points that are within the selection rectangle
    let mut new_selection = Selection::new();

    // Get the active sort's x-offset to apply to hit-testing
    let offset_x = data.active_sort_x_offset;

    for path in data.paths.iter() {
        match path {
            Path::Cubic(cubic) => {
                for pt in cubic.points.iter() {
                    // Apply x-offset in design space before converting to screen
                    let offset_point = Point::new(pt.point.x + offset_x, pt.point.y);
                    let screen_pos = data.viewport.to_screen(offset_point);

                    // Check if point is inside the rectangle
                    if rect.contains(screen_pos) {
                        new_selection.insert(pt.id);
                    }
                }
            }
            Path::Quadratic(quadratic) => {
                for pt in quadratic.points.iter() {
                    // Apply x-offset in design space before converting to screen
                    let offset_point = Point::new(pt.point.x + offset_x, pt.point.y);
                    let screen_pos = data.viewport.to_screen(offset_point);

                    // Check if point is inside the rectangle
                    if rect.contains(screen_pos) {
                        new_selection.insert(pt.id);
                    }
                }
            }
            Path::Hyper(hyper) => {
                for pt in hyper.points.iter() {
                    // Apply x-offset in design space before converting to screen
                    let offset_point = Point::new(pt.point.x + offset_x, pt.point.y);
                    let screen_pos = data.viewport.to_screen(offset_point);

                    // Check if point is inside the rectangle
                    if rect.contains(screen_pos) {
                        new_selection.insert(pt.id);
                    }
                }
            }
        }
    }

    // Apply additive logic if shift is held
    if toggle {
        // Union: keep previous selection and add new points
        let mut result = previous_selection.clone();
        for id in new_selection.iter() {
            result.insert(*id);
        }
        data.selection = result;
    } else {
        // Normal mode: replace selection with points in rectangle
        data.selection = new_selection;
    }
}

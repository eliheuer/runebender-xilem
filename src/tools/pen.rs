// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Pen tool for drawing new paths

use crate::cubic_path::CubicPath;
use crate::edit_session::EditSession;
use crate::edit_types::EditType;
use crate::entity_id::EntityId;
use crate::mouse::{MouseDelegate, MouseEvent};
use crate::path::Path;
use crate::point::{PathPoint, PointType};
use crate::point_list::PathPoints;
use crate::tools::{Tool, ToolId};
use kurbo::Affine;
use masonry::vello::Scene;
use masonry::vello::peniko;
use std::sync::Arc;
use tracing;

// ===== Constants =====

/// Distance threshold for closing a path (in design units)
const CLOSE_PATH_DISTANCE: f64 = 20.0;

/// Distance threshold for snapping to curves (in screen pixels)
const CURVE_SNAP_DISTANCE: f64 = 10.0;

// ===== PenTool Struct =====

/// The pen tool - used for drawing new paths
#[derive(Debug, Clone, Default)]
pub struct PenTool {
    /// Points being added to the current path
    current_path_points: Vec<PathPoint>,

    /// Whether we're actively drawing a path
    drawing: bool,

    /// Current mouse position (for hover detection)
    mouse_pos: Option<kurbo::Point>,

    /// Snapped segment information (segment + parameter t on segment)
    /// When Some, the preview dot should snap to this curve position
    snapped_segment: Option<(crate::path_segment::SegmentInfo, f64)>,
}

// ===== Tool Implementation =====

#[allow(dead_code)]
impl Tool for PenTool {
    fn id(&self) -> ToolId {
        ToolId::Pen
    }

    fn paint(
        &mut self,
        scene: &mut Scene,
        session: &EditSession,
        _transform: &Affine,
    ) {
        use masonry::vello::peniko::Brush;

        let orange_color = crate::theme::point::SELECTED_OUTER;
        let brush = Brush::Solid(orange_color);

        // Check if mouse is hovering near first point (for close feedback)
        let hovering_close = self.check_hovering_close(session);

        // Draw the preview path (orange lines between points) - only
        // while actively drawing. Note: We don't return early here
        // anymore because we want to show the preview dot even when not
        // drawing
        if self.drawing && !self.current_path_points.is_empty() {
            self.draw_preview_path(scene, session, &brush, hovering_close);
        }

        // Draw circles at each point (only when drawing)
        if self.drawing {
            self.draw_path_points(scene, session, &brush, hovering_close);
        }

        // Draw preview circle at current mouse position (showing where
        // next point will be). If snapped to a curve, show the preview
        // dot on the curve instead of at mouse position
        self.draw_preview_dot(scene, session, &brush);
    }

    fn edit_type(&self) -> Option<EditType> {
        if self.drawing {
            Some(EditType::Normal)
        } else {
            None
        }
    }
}

// ===== MouseDelegate Implementation =====

#[allow(dead_code)]
impl MouseDelegate for PenTool {
    type Data = EditSession;

    fn left_click(
        &mut self,
        event: MouseEvent,
        data: &mut EditSession,
    ) {
        // Check if we're snapped to a curve segment
        // If so, insert a point on the segment instead of starting a
        // new path
        if let Some((segment_info, t)) = &self.snapped_segment {
            tracing::debug!(
                "Pen tool: inserting point on curve at t={}",
                t
            );
            data.insert_point_on_segment(segment_info, *t);
            // Clear snapping after insertion
            self.snapped_segment = None;
            return;
        }

        // Convert screen position to design space
        let design_pos = data.viewport.screen_to_design(event.pos);

        // Check if we're clicking near the first point to close the
        // path
        if self.should_close_path(design_pos) {
            tracing::debug!("Pen tool: closing path at first point");
            self.close_path(data);
            return;
        }

        // Create a new on-curve point
        let point = PathPoint {
            id: EntityId::next(),
            point: design_pos,
            typ: PointType::OnCurve { smooth: false },
        };

        self.current_path_points.push(point);
        self.drawing = true;

        tracing::debug!(
            "Pen tool: added point at {:?}, total points: {}",
            design_pos,
            self.current_path_points.len()
        );
    }

    fn mouse_moved(
        &mut self,
        event: MouseEvent,
        data: &mut EditSession,
    ) {
        // Track mouse position for hover feedback
        self.mouse_pos = Some(event.pos);

        // Check for curve snapping (only when not actively drawing a
        // path). This prevents snapping while building a new path
        if !self.drawing {
            // Hit test segments at cursor position
            if let Some((segment_info, t)) = data.hit_test_segments(
                event.pos,
                CURVE_SNAP_DISTANCE,
            ) {
                // Store the snapped segment for rendering and click
                // handling
                self.snapped_segment = Some((segment_info, t));
            } else {
                // Clear snapping if cursor moved away
                self.snapped_segment = None;
            }
        } else {
            // Clear snapping while drawing
            self.snapped_segment = None;
        }
    }

    fn cancel(&mut self, data: &mut EditSession) {
        // Finish the path if we have enough points (Escape key)
        if self.current_path_points.len() >= 2 {
            self.finish_path(data);
        } else {
            // Cancel completely if not enough points
            self.current_path_points.clear();
            self.drawing = false;
        }
        tracing::debug!("Pen tool: finished/cancelled");
    }
}

// ===== Helper Methods =====

impl PenTool {
    /// Check if mouse is hovering near the first point (for close
    /// feedback)
    fn check_hovering_close(
        &self,
        session: &EditSession,
    ) -> bool {
        if !self.drawing || self.current_path_points.len() < 3 {
            return false;
        }

        let Some(mouse_screen) = self.mouse_pos else {
            return false;
        };

        let mouse_design = session.viewport.screen_to_design(mouse_screen);
        let first_point = self.current_path_points[0].point;
        let distance = ((mouse_design.x - first_point.x).powi(2)
            + (mouse_design.y - first_point.y).powi(2))
        .sqrt();

        distance < CLOSE_PATH_DISTANCE
    }

    /// Draw the preview path (orange lines between points)
    fn draw_preview_path(
        &self,
        scene: &mut Scene,
        session: &EditSession,
        brush: &masonry::vello::peniko::Brush,
        hovering_close: bool,
    ) {
        use kurbo::{BezPath, Point};

        let mut bez_path = BezPath::new();
        for (i, pt) in self.current_path_points.iter().enumerate() {
            let design_pt = Point::new(pt.point.x, pt.point.y);
            let screen_pt = session.viewport.to_screen(design_pt);

            if i == 0 {
                bez_path.move_to(screen_pt);
            } else {
                bez_path.line_to(screen_pt);
            }
        }

        // Draw preview line to current mouse position (or closing line
        // if hovering near first point)
        if let Some(mouse_screen) = self.mouse_pos {
            if hovering_close {
                // Show closing line to first point
                if let Some(first_pt) = self.current_path_points.first() {
                    let design_pt = Point::new(
                        first_pt.point.x,
                        first_pt.point.y,
                    );
                    let screen_pt = session.viewport.to_screen(design_pt);
                    bez_path.line_to(screen_pt);
                }
            } else {
                // Show preview line to current mouse position
                bez_path.line_to(mouse_screen);
            }
        }

        // Use dashed stroke for preview (like selection marquee)
        let stroke = kurbo::Stroke::new(2.0).with_dashes(0.0, [4.0, 4.0]);
        scene.stroke(
            &stroke,
            Affine::IDENTITY,
            brush,
            None,
            &bez_path,
        );
    }

    /// Draw circles at each point in the current path
    fn draw_path_points(
        &self,
        scene: &mut Scene,
        session: &EditSession,
        brush: &masonry::vello::peniko::Brush,
        hovering_close: bool,
    ) {
        use kurbo::Point;

        for (i, pt) in self.current_path_points.iter().enumerate() {
            let design_pt = Point::new(pt.point.x, pt.point.y);
            let screen_pt = session.viewport.to_screen(design_pt);

            // First point gets special treatment when hovering
            if i == 0 && hovering_close {
                // Draw larger circle to show close zone
                let close_zone = kurbo::Circle::new(
                    screen_pt,
                    CLOSE_PATH_DISTANCE * session.viewport.zoom,
                );
                let zone_stroke = kurbo::Stroke::new(1.0);
                scene.stroke(
                    &zone_stroke,
                    Affine::IDENTITY,
                    brush,
                    None,
                    &close_zone,
                );
            }

            // Draw point circle
            let circle = kurbo::Circle::new(screen_pt, 4.0);
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                brush,
                None,
                &circle,
            );
        }
    }

    /// Draw preview dot at current mouse position or snapped position
    fn draw_preview_dot(
        &self,
        scene: &mut Scene,
        session: &EditSession,
        brush: &masonry::vello::peniko::Brush,
    ) {
        // Calculate preview position (snapped or raw mouse position)
        let preview_pos = if let Some((segment_info, t)) =
            &self.snapped_segment
        {
            // Evaluate the segment at parameter t to get the snapped
            // position in design space
            let snapped_design_pos = segment_info.segment.eval(*t);
            // Convert to screen space
            Some(session.viewport.to_screen(snapped_design_pos))
        } else {
            // Not snapped - use raw mouse position
            self.mouse_pos
        };

        let Some(preview_screen_pos) = preview_pos else {
            return;
        };

        // Draw the orange preview dot
        let preview_circle = kurbo::Circle::new(preview_screen_pos, 4.0);
        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            brush,
            None,
            &preview_circle,
        );

        // If snapped to a curve, draw a larger circle indicator around
        // it
        if self.snapped_segment.is_some() {
            let indicator_circle =
                kurbo::Circle::new(preview_screen_pos, 8.0);
            let stroke = kurbo::Stroke::new(1.5);
            scene.stroke(
                &stroke,
                Affine::IDENTITY,
                brush,
                None,
                &indicator_circle,
            );
        }
    }

    /// Check if we should close the path (clicking near first point)
    fn should_close_path(&self, design_pos: kurbo::Point) -> bool {
        if self.current_path_points.len() < 3 {
            return false;
        }

        let first_point = self.current_path_points[0].point;
        let distance = ((design_pos.x - first_point.x).powi(2)
            + (design_pos.y - first_point.y).powi(2))
        .sqrt();

        distance < CLOSE_PATH_DISTANCE
    }

    /// Add the finished path to the session (open path)
    fn add_open_path(&mut self, data: &mut EditSession) {
        if self.current_path_points.len() < 2 {
            return;
        }

        // Create a new open path from the points
        let path_points =
            PathPoints::from_vec(self.current_path_points.clone());
        let cubic_path = CubicPath {
            points: path_points,
            closed: false,
            id: EntityId::next(),
        };

        let path = Path::Cubic(cubic_path);
        let mut paths = (*data.paths).clone();
        paths.push(path);
        data.paths = Arc::new(paths);

        tracing::debug!(
            "Pen tool: added open path with {} points",
            self.current_path_points.len()
        );
    }

    /// Close the current path and finish drawing
    fn close_path(&mut self, data: &mut EditSession) {
        if self.current_path_points.len() < 3 {
            return;
        }

        // Create a closed path from the points
        let path_points =
            PathPoints::from_vec(self.current_path_points.clone());
        let cubic_path = CubicPath {
            points: path_points,
            closed: true, // Mark as closed
            id: EntityId::next(),
        };

        let path = Path::Cubic(cubic_path);
        let mut paths = (*data.paths).clone();
        paths.push(path);
        data.paths = Arc::new(paths);

        tracing::debug!(
            "Pen tool: closed path with {} points",
            self.current_path_points.len()
        );

        // Reset for next path
        self.current_path_points.clear();
        self.drawing = false;
    }

    /// Finish drawing and reset for next path (called on Escape or tool
    /// change)
    pub fn finish_path(&mut self, data: &mut EditSession) {
        // If we have enough points, add the open path
        if self.current_path_points.len() >= 2 {
            self.add_open_path(data);
        }

        self.current_path_points.clear();
        self.drawing = false;
    }
}

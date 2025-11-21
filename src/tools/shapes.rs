// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Shapes tool for creating geometric primitives
//!
//! This tool provides a unified interface for creating various shapes:
//! - Rectangle
//! - Ellipse
//! - (Future: RoundedRect, Star, etc.)
//!
//! Ported from Runebender Druid implementation.

use crate::cubic_path::CubicPath;
use crate::edit_session::EditSession;
use crate::edit_types::EditType;
use crate::entity_id::EntityId;
use crate::mouse::{MouseDelegate, MouseEvent};
use crate::path::Path;
use crate::point::{PathPoint, PointType};
use crate::point_list::PathPoints;
use crate::tools::{Tool, ToolId};
use kurbo::{Affine, Point, Rect, Shape};
use masonry::vello::peniko::{Brush, Fill};
use masonry::vello::Scene;
use std::sync::Arc;
use tracing;

// ===== Shape Type =====

/// Type of shape being drawn
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeType {
    /// Rectangle shape
    Rectangle,
    /// Ellipse/circle shape
    Ellipse,
}

// ===== Gesture State =====

/// State of the shape drawing gesture
#[derive(Debug, Clone)]
enum GestureState {
    /// Ready to start drawing
    Ready,
    /// Mouse down, not yet dragging
    Down(Point),
    /// Actively dragging
    Begun { start: Point, current: Point },
    /// Drag finished, ready to finalize
    Finished,
}

// ===== ShapesTool Struct =====

/// The shapes tool - unified tool for creating geometric primitives
#[derive(Debug, Clone)]
pub struct ShapesTool {
    /// Current shape type being drawn
    shape_type: ShapeType,
    /// Drawing gesture state
    gesture: GestureState,
    /// Whether shift key is locked for constraining shapes
    shift_locked: bool,
}

impl Default for ShapesTool {
    fn default() -> Self {
        Self {
            shape_type: ShapeType::Rectangle,
            gesture: GestureState::Ready,
            shift_locked: false,
        }
    }
}

// ===== Tool Implementation =====

impl Tool for ShapesTool {
    fn id(&self) -> ToolId {
        ToolId::Shapes
    }

    fn paint(
        &mut self,
        scene: &mut Scene,
        session: &EditSession,
        _transform: &Affine,
    ) {
        // Paint preview of the shape being drawn
        if let Some(rect) = self.current_drag_rect(session) {
            self.paint_shape_preview(scene, rect);
        }
    }

    fn edit_type(&self) -> Option<EditType> {
        match self.gesture {
            GestureState::Begun { .. } => Some(EditType::Normal),
            _ => None,
        }
    }
}

// ===== MouseDelegate Implementation =====

impl MouseDelegate for ShapesTool {
    type Data = EditSession;

    fn left_down(&mut self, event: MouseEvent, _data: &mut EditSession) {
        self.gesture = GestureState::Down(event.pos);
        // Note: shift_locked is controlled by keyboard events, not mouse events
        tracing::debug!("Shapes tool: mouse down at {:?}, shift={}", event.pos, self.shift_locked);
    }

    fn left_drag_began(
        &mut self,
        event: MouseEvent,
        drag: crate::mouse::Drag,
        data: &mut EditSession,
    ) {
        // Convert to design space
        let start = data.viewport.screen_to_design(drag.start);
        let current = data.viewport.screen_to_design(drag.current);

        // Note: shift_locked is controlled by keyboard events, not mouse events
        self.gesture = GestureState::Begun { start, current };
        tracing::debug!("Shapes tool: drag began, shift={}", self.shift_locked);
    }

    fn left_drag_changed(
        &mut self,
        event: MouseEvent,
        drag: crate::mouse::Drag,
        data: &mut EditSession,
    ) {
        if let GestureState::Begun { start, .. } = self.gesture {
            let current = data.viewport.screen_to_design(drag.current);
            // Note: shift_locked is controlled by keyboard events, not mouse events
            self.gesture = GestureState::Begun { start, current };
        }
    }

    fn left_drag_ended(
        &mut self,
        event: MouseEvent,
        drag: crate::mouse::Drag,
        data: &mut EditSession,
    ) {
        if let GestureState::Begun { start, .. } = self.gesture {
            let current = data.viewport.screen_to_design(drag.current);

            // Note: shift_locked is controlled by keyboard events, not mouse events
            // The final shape uses whatever shift state was set by keyboard

            // Get the final rectangle
            let (p0, p1) = self.pts_for_rect(start, current);
            let rect = Rect::from_points(p0, p1);

            // Create path based on shape type
            let path = match self.shape_type {
                ShapeType::Rectangle => self.make_rect_path(rect),
                ShapeType::Ellipse => self.make_ellipse_path(rect),
            };

            // Add to session (Arc pattern - clone, modify, reassign)
            let mut paths = (*data.paths).clone();
            paths.push(path);
            data.paths = Arc::new(paths);

            self.gesture = GestureState::Finished;
            tracing::debug!("Shapes tool: created {:?}", self.shape_type);
        }
    }

    fn left_up(&mut self, _event: MouseEvent, _data: &mut EditSession) {
        // Reset gesture state
        self.gesture = GestureState::Ready;
    }

    fn cancel(&mut self, _data: &mut EditSession) {
        self.gesture = GestureState::Ready;
        tracing::debug!("Shapes tool: cancelled");
    }
}

// ===== Helper Methods =====

impl ShapesTool {
    /// Set the current shape type
    pub fn set_shape_type(&mut self, shape_type: ShapeType) {
        self.shape_type = shape_type;
    }

    /// Get the current shape type
    pub fn shape_type(&self) -> ShapeType {
        self.shape_type
    }

    /// Set the shift-lock state (for keyboard-driven constraint toggling)
    pub fn set_shift_locked(&mut self, locked: bool) {
        self.shift_locked = locked;
    }

    /// Get points for rectangle, applying shift-lock constraint if needed
    fn pts_for_rect(&self, start: Point, current: Point) -> (Point, Point) {
        if self.shift_locked {
            // Constrain to square/circle
            let delta = current - start;
            let size = delta.x.abs().max(delta.y.abs());
            let constrained = Point::new(
                start.x + size * delta.x.signum(),
                start.y + size * delta.y.signum(),
            );
            tracing::debug!(
                "pts_for_rect: CONSTRAINED - start={:?}, current={:?}, delta={:?}, size={}, constrained={:?}",
                start, current, delta, size, constrained
            );
            (start, constrained)
        } else {
            tracing::debug!("pts_for_rect: unconstrained - start={:?}, current={:?}", start, current);
            (start, current)
        }
    }

    /// Get the current drag rectangle in screen space for preview
    fn current_drag_rect(&self, session: &EditSession) -> Option<Rect> {
        if let GestureState::Begun { start, current } = self.gesture {
            let (p0, p1) = self.pts_for_rect(start, current);

            // Convert to screen space
            let screen_p0 = session.viewport.to_screen(p0);
            let screen_p1 = session.viewport.to_screen(p1);

            Some(Rect::from_points(screen_p0, screen_p1))
        } else {
            None
        }
    }

    /// Paint shape preview during drag
    fn paint_shape_preview(&self, scene: &mut Scene, rect: Rect) {
        let brush = Brush::Solid(crate::theme::tool_preview::LINE_COLOR);

        match self.shape_type {
            ShapeType::Rectangle => {
                // Draw dashed rectangle outline
                let stroke = kurbo::Stroke::new(crate::theme::tool_preview::LINE_WIDTH)
                    .with_dashes(
                        crate::theme::tool_preview::LINE_DASH_OFFSET,
                        crate::theme::tool_preview::LINE_DASH,
                    );
                scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &rect);
            }
            ShapeType::Ellipse => {
                // Draw dashed ellipse outline
                let ellipse = kurbo::Ellipse::from_rect(rect);
                let stroke = kurbo::Stroke::new(crate::theme::tool_preview::LINE_WIDTH)
                    .with_dashes(
                        crate::theme::tool_preview::LINE_DASH_OFFSET,
                        crate::theme::tool_preview::LINE_DASH,
                    );
                scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &ellipse);
            }
        }

        // Draw corner dots
        let dot_radius = crate::theme::tool_preview::DOT_RADIUS;
        for &pt in &[rect.origin(), rect.origin() + rect.size().to_vec2()] {
            let circle = kurbo::Circle::new(pt, dot_radius);
            scene.fill(Fill::NonZero, Affine::IDENTITY, &brush, None, &circle);
        }
    }

    /// Create a rectangle path from a rect
    fn make_rect_path(&self, rect: Rect) -> Path {
        // Rectangle has 4 corners, all on-curve (line points)
        let p0 = rect.origin();
        let p1 = Point::new(rect.max_x(), rect.min_y());
        let p2 = Point::new(rect.max_x(), rect.max_y());
        let p3 = Point::new(rect.min_x(), rect.max_y());

        // Create path points (all on-curve corners for straight lines)
        let points = vec![
            PathPoint {
                id: EntityId::next(),
                point: p0,
                typ: PointType::OnCurve { smooth: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: p1,
                typ: PointType::OnCurve { smooth: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: p2,
                typ: PointType::OnCurve { smooth: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: p3,
                typ: PointType::OnCurve { smooth: false },
            },
        ];

        let path_points = PathPoints::from_vec(points);
        let cubic = CubicPath {
            points: path_points,
            closed: true,
            id: EntityId::next(),
        };

        Path::Cubic(cubic)
    }

    /// Create an ellipse path from a rect (bounding box)
    fn make_ellipse_path(&self, rect: Rect) -> Path {
        // Use kurbo's ellipse-to-path conversion
        let ellipse = kurbo::Ellipse::from_rect(rect);
        let bez_path = ellipse.to_path(0.1); // tolerance for curve approximation

        // Convert BezPath to PathPoints
        let mut points = Vec::new();
        let mut off_curve_buffer: Vec<Point> = Vec::new();

        for el in bez_path.elements() {
            match el {
                kurbo::PathEl::MoveTo(p) => {
                    // First point is typically a move - create as smooth on-curve point
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p,
                        typ: PointType::OnCurve { smooth: true },
                    });
                }
                kurbo::PathEl::LineTo(p) => {
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p,
                        typ: PointType::OnCurve { smooth: false },
                    });
                }
                kurbo::PathEl::QuadTo(_p1, _p2) => {
                    // Quadratics not typically generated by ellipse.to_path()
                    // but handle just in case
                }
                kurbo::PathEl::CurveTo(p1, p2, p3) => {
                    // Add off-curve points
                    off_curve_buffer.push(*p1);
                    off_curve_buffer.push(*p2);
                    // Add on-curve point
                    // Flush off-curve points first
                    for off_p in off_curve_buffer.drain(..) {
                        points.push(PathPoint {
                            id: EntityId::next(),
                            point: off_p,
                            typ: PointType::OffCurve { auto: false },
                        });
                    }
                    points.push(PathPoint {
                        id: EntityId::next(),
                        point: *p3,
                        typ: PointType::OnCurve { smooth: true },
                    });
                }
                kurbo::PathEl::ClosePath => {
                    // Path will be marked as closed
                }
            }
        }

        let path_points = PathPoints::from_vec(points);
        let cubic = CubicPath {
            points: path_points,
            closed: true,
            id: EntityId::next(),
        };

        Path::Cubic(cubic)
    }
}

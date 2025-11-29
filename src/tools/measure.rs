// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

//! Measure tool for measuring distances and angles
//!
//! Ported from Runebender Druid implementation.

use crate::edit_session::EditSession;
use crate::edit_types::EditType;
use crate::mouse::{MouseDelegate, MouseEvent};
use crate::tools::{Tool, ToolId};
use kurbo::{Affine, Circle, Line, Point, Rect, Size};
use masonry::vello::peniko::{Brush, Color, Fill};
use masonry::vello::Scene;
use masonry::core::{BrushIndex, StyleProperty, render_text};
use parley::GenericFamily;
use parley::{FontContext, LayoutContext};
use tracing;

// ===== Constants =====

/// Tolerance for fuzzy intersection clustering
const MEASURE_FUZZY_TOLERANCE: f64 = 0.1;

// ===== MeasureTool Struct =====

/// The measure tool - used for measuring distances and angles
#[derive(Debug, Clone, Default)]
pub struct MeasureTool {
    /// The measurement line (in screen space), None when not measuring
    line: Option<Line>,
    /// Whether we're actively dragging to create a measurement
    dragging: bool,
    /// Start point of drag (in screen space)
    drag_start: Option<Point>,
}

// ===== Tool Implementation =====

impl Tool for MeasureTool {
    fn id(&self) -> ToolId {
        ToolId::Measure
    }

    fn paint(
        &mut self,
        scene: &mut Scene,
        session: &EditSession,
        _transform: &Affine,
    ) {
        // Paint the measurement line and info if present
        if let Some(line) = self.line {
            self.paint_measurement(scene, session, line);
        }
    }

    fn edit_type(&self) -> Option<EditType> {
        if self.dragging {
            Some(EditType::Normal)
        } else {
            None
        }
    }
}

// ===== MouseDelegate Implementation =====

impl MouseDelegate for MeasureTool {
    type Data = EditSession;

    fn left_down(&mut self, event: MouseEvent, _data: &mut EditSession) {
        self.dragging = true;
        self.drag_start = Some(event.pos);
        self.line = Some(Line::new(event.pos, event.pos));
        tracing::debug!("Measure tool: started drag at {:?}", event.pos);
    }

    fn left_drag_changed(
        &mut self,
        _event: MouseEvent,
        drag: crate::mouse::Drag,
        _data: &mut EditSession,
    ) {
        if self.dragging
            && let Some(start) = self.drag_start {
                // TODO: Add shift-key axis locking support when we have modifier key state
                self.line = Some(Line::new(start, drag.current));
            }
    }

    fn left_up(&mut self, _event: MouseEvent, _data: &mut EditSession) {
        self.dragging = false;
        // Keep the line visible after drag ends
        tracing::debug!("Measure tool: finished drag");
    }

    fn cancel(&mut self, _data: &mut EditSession) {
        self.line = None;
        self.dragging = false;
        self.drag_start = None;
        tracing::debug!("Measure tool: cancelled");
    }
}

// ===== Helper Methods =====

impl MeasureTool {
    /// Paint coordinate labels on all points in all paths
    /// Note: Currently disabled since Vello doesn't support text rendering yet
    #[allow(dead_code)]
    fn paint_coords(&self, _scene: &mut Scene, _session: &EditSession) {
        // TODO: Implement when Vello text rendering is available
        // For now, we just show the measurement line, not coordinate labels
    }

    /// Paint the measurement line and all associated info
    fn paint_measurement(&self, scene: &mut Scene, session: &EditSession, line: Line) {
        // Draw the measurement line using theme colors (same as pen/knife preview)
        let stroke = kurbo::Stroke::new(crate::theme::tool_preview::LINE_WIDTH)
            .with_dashes(
                crate::theme::tool_preview::LINE_DASH_OFFSET,
                crate::theme::tool_preview::LINE_DASH,
            );
        let brush = Brush::Solid(crate::theme::tool_preview::LINE_COLOR);
        scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &line);

        // TODO: Angle display - commented out for now but may be useful later
        // // Calculate and display angle
        // let angle = atan_to_angle((line.p1 - line.p0).atan2());
        // let angle_offset = if angle < 90.0 {
        //     Vec2::new(14.0, -6.0)
        // } else if angle < 180.0 {
        //     Vec2::new(-14.0, -6.0)
        // } else {
        //     Vec2::new(-14.0, 8.0)
        // };
        // let angle_label = format!("{:.1}°", angle);
        // draw_info_bubble(scene, line.p1 + angle_offset, angle_label);

        // Convert line to design space
        let p0 = session.viewport.screen_to_design(line.p0);
        let p1 = session.viewport.screen_to_design(line.p1);
        let design_line = Line::new(p0, p1);
        let design_len = (design_line.p1 - design_line.p0).hypot();

        // Compute intersections with paths
        let intersections = self.compute_measurement(session, design_line);

        // Draw dots at intersection points using theme radius
        for t in &intersections {
            let pt = line.p0.lerp(line.p1, *t);
            let circle = Circle::new(pt, crate::theme::tool_preview::DOT_RADIUS);
            scene.fill(Fill::NonZero, Affine::IDENTITY, &brush, None, &circle);
        }

        // Draw length labels between intersections
        for i in 0..intersections.len().saturating_sub(1) {
            let t0 = intersections[i];
            let t1 = intersections[i + 1];
            let tmid = 0.5 * (t0 + t1);
            let seg_len = design_len * (t1 - t0);
            let center = design_line.p0.lerp(design_line.p1, tmid);
            let center_screen = session.viewport.to_screen(center);
            let len_label = format!("{:.1}", seg_len);
            draw_info_bubble(scene, center_screen, len_label);
        }
    }

    /// Compute measurement intersections with all paths
    /// Returns a sorted list of t values (0.0 to 1.0) along the measurement line
    fn compute_measurement(&self, session: &EditSession, design_line: Line) -> Vec<f64> {
        const T_SCALE: f64 = (1u64 << 63) as f64;
        let mut intersections = vec![0, T_SCALE as u64];

        // Find all intersections with path segments
        for path in session.paths.iter() {
            match path {
                crate::path::Path::Cubic(cubic) => {
                    for seg_info in cubic.iter_segments() {
                        // Use intersection function from knife tool
                        let hits = crate::tools::knife::intersect_line_segment(
                            design_line,
                            &seg_info.segment,
                        );
                        for (_seg_t, line_t) in hits {
                            let t_fixed = (line_t.clamp(0.0, 1.0) * T_SCALE) as u64;
                            intersections.push(t_fixed);
                        }
                    }
                }
                crate::path::Path::Hyper(hyper) => {
                    for seg_info in hyper.iter_segments() {
                        // Use intersection function from knife tool
                        let hits = crate::tools::knife::intersect_line_segment(
                            design_line,
                            &seg_info.segment,
                        );
                        for (_seg_t, line_t) in hits {
                            let t_fixed = (line_t.clamp(0.0, 1.0) * T_SCALE) as u64;
                            intersections.push(t_fixed);
                        }
                    }
                }
                crate::path::Path::Quadratic(_) => {
                    // Quadratic paths not yet supported for measurement
                }
            }
        }

        intersections.sort_unstable();

        // Cluster nearby intersections (fuzzy tolerance)
        let line_len = (design_line.p1 - design_line.p0).hypot();
        // Avoid division by zero or very small numbers
        let thresh = if line_len > 1e-6 {
            MEASURE_FUZZY_TOLERANCE / line_len
        } else {
            f64::INFINITY
        };

        let mut result = Vec::with_capacity(intersections.len());
        let mut t_cluster_start = -1.0;
        let mut t_last = -1.0;

        for t_fixed in intersections {
            let t = t_fixed as f64 / T_SCALE;
            if t - t_last > thresh {
                t_cluster_start = t;
                result.push(t);
            } else if let Some(last) = result.last_mut() {
                // Merge into existing cluster
                let cluster_t = if t_cluster_start == 0.0 {
                    0.0
                } else if t == 1.0 {
                    1.0
                } else {
                    0.5 * (t_cluster_start + t)
                };
                *last = cluster_t;
            }
            t_last = t;
        }

        result
    }
}

// ===== Standalone Helper Functions =====

/// Convert atan2 to angle in degrees
fn atan_to_angle(atan: f64) -> f64 {
    if !atan.is_finite() {
        return 0.0;
    }
    let mut angle = atan * (-180.0 / std::f64::consts::PI);
    if angle < -90.0 {
        angle += 360.0;
    }
    angle
}

/// Format a point as "x, y" with one decimal place (trimming .0)
fn format_point(pt: Point) -> String {
    let x = format!("{:.1}", pt.x);
    let y = format!("{:.1}", pt.y);
    format!(
        "{}, {}",
        x.trim_end_matches(".0"),
        y.trim_end_matches(".0")
    )
}

/// Draw a text label at a position with a given color
/// Note: Vello doesn't have text rendering yet, so this is a placeholder
/// that draws a small colored circle instead
fn draw_label(_scene: &mut Scene, _label: String, _pos: Point, _color: Color) {
    // TODO: Implement text rendering when Vello supports it
    // For now, we skip drawing text labels
    // This would need vello-text or a similar text rendering solution
}

/// Draw an info bubble with text (rounded rectangle background + text)
fn draw_info_bubble(scene: &mut Scene, pos: Point, label: impl Into<String>) {
    let label_str = label.into();

    // Format the number - if it's a whole number, show no decimal
    let formatted_label = if let Ok(num) = label_str.parse::<f64>() {
        if num.fract() == 0.0 {
            format!("{}", num as i64)
        } else {
            format!("{:.1}", num)
        }
    } else {
        label_str
    };

    // Create text layout
    let mut font_cx = FontContext::default();
    let mut layout_cx = LayoutContext::new();

    let mut builder = layout_cx.ranged_builder(&mut font_cx, &formatted_label, 1.0, false);
    builder.push_default(StyleProperty::FontSize(14.0));
    builder.push_default(StyleProperty::FontStack(parley::FontStack::Single(
        parley::FontFamily::Generic(GenericFamily::SansSerif)
    )));
    builder.push_default(StyleProperty::Brush(BrushIndex(0))); // Index into brushes array
    let mut layout = builder.build(&formatted_label);
    layout.break_all_lines(None);

    // Get text dimensions
    let text_width = layout.width() as f64;
    let text_height = layout.height() as f64;

    // Draw green background bubble (like knife tool X marks)
    let bubble_padding = 4.0;
    let bubble = Rect::from_center_size(
        pos,
        Size::new(text_width + bubble_padding * 2.0, text_height + bubble_padding * 2.0)
    ).to_rounded_rect(4.0);

    let bubble_brush = Brush::Solid(crate::theme::point::CORNER_INNER);
    scene.fill(Fill::NonZero, Affine::IDENTITY, &bubble_brush, None, &bubble);

    // Draw dark gray text on top
    let text_pos = Point::new(
        pos.x - text_width / 2.0,
        pos.y - text_height / 2.0,
    );

    let text_color = Color::from_rgb8(0x30, 0x30, 0x30); // Dark gray
    let brushes = vec![Brush::Solid(text_color)];

    render_text(
        scene,
        Affine::translate((text_pos.x, text_pos.y)),
        &layout,
        &brushes,
        false, // No hinting
    );
}

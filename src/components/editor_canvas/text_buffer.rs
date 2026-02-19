// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Text buffer rendering for EditorWidget (multi-sort layout)

use super::EditorWidget;
use crate::model::read_workspace;
use crate::theme;
use kurbo::{Affine, Point, Stroke};
use masonry::vello::Scene;
use masonry::vello::peniko::Brush;

impl EditorWidget {
    /// Render the text buffer with multiple sorts (Phase 3)
    ///
    /// This renders all sorts in the text buffer, laying them out horizontally
    /// with correct spacing based on advance widths.
    pub(super) fn render_text_buffer(
        &self,
        scene: &mut Scene,
        transform: &Affine,
        is_preview_mode: bool,
    ) {
        let buffer = match &self.session.text_buffer {
            Some(buf) => buf,
            None => return,
        };

        // Check text direction for RTL support
        let is_rtl = self.session.text_direction.is_rtl();

        // For RTL: calculate total width first so we can start from the right
        let total_width = if is_rtl {
            self.calculate_buffer_width()
        } else {
            0.0 // Not needed for LTR
        };

        // Starting position: LTR starts at 0, RTL starts at total_width
        let mut x_offset = if is_rtl { total_width } else { 0.0 };
        let mut baseline_y = 0.0;
        let cursor_position = buffer.cursor();

        // UPM height for line spacing (top of UPM on new line aligns with bottom of descender on previous line)
        let upm_height = self.session.ascender - self.session.descender;

        // Phase 6: Calculate cursor position while rendering sorts
        let mut cursor_x = x_offset;
        let mut cursor_y = 0.0;

        // Track previous glyph for kerning lookup
        let mut prev_glyph_name: Option<String> = None;
        let mut prev_glyph_group: Option<String> = None;

        tracing::debug!(
            "[Cursor] buffer.len()={}, cursor_position={}, is_rtl={}",
            buffer.len(),
            cursor_position,
            is_rtl
        );

        // Cursor at position 0 (before any sorts)
        if cursor_position == 0 {
            cursor_x = x_offset;
            cursor_y = baseline_y;
            tracing::debug!("[Cursor] Position 0: ({}, {})", cursor_x, cursor_y);
        }

        // Collect sort render data for two-pass rendering
        // (First pass: metrics behind, second pass: glyphs on top)
        struct SortRenderData {
            index: usize,
            name: String,
            x_offset: f64,
            baseline_y: f64,
            advance_width: f64,
            is_active: bool,
        }
        let mut sort_render_data: Vec<SortRenderData> = Vec::new();

        for (index, sort) in buffer.iter().enumerate() {
            match &sort.kind {
                crate::sort::SortKind::Glyph {
                    name,
                    advance_width,
                    ..
                } => {
                    // For RTL: move x left BEFORE drawing this glyph
                    if is_rtl {
                        x_offset -= advance_width;
                    }

                    // Apply kerning if we have a previous glyph
                    if let Some(prev_name) = &prev_glyph_name
                        && let Some(workspace_arc) = &self.session.workspace
                    {
                        let workspace = read_workspace(workspace_arc);

                        // Get current glyph's left kerning group
                        let curr_group = workspace
                            .get_glyph(name)
                            .and_then(|g| g.left_group.as_deref());

                        // Look up kerning value
                        let kern_value = crate::model::kerning::lookup_kerning(
                            &workspace.kerning,
                            &workspace.groups,
                            prev_name,
                            prev_glyph_group.as_deref(),
                            name,
                            curr_group,
                        );

                        if is_rtl {
                            x_offset -= kern_value; // RTL: kerning moves left
                        } else {
                            x_offset += kern_value;
                        }
                    }

                    // Store data for two-pass rendering
                    sort_render_data.push(SortRenderData {
                        index,
                        name: name.clone(),
                        x_offset,
                        baseline_y,
                        advance_width: *advance_width,
                        is_active: sort.is_active,
                    });

                    // For LTR: advance x forward AFTER processing
                    if !is_rtl {
                        x_offset += advance_width;
                    }
                    // (For RTL, we already moved x_offset before drawing)

                    // Update previous glyph info for next iteration
                    prev_glyph_name = Some(name.clone());
                    if let Some(workspace_arc) = &self.session.workspace {
                        let workspace = read_workspace(workspace_arc);
                        prev_glyph_group = workspace
                            .get_glyph(name)
                            .and_then(|g| g.right_group.clone());
                    }
                }
                crate::sort::SortKind::LineBreak => {
                    // Line break: reset x, move y down by UPM height
                    // Top of UPM on new line aligns with bottom of descender on previous line
                    x_offset = if is_rtl { total_width } else { 0.0 };
                    baseline_y -= upm_height;

                    // Reset kerning tracking (no kerning across lines)
                    prev_glyph_name = None;
                    prev_glyph_group = None;
                }
            }

            // Track cursor position AFTER processing this sort
            // The cursor is positioned after the sort at this index
            if index + 1 == cursor_position {
                cursor_x = x_offset;
                cursor_y = baseline_y;
                tracing::debug!(
                    "[Cursor] After sort {}: ({}, {})",
                    index,
                    cursor_x,
                    cursor_y
                );
            }
        }

        // PASS 1: Render all metrics FIRST (behind glyphs)
        if !is_preview_mode {
            for data in &sort_render_data {
                if self.session.text_mode_active {
                    // Determine metrics color based on kern mode
                    let metrics_color = if self.kern_mode_active {
                        if Some(data.index) == self.kern_sort_index {
                            // Active dragged glyph: bright turquoise-green
                            masonry::vello::peniko::Color::from_rgb8(0x00, 0xff, 0xcc)
                        } else if Some(data.index + 1) == self.kern_sort_index {
                            // Previous glyph: orange (selection marquee color)
                            masonry::vello::peniko::Color::from_rgb8(0xff, 0xaa, 0x33)
                        } else {
                            // Normal gray
                            theme::metrics::GUIDE
                        }
                    } else {
                        // Normal gray when not in kern mode
                        theme::metrics::GUIDE
                    };

                    // Text mode: minimal metrics for all sorts
                    self.render_sort_minimal_metrics(
                        scene,
                        data.x_offset,
                        data.baseline_y,
                        data.advance_width,
                        transform,
                        metrics_color,
                    );
                } else if data.is_active {
                    // Non-text mode: full metrics only for active sort
                    self.render_sort_metrics(
                        scene,
                        data.x_offset,
                        data.baseline_y,
                        data.advance_width,
                        transform,
                    );
                }
                // Inactive sorts in non-text mode: no metrics at all
            }
        }

        // PASS 2: Render all glyphs SECOND (on top of metrics)
        for data in &sort_render_data {
            let sort_position = Point::new(data.x_offset, data.baseline_y);

            if data.is_active && !is_preview_mode && !self.session.text_mode_active {
                // Non-text mode: render active sort with control points (editable)
                self.render_active_sort(scene, &data.name, sort_position, transform);
            } else {
                // All other cases: render as filled preview
                self.render_inactive_sort(scene, &data.name, sort_position, transform);
            }
        }

        // Cursor might be at the end of the buffer (after all sorts)
        if cursor_position >= buffer.len() {
            cursor_x = x_offset;
            cursor_y = baseline_y;
            tracing::debug!("[Cursor] At end: ({}, {})", cursor_x, cursor_y);
        }

        tracing::debug!("[Cursor] Final position: ({}, {})", cursor_x, cursor_y);

        // Phase 6: Render cursor in text mode (not in preview mode)
        if !is_preview_mode {
            self.render_text_cursor(scene, cursor_x, cursor_y, transform);
        }
    }

    /// Calculate the total width of the text buffer (for RTL rendering)
    ///
    /// This sums all glyph advance widths to determine where RTL text should start.
    pub(super) fn calculate_buffer_width(&self) -> f64 {
        let buffer = match &self.session.text_buffer {
            Some(buf) => buf,
            None => return 0.0,
        };

        let mut total_width = 0.0;
        for sort in buffer.iter() {
            if let crate::sort::SortKind::Glyph { advance_width, .. } = &sort.kind {
                total_width += advance_width;
            }
        }
        total_width
    }

    /// Render an active sort with control points and handles
    fn render_active_sort(
        &self,
        scene: &mut Scene,
        _glyph_name: &str,
        position: Point,
        transform: &Affine,
    ) {
        use super::drawing::draw_paths_with_points;

        // Render the active sort using session.paths (the editable version)
        // The caller already verified this is the active sort via sort.is_active

        // Apply position offset
        let sort_transform = *transform * Affine::translate(position.to_vec2());

        // Render path stroke
        let mut glyph_path = kurbo::BezPath::new();
        for path in self.session.paths.iter() {
            glyph_path.extend(path.to_bezpath());
        }

        if !glyph_path.is_empty() {
            let transformed_path = sort_transform * &glyph_path;
            let stroke = Stroke::new(theme::size::PATH_STROKE_WIDTH);
            let brush = Brush::Solid(theme::path::STROKE);
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &transformed_path);

            // Draw control points and handles
            // Note: This uses session paths which already have the correct structure
            draw_paths_with_points(scene, &self.session, &sort_transform);
        }

        // Render components for the active glyph
        // Use distinct color only in non-text mode (to distinguish from editable paths)
        if let Some(workspace) = &self.session.workspace {
            let workspace_guard = read_workspace(workspace);
            let use_component_color = !self.session.text_mode_active;
            self.render_glyph_components(
                scene,
                &self.session.glyph,
                &sort_transform,
                &workspace_guard,
                true, // is_active_sort
                use_component_color,
            );
        }
    }

    /// Render an inactive sort as a filled preview
    fn render_inactive_sort(
        &self,
        scene: &mut Scene,
        glyph_name: &str,
        position: Point,
        transform: &Affine,
    ) {
        // Load glyph from workspace and render as filled
        let workspace = match &self.session.workspace {
            Some(ws) => ws,
            None => return,
        };

        let workspace_guard = read_workspace(workspace);
        let glyph = match workspace_guard.glyphs.get(glyph_name) {
            Some(g) => g,
            None => {
                tracing::warn!("Glyph '{}' not found in workspace", glyph_name);
                return;
            }
        };

        // Apply position offset
        let sort_transform = *transform * Affine::translate(position.to_vec2());

        // Build BezPath from glyph contours
        let mut glyph_path = kurbo::BezPath::new();
        for contour in &glyph.contours {
            let path = crate::path::Path::from_contour(contour);
            glyph_path.extend(path.to_bezpath());
        }

        if !glyph_path.is_empty() {
            let transformed_path = sort_transform * &glyph_path;
            let fill_brush = Brush::Solid(theme::path::PREVIEW_FILL);
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                &fill_brush,
                None,
                &transformed_path,
            );
        }

        // Render components (references to other glyphs)
        // Inactive sorts always use regular fill color (not distinct component color)
        self.render_glyph_components(
            scene,
            glyph,
            &sort_transform,
            &workspace_guard,
            false, // is_active_sort
            false, // use_component_color
        );
    }

    /// Render components of a glyph recursively
    ///
    /// Components are rendered in a distinct color only when in an active sort
    /// in non-text mode (to distinguish from editable paths). In text mode or
    /// inactive sorts, they use the same fill color as regular glyphs.
    ///
    /// Parameters:
    /// - `is_active_sort`: true if rendering the active (editable) sort
    /// - `use_component_color`: true to use distinct component color (blue)
    fn render_glyph_components(
        &self,
        scene: &mut Scene,
        glyph: &crate::model::workspace::Glyph,
        transform: &Affine,
        workspace: &crate::model::workspace::Workspace,
        is_active_sort: bool,
        use_component_color: bool,
    ) {
        for component in &glyph.components {
            // Look up the base glyph
            let base_glyph = match workspace.glyphs.get(&component.base) {
                Some(g) => g,
                None => {
                    tracing::warn!(
                        "Component base glyph '{}' not found in workspace",
                        component.base
                    );
                    continue;
                }
            };

            // Combine transform: parent transform * component transform
            let component_transform = *transform * component.transform;

            // Build BezPath from base glyph's contours
            let mut component_path = kurbo::BezPath::new();
            for contour in &base_glyph.contours {
                let path = crate::path::Path::from_contour(contour);
                component_path.extend(path.to_bezpath());
            }

            // Render the component
            if !component_path.is_empty() {
                let transformed_path = component_transform * &component_path;

                // Check if this component is selected (only relevant for active sort)
                let is_selected =
                    is_active_sort && self.session.selected_component == Some(component.id);

                // Determine fill color based on context
                let fill_color = if is_selected {
                    // Brighter blue for selected component
                    peniko::Color::from_rgb8(0x88, 0xbb, 0xff)
                } else if use_component_color {
                    // Blue for components in active sort (non-text mode)
                    theme::component::FILL
                } else {
                    // Same as regular glyph fill for text mode or inactive sorts
                    theme::path::PREVIEW_FILL
                };

                let fill_brush = Brush::Solid(fill_color);
                scene.fill(
                    peniko::Fill::NonZero,
                    Affine::IDENTITY,
                    &fill_brush,
                    None,
                    &transformed_path,
                );

                // Draw selection outline if selected
                if is_selected {
                    let stroke = Stroke::new(2.0);
                    let stroke_brush = Brush::Solid(theme::selection::RECT_STROKE);
                    scene.stroke(
                        &stroke,
                        Affine::IDENTITY,
                        &stroke_brush,
                        None,
                        &transformed_path,
                    );
                }
            }

            // Recursively render nested components
            self.render_glyph_components(
                scene,
                base_glyph,
                &component_transform,
                workspace,
                is_active_sort,
                use_component_color,
            );
        }
    }

    /// Render the text cursor (Phase 6)
    ///
    /// Draws a vertical line at the cursor position in design space, aligned with sort metrics
    /// Only visible in text edit mode. Includes triangular indicators at top and bottom.
    fn render_text_cursor(
        &self,
        scene: &mut Scene,
        cursor_x: f64,
        baseline_y: f64,
        transform: &Affine,
    ) {
        // Only render cursor in text edit mode
        if !self.session.text_mode_active {
            return;
        }

        // Draw cursor as a vertical line from ascender to descender (matching sort metrics)
        // Offset by baseline_y to support multi-line text
        let cursor_top = Point::new(cursor_x, baseline_y + self.session.ascender);
        let cursor_bottom = Point::new(cursor_x, baseline_y + self.session.descender);

        // Transform to screen coordinates
        let cursor_top_screen = *transform * cursor_top;
        let cursor_bottom_screen = *transform * cursor_bottom;

        let cursor_line = kurbo::Line::new(cursor_top_screen, cursor_bottom_screen);

        // Use orange color (same as selection marquee) with 1.5px stroke
        let stroke = Stroke::new(1.5);
        let brush = Brush::Solid(theme::selection::RECT_STROKE);

        scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &cursor_line);

        // Draw triangular indicators at top and bottom (like Glyphs app)
        // Triangle size in screen space - slightly smaller than 4x
        let triangle_width = 24.0;
        let triangle_height = 16.0;

        // Top triangle (pointing down/inward, aligned with ascender)
        // Base at ascender, tip extends downward into the metrics box
        let mut top_triangle = kurbo::BezPath::new();
        top_triangle.move_to((
            cursor_top_screen.x - triangle_width / 2.0,
            cursor_top_screen.y,
        )); // Left corner at ascender
        top_triangle.line_to((
            cursor_top_screen.x + triangle_width / 2.0,
            cursor_top_screen.y,
        )); // Right corner at ascender
        top_triangle.line_to((cursor_top_screen.x, cursor_top_screen.y + triangle_height)); // Tip below, pointing down
        top_triangle.close_path();

        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            &brush,
            None,
            &top_triangle,
        );

        // Bottom triangle (pointing up/inward, aligned with descender)
        // Base at descender, tip extends upward into the metrics box
        let mut bottom_triangle = kurbo::BezPath::new();
        bottom_triangle.move_to((
            cursor_bottom_screen.x - triangle_width / 2.0,
            cursor_bottom_screen.y,
        )); // Left corner at descender
        bottom_triangle.line_to((
            cursor_bottom_screen.x + triangle_width / 2.0,
            cursor_bottom_screen.y,
        )); // Right corner at descender
        bottom_triangle.line_to((
            cursor_bottom_screen.x,
            cursor_bottom_screen.y - triangle_height,
        )); // Tip above, pointing up
        bottom_triangle.close_path();

        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            &brush,
            None,
            &bottom_triangle,
        );
    }

    /// Render metrics box for a single sort (Phase 6)
    ///
    /// This draws the bounding rectangle that defines the sort.
    /// Shows the advance width, baseline, ascender, descender, and font metrics.
    fn render_sort_metrics(
        &self,
        scene: &mut Scene,
        x_offset: f64,
        baseline_y: f64,
        advance_width: f64,
        transform: &Affine,
    ) {
        let stroke = Stroke::new(theme::size::METRIC_LINE_WIDTH);
        let brush = Brush::Solid(theme::metrics::GUIDE);

        // Draw vertical lines (left and right edges of the sort)
        // Offset by baseline_y to support multi-line text
        let left_top = Point::new(x_offset, baseline_y + self.session.ascender);
        let left_bottom = Point::new(x_offset, baseline_y + self.session.descender);
        let left_line = kurbo::Line::new(*transform * left_top, *transform * left_bottom);
        scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &left_line);

        let right_top = Point::new(x_offset + advance_width, baseline_y + self.session.ascender);
        let right_bottom = Point::new(
            x_offset + advance_width,
            baseline_y + self.session.descender,
        );
        let right_line = kurbo::Line::new(*transform * right_top, *transform * right_bottom);
        scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &right_line);

        // Draw horizontal lines (baseline, ascender, descender, etc.)
        // Offset by baseline_y to support multi-line text
        let draw_hline = |scene: &mut Scene, y: f64| {
            let start = Point::new(x_offset, baseline_y + y);
            let end = Point::new(x_offset + advance_width, baseline_y + y);
            let line = kurbo::Line::new(*transform * start, *transform * end);
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &line);
        };

        // Descender (bottom of metrics box)
        draw_hline(scene, self.session.descender);

        // Baseline (y=0)
        draw_hline(scene, 0.0);

        // X-height (if available)
        if let Some(x_height) = self.session.x_height {
            draw_hline(scene, x_height);
        }

        // Cap-height (if available)
        if let Some(cap_height) = self.session.cap_height {
            draw_hline(scene, cap_height);
        }

        // Ascender (top of metrics box)
        draw_hline(scene, self.session.ascender);
    }

    /// Render minimal metrics markers for text mode (Glyphs.app style)
    ///
    /// Shows cross markers (+) at each edge point where metrics lines would be
    fn render_sort_minimal_metrics(
        &self,
        scene: &mut Scene,
        x_offset: f64,
        baseline_y: f64,
        advance_width: f64,
        transform: &Affine,
        color: masonry::vello::peniko::Color,
    ) {
        let stroke = Stroke::new(theme::size::METRIC_LINE_WIDTH);
        let brush = Brush::Solid(color);
        let cross_size = 24.0; // Length of each arm of the cross from center

        // Helper to draw a cross (+) at a given point
        let draw_cross = |scene: &mut Scene, x: f64, y: f64| {
            // Horizontal line
            let h_line = kurbo::Line::new(
                *transform * Point::new(x - cross_size, y),
                *transform * Point::new(x + cross_size, y),
            );
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &h_line);

            // Vertical line
            let v_line = kurbo::Line::new(
                *transform * Point::new(x, y - cross_size),
                *transform * Point::new(x, y + cross_size),
            );
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &v_line);
        };

        // Left edge crosses
        draw_cross(scene, x_offset, baseline_y + self.session.descender); // Bottom
        draw_cross(scene, x_offset, baseline_y); // Baseline
        draw_cross(scene, x_offset, baseline_y + self.session.ascender); // Top

        // Right edge crosses
        draw_cross(
            scene,
            x_offset + advance_width,
            baseline_y + self.session.descender,
        ); // Bottom
        draw_cross(scene, x_offset + advance_width, baseline_y); // Baseline
        draw_cross(
            scene,
            x_offset + advance_width,
            baseline_y + self.session.ascender,
        ); // Top
    }
}

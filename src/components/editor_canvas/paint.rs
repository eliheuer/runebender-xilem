// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Paint helper methods for EditorWidget

use super::EditorWidget;
use super::drawing::{draw_design_grid, draw_metrics_guides, draw_paths_with_points};
use crate::theme;
use kurbo::{Affine, Stroke};
use masonry::kurbo::Size;
use masonry::util::fill_color;
use masonry::vello::Scene;
use masonry::vello::peniko::Brush;

impl EditorWidget {
    // ============================================================================
    // PAINT HELPER METHODS
    // ============================================================================

    pub(super) fn paint_background(&self, scene: &mut Scene, canvas_size: Size) {
        let bg_rect = canvas_size.to_rect();
        fill_color(scene, &bg_rect, crate::theme::canvas::BACKGROUND);
    }

    pub(super) fn is_preview_mode(&self) -> bool {
        self.session.current_tool.id() == crate::tools::ToolId::Preview
    }

    pub(super) fn paint_text_buffer_mode(
        &mut self,
        scene: &mut Scene,
        transform: &Affine,
        is_preview_mode: bool,
    ) {
        if !is_preview_mode {
            draw_design_grid(scene, &self.session, self.size);
        }

        self.render_text_buffer(scene, transform, is_preview_mode);

        if !is_preview_mode {
            self.paint_tool_overlay(scene, transform);
        }
    }

    pub(super) fn paint_single_glyph_mode(
        &mut self,
        scene: &mut Scene,
        transform: &Affine,
        is_preview_mode: bool,
    ) {
        if !is_preview_mode {
            draw_design_grid(scene, &self.session, self.size);
            draw_metrics_guides(scene, transform, &self.session, self.size);
        }

        let glyph_path = self.build_glyph_path();
        if glyph_path.is_empty() {
            return;
        }

        let transformed_path = *transform * &glyph_path;

        if is_preview_mode {
            self.paint_glyph_preview(scene, &transformed_path);
        } else {
            self.paint_glyph_edit_mode(scene, &transformed_path, transform);
        }
    }

    fn build_glyph_path(&self) -> kurbo::BezPath {
        let mut glyph_path = kurbo::BezPath::new();
        for path in self.session.paths.iter() {
            glyph_path.extend(path.to_bezpath());
        }
        glyph_path
    }

    fn paint_glyph_preview(&self, scene: &mut Scene, path: &kurbo::BezPath) {
        let fill_brush = Brush::Solid(theme::path::PREVIEW_FILL);
        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            &fill_brush,
            None,
            path,
        );
    }

    fn paint_glyph_edit_mode(
        &mut self,
        scene: &mut Scene,
        path: &kurbo::BezPath,
        transform: &Affine,
    ) {
        let stroke = Stroke::new(theme::size::PATH_STROKE_WIDTH);
        let brush = Brush::Solid(theme::path::STROKE);
        scene.stroke(&stroke, Affine::IDENTITY, &brush, None, path);

        draw_paths_with_points(scene, &self.session, transform);

        self.paint_tool_overlay(scene, transform);
    }

    fn paint_tool_overlay(&mut self, scene: &mut Scene, transform: &Affine) {
        let select_tool = crate::tools::ToolBox::for_id(crate::tools::ToolId::Select);
        let mut tool = std::mem::replace(&mut self.session.current_tool, select_tool);
        tool.paint(scene, &self.session, transform);
        self.session.current_tool = tool;
    }

    /// Initialize viewport positioning to center the glyph
    pub(super) fn initialize_viewport(&mut self, canvas_size: Size) {
        let ascender = self.session.ascender;
        let descender = self.session.descender;

        // Calculate the visible height in design space
        let design_height = ascender - descender;

        // Center the viewport on the canvas
        let center_x = canvas_size.width / 2.0;
        let center_y = canvas_size.height / 2.0;

        // Create a transform that:
        // 1. Scales to fit the canvas (with some padding)
        // 2. Centers the glyph
        let padding = 0.6; // Leave 40% padding (more zoomed out)
        let scale = (canvas_size.height * padding) / design_height;

        // Center point in design space (middle of advance width,
        // middle of height)
        let design_center_x = self.session.glyph.width / 2.0;
        let design_center_y = (ascender + descender) / 2.0;

        // Update the viewport to match our rendering transform
        // The viewport uses: zoom (scale) and offset (translation
        // after scale)
        self.session.viewport.zoom = scale;
        // Offset calculation based on to_screen formula:
        // screen.x = design.x * zoom + offset.x
        // screen.y = -design.y * zoom + offset.y
        // For design_center to map to canvas_center:
        self.session.viewport.offset = kurbo::Vec2::new(
            center_x - design_center_x * scale,
            center_y + design_center_y * scale, // Y is flipped
        );

        self.session.viewport_initialized = true;
    }
}

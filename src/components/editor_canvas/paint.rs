// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Paint helper methods for EditorWidget

use super::EditorWidget;
use super::drawing::{draw_design_grid, draw_metrics_guides, draw_paths_with_points};
use crate::theme;
use kurbo::{Affine, RoundedRect, Stroke};
use masonry::core::{BrushIndex, StyleProperty, render_text};
use masonry::kurbo::Size;
use masonry::util::fill_color;
use masonry::vello::Scene;
use masonry::vello::peniko::{Brush, Fill, ImageBrush};
use parley::{FontContext, FontStack, LayoutContext};

thread_local! {
    static MENU_FONT_CX: std::cell::RefCell<FontContext> =
        std::cell::RefCell::new(FontContext::default());
    static MENU_LAYOUT_CX: std::cell::RefCell<
        LayoutContext<BrushIndex>,
    > = std::cell::RefCell::new(LayoutContext::new());
}

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

        self.paint_background_image(scene, transform);
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

        self.paint_background_image(scene, transform);

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

    /// Paint the background reference image (if any) behind the glyph.
    ///
    /// The image is rendered in design space with opacity, then
    /// optionally a selection border and resize handles are drawn.
    fn paint_background_image(
        &self,
        scene: &mut Scene,
        transform: &Affine,
    ) {
        let bg = match &self.session.background_image {
            Some(bg) => bg,
            None => return,
        };

        // Build transform: viewport × translate(position) × scale ×
        // y-flip (images are Y-down, design space is Y-up)
        let image_transform = *transform
            * Affine::translate(bg.position.to_vec2())
            * Affine::scale_non_uniform(bg.scale_x, -bg.scale_y)
            * Affine::translate((0.0, -(bg.height as f64)));

        let brush = ImageBrush::new(bg.image_data.clone())
            .with_alpha(bg.opacity as f32);
        scene.draw_image(&brush, image_transform);

        // Draw selection UI when selected and not locked
        if bg.selected && !bg.locked {
            self.paint_image_selection(scene, transform, bg);
        }
    }

    /// Draw the selection border and all 8 resize handles for the
    /// background image.
    fn paint_image_selection(
        &self,
        scene: &mut Scene,
        transform: &Affine,
        bg: &crate::editing::background_image::BackgroundImage,
    ) {
        let bounds = bg.bounds();

        // --- Dashed selection border ---
        let p0 = *transform * kurbo::Point::new(bounds.x0, bounds.y0);
        let p1 = *transform * kurbo::Point::new(bounds.x1, bounds.y0);
        let p2 = *transform * kurbo::Point::new(bounds.x1, bounds.y1);
        let p3 = *transform * kurbo::Point::new(bounds.x0, bounds.y1);
        let mut border_path = kurbo::BezPath::new();
        border_path.move_to(p0);
        border_path.line_to(p1);
        border_path.line_to(p2);
        border_path.line_to(p3);
        border_path.close_path();

        let stroke = Stroke::new(
            theme::background_image::SELECTION_BORDER_WIDTH,
        );
        let dash_pattern = [6.0, 4.0];
        let dashed = stroke.with_dashes(0.0, dash_pattern);
        let border_brush =
            Brush::Solid(theme::background_image::SELECTION_BORDER);
        scene.stroke(
            &dashed,
            Affine::IDENTITY,
            &border_brush,
            None,
            &border_path,
        );

        let handle_r = theme::background_image::HANDLE_RADIUS;
        let handle_stroke = Stroke::new(
            theme::background_image::HANDLE_STROKE_WIDTH,
        );
        let fill_brush =
            Brush::Solid(theme::background_image::HANDLE_FILL);
        let stroke_brush =
            Brush::Solid(theme::background_image::HANDLE_STROKE);

        // --- Corner handles (circles) — proportional scaling ---
        for corner in &bg.corner_positions() {
            let sp = *transform * *corner;
            let circle = kurbo::Circle::new(sp, handle_r);
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                &fill_brush,
                None,
                &circle,
            );
            scene.stroke(
                &handle_stroke,
                Affine::IDENTITY,
                &stroke_brush,
                None,
                &circle,
            );
        }

        // --- Side handles (squares) — free single-axis scaling ---
        let half = handle_r;
        for side in &bg.side_positions() {
            let sp = *transform * *side;
            let rect = kurbo::Rect::new(
                sp.x - half,
                sp.y - half,
                sp.x + half,
                sp.y + half,
            );
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                &fill_brush,
                None,
                &rect,
            );
            scene.stroke(
                &handle_stroke,
                Affine::IDENTITY,
                &stroke_brush,
                None,
                &rect,
            );
        }
    }

    // ====================================================================
    // CONTEXT MENU
    // ====================================================================

    /// Paint the right-click context menu overlay.
    pub(super) fn paint_context_menu(&self, scene: &mut Scene) {
        let menu = match &self.context_menu {
            Some(m) => m,
            None => return,
        };

        let item_h = theme::context_menu::ITEM_HEIGHT;
        let pad = theme::context_menu::PADDING;
        let menu_w = theme::context_menu::MENU_WIDTH;
        let radius = theme::context_menu::BORDER_RADIUS;
        let total_h =
            pad * 2.0 + menu.items.len() as f64 * item_h;

        // Menu background with rounded corners
        let menu_rect = kurbo::Rect::new(
            menu.position.x,
            menu.position.y,
            menu.position.x + menu_w,
            menu.position.y + total_h,
        );
        let rounded = RoundedRect::from_rect(menu_rect, radius);
        let bg_brush =
            Brush::Solid(theme::context_menu::BACKGROUND);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &bg_brush,
            None,
            &rounded,
        );

        // Border
        let border_brush =
            Brush::Solid(theme::context_menu::BORDER);
        let border_stroke = Stroke::new(1.0);
        scene.stroke(
            &border_stroke,
            Affine::IDENTITY,
            &border_brush,
            None,
            &rounded,
        );

        // Draw each item
        MENU_FONT_CX.with(|font_cell| {
            MENU_LAYOUT_CX.with(|layout_cell| {
                let mut font_cx = font_cell.borrow_mut();
                let mut layout_cx = layout_cell.borrow_mut();

                for (i, item) in menu.items.iter().enumerate() {
                    let item_y =
                        menu.position.y + pad + i as f64 * item_h;

                    // Hover highlight
                    if menu.hover_index == Some(i) {
                        let hover_rect = kurbo::Rect::new(
                            menu.position.x + 2.0,
                            item_y,
                            menu.position.x + menu_w - 2.0,
                            item_y + item_h,
                        );
                        let hover_rounded =
                            RoundedRect::from_rect(hover_rect, 3.0);
                        let hover_brush =
                            Brush::Solid(theme::context_menu::HOVER);
                        scene.fill(
                            Fill::NonZero,
                            Affine::IDENTITY,
                            &hover_brush,
                            None,
                            &hover_rounded,
                        );
                    }

                    // Text label
                    let mut builder = layout_cx.ranged_builder(
                        &mut font_cx,
                        &item.label,
                        1.0,
                        false,
                    );
                    builder.push_default(StyleProperty::FontSize(
                        theme::context_menu::FONT_SIZE,
                    ));
                    builder.push_default(
                        StyleProperty::FontStack(FontStack::Single(
                            parley::FontFamily::Generic(
                                parley::GenericFamily::SansSerif,
                            ),
                        )),
                    );
                    builder.push_default(
                        StyleProperty::Brush(BrushIndex(0)),
                    );
                    let mut text_layout =
                        builder.build(&item.label);
                    text_layout.break_all_lines(None);

                    let text_x = menu.position.x
                        + theme::context_menu::TEXT_INSET;
                    // Vertically center text in item
                    let text_y = item_y
                        + (item_h - theme::context_menu::FONT_SIZE
                            as f64)
                            / 2.0;

                    let brushes = vec![Brush::Solid(
                        theme::context_menu::TEXT,
                    )];
                    render_text(
                        scene,
                        Affine::translate((text_x, text_y)),
                        &text_layout,
                        &brushes,
                        false,
                    );
                }
            });
        });
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

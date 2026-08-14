// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph anatomy preview panel
//!
//! Displays the selected glyph's outline stroke with control points
//! and handle lines — a static "x-ray" view into the glyph's
//! structure. Panel height adapts to available space via flex
//! layout; the glyph is centered with uniform padding.

use kurbo::{Axis, Affine, BezPath, Circle, Line, Rect, RoundedRect, Shape, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, MeasureCtx, ChildrenIds, EventCtx, LayoutCtx, PaintCtx, PointerEvent,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
};
use masonry::imaging::Painter;
use masonry::layout::{LenReq, Length};
use masonry::peniko::Brush;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use crate::data::AppState;
use crate::model::read_workspace;
use crate::model::workspace::{Contour, PointType};
use crate::model::{glyph_renderer, workspace};
use crate::theme;

// ============================================================
// Public API
// ============================================================

/// Create a glyph anatomy panel view from app state.
///
/// The panel fills all available vertical space (via flex
/// layout) and centers the glyph with uniform padding.
pub fn glyph_anatomy_panel(state: &AppState) -> impl WidgetView<AppState> + use<> {
    let contours = if let Some(ref glyph_name) = state.selected_glyph {
        extract_contours(state, glyph_name)
    } else {
        Vec::new()
    };

    glyph_anatomy_view(contours)
}

/// Extract glyph contours for the selected glyph
fn extract_contours(state: &AppState, glyph_name: &str) -> Vec<Contour> {
    let workspace_arc = match state.active_workspace() {
        Some(w) => w,
        None => return Vec::new(),
    };
    let workspace = read_workspace(&workspace_arc);
    match workspace.get_glyph(glyph_name) {
        Some(g) => g.contours.clone(),
        None => Vec::new(),
    }
}

/// Build a bezpath from contours (used by the widget for
/// both layout and paint).
fn build_outline_from_contours(contours: &[Contour]) -> BezPath {
    let glyph = workspace::Glyph {
        name: String::new(),
        width: 0.0,
        height: None,
        codepoints: Vec::new(),
        contours: contours.to_vec(),
        components: Vec::new(),
        left_group: None,
        right_group: None,
        mark_color: None,
    };
    glyph_renderer::glyph_to_bezpath(&glyph)
}

// ============================================================
// Constants
// ============================================================

/// Padding inside the panel (uniform on all sides)
const PANEL_PAD: f64 = 16.0;

/// Smaller point radius for the preview panel
const PREVIEW_POINT_RADIUS: f64 = 2.5;

/// Smaller square half-size for corner points
const PREVIEW_CORNER_HALF: f64 = 2.0;

/// Smaller off-curve point radius
const PREVIEW_OFFCURVE_RADIUS: f64 = 1.8;

/// Thinner handle lines
const PREVIEW_HANDLE_WIDTH: f64 = 0.75;

// ============================================================
// Custom Masonry Widget
// ============================================================

struct GlyphAnatomyWidget {
    contours: Vec<Contour>,
}

impl GlyphAnatomyWidget {
    fn new(contours: Vec<Contour>) -> Self {
        Self { contours }
    }

    /// Compute a transform that centers the glyph's bounding box
    /// in the panel with uniform padding.
    fn compute_transform(&self, outline: &BezPath, size: Size) -> Affine {
        let bounds = outline.bounding_box();
        let glyph_w = bounds.width();
        let glyph_h = bounds.height();

        if glyph_w <= 0.0 || glyph_h <= 0.0 {
            return Affine::IDENTITY;
        }

        let draw_w = size.width - PANEL_PAD * 2.0;
        let draw_h = size.height - PANEL_PAD * 2.0;

        if draw_w <= 0.0 || draw_h <= 0.0 {
            return Affine::IDENTITY;
        }

        // Scale to fit, preserving aspect ratio
        let scale_x = draw_w / glyph_w;
        let scale_y = draw_h / glyph_h;
        let scale = scale_x.min(scale_y);

        // Center horizontally and vertically
        let scaled_w = glyph_w * scale;
        let scaled_h = glyph_h * scale;
        let x_offset = PANEL_PAD + (draw_w - scaled_w) / 2.0;
        let y_offset = PANEL_PAD + (draw_h - scaled_h) / 2.0;

        // UFO Y-up → screen Y-down: flip Y
        Affine::new([
            scale,
            0.0,
            0.0,
            -scale,
            x_offset - bounds.x0 * scale,
            y_offset + bounds.y1 * scale,
        ])
    }

    /// Draw the glyph outline (stroked, not filled)
    fn paint_outline(&self, painter: &mut Painter<'_>, transform: Affine, outline: &BezPath) {
        let color = theme::grid::CELL_SELECTED_OUTLINE;
        let transformed = transform * outline;
        let stroke = kurbo::Stroke::new(theme::size::PATH_STROKE_WIDTH);
        painter.stroke(&transformed, &stroke, &Brush::Solid(color)).draw();
    }

    /// Draw handle lines between on-curve and adjacent off-curve
    /// points.
    fn paint_handles(&self, painter: &mut Painter<'_>, transform: Affine) {
        let color = Brush::Solid(theme::grid::CELL_SELECTED_OUTLINE);
        let stroke = kurbo::Stroke::new(PREVIEW_HANDLE_WIDTH);

        for contour in &self.contours {
            let pts = &contour.points;
            let n = pts.len();
            if n < 2 {
                continue;
            }
            for i in 0..n {
                let curr = &pts[i];
                let next = &pts[(i + 1) % n];

                let curr_off = matches!(curr.point_type, PointType::OffCurve);
                let next_off = matches!(next.point_type, PointType::OffCurve);

                // Draw line if exactly one endpoint is off-curve
                if curr_off != next_off {
                    let p0 = transform * kurbo::Point::new(curr.x, curr.y);
                    let p1 = transform * kurbo::Point::new(next.x, next.y);
                    painter.stroke(&Line::new(p0, p1), &stroke, &color).draw();
                }
            }
        }
    }

    /// Draw control points — all in selection color
    fn paint_points(&self, painter: &mut Painter<'_>, transform: Affine) {
        let color = theme::grid::CELL_SELECTED_OUTLINE;
        let bg = theme::panel::BACKGROUND;

        for contour in &self.contours {
            for pt in &contour.points {
                let screen = transform * kurbo::Point::new(pt.x, pt.y);
                match pt.point_type {
                    PointType::Curve | PointType::QCurve => {
                        draw_circle_point(painter, screen, PREVIEW_POINT_RADIUS, bg, color);
                    }
                    PointType::Line | PointType::Move => {
                        draw_square_point(painter, screen, PREVIEW_CORNER_HALF, bg, color);
                    }
                    PointType::OffCurve => {
                        draw_circle_point(painter, screen, PREVIEW_OFFCURVE_RADIUS, bg, color);
                    }
                    PointType::Hyper | PointType::HyperCorner => {
                        draw_circle_point(painter, screen, PREVIEW_POINT_RADIUS, bg, color);
                    }
                }
            }
        }
    }
}

/// Draw a circular point (outer ring + inner fill)
fn draw_circle_point(
    painter: &mut Painter<'_>,
    center: kurbo::Point,
    radius: f64,
    inner_color: masonry::peniko::Color,
    outer_color: masonry::peniko::Color,
) {
    let outer = Circle::new(center, radius);
    painter.fill(&outer, &Brush::Solid(outer_color)).draw();
    let inner = Circle::new(center, radius * 0.6);
    painter.fill(&inner, &Brush::Solid(inner_color)).draw();
}

/// Draw a square point (outer border + inner fill)
fn draw_square_point(
    painter: &mut Painter<'_>,
    center: kurbo::Point,
    half_size: f64,
    inner_color: masonry::peniko::Color,
    outer_color: masonry::peniko::Color,
) {
    let outer = Rect::new(
        center.x - half_size,
        center.y - half_size,
        center.x + half_size,
        center.y + half_size,
    );
    painter.fill(&outer, &Brush::Solid(outer_color)).draw();
    let inset = half_size * 0.4;
    let inner = Rect::new(
        center.x - half_size + inset,
        center.y - half_size + inset,
        center.x + half_size - inset,
        center.y + half_size - inset,
    );
    painter.fill(&inner, &Brush::Solid(inner_color)).draw();
}

// ============================================================
// Widget trait implementation
// ============================================================

impl Widget for GlyphAnatomyWidget {
    type Action = ();

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        crate::components::measure_fill(len_req, 0.0)
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &PropertiesRef<'_>,
        _size: Size,
    ) {
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let size = ctx.content_box().size();

        // Panel background and border
        let panel_rect = RoundedRect::from_rect(
            Rect::from_origin_size(kurbo::Point::ZERO, size),
            theme::size::PANEL_RADIUS,
        );
        painter.fill(&panel_rect, &Brush::Solid(theme::panel::BACKGROUND)).draw();
        painter.stroke(&panel_rect, &kurbo::Stroke::new(theme::size::TOOLBAR_BORDER_WIDTH), &Brush::Solid(theme::panel::OUTLINE)).draw();

        // Nothing to draw if no contours
        if self.contours.is_empty() {
            return;
        }

        let outline = build_outline_from_contours(&self.contours);
        if outline.is_empty() {
            return;
        }

        let transform = self.compute_transform(&outline, size);

        // Draw layers back-to-front
        self.paint_outline(painter, transform, &outline);
        self.paint_handles(painter, transform);
        self.paint_points(painter, transform);
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn on_pointer_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &PointerEvent,
    ) {
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
    }
}

// ============================================================
// Xilem View wrapper
// ============================================================

fn glyph_anatomy_view(contours: Vec<Contour>) -> GlyphAnatomyView {
    GlyphAnatomyView { contours }
}

#[must_use = "View values do nothing unless provided to Xilem."]
struct GlyphAnatomyView {
    contours: Vec<Contour>,
}

impl ViewMarker for GlyphAnatomyView {}

impl View<AppState, (), ViewCtx> for GlyphAnatomyView {
    type Element = Pod<GlyphAnatomyWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut AppState,
    ) -> (Self::Element, Self::ViewState) {
        let widget = GlyphAnatomyWidget::new(self.contours.clone());
        let pod = ctx.create_pod(widget);
        (pod, ())
    }

    fn rebuild(
        &self,
        _prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut AppState,
    ) {
        element.widget.contours = self.contours.clone();
        element.ctx.request_render();
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        _message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut AppState,
    ) -> MessageResult<()> {
        MessageResult::Stale
    }
}

// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph anatomy preview panel
//!
//! Displays the selected glyph's outline stroke with control points
//! and handle lines — a static "x-ray" view into the glyph's
//! structure.

use kurbo::{
    Affine, BezPath, Circle, Line, Rect, RoundedRect, Shape,
    Size,
};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx,
    PaintCtx, PointerEvent, PropertiesMut, PropertiesRef,
    RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
};
use masonry::vello::Scene;
use masonry::vello::peniko::{Brush, Fill};
use xilem::core::{
    MessageContext, MessageResult, Mut, View, ViewMarker,
};
use xilem::{Pod, ViewCtx, WidgetView};

use crate::data::AppState;
use crate::theme;
use crate::workspace::{Contour, PointType};
use crate::{glyph_renderer, workspace};

// ============================================================
// Public API
// ============================================================

/// Create a glyph anatomy panel view from app state.
///
/// Reads the selected glyph's contours from the workspace and
/// passes them to the widget. Shows blank when nothing is
/// selected.
pub fn glyph_anatomy_panel(
    state: &AppState,
) -> impl WidgetView<AppState> + use<> {
    let contours = if let Some(ref glyph_name) =
        state.selected_glyph
    {
        extract_contours(state, glyph_name)
    } else {
        Vec::new()
    };

    glyph_anatomy_view(contours)
}

/// Extract glyph contours for the selected glyph
fn extract_contours(
    state: &AppState,
    glyph_name: &str,
) -> Vec<Contour> {
    let workspace_arc = match state.active_workspace() {
        Some(w) => w,
        None => return Vec::new(),
    };
    let workspace = workspace_arc.read().unwrap();
    match workspace.get_glyph(glyph_name) {
        Some(g) => g.contours.clone(),
        None => Vec::new(),
    }
}

// ============================================================
// Constants
// ============================================================

/// Padding inside the panel
const PANEL_PAD: f64 = 16.0;

/// Fixed height of the anatomy panel
const PANEL_HEIGHT: f64 = 320.0;

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

    /// Build the glyph outline bezpath from contours
    fn build_outline(&self) -> BezPath {
        let glyph = workspace::Glyph {
            name: String::new(),
            width: 0.0,
            height: None,
            codepoints: Vec::new(),
            contours: self.contours.clone(),
            components: Vec::new(),
            left_group: None,
            right_group: None,
            mark_color: None,
        };
        glyph_renderer::glyph_to_bezpath(&glyph)
    }

    /// Compute a transform that centers the glyph's bounding box
    /// in the panel, scaled to fill the available width.
    fn compute_transform(
        &self,
        outline: &BezPath,
        size: Size,
    ) -> Affine {
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

        // Scale to fill width, but cap at height
        let scale_x = draw_w / glyph_w;
        let scale_y = draw_h / glyph_h;
        let scale = scale_x.min(scale_y);

        // Center horizontally and vertically
        let scaled_w = glyph_w * scale;
        let scaled_h = glyph_h * scale;
        let x_offset =
            PANEL_PAD + (draw_w - scaled_w) / 2.0;
        let y_offset =
            PANEL_PAD + (draw_h - scaled_h) / 2.0;

        // UFO Y-up → screen Y-down: flip Y
        // Map bounds.max_y (top of glyph in UFO) to y_offset
        // (top of draw area on screen)
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
    fn paint_outline(
        &self,
        scene: &mut Scene,
        transform: Affine,
        outline: &BezPath,
    ) {
        let transformed = transform * outline;
        let stroke =
            kurbo::Stroke::new(theme::size::PATH_STROKE_WIDTH);
        scene.stroke(
            &stroke,
            Affine::IDENTITY,
            &Brush::Solid(theme::path::STROKE),
            None,
            &transformed,
        );
    }

    /// Draw handle lines between on-curve and adjacent off-curve
    /// points.
    fn paint_handles(
        &self,
        scene: &mut Scene,
        transform: Affine,
    ) {
        let handle_color = Brush::Solid(theme::handle::LINE);
        let stroke =
            kurbo::Stroke::new(theme::size::HANDLE_LINE_WIDTH);

        for contour in &self.contours {
            let pts = &contour.points;
            let n = pts.len();
            if n < 2 {
                continue;
            }
            for i in 0..n {
                let curr = &pts[i];
                let next = &pts[(i + 1) % n];

                let curr_off = matches!(
                    curr.point_type,
                    PointType::OffCurve
                );
                let next_off = matches!(
                    next.point_type,
                    PointType::OffCurve
                );

                // Draw line if exactly one endpoint is off-curve
                if curr_off != next_off {
                    let p0 = transform
                        * kurbo::Point::new(curr.x, curr.y);
                    let p1 = transform
                        * kurbo::Point::new(next.x, next.y);
                    scene.stroke(
                        &stroke,
                        Affine::IDENTITY,
                        &handle_color,
                        None,
                        &Line::new(p0, p1),
                    );
                }
            }
        }
    }

    /// Draw control points colored by type
    fn paint_points(
        &self,
        scene: &mut Scene,
        transform: Affine,
    ) {
        for contour in &self.contours {
            for pt in &contour.points {
                let screen = transform
                    * kurbo::Point::new(pt.x, pt.y);
                match pt.point_type {
                    PointType::Curve | PointType::QCurve => {
                        draw_circle_point(
                            scene,
                            screen,
                            theme::size::SMOOTH_POINT_RADIUS,
                            theme::point::SMOOTH_INNER,
                            theme::point::SMOOTH_OUTER,
                        );
                    }
                    PointType::Line | PointType::Move => {
                        draw_square_point(
                            scene,
                            screen,
                            theme::size::CORNER_POINT_HALF_SIZE,
                            theme::point::CORNER_INNER,
                            theme::point::CORNER_OUTER,
                        );
                    }
                    PointType::OffCurve => {
                        draw_circle_point(
                            scene,
                            screen,
                            theme::size::OFFCURVE_POINT_RADIUS,
                            theme::point::OFFCURVE_INNER,
                            theme::point::OFFCURVE_OUTER,
                        );
                    }
                    PointType::Hyper
                    | PointType::HyperCorner => {
                        draw_circle_point(
                            scene,
                            screen,
                            theme::size::HYPER_POINT_RADIUS,
                            theme::point::HYPER_INNER,
                            theme::point::HYPER_OUTER,
                        );
                    }
                }
            }
        }
    }
}

/// Draw a circular point (outer ring + inner fill)
fn draw_circle_point(
    scene: &mut Scene,
    center: kurbo::Point,
    radius: f64,
    inner_color: masonry::vello::peniko::Color,
    outer_color: masonry::vello::peniko::Color,
) {
    let outer = Circle::new(center, radius);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Brush::Solid(outer_color),
        None,
        &outer,
    );
    let inner = Circle::new(center, radius * 0.6);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Brush::Solid(inner_color),
        None,
        &inner,
    );
}

/// Draw a square point (outer border + inner fill)
fn draw_square_point(
    scene: &mut Scene,
    center: kurbo::Point,
    half_size: f64,
    inner_color: masonry::vello::peniko::Color,
    outer_color: masonry::vello::peniko::Color,
) {
    let outer = Rect::new(
        center.x - half_size,
        center.y - half_size,
        center.x + half_size,
        center.y + half_size,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Brush::Solid(outer_color),
        None,
        &outer,
    );
    let inset = half_size * 0.4;
    let inner = Rect::new(
        center.x - half_size + inset,
        center.y - half_size + inset,
        center.x + half_size - inset,
        center.y + half_size - inset,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Brush::Solid(inner_color),
        None,
        &inner,
    );
}

// ============================================================
// Widget trait implementation
// ============================================================

impl Widget for GlyphAnatomyWidget {
    type Action = ();

    fn register_children(
        &mut self,
        _ctx: &mut RegisterCtx<'_>,
    ) {
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        bc.constrain(Size::new(
            bc.max().width,
            PANEL_HEIGHT,
        ))
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        scene: &mut Scene,
    ) {
        let size = ctx.size();

        // Panel background and border
        let panel_rect = RoundedRect::from_rect(
            Rect::from_origin_size(
                kurbo::Point::ZERO,
                size,
            ),
            theme::size::PANEL_RADIUS,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(theme::panel::BACKGROUND),
            None,
            &panel_rect,
        );
        scene.stroke(
            &kurbo::Stroke::new(
                theme::size::TOOLBAR_BORDER_WIDTH,
            ),
            Affine::IDENTITY,
            &Brush::Solid(theme::panel::OUTLINE),
            None,
            &panel_rect,
        );

        // Nothing to draw if no contours
        if self.contours.is_empty() {
            return;
        }

        let outline = self.build_outline();
        if outline.is_empty() {
            return;
        }

        let transform =
            self.compute_transform(&outline, size);

        // Draw layers back-to-front
        self.paint_outline(scene, transform, &outline);
        self.paint_handles(scene, transform);
        self.paint_points(scene, transform);
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

fn glyph_anatomy_view(
    contours: Vec<Contour>,
) -> GlyphAnatomyView {
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
        let widget =
            GlyphAnatomyWidget::new(self.contours.clone());
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
        // Always update — contour count alone can't detect
        // switching between glyphs with the same number of
        // contours.
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
        _message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut AppState,
    ) -> MessageResult<()> {
        MessageResult::Stale
    }
}

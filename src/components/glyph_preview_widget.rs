// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Reusable component for rendering filled font glyphs using Vello
//!
//! This module provides a unified glyph rendering component that is used
//! throughout the application wherever glyph previews are needed:
//!
//! - **Glyph Grid**: Displays each glyph in the grid cells
//! - **Editor Preview Pane**: Shows a larger preview of the glyph
//!
//! The component handles all the complexity of glyph rendering:
//!
//! - **GPU-accelerated rendering** via Vello
//! - **Uniform scaling** based on units-per-em (UPM)
//! - **Baseline positioning** for proper vertical alignment
//! - **Horizontal centering** with optional advance-width centering
//! - **Y-axis flipping** to convert from font coordinate space (Y-up)
//!   to screen coordinate space (Y-down)
//!
//! The component consists of two layers:
//!
//! - **`GlyphWidget`**: Low-level Masonry widget for rendering
//! - **`GlyphView`**: Xilem View wrapper that integrates with the
//!   reactive UI

use kurbo::{Axis, Affine, BezPath, Shape};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, MeasureCtx, ChildrenIds, LayoutCtx, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget,
};
use masonry::kurbo::Size;
use masonry::imaging::Painter;
use masonry::layout::{LenReq, Length};
use masonry::peniko::{Brush, Color};

/// A widget that renders a glyph from a BezPath
pub struct GlyphWidget {
    /// The bezier path representing the glyph outline
    path: BezPath,
    /// The color to fill the glyph with
    color: Color,
    /// Target display size for the widget
    size: Size,
    /// Units per em from the font (for uniform scaling)
    upm: f64,
    /// Baseline offset as a fraction of height (0.0 = bottom, 1.0 = top)
    baseline_offset: f64,
    /// Optional advance width for stable horizontal centering
    /// When provided, centers based on this width instead of bounding box
    advance_width: Option<f64>,
    /// When true, fit the glyph to the widget bounds with equal
    /// margins on all sides (ignores UPM and baseline positioning)
    fit_to_bounds: bool,
}

impl GlyphWidget {
    /// Create a new GlyphWidget from a BezPath
    pub fn new(path: BezPath, size: Size, upm: f64) -> Self {
        Self {
            path,
            color: crate::theme::grid::GLYPH_COLOR,
            size,
            upm,
            baseline_offset: 0.16, // Higher = more space at bottom
            advance_width: None,
            fit_to_bounds: false,
        }
    }

    /// Set the fill color for the glyph
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set the baseline offset (0.0 = bottom, 1.0 = top)
    pub fn with_baseline_offset(mut self, offset: f64) -> Self {
        self.baseline_offset = offset;
        self
    }

    /// Set the advance width for stable horizontal centering
    pub fn with_advance_width(mut self, width: f64) -> Self {
        self.advance_width = Some(width);
        self
    }

    /// Update the glyph path (for use in View::rebuild)
    pub fn set_path(&mut self, path: BezPath) {
        self.path = path;
    }

    /// Update the glyph color (for use in View::rebuild)
    pub fn set_color(&mut self, color: Color) {
        self.color = color;
    }

    /// Update the UPM value (for use in View::rebuild)
    pub fn set_upm(&mut self, upm: f64) {
        self.upm = upm;
    }

    /// Update the baseline offset (for use in View::rebuild)
    pub fn set_baseline_offset(&mut self, offset: f64) {
        self.baseline_offset = offset;
    }

    /// Update the widget size (for use in View::rebuild)
    pub fn set_size(&mut self, size: Size) {
        self.size = size;
    }

    /// Fit the glyph to the widget bounds with equal margins
    pub fn with_fit_to_bounds(mut self, fit: bool) -> Self {
        self.fit_to_bounds = fit;
        self
    }

    /// Update the advance width (for use in View::rebuild)
    pub fn set_advance_width(&mut self, width: Option<f64>) {
        self.advance_width = width;
    }
}

impl Widget for GlyphWidget {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {
        // Leaf widget - no children
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
        // No state to update
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        crate::components::measure_fixed(axis, self.size)
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &PropertiesRef<'_>,
        _size: Size,
    ) {
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        if self.path.is_empty() {
            return;
        }

        let bounds = self.path.bounding_box();
        let widget_size = ctx.content_box().size();

        let transformed_path = if self.fit_to_bounds {
            // Fit glyph to widget with equal margins on all sides
            let margin = 0.1; // 10% margin on each side
            let usable_w = widget_size.width * (1.0 - 2.0 * margin);
            let usable_h = widget_size.height * (1.0 - 2.0 * margin);
            let scale = (usable_w / bounds.width())
                .min(usable_h / bounds.height());
            let scaled_w = bounds.width() * scale;
            let scaled_h = bounds.height() * scale;
            let x_off = (widget_size.width - scaled_w) / 2.0
                - bounds.x0 * scale;
            let y_off = (widget_size.height - scaled_h) / 2.0
                + bounds.y1 * scale;
            let transform =
                Affine::new([scale, 0.0, 0.0, -scale, x_off, y_off]);
            transform * &self.path
        } else {
            // UPM-based scaling with baseline positioning
            let scale = widget_size.height / self.upm;
            let scale = scale * 0.8;

            let x_translation =
                if let Some(advance_width) = self.advance_width {
                    let scaled_advance = advance_width * scale;
                    (widget_size.width - scaled_advance) / 2.0
                } else {
                    let scaled_width = bounds.width() * scale;
                    let l_pad =
                        (widget_size.width - scaled_width) / 2.0;
                    l_pad - bounds.x0 * scale
                };

            let baseline = widget_size.height * self.baseline_offset;

            let transform = Affine::new([
                scale,
                0.0,
                0.0,
                -scale,
                x_translation,
                widget_size.height - baseline,
            ]);
            transform * &self.path
        };

        // Render the glyph using NonZero fill rule
        // This ensures overlapping shapes (like Arabic connectors) fill correctly
        // without gaps, unlike EvenOdd which alternates fill at each crossing
        painter.fill(&transformed_path, &Brush::Solid(self.color)).transform(kurbo::Affine::IDENTITY).draw();
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
        // Could add accessibility description for the glyph if needed
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

// ===== Xilem View Wrapper =====

use std::marker::PhantomData;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

/// Create a glyph view from a BezPath
pub fn glyph_view<State, Action>(
    path: BezPath,
    width: f64,
    height: f64,
    upm: f64,
) -> GlyphView<State, Action> {
    GlyphView {
        path,
        size: Size::new(width, height),
        color: None,
        upm,
        baseline_offset: None,
        advance_width: None,
        fit_to_bounds: false,
        phantom: PhantomData,
    }
}

/// The Xilem View for GlyphWidget
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct GlyphView<State, Action = ()> {
    path: BezPath,
    size: Size,
    color: Option<Color>,
    upm: f64,
    baseline_offset: Option<f64>,
    advance_width: Option<f64>,
    fit_to_bounds: bool,
    phantom: PhantomData<fn() -> (State, Action)>,
}

// Builder methods for configuring the glyph view
impl<State, Action> GlyphView<State, Action> {
    /// Set the glyph fill color
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Set the advance width for stable horizontal centering
    pub fn advance_width(mut self, width: f64) -> Self {
        self.advance_width = Some(width);
        self
    }

    /// Fit the glyph to widget bounds with equal margins
    pub fn fit_to_bounds(mut self) -> Self {
        self.fit_to_bounds = true;
        self
    }
}

// Marker trait implementation (required for Xilem Views)
impl<State, Action> ViewMarker for GlyphView<State, Action> {}

// Xilem View trait implementation (build, rebuild, teardown, message)
impl<State: 'static, Action: 'static> View<State, Action, ViewCtx> for GlyphView<State, Action> {
    type Element = Pod<GlyphWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let mut widget = GlyphWidget::new(self.path.clone(), self.size, self.upm);
        if let Some(color) = self.color {
            widget = widget.with_color(color);
        }
        if let Some(offset) = self.baseline_offset {
            widget = widget.with_baseline_offset(offset);
        }
        if let Some(width) = self.advance_width {
            widget = widget.with_advance_width(width);
        }
        if self.fit_to_bounds {
            widget = widget.with_fit_to_bounds(true);
        }
        (ctx.create_pod(widget), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        // Get mutable access to the widget
        let mut widget = element.downcast::<GlyphWidget>();

        // Update the widget's path if it has changed
        // This is crucial for the glyph grid to show updated previews
        // after editing
        if self.path != prev.path {
            widget.widget.set_path(self.path.clone());
            widget.ctx.request_render();
        }

        // Update other properties if they changed
        if self.color != prev.color
            && let Some(color) = self.color
        {
            widget.widget.set_color(color);
            widget.ctx.request_render();
        }

        if self.upm != prev.upm {
            widget.widget.set_upm(self.upm);
            widget.ctx.request_render();
        }

        if self.baseline_offset != prev.baseline_offset
            && let Some(offset) = self.baseline_offset
        {
            widget.widget.set_baseline_offset(offset);
            widget.ctx.request_render();
        }

        if self.size != prev.size {
            widget.widget.set_size(self.size);
            widget.ctx.request_render();
        }

        if self.advance_width != prev.advance_width {
            widget.widget.set_advance_width(self.advance_width);
            widget.ctx.request_render();
        }
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
        // No cleanup needed
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        _message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) -> MessageResult<Action> {
        // GlyphWidget doesn't produce any messages
        MessageResult::Stale
    }
}

// ===== Multi-Glyph Widget for Text Buffer Preview =====
//
// This widget renders multiple glyph paths separately to avoid winding conflicts
// that occur when combining paths with `.extend()`. Each glyph is rendered with
// its own `painter.fill()` call using NonZero fill rule.

/// A widget that renders multiple glyph paths (for text buffer preview)
///
/// Unlike combining all paths into one BezPath, this renders each glyph
/// separately to avoid winding direction conflicts at overlap points.
pub struct MultiGlyphWidget {
    /// Individual glyph paths, each will be rendered separately
    paths: Vec<BezPath>,
    /// The color to fill all glyphs with
    color: Color,
    /// Target display size for the widget
    size: Size,
    /// Units per em from the font (for uniform scaling)
    upm: f64,
    /// Baseline offset as a fraction of height (0.0 = bottom, 1.0 = top)
    baseline_offset: f64,
    /// When true, fit to combined bounding box instead of using UPM
    fit_to_bounds: bool,
}

impl MultiGlyphWidget {
    /// Create a new MultiGlyphWidget from a vector of BezPaths
    pub fn new(paths: Vec<BezPath>, size: Size, upm: f64) -> Self {
        Self {
            paths,
            color: crate::theme::grid::GLYPH_COLOR,
            size,
            upm,
            baseline_offset: 0.16,
            fit_to_bounds: false,
        }
    }

    /// Set the fill color for all glyphs
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set the baseline offset (0.0 = bottom, 1.0 = top)
    pub fn with_baseline_offset(mut self, offset: f64) -> Self {
        self.baseline_offset = offset;
        self
    }

    /// Enable fit-to-bounds mode (ignore UPM, fit to bounding box)
    pub fn with_fit_to_bounds(mut self) -> Self {
        self.fit_to_bounds = true;
        self
    }

    /// Update the glyph paths (for use in View::rebuild)
    pub fn set_paths(&mut self, paths: Vec<BezPath>) {
        self.paths = paths;
    }

    /// Update the glyph color (for use in View::rebuild)
    pub fn set_color(&mut self, color: Color) {
        self.color = color;
    }

    /// Update the UPM value (for use in View::rebuild)
    pub fn set_upm(&mut self, upm: f64) {
        self.upm = upm;
    }

    /// Update the baseline offset (for use in View::rebuild)
    pub fn set_baseline_offset(&mut self, offset: f64) {
        self.baseline_offset = offset;
    }

    /// Update the widget size (for use in View::rebuild)
    pub fn set_size(&mut self, size: Size) {
        self.size = size;
    }

    /// Update the fit-to-bounds flag (for use in View::rebuild)
    pub fn set_fit_to_bounds(&mut self, fit: bool) {
        self.fit_to_bounds = fit;
    }
}

impl Widget for MultiGlyphWidget {
    type Action = NoAction;

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {
        // Leaf widget - no children
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
        // No state to update
    }

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        crate::components::measure_fixed(axis, self.size)
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &PropertiesRef<'_>,
        _size: Size,
    ) {
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        if self.paths.is_empty() {
            return;
        }

        // Calculate combined bounding box for centering
        let mut combined_bounds: Option<kurbo::Rect> = None;
        for path in &self.paths {
            if !path.is_empty() {
                let bounds = path.bounding_box();
                combined_bounds = Some(match combined_bounds {
                    Some(existing) => existing.union(bounds),
                    None => bounds,
                });
            }
        }

        let bounds = match combined_bounds {
            Some(b) => b,
            None => return,
        };

        let widget_size = ctx.content_box().size();

        let transform = if self.fit_to_bounds {
            // Fit combined bounding box to widget with equal margins
            let margin = 0.1;
            let usable_w = widget_size.width * (1.0 - 2.0 * margin);
            let usable_h = widget_size.height * (1.0 - 2.0 * margin);
            let scale = if bounds.width() > 0.0 && bounds.height() > 0.0 {
                (usable_w / bounds.width()).min(usable_h / bounds.height())
            } else {
                1.0
            };
            let scaled_w = bounds.width() * scale;
            let scaled_h = bounds.height() * scale;
            let x_off = (widget_size.width - scaled_w) / 2.0
                - bounds.x0 * scale;
            let y_off = (widget_size.height - scaled_h) / 2.0
                + bounds.y1 * scale;
            Affine::new([scale, 0.0, 0.0, -scale, x_off, y_off])
        } else {
            // UPM-based scaling with baseline offset
            let scale = widget_size.height / self.upm;
            let scale = scale * 0.8;
            let scaled_width = bounds.width() * scale;
            let l_pad = (widget_size.width - scaled_width) / 2.0;
            let x_translation = l_pad - bounds.x0 * scale;
            let baseline = widget_size.height * self.baseline_offset;
            Affine::new([
                scale,
                0.0,
                0.0,
                -scale,
                x_translation,
                widget_size.height - baseline,
            ])
        };

        // Clip to widget bounds so glyphs don't overflow
        let clip_rect = kurbo::Rect::from_origin_size(
            kurbo::Point::ZERO,
            widget_size,
        );
        painter.push_fill_clip(clip_rect);

        // Render each glyph path SEPARATELY to avoid winding conflicts
        let brush = Brush::Solid(self.color);
        for path in &self.paths {
            if !path.is_empty() {
                let transformed_path = transform * path;
                painter.fill(&transformed_path, &brush).transform(kurbo::Affine::IDENTITY).draw();
            }
        }

        painter.pop_clip();
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
}

// ===== Xilem View for MultiGlyphWidget =====

/// Create a multi-glyph view from a vector of BezPaths
pub fn multi_glyph_view<State, Action>(
    paths: Vec<BezPath>,
    width: f64,
    height: f64,
    upm: f64,
) -> MultiGlyphView<State, Action> {
    MultiGlyphView {
        paths,
        size: Size::new(width, height),
        color: None,
        upm,
        baseline_offset: None,
        fit_to_bounds: false,
        phantom: PhantomData,
    }
}

/// The Xilem View for MultiGlyphWidget
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct MultiGlyphView<State, Action = ()> {
    paths: Vec<BezPath>,
    size: Size,
    color: Option<Color>,
    upm: f64,
    baseline_offset: Option<f64>,
    fit_to_bounds: bool,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> MultiGlyphView<State, Action> {
    /// Set the glyph fill color
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Set the baseline offset (0.0 = bottom, 1.0 = top)
    pub fn baseline_offset(mut self, offset: f64) -> Self {
        self.baseline_offset = Some(offset);
        self
    }

    /// Enable fit-to-bounds mode
    pub fn fit_to_bounds(mut self) -> Self {
        self.fit_to_bounds = true;
        self
    }
}

impl<State, Action> ViewMarker for MultiGlyphView<State, Action> {}

impl<State: 'static, Action: 'static> View<State, Action, ViewCtx>
    for MultiGlyphView<State, Action>
{
    type Element = Pod<MultiGlyphWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let mut widget = MultiGlyphWidget::new(self.paths.clone(), self.size, self.upm);
        if let Some(color) = self.color {
            widget = widget.with_color(color);
        }
        if let Some(offset) = self.baseline_offset {
            widget = widget.with_baseline_offset(offset);
        }
        if self.fit_to_bounds {
            widget = widget.with_fit_to_bounds();
        }
        (ctx.create_pod(widget), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        let mut widget = element.downcast::<MultiGlyphWidget>();

        if self.paths != prev.paths {
            widget.widget.set_paths(self.paths.clone());
            widget.ctx.request_render();
        }

        if self.color != prev.color
            && let Some(color) = self.color
        {
            widget.widget.set_color(color);
            widget.ctx.request_render();
        }

        if self.upm != prev.upm {
            widget.widget.set_upm(self.upm);
            widget.ctx.request_render();
        }

        if self.baseline_offset != prev.baseline_offset
            && let Some(offset) = self.baseline_offset
        {
            widget.widget.set_baseline_offset(offset);
            widget.ctx.request_render();
        }

        if self.size != prev.size {
            widget.widget.set_size(self.size);
            widget.ctx.request_render();
        }

        if self.fit_to_bounds != prev.fit_to_bounds {
            widget.widget.set_fit_to_bounds(self.fit_to_bounds);
            widget.ctx.request_render();
        }
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
        _app_state: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Stale
    }
}

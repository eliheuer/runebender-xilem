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

use kurbo::{Affine, BezPath, Shape};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, LayoutCtx, NoAction, PaintCtx, PropertiesMut,
    PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget,
};
use masonry::kurbo::Size;
use masonry::vello::Scene;
use masonry::vello::peniko::{Brush, Color, Fill};

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

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        // Use the requested size, constrained by the BoxConstraints
        bc.constrain(self.size)
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        if self.path.is_empty() {
            return;
        }

        // Get the bounding box of the glyph path
        let bounds = self.path.bounding_box();
        let widget_size = ctx.size();

        // Calculate uniform scale based on UPM (units per em)
        // This ensures all glyphs are rendered at the same scale
        let scale = widget_size.height / self.upm;
        let scale = scale * 0.8; // 20% smaller (0.8 = 80% of original)

        // Center the glyph horizontally
        // If advance_width is provided, use it for stable centering
        // (prevents shifting during edits)
        // Otherwise, fall back to bounding box centering
        let x_translation = if let Some(advance_width) = self.advance_width {
            // Center based on advance width - stays constant while editing
            // Calculate where to position x=0 in font space so the advance
            // width is centered
            let scaled_advance = advance_width * scale;
            (widget_size.width - scaled_advance) / 2.0
        } else {
            // Fall back to bounding box centering
            // Center the visual bounding box of the glyph
            let scaled_width = bounds.width() * scale;
            let l_pad = (widget_size.width - scaled_width) / 2.0;
            l_pad - bounds.x0 * scale
        };

        // Position baseline to center glyphs vertically
        // (adjusted for better visual balance)
        // Higher percentage = baseline higher in cell = more space at bottom,
        // less at top
        let baseline = widget_size.height * self.baseline_offset;

        let transform = Affine::new([
            scale,                         // x scale
            0.0,                           // x skew
            0.0,                           // y skew
            -scale,                        // y scale (negative to flip Y axis)
            x_translation,                 // x translation (centering)
            widget_size.height - baseline, // y translation (baseline positioning)
        ]);

        // Apply transform to path
        let transformed_path = transform * &self.path;

        // Render the glyph using NonZero fill rule
        // This ensures overlapping shapes (like Arabic connectors) fill correctly
        // without gaps, unlike EvenOdd which alternates fill at each crossing
        scene.fill(
            Fill::NonZero,
            kurbo::Affine::IDENTITY,
            &Brush::Solid(self.color),
            None,
            &transformed_path,
        );
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
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
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
    phantom: PhantomData<fn() -> (State, Action)>,
}

// Builder methods for configuring the glyph view
impl<State, Action> GlyphView<State, Action> {
    /// Set the glyph fill color
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
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
        _message: &mut MessageContext,
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
// its own `scene.fill()` call using NonZero fill rule.

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
}

impl Widget for MultiGlyphWidget {
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

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        bc.constrain(self.size)
    }

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
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

        let widget_size = ctx.size();

        // Calculate uniform scale based on UPM
        let scale = widget_size.height / self.upm;
        let scale = scale * 0.8; // 20% smaller

        // Center horizontally based on combined bounding box
        let scaled_width = bounds.width() * scale;
        let l_pad = (widget_size.width - scaled_width) / 2.0;
        let x_translation = l_pad - bounds.x0 * scale;

        // Position baseline
        let baseline = widget_size.height * self.baseline_offset;

        let transform = Affine::new([
            scale,
            0.0,
            0.0,
            -scale,
            x_translation,
            widget_size.height - baseline,
        ]);

        // Render each glyph path SEPARATELY to avoid winding conflicts
        let brush = Brush::Solid(self.color);
        for path in &self.paths {
            if !path.is_empty() {
                let transformed_path = transform * path;
                scene.fill(
                    Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    &brush,
                    None,
                    &transformed_path,
                );
            }
        }
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
        _app_state: &mut State,
    ) -> MessageResult<Action> {
        MessageResult::Stale
    }
}

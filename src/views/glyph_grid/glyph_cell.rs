// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! GlyphCellWidget — custom Masonry widget for individual glyph
//! cells in the grid, with Xilem View wrapper

use std::marker::PhantomData;

use kurbo::{Axis, Affine, BezPath, Rect, RoundedRect, Shape, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, MeasureCtx, BrushIndex, ChildrenIds, EventCtx, LayoutCtx, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    StyleProperty, TextEvent, Update, UpdateCtx, Widget, render_text,
};
use masonry::imaging::Painter;
use masonry::layout::{LenReq, Length};
use masonry::peniko::{Brush, Color};
use parley::{FontContext, LayoutContext};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::theme;

// ============================================================
// GlyphCellAction
// ============================================================

/// Actions emitted by the glyph cell widget
#[derive(Debug, Clone)]
pub(super) enum GlyphCellAction {
    /// Single-click without shift — select this glyph only
    Select(String),
    /// Single-click with shift — toggle in/out of multi-select
    ShiftSelect(String),
    /// Double-click — open glyph in editor
    Open(String),
}

// ============================================================
// GlyphCellWidget (custom Masonry widget)
// ============================================================

/// Font size for cell labels
const CELL_LABEL_SIZE: f64 = 16.0;

thread_local! {
    static FONT_CX: std::cell::RefCell<FontContext> =
        std::cell::RefCell::new(FontContext::default());
    static LAYOUT_CX: std::cell::RefCell<
        LayoutContext<BrushIndex>,
    > = std::cell::RefCell::new(LayoutContext::new());
}
/// Height reserved for the label area at the bottom of the cell
const CELL_LABEL_HEIGHT: f64 = 56.0;
/// Padding around the glyph preview and labels
const CELL_PAD: f64 = 8.0;

/// Custom widget that renders a glyph cell and handles
/// click, double-click, and shift-click events.
pub(super) struct GlyphCellWidget {
    glyph_name: String,
    path: Option<BezPath>,
    codepoints: Vec<char>,
    upm: f64,
    is_selected: bool,
    mark_color: Option<usize>,
}

impl GlyphCellWidget {
    fn new(
        glyph_name: String,
        path: Option<BezPath>,
        codepoints: Vec<char>,
        upm: f64,
        is_selected: bool,
        mark_color: Option<usize>,
    ) -> Self {
        Self {
            glyph_name,
            path,
            codepoints,
            upm,
            is_selected,
            mark_color,
        }
    }

    /// Resolve the mark color to a Color value
    fn mark(&self) -> Option<Color> {
        self.mark_color.map(theme::mark::color)
    }

    /// Get (background, border) colors for this cell
    fn cell_colors(&self, is_hovered: bool) -> (Color, Color) {
        if self.is_selected || is_hovered {
            (
                theme::grid::CELL_BACKGROUND,
                theme::grid::CELL_SELECTED_OUTLINE,
            )
        } else if let Some(color) = self.mark() {
            (theme::grid::CELL_BACKGROUND, color)
        } else {
            (theme::grid::CELL_BACKGROUND, theme::grid::CELL_OUTLINE)
        }
    }

    /// Paint the glyph bezpath into the preview area
    fn paint_glyph(&self, painter: &mut Painter<'_>, preview_rect: Rect) {
        let path = match &self.path {
            Some(p) if !p.is_empty() => p,
            _ => return,
        };

        let bounds = path.bounding_box();
        let scale = preview_rect.height() / self.upm;
        let scale = scale * 0.65;

        // Center horizontally based on bounding box
        let scaled_width = bounds.width() * scale;
        let left_pad = (preview_rect.width() - scaled_width) / 2.0;
        let x_translation = preview_rect.x0 + left_pad - bounds.x0 * scale;

        // Baseline at ~20% from bottom of preview area
        let baseline_offset = 0.20;
        let baseline = preview_rect.height() * baseline_offset;

        let transform = Affine::new([
            scale,
            0.0,
            0.0,
            -scale,
            x_translation,
            preview_rect.y1 - baseline,
        ]);

        let transformed_path = transform * path;
        let color = if self.is_selected {
            theme::grid::CELL_SELECTED_OUTLINE
        } else {
            self.mark().unwrap_or(theme::grid::CELL_OUTLINE)
        };
        painter.fill(&transformed_path, &Brush::Solid(color)).draw();
    }

    /// Paint the name and unicode labels
    fn paint_labels(&self, painter: &mut Painter<'_>, label_rect: Rect, is_hovered: bool) {
        let text_color = if self.is_selected || is_hovered {
            theme::grid::CELL_SELECTED_OUTLINE
        } else {
            self.mark().unwrap_or(theme::grid::CELL_TEXT)
        };

        let display_name = format_display_name(&self.glyph_name);
        let unicode_display = format_unicode_display(&self.codepoints);

        FONT_CX.with(|font_cell| {
            LAYOUT_CX.with(|layout_cell| {
                let mut font_cx = font_cell.borrow_mut();
                let mut layout_cx = layout_cell.borrow_mut();

                // Name label
                let mut builder = layout_cx.ranged_builder(&mut font_cx, &display_name, 1.0, false);
                builder.push_default(StyleProperty::FontSize(CELL_LABEL_SIZE as f32));
                builder.push_default(StyleProperty::FontFamily(parley::FontFamily::Single(parley::FontFamilyName::Generic(parley::GenericFamily::SansSerif))));
                builder.push_default(StyleProperty::Brush(BrushIndex(0)));
                let mut name_layout = builder.build(&display_name);
                name_layout.break_all_lines(None);

                let brushes = vec![Brush::Solid(text_color)];
                // Anchor labels from bottom of label area
                // Subtract less than full text height to
                // compensate for visual line-height padding
                let two_lines = CELL_LABEL_SIZE * 2.0 + 4.0;
                let name_y = label_rect.y1 - two_lines + 5.0;
                render_text(
                    painter,
                    Affine::translate((label_rect.x0, name_y)),
                    &name_layout,
                    &brushes,
                    false,
                );

                // Unicode label
                if !unicode_display.is_empty() {
                    let mut builder =
                        layout_cx.ranged_builder(&mut font_cx, &unicode_display, 1.0, false);
                    builder.push_default(StyleProperty::FontSize(CELL_LABEL_SIZE as f32));
                    builder.push_default(StyleProperty::FontFamily(parley::FontFamily::Single(parley::FontFamilyName::Generic(parley::GenericFamily::SansSerif))));
                    builder.push_default(StyleProperty::Brush(BrushIndex(0)));
                    let mut uni_layout = builder.build(&unicode_display);
                    uni_layout.break_all_lines(None);

                    let uni_y = name_y + CELL_LABEL_SIZE + 2.0;
                    render_text(
                        painter,
                        Affine::translate((label_rect.x0, uni_y)),
                        &uni_layout,
                        &brushes,
                        false,
                    );
                }
            });
        });
    }
}

impl Widget for GlyphCellWidget {
    type Action = GlyphCellAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::HoveredChanged(_)) {
            ctx.request_render();
        }
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
        let (bg_color, border_color) = self.cell_colors(ctx.is_hovered());

        // Panel background and border
        let panel_rect = RoundedRect::from_rect(
            Rect::from_origin_size(kurbo::Point::ZERO, size),
            theme::size::PANEL_RADIUS,
        );
        painter.fill(&panel_rect, &Brush::Solid(bg_color)).draw();
        painter.stroke(&panel_rect, &kurbo::Stroke::new(theme::size::TOOLBAR_BORDER_WIDTH), &Brush::Solid(border_color)).draw();

        // Glyph preview area (above labels, inset by padding)
        let preview_height = (size.height - CELL_LABEL_HEIGHT).max(0.0);
        let preview_rect = Rect::new(CELL_PAD, CELL_PAD, size.width - CELL_PAD, preview_height);
        self.paint_glyph(painter, preview_rect);

        // Label area (bottom of cell, inset by padding)
        let label_rect = Rect::new(
            CELL_PAD,
            preview_height,
            size.width - CELL_PAD,
            size.height - CELL_PAD,
        );
        self.paint_labels(painter, label_rect, ctx.is_hovered());
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
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
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            state,
            ..
        }) = event
        {
            let name = self.glyph_name.clone();
            if state.count >= 2 {
                ctx.submit_action::<GlyphCellAction>(GlyphCellAction::Open(name));
            } else if state.modifiers.shift() {
                ctx.submit_action::<GlyphCellAction>(GlyphCellAction::ShiftSelect(name));
            } else {
                ctx.submit_action::<GlyphCellAction>(GlyphCellAction::Select(name));
            }
            // Don't set_handled — let Down bubble to the
            // GridScrollWidget container so it grabs focus
            // for arrow key scrolling.
        }
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
// GlyphCellView (Xilem View wrapper)
// ============================================================

type GlyphCellCallback<State> = Box<dyn Fn(&mut State, GlyphCellAction) + Send + Sync>;

pub(super) fn glyph_cell_view<State, Action>(
    glyph_name: String,
    path: Option<BezPath>,
    codepoints: Vec<char>,
    is_selected: bool,
    upm: f64,
    mark_color: Option<usize>,
    callback: impl Fn(&mut State, GlyphCellAction) + Send + Sync + 'static,
) -> GlyphCellView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    GlyphCellView {
        glyph_name,
        path,
        codepoints,
        is_selected,
        upm,
        mark_color,
        callback: Box::new(callback),
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub(super) struct GlyphCellView<State, Action = ()> {
    glyph_name: String,
    path: Option<BezPath>,
    codepoints: Vec<char>,
    is_selected: bool,
    upm: f64,
    mark_color: Option<usize>,
    callback: GlyphCellCallback<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker for GlyphCellView<State, Action> {}

impl<State: 'static, Action: 'static + Default> View<State, Action, ViewCtx>
    for GlyphCellView<State, Action>
{
    type Element = Pod<GlyphCellWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = GlyphCellWidget::new(
            self.glyph_name.clone(),
            self.path.clone(),
            self.codepoints.clone(),
            self.upm,
            self.is_selected,
            self.mark_color,
        );
        let pod = ctx.create_pod(widget);
        ctx.record_action_source(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        let mut changed = false;
        let w = &mut element.widget;
        if w.is_selected != self.is_selected {
            w.is_selected = self.is_selected;
            changed = true;
        }
        if w.glyph_name != self.glyph_name {
            w.glyph_name = self.glyph_name.clone();
            changed = true;
        }
        if self.path != prev.path {
            w.path = self.path.clone();
            changed = true;
        }
        if self.codepoints != prev.codepoints {
            w.codepoints = self.codepoints.clone();
            changed = true;
        }
        if self.upm != prev.upm {
            w.upm = self.upm;
            changed = true;
        }
        if self.mark_color != prev.mark_color {
            w.mark_color = self.mark_color;
            changed = true;
        }
        if changed {
            element.ctx.request_render();
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
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<GlyphCellAction>() {
            Some(action) => {
                (self.callback)(app_state, *action);
                MessageResult::Action(Action::default())
            }
            None => MessageResult::Stale,
        }
    }
}

// ============================================================
// Cell Formatting Helpers
// ============================================================

/// Format display name — show full name, the cell clips overflow
fn format_display_name(glyph_name: &str) -> String {
    glyph_name.to_string()
}

/// Format Unicode codepoint display string
fn format_unicode_display(codepoints: &[char]) -> String {
    if let Some(first_char) = codepoints.first() {
        format!("U+{:04X}", *first_char as u32)
    } else {
        String::new()
    }
}

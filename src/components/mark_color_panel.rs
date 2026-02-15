// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Mark color palette panel for glyph workflow organization
//!
//! Displays a grid of 12 color swatches plus a "clear" swatch.
//! Clicking a swatch sets the selected glyph's mark color.

use kurbo::{Affine, Circle, Rect, RoundedRect, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, BrushIndex, ChildrenIds, EventCtx,
    LayoutCtx, PaintCtx, PointerButton, PointerButtonEvent,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    StyleProperty, TextEvent, Update, UpdateCtx, Widget,
    render_text,
};
use masonry::vello::Scene;
use masonry::vello::peniko::{Brush, Color, Fill};
use parley::{FontContext, FontStack, LayoutContext};
use std::marker::PhantomData;
use xilem::core::{
    MessageContext, MessageResult, Mut, View, ViewMarker,
};
use xilem::{Pod, ViewCtx};

use crate::components::CATEGORY_PANEL_WIDTH;
use crate::theme;

// ============================================================
// Layout constants
// ============================================================

/// Left/right text inset within the panel
const TEXT_INSET: f64 = 12.0;
/// Vertical padding above the header label
const HEADER_TOP: f64 = 10.0;
/// Gap between header and first swatch row
const HEADER_GAP: f64 = 8.0;
/// Font size for the header label
const HEADER_FONT_SIZE: f64 = 16.0;
/// Swatch diameter
const SWATCH_SIZE: f64 = 24.0;
/// Gap between swatches
const SWATCH_GAP: f64 = 4.0;
/// Number of columns in the swatch grid
const SWATCH_COLS: usize = 7;
/// Horizontal inset for the swatch grid
const SWATCH_INSET: f64 = 10.0;
/// Total panel height
const PANEL_HEIGHT: f64 = 100.0;

// ============================================================
// Action
// ============================================================

/// Action emitted when a mark color swatch is clicked.
/// `None` means "clear", `Some(index)` is a palette index.
#[derive(Debug, Clone, Copy)]
pub struct MarkColorSelected(pub Option<usize>);

// ============================================================
// Custom Masonry Widget
// ============================================================

/// A custom widget that renders a grid of color swatches
pub struct MarkColorPanelWidget {
    selected_color: Option<usize>,
    hover_index: Option<usize>,
}

impl MarkColorPanelWidget {
    pub fn new(selected_color: Option<usize>) -> Self {
        Self {
            selected_color,
            hover_index: None,
        }
    }

    /// Y offset where the swatch grid begins
    fn swatches_top(&self) -> f64 {
        HEADER_TOP + HEADER_FONT_SIZE + HEADER_GAP
    }

    /// Center point for swatch at grid position (col, row)
    fn swatch_center(
        &self,
        col: usize,
        row: usize,
    ) -> (f64, f64) {
        let top = self.swatches_top();
        let radius = SWATCH_SIZE / 2.0;
        let cx = SWATCH_INSET
            + radius
            + col as f64 * (SWATCH_SIZE + SWATCH_GAP);
        let cy =
            top + radius + row as f64 * (SWATCH_SIZE + SWATCH_GAP);
        (cx, cy)
    }

    /// Hit-test: which swatch index (0–11 for colors, 12 for
    /// clear) is at the given position? Returns None if no hit.
    fn swatch_at_pos(&self, x: f64, y: f64) -> Option<usize> {
        let radius = SWATCH_SIZE / 2.0;
        // 13 color swatches + 1 clear = 14 total
        for i in 0..14 {
            let (col, row) = self.swatch_grid_pos(i);
            let (cx, cy) = self.swatch_center(col, row);
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= radius * radius {
                return Some(i);
            }
        }
        None
    }

    /// Map a linear swatch index to (col, row) in the grid.
    /// Index 0–12 = color swatches, index 13 = clear swatch.
    fn swatch_grid_pos(
        &self,
        index: usize,
    ) -> (usize, usize) {
        let col = index % SWATCH_COLS;
        let row = index / SWATCH_COLS;
        (col, row)
    }
}

impl Widget for MarkColorPanelWidget {
    type Action = MarkColorSelected;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

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
        let width = CATEGORY_PANEL_WIDTH;
        let height = PANEL_HEIGHT;
        bc.constrain(Size::new(width, height))
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        scene: &mut Scene,
    ) {
        let size = ctx.size();

        // --- Panel background and border ---
        let panel_rect = RoundedRect::from_rect(
            Rect::from_origin_size(kurbo::Point::ZERO, size),
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

        // --- Header ---
        let mut font_cx = FontContext::default();
        let mut layout_cx = LayoutContext::new();

        let header_text = "Colors";
        let mut builder = layout_cx.ranged_builder(
            &mut font_cx,
            header_text,
            1.0,
            false,
        );
        builder.push_default(StyleProperty::FontSize(
            HEADER_FONT_SIZE as f32,
        ));
        builder.push_default(StyleProperty::FontStack(
            FontStack::Single(parley::FontFamily::Generic(
                parley::GenericFamily::SansSerif,
            )),
        ));
        builder
            .push_default(StyleProperty::Brush(BrushIndex(0)));
        let mut layout = builder.build(header_text);
        layout.break_all_lines(None);

        let header_color: Color = theme::grid::CELL_TEXT;
        let header_brushes = vec![Brush::Solid(header_color)];
        render_text(
            scene,
            Affine::translate((TEXT_INSET, HEADER_TOP)),
            &layout,
            &header_brushes,
            false,
        );

        // --- Color swatches (12 colors) ---
        let radius = SWATCH_SIZE / 2.0;

        for i in 0..theme::mark::COUNT {
            let color = theme::mark::COLORS[i];
            let (col, row) = self.swatch_grid_pos(i);
            let (cx, cy) = self.swatch_center(col, row);
            let circle = Circle::new((cx, cy), radius);

            // Fill with mark color
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &Brush::Solid(color),
                None,
                &circle,
            );

            // Hover ring
            if self.hover_index == Some(i) {
                scene.stroke(
                    &kurbo::Stroke::new(1.5),
                    Affine::IDENTITY,
                    &Brush::Solid(theme::base::L),
                    None,
                    &circle,
                );
            }

            // Selected ring (white outline for current glyph color)
            if self.selected_color == Some(i) {
                let outer =
                    Circle::new((cx, cy), radius + 1.5);
                scene.stroke(
                    &kurbo::Stroke::new(2.0),
                    Affine::IDENTITY,
                    &Brush::Solid(Color::WHITE),
                    None,
                    &outer,
                );
            }
        }

        // --- Clear swatch (index 13) ---
        let (col, row) = self.swatch_grid_pos(13);
        let (cx, cy) = self.swatch_center(col, row);
        let circle = Circle::new((cx, cy), radius);

        // Hollow circle (outline only)
        scene.stroke(
            &kurbo::Stroke::new(1.5),
            Affine::IDENTITY,
            &Brush::Solid(theme::base::F),
            None,
            &circle,
        );

        // X mark inside
        let x_size = radius * 0.45;
        let stroke = kurbo::Stroke::new(1.5);
        let x_color = Brush::Solid(theme::base::F);
        scene.stroke(
            &stroke,
            Affine::IDENTITY,
            &x_color,
            None,
            &kurbo::Line::new(
                (cx - x_size, cy - x_size),
                (cx + x_size, cy + x_size),
            ),
        );
        scene.stroke(
            &stroke,
            Affine::IDENTITY,
            &x_color,
            None,
            &kurbo::Line::new(
                (cx + x_size, cy - x_size),
                (cx - x_size, cy + x_size),
            ),
        );

        // Hover ring for clear swatch
        if self.hover_index == Some(13) {
            scene.stroke(
                &kurbo::Stroke::new(1.5),
                Affine::IDENTITY,
                &Brush::Solid(theme::base::L),
                None,
                &circle,
            );
        }

        // Selected ring for clear (when glyph has no mark color)
        if self.selected_color.is_none() {
            // Don't show selected ring on clear — it's the default
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::List
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
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let pos = ctx.local_position(state.position);
                if let Some(index) = self.swatch_at_pos(pos.x, pos.y)
                {
                    let color = if index < theme::mark::COUNT {
                        Some(index)
                    } else {
                        None // Clear swatch
                    };
                    ctx.submit_action::<MarkColorSelected>(
                        MarkColorSelected(color),
                    );
                }
                ctx.set_handled();
            }
            PointerEvent::Move(pointer_move) => {
                let pos = ctx.local_position(
                    pointer_move.current.position,
                );
                let new_hover =
                    self.swatch_at_pos(pos.x, pos.y);
                if new_hover != self.hover_index {
                    self.hover_index = new_hover;
                    ctx.request_render();
                }
            }
            PointerEvent::Leave(_) => {
                if self.hover_index.is_some() {
                    self.hover_index = None;
                    ctx.request_render();
                }
            }
            _ => {}
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
// Xilem View wrapper
// ============================================================

type MarkColorCallback<State> =
    Box<dyn Fn(&mut State, Option<usize>) + Send + Sync>;

pub fn mark_color_panel<State, Action>(
    current_color: Option<usize>,
    callback: impl Fn(&mut State, Option<usize>)
        + Send
        + Sync
        + 'static,
) -> MarkColorPanelView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    MarkColorPanelView {
        current_color,
        callback: Box::new(callback),
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct MarkColorPanelView<State, Action = ()> {
    current_color: Option<usize>,
    callback: MarkColorCallback<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker
    for MarkColorPanelView<State, Action>
{
}

impl<State: 'static, Action: 'static + Default>
    View<State, Action, ViewCtx>
    for MarkColorPanelView<State, Action>
{
    type Element = Pod<MarkColorPanelWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget =
            MarkColorPanelWidget::new(self.current_color);
        let pod = ctx.create_pod(widget);
        ctx.record_action(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        _prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        if element.widget.selected_color != self.current_color {
            element.widget.selected_color = self.current_color;
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
        message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<MarkColorSelected>() {
            Some(action) => {
                (self.callback)(app_state, action.0);
                MessageResult::Action(Action::default())
            }
            None => MessageResult::Stale,
        }
    }
}

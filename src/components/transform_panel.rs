// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Transform panel — compact 2-column icon button grid for flip, rotate,
//! duplicate, and boolean operations.
//!
//! Follows the same custom-widget pattern as `edit_mode_toolbar.rs`,
//! reusing the shared toolbar paint helpers from `toolbars.rs`.

use std::marker::PhantomData;

use kurbo::{Affine, BezPath, Point, Rect, RoundedRect, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, BrushIndex, ChildrenIds, EventCtx,
    LayoutCtx, PaintCtx, PointerButton, PointerButtonEvent,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    StyleProperty, TextEvent, Update, UpdateCtx, Widget,
    render_text,
};
use masonry::vello::Scene;
use parley::{FontContext, GenericFamily, LayoutContext};
use peniko::Brush;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::components::toolbars::{
    ButtonState, paint_button, paint_icon, paint_panel,
};
use crate::theme::size::{
    TOOLBAR_ITEM_SIZE, TOOLBAR_ITEM_SPACING, TOOLBAR_PADDING,
};

// ================================================================
// CONSTANTS
// ================================================================

/// Number of columns in the grid
const COLS: usize = 2;
/// Number of rows in the grid
const ROWS: usize = 5;

// ================================================================
// TRANSFORM ACTION
// ================================================================

/// Actions the transform panel can trigger
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformAction {
    FlipH,
    FlipV,
    RotateCW,
    RotateCCW,
    Duplicate,
    DuplicateRepeat,
    Union,
    Subtract,
    Intersect,
    Exclude,
}

impl TransformAction {
    /// Human-readable label for tooltip display
    fn label(self) -> &'static str {
        match self {
            Self::FlipH => "Flip Horizontal",
            Self::FlipV => "Flip Vertical",
            Self::RotateCW => "Rotate 90° CW",
            Self::RotateCCW => "Rotate 90° CCW",
            Self::Duplicate => "Duplicate",
            Self::DuplicateRepeat => "Dup + Repeat",
            Self::Union => "Union",
            Self::Subtract => "Subtract",
            Self::Intersect => "Intersect",
            Self::Exclude => "Exclude (XOR)",
        }
    }
}

/// All buttons in row-major order (left-to-right, top-to-bottom)
const BUTTONS: &[TransformAction] = &[
    TransformAction::FlipH,
    TransformAction::FlipV,
    TransformAction::RotateCW,
    TransformAction::RotateCCW,
    TransformAction::Duplicate,
    TransformAction::DuplicateRepeat,
    TransformAction::Union,
    TransformAction::Subtract,
    TransformAction::Intersect,
    TransformAction::Exclude,
];

// ================================================================
// WIDGET
// ================================================================

/// Compact 2-column transform panel widget
pub struct TransformPanelWidget {
    /// Whether the glyph has any selected points
    has_selection: bool,
    /// Number of contours (for boolean ops)
    contour_count: usize,
    /// Currently hovered button index
    hover_index: Option<usize>,
}

impl TransformPanelWidget {
    pub fn new(has_selection: bool, contour_count: usize) -> Self {
        Self {
            has_selection,
            contour_count,
            hover_index: None,
        }
    }

    /// Calculate the overall widget size
    fn size() -> Size {
        let width = TOOLBAR_PADDING * 2.0
            + COLS as f64 * TOOLBAR_ITEM_SIZE
            + (COLS - 1) as f64 * TOOLBAR_ITEM_SPACING;
        let height = TOOLBAR_PADDING * 2.0
            + ROWS as f64 * TOOLBAR_ITEM_SIZE
            + (ROWS - 1) as f64 * TOOLBAR_ITEM_SPACING;
        Size::new(width, height)
    }

    /// Get the rect for button at (row, col)
    fn button_rect_at(row: usize, col: usize) -> Rect {
        let x = TOOLBAR_PADDING
            + col as f64
                * (TOOLBAR_ITEM_SIZE + TOOLBAR_ITEM_SPACING);
        let y = TOOLBAR_PADDING
            + row as f64
                * (TOOLBAR_ITEM_SIZE + TOOLBAR_ITEM_SPACING);
        Rect::new(
            x,
            y,
            x + TOOLBAR_ITEM_SIZE,
            y + TOOLBAR_ITEM_SIZE,
        )
    }

    /// Get the rect for a button by flat index
    fn button_rect(index: usize) -> Rect {
        let row = index / COLS;
        let col = index % COLS;
        Self::button_rect_at(row, col)
    }

    /// Find which button index was hit
    fn button_at_point(&self, point: Point) -> Option<usize> {
        for i in 0..BUTTONS.len() {
            if Self::button_rect(i).contains(point) {
                return Some(i);
            }
        }
        None
    }

    /// Whether a button is enabled
    fn is_enabled(&self, action: TransformAction) -> bool {
        match action {
            TransformAction::FlipH
            | TransformAction::FlipV
            | TransformAction::RotateCW
            | TransformAction::RotateCCW
            | TransformAction::Duplicate
            | TransformAction::DuplicateRepeat => self.has_selection,
            TransformAction::Union
            | TransformAction::Subtract
            | TransformAction::Intersect
            | TransformAction::Exclude => self.contour_count >= 2,
        }
    }

    /// Get icon BezPath for a transform action
    fn icon_for(action: TransformAction) -> BezPath {
        match action {
            TransformAction::FlipH => icon_flip_h(),
            TransformAction::FlipV => icon_flip_v(),
            TransformAction::RotateCW => icon_rotate_cw(),
            TransformAction::RotateCCW => icon_rotate_ccw(),
            TransformAction::Duplicate => icon_duplicate(),
            TransformAction::DuplicateRepeat => {
                icon_duplicate_repeat()
            }
            TransformAction::Union => icon_union(),
            TransformAction::Subtract => icon_subtract(),
            TransformAction::Intersect => icon_intersect(),
            TransformAction::Exclude => icon_exclude(),
        }
    }
}

impl Widget for TransformPanelWidget {
    type Action = TransformAction;

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
        bc.constrain(Self::size())
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        scene: &mut Scene,
    ) {
        let size = ctx.size();
        paint_panel(scene, size);

        for (i, &action) in BUTTONS.iter().enumerate() {
            let rect = Self::button_rect(i);
            let enabled = self.is_enabled(action);
            let is_hovered =
                self.hover_index == Some(i) && enabled;

            let state = ButtonState::new(is_hovered, false);
            paint_button(scene, rect, state);

            // Draw icon (dimmed if disabled)
            let icon = Self::icon_for(action);
            if enabled {
                paint_icon(scene, icon, rect, state);
            } else {
                // Draw dimmed icon
                paint_icon(
                    scene,
                    icon,
                    rect,
                    ButtonState::default(),
                );
            }
        }

        // Draw tooltip for hovered button
        if let Some(i) = self.hover_index {
            let action = BUTTONS[i];
            if self.is_enabled(action) {
                let label = action.label();
                let btn_rect = Self::button_rect(i);

                // Build text layout
                let mut font_cx = FontContext::default();
                let mut layout_cx = LayoutContext::new();
                let mut builder = layout_cx
                    .ranged_builder(
                        &mut font_cx, label, 1.0, false,
                    );
                builder.push_default(
                    StyleProperty::FontSize(12.0),
                );
                builder.push_default(
                    StyleProperty::FontStack(
                        parley::FontStack::Single(
                            parley::FontFamily::Generic(
                                GenericFamily::SansSerif,
                            ),
                        ),
                    ),
                );
                builder.push_default(
                    StyleProperty::Brush(BrushIndex(0)),
                );
                let mut layout = builder.build(label);
                layout.break_all_lines(None);

                let text_w = layout.width() as f64;
                let text_h = layout.height() as f64;
                let padding = 5.0;
                let gap = 6.0;

                // Position tooltip to the left of the button
                let tip_right = btn_rect.x0 - gap;
                let tip_x = tip_right - text_w - padding * 2.0;
                let tip_y = btn_rect.center().y
                    - (text_h + padding * 2.0) / 2.0;

                // Background bubble
                let bubble = RoundedRect::from_rect(
                    Rect::new(
                        tip_x,
                        tip_y,
                        tip_right,
                        tip_y + text_h + padding * 2.0,
                    ),
                    4.0,
                );
                let bg_brush =
                    Brush::Solid(peniko::Color::from_rgba8(
                        40, 40, 40, 230,
                    ));
                scene.fill(
                    peniko::Fill::NonZero,
                    Affine::IDENTITY,
                    &bg_brush,
                    None,
                    &bubble,
                );

                // Draw text
                let text_color = peniko::Color::from_rgba8(
                    240, 240, 240, 255,
                );
                let brushes = vec![Brush::Solid(text_color)];
                render_text(
                    scene,
                    Affine::translate((
                        tip_x + padding,
                        tip_y + padding,
                    )),
                    &layout,
                    &brushes,
                    false,
                );
            }
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Toolbar
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
                let local =
                    ctx.local_position(state.position);
                if let Some(i) = self.button_at_point(local) {
                    let action = BUTTONS[i];
                    if self.is_enabled(action) {
                        ctx.submit_action::<TransformAction>(
                            action,
                        );
                    }
                }
                ctx.set_handled();
            }
            PointerEvent::Move(pointer_move) => {
                let local = ctx.local_position(
                    pointer_move.current.position,
                );
                let new_hover = self.button_at_point(local);
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

// ================================================================
// XILEM VIEW WRAPPER
// ================================================================

/// Create a transform panel view
pub fn transform_panel<Action>(
    has_selection: bool,
    contour_count: usize,
    callback: impl Fn(&mut crate::data::AppState, TransformAction)
        + Send
        + Sync
        + 'static,
) -> TransformPanelView<Action> {
    TransformPanelView {
        has_selection,
        contour_count,
        callback: Box::new(callback),
        phantom: PhantomData,
    }
}

type TransformCallback = Box<
    dyn Fn(&mut crate::data::AppState, TransformAction)
        + Send
        + Sync,
>;

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct TransformPanelView<Action = ()> {
    has_selection: bool,
    contour_count: usize,
    callback: TransformCallback,
    phantom: PhantomData<fn() -> Action>,
}

impl<Action> ViewMarker for TransformPanelView<Action> {}

impl<Action: 'static + Default>
    View<crate::data::AppState, Action, ViewCtx>
    for TransformPanelView<Action>
{
    type Element = Pod<TransformPanelWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut crate::data::AppState,
    ) -> (Self::Element, Self::ViewState) {
        let widget = TransformPanelWidget::new(
            self.has_selection,
            self.contour_count,
        );
        let pod = ctx.create_pod(widget);
        ctx.record_action(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        _prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut crate::data::AppState,
    ) {
        let mut widget =
            element.downcast::<TransformPanelWidget>();
        let changed = widget.widget.has_selection
            != self.has_selection
            || widget.widget.contour_count
                != self.contour_count;
        if changed {
            widget.widget.has_selection = self.has_selection;
            widget.widget.contour_count = self.contour_count;
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
        message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        app_state: &mut crate::data::AppState,
    ) -> MessageResult<Action> {
        match message.take_message::<TransformAction>() {
            Some(action) => {
                (self.callback)(app_state, *action);
                MessageResult::Action(Action::default())
            }
            None => MessageResult::Stale,
        }
    }
}

// ================================================================
// ICON DEFINITIONS
// ================================================================
//
// Simple geometric icons drawn at ~768 unit scale (matching other
// toolbar icons). These are placeholders — replace with proper
// glyphs from the icon font later.

/// Flip horizontal: two mirrored right-angle shapes with a
/// vertical dashed axis
fn icon_flip_h() -> BezPath {
    let mut bez = BezPath::new();
    // Left arrow pointing right
    bez.move_to((100.0, 384.0));
    bez.line_to((300.0, 184.0));
    bez.line_to((300.0, 584.0));
    bez.close_path();
    // Vertical axis line
    bez.move_to((384.0, 100.0));
    bez.line_to((384.0, 668.0));
    bez.line_to((400.0, 668.0));
    bez.line_to((400.0, 100.0));
    bez.close_path();
    // Right arrow pointing left (mirrored)
    bez.move_to((684.0, 384.0));
    bez.line_to((484.0, 184.0));
    bez.line_to((484.0, 584.0));
    bez.close_path();
    bez
}

/// Flip vertical: two mirrored shapes with a horizontal axis
fn icon_flip_v() -> BezPath {
    let mut bez = BezPath::new();
    // Top arrow pointing down
    bez.move_to((384.0, 100.0));
    bez.line_to((184.0, 300.0));
    bez.line_to((584.0, 300.0));
    bez.close_path();
    // Horizontal axis line
    bez.move_to((100.0, 384.0));
    bez.line_to((668.0, 384.0));
    bez.line_to((668.0, 400.0));
    bez.line_to((100.0, 400.0));
    bez.close_path();
    // Bottom arrow pointing up (mirrored)
    bez.move_to((384.0, 684.0));
    bez.line_to((184.0, 484.0));
    bez.line_to((584.0, 484.0));
    bez.close_path();
    bez
}

/// Rotate clockwise: curved arrow going clockwise
fn icon_rotate_cw() -> BezPath {
    let mut bez = BezPath::new();
    // Arc body (thick arc, outer then inner)
    // Outer arc (top-right quadrant + a bit more)
    bez.move_to((384.0, 120.0));
    bez.curve_to(
        (550.0, 120.0),
        (668.0, 238.0),
        (668.0, 404.0),
    );
    bez.line_to((668.0, 500.0));
    // Arrowhead pointing down
    bez.line_to((740.0, 440.0));
    bez.line_to((668.0, 570.0));
    bez.line_to((596.0, 440.0));
    bez.line_to((668.0, 500.0));
    // Back up on inner arc
    bez.line_to((600.0, 404.0));
    bez.curve_to(
        (600.0, 276.0),
        (512.0, 188.0),
        (384.0, 188.0),
    );
    bez.line_to((384.0, 120.0));
    bez.close_path();
    // Left part of arc
    bez.move_to((384.0, 120.0));
    bez.curve_to(
        (218.0, 120.0),
        (100.0, 238.0),
        (100.0, 404.0),
    );
    bez.curve_to(
        (100.0, 570.0),
        (218.0, 668.0),
        (384.0, 668.0),
    );
    bez.line_to((384.0, 600.0));
    bez.curve_to(
        (256.0, 600.0),
        (168.0, 532.0),
        (168.0, 404.0),
    );
    bez.curve_to(
        (168.0, 276.0),
        (256.0, 188.0),
        (384.0, 188.0),
    );
    bez.line_to((384.0, 120.0));
    bez.close_path();
    bez
}

/// Rotate counter-clockwise: curved arrow going CCW
fn icon_rotate_ccw() -> BezPath {
    let mut bez = BezPath::new();
    // Arc body (outer)
    bez.move_to((384.0, 120.0));
    bez.curve_to(
        (218.0, 120.0),
        (100.0, 238.0),
        (100.0, 404.0),
    );
    bez.line_to((100.0, 500.0));
    // Arrowhead pointing down-left
    bez.line_to((28.0, 440.0));
    bez.line_to((100.0, 570.0));
    bez.line_to((172.0, 440.0));
    bez.line_to((100.0, 500.0));
    // Inner arc back up
    bez.line_to((168.0, 404.0));
    bez.curve_to(
        (168.0, 276.0),
        (256.0, 188.0),
        (384.0, 188.0),
    );
    bez.line_to((384.0, 120.0));
    bez.close_path();
    // Right side of arc
    bez.move_to((384.0, 120.0));
    bez.curve_to(
        (550.0, 120.0),
        (668.0, 238.0),
        (668.0, 404.0),
    );
    bez.curve_to(
        (668.0, 570.0),
        (550.0, 668.0),
        (384.0, 668.0),
    );
    bez.line_to((384.0, 600.0));
    bez.curve_to(
        (512.0, 600.0),
        (600.0, 532.0),
        (600.0, 404.0),
    );
    bez.curve_to(
        (600.0, 276.0),
        (512.0, 188.0),
        (384.0, 188.0),
    );
    bez.line_to((384.0, 120.0));
    bez.close_path();
    bez
}

/// Duplicate: two overlapping squares
fn icon_duplicate() -> BezPath {
    let mut bez = BezPath::new();
    // Back square (larger, offset)
    bez.move_to((150.0, 100.0));
    bez.line_to((550.0, 100.0));
    bez.line_to((550.0, 500.0));
    bez.line_to((150.0, 500.0));
    bez.close_path();
    // Inner cutout of back square
    bez.move_to((200.0, 150.0));
    bez.line_to((200.0, 450.0));
    bez.line_to((500.0, 450.0));
    bez.line_to((500.0, 150.0));
    bez.close_path();
    // Front square (offset down-right)
    bez.move_to((250.0, 268.0));
    bez.line_to((650.0, 268.0));
    bez.line_to((650.0, 668.0));
    bez.line_to((250.0, 668.0));
    bez.close_path();
    // Inner cutout of front square
    bez.move_to((300.0, 318.0));
    bez.line_to((300.0, 618.0));
    bez.line_to((600.0, 618.0));
    bez.line_to((600.0, 318.0));
    bez.close_path();
    bez
}

/// Duplicate + repeat: three overlapping squares
fn icon_duplicate_repeat() -> BezPath {
    let mut bez = BezPath::new();
    // First square (back)
    bez.move_to((100.0, 68.0));
    bez.line_to((450.0, 68.0));
    bez.line_to((450.0, 418.0));
    bez.line_to((100.0, 418.0));
    bez.close_path();
    bez.move_to((145.0, 113.0));
    bez.line_to((145.0, 373.0));
    bez.line_to((405.0, 373.0));
    bez.line_to((405.0, 113.0));
    bez.close_path();
    // Second square (middle)
    bez.move_to((220.0, 218.0));
    bez.line_to((570.0, 218.0));
    bez.line_to((570.0, 568.0));
    bez.line_to((220.0, 568.0));
    bez.close_path();
    bez.move_to((265.0, 263.0));
    bez.line_to((265.0, 523.0));
    bez.line_to((525.0, 523.0));
    bez.line_to((525.0, 263.0));
    bez.close_path();
    // Third square (front)
    bez.move_to((340.0, 368.0));
    bez.line_to((690.0, 368.0));
    bez.line_to((690.0, 718.0));
    bez.line_to((340.0, 718.0));
    bez.close_path();
    bez.move_to((385.0, 413.0));
    bez.line_to((385.0, 673.0));
    bez.line_to((645.0, 673.0));
    bez.line_to((645.0, 413.0));
    bez.close_path();
    bez
}

/// Union: two overlapping circles merged
fn icon_union() -> BezPath {
    let mut bez = BezPath::new();
    // Simple: two overlapping filled circles as a union shape
    // Left circle
    let cx1 = 300.0;
    let cy = 384.0;
    let r = 220.0;
    bez.move_to((cx1 + r, cy));
    bez.curve_to(
        (cx1 + r, cy + r * 0.55),
        (cx1 + r * 0.55, cy + r),
        (cx1, cy + r),
    );
    bez.curve_to(
        (cx1 - r * 0.55, cy + r),
        (cx1 - r, cy + r * 0.55),
        (cx1 - r, cy),
    );
    bez.curve_to(
        (cx1 - r, cy - r * 0.55),
        (cx1 - r * 0.55, cy - r),
        (cx1, cy - r),
    );
    bez.curve_to(
        (cx1 + r * 0.55, cy - r),
        (cx1 + r, cy - r * 0.55),
        (cx1 + r, cy),
    );
    bez.close_path();
    // Right circle
    let cx2 = 468.0;
    bez.move_to((cx2 + r, cy));
    bez.curve_to(
        (cx2 + r, cy + r * 0.55),
        (cx2 + r * 0.55, cy + r),
        (cx2, cy + r),
    );
    bez.curve_to(
        (cx2 - r * 0.55, cy + r),
        (cx2 - r, cy + r * 0.55),
        (cx2 - r, cy),
    );
    bez.curve_to(
        (cx2 - r, cy - r * 0.55),
        (cx2 - r * 0.55, cy - r),
        (cx2, cy - r),
    );
    bez.curve_to(
        (cx2 + r * 0.55, cy - r),
        (cx2 + r, cy - r * 0.55),
        (cx2 + r, cy),
    );
    bez.close_path();
    bez
}

/// Subtract: circle with a bite taken out
fn icon_subtract() -> BezPath {
    let mut bez = BezPath::new();
    // Single filled circle (left) — the "subtract" concept
    // shown as one circle with a dashed outline for the second
    let cx = 320.0;
    let cy = 384.0;
    let r = 230.0;
    bez.move_to((cx + r, cy));
    bez.curve_to(
        (cx + r, cy + r * 0.55),
        (cx + r * 0.55, cy + r),
        (cx, cy + r),
    );
    bez.curve_to(
        (cx - r * 0.55, cy + r),
        (cx - r, cy + r * 0.55),
        (cx - r, cy),
    );
    bez.curve_to(
        (cx - r, cy - r * 0.55),
        (cx - r * 0.55, cy - r),
        (cx, cy - r),
    );
    bez.curve_to(
        (cx + r * 0.55, cy - r),
        (cx + r, cy - r * 0.55),
        (cx + r, cy),
    );
    bez.close_path();
    // Right circle as hole (inner contour, reverse winding)
    let cx2 = 448.0;
    let r2 = 200.0;
    bez.move_to((cx2 + r2, cy));
    bez.curve_to(
        (cx2 + r2, cy - r2 * 0.55),
        (cx2 + r2 * 0.55, cy - r2),
        (cx2, cy - r2),
    );
    bez.curve_to(
        (cx2 - r2 * 0.55, cy - r2),
        (cx2 - r2, cy - r2 * 0.55),
        (cx2 - r2, cy),
    );
    bez.curve_to(
        (cx2 - r2, cy + r2 * 0.55),
        (cx2 - r2 * 0.55, cy + r2),
        (cx2, cy + r2),
    );
    bez.curve_to(
        (cx2 + r2 * 0.55, cy + r2),
        (cx2 + r2, cy + r2 * 0.55),
        (cx2 + r2, cy),
    );
    bez.close_path();
    bez
}

/// Intersect: lens/vesica shape (overlap area of two circles)
fn icon_intersect() -> BezPath {
    let mut bez = BezPath::new();
    // Vesica piscis / lens shape — the intersection of
    // two overlapping circles
    let cy = 384.0;
    bez.move_to((384.0, 164.0));
    bez.curve_to(
        (300.0, 220.0),
        (250.0, 296.0),
        (250.0, 384.0),
    );
    bez.curve_to(
        (250.0, 472.0),
        (300.0, 548.0),
        (384.0, 604.0),
    );
    bez.curve_to(
        (468.0, 548.0),
        (518.0, 472.0),
        (518.0, cy),
    );
    bez.curve_to(
        (518.0, 296.0),
        (468.0, 220.0),
        (384.0, 164.0),
    );
    bez.close_path();
    bez
}

/// Exclude (XOR): two circles with overlap removed
fn icon_exclude() -> BezPath {
    let mut bez = BezPath::new();
    // Left circle (outer)
    let cx1 = 300.0;
    let cy = 384.0;
    let r = 210.0;
    bez.move_to((cx1 + r, cy));
    bez.curve_to(
        (cx1 + r, cy + r * 0.55),
        (cx1 + r * 0.55, cy + r),
        (cx1, cy + r),
    );
    bez.curve_to(
        (cx1 - r * 0.55, cy + r),
        (cx1 - r, cy + r * 0.55),
        (cx1 - r, cy),
    );
    bez.curve_to(
        (cx1 - r, cy - r * 0.55),
        (cx1 - r * 0.55, cy - r),
        (cx1, cy - r),
    );
    bez.curve_to(
        (cx1 + r * 0.55, cy - r),
        (cx1 + r, cy - r * 0.55),
        (cx1 + r, cy),
    );
    bez.close_path();
    // Right circle (outer)
    let cx2 = 468.0;
    bez.move_to((cx2 + r, cy));
    bez.curve_to(
        (cx2 + r, cy + r * 0.55),
        (cx2 + r * 0.55, cy + r),
        (cx2, cy + r),
    );
    bez.curve_to(
        (cx2 - r * 0.55, cy + r),
        (cx2 - r, cy + r * 0.55),
        (cx2 - r, cy),
    );
    bez.curve_to(
        (cx2 - r, cy - r * 0.55),
        (cx2 - r * 0.55, cy - r),
        (cx2, cy - r),
    );
    bez.curve_to(
        (cx2 + r * 0.55, cy - r),
        (cx2 + r, cy - r * 0.55),
        (cx2 + r, cy),
    );
    bez.close_path();
    // Overlap lens (hole — reverse winding) to punch out
    // the intersection area, creating XOR effect
    bez.move_to((384.0, 174.0));
    bez.curve_to(
        (460.0, 224.0),
        (508.0, 298.0),
        (508.0, cy),
    );
    bez.curve_to(
        (508.0, 470.0),
        (460.0, 544.0),
        (384.0, 594.0),
    );
    bez.curve_to(
        (308.0, 544.0),
        (260.0, 470.0),
        (260.0, cy),
    );
    bez.curve_to(
        (260.0, 298.0),
        (308.0, 224.0),
        (384.0, 174.0),
    );
    bez.close_path();
    bez
}

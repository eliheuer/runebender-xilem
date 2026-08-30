// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A right-click menu that is not trapped inside the canvas.
//!
//! The editor used to paint its context menu into its own scene and hit
//! test it by hand, which works until the menu is opened near an edge:
//! it is clipped by the canvas it belongs to, because it is part of that
//! canvas.
//!
//! Masonry has the right mechanism, and has had it all along. A widget
//! can create a *layer*, which is a widget rooted above the tree in
//! window coordinates, receiving pointer events before anything else.
//! Masonry's own tooltip and selector menu use it.
//!
//! What is missing is the reactive half: there is no Xilem view for
//! layers, so an application built out of views cannot open one. This
//! file only exists because the editor is a hand-written Masonry widget
//! and can therefore reach `create_layer` directly. An application that
//! stayed in view-land could not do this at all, which is the gap worth
//! recording (XILEM-GAPS.md).

use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, Layer, LayoutCtx, MeasureCtx, NoAction, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef,
    RegisterCtx, Widget, WidgetId,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size, Stroke};
use masonry::layout::{LenReq, Length};

use crate::editor::EditorWidget;
use crate::text_label::{self, Anchor};
use crate::theme::Palette;

/// Row height, and the menu's width. Fixed, because a context menu that
/// resizes to its longest label is harder to aim at than one that does
/// not.
const ROW: f64 = 24.0;
const WIDTH: f64 = 184.0;
const PAD: f64 = 4.0;

/// What a row does when it is chosen.
///
/// The ops are the session ops the editor already exposes; `AddAnchor`
/// is separate because it needs the position the menu was opened at.
#[derive(Clone, Copy)]
pub enum MenuAction {
    /// Run a session operation, and report whether the glyph changed.
    Op(fn(&mut crate::session::Session) -> bool),
    /// Add an anchor where the menu was opened.
    AddAnchor,
}

/// One row.
pub struct MenuRow {
    pub label: &'static str,
    pub action: MenuAction,
}

/// The context menu, as a layer.
pub struct ContextMenu {
    /// The editor that opened it, so the choice can be applied there.
    creator: WidgetId,
    rows: &'static [MenuRow],
    palette: Arc<Palette>,
    /// Where it was opened, in design space, for `AddAnchor`.
    at: Point,
    hovered: Option<usize>,
    size: Size,
}

impl ContextMenu {
    pub fn new(
        creator: WidgetId,
        rows: &'static [MenuRow],
        palette: Arc<Palette>,
        at: Point,
    ) -> Self {
        Self {
            creator,
            rows,
            palette,
            at,
            hovered: None,
            size: Size::ZERO,
        }
    }

    /// The row under a local point, if any.
    fn row_at(&self, point: Point) -> Option<usize> {
        if point.x < 0.0 || point.x > WIDTH {
            return None;
        }
        let index = ((point.y - PAD) / ROW).floor();
        (index >= 0.0 && (index as usize) < self.rows.len()).then_some(index as usize)
    }
}

impl Widget for ContextMenu {
    type Action = NoAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        _cross: Option<Length>,
    ) -> Length {
        match axis {
            Axis::Horizontal => Length::px(WIDTH),
            Axis::Vertical => Length::px(self.rows.len() as f64 * ROW + PAD * 2.0),
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let pal = &self.palette;
        let frame = self.size.to_rect();
        painter.fill(frame.to_rounded_rect(6.0), pal.panel).draw();
        painter
            .stroke(
                &frame.to_rounded_rect(6.0),
                &Stroke::new(1.0),
                pal.role("gridBorder"),
            )
            .draw();
        for (index, row) in self.rows.iter().enumerate() {
            let top = PAD + index as f64 * ROW;
            let rect = Rect::new(2.0, top, WIDTH - 2.0, top + ROW);
            if self.hovered == Some(index) {
                painter
                    .fill(
                        rect.to_rounded_rect(4.0),
                        pal.role("gridSelected").with_alpha(0.25),
                    )
                    .draw();
            }
            text_label::draw(
                painter,
                Point::new(10.0, top + ROW / 2.0 + 4.0),
                row.label,
                12.0,
                pal.text,
                Anchor::Start,
            );
        }
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let hovered = self.row_at(ctx.local_position(current.position));
                if hovered != self.hovered {
                    self.hovered = hovered;
                    ctx.request_render();
                }
                ctx.set_handled();
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let Some(index) = self.row_at(ctx.local_position(state.position)) else {
                    return;
                };
                let action = self.rows[index].action;
                let at = self.at;
                let self_id = ctx.widget_id();
                // The choice is applied on the editor that opened this,
                // which is also where the layer is torn down. Masonry's
                // own selector menu does the same thing for the same
                // reason: a layer is not a child of its creator, so it
                // cannot submit an action that reaches it.
                ctx.mutate_later(self.creator, move |mut editor| {
                    let mut editor = editor.downcast::<EditorWidget>();
                    EditorWidget::apply_menu_choice(&mut editor, action, at);
                    editor.ctx.remove_layer(self_id);
                });
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Menu
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

    fn as_layer(&mut self) -> Option<&mut dyn Layer> {
        Some(self)
    }
}

impl Layer for ContextMenu {
    fn capture_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        let dismiss = match event {
            PointerEvent::Down(PointerButtonEvent { state, .. }) => !ctx
                .border_box()
                .contains(ctx.local_position(state.position)),
            PointerEvent::Cancel(..) => true,
            _ => false,
        };
        if dismiss {
            let self_id = ctx.widget_id();
            ctx.mutate_later(self.creator, move |mut editor| {
                let mut editor = editor.downcast::<EditorWidget>();
                EditorWidget::forget_menu(&mut editor);
                editor.ctx.remove_layer(self_id);
            });
        }
    }
}

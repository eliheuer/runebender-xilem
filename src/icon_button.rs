// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A 30x30 icon tile that paints one of runebender-core's toolbar icons
//! and reports clicks. Matches runebender-gpui's `icon_tile`.
//!
//! xix note: an icon button that paints a vector path is something the
//! framework should offer; here we paint the core icon directly.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, Size, Stroke};
use masonry::layout::{LenReq, Length};
use runebender_core::theme_oklch::toolbar_icons;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

const TILE: f64 = 30.0;

#[derive(Debug)]
pub struct IconClicked;

pub struct IconWidget {
    icon: &'static str,
    active: bool,
    fg: Color,
    fg_active: Color,
    active_bg: Color,
    hover_bg: Color,
    size: Size,
    hovered: bool,
}

impl Widget for IconWidget {
    type Action = IconClicked;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross: Option<Length>,
    ) -> Length {
        Length::px(TILE)
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let rect = self.size.to_rect();
        if self.active {
            painter.fill(rect.to_rounded_rect(6.0), self.active_bg).draw();
        } else if self.hovered {
            painter.fill(rect.to_rounded_rect(6.0), self.hover_bg).draw();
        }
        let Some(icon) = toolbar_icons().get(self.icon) else {
            return;
        };
        let pad = self.size.width.min(self.size.height) * 0.22;
        let vb = icon.view_box;
        let scale = ((self.size.width - pad * 2.0) / vb.width())
            .min((self.size.height - pad * 2.0) / vb.height());
        let dx = (self.size.width - vb.width() * scale) / 2.0;
        let dy = (self.size.height - vb.height() * scale) / 2.0;
        let t = Affine::translate((dx, dy)) * Affine::scale(scale) * Affine::translate((-vb.x0, -vb.y0));
        let color = if self.active { self.fg_active } else { self.fg };
        let path = t * icon.path.clone();
        if icon.stroke {
            painter.stroke(&path, &Stroke::new(1.5), color).draw();
        } else {
            painter.fill(&path, color).draw();
        }
    }

    fn on_pointer_event(&mut self, ctx: &mut EventCtx<'_>, _props: &mut PropertiesMut<'_>, event: &PointerEvent) {
        match event {
            PointerEvent::Enter(_) => {
                self.hovered = true;
                ctx.request_render();
            }
            PointerEvent::Leave(_) => {
                self.hovered = false;
                ctx.request_render();
            }
            PointerEvent::Down(PointerButtonEvent { button: Some(PointerButton::Primary), .. }) => {
                ctx.submit_action::<IconClicked>(IconClicked);
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(&mut self, _ctx: &mut AccessCtx<'_>, _props: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.icon);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

pub struct IconView<F> {
    icon: &'static str,
    active: bool,
    fg: Color,
    fg_active: Color,
    active_bg: Color,
    hover_bg: Color,
    on_click: F,
}

#[allow(clippy::too_many_arguments)]
pub fn icon_button<State: 'static, F: Fn(&mut State) + 'static>(
    icon: &'static str,
    active: bool,
    fg: Color,
    fg_active: Color,
    active_bg: Color,
    hover_bg: Color,
    on_click: F,
) -> IconView<F> {
    IconView { icon, active, fg, fg_active, active_bg, hover_bg, on_click }
}

impl<F> ViewMarker for IconView<F> {}
impl<State: 'static, F: Fn(&mut State) + 'static> View<State, (), ViewCtx> for IconView<F> {
    type Element = Pod<IconWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        let w = IconWidget {
            icon: self.icon,
            active: self.active,
            fg: self.fg,
            fg_active: self.fg_active,
            active_bg: self.active_bg,
            hover_bg: self.hover_bg,
            size: Size::ZERO,
            hovered: false,
        };
        (ctx.with_action_widget(|ctx| ctx.create_pod(w)), ())
    }

    fn rebuild(&self, prev: &Self, (): &mut Self::ViewState, _ctx: &mut ViewCtx, mut el: Mut<'_, Self::Element>, _: &mut State) {
        if self.active != prev.active {
            el.widget.active = self.active;
            el.ctx.request_render();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(&self, (): &mut Self::ViewState, message: &mut MessageCtx, _el: Mut<'_, Self::Element>, state: &mut State) -> MessageResult<()> {
        match message.take_message::<IconClicked>() {
            Some(_) => {
                (self.on_click)(state);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

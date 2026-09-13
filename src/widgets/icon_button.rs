// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A 24x24 icon tile that paints one of runebender-core's toolbar icons
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
use masonry::kurbo::{Affine, Axis, BezPath, Size, Stroke};
use masonry::layout::{LenReq, Length};
use runebender_core::ui::theme::toolbar_icons;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::view::design::{RAIL_TAB_ICON, RAIL_TAB_RADIUS};

const TILE: f64 = 24.0;

#[derive(Debug)]
pub(crate) struct IconClicked;

pub(crate) struct IconWidget {
    icon: &'static str,
    active: bool,
    fg: Color,
    fg_active: Color,
    active_bg: Color,
    hover_bg: Color,
    rail: Option<(Color, Color, f64)>,
    icon_size: Option<f64>,
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

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let rect = self.size.to_rect();
        if let Some((background, border, _)) = self.rail {
            // Open at the bottom when selected, joining the panel below.
            let r = RAIL_TAB_RADIUS;
            let w = self.size.width;
            let h = self.size.height;
            if self.active {
                let mut face = BezPath::new();
                face.move_to((0.5, h));
                face.line_to((0.5, r));
                face.quad_to((0.5, 0.5), (r, 0.5));
                face.line_to((w - r, 0.5));
                face.quad_to((w - 0.5, 0.5), (w - 0.5, r));
                face.line_to((w - 0.5, h));
                painter.fill(&face, background).draw();
                painter.stroke(&face, &Stroke::new(1.0), border).draw();
            } else {
                let face = rect.inset(-0.5).to_rounded_rect(r);
                painter.fill(face, background).draw();
                painter.stroke(face, &Stroke::new(1.0), border).draw();
            }
        }

        if self.rail.is_none() && self.active {
            painter
                .fill(rect.to_rounded_rect(6.0), self.active_bg)
                .draw();
        } else if self.rail.is_none() && self.hovered {
            painter
                .fill(rect.to_rounded_rect(6.0), self.hover_bg)
                .draw();
        }
        let Some(icon) = toolbar_icons().get(self.icon) else {
            return;
        };
        let pad = self.size.width.min(self.size.height) * 0.10;
        let vb = icon.view_box;
        let scale = if self.rail.is_some() {
            RAIL_TAB_ICON / vb.width().max(vb.height())
        } else if let Some(side) = self.icon_size {
            side.min(self.size.width).min(self.size.height) / vb.width().max(vb.height())
        } else {
            ((self.size.width - pad * 2.0) / vb.width())
                .min((self.size.height - pad * 2.0) / vb.height())
        };
        let dx = (self.size.width - vb.width() * scale) / 2.0;
        let dy = (self.size.height - vb.height() * scale) / 2.0
            - if self.rail.is_some() && self.active {
                self.rail.map(|(_, _, rise)| rise).unwrap_or_default()
            } else {
                0.0
            };
        let t = Affine::translate((dx, dy))
            * Affine::scale(scale)
            * Affine::translate((-vb.x0, -vb.y0));
        let color = if self.active || (self.rail.is_some() && self.hovered) {
            self.fg_active
        } else {
            self.fg
        };
        let path = t * icon.path.clone();
        if icon.stroke {
            painter.stroke(&path, &Stroke::new(1.5), color).draw();
        } else {
            painter.fill(&path, color).draw();
        }
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Enter(_) => {
                self.hovered = true;
                ctx.request_render();
            }
            PointerEvent::Leave(_) => {
                self.hovered = false;
                ctx.request_render();
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                ctx.submit_action::<IconClicked>(IconClicked);
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.icon);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

pub(crate) struct IconView<F> {
    icon: &'static str,
    active: bool,
    fg: Color,
    fg_active: Color,
    active_bg: Color,
    hover_bg: Color,
    rail: Option<(Color, Color, f64)>,
    icon_size: Option<f64>,
    on_click: F,
}

pub(crate) fn icon_button<State: 'static, F: Fn(&mut State) + 'static>(
    icon: &'static str,
    active: bool,
    fg: Color,
    fg_active: Color,
    active_bg: Color,
    hover_bg: Color,
    on_click: F,
) -> IconView<F> {
    IconView {
        icon,
        active,
        fg,
        fg_active,
        active_bg,
        hover_bg,
        rail: None,
        icon_size: None,
        on_click,
    }
}

impl<F> IconView<F> {
    /// Set the icon's maximum ink extent in logical pixels, clamped to its tile.
    /// Rail tabs continue to use the rail's own icon-size token.
    pub(crate) fn icon_size(mut self, size: f64) -> Self {
        self.icon_size = Some(size.max(0.0));
        self
    }

    /// Paint a GPUI-style rail tab around the icon.
    pub(crate) fn rail_tab(mut self, background: Color, border: Color, icon_rise: f64) -> Self {
        self.rail = Some((background, border, icon_rise));
        self
    }
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
            rail: self.rail,
            icon_size: self.icon_size,
            size: Size::ZERO,
            hovered: false,
        };
        (ctx.with_action_widget(|ctx| ctx.create_pod(w)), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.active != prev.active
            || self.rail != prev.rail
            || self.fg != prev.fg
            || self.fg_active != prev.fg_active
            || self.icon_size != prev.icon_size
        {
            el.widget.active = self.active;
            el.widget.rail = self.rail;
            el.widget.icon_size = self.icon_size;
            el.widget.fg = self.fg;
            el.widget.fg_active = self.fg_active;
            el.ctx.request_render();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _el: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<()> {
        match message.take_message::<IconClicked>() {
            Some(_) => {
                (self.on_click)(state);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

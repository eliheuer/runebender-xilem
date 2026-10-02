// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An icon tile that paints a glyph from the bundled icon UFO and reports clicks.

use std::sync::{Arc, Mutex};

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Widget, WidgetId,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, BezPath, Insets, Rect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use runebender::ui::icons::icons;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::application::view::design::RAIL_TAB_ICON;
use crate::application::widgets::icon_paint;

const TILE: f64 = 24.0;

#[derive(Debug)]
pub(crate) struct IconClicked;

pub(crate) struct IconWidget {
    icon: &'static str,
    label: &'static str,
    active: bool,
    fg: Color,
    fg_active: Color,
    active_bg: Color,
    hover_bg: Color,
    rail: Option<(Color, Color, f64, f64)>,
    icon_size: Option<f64>,
    tile_size: f64,
    corner_radius: f64,
    focus_target: Option<Arc<Mutex<Option<WidgetId>>>>,
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
        Length::px(self.tile_size)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        let flare = self
            .rail
            .filter(|_| self.active)
            .map_or(0.0, |(_, _, _, radius)| {
                radius.max(0.5).min(size.width.min(size.height) / 2.0)
            });
        ctx.set_paint_insets(Insets::new(
            flare,
            0.0,
            flare,
            if flare > 0.0 { 1.0 } else { 0.0 },
        ));
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let rect = self.size.to_rect();
        if let Some((background, border, _, radius)) = self.rail {
            // Open at the bottom, with outward curves joining the panel below.
            let r = radius
                .max(0.5)
                .min(self.size.width.min(self.size.height) / 2.0);
            let w = self.size.width;
            let h = self.size.height;
            if self.active {
                let mut face = BezPath::new();
                // Quarter-circle control points match Kurbo's rounded rectangles.
                let k = r * 0.552_284_749_830_793_6;
                let left = 0.5;
                let right = w - 0.5;
                let top = 0.5;
                let bottom = h - 0.5;
                face.move_to((left - r, bottom));
                face.curve_to(
                    (left - r + k, bottom),
                    (left, bottom - r + k),
                    (left, bottom - r),
                );
                face.line_to((left, top + r));
                face.curve_to((left, top + r - k), (left + r - k, top), (left + r, top));
                face.line_to((right - r, top));
                face.curve_to((right - r + k, top), (right, top + r - k), (right, top + r));
                face.line_to((right, bottom - r));
                face.curve_to(
                    (right, bottom - r + k),
                    (right + r - k, bottom),
                    (right + r, bottom),
                );
                let mut fill = face.clone();
                fill.line_to((w - 0.5 + r, h + 1.0));
                fill.line_to((0.5 - r, h + 1.0));
                fill.close_path();
                painter.fill(&fill, background).draw();
                painter.stroke(&face, &Stroke::new(1.0), border).draw();
            } else if radius > 0.0 {
                let face = rect.inset(0.5).to_rounded_rect(r);
                painter.fill(face, background).draw();
                painter.stroke(face, &Stroke::new(1.0), border).draw();
            } else {
                let face = rect.inset(0.5);
                painter.fill(face, background).draw();
                painter.stroke(face, &Stroke::new(1.0), border).draw();
            }
        }

        if self.rail.is_none() && (self.active || self.hovered) {
            let fill = if self.active {
                self.active_bg
            } else {
                self.hover_bg
            };
            if self.corner_radius > 0.0 {
                painter
                    .fill(rect.to_rounded_rect(self.corner_radius), fill)
                    .draw();
            } else {
                painter.fill(rect, fill).draw();
            }
        }
        let color = if self.active || (self.rail.is_some() && self.hovered) {
            self.fg_active
        } else {
            self.fg
        };
        let Some(icon) = icons().get(self.icon) else {
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
                self.rail.map(|(_, _, rise, _)| rise).unwrap_or_default()
            } else {
                0.0
            };
        icon_paint::paint(
            painter,
            self.icon,
            Rect::new(dx, dy, dx + vb.width() * scale, dy + vb.height() * scale),
            color,
        );
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
                if let Some(target) = self
                    .focus_target
                    .as_ref()
                    .and_then(|target| *target.lock().unwrap_or_else(|error| error.into_inner()))
                {
                    ctx.set_focus(target);
                }
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
        node.set_label(self.label);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

pub(crate) struct IconView<F> {
    icon: &'static str,
    label: &'static str,
    active: bool,
    fg: Color,
    fg_active: Color,
    active_bg: Color,
    hover_bg: Color,
    rail: Option<(Color, Color, f64, f64)>,
    icon_size: Option<f64>,
    tile_size: f64,
    corner_radius: f64,
    focus_target: Option<Arc<Mutex<Option<WidgetId>>>>,
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
        label: icon,
        active,
        fg,
        fg_active,
        active_bg,
        hover_bg,
        rail: None,
        icon_size: None,
        tile_size: TILE,
        corner_radius: 0.0,
        focus_target: None,
        on_click,
    }
}

/// An icon button with a descriptive accessibility label.
pub(crate) fn named_icon_button<State: 'static, F: Fn(&mut State) + 'static>(
    label: &'static str,
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
        label,
        active,
        fg,
        fg_active,
        active_bg,
        hover_bg,
        rail: None,
        icon_size: None,
        tile_size: TILE,
        corner_radius: 0.0,
        focus_target: None,
        on_click,
    }
}

impl<F> IconView<F> {
    /// Apply the active theme to the control background.
    pub(crate) fn corner_radius(mut self, radius: f64) -> Self {
        self.corner_radius = radius;
        self
    }

    /// Set the icon's maximum ink extent in logical pixels, clamped to its tile.
    /// Rail tabs continue to use the rail's own icon-size token.
    pub(crate) fn icon_size(mut self, size: f64) -> Self {
        self.icon_size = Some(size.max(0.0));
        self
    }

    /// Set the square pointer target for a compact icon control.
    pub(crate) fn tile_size(mut self, size: f64) -> Self {
        self.tile_size = size.max(0.0);
        self
    }

    /// Transfer focus to another widget as part of this control's click.
    pub(crate) fn focus_target(mut self, target: Arc<Mutex<Option<WidgetId>>>) -> Self {
        self.focus_target = Some(target);
        self
    }

    /// Paint a GPUI-style rail tab around the icon.
    pub(crate) fn rail_tab(
        mut self,
        background: Color,
        border: Color,
        icon_rise: f64,
        radius: f64,
    ) -> Self {
        self.rail = Some((background, border, icon_rise, radius));
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
            label: self.label,
            active: self.active,
            fg: self.fg,
            fg_active: self.fg_active,
            active_bg: self.active_bg,
            hover_bg: self.hover_bg,
            rail: self.rail,
            icon_size: self.icon_size,
            tile_size: self.tile_size,
            corner_radius: self.corner_radius,
            focus_target: self.focus_target.clone(),
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
        if self.icon != prev.icon
            || self.active != prev.active
            || self.rail != prev.rail
            || self.fg != prev.fg
            || self.fg_active != prev.fg_active
            || self.active_bg != prev.active_bg
            || self.hover_bg != prev.hover_bg
            || self.icon_size != prev.icon_size
            || self.corner_radius != prev.corner_radius
            || self.tile_size != prev.tile_size
        {
            el.widget.icon = self.icon;
            el.widget.active = self.active;
            el.widget.rail = self.rail;
            el.widget.icon_size = self.icon_size;
            el.widget.tile_size = self.tile_size;
            el.widget.corner_radius = self.corner_radius;
            el.widget.fg = self.fg;
            el.widget.fg_active = self.fg_active;
            el.widget.active_bg = self.active_bg;
            el.widget.hover_bg = self.hover_bg;
            el.ctx.request_render();
        }
        el.widget.label = self.label;
        el.widget.focus_target = self.focus_target.clone();
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

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::core::NewWidget;
    use masonry::theme::default_property_set;
    use masonry::widgets::{Button, Flex, Label};
    use masonry_testing::TestHarness;

    #[test]
    fn a_tool_control_can_focus_the_editor_on_click() {
        let editor = NewWidget::new(Button::new(Label::new("editor").prepare()));
        let editor_id = editor.id();
        let target = Arc::new(Mutex::new(Some(editor_id)));
        let tool = NewWidget::new(IconWidget {
            icon: "text",
            label: "Text tool",
            active: false,
            fg: Color::BLACK,
            fg_active: Color::BLACK,
            active_bg: Color::TRANSPARENT,
            hover_bg: Color::TRANSPARENT,
            rail: None,
            icon_size: None,
            tile_size: TILE,
            corner_radius: 0.0,
            focus_target: Some(target),
            size: Size::ZERO,
            hovered: false,
        });
        let tool_id = tool.id();
        let row = Flex::row().with_fixed(tool).with_fixed(editor).prepare();
        let mut harness = TestHarness::create_with_size(default_property_set(), row, (160, 40));

        harness.mouse_click_on(tool_id, Some(PointerButton::Primary));

        assert_eq!(harness.focused_widget_id(), Some(editor_id));
    }
}

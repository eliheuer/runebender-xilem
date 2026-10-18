// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An icon tile that paints a glyph from the bundled icon UFO and reports clicks.
//!
//! Its look comes from a [`ControlStyle`], like every other pressable
//! control: the face and ink for rest, hover, pressed, and selected, and a
//! keyboard focus ring. A click lands on release inside the tile.

use std::sync::{Arc, Mutex};

use masonry::accesskit::{Action, Node, Role, Toggled};
use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget, WidgetId,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, BezPath, Insets, Rect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use runebender::ui::icons::icons;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::application::design::{self, ControlStyle, RAIL_TAB_ICON};
use crate::application::widgets::icon_paint;

const TILE: f64 = 24.0;

#[derive(Debug)]
pub(crate) struct IconClicked;

pub(crate) struct IconWidget {
    icon: &'static str,
    label: &'static str,
    style: ControlStyle,
    /// The rail tab's icon rise and corner radius, when this is a rail tab.
    rail: Option<(f64, f64)>,
    icon_size: Option<f64>,
    tile_size: f64,
    focus_target: Option<Arc<Mutex<Option<WidgetId>>>>,
    size: Size,
    hovered: bool,
    pressed: bool,
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
            .filter(|_| self.style.is_selected())
            .map_or(0.0, |(_, radius)| {
                design::rail_tab_flare(radius.max(0.5).min(size.width.min(size.height) / 2.0))
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
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        // Paint in the pixel-aligned box, not the unrounded layout size, so neighboring
        // tabs keep whole-pixel edges and equal gaps.
        let rect = ctx.border_box();
        let size = rect.size();
        let face = self.style.face(self.hovered, self.pressed);
        let line = self.style.stroke();
        if let Some((_, radius)) = self.rail {
            // Open at the bottom, with outward curves joining the panel below.
            let r = radius.max(0.5).min(size.width.min(size.height) / 2.0);
            if self.style.is_selected() {
                // The rail leaves this shape open, so the panel's face shows through it;
                // a fill is painted only when the face has one.
                let (outline, fill) = rail_tab_face(rect, radius);
                if face.fill.components[3] > 0.0 {
                    painter.fill(&fill, face.fill).draw();
                }
                painter
                    .stroke(&outline, &Stroke::new(line), face.outline)
                    .draw();
            } else {
                // Kurbo insets grow a rectangle for positive values. Move the stroke
                // inward so inactive tops align with the active tab's top edge.
                let shape = rect.inset(-line / 2.0).to_rounded_rect(r);
                painter.fill(shape, face.fill).draw();
                painter
                    .stroke(shape, &Stroke::new(line), face.outline)
                    .draw();
            }
        } else {
            let shape = rect.to_rounded_rect(self.style.radius());
            painter.fill(shape, face.fill).draw();
            if face.outline.components[3] > 0.0 {
                painter
                    .stroke(
                        rect.inset(-line / 2.0).to_rounded_rect(self.style.radius()),
                        &Stroke::new(line),
                        face.outline,
                    )
                    .draw();
            }
        }
        if ctx.is_focus_target() {
            let ring = rect
                .inset(-line * 1.5)
                .to_rounded_rect((self.style.radius() - line).max(0.0));
            painter
                .stroke(ring, &Stroke::new(line.max(1.0)), self.style.focus_ring())
                .draw();
        }
        let Some(icon) = icons().get(self.icon) else {
            return;
        };
        let pad = size.width.min(size.height) * 0.10;
        let vb = icon.view_box;
        let scale = if self.rail.is_some() {
            self.icon_size.unwrap_or(RAIL_TAB_ICON) / vb.width().max(vb.height())
        } else if let Some(side) = self.icon_size {
            side.min(size.width).min(size.height) / vb.width().max(vb.height())
        } else {
            ((size.width - pad * 2.0) / vb.width()).min((size.height - pad * 2.0) / vb.height())
        };
        let dx = rect.x0 + (size.width - vb.width() * scale) / 2.0;
        let dy = rect.y0 + (size.height - vb.height() * scale) / 2.0
            - match self.rail {
                Some((rise, _)) if self.style.is_selected() => rise,
                _ => 0.0,
            };
        icon_paint::paint(
            painter,
            self.icon,
            Rect::new(dx, dy, dx + vb.width() * scale, dy + vb.height() * scale),
            face.ink,
        );
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
                ..
            }) => {
                ctx.capture_pointer();
                ctx.set_handled();
            }
            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                if ctx.is_active() && ctx.is_hovered() {
                    if let Some(target) = self.focus_target.as_ref().and_then(|target| {
                        *target.lock().unwrap_or_else(|error| error.into_inner())
                    }) {
                        ctx.set_focus(target);
                    }
                    ctx.submit_action::<IconClicked>(IconClicked);
                }
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if let TextEvent::Keyboard(event) = event
            && event.state.is_up()
            && (matches!(&event.key, Key::Character(c) if c == " ")
                || event.key == Key::Named(NamedKey::Enter))
        {
            ctx.submit_action::<IconClicked>(IconClicked);
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action == Action::Click {
            ctx.submit_action::<IconClicked>(IconClicked);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::HoveredChanged(_) | Update::ActiveChanged(_) => {
                self.hovered = ctx.is_hovered();
                self.pressed = ctx.is_active();
                ctx.request_render();
            }
            Update::FocusChanged(_) => ctx.request_render(),
            _ => {}
        }
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accessibility_role(&self) -> Role {
        if self.rail.is_some() {
            Role::Tab
        } else {
            Role::Button
        }
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.label);
        node.add_action(Action::Click);
        if self.rail.is_some() {
            node.set_selected(self.style.is_selected());
        } else if self.style.is_selectable() {
            node.set_toggled(if self.style.is_selected() {
                Toggled::True
            } else {
                Toggled::False
            });
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

/// The active rail tab's outline and filled area for its box `rect`.
///
/// The outline rises from outward flares at the bottom to rounded top corners, so the tab
/// opens onto the panel below. The fill closes the outline just below `rect`, covering the
/// rail's bottom keyline. Returns `(outline, fill)`.
pub(crate) fn rail_tab_face(rect: Rect, radius: f64) -> (BezPath, BezPath) {
    let r = radius.max(0.5).min(rect.width().min(rect.height()) / 2.0);
    // Quarter-circle control points match Kurbo's rounded rectangles.
    let k = r * 0.552_284_749_830_793_6;
    let flare = design::rail_tab_flare(r);
    let kf = flare * 0.552_284_749_830_793_6;
    let left = rect.x0 + 0.5;
    let right = rect.x1 - 0.5;
    let top = rect.y0 + 0.5;
    let bottom = rect.y1 - 0.5;
    let mut outline = BezPath::new();
    outline.move_to((left - flare, bottom));
    outline.curve_to(
        (left - flare + kf, bottom),
        (left, bottom - flare + kf),
        (left, bottom - flare),
    );
    outline.line_to((left, top + r));
    outline.curve_to((left, top + r - k), (left + r - k, top), (left + r, top));
    outline.line_to((right - r, top));
    outline.curve_to((right - r + k, top), (right, top + r - k), (right, top + r));
    outline.line_to((right, bottom - flare));
    outline.curve_to(
        (right, bottom - flare + kf),
        (right + flare - kf, bottom),
        (right + flare, bottom),
    );
    let mut fill = outline.clone();
    fill.line_to((right + flare, rect.y1 + 1.0));
    fill.line_to((left - flare, rect.y1 + 1.0));
    fill.close_path();
    (outline, fill)
}

pub(crate) struct IconView<F> {
    icon: &'static str,
    label: &'static str,
    style: ControlStyle,
    rail: Option<(f64, f64)>,
    icon_size: Option<f64>,
    tile_size: f64,
    focus_target: Option<Arc<Mutex<Option<WidgetId>>>>,
    on_click: F,
}

/// An icon control drawn in `style`, announced to assistive technology as `label`.
pub(crate) fn icon_button<State: 'static, F: Fn(&mut State) + 'static>(
    style: ControlStyle,
    icon: &'static str,
    label: &'static str,
    on_click: F,
) -> IconView<F> {
    IconView {
        icon,
        label,
        style,
        rail: None,
        icon_size: None,
        tile_size: TILE,
        focus_target: None,
        on_click,
    }
}

impl<F> IconView<F> {
    /// Set the icon's maximum ink extent in logical pixels, clamped to its tile.
    /// Rail tabs default to the rail's own icon-size token.
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

    /// Draw the control as a rail tab: the open tab rises by `icon_rise`
    /// and flares into the panel below at `radius`.
    pub(crate) fn rail_tab(mut self, icon_rise: f64, radius: f64) -> Self {
        self.rail = Some((icon_rise, radius));
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
            style: self.style,
            rail: self.rail,
            icon_size: self.icon_size,
            tile_size: self.tile_size,
            focus_target: self.focus_target.clone(),
            size: Size::ZERO,
            hovered: false,
            pressed: false,
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
            || self.style != prev.style
            || self.rail != prev.rail
            || self.icon_size != prev.icon_size
            || self.tile_size != prev.tile_size
        {
            el.widget.icon = self.icon;
            el.widget.style = self.style;
            el.widget.rail = self.rail;
            el.widget.icon_size = self.icon_size;
            el.widget.tile_size = self.tile_size;
            el.ctx.request_layout();
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
    use crate::application::design::ControlKind;
    use crate::application::design::theme::Palette;
    use crate::application::view::default_property_set;
    use masonry::core::NewWidget;
    use masonry::widgets::{Button, Flex, Label};
    use masonry_testing::TestHarness;

    fn tool(focus_target: Option<Arc<Mutex<Option<WidgetId>>>>) -> IconWidget {
        IconWidget {
            icon: "text",
            label: "Text tool",
            style: ControlStyle::new(&Palette::load("gray"), ControlKind::Tool),
            rail: None,
            icon_size: None,
            tile_size: TILE,
            focus_target,
            size: Size::ZERO,
            hovered: false,
            pressed: false,
        }
    }

    #[test]
    fn a_tool_control_can_focus_the_editor_on_click() {
        let editor = NewWidget::new(Button::new(Label::new("editor").prepare()));
        let editor_id = editor.id();
        let target = Arc::new(Mutex::new(Some(editor_id)));
        let tool = NewWidget::new(tool(Some(target)));
        let tool_id = tool.id();
        let row = Flex::row().with_fixed(tool).with_fixed(editor).prepare();
        let mut harness = TestHarness::create_with_size(default_property_set(), row, (160, 40));

        harness.mouse_click_on(tool_id, Some(PointerButton::Primary));

        assert_eq!(harness.focused_widget_id(), Some(editor_id));
    }

    #[test]
    fn an_icon_control_shows_hover_and_press() {
        let widget = NewWidget::new(tool(None));
        let id = widget.id();
        let mut harness = TestHarness::create_with_size(default_property_set(), widget, (24, 24));
        let rest = harness.render();
        harness.mouse_move_to(id);
        let hover = harness.render();
        harness.mouse_button_press(Some(PointerButton::Primary));
        let pressed = harness.render();
        assert!(
            harness.pop_action::<IconClicked>().is_none(),
            "fires on release"
        );
        assert_ne!(rest, hover);
        assert_ne!(hover, pressed);
        harness.mouse_button_release(Some(PointerButton::Primary));
        assert!(harness.pop_action::<IconClicked>().is_some());
    }
}

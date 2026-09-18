// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! An icon tile that paints either one of Runebender's toolbar icons
//! or a small geometry-only status mark, and reports clicks.

use std::sync::{Arc, Mutex};

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Widget, WidgetId,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, BezPath, Line, Size, Stroke};
use masonry::layout::{LenReq, Length};
use runebender::ui::theme::toolbar_icons;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::application::view::design::{RAIL_TAB_ICON, RAIL_TAB_RADIUS};

const TILE: f64 = 24.0;

/// Small geometry-only marks used by GPUI where no toolbar asset exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IconMark {
    /// A centered plus sign.
    Plus,
    /// A centered horizontal minus sign.
    Minus,
    /// Four outlined cells: the glyph grid view.
    Grid,
    /// Three horizontal rules: the glyph list view.
    List,
    /// The proof-strip visibility control.
    EyeOpen,
    /// The hidden proof-strip state.
    EyeClosed,
    /// A half-filled circle for reversing proof contrast.
    Invert,
    /// A split pane with its left dock open.
    SidebarOpen,
    /// A split pane with its left dock collapsed.
    SidebarClosed,
}

#[derive(Debug)]
pub(crate) struct IconClicked;

pub(crate) struct IconWidget {
    icon: &'static str,
    mark: Option<IconMark>,
    active: bool,
    fg: Color,
    fg_active: Color,
    active_bg: Color,
    hover_bg: Color,
    rail: Option<(Color, Color, f64)>,
    icon_size: Option<f64>,
    frame: Option<(Color, Color)>,
    tile_size: f64,
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
        if let Some((background, border)) = self.frame {
            let face = rect.inset(0.5);
            let background = if self.active {
                self.active_bg
            } else if self.hovered {
                self.hover_bg
            } else {
                background
            };
            painter.fill(face, background).draw();
            painter.stroke(face, &Stroke::new(1.0), border).draw();
        }
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

        if self.rail.is_none() && self.frame.is_none() && self.active {
            painter
                .fill(rect.to_rounded_rect(6.0), self.active_bg)
                .draw();
        } else if self.rail.is_none() && self.frame.is_none() && self.hovered {
            painter
                .fill(rect.to_rounded_rect(6.0), self.hover_bg)
                .draw();
        }
        let color = if self.active || (self.rail.is_some() && self.hovered) {
            self.fg_active
        } else {
            self.fg
        };
        if let Some(mark) = self.mark {
            paint_mark(painter, self.size, self.icon_size, color, mark);
            return;
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
        node.set_label(self.icon);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

/// Paint the same four-cell grid and three-rule list marks as GPUI's
/// `glyph_free_icon`, without relying on font glyph coverage.
fn paint_mark(
    painter: &mut Painter<'_>,
    size: Size,
    maximum_extent: Option<f64>,
    color: Color,
    mark: IconMark,
) {
    use masonry::kurbo::{Arc, Circle, Shape as _};

    let extent = maximum_extent
        .unwrap_or(f64::INFINITY)
        .min(size.width)
        .min(size.height);
    let origin = ((size.width - extent) / 2.0, (size.height - extent) / 2.0);
    let center = (size.width / 2.0, size.height / 2.0);
    match mark {
        IconMark::EyeOpen | IconMark::EyeClosed => {
            let rx = extent * 0.40;
            let ry = extent * 0.30;
            let mut eye = BezPath::new();
            eye.move_to((center.0 - rx, center.1));
            eye.quad_to((center.0, center.1 - ry * 2.2), (center.0 + rx, center.1));
            eye.quad_to((center.0, center.1 + ry * 2.2), (center.0 - rx, center.1));
            if mark == IconMark::EyeClosed {
                eye.move_to((center.0 - rx, center.1 + ry));
                eye.line_to((center.0 + rx, center.1 - ry));
            }
            painter.stroke(&eye, &Stroke::new(1.2), color).draw();
            if mark == IconMark::EyeOpen {
                painter.fill(Circle::new(center, ry * 0.62), color).draw();
            }
            return;
        }
        IconMark::Invert => {
            let radius = extent / 2.0 - 1.5;
            let circle = Circle::new(center, radius);
            painter.stroke(circle, &Stroke::new(1.2), color).draw();
            let half = Arc {
                center: center.into(),
                radii: (radius, radius).into(),
                start_angle: std::f64::consts::FRAC_PI_2,
                sweep_angle: std::f64::consts::PI,
                x_rotation: 0.0,
            }
            .to_path(0.1);
            painter.fill(&half, color).draw();
            return;
        }
        IconMark::SidebarOpen | IconMark::SidebarClosed => {
            // The shared footer frame supplies the outer rectangle. This mark
            // draws only the pane divider, keeping its stroke identical to the
            // plus, minus, grid, and list controls beside it.
            let split = snap_half(
                origin.0
                    + extent
                        * if mark == IconMark::SidebarOpen {
                            0.37
                        } else {
                            0.18
                        },
            );
            painter
                .stroke(
                    Line::new(
                        (split, snap_half(origin.1 + 2.0)),
                        (split, snap_half(origin.1 + extent - 2.0)),
                    ),
                    &Stroke::new(1.0),
                    color,
                )
                .draw();
            return;
        }
        IconMark::Plus | IconMark::Minus | IconMark::Grid | IconMark::List => {}
    }
    let path = mark_path(size, maximum_extent, mark);
    painter.stroke(&path, &Stroke::new(1.0), color).draw();
}

fn mark_path(size: Size, maximum_extent: Option<f64>, mark: IconMark) -> BezPath {
    let center = (size.width / 2.0, size.height / 2.0);
    let extent = maximum_extent
        .unwrap_or(f64::INFINITY)
        .min(size.width)
        .min(size.height);
    let radius = extent / 2.0 * 0.56;
    let mut path = BezPath::new();
    match mark {
        IconMark::Plus | IconMark::Minus => {
            path.move_to((snap_half(center.0 - radius), snap_half(center.1)));
            path.line_to((snap_half(center.0 + radius), snap_half(center.1)));
            if mark == IconMark::Plus {
                path.move_to((snap_half(center.0), snap_half(center.1 - radius)));
                path.line_to((snap_half(center.0), snap_half(center.1 + radius)));
            }
        }
        IconMark::Grid => {
            let gap = radius * 0.35;
            let side = radius - gap / 2.0;
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let (x0, y0) = (center.0 + sx * gap / 2.0, center.1 + sy * gap / 2.0);
                let (x1, y1) = (x0 + sx * side, y0 + sy * side);
                let (x0, y0, x1, y1) = (snap_half(x0), snap_half(y0), snap_half(x1), snap_half(y1));
                path.move_to((x0, y0));
                path.line_to((x1, y0));
                path.line_to((x1, y1));
                path.line_to((x0, y1));
                path.close_path();
            }
        }
        IconMark::List => {
            for dy in [-radius * 0.8, 0.0, radius * 0.8] {
                path.move_to((snap_half(center.0 - radius), snap_half(center.1 + dy)));
                path.line_to((snap_half(center.0 + radius), snap_half(center.1 + dy)));
            }
        }
        IconMark::EyeOpen
        | IconMark::EyeClosed
        | IconMark::Invert
        | IconMark::SidebarOpen
        | IconMark::SidebarClosed => unreachable!("painted before mark_path"),
    }
    path
}

/// Snap a status-mark coordinate to a half logical pixel so both 1× and 2×
/// strokes remain stable instead of landing on arbitrary subpixels.
fn snap_half(value: f64) -> f64 {
    (value * 2.0).round() / 2.0
}

pub(crate) struct IconView<F> {
    icon: &'static str,
    mark: Option<IconMark>,
    active: bool,
    fg: Color,
    fg_active: Color,
    active_bg: Color,
    hover_bg: Color,
    rail: Option<(Color, Color, f64)>,
    icon_size: Option<f64>,
    frame: Option<(Color, Color)>,
    tile_size: f64,
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
        mark: None,
        active,
        fg,
        fg_active,
        active_bg,
        hover_bg,
        rail: None,
        icon_size: None,
        frame: None,
        tile_size: TILE,
        focus_target: None,
        on_click,
    }
}

/// A button for one of GPUI's geometry-only marks.
pub(crate) fn mark_button<State: 'static, F: Fn(&mut State) + 'static>(
    label: &'static str,
    mark: IconMark,
    active: bool,
    fg: Color,
    fg_active: Color,
    active_bg: Color,
    hover_bg: Color,
    on_click: F,
) -> IconView<F> {
    IconView {
        icon: label,
        mark: Some(mark),
        active,
        fg,
        fg_active,
        active_bg,
        hover_bg,
        rail: None,
        icon_size: None,
        frame: None,
        tile_size: TILE,
        focus_target: None,
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

    /// Give the icon a square keylined control face.
    pub(crate) fn framed(mut self, background: Color, border: Color) -> Self {
        self.frame = Some((background, border));
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
            mark: self.mark,
            active: self.active,
            fg: self.fg,
            fg_active: self.fg_active,
            active_bg: self.active_bg,
            hover_bg: self.hover_bg,
            rail: self.rail,
            icon_size: self.icon_size,
            frame: self.frame,
            tile_size: self.tile_size,
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
        if self.active != prev.active
            || self.rail != prev.rail
            || self.fg != prev.fg
            || self.fg_active != prev.fg_active
            || self.active_bg != prev.active_bg
            || self.hover_bg != prev.hover_bg
            || self.icon_size != prev.icon_size
            || self.frame != prev.frame
            || self.tile_size != prev.tile_size
        {
            el.widget.active = self.active;
            el.widget.rail = self.rail;
            el.widget.icon_size = self.icon_size;
            el.widget.frame = self.frame;
            el.widget.tile_size = self.tile_size;
            el.widget.fg = self.fg;
            el.widget.fg_active = self.fg_active;
            el.widget.active_bg = self.active_bg;
            el.widget.hover_bg = self.hover_bg;
            el.ctx.request_render();
        }
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
    use masonry::kurbo::Shape as _;
    use masonry::theme::default_property_set;
    use masonry::widgets::{Button, Flex, Label};
    use masonry_testing::TestHarness;

    #[test]
    fn geometry_marks_are_centered_and_distinct() {
        let size = Size::new(20.0, 20.0);
        let grid = mark_path(size, None, IconMark::Grid);
        let list = mark_path(size, None, IconMark::List);
        let plus = mark_path(size, None, IconMark::Plus);
        let minus = mark_path(size, None, IconMark::Minus);

        assert_eq!(grid.elements().len(), 20);
        assert_eq!(list.elements().len(), 6);
        assert_eq!(plus.elements().len(), 4);
        assert_eq!(minus.elements().len(), 2);
        for bounds in [
            grid.bounding_box(),
            list.bounding_box(),
            plus.bounding_box(),
            minus.bounding_box(),
        ] {
            assert!((bounds.center().x - 10.0).abs() < 0.01);
            assert!((bounds.center().y - 10.0).abs() < 0.01);
            assert!(bounds.x0 > 0.0 && bounds.y0 > 0.0);
            assert!(bounds.x1 < size.width && bounds.y1 < size.height);
        }
    }

    #[test]
    fn a_tool_control_can_focus_the_editor_on_click() {
        let editor = NewWidget::new(Button::new(Label::new("editor").prepare()));
        let editor_id = editor.id();
        let target = Arc::new(Mutex::new(Some(editor_id)));
        let tool = NewWidget::new(IconWidget {
            icon: "text",
            mark: None,
            active: false,
            fg: Color::BLACK,
            fg_active: Color::BLACK,
            active_bg: Color::TRANSPARENT,
            hover_bg: Color::TRANSPARENT,
            rail: None,
            icon_size: None,
            frame: None,
            tile_size: TILE,
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

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Compact tool groups in the editor header, with icon-UFO dropdown marks.

use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, Layer, LayerType, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef,
    RegisterCtx, TextEvent, Widget, WidgetId,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size, Stroke, Vec2};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::application::view::design::{ControlSize, Space, TITLEBAR_HEIGHT};
use crate::application::view::theme::Palette;
use crate::application::widgets::icon_paint;
use crate::application::widgets::text_label::{self, Anchor};
use crate::application::workspace::{Tool, Workspace};

const ROW_HEIGHT: f64 = 26.0;
const MENU_WIDTH: f64 = 178.0;
const MENU_PAD: f64 = 4.0;
const MENU_SHADOW: f64 = Space::Sm.px();
const MENU_GAP: f64 = TITLEBAR_HEIGHT - ControlSize::Icon.px();
// The font's visible ink sits slightly below its centered line box.
const MENU_CONTENT_RISE: f64 = 1.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolGroup {
    Select,
    Shapes,
}

#[derive(Clone, Copy)]
struct Choice {
    label: &'static str,
    icon: &'static str,
    tool: Tool,
}

const SELECT_CHOICES: &[Choice] = &[
    Choice {
        label: "Select",
        icon: "select",
        tool: Tool::Select,
    },
    Choice {
        label: "Lasso",
        icon: "lasso",
        tool: Tool::Lasso,
    },
];
const SHAPE_CHOICES: &[Choice] = &[
    Choice {
        label: "Rectangle",
        icon: "shape-rectangle",
        tool: Tool::Rect,
    },
    Choice {
        label: "Ellipse",
        icon: "shape-ellipse",
        tool: Tool::Ellipse,
    },
    Choice {
        label: "Metaball",
        icon: "shape-metaball",
        tool: Tool::Metaball,
    },
];

impl ToolGroup {
    fn icon(self) -> &'static str {
        match self {
            Self::Select => "select-menu",
            Self::Shapes => "shapes-menu",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Select => "Selection tools",
            Self::Shapes => "Shape tools",
        }
    }

    fn choices(self) -> &'static [Choice] {
        match self {
            Self::Select => SELECT_CHOICES,
            Self::Shapes => SHAPE_CHOICES,
        }
    }

    pub(crate) fn contains(self, tool: Tool) -> bool {
        self.choices().iter().any(|choice| choice.tool == tool)
    }
}

pub(crate) struct ToolGroupView {
    group: ToolGroup,
    active: Tool,
    palette: Arc<Palette>,
}

pub(crate) fn tool_group(group: ToolGroup, active: Tool, palette: Arc<Palette>) -> ToolGroupView {
    ToolGroupView {
        group,
        active,
        palette,
    }
}

pub(crate) struct ToolGroupWidget {
    group: ToolGroup,
    active: Tool,
    palette: Arc<Palette>,
    open: Option<WidgetId>,
    hovered: bool,
    size: Size,
}

impl ToolGroupWidget {
    fn toggle_menu(&mut self, ctx: &mut EventCtx<'_>) {
        if let Some(id) = self.open.take() {
            ctx.remove_layer(id);
        } else {
            let menu = NewWidget::new(ToolMenu {
                creator: ctx.widget_id(),
                group: self.group,
                active: self.active,
                palette: self.palette.clone(),
                selected: self
                    .group
                    .choices()
                    .iter()
                    .position(|choice| choice.tool == self.active)
                    .unwrap_or(0),
                hovered: None,
                size: Size::ZERO,
            });
            let id = menu.id();
            ctx.create_layer(
                LayerType::Other,
                menu,
                ctx.to_window(Point::new(-MENU_SHADOW, self.size.height + MENU_GAP)),
            );
            self.open = Some(id);
            ctx.set_focus(id);
        }
        ctx.request_render();
    }
}

impl Widget for ToolGroupWidget {
    type Action = Tool;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross: Option<Length>,
    ) -> Length {
        Length::px(ControlSize::Icon.px())
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
        let ink = if self.group.contains(self.active) || self.hovered || self.open.is_some() {
            self.palette.header_ink
        } else {
            self.palette.header_ink.with_alpha(0.5)
        };
        let pad = self.size.width.min(self.size.height) * 0.10;
        icon_paint::paint(painter, self.group.icon(), rect.inset(pad), ink);
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
                self.toggle_menu(ctx);
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
        if let TextEvent::Keyboard(key) = event
            && key.state == KeyState::Down
        {
            match &key.key {
                Key::Named(NamedKey::Enter | NamedKey::ArrowDown) => {
                    self.toggle_menu(ctx);
                    ctx.set_handled();
                }
                Key::Character(c) if c == " " => {
                    self.toggle_menu(ctx);
                    ctx.set_handled();
                }
                _ => {}
            }
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
        node.set_label(self.group.label());
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

struct ToolMenu {
    creator: WidgetId,
    group: ToolGroup,
    active: Tool,
    palette: Arc<Palette>,
    selected: usize,
    hovered: Option<usize>,
    size: Size,
}

impl ToolMenu {
    fn row_at(&self, at: Point) -> Option<usize> {
        if at.x < MENU_SHADOW || at.x >= MENU_SHADOW + MENU_WIDTH || at.y < MENU_PAD {
            return None;
        }
        let mut bottom = MENU_PAD + ROW_HEIGHT;
        for index in 0..self.group.choices().len() {
            if at.y < bottom {
                return Some(index);
            }
            bottom += ROW_HEIGHT;
        }
        None
    }

    fn close(&self, ctx: &mut EventCtx<'_>, chosen: Option<Tool>) {
        let creator = self.creator;
        let popup = ctx.widget_id();
        ctx.set_focus(creator);
        ctx.mutate_later(creator, move |mut parent| {
            let mut parent = parent.downcast::<ToolGroupWidget>();
            parent.widget.open = None;
            parent.ctx.remove_layer(popup);
            if let Some(tool) = chosen {
                parent.ctx.submit_action::<Tool>(tool);
            }
            parent.ctx.request_render();
        });
    }
}

impl Widget for ToolMenu {
    type Action = ();

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
            Axis::Horizontal => Length::px(MENU_WIDTH + MENU_SHADOW),
            Axis::Vertical => Length::px(
                self.group.choices().len() as f64 * ROW_HEIGHT + MENU_PAD * 2.0 + MENU_SHADOW,
            ),
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
        let frame = Rect::new(
            MENU_SHADOW,
            0.0,
            MENU_SHADOW + MENU_WIDTH,
            self.size.height - MENU_SHADOW,
        );
        let shape =
            crate::application::view::design::rounded_rect_path(frame, self.palette.corner_radius);
        painter
            .fill(
                crate::application::view::design::rounded_rect_path(
                    frame + Vec2::new(-MENU_SHADOW, MENU_SHADOW),
                    self.palette.corner_radius,
                ),
                self.palette.cell_shadow().with_alpha(0.5),
            )
            .draw();
        painter.fill(&shape, self.palette.header).draw();
        painter
            .stroke(&shape, &Stroke::new(1.0), self.palette.outline)
            .draw();
        for (index, choice) in self.group.choices().iter().enumerate() {
            let top = MENU_PAD + index as f64 * ROW_HEIGHT;
            let selected = self.hovered == Some(index) || self.selected == index;
            if selected {
                painter
                    .fill(
                        crate::application::view::design::rounded_rect_path(
                            Rect::new(
                                MENU_SHADOW + MENU_PAD,
                                top,
                                MENU_SHADOW + MENU_WIDTH - MENU_PAD,
                                top + ROW_HEIGHT,
                            ),
                            self.palette.control_radius,
                        ),
                        self.palette.header_ink.with_alpha(0.18),
                    )
                    .draw();
            }
            let ink = self.palette.header_ink;
            icon_paint::paint(
                painter,
                choice.icon,
                Rect::new(
                    MENU_SHADOW + 8.0,
                    top + 4.0 - MENU_CONTENT_RISE,
                    MENU_SHADOW + 26.0,
                    top + 22.0 - MENU_CONTENT_RISE,
                ),
                ink,
            );
            text_label::draw(
                painter,
                Point::new(
                    MENU_SHADOW + 34.0,
                    top + ROW_HEIGHT / 2.0 - MENU_CONTENT_RISE,
                ),
                choice.label,
                13.0,
                ink,
                Anchor::Start,
            );
            if choice.tool == self.active {
                icon_paint::paint(
                    painter,
                    "menu-check",
                    Rect::new(
                        MENU_SHADOW + MENU_WIDTH - 21.0,
                        top + 5.0 - MENU_CONTENT_RISE,
                        MENU_SHADOW + MENU_WIDTH - 7.0,
                        top + 19.0 - MENU_CONTENT_RISE,
                    ),
                    ink,
                );
            }
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
                if self.hovered != hovered {
                    self.hovered = hovered;
                    if let Some(index) = hovered {
                        self.selected = index;
                    }
                    ctx.request_render();
                }
                ctx.set_handled();
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                if let Some(index) = self.row_at(ctx.local_position(state.position)) {
                    self.close(ctx, Some(self.group.choices()[index].tool));
                    ctx.set_handled();
                } else {
                    self.close(ctx, None);
                    ctx.set_handled();
                }
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
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        match key.key {
            Key::Named(NamedKey::ArrowDown) => {
                self.selected = (self.selected + 1) % self.group.choices().len();
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.selected =
                    (self.selected + self.group.choices().len() - 1) % self.group.choices().len();
            }
            Key::Named(NamedKey::Enter) => {
                self.close(ctx, Some(self.group.choices()[self.selected].tool));
            }
            Key::Character(ref value) if value == " " => {
                self.close(ctx, Some(self.group.choices()[self.selected].tool));
            }
            Key::Named(NamedKey::Escape) => self.close(ctx, None),
            _ => return,
        }
        ctx.request_render();
        ctx.set_handled();
    }

    fn accessibility_role(&self) -> Role {
        Role::Menu
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.group.label());
    }

    fn accepts_focus(&self) -> bool {
        true
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn as_layer(&mut self) -> Option<&mut dyn Layer> {
        Some(self)
    }
}

impl Layer for ToolMenu {
    fn capture_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(PointerButtonEvent { state, .. }) = event
            && !ctx
                .border_box()
                .contains(ctx.local_position(state.position))
        {
            self.close(ctx, None);
            ctx.set_handled();
        }
    }
}

impl ViewMarker for ToolGroupView {}

impl View<Workspace, (), ViewCtx> for ToolGroupView {
    type Element = Pod<ToolGroupWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut Workspace) -> (Self::Element, Self::ViewState) {
        let widget = ToolGroupWidget {
            group: self.group,
            active: self.active,
            palette: self.palette.clone(),
            open: None,
            hovered: false,
            size: Size::ZERO,
        };
        (ctx.with_action_widget(|ctx| ctx.create_pod(widget)), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut el: Mut<'_, Self::Element>,
        _: &mut Workspace,
    ) {
        if self.group != prev.group
            || self.active != prev.active
            || !Arc::ptr_eq(&self.palette, &prev.palette)
        {
            el.widget.group = self.group;
            el.widget.active = self.active;
            el.widget.palette = self.palette.clone();
            el.ctx.request_render();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, mut el: Mut<'_, Self::Element>) {
        if let Some(id) = el.widget.open.take() {
            el.ctx.remove_layer(id);
        }
    }

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _el: Mut<'_, Self::Element>,
        state: &mut Workspace,
    ) -> MessageResult<()> {
        match message.take_message::<Tool>() {
            Some(tool) => {
                state.select_tool(*tool);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::theme::default_property_set;
    use masonry::widgets::Flex;
    use masonry_testing::TestHarness;

    #[test]
    fn grouped_select_menu_returns_the_chosen_tool() {
        let widget = ToolGroupWidget {
            group: ToolGroup::Select,
            active: Tool::Select,
            palette: Arc::new(Palette::load("gray")),
            open: None,
            hovered: false,
            size: Size::ZERO,
        };
        let widget = NewWidget::new(widget);
        let id = widget.id();
        let root = Flex::column()
            .with_fixed(widget)
            .with_spacer(100.0)
            .prepare();
        let mut harness = TestHarness::create_with_size(default_property_set(), root, (200, 120));
        let origin = harness
            .get_widget_with_id(id)
            .ctx()
            .to_window(Point::ORIGIN);
        harness.mouse_click_on(id, Some(PointerButton::Primary));
        assert!(
            harness
                .get_widget_with_id(id)
                .downcast::<ToolGroupWidget>()
                .unwrap()
                .inner()
                .open
                .is_some()
        );
        harness.mouse_move(
            origin
                + (
                    80.0,
                    ControlSize::Icon.px() + MENU_GAP + MENU_PAD + ROW_HEIGHT * 1.5,
                ),
        );
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        let (chosen, _) = harness
            .pop_action::<Tool>()
            .expect("menu choice is dispatched");
        assert_eq!(chosen, Tool::Lasso);
        assert!(
            harness
                .get_widget_with_id(id)
                .downcast::<ToolGroupWidget>()
                .unwrap()
                .inner()
                .open
                .is_none()
        );
    }
}

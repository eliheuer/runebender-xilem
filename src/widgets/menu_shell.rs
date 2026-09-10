// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Accessible in-window application menus for platforms without a native bar.
//!
//! The shell is a Masonry widget because opening a popup layer is not exposed by
//! Xilem's view API. It wraps the application, so unhandled shortcuts and F10
//! reach it regardless of which descendant owns focus. The popup reports its
//! result to the shell with the same `mutate_later` pattern as Masonry's selector.

use std::sync::Arc;

use masonry::accesskit::{Node, Role};
#[cfg(not(target_os = "macos"))]
use masonry::core::WidgetMut;
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, Layer, LayerType, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate, PropertiesMut, PropertiesRef,
    RegisterCtx, TextEvent, Widget, WidgetId, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size, Stroke};
use masonry::layout::{LenReq, Length};
#[cfg(not(target_os = "macos"))]
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
#[cfg(not(target_os = "macos"))]
use xilem::{Pod, ViewCtx, WidgetView};

#[cfg(not(target_os = "macos"))]
use crate::Workspace;
use crate::actions::{ACTIONS, MENUS};
use crate::view::theme::Palette;
use crate::widgets::shortcuts::{self, AppAction};
use crate::widgets::text_label::{self, Anchor};

const BAR_HEIGHT: f64 = 24.0;
const TITLE_PAD: f64 = 10.0;
const ROW_HEIGHT: f64 = 24.0;
const POPUP_PAD: f64 = 4.0;
const POPUP_WIDTH: f64 = 220.0;

fn title_width(title: &str) -> f64 {
    title.chars().count() as f64 * 7.25 + TITLE_PAD * 2.0
}

fn title_rect(index: usize) -> Rect {
    let x0 = MENUS[..index].iter().map(|title| title_width(title)).sum();
    Rect::new(x0, 0.0, x0 + title_width(MENUS[index]), BAR_HEIGHT)
}

fn menu_at(point: Point) -> Option<usize> {
    MENUS
        .iter()
        .enumerate()
        .find_map(|(index, _)| title_rect(index).contains(point).then_some(index))
}

fn entries(menu: usize) -> impl Iterator<Item = &'static crate::actions::Entry> {
    ACTIONS
        .iter()
        .filter(move |entry| entry.menu == MENUS[menu])
}

/// The application content plus a menu bar and window-level shortcut scope.
pub(crate) struct MenuShell {
    inner: WidgetPod<dyn Widget>,
    palette: Arc<Palette>,
    open: Option<WidgetId>,
    active_menu: Option<usize>,
    selected: usize,
    focus_before: Option<WidgetId>,
    size: Size,
}

impl MenuShell {
    fn new(child: NewWidget<impl Widget + ?Sized>, palette: Arc<Palette>) -> Self {
        Self {
            inner: child.erased().to_pod(),
            palette,
            open: None,
            active_menu: None,
            selected: 0,
            focus_before: None,
            size: Size::ZERO,
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn child_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.inner)
    }

    fn show_menu(&mut self, ctx: &mut EventCtx<'_>, index: usize, remember_focus: bool) {
        if remember_focus {
            self.focus_before = ctx.focus_target_id().filter(|id| *id != ctx.widget_id());
        }
        if let Some(id) = self.open.take() {
            ctx.remove_layer(id);
        }
        let popup = NewWidget::new(MenuPopup::new(
            ctx.widget_id(),
            index,
            self.palette.clone(),
            self.focus_before,
        ));
        let id = popup.id();
        let at = ctx.to_window(Point::new(title_rect(index).x0, BAR_HEIGHT));
        ctx.create_layer(LayerType::Other, popup, at);
        ctx.request_focus();
        self.open = Some(id);
        self.active_menu = Some(index);
        self.selected = 0;
        ctx.request_render();
    }

    fn close(&mut self, ctx: &mut EventCtx<'_>, restore_focus: Option<WidgetId>) {
        if let Some(id) = self.open.take() {
            ctx.remove_layer(id);
        }
        self.active_menu = None;
        if let Some(id) = restore_focus.or(self.focus_before.take()) {
            ctx.set_focus(id);
        }
        ctx.request_render();
    }
}

impl Widget for MenuShell {
    type Action = AppAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        _len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        let child = ctx.redirect_measurement(&mut self.inner, axis, cross_length);
        match axis {
            Axis::Horizontal => child,
            Axis::Vertical => child.saturating_add(Length::px(BAR_HEIGHT)),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        let child_size = Size::new(size.width, (size.height - BAR_HEIGHT).max(0.0));
        ctx.run_layout(&mut self.inner, child_size);
        ctx.place_child(&mut self.inner, Point::new(0.0, BAR_HEIGHT));
        ctx.derive_baselines(&self.inner);
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let pal = &self.palette;
        painter
            .fill(Rect::new(0.0, 0.0, self.size.width, BAR_HEIGHT), pal.header)
            .draw();
        painter
            .stroke(
                Rect::new(0.0, BAR_HEIGHT - 1.0, self.size.width, BAR_HEIGHT),
                &Stroke::new(1.0),
                pal.role("gridBorder"),
            )
            .draw();
        for (index, title) in MENUS.iter().enumerate() {
            let rect = title_rect(index);
            if self.active_menu == Some(index) {
                painter.fill(rect, pal.role("gridSelected")).draw();
            }
            let ink = if self.active_menu == Some(index) {
                pal.role("selectedInk")
            } else {
                pal.header_ink
            };
            text_label::draw(
                painter,
                Point::new(rect.x0 + TITLE_PAD, BAR_HEIGHT / 2.0 + 4.0),
                title,
                13.0,
                ink,
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
        let point = match event {
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                ctx.local_position(current.position)
            }
            PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                ctx.local_position(state.position)
            }
            _ => return,
        };
        let Some(index) = menu_at(point) else { return };
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                ..
            }) => {
                if self.active_menu == Some(index) {
                    self.close(ctx, None);
                } else {
                    self.show_menu(ctx, index, true);
                }
                ctx.set_handled();
            }
            PointerEvent::Move(..) if self.open.is_some() && self.active_menu != Some(index) => {
                self.show_menu(ctx, index, false);
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
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        if matches!(key.key, Key::Named(NamedKey::F10 | NamedKey::Alt)) {
            self.show_menu(ctx, 0, true);
            ctx.set_handled();
            return;
        }
        if let Some(menu) = self.active_menu {
            let count = entries(menu).count();
            match key.key {
                Key::Named(NamedKey::ArrowDown) => self.selected = (self.selected + 1) % count,
                Key::Named(NamedKey::ArrowUp) => {
                    self.selected = (self.selected + count - 1) % count;
                }
                Key::Named(NamedKey::Home) => self.selected = 0,
                Key::Named(NamedKey::End) => self.selected = count - 1,
                Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight) => {
                    let backwards = matches!(key.key, Key::Named(NamedKey::ArrowLeft));
                    let next = if backwards {
                        (menu + MENUS.len() - 1) % MENUS.len()
                    } else {
                        (menu + 1) % MENUS.len()
                    };
                    self.show_menu(ctx, next, false);
                    ctx.set_handled();
                    return;
                }
                Key::Named(NamedKey::Enter) => {
                    if let Some(action) = entries(menu).nth(self.selected).map(|entry| entry.action)
                    {
                        self.close(ctx, None);
                        ctx.submit_action::<AppAction>(action);
                    }
                    ctx.set_handled();
                    return;
                }
                Key::Character(ref c) if c.as_str() == " " => {
                    if let Some(action) = entries(menu).nth(self.selected).map(|entry| entry.action)
                    {
                        self.close(ctx, None);
                        ctx.submit_action::<AppAction>(action);
                    }
                    ctx.set_handled();
                    return;
                }
                Key::Named(NamedKey::Escape) => {
                    self.close(ctx, None);
                    ctx.set_handled();
                    return;
                }
                _ => return,
            }
            if let Some(id) = self.open {
                let selected = self.selected;
                ctx.mutate_later(id, move |mut popup| {
                    let mut popup = popup.downcast::<MenuPopup>();
                    popup.widget.selected = selected;
                    popup.ctx.request_render();
                });
            }
            ctx.set_handled();
            ctx.request_render();
            return;
        }
        let cmd = key.modifiers.meta() || key.modifiers.ctrl();
        if let Some(action) = shortcuts::keymap(&key.key, cmd) {
            ctx.submit_action::<AppAction>(action);
            ctx.set_handled();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::MenuBar
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.inner.id()])
    }

    fn accepts_focus(&self) -> bool {
        true
    }
}

struct MenuPopup {
    creator: WidgetId,
    menu: usize,
    selected: usize,
    palette: Arc<Palette>,
    focus_before: Option<WidgetId>,
    size: Size,
}

impl MenuPopup {
    fn new(
        creator: WidgetId,
        menu: usize,
        palette: Arc<Palette>,
        focus_before: Option<WidgetId>,
    ) -> Self {
        Self {
            creator,
            menu,
            selected: 0,
            palette,
            focus_before,
            size: Size::ZERO,
        }
    }

    fn row_at(&self, point: Point) -> Option<usize> {
        if !(0.0..=POPUP_WIDTH).contains(&point.x) {
            return None;
        }
        let row = ((point.y - POPUP_PAD) / ROW_HEIGHT).floor() as isize;
        (row >= 0 && (row as usize) < entries(self.menu).count()).then_some(row as usize)
    }

    fn choose(&self, ctx: &mut EventCtx<'_>) {
        let Some(action) = entries(self.menu)
            .nth(self.selected)
            .map(|entry| entry.action)
        else {
            return;
        };
        let creator = self.creator;
        let popup = ctx.widget_id();
        let focus = self.focus_before;
        if let Some(id) = focus {
            ctx.set_focus(id);
        }
        ctx.mutate_later(creator, move |mut shell| {
            let mut shell = shell.downcast::<MenuShell>();
            shell.widget.open = None;
            shell.widget.active_menu = None;
            shell.widget.focus_before = None;
            shell.ctx.remove_layer(popup);
            shell.ctx.submit_action::<AppAction>(action);
            shell.ctx.request_render();
        });
    }

    fn dismiss(&self, ctx: &mut EventCtx<'_>) {
        let creator = self.creator;
        let popup = ctx.widget_id();
        let focus = self.focus_before;
        if let Some(id) = focus {
            ctx.set_focus(id);
        }
        ctx.mutate_later(creator, move |mut shell| {
            let mut shell = shell.downcast::<MenuShell>();
            shell.widget.open = None;
            shell.widget.active_menu = None;
            shell.widget.focus_before = None;
            shell.ctx.remove_layer(popup);
            shell.ctx.request_render();
        });
    }
}

impl Widget for MenuPopup {
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
            Axis::Horizontal => Length::px(POPUP_WIDTH),
            Axis::Vertical => {
                Length::px(entries(self.menu).count() as f64 * ROW_HEIGHT + POPUP_PAD * 2.0)
            }
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
        painter.fill(self.size.to_rect(), pal.panel).draw();
        painter
            .stroke(
                self.size.to_rect(),
                &Stroke::new(1.0),
                pal.role("gridBorder"),
            )
            .draw();
        for (index, entry) in entries(self.menu).enumerate() {
            let top = POPUP_PAD + index as f64 * ROW_HEIGHT;
            if self.selected == index {
                painter
                    .fill(
                        Rect::new(2.0, top, POPUP_WIDTH - 2.0, top + ROW_HEIGHT),
                        pal.role("gridSelected"),
                    )
                    .draw();
            }
            text_label::draw(
                painter,
                Point::new(10.0, top + ROW_HEIGHT / 2.0 + 4.0),
                entry.title,
                13.0,
                if self.selected == index {
                    pal.role("selectedInk")
                } else {
                    pal.text
                },
                Anchor::Start,
            );
            if let Some(accelerator) = entry.accelerator {
                text_label::draw(
                    painter,
                    Point::new(POPUP_WIDTH - 10.0, top + ROW_HEIGHT / 2.0 + 4.0),
                    accelerator,
                    13.0,
                    if self.selected == index {
                        pal.role("selectedInk")
                    } else {
                        pal.role("textMuted")
                    },
                    Anchor::End,
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
                if let Some(row) = self.row_at(ctx.local_position(current.position)) {
                    if row != self.selected {
                        self.selected = row;
                        ctx.request_render();
                    }
                }
                ctx.set_handled();
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                if let Some(row) = self.row_at(ctx.local_position(state.position)) {
                    self.selected = row;
                    self.choose(ctx);
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
        let count = entries(self.menu).count();
        match key.key {
            Key::Named(NamedKey::ArrowDown) => self.selected = (self.selected + 1) % count,
            Key::Named(NamedKey::ArrowUp) => self.selected = (self.selected + count - 1) % count,
            Key::Named(NamedKey::Home) => self.selected = 0,
            Key::Named(NamedKey::End) => self.selected = count - 1,
            Key::Named(NamedKey::Enter) => self.choose(ctx),
            Key::Character(ref c) if c.as_str() == " " => self.choose(ctx),
            Key::Named(NamedKey::Escape) => self.dismiss(ctx),
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight) => {
                let backwards = matches!(key.key, Key::Named(NamedKey::ArrowLeft));
                self.menu = if backwards {
                    (self.menu + MENUS.len() - 1) % MENUS.len()
                } else {
                    (self.menu + 1) % MENUS.len()
                };
                self.selected = 0;
                let menu = self.menu;
                let creator = self.creator;
                ctx.mutate_later(creator, move |mut shell| {
                    let mut shell = shell.downcast::<MenuShell>();
                    shell.widget.active_menu = Some(menu);
                    shell.ctx.request_render();
                });
                ctx.request_layout();
            }
            _ => return,
        }
        ctx.set_handled();
        ctx.request_render();
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
    fn accepts_focus(&self) -> bool {
        true
    }
    fn as_layer(&mut self) -> Option<&mut dyn Layer> {
        Some(self)
    }
}

impl Layer for MenuPopup {
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
            self.dismiss(ctx);
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) struct MenuShellView<V> {
    inner: V,
    palette: Arc<Palette>,
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn menu_shell<V: WidgetView<Workspace>>(
    inner: V,
    palette: Arc<Palette>,
) -> MenuShellView<V> {
    MenuShellView { inner, palette }
}

#[cfg(not(target_os = "macos"))]
impl<V> ViewMarker for MenuShellView<V> {}
#[cfg(not(target_os = "macos"))]
impl<V> View<Workspace, (), ViewCtx> for MenuShellView<V>
where
    V: WidgetView<Workspace>,
{
    type Element = Pod<MenuShell>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, app: &mut Workspace) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.inner.build(ctx, app);
        let pod = ctx.with_action_widget(|ctx| {
            ctx.create_pod(MenuShell::new(child.new_widget, self.palette.clone()))
        });
        (pod, child_state)
    }
    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut Workspace,
    ) {
        element.widget.palette = self.palette.clone();
        let mut child = MenuShell::child_mut(&mut element);
        self.inner
            .rebuild(&prev.inner, view_state, ctx, child.downcast(), app);
    }
    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let mut child = MenuShell::child_mut(&mut element);
        self.inner.teardown(view_state, ctx, child.downcast());
    }
    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut Workspace,
    ) -> MessageResult<()> {
        if message.remaining_path().is_empty() {
            return match message.take_message::<AppAction>() {
                Some(action) => {
                    app.dispatch(*action);
                    MessageResult::Action(())
                }
                None => MessageResult::Stale,
            };
        }
        let mut child = MenuShell::child_mut(&mut element);
        self.inner
            .message(view_state, message, child.downcast(), app)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::core::keyboard::{Code, KeyboardEvent, Modifiers};
    use masonry::properties::Dimensions;
    use masonry::theme::default_property_set;
    use masonry::widgets::{Button, Label};
    use masonry_testing::TestHarness;

    fn key(key: Key) -> TextEvent {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key,
            code: Code::Unidentified,
            modifiers: Modifiers::empty(),
            ..KeyboardEvent::default()
        })
    }

    fn harness() -> (TestHarness<MenuShell>, WidgetId) {
        let button = NewWidget::new(Button::new(Label::new("editor").prepare()));
        let button_id = button.id();
        let shell = MenuShell::new(button, Arc::new(Palette::load("gray")))
            .prepare()
            .with_props(Dimensions::MAX);
        (
            TestHarness::create_with_size(default_property_set(), shell, (800, 600)),
            button_id,
        )
    }

    #[test]
    fn f10_opens_and_escape_restores_focus() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));

        harness.process_text_event(key(Key::Named(NamedKey::F10)));
        assert_ne!(harness.focused_widget_id(), Some(button_id));

        harness.process_text_event(key(Key::Named(NamedKey::Escape)));
        assert_eq!(harness.focused_widget_id(), Some(button_id));
        assert!(harness.pop_action::<AppAction>().is_none());
    }

    #[test]
    fn keyboard_navigation_dispatches_one_shared_table_action() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));

        harness.process_text_event(key(Key::Named(NamedKey::F10)));
        harness.process_text_event(key(Key::Named(NamedKey::ArrowDown)));
        harness.process_text_event(key(Key::Named(NamedKey::Enter)));

        let action = harness.pop_action::<AppAction>();
        assert_eq!(action.map(|(action, _)| action), Some(AppAction::Save));
        assert_eq!(harness.focused_widget_id(), Some(button_id));
        assert!(harness.pop_action::<AppAction>().is_none());
    }

    #[test]
    fn left_and_right_wrap_top_level_menus() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));

        harness.process_text_event(key(Key::Named(NamedKey::F10)));
        harness.process_text_event(key(Key::Named(NamedKey::ArrowLeft)));
        let active = harness.edit_root_widget(|root| root.widget.active_menu);
        assert_eq!(active, Some(MENUS.len() - 1));

        harness.process_text_event(key(Key::Named(NamedKey::ArrowRight)));
        let active = harness.edit_root_widget(|root| root.widget.active_menu);
        assert_eq!(active, Some(0));
    }
}

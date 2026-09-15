// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Accessible in-window application menus for platforms without a native bar.
//!
//! The shell is a Masonry widget because opening a popup layer is not exposed by
//! Xilem's view API. It wraps the application, so unhandled shortcuts and F10
//! reach it regardless of which descendant owns focus. The popup reports its
//! result to the shell with the same `mutate_later` pattern as Masonry's selector.

use std::sync::Arc;

use masonry::accesskit::{Node, Role, Toggled};
use masonry::core::WidgetMut;
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, EventCtx, Layer, LayerType, LayoutCtx, MeasureCtx,
    NewWidget, PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget, WidgetId,
    WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Line, Point, Rect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

use crate::AppState;
use crate::actions::{ACTIONS, MENUS};
use crate::view::theme::Palette;
use crate::widgets::shortcuts::AppAction;
use crate::widgets::text_label::{self, Anchor};
use runebender_core::outline::glyph_paths::round_units;

#[path = "menu_header.rs"]
mod header;

const BAR_HEIGHT: f64 = crate::view::design::TITLEBAR_HEIGHT;

/// Platforms without an OS menu share one application header row.
pub(crate) fn in_window() -> bool {
    !cfg!(target_os = "macos") || std::env::var("RUNEBENDER_IN_WINDOW_MENU").is_ok()
}
const TITLE_PAD: f64 = 10.0;
const ROW_HEIGHT: f64 = 24.0;
const POPUP_PAD: f64 = 4.0;
const POPUP_WIDTH: f64 = 220.0;

fn title_width(title: &str) -> f64 {
    title.chars().count() as f64 * 7.25 + TITLE_PAD * 2.0
}

// Disabled commands remain legible, but never receive an active highlight.
fn row_ink(pal: &Palette, enabled: bool, selected: bool) -> xilem::Color {
    if !enabled {
        pal.text_muted
    } else if selected {
        pal.selected_ink()
    } else {
        pal.text
    }
}

fn shortcut_label(accelerator: &str) -> String {
    accelerator.replace("CmdOrCtrl", "Ctrl")
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

fn indexed_entries(menu: usize) -> impl Iterator<Item = (usize, &'static crate::actions::Entry)> {
    ACTIONS
        .iter()
        .enumerate()
        .filter(move |(_, entry)| entry.menu == MENUS[menu])
}

#[derive(Clone, Copy)]
enum MenuRow {
    Action(usize),
    Submenu(&'static str),
}

fn row_label(row: MenuRow) -> &'static str {
    match row {
        MenuRow::Action(index) => ACTIONS[index].title,
        MenuRow::Submenu(name) => name,
    }
}

fn row_state(row: MenuRow, states: &[EntryState]) -> EntryState {
    match row {
        MenuRow::Action(index) => states[index],
        MenuRow::Submenu(_) => EntryState {
            enabled: true,
            checked: None,
        },
    }
}

fn row_separator(row: MenuRow, menu: usize) -> bool {
    match row {
        MenuRow::Action(index) => ACTIONS[index].separator_before(),
        MenuRow::Submenu(name) => indexed_entries(menu)
            .find(|(_, entry)| entry.submenu() == Some(name))
            .is_some_and(|(_, entry)| entry.separator_before()),
    }
}

fn rows(menu: usize, submenu: Option<&'static str>) -> Vec<MenuRow> {
    if let Some(submenu) = submenu {
        return indexed_entries(menu)
            .filter(|(_, entry)| entry.submenu() == Some(submenu))
            .map(|(index, _)| MenuRow::Action(index))
            .collect();
    }
    let mut rows = Vec::new();
    let mut last_submenu = None;
    for (index, entry) in indexed_entries(menu) {
        match entry.submenu() {
            Some(name) if last_submenu != Some(name) => {
                rows.push(MenuRow::Submenu(name));
                last_submenu = Some(name);
            }
            Some(_) => {}
            None => rows.push(MenuRow::Action(index)),
        }
    }
    rows
}

#[derive(Clone, Copy)]
struct EntryState {
    enabled: bool,
    checked: Option<bool>,
}

/// The application content plus a menu bar and window-level shortcut scope.
pub(crate) struct MenuShell {
    inner: WidgetPod<dyn Widget>,
    integrated_header: bool,
    accessible_titles: Vec<WidgetPod<AccessibleMenuTitle>>,
    palette: Arc<Palette>,
    states: Arc<Vec<EntryState>>,
    open: Option<WidgetId>,
    active_menu: Option<usize>,
    active_submenu: Option<&'static str>,
    selected: usize,
    focus_before: Option<WidgetId>,
    initial_menu: Option<usize>,
    size: Size,
}

impl MenuShell {
    fn new(
        child: NewWidget<impl Widget + ?Sized>,
        palette: Arc<Palette>,
        states: Arc<Vec<EntryState>>,
    ) -> Self {
        Self {
            inner: child.erased().to_pod(),
            integrated_header: false,
            accessible_titles: MENUS
                .iter()
                .map(|label| NewWidget::new(AccessibleMenuTitle { label }).to_pod())
                .collect(),
            palette,
            states,
            open: None,
            active_menu: None,
            active_submenu: None,
            selected: 0,
            focus_before: None,
            initial_menu: std::env::var("RUNEBENDER_MENU_OPEN")
                .ok()
                .and_then(|title| MENUS.iter().position(|menu| *menu == title)),
            size: Size::ZERO,
        }
    }

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
        let popup = new_popup(
            ctx.widget_id(),
            index,
            None,
            self.palette.clone(),
            self.states.clone(),
            self.focus_before,
        );
        let id = popup.id();
        let at = ctx.to_window(Point::new(title_rect(index).x0, BAR_HEIGHT));
        ctx.create_layer(LayerType::Other, popup, at);
        ctx.request_focus();
        self.open = Some(id);
        self.active_menu = Some(index);
        self.active_submenu = None;
        self.selected = 0;
        ctx.request_render();
    }

    fn show_submenu(&mut self, ctx: &mut EventCtx<'_>, menu: usize, submenu: &'static str) {
        if let Some(id) = self.open.take() {
            ctx.remove_layer(id);
        }
        let popup = new_popup(
            ctx.widget_id(),
            menu,
            Some(submenu),
            self.palette.clone(),
            self.states.clone(),
            self.focus_before,
        );
        let id = popup.id();
        let at = ctx.to_window(Point::new(title_rect(menu).x0 + POPUP_WIDTH, BAR_HEIGHT));
        ctx.create_layer(LayerType::Other, popup, at);
        self.open = Some(id);
        self.active_menu = Some(menu);
        self.active_submenu = Some(submenu);
        self.selected = 0;
        ctx.request_render();
    }

    fn close(&mut self, ctx: &mut EventCtx<'_>, restore_focus: Option<WidgetId>) {
        if let Some(id) = self.open.take() {
            ctx.remove_layer(id);
        }
        self.active_menu = None;
        self.active_submenu = None;
        if let Some(id) = restore_focus.or(self.focus_before.take()) {
            ctx.set_focus(id);
        }
        ctx.request_render();
    }

    fn activate_selected(&mut self, ctx: &mut EventCtx<'_>, menu: usize) {
        let Some(row) = rows(menu, self.active_submenu).get(self.selected).copied() else {
            return;
        };
        match row {
            MenuRow::Action(index) if self.states[index].enabled => {
                let action = ACTIONS[index].action;
                self.close(ctx, None);
                ctx.submit_action::<AppAction>(action);
            }
            MenuRow::Submenu(name) => self.show_submenu(ctx, menu, name),
            MenuRow::Action(_) => {}
        }
    }
}

impl Widget for MenuShell {
    type Action = AppAction;

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.inner);
        for title in &mut self.accessible_titles {
            ctx.register_child(title);
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded)
            && let Some(index) = self.initial_menu.take()
        {
            let popup = new_popup(
                ctx.widget_id(),
                index,
                None,
                self.palette.clone(),
                self.states.clone(),
                None,
            );
            let id = popup.id();
            ctx.create_layer(
                LayerType::Other,
                popup,
                Point::new(title_rect(index).x0, BAR_HEIGHT),
            );
            self.open = Some(id);
            self.active_menu = Some(index);
            self.active_submenu = None;
        }
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
            Axis::Vertical => child.saturating_add(Length::px(if self.integrated_header {
                0.0
            } else {
                BAR_HEIGHT
            })),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        let inset = if self.integrated_header {
            0.0
        } else {
            BAR_HEIGHT
        };
        let child_size = Size::new(size.width, (size.height - inset).max(0.0));
        ctx.run_layout(&mut self.inner, child_size);
        ctx.place_child(&mut self.inner, Point::new(0.0, inset));
        for (index, title) in self.accessible_titles.iter_mut().enumerate() {
            let rect = title_rect(index);
            ctx.run_layout(title, rect.size());
            ctx.place_child(title, rect.origin());
        }
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
            .fill(
                Rect::new(0.0, BAR_HEIGHT - 1.0, self.size.width, BAR_HEIGHT),
                pal.outline,
            )
            .draw();
        for (index, title) in MENUS.iter().enumerate() {
            let rect = title_rect(index);
            if self.active_menu == Some(index) {
                painter.fill(rect, pal.selected_bg()).draw();
            }
            let ink = if self.active_menu == Some(index) {
                pal.selected_ink()
            } else {
                pal.header_ink
            };
            text_label::draw(
                painter,
                Point::new(rect.x0 + TITLE_PAD, BAR_HEIGHT / 2.0),
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
        let Some(index) = menu_at(point) else {
            return;
        };
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
            let menu_rows = rows(menu, self.active_submenu);
            let count = menu_rows.len();
            match key.key {
                Key::Named(NamedKey::ArrowDown) => self.selected = (self.selected + 1) % count,
                Key::Named(NamedKey::ArrowUp) => {
                    self.selected = (self.selected + count - 1) % count;
                }
                Key::Named(NamedKey::Home) => self.selected = 0,
                Key::Named(NamedKey::End) => self.selected = count - 1,
                Key::Named(NamedKey::ArrowLeft) if self.active_submenu.is_some() => {
                    self.show_menu(ctx, menu, false);
                    ctx.set_handled();
                    return;
                }
                Key::Named(NamedKey::ArrowRight)
                    if matches!(menu_rows.get(self.selected), Some(MenuRow::Submenu(_))) =>
                {
                    self.activate_selected(ctx, menu);
                    ctx.set_handled();
                    return;
                }
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
                    self.activate_selected(ctx, menu);
                    ctx.set_handled();
                    return;
                }
                Key::Character(ref c) if c.as_str() == " " => {
                    self.activate_selected(ctx, menu);
                    ctx.set_handled();
                    return;
                }
                Key::Named(NamedKey::Escape) => {
                    if self.active_submenu.is_some() {
                        self.show_menu(ctx, menu, false);
                    } else {
                        self.close(ctx, None);
                    }
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
        if let Some(action) = crate::actions::action_for_key_in_window(&key.key, key.modifiers) {
            if ACTIONS
                .iter()
                .position(|entry| entry.action == action)
                .is_some_and(|index| self.states[index].enabled)
            {
                ctx.submit_action::<AppAction>(action);
            }
            ctx.set_handled();
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action != masonry::accesskit::Action::Click {
            return;
        }
        let target = ctx.target();
        if let Some(index) = self
            .accessible_titles
            .iter()
            .position(|title| title.id() == target)
        {
            self.show_menu(ctx, index, true);
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
        node: &mut Node,
    ) {
        node.set_label("Application menu");
    }

    fn children_ids(&self) -> ChildrenIds {
        std::iter::once(self.inner.id())
            .chain(self.accessible_titles.iter().map(|title| title.id()))
            .collect()
    }

    fn accepts_focus(&self) -> bool {
        true
    }
}

struct AccessibleMenuTitle {
    label: &'static str,
}

impl Widget for AccessibleMenuTitle {
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
            Axis::Horizontal => Length::px(title_width(self.label)),
            Axis::Vertical => Length::px(BAR_HEIGHT),
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        Role::MenuItem
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.label);
        node.add_action(masonry::accesskit::Action::Click);
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }
}

struct AccessibleMenuItem {
    label: &'static str,
    state: EntryState,
    row: MenuRow,
    creator: WidgetId,
    popup: WidgetId,
    menu: usize,
    palette: Arc<Palette>,
    states: Arc<Vec<EntryState>>,
    focus_before: Option<WidgetId>,
}

impl Widget for AccessibleMenuItem {
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
            Axis::Vertical => Length::px(ROW_HEIGHT),
        }
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _painter: &mut Painter<'_>,
    ) {
    }

    fn accessibility_role(&self) -> Role {
        if self.state.checked.is_some() {
            Role::MenuItemCheckBox
        } else {
            Role::MenuItem
        }
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(self.label);
        if !self.state.enabled {
            node.set_disabled();
        }
        if let Some(checked) = self.state.checked {
            node.set_toggled(Toggled::from(checked));
        }
        if self.state.enabled {
            node.add_action(masonry::accesskit::Action::Click);
        }
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if event.action != masonry::accesskit::Action::Click || !self.state.enabled {
            return;
        }
        let creator = self.creator;
        let old_popup = self.popup;
        let menu = self.menu;
        let focus = self.focus_before;
        match self.row {
            MenuRow::Submenu(name) => {
                let palette = self.palette.clone();
                let states = self.states.clone();
                ctx.mutate_later(creator, move |mut shell| {
                    let mut shell = shell.downcast::<MenuShell>();
                    shell.ctx.remove_layer(old_popup);
                    let popup = new_popup(creator, menu, Some(name), palette, states, focus);
                    let id = popup.id();
                    shell.ctx.create_layer(
                        LayerType::Other,
                        popup,
                        Point::new(title_rect(menu).x0 + POPUP_WIDTH, BAR_HEIGHT),
                    );
                    shell.widget.open = Some(id);
                    shell.widget.active_submenu = Some(name);
                    shell.widget.selected = 0;
                    shell.ctx.request_render();
                });
            }
            MenuRow::Action(index) => {
                let action = ACTIONS[index].action;
                if let Some(id) = focus {
                    ctx.set_focus(id);
                }
                ctx.mutate_later(creator, move |mut shell| {
                    let mut shell = shell.downcast::<MenuShell>();
                    shell.widget.open = None;
                    shell.widget.active_menu = None;
                    shell.widget.active_submenu = None;
                    shell.widget.focus_before = None;
                    shell.ctx.remove_layer(old_popup);
                    shell.ctx.submit_action::<AppAction>(action);
                    shell.ctx.request_render();
                });
            }
        }
        ctx.set_handled();
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }
}

struct MenuPopup {
    creator: WidgetId,
    menu: usize,
    submenu: Option<&'static str>,
    selected: usize,
    palette: Arc<Palette>,
    states: Arc<Vec<EntryState>>,
    focus_before: Option<WidgetId>,
    size: Size,
    accessible_rows: Vec<WidgetPod<AccessibleMenuItem>>,
}

impl MenuPopup {
    fn new(
        creator: WidgetId,
        menu: usize,
        submenu: Option<&'static str>,
        palette: Arc<Palette>,
        states: Arc<Vec<EntryState>>,
        focus_before: Option<WidgetId>,
    ) -> Self {
        Self {
            creator,
            menu,
            submenu,
            selected: 0,
            palette,
            states,
            focus_before,
            size: Size::ZERO,
            accessible_rows: Vec::new(),
        }
    }

    fn row_at(&self, point: Point) -> Option<usize> {
        if !(0.0..=POPUP_WIDTH).contains(&point.x) {
            return None;
        }
        let row = round_units(((point.y - POPUP_PAD) / ROW_HEIGHT).floor());
        usize::try_from(row)
            .ok()
            .filter(|row| *row < rows(self.menu, self.submenu).len())
    }

    fn choose(&self, ctx: &mut EventCtx<'_>) {
        let Some(row) = rows(self.menu, self.submenu).get(self.selected).copied() else {
            return;
        };
        if let MenuRow::Submenu(name) = row {
            let creator = self.creator;
            let old_popup = ctx.widget_id();
            let menu = self.menu;
            let palette = self.palette.clone();
            let states = self.states.clone();
            let focus = self.focus_before;
            ctx.mutate_later(creator, move |mut shell| {
                let mut shell = shell.downcast::<MenuShell>();
                shell.ctx.remove_layer(old_popup);
                let popup = new_popup(creator, menu, Some(name), palette, states, focus);
                let id = popup.id();
                shell.ctx.create_layer(
                    LayerType::Other,
                    popup,
                    Point::new(title_rect(menu).x0 + POPUP_WIDTH, BAR_HEIGHT),
                );
                shell.widget.open = Some(id);
                shell.widget.active_submenu = Some(name);
                shell.widget.selected = 0;
                shell.ctx.request_render();
            });
            return;
        }
        let MenuRow::Action(index) = row else {
            return;
        };
        if !self.states[index].enabled {
            return;
        }
        let entry = &ACTIONS[index];
        let action = entry.action;
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
            shell.widget.active_submenu = None;
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
            shell.widget.active_submenu = None;
            shell.widget.focus_before = None;
            shell.ctx.remove_layer(popup);
            shell.ctx.request_render();
        });
    }
}

fn new_popup(
    creator: WidgetId,
    menu: usize,
    submenu: Option<&'static str>,
    palette: Arc<Palette>,
    states: Arc<Vec<EntryState>>,
    focus_before: Option<WidgetId>,
) -> NewWidget<MenuPopup> {
    let mut popup = NewWidget::new(MenuPopup::new(
        creator,
        menu,
        submenu,
        palette.clone(),
        states.clone(),
        focus_before,
    ));
    let popup_id = popup.id();
    popup.widget.accessible_rows = rows(menu, submenu)
        .into_iter()
        .map(|row| {
            NewWidget::new(AccessibleMenuItem {
                label: row_label(row),
                state: row_state(row, &states),
                row,
                creator,
                popup: popup_id,
                menu,
                palette: palette.clone(),
                states: states.clone(),
                focus_before,
            })
            .to_pod()
        })
        .collect();
    popup
}

impl Widget for MenuPopup {
    type Action = ();
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for row in &mut self.accessible_rows {
            ctx.register_child(row);
        }
    }
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
            Axis::Vertical => Length::px(
                rows(self.menu, self.submenu).len() as f64 * ROW_HEIGHT + POPUP_PAD * 2.0,
            ),
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        for (index, row) in self.accessible_rows.iter_mut().enumerate() {
            ctx.run_layout(row, Size::new(POPUP_WIDTH, ROW_HEIGHT));
            ctx.place_child(row, Point::new(0.0, POPUP_PAD + index as f64 * ROW_HEIGHT));
        }
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
            .stroke(self.size.to_rect(), &Stroke::new(1.0), pal.outline)
            .draw();
        for (index, row) in rows(self.menu, self.submenu).into_iter().enumerate() {
            let state = row_state(row, &self.states);
            let label = row_label(row);
            let top = POPUP_PAD + index as f64 * ROW_HEIGHT;
            if row_separator(row, self.menu) {
                painter
                    .stroke(
                        Line::new(Point::new(6.0, top), Point::new(POPUP_WIDTH - 6.0, top)),
                        &Stroke::new(1.0),
                        pal.outline,
                    )
                    .draw();
            }
            if self.selected == index && state.enabled {
                painter
                    .fill(
                        Rect::new(2.0, top, POPUP_WIDTH - 2.0, top + ROW_HEIGHT),
                        pal.selected_bg(),
                    )
                    .draw();
            }
            text_label::draw(
                painter,
                Point::new(26.0, top + ROW_HEIGHT / 2.0),
                label,
                13.0,
                row_ink(pal, state.enabled, self.selected == index),
                Anchor::Start,
            );
            if state.checked == Some(true) {
                let ink = row_ink(pal, state.enabled, self.selected == index);
                painter
                    .stroke(
                        Line::new(Point::new(10.0, top + 12.0), Point::new(14.0, top + 16.0)),
                        &Stroke::new(1.5),
                        ink,
                    )
                    .draw();
                painter
                    .stroke(
                        Line::new(Point::new(14.0, top + 16.0), Point::new(21.0, top + 8.0)),
                        &Stroke::new(1.5),
                        ink,
                    )
                    .draw();
            }
            if let MenuRow::Submenu(_) = row {
                text_label::draw(
                    painter,
                    Point::new(POPUP_WIDTH - 10.0, top + ROW_HEIGHT / 2.0),
                    "›",
                    15.0,
                    row_ink(pal, state.enabled, self.selected == index),
                    Anchor::End,
                );
            }
            if let MenuRow::Action(state_index) = row
                && let Some(accelerator) = ACTIONS[state_index].accelerator
            {
                let accelerator = shortcut_label(accelerator);
                text_label::draw(
                    painter,
                    Point::new(POPUP_WIDTH - 10.0, top + ROW_HEIGHT / 2.0),
                    &accelerator,
                    13.0,
                    row_ink(pal, state.enabled, self.selected == index),
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
                if let Some(row) = self.row_at(ctx.local_position(current.position))
                    && row != self.selected
                {
                    self.selected = row;
                    ctx.request_render();
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
        let count = rows(self.menu, self.submenu).len();
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
        node: &mut Node,
    ) {
        node.set_label(MENUS[self.menu]);
    }
    fn children_ids(&self) -> ChildrenIds {
        self.accessible_rows.iter().map(|row| row.id()).collect()
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

pub(crate) struct MenuShellView<V> {
    inner: V,
    palette: Arc<Palette>,
    states: Arc<Vec<EntryState>>,
}

fn entry_states(app: &AppState) -> Arc<Vec<EntryState>> {
    Arc::new(
        ACTIONS
            .iter()
            .map(|entry| EntryState {
                enabled: entry.enabled(app),
                checked: entry.checked(app),
            })
            .collect(),
    )
}

pub(crate) fn menu_shell<V: WidgetView<AppState>>(
    inner: V,
    palette: Arc<Palette>,
    app: &AppState,
) -> MenuShellView<impl WidgetView<AppState>> {
    use crate::*;
    use masonry::properties::Padding;
    let menu_width: f64 = MENUS.iter().map(|title| title_width(title)).sum();
    let row = sized_box(header::view(app))
        .padding(Padding {
            left: Length::px(menu_width),
            right: Space::Md.length(),
            top: Length::ZERO,
            bottom: Length::ZERO,
        })
        .dims(Dimensions::new(
            Dim::Stretch,
            Dim::Fixed(Length::px(BAR_HEIGHT)),
        ));
    let inner = flex_col((row, inner.flex(1.0))).gap(Space::None);
    MenuShellView {
        inner,
        palette,
        states: entry_states(app),
    }
}

impl<V> ViewMarker for MenuShellView<V> {}
impl<V> View<AppState, (), ViewCtx> for MenuShellView<V>
where
    V: WidgetView<AppState>,
{
    type Element = Pod<MenuShell>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, app: &mut AppState) -> (Self::Element, Self::ViewState) {
        let (child, child_state) = self.inner.build(ctx, app);
        let pod = ctx.with_action_widget(|ctx| {
            let mut shell =
                MenuShell::new(child.new_widget, self.palette.clone(), self.states.clone());
            shell.integrated_header = true;
            ctx.create_pod(shell)
        });
        (pod, child_state)
    }
    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut AppState,
    ) {
        if !Arc::ptr_eq(&element.widget.palette, &self.palette) {
            element.ctx.request_render();
        }
        element.widget.palette = self.palette.clone();
        element.widget.states = self.states.clone();
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
        app: &mut AppState,
    ) -> MessageResult<()> {
        if message.remaining_path().is_empty() {
            return match message.take_message::<AppAction>() {
                Some(action) => {
                    if crate::actions::action_enabled(*action, app) {
                        app.dispatch(*action);
                    }
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
    use masonry::accesskit::{ActionRequest, TreeId};
    use masonry::core::keyboard::{Code, KeyboardEvent, Modifiers};
    use masonry::properties::Dimensions;
    use masonry::theme::default_property_set;
    use masonry::widgets::{Button, Label};
    use masonry_testing::TestHarness;

    fn key(key: Key) -> TextEvent {
        key_with_modifiers(key, Modifiers::empty())
    }

    fn key_with_modifiers(key: Key, modifiers: Modifiers) -> TextEvent {
        TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key,
            code: Code::Unidentified,
            modifiers,
            ..KeyboardEvent::default()
        })
    }

    fn harness() -> (TestHarness<MenuShell>, WidgetId) {
        let button = NewWidget::new(Button::new(Label::new("editor").prepare()));
        let button_id = button.id();
        let states = Arc::new(
            ACTIONS
                .iter()
                .map(|_| EntryState {
                    enabled: true,
                    checked: None,
                })
                .collect(),
        );
        let shell = MenuShell::new(button, Arc::new(Palette::load("gray")), states)
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
    fn quit_shortcut_dispatches_through_application_state() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));
        let mut modifiers = Modifiers::empty();
        modifiers.set(Modifiers::META, true);

        harness.process_text_event(key_with_modifiers(Key::Character("q".into()), modifiers));

        assert_eq!(
            harness.pop_action::<AppAction>().map(|(action, _)| action),
            Some(AppAction::Quit)
        );
    }

    #[test]
    fn keyboard_navigation_dispatches_one_shared_table_action() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));

        harness.process_text_event(key(Key::Named(NamedKey::F10)));
        harness.process_text_event(key(Key::Named(NamedKey::ArrowRight)));
        harness.process_text_event(key(Key::Named(NamedKey::ArrowDown)));
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

    #[test]
    fn keyboard_enters_submenu_and_dispatches_choice() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));

        harness.process_text_event(key(Key::Named(NamedKey::F10)));
        harness.process_text_event(key(Key::Named(NamedKey::ArrowLeft)));
        for _ in 0..10 {
            harness.process_text_event(key(Key::Named(NamedKey::ArrowDown)));
        }
        harness.process_text_event(key(Key::Named(NamedKey::ArrowRight)));
        let submenu = harness.edit_root_widget(|root| root.widget.active_submenu);
        assert_eq!(submenu, Some("Theme"));

        harness.process_text_event(key(Key::Named(NamedKey::Enter)));
        let action = harness.pop_action::<AppAction>();
        assert_eq!(
            action.map(|(action, _)| action),
            Some(AppAction::Theme("dark"))
        );
        assert_eq!(harness.focused_widget_id(), Some(button_id));
    }

    #[test]
    fn pointer_down_outside_dismisses_and_restores_focus() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));
        harness.process_text_event(key(Key::Named(NamedKey::F10)));

        harness.mouse_move(Point::new(700.0, 500.0));
        harness.mouse_button_press(Some(PointerButton::Primary));

        let active = harness.edit_root_widget(|root| root.widget.active_menu);
        assert_eq!(active, None);
        assert_eq!(harness.focused_widget_id(), Some(button_id));
    }

    #[test]
    fn pointer_opens_bar_and_dispatches_a_row_once() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));

        let file = MENUS.iter().position(|menu| *menu == "File").unwrap();
        harness.mouse_move(title_rect(file).center());
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        let active = harness.edit_root_widget(|root| root.widget.active_menu);
        assert_eq!(active, Some(file));

        harness.mouse_move(Point::new(
            title_rect(file).x0 + 40.0,
            BAR_HEIGHT + POPUP_PAD + ROW_HEIGHT / 2.0,
        ));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        let action = harness.pop_action::<AppAction>();
        assert_eq!(action.map(|(action, _)| action), Some(AppAction::NewFont));
        assert!(harness.pop_action::<AppAction>().is_none());
    }

    #[test]
    fn pointer_switches_titles_while_a_menu_is_open() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));

        harness.mouse_move(Point::new(20.0, 12.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));

        let view = MENUS.len() - 1;
        let view_title = title_rect(view).center();
        harness.mouse_move(view_title);

        let active = harness.edit_root_widget(|root| root.widget.active_menu);
        assert_eq!(active, Some(view));
        assert_ne!(harness.focused_widget_id(), Some(button_id));
    }

    #[test]
    fn pointer_enters_submenu_and_dispatches_choice() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));

        let view = MENUS.len() - 1;
        harness.mouse_move(title_rect(view).center());
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));

        let theme_row = 10.0;
        harness.mouse_move(Point::new(
            title_rect(view).x0 + 40.0,
            BAR_HEIGHT + POPUP_PAD + (theme_row + 0.5) * ROW_HEIGHT,
        ));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        let submenu = harness.edit_root_widget(|root| root.widget.active_submenu);
        assert_eq!(submenu, Some("Theme"));

        harness.mouse_move(Point::new(
            title_rect(view).x0 + POPUP_WIDTH + 40.0,
            BAR_HEIGHT + POPUP_PAD + ROW_HEIGHT / 2.0,
        ));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));

        let action = harness.pop_action::<AppAction>();
        assert_eq!(
            action.map(|(action, _)| action),
            Some(AppAction::Theme("dark"))
        );
        assert_eq!(harness.focused_widget_id(), Some(button_id));
        assert!(harness.pop_action::<AppAction>().is_none());
    }

    #[test]
    fn accessibility_click_dispatches_and_closes_the_menu() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));
        let file = MENUS.iter().position(|menu| *menu == "File").unwrap();
        let file_title = harness.edit_root_widget(|root| root.widget.accessible_titles[file].id());
        harness.process_access_event(ActionRequest {
            action: masonry::accesskit::Action::Click,
            target_tree: TreeId::ROOT,
            target_node: file_title.into(),
            data: None,
        });

        let popup_id = harness
            .edit_root_widget(|root| root.widget.open)
            .expect("F10 creates a popup layer");
        let first_row = harness
            .get_widget_with_id(popup_id)
            .downcast::<MenuPopup>()
            .expect("the layer is a menu popup")
            .inner()
            .accessible_rows[0]
            .id();
        harness.process_access_event(ActionRequest {
            action: masonry::accesskit::Action::Click,
            target_tree: TreeId::ROOT,
            target_node: first_row.into(),
            data: None,
        });

        let action = harness.pop_action::<AppAction>();
        assert_eq!(action.map(|(action, _)| action), Some(AppAction::NewFont));
        assert_eq!(harness.focused_widget_id(), Some(button_id));
        assert_eq!(harness.edit_root_widget(|root| root.widget.open), None);
        assert!(harness.pop_action::<AppAction>().is_none());
    }

    #[test]
    fn accessibility_click_opens_a_top_level_menu() {
        let (mut harness, button_id) = harness();
        harness.focus_on(Some(button_id));
        let view = MENUS.len() - 1;
        let title_id = harness.edit_root_widget(|root| root.widget.accessible_titles[view].id());

        harness.process_access_event(ActionRequest {
            action: masonry::accesskit::Action::Click,
            target_tree: TreeId::ROOT,
            target_node: title_id.into(),
            data: None,
        });

        assert_eq!(
            harness.edit_root_widget(|root| root.widget.active_menu),
            Some(view)
        );
        assert_ne!(harness.focused_widget_id(), Some(button_id));
        assert!(harness.pop_action::<AppAction>().is_none());
    }
}

#[cfg(test)]
mod contrast_tests {
    use super::*;

    fn contrast(a: xilem::Color, b: xilem::Color) -> f64 {
        fn luminance(c: xilem::Color) -> f64 {
            let linear = |v: f32| {
                let v = f64::from(v);
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            linear(c.components[0]) * 0.2126
                + linear(c.components[1]) * 0.7152
                + linear(c.components[2]) * 0.0722
        }
        let a = luminance(a);
        let b = luminance(b);
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn menu_text_is_readable_in_every_theme_and_state() {
        for theme in ["gray", "light", "dark"] {
            let pal = Palette::load(theme);
            assert!(
                contrast(pal.header_ink, pal.header) >= 4.5,
                "{theme} header"
            );
            assert!(
                contrast(row_ink(&pal, true, false), pal.panel) >= 4.5,
                "{theme} enabled"
            );
            assert!(
                contrast(row_ink(&pal, true, true), pal.selected_bg()) >= 4.5,
                "{theme} selected"
            );
            assert!(
                contrast(row_ink(&pal, false, false), pal.panel) >= 3.0,
                "{theme} disabled"
            );
            assert_eq!(row_ink(&pal, false, true), row_ink(&pal, false, false));
        }
    }
}

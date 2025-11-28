// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! System toolbar widget - file operations toolbar (save, etc.)
//!
//! This toolbar provides buttons for common file operations like save.
//! It appears in both the glyph grid view and editor view.

use kurbo::{BezPath, Point, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, EventCtx, LayoutCtx, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut,
    PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
};
use masonry::vello::Scene;
use std::time::Instant;

// Import shared toolbar functionality
use crate::components::toolbars::{
    button_rect, calculate_toolbar_size, paint_button, paint_icon,
    paint_panel, ButtonState,
};

/// System toolbar button types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemToolbarButton {
    /// Save the current file/project
    Save,
}

/// How long to show the flash state (in milliseconds)
const FLASH_DURATION_MS: u128 = 1500;

/// System toolbar widget
pub struct SystemToolbarWidget {
    /// Currently hovered button
    hover_button: Option<SystemToolbarButton>,
    /// Whether button is in active/clicked state (for visual feedback)
    flash_on: bool,
    /// When the flash started (to know when to allow reset)
    flash_start: Option<Instant>,
}

impl SystemToolbarWidget {
    pub fn new() -> Self {
        Self {
            hover_button: None,
            flash_on: false,
            flash_start: None,
        }
    }

    /// Get the icon path for a button
    fn icon_for_button(button: SystemToolbarButton) -> BezPath {
        match button {
            SystemToolbarButton::Save => save_icon(),
        }
    }

    /// Find which button was clicked
    fn button_at_point(&self, point: Point) -> Option<SystemToolbarButton> {
        if button_rect(0).contains(point) {
            return Some(SystemToolbarButton::Save);
        }
        None
    }
}

/// Action sent when a system toolbar button is clicked
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SystemToolbarAction(pub SystemToolbarButton);

impl Widget for SystemToolbarWidget {
    type Action = SystemToolbarAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {
        // Leaf widget - no children
    }

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
        let size = calculate_toolbar_size(1);
        bc.constrain(size)
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        scene: &mut Scene,
    ) {
        let size = ctx.size();
        paint_panel(scene, size);
        self.paint_button(scene);
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

    fn children_ids(&self) -> masonry::core::ChildrenIds {
        masonry::core::ChildrenIds::new()
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
                let local_pos = ctx.local_position(state.position);
                if let Some(button) = self.button_at_point(local_pos) {
                    ctx.submit_action::<SystemToolbarAction>(
                        SystemToolbarAction(button),
                    );
                    // Show active state for visual feedback
                    self.flash_on = true;
                    self.flash_start = Some(Instant::now());
                    ctx.request_render();
                }
                ctx.set_handled();
            }
            PointerEvent::Move(pointer_move) => {
                let local_pos = ctx.local_position(pointer_move.current.position);
                let new_hover = self.button_at_point(local_pos);
                if new_hover != self.hover_button {
                    self.hover_button = new_hover;
                    ctx.request_render();
                }
            }
            PointerEvent::Leave(_) => {
                if self.hover_button.is_some() {
                    self.hover_button = None;
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

impl SystemToolbarWidget {
    fn paint_button(&self, scene: &mut Scene) {
        let rect = button_rect(0);
        let is_hovered = self.hover_button == Some(SystemToolbarButton::Save);
        // Use flash_on state for selected appearance during flash animation
        let is_selected = self.flash_on;
        let state = ButtonState::new(is_hovered, is_selected);

        paint_button(scene, rect, state);
        let icon = Self::icon_for_button(SystemToolbarButton::Save);
        paint_icon(scene, icon, rect, state);
    }
}

/// Save icon - floppy disk shape
fn save_icon() -> BezPath {
    let mut path = BezPath::new();

    // Draw a simple floppy disk icon centered at origin
    // Outer rectangle with cut corner
    let size = 28.0;
    let half = size / 2.0;
    let corner_cut = 6.0;

    // Main body (with top-right corner cut)
    path.move_to((-half, -half));
    path.line_to((half - corner_cut, -half));
    path.line_to((half, -half + corner_cut));
    path.line_to((half, half));
    path.line_to((-half, half));
    path.close_path();

    // Label area (bottom rectangle)
    let label_height = 10.0;
    let label_width = 18.0;
    let label_x = -label_width / 2.0;
    let label_y = half - label_height - 3.0;

    path.move_to((label_x, label_y));
    path.line_to((label_x + label_width, label_y));
    path.line_to((label_x + label_width, label_y + label_height));
    path.line_to((label_x, label_y + label_height));
    path.close_path();

    // Shutter area (top rectangle)
    let shutter_width = 14.0;
    let shutter_height = 8.0;
    let shutter_x = -shutter_width / 2.0;
    let shutter_y = -half + 2.0;

    path.move_to((shutter_x, shutter_y));
    path.line_to((shutter_x + shutter_width, shutter_y));
    path.line_to((shutter_x + shutter_width, shutter_y + shutter_height));
    path.line_to((shutter_x, shutter_y + shutter_height));
    path.close_path();

    path
}

// ===== XILEM VIEW WRAPPER =====

use std::marker::PhantomData;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

type SystemToolbarCallback<State> =
    Box<dyn Fn(&mut State, SystemToolbarButton) + Send + Sync>;

/// Public API to create a system toolbar view
pub fn system_toolbar_view<State, Action>(
    callback: impl Fn(&mut State, SystemToolbarButton) + Send + Sync + 'static,
) -> SystemToolbarView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    SystemToolbarView {
        callback: Box::new(callback),
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SystemToolbarView<State, Action = ()> {
    callback: SystemToolbarCallback<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker for SystemToolbarView<State, Action> {}

impl<State: 'static, Action: 'static + Default> View<State, Action, ViewCtx>
    for SystemToolbarView<State, Action>
{
    type Element = Pod<SystemToolbarWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget = SystemToolbarWidget::new();
        let pod = ctx.create_pod(widget);
        ctx.record_action(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        _prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        // Reset flash state when view rebuilds (user made changes)
        // But only if enough time has passed since the flash started
        // (to avoid resetting immediately due to save updating state)
        if let Some(start) = element.widget.flash_start {
            if start.elapsed().as_millis() >= FLASH_DURATION_MS {
                element.widget.flash_on = false;
                element.widget.flash_start = None;
            }
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
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<SystemToolbarAction>() {
            Some(action) => {
                (self.callback)(app_state, action.0);
                MessageResult::Action(Action::default())
            }
            None => MessageResult::Stale,
        }
    }
}

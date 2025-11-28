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

/// Save icon - U+E000 from untitled.ufo
fn save_icon() -> BezPath {
    let mut path = BezPath::new();

    // Glyph metrics: width=600, bbox roughly (44,44) to (571,556)
    // Center: ~307.5, 300  Size: ~527 x 512
    // Scale to fit in ~24x24 icon, centered at origin
    let scale = 24.0 / 512.0;
    let cx = 307.5;
    let cy = 300.0;

    // Helper to transform points
    // IMPORTANT: Font coordinates have Y going UP, screen coordinates have Y going DOWN
    // We negate Y to flip the glyph right-side up for screen rendering
    let t = |x: f64, y: f64| -> (f64, f64) {
        ((x - cx) * scale, -(y - cy) * scale)
    };

    // Contour 1
    let p = t(227.0, 115.0); path.move_to(p);
    path.curve_to(t(238.0, 115.0), t(255.0, 95.0), t(255.0, 83.0));
    path.curve_to(t(255.0, 71.0), t(234.0, 44.0), t(224.0, 44.0));
    path.curve_to(t(215.0, 44.0), t(191.0, 60.0), t(191.0, 67.0));
    path.curve_to(t(191.0, 73.0), t(220.0, 115.0), t(227.0, 115.0));
    path.close_path();

    // Contour 2
    let p = t(134.0, 161.0); path.move_to(p);
    path.curve_to(t(96.0, 161.0), t(44.0, 169.0), t(44.0, 225.0));
    path.curve_to(t(44.0, 248.0), t(48.0, 267.0), t(58.0, 267.0));
    path.curve_to(t(67.0, 267.0), t(62.0, 251.0), t(86.0, 236.0));
    path.curve_to(t(99.0, 228.0), t(118.0, 221.0), t(183.0, 221.0));
    path.curve_to(t(321.0, 221.0), t(377.0, 263.0), t(377.0, 277.0));
    path.curve_to(t(377.0, 283.0), t(374.0, 287.0), t(369.0, 287.0));
    path.curve_to(t(363.0, 287.0), t(359.0, 285.0), t(352.0, 285.0));
    path.curve_to(t(342.0, 285.0), t(332.0, 290.0), t(332.0, 306.0));
    path.curve_to(t(332.0, 324.0), t(354.0, 358.0), t(368.0, 358.0));
    path.curve_to(t(387.0, 358.0), t(403.0, 333.0), t(403.0, 304.0));
    path.curve_to(t(403.0, 251.0), t(357.0, 161.0), t(134.0, 161.0));
    path.close_path();

    // Contour 3
    let p = t(169.0, 556.0); path.move_to(p);
    path.curve_to(t(180.0, 556.0), t(197.0, 536.0), t(197.0, 524.0));
    path.curve_to(t(197.0, 512.0), t(176.0, 485.0), t(166.0, 485.0));
    path.curve_to(t(157.0, 485.0), t(133.0, 501.0), t(133.0, 508.0));
    path.curve_to(t(133.0, 514.0), t(162.0, 556.0), t(169.0, 556.0));
    path.close_path();

    // Contour 4
    let p = t(355.0, 512.0); path.move_to(p);
    path.curve_to(t(366.0, 512.0), t(381.0, 492.0), t(381.0, 480.0));
    path.curve_to(t(381.0, 468.0), t(362.0, 441.0), t(352.0, 441.0));
    path.curve_to(t(343.0, 441.0), t(319.0, 457.0), t(319.0, 464.0));
    path.curve_to(t(319.0, 470.0), t(348.0, 512.0), t(355.0, 512.0));
    path.close_path();

    // Contour 5
    let p = t(231.0, 278.0); path.move_to(p);
    path.curve_to(t(214.0, 278.0), t(196.0, 290.0), t(196.0, 333.0));
    path.curve_to(t(196.0, 435.0), t(224.0, 532.0), t(238.0, 532.0));
    path.curve_to(t(243.0, 532.0), t(244.0, 529.0), t(244.0, 525.0));
    path.curve_to(t(244.0, 513.0), t(226.0, 473.0), t(226.0, 384.0));
    path.curve_to(t(226.0, 353.0), t(234.0, 338.0), t(247.0, 338.0));
    path.curve_to(t(266.0, 338.0), t(268.0, 369.0), t(274.0, 369.0));
    path.curve_to(t(278.0, 369.0), t(280.0, 363.0), t(280.0, 353.0));
    path.curve_to(t(280.0, 325.0), t(260.0, 278.0), t(231.0, 278.0));
    path.close_path();

    // Contour 6
    let p = t(84.0, 276.0); path.move_to(p);
    path.curve_to(t(78.0, 276.0), t(75.0, 278.0), t(75.0, 283.0));
    path.curve_to(t(75.0, 291.0), t(120.0, 320.0), t(137.0, 341.0));
    path.curve_to(t(142.0, 347.0), t(151.0, 365.0), t(157.0, 384.0));
    path.curve_to(t(167.0, 413.0), t(171.0, 436.0), t(176.0, 436.0));
    path.curve_to(t(179.0, 436.0), t(181.0, 434.0), t(181.0, 429.0));
    path.curve_to(t(181.0, 397.0), t(168.0, 333.0), t(152.0, 308.0));
    path.curve_to(t(137.0, 285.0), t(94.0, 276.0), t(84.0, 276.0));
    path.close_path();

    // Contour 7
    let p = t(364.0, 168.0); path.move_to(p);
    path.curve_to(t(360.0, 168.0), t(357.0, 170.0), t(357.0, 175.0));
    path.curve_to(t(357.0, 185.0), t(414.0, 215.0), t(428.0, 232.0));
    path.curve_to(t(442.0, 249.0), t(451.0, 280.0), t(459.0, 291.0));
    path.curve_to(t(471.0, 308.0), t(500.0, 313.0), t(507.0, 321.0));
    path.curve_to(t(514.0, 329.0), t(529.0, 347.0), t(529.0, 353.0));
    path.curve_to(t(529.0, 359.0), t(505.0, 364.0), t(495.0, 364.0));
    path.curve_to(t(489.0, 364.0), t(471.0, 343.0), t(465.0, 343.0));
    path.curve_to(t(461.0, 343.0), t(459.0, 345.0), t(459.0, 349.0));
    path.curve_to(t(459.0, 356.0), t(486.0, 390.0), t(503.0, 390.0));
    path.curve_to(t(520.0, 390.0), t(536.0, 380.0), t(543.0, 380.0));
    path.curve_to(t(549.0, 380.0), t(559.0, 391.0), t(565.0, 391.0));
    path.curve_to(t(569.0, 391.0), t(571.0, 387.0), t(571.0, 383.0));
    path.curve_to(t(571.0, 378.0), t(564.0, 365.0), t(559.0, 360.0));
    path.curve_to(t(556.0, 357.0), t(548.0, 351.0), t(543.0, 345.0));
    path.curve_to(t(538.0, 339.0), t(520.0, 302.0), t(512.0, 292.0));
    path.curve_to(t(504.0, 282.0), t(471.0, 270.0), t(464.0, 260.0));
    path.curve_to(t(457.0, 250.0), t(441.0, 194.0), t(424.0, 184.0));
    path.curve_to(t(410.0, 176.0), t(383.0, 168.0), t(364.0, 168.0));
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

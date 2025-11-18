// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Workspace toolbar widget - navigation toolbar for switching between
//! workspaces (edit views) and the glyph grid view
//!
//! Similar to tabs in Glyphs app, this toolbar allows users to switch
//! between multiple editor workspaces and return to the glyph grid view.

use kurbo::{BezPath, Point, Rect, RoundedRect, Shape, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, EventCtx, LayoutCtx, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut,
    PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
};
use masonry::vello::Scene;

// Import shared toolbar functionality
use crate::components::toolbars::{
    button_rect, calculate_toolbar_size, paint_button, paint_icon,
    paint_panel, ButtonState,
};

/// Workspace toolbar button types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceToolbarButton {
    /// Return to glyph grid view
    GlyphGrid,
}

/// Workspace toolbar widget
pub struct WorkspaceToolbarWidget {
    /// Currently hovered button
    hover_button: Option<WorkspaceToolbarButton>,
}

impl WorkspaceToolbarWidget {
    pub fn new() -> Self {
        Self { hover_button: None }
    }

    /// Get the icon path for a button
    fn icon_for_button(button: WorkspaceToolbarButton) -> BezPath {
        match button {
            WorkspaceToolbarButton::GlyphGrid => glyph_grid_icon(),
        }
    }

    /// Find which button was clicked
    fn button_at_point(&self, point: Point) -> Option<WorkspaceToolbarButton> {
        // Currently only one button (glyph grid)
        if button_rect(0).contains(point) {
            return Some(WorkspaceToolbarButton::GlyphGrid);
        }
        None
    }

}

/// Action sent when a workspace toolbar button is clicked
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkspaceToolbarAction(pub WorkspaceToolbarButton);

impl Widget for WorkspaceToolbarWidget {
    type Action = WorkspaceToolbarAction;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {
        // Leaf widget - no children
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
        // No updates needed
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        let size = calculate_toolbar_size(1); // Currently only one button
        bc.constrain(size)
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        scene: &mut Scene,
    ) {
        let size = ctx.size();

        // Draw background panel
        paint_panel(scene, size);

        // Draw button
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
        // TODO: Add accessibility info
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
                self.handle_pointer_down(ctx, state);
            }
            PointerEvent::Move(pointer_move) => {
                self.handle_pointer_move(ctx, pointer_move);
            }
            PointerEvent::Leave(_) => {
                self.handle_pointer_leave(ctx);
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
        // No text handling needed
    }
}

impl WorkspaceToolbarWidget {
    /// Paint the glyph grid button
    fn paint_button(&self, scene: &mut Scene) {
        let rect = button_rect(0);
        let is_hovered =
            self.hover_button == Some(WorkspaceToolbarButton::GlyphGrid);

        // Workspace toolbar buttons don't have a selected state
        let state = ButtonState::new(is_hovered, false);

        // Draw button background and border
        paint_button(scene, rect, state);

        // Draw icon
        let icon = Self::icon_for_button(WorkspaceToolbarButton::GlyphGrid);
        paint_icon(scene, icon, rect, state);
    }

    /// Handle pointer down event
    fn handle_pointer_down(
        &mut self,
        ctx: &mut EventCtx<'_>,
        state: &masonry::core::PointerState,
    ) {
        let local_pos = ctx.local_position(state.position);
        if let Some(button) = self.button_at_point(local_pos) {
            ctx.submit_action::<WorkspaceToolbarAction>(
                WorkspaceToolbarAction(button),
            );
            ctx.request_render();
        }
        // Always consume the event to prevent it from reaching the
        // editor
        ctx.set_handled();
    }

    /// Handle pointer move event (for hover state)
    fn handle_pointer_move(
        &mut self,
        ctx: &mut EventCtx<'_>,
        pointer_move: &masonry::core::PointerUpdate,
    ) {
        let local_pos = ctx.local_position(pointer_move.current.position);
        let new_hover = self.button_at_point(local_pos);
        if new_hover != self.hover_button {
            self.hover_button = new_hover;
            ctx.request_render();
        }
    }

    /// Handle pointer leave event (clear hover state)
    fn handle_pointer_leave(&mut self, ctx: &mut EventCtx<'_>) {
        if self.hover_button.is_some() {
            self.hover_button = None;
            ctx.request_render();
        }
    }
}

/// Glyph grid icon - 3x3 grid of squares
fn glyph_grid_icon() -> BezPath {
    let mut path = BezPath::new();

    // Draw a 3x3 grid of small squares
    let grid_size = 32.0;
    let cell_size = 8.0;
    let gap = 4.0;
    let offset = -(grid_size / 2.0);

    for row in 0..3 {
        for col in 0..3 {
            let x = offset + col as f64 * (cell_size + gap);
            let y = offset + row as f64 * (cell_size + gap);
            let rect = Rect::new(x, y, x + cell_size, y + cell_size);
            let rounded_rect = RoundedRect::from_rect(rect, 1.0);
            // Convert RoundedRect to BezPath using the Shape trait
            let rect_path = rounded_rect.to_path(0.1);
            path.extend(rect_path);
        }
    }

    path
}

// ===== XILEM VIEW WRAPPER =====

use std::marker::PhantomData;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

/// Callback type for workspace toolbar button clicks
type WorkspaceToolbarCallback<State> =
    Box<dyn Fn(&mut State, WorkspaceToolbarButton) + Send + Sync>;

/// Xilem view for the workspace toolbar
pub struct WorkspaceToolbarView<State, Action = ()> {
    callback: WorkspaceToolbarCallback<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker for WorkspaceToolbarView<State, Action> {}

impl<State: 'static, Action: 'static + Default> View<State, Action, ViewCtx>
    for WorkspaceToolbarView<State, Action>
{
    type Element = Pod<WorkspaceToolbarWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget = WorkspaceToolbarWidget::new();
        (
            ctx.with_action_widget(|ctx| ctx.create_pod(widget)),
            (),
        )
    }

    fn rebuild(
        &self,
        _prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        // No state to rebuild
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
        // No teardown needed
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<WorkspaceToolbarAction>() {
            Some(action) => {
                (self.callback)(app_state, action.0);
                MessageResult::Action(Action::default())
            }
            None => MessageResult::Stale,
        }
    }
}

/// Helper function to create a workspace toolbar view
pub fn workspace_toolbar_view<State, Action>(
    callback: impl Fn(&mut State, WorkspaceToolbarButton)
        + Send
        + Sync
        + 'static,
) -> WorkspaceToolbarView<State, Action>
where
    Action: 'static,
{
    WorkspaceToolbarView {
        callback: Box::new(callback),
        phantom: PhantomData,
    }
}


// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Text direction sub-toolbar widget - LTR/RTL selection for the text tool
//!
//! This toolbar appears below the main edit mode toolbar when the text
//! tool is selected, allowing users to choose between left-to-right
//! and right-to-left text direction.

use crate::shaping::TextDirection;
use kurbo::{BezPath, Point, Shape, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent,
    PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget,
};
use masonry::vello::Scene;
use std::marker::PhantomData;
use tracing;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

// Import shared toolbar functionality
use crate::components::toolbars::{
    button_rect, calculate_toolbar_size, paint_button, paint_icon,
    paint_panel, ButtonState,
};

/// Available text directions in display order
const TOOLBAR_DIRECTIONS: &[TextDirection] = &[
    TextDirection::LeftToRight,
    TextDirection::RightToLeft,
];

/// Text direction sub-toolbar widget
pub struct TextDirectionToolbarWidget {
    /// Currently selected text direction
    selected_direction: TextDirection,
    /// Currently hovered direction (if any)
    hover_direction: Option<TextDirection>,
}

impl TextDirectionToolbarWidget {
    pub fn new(selected_direction: TextDirection) -> Self {
        Self {
            selected_direction,
            hover_direction: None,
        }
    }

    /// Get the icon path for a text direction
    fn icon_for_direction(direction: TextDirection) -> BezPath {
        match direction {
            TextDirection::LeftToRight => ltr_icon(),
            TextDirection::RightToLeft => rtl_icon(),
        }
    }

    /// Find which direction was clicked
    fn direction_at_point(&self, point: Point) -> Option<TextDirection> {
        for (i, &direction) in TOOLBAR_DIRECTIONS.iter().enumerate() {
            if button_rect(i).contains(point) {
                return Some(direction);
            }
        }
        None
    }
}

/// Action sent when a text direction is selected
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TextDirectionSelected(pub TextDirection);

impl Widget for TextDirectionToolbarWidget {
    type Action = TextDirectionSelected;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {
        // Leaf widget - no children
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
        // No update logic needed
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        let size = calculate_toolbar_size(TOOLBAR_DIRECTIONS.len());
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

        // Paint each direction button
        for (i, &direction) in TOOLBAR_DIRECTIONS.iter().enumerate() {
            let rect = button_rect(i);
            let state = ButtonState::new(
                self.hover_direction == Some(direction),
                self.selected_direction == direction,
            );

            paint_button(scene, rect, state);
            paint_icon(scene, Self::icon_for_direction(direction), rect, state);
        }
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

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Move(state) => {
                let local_pos = ctx.local_position(state.current.position);
                let new_hover = self.direction_at_point(local_pos);
                if new_hover != self.hover_direction {
                    self.hover_direction = new_hover;
                    ctx.request_render();
                }
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let local_pos = ctx.local_position(state.position);
                if let Some(direction) = self.direction_at_point(local_pos) {
                    tracing::debug!("Text direction toolbar: clicked {:?}", direction);
                    self.selected_direction = direction;
                    ctx.request_render();
                    ctx.submit_action::<TextDirectionSelected>(
                        TextDirectionSelected(direction),
                    );
                }
            }
            PointerEvent::Leave(_) => {
                if self.hover_direction.is_some() {
                    self.hover_direction = None;
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
        // No text handling needed
    }
}

// --- Icon Definitions ---

/// LTR icon - arrow pointing right with horizontal lines
fn ltr_icon() -> BezPath {
    let mut bez = BezPath::new();

    // Arrow pointing right (main directional indicator)
    // Arrow shaft
    bez.move_to((150.0, 384.0));
    bez.line_to((550.0, 384.0));

    // Arrow head
    bez.move_to((450.0, 284.0));
    bez.line_to((550.0, 384.0));
    bez.line_to((450.0, 484.0));

    // Text lines (left-aligned)
    bez.move_to((150.0, 200.0));
    bez.line_to((500.0, 200.0));

    bez.move_to((150.0, 568.0));
    bez.line_to((450.0, 568.0));

    // Convert to filled shapes by using thick strokes
    // Since we're drawing lines, we need to convert to a filled path
    let mut filled = BezPath::new();

    // Arrow shaft (as rectangle)
    let shaft_height = 40.0;
    filled.move_to((150.0, 384.0 - shaft_height / 2.0));
    filled.line_to((500.0, 384.0 - shaft_height / 2.0));
    filled.line_to((500.0, 384.0 + shaft_height / 2.0));
    filled.line_to((150.0, 384.0 + shaft_height / 2.0));
    filled.close_path();

    // Arrow head (triangle pointing right)
    filled.move_to((480.0, 384.0 - 100.0));
    filled.line_to((620.0, 384.0));
    filled.line_to((480.0, 384.0 + 100.0));
    filled.close_path();

    // Top text line
    let line_height = 30.0;
    filled.move_to((150.0, 200.0 - line_height / 2.0));
    filled.line_to((500.0, 200.0 - line_height / 2.0));
    filled.line_to((500.0, 200.0 + line_height / 2.0));
    filled.line_to((150.0, 200.0 + line_height / 2.0));
    filled.close_path();

    // Bottom text line (shorter to show alignment)
    filled.move_to((150.0, 568.0 - line_height / 2.0));
    filled.line_to((400.0, 568.0 - line_height / 2.0));
    filled.line_to((400.0, 568.0 + line_height / 2.0));
    filled.line_to((150.0, 568.0 + line_height / 2.0));
    filled.close_path();

    filled
}

/// RTL icon - arrow pointing left with horizontal lines
fn rtl_icon() -> BezPath {
    let mut filled = BezPath::new();

    // Arrow shaft (as rectangle) pointing left
    let shaft_height = 40.0;
    filled.move_to((268.0, 384.0 - shaft_height / 2.0));
    filled.line_to((618.0, 384.0 - shaft_height / 2.0));
    filled.line_to((618.0, 384.0 + shaft_height / 2.0));
    filled.line_to((268.0, 384.0 + shaft_height / 2.0));
    filled.close_path();

    // Arrow head (triangle pointing left)
    filled.move_to((288.0, 384.0 - 100.0));
    filled.line_to((148.0, 384.0));
    filled.line_to((288.0, 384.0 + 100.0));
    filled.close_path();

    // Top text line (right-aligned)
    let line_height = 30.0;
    filled.move_to((268.0, 200.0 - line_height / 2.0));
    filled.line_to((618.0, 200.0 - line_height / 2.0));
    filled.line_to((618.0, 200.0 + line_height / 2.0));
    filled.line_to((268.0, 200.0 + line_height / 2.0));
    filled.close_path();

    // Bottom text line (shorter, right-aligned)
    filled.move_to((368.0, 568.0 - line_height / 2.0));
    filled.line_to((618.0, 568.0 - line_height / 2.0));
    filled.line_to((618.0, 568.0 + line_height / 2.0));
    filled.line_to((368.0, 568.0 + line_height / 2.0));
    filled.close_path();

    filled
}

// --- Xilem View Wrapper ---

/// Public API to create a text direction toolbar view
pub fn text_direction_toolbar_view<State, Action>(
    selected_direction: TextDirection,
    callback: impl Fn(&mut State, TextDirection) + Send + Sync + 'static,
) -> TextDirectionToolbarView<State, Action>
where
    Action: 'static,
{
    TextDirectionToolbarView {
        selected_direction,
        callback: Box::new(callback),
        phantom: PhantomData,
    }
}

/// The Xilem View for TextDirectionToolbarWidget
type TextDirectionToolbarCallback<State> =
    Box<dyn Fn(&mut State, TextDirection) + Send + Sync>;

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct TextDirectionToolbarView<State, Action = ()> {
    selected_direction: TextDirection,
    callback: TextDirectionToolbarCallback<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker for TextDirectionToolbarView<State, Action> {}

impl<State: 'static, Action: 'static + Default> View<State, Action, ViewCtx>
    for TextDirectionToolbarView<State, Action>
{
    type Element = Pod<TextDirectionToolbarWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget = TextDirectionToolbarWidget::new(self.selected_direction);
        let pod = ctx.create_pod(widget);
        ctx.record_action(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        _prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        // Update widget if selected direction changed
        let mut widget = element.downcast::<TextDirectionToolbarWidget>();
        if widget.widget.selected_direction != self.selected_direction {
            widget.widget.selected_direction = self.selected_direction;
            widget.ctx.request_render();
        }
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
        // No cleanup needed
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        // Handle text direction selection actions from widget
        match message.take_message::<TextDirectionSelected>() {
            Some(action) => {
                (self.callback)(app_state, action.0);
                MessageResult::Action(Action::default())
            }
            None => MessageResult::Stale,
        }
    }
}

// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Shapes sub-toolbar widget - shape type selection for the shapes tool
//!
//! This toolbar appears below the main edit mode toolbar when the shapes
//! tool is selected, allowing users to choose between rectangle, ellipse,
//! and other shape types.

use crate::tools::shapes::ShapeType;
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

/// Available shape types in display order
const TOOLBAR_SHAPES: &[ShapeType] = &[ShapeType::Rectangle, ShapeType::Ellipse];

/// Shapes sub-toolbar widget
pub struct ShapesToolbarWidget {
    /// Currently selected shape type
    selected_shape: ShapeType,
    /// Currently hovered shape (if any)
    hover_shape: Option<ShapeType>,
}

impl ShapesToolbarWidget {
    pub fn new(selected_shape: ShapeType) -> Self {
        Self {
            selected_shape,
            hover_shape: None,
        }
    }

    /// Get the icon path for a shape type
    fn icon_for_shape(shape: ShapeType) -> BezPath {
        match shape {
            ShapeType::Rectangle => rectangle_icon(),
            ShapeType::Ellipse => ellipse_icon(),
        }
    }

    /// Find which shape was clicked
    fn shape_at_point(&self, point: Point) -> Option<ShapeType> {
        for (i, &shape) in TOOLBAR_SHAPES.iter().enumerate() {
            if button_rect(i).contains(point) {
                return Some(shape);
            }
        }
        None
    }
}

/// Action sent when a shape type is selected
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ShapeSelected(pub ShapeType);

impl Widget for ShapesToolbarWidget {
    type Action = ShapeSelected;

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
        let size = calculate_toolbar_size(TOOLBAR_SHAPES.len());
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

        // Paint each shape button
        for (i, &shape) in TOOLBAR_SHAPES.iter().enumerate() {
            let rect = button_rect(i);
            let state = ButtonState::new(
                self.hover_shape == Some(shape),
                self.selected_shape == shape,
            );

            paint_button(scene, rect, state);
            paint_icon(scene, Self::icon_for_shape(shape), rect, state);
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
                let new_hover = self.shape_at_point(local_pos);
                if new_hover != self.hover_shape {
                    self.hover_shape = new_hover;
                    ctx.request_render();
                }
            }
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let local_pos = ctx.local_position(state.position);
                if let Some(shape) = self.shape_at_point(local_pos) {
                    tracing::debug!("Shapes toolbar: clicked {:?}", shape);
                    self.selected_shape = shape;
                    ctx.request_render();
                    ctx.submit_action::<ShapeSelected>(ShapeSelected(shape));
                }
            }
            PointerEvent::Leave(_) => {
                if self.hover_shape.is_some() {
                    self.hover_shape = None;
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

fn rectangle_icon() -> BezPath {
    // Simple rectangle icon - just an outline
    let mut bez = BezPath::new();

    // Rectangle outline (centered, with some padding)
    let left = 150.0;
    let right = 618.0;
    let top = 150.0;
    let bottom = 618.0;

    bez.move_to((left, top));
    bez.line_to((right, top));
    bez.line_to((right, bottom));
    bez.line_to((left, bottom));
    bez.close_path();

    bez
}

fn ellipse_icon() -> BezPath {
    // Simple ellipse/circle icon
    // Create a circle using kurbo's ellipse
    let center = Point::new(384.0, 384.0);
    let radius = 234.0;
    let ellipse = kurbo::Ellipse::new(center, (radius, radius), 0.0);

    // Convert to BezPath using kurbo's Shape trait
    ellipse.into_path(0.1)
}

// --- Xilem View Wrapper ---

/// Public API to create a shapes toolbar view
pub fn shapes_toolbar_view<State, Action>(
    selected_shape: ShapeType,
    callback: impl Fn(&mut State, ShapeType) + Send + Sync + 'static,
) -> ShapesToolbarView<State, Action>
where
    Action: 'static,
{
    ShapesToolbarView {
        selected_shape,
        callback: Box::new(callback),
        phantom: PhantomData,
    }
}

/// The Xilem View for ShapesToolbarWidget
type ShapesToolbarCallback<State> =
    Box<dyn Fn(&mut State, ShapeType) + Send + Sync>;

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct ShapesToolbarView<State, Action = ()> {
    selected_shape: ShapeType,
    callback: ShapesToolbarCallback<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker for ShapesToolbarView<State, Action> {}

impl<State: 'static, Action: 'static + Default> View<State, Action, ViewCtx>
    for ShapesToolbarView<State, Action>
{
    type Element = Pod<ShapesToolbarWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget = ShapesToolbarWidget::new(self.selected_shape);
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
        // Update widget if selected shape changed
        let mut widget = element.downcast::<ShapesToolbarWidget>();
        if widget.widget.selected_shape != self.selected_shape {
            widget.widget.selected_shape = self.selected_shape;
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
        // Handle shape selection actions from widget
        match message.take_message::<ShapeSelected>() {
            Some(action) => {
                (self.callback)(app_state, action.0);
                MessageResult::Nop
            }
            None => MessageResult::Stale,
        }
    }
}

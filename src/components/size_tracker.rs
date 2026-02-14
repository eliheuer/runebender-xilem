// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Size tracker widget - reports container width for responsive layouts

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx, PaintCtx,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, TextEvent, Update,
    UpdateCtx, Widget,
};
use masonry::vello::Scene;
use kurbo::Size;
use std::marker::PhantomData;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

/// Action emitted when size changes
#[derive(Clone, Copy, Debug)]
pub struct SizeChanged {
    pub width: f64,
    #[allow(dead_code)]
    pub height: f64,
}

/// A widget that tracks its size and reports changes
pub struct SizeTrackerWidget {
    size: Size,
    last_reported_width: f64,
    last_reported_height: f64,
}

impl SizeTrackerWidget {
    pub fn new() -> Self {
        Self {
            size: Size::ZERO,
            last_reported_width: 0.0,
            last_reported_height: 0.0,
        }
    }
}

impl Widget for SizeTrackerWidget {
    type Action = SizeChanged;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {
        // No children
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
        ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        // Take full size
        self.size = bc.max();

        // Report size change if width or height changed significantly
        let width_changed =
            (self.size.width - self.last_reported_width).abs() > 1.0;
        let height_changed =
            (self.size.height - self.last_reported_height).abs() > 1.0;
        if width_changed || height_changed {
            self.last_reported_width = self.size.width;
            self.last_reported_height = self.size.height;
            ctx.submit_action::<SizeChanged>(SizeChanged {
                width: self.size.width,
                height: self.size.height,
            });
        }

        self.size
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        _scene: &mut Scene,
    ) {
        // Invisible - no painting
    }

    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
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

    fn on_pointer_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &PointerEvent,
    ) {
        // No pointer handling needed
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

// --- Xilem View Wrapper ---

/// Public API to create a size tracker view
///
/// Callback receives `(state, width, height)`.
pub fn size_tracker<State, Action>(
    on_size_change: impl Fn(&mut State, f64, f64) + Send + Sync + 'static,
) -> SizeTrackerView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    SizeTrackerView {
        on_size_change: Box::new(on_size_change),
        phantom: PhantomData,
    }
}

type SizeCallback<State> =
    Box<dyn Fn(&mut State, f64, f64) + Send + Sync>;

/// The Xilem View for SizeTrackerWidget
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct SizeTrackerView<State, Action = ()> {
    on_size_change: SizeCallback<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker for SizeTrackerView<State, Action> {}

impl<State: 'static, Action: 'static + Default> View<State, Action, ViewCtx>
    for SizeTrackerView<State, Action>
{
    type Element = Pod<SizeTrackerWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget = SizeTrackerWidget::new();
        let pod = ctx.create_pod(widget);
        ctx.record_action(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        _prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        // Nothing to rebuild
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
        match message.take_message::<SizeChanged>() {
            Some(sc) => {
                (self.on_size_change)(
                    app_state, sc.width, sc.height,
                );
                MessageResult::Action(Action::default())
            }
            None => MessageResult::Stale,
        }
    }
}

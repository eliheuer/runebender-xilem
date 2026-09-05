// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The empty part of the title bar: a press on it moves the window, a
//! double click zooms it, the way the system title bar behaves. This
//! is what lets the header stand in for the title bar on macOS, where
//! the window opens with a transparent title bar and the header runs
//! under the traffic lights.
//!
//! Masonry has both calls: `drag_window` and `toggle_maximized` send
//! signals the winit runner answers. What it lacks is a view for them,
//! so this is a widget of no size and no paint that only listens.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Size};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

pub(crate) struct DragRegionWidget;

impl Widget for DragRegionWidget {
    type Action = ();

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        _len_req: LenReq,
        _cross: Option<Length>,
    ) -> Length {
        // No size of its own: it takes what the row gives it.
        Length::px(0.0)
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, _size: Size) {}

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, _p: &mut Painter<'_>) {
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            state,
            ..
        }) = event
        {
            if state.count >= 2 {
                ctx.toggle_maximized();
            } else {
                ctx.drag_window();
            }
            ctx.set_handled();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::TitleBar
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
}

/// The view: a drag handle with no content.
pub(crate) struct DragRegion;

pub(crate) fn drag_region() -> DragRegion {
    DragRegion
}

impl ViewMarker for DragRegion {}
impl<State: 'static> View<State, (), ViewCtx> for DragRegion {
    type Element = Pod<DragRegionWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        (ctx.create_pod(DragRegionWidget), ())
    }

    fn rebuild(
        &self,
        _: &Self,
        (): &mut Self::ViewState,
        _: &mut ViewCtx,
        _: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        _: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        _: &mut State,
    ) -> MessageResult<()> {
        MessageResult::Stale
    }
}

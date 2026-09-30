// Copyright 2025 the Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Invisible workspace splitter view.
//! Adapted from pinned Xilem/Masonry b81d8d7; only the splitter paint is suppressed.
//! Retain native cursor, keyboard, pointer, focus and accessibility behavior on updates.

use std::marker::PhantomData;

use super::quiet_split_widget as widgets;
use masonry::kurbo::Axis;
use masonry::layout::{AsUnit, Length};
use masonry::widgets::SplitPoint;

use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker};
use xilem::{Pod, ViewCtx, WidgetView};

pub(crate) fn split<State, Action, ChildA, ChildB>(
    child1: ChildA,
    child2: ChildB,
) -> Split<ChildA, ChildB, State, Action>
where
    ChildA: WidgetView<State, Action>,
    ChildB: WidgetView<State, Action>,
    State: 'static,
{
    Split {
        split_axis: Axis::Horizontal,
        split_point: SplitPoint::Fraction(0.5),
        min_lengths: (Length::ZERO, Length::ZERO),
        bar_thickness: 6.px(),
        min_bar_area: 6.px(),
        solid_bar: false,
        draggable: true,
        child1,
        child2,
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub(crate) struct Split<ChildA, ChildB, State, Action = ()> {
    split_axis: Axis,
    split_point: SplitPoint,
    min_lengths: (Length, Length),
    bar_thickness: Length,
    min_bar_area: Length,
    solid_bar: bool,
    draggable: bool,
    child1: ChildA,
    child2: ChildB,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<ChildA, ChildB, State, Action> Split<ChildA, ChildB, State, Action> {
    pub(crate) fn split_axis(mut self, axis: Axis) -> Self {
        self.split_axis = axis;
        self
    }

    pub(crate) fn split_point_from_start(mut self, split_point: Length) -> Self {
        self.split_point = SplitPoint::FromStart(split_point);
        self
    }

    pub(crate) fn split_point_from_end(mut self, split_point: Length) -> Self {
        self.split_point = SplitPoint::FromEnd(split_point);
        self
    }

    pub(crate) fn min_lengths(mut self, first: Length, second: Length) -> Self {
        self.min_lengths = (first, second);
        self
    }

    #[track_caller]
    pub(crate) fn bar_thickness(mut self, bar_thickness: Length) -> Self {
        self.bar_thickness = bar_thickness;
        self
    }

    #[track_caller]
    pub(crate) fn min_bar_area(mut self, min_bar_area: Length) -> Self {
        self.min_bar_area = min_bar_area;
        self
    }

    pub(crate) fn draggable(mut self, draggable: bool) -> Self {
        self.draggable = draggable;
        self
    }

    pub(crate) fn solid_bar(mut self, solid: bool) -> Self {
        self.solid_bar = solid;
        self
    }
}

// Use a distinctive number here, to be able to catch bugs.
// These were selected based on a random multiple (less than 1000) of 40960000.
// That base is chosen so that there are at least three trailing zeroes in both the hex
// and decimal forms, making the +1 obvious.

const CHILD1_VIEW_ID: ViewId = ViewId::new(0x65edc0000);
const CHILD2_VIEW_ID: ViewId = ViewId::new(0x65edc0001);

impl<ChildA, ChildB, State, Action> ViewMarker for Split<ChildA, ChildB, State, Action> {}
impl<ChildA, ChildB, State, Action> View<State, Action, ViewCtx>
    for Split<ChildA, ChildB, State, Action>
where
    State: 'static,
    Action: 'static,
    ChildA: WidgetView<State, Action>,
    ChildB: WidgetView<State, Action>,
{
    type Element = Pod<widgets::Split<ChildA::Widget, ChildB::Widget>>;

    type ViewState = (ChildA::ViewState, ChildB::ViewState);

    fn build(&self, ctx: &mut ViewCtx, app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child1, child1_state) =
            ctx.with_id(CHILD1_VIEW_ID, |ctx| self.child1.build(ctx, app_state));
        let (child2, child2_state) =
            ctx.with_id(CHILD2_VIEW_ID, |ctx| self.child2.build(ctx, app_state));

        let widget_pod = ctx.create_pod(
            widgets::Split::new(child1.new_widget, child2.new_widget)
                .split_axis(self.split_axis)
                .split_point(self.split_point)
                .min_lengths(self.min_lengths.0, self.min_lengths.1)
                .bar_thickness(self.bar_thickness)
                .min_bar_area(self.min_bar_area)
                .draggable(self.draggable)
                .solid_bar(self.solid_bar),
        );

        (widget_pod, (child1_state, child2_state))
    }

    fn rebuild(
        &self,
        prev: &Self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) {
        if prev.split_axis != self.split_axis {
            widgets::Split::set_split_axis(&mut element, self.split_axis);
        }

        if prev.split_point != self.split_point {
            widgets::Split::set_split_point(&mut element, self.split_point);
        }

        if prev.min_lengths != self.min_lengths {
            widgets::Split::set_min_lengths(&mut element, self.min_lengths.0, self.min_lengths.1);
        }

        if prev.bar_thickness != self.bar_thickness {
            widgets::Split::set_bar_thickness(&mut element, self.bar_thickness);
        }

        if prev.min_bar_area != self.min_bar_area {
            widgets::Split::set_min_bar_area(&mut element, self.min_bar_area);
        }

        if prev.draggable != self.draggable {
            widgets::Split::set_draggable(&mut element, self.draggable);
        }

        if prev.solid_bar != self.solid_bar {
            widgets::Split::set_bar_solid(&mut element, self.solid_bar);
        }

        ctx.with_id(CHILD1_VIEW_ID, |ctx| {
            let child1_element = widgets::Split::child1_mut(&mut element);
            self.child1.rebuild(
                &prev.child1,
                &mut view_state.0,
                ctx,
                child1_element,
                app_state,
            );
        });

        ctx.with_id(CHILD2_VIEW_ID, |ctx| {
            let child2_element = widgets::Split::child2_mut(&mut element);
            self.child2.rebuild(
                &prev.child2,
                &mut view_state.1,
                ctx,
                child2_element,
                app_state,
            );
        });
    }

    fn teardown(
        &self,
        view_state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        let child1_element = widgets::Split::child1_mut(&mut element);
        self.child1.teardown(&mut view_state.0, ctx, child1_element);

        let child2_element = widgets::Split::child2_mut(&mut element);
        self.child2.teardown(&mut view_state.1, ctx, child2_element);
    }

    fn message(
        &self,
        view_state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_first() {
            Some(CHILD1_VIEW_ID) => {
                let child1_element = widgets::Split::child1_mut(&mut element);
                self.child1
                    .message(&mut view_state.0, message, child1_element, app_state)
            }
            Some(CHILD2_VIEW_ID) => {
                let child2_element = widgets::Split::child2_mut(&mut element);
                self.child2
                    .message(&mut view_state.1, message, child2_element, app_state)
            }
            _ => MessageResult::Stale,
        }
    }
}

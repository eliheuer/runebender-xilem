// Copyright 2019 the Xilem Authors and the Druid Authors
// SPDX-License-Identifier: Apache-2.0

//! Invisible workspace splitter interaction and layout.
//! Adapted from pinned Xilem/Masonry b81d8d7; only the splitter paint is suppressed.
//! Retain native cursor, keyboard, pointer, focus and accessibility behavior on updates.

use masonry::accesskit::{ActionData, Node, Role};

use masonry::core::keyboard::{Key, NamedKey};
use masonry::core::{
    AccessCtx, AccessEvent, ChildrenIds, CursorIcon, EventCtx, FromDynWidget, LayoutCtx,
    MeasureCtx, NewWidget, NoAction, PaintCtx, PointerButtonEvent, PointerEvent, PointerUpdate,
    PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
    WidgetMut, WidgetPod,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{AsUnit, LayoutSize, LenReq, Length};

use masonry::widgets::SplitPoint;

pub(crate) struct Split<ChildA, ChildB>
where
    ChildA: Widget + ?Sized,
    ChildB: Widget + ?Sized,
{
    split_axis: Axis,
    split_point_chosen: SplitPoint,
    split_point_effective: f64,
    min_lengths: (Length, Length),
    bar_thickness: Length,
    min_bar_area: Length,
    solid: bool,
    draggable: bool,
    click_offset: f64,
    child1: WidgetPod<ChildA>,
    child2: WidgetPod<ChildB>,
}

// --- MARK: BUILDERS
impl<ChildA: Widget + ?Sized, ChildB: Widget + ?Sized> Split<ChildA, ChildB> {
    pub(crate) fn new(child1: NewWidget<ChildA>, child2: NewWidget<ChildB>) -> Self {
        Self {
            split_axis: Axis::Horizontal,
            split_point_chosen: SplitPoint::Fraction(0.5),
            split_point_effective: 0.5,
            min_lengths: (Length::ZERO, Length::ZERO),
            bar_thickness: 6.px(),
            min_bar_area: 6.px(),
            solid: false,
            draggable: true,
            click_offset: 0.0,
            child1: child1.to_pod(),
            child2: child2.to_pod(),
        }
    }

    pub(crate) fn split_axis(mut self, split_axis: Axis) -> Self {
        self.split_axis = split_axis;
        self
    }

    pub(crate) fn split_point(mut self, split_point: SplitPoint) -> Self {
        self.split_point_chosen = match split_point {
            SplitPoint::Fraction(frac) => SplitPoint::Fraction(frac.clamp(0.0, 1.0)),
            other => other,
        };
        self
    }

    pub(crate) fn min_lengths(mut self, first: Length, second: Length) -> Self {
        self.min_lengths = (first, second);
        self
    }

    pub(crate) fn bar_thickness(mut self, bar_thickness: Length) -> Self {
        self.bar_thickness = bar_thickness;
        self
    }

    pub(crate) fn min_bar_area(mut self, min_bar_area: Length) -> Self {
        self.min_bar_area = min_bar_area;
        self
    }

    pub(crate) fn draggable(mut self, draggable: bool) -> Self {
        self.draggable = draggable;
        self
    }

    pub(crate) fn solid_bar(mut self, solid: bool) -> Self {
        self.solid = solid;
        self
    }
}

// --- MARK: METHODS
impl<ChildA: Widget + ?Sized, ChildB: Widget + ?Sized> Split<ChildA, ChildB> {
    #[inline]
    fn bar_area(&self) -> f64 {
        self.bar_thickness.max(self.min_bar_area).get()
    }

    fn bar_center(&self, length: f64) -> f64 {
        let (edge1, edge2) = self.bar_edges(length);
        (edge1 + edge2) * 0.5
    }

    fn bar_edges(&self, length: f64) -> (f64, f64) {
        let bar_thickness = self.bar_thickness.get();
        let reduced_length = length - bar_thickness;
        let edge = reduced_length * self.split_point_effective;
        (edge, edge + bar_thickness)
    }

    fn bar_area_edges(&self, length: f64) -> (f64, f64) {
        let (edge1, edge2) = self.bar_edges(length);
        let (space1, space2) = (edge1.max(0.), (length - edge2).max(0.));
        let padding = self.bar_area() - self.bar_thickness.get();

        // Half the padding to the first edge
        let pad1 = (0.5 * padding).min(space1);
        // Remainder to the second edge
        let pad2 = (padding - pad1).min(space2);
        // First edge gets more, in case space2 was low but space1 is high
        let pad1 = (padding - pad2).min(space1);

        (edge1 - pad1, edge2 + pad2)
    }

    fn bar_area_hit_test(&self, length: f64, pos: f64) -> bool {
        let (edge1, edge2) = self.bar_area_edges(length);
        pos >= edge1 && pos <= edge2
    }

    fn split_side_limits(&self, length: f64) -> (f64, f64) {
        let (min_child1, min_child2) = self.min_lengths;
        let mut min_limit = min_child1.get();
        let mut max_limit = (length - min_child2.get()).max(0.0);

        if min_limit > max_limit {
            min_limit = 0.5 * (min_limit + max_limit);
            max_limit = min_limit;
        }

        (min_limit, max_limit)
    }

    fn calc_effective_split_point(&self, length: f64) -> f64 {
        let (min_limit, max_limit) = self.split_side_limits(length);
        if length <= f64::EPSILON {
            0.5
        } else {
            let child1_len = match self.split_point_chosen {
                SplitPoint::Fraction(frac) => length * frac,
                SplitPoint::FromStart(len) => len.get(),
                SplitPoint::FromEnd(len) => length - len.get(),
            };
            (child1_len / length).clamp(min_limit / length, max_limit / length)
        }
    }

    fn set_chosen_from_child1_len(&mut self, length: f64, child1_len: f64) {
        let (min_limit, max_limit) = self.split_side_limits(length);
        let child1_len = child1_len.clamp(min_limit, max_limit);

        match self.split_point_chosen {
            SplitPoint::Fraction(_) => {
                self.split_point_chosen = SplitPoint::Fraction(if length <= f64::EPSILON {
                    0.5
                } else {
                    child1_len / length
                });
            }
            SplitPoint::FromStart(_) => {
                self.split_point_chosen = SplitPoint::FromStart(Length::px(child1_len));
            }
            SplitPoint::FromEnd(_) => {
                let child2_len = (length - child1_len).max(0.0);
                self.split_point_chosen = SplitPoint::FromEnd(Length::px(child2_len));
            }
        }
    }

    fn update_split_point_from_bar_center(&mut self, total_length: f64, bar_center: f64) {
        let bar_thickness = self.bar_thickness.get();
        let split_space = (total_length - bar_thickness).max(0.0);
        let child1_len = bar_center - bar_thickness * 0.5;
        self.set_chosen_from_child1_len(split_space, child1_len);
    }
}

// --- MARK: WIDGETMUT
impl<ChildA, ChildB> Split<ChildA, ChildB>
where
    ChildA: Widget + FromDynWidget + ?Sized,
    ChildB: Widget + FromDynWidget + ?Sized,
{
    pub(crate) fn child1_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, ChildA> {
        this.ctx.get_mut(&mut this.widget.child1)
    }

    pub(crate) fn child2_mut<'t>(this: &'t mut WidgetMut<'_, Self>) -> WidgetMut<'t, ChildB> {
        this.ctx.get_mut(&mut this.widget.child2)
    }

    pub(crate) fn set_split_axis(this: &mut WidgetMut<'_, Self>, split_axis: Axis) {
        this.widget.split_axis = split_axis;
        this.ctx.request_layout();
    }

    pub(crate) fn set_split_point(this: &mut WidgetMut<'_, Self>, split_point: SplitPoint) {
        this.widget.split_point_chosen = match split_point {
            SplitPoint::Fraction(frac) => SplitPoint::Fraction(frac.clamp(0.0, 1.0)),
            other => other,
        };
        this.ctx.request_layout();
    }

    pub(crate) fn set_min_lengths(this: &mut WidgetMut<'_, Self>, first: Length, second: Length) {
        this.widget.min_lengths = (first, second);
        this.ctx.request_layout();
    }

    pub(crate) fn set_bar_thickness(this: &mut WidgetMut<'_, Self>, bar_thickness: Length) {
        this.widget.bar_thickness = bar_thickness;
        this.ctx.request_layout();
    }

    pub(crate) fn set_min_bar_area(this: &mut WidgetMut<'_, Self>, min_bar_area: Length) {
        this.widget.min_bar_area = min_bar_area;
        this.ctx.request_layout();
    }

    pub(crate) fn set_draggable(this: &mut WidgetMut<'_, Self>, draggable: bool) {
        this.widget.draggable = draggable;
        // Bar mutability impacts appearance, but not accessibility node
        // TODO - This might change in a future implementation
        this.ctx.request_paint_only();
    }

    pub(crate) fn set_bar_solid(this: &mut WidgetMut<'_, Self>, solid: bool) {
        this.widget.solid = solid;
        // Bar solidity impacts appearance, but not accessibility node
        this.ctx.request_paint_only();
    }
}

// --- MARK: IMPL WIDGET
impl<ChildA, ChildB> Widget for Split<ChildA, ChildB>
where
    ChildA: Widget + ?Sized,
    ChildB: Widget + ?Sized,
{
    type Action = NoAction;

    fn accepts_focus(&self) -> bool {
        true
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if self.draggable {
            match event {
                PointerEvent::Down(PointerButtonEvent { state, .. }) => {
                    let pos = ctx
                        .local_position(state.position)
                        .get_coord(self.split_axis);
                    let length = ctx.content_box().size().get_coord(self.split_axis);
                    if self.bar_area_hit_test(length, pos) {
                        ctx.set_handled();
                        ctx.capture_pointer();
                        ctx.request_focus();
                        // Save the delta between the click position and the bar center.
                        self.click_offset = pos - self.bar_center(length);
                    }
                }
                PointerEvent::Move(PointerUpdate { current, .. }) if ctx.is_active() => {
                    let pos = ctx
                        .local_position(current.position)
                        .get_coord(self.split_axis);
                    let length = ctx.content_box().size().get_coord(self.split_axis);
                    // If widget has pointer capture, assume always it's hovered
                    let effective_center = pos - self.click_offset;
                    self.update_split_point_from_bar_center(length, effective_center);
                    ctx.request_layout();
                }
                PointerEvent::Up(..) | PointerEvent::Cancel(..) => {
                    self.click_offset = 0.0;
                }
                _ => {}
            }
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        if ctx.is_disabled() || !ctx.is_focus_target() || !self.draggable {
            return;
        }

        let TextEvent::Keyboard(key_event) = event else {
            return;
        };
        if !key_event.state.is_down() {
            return;
        }

        let length = ctx.content_box().size().get_coord(self.split_axis);
        let bar_thickness = self.bar_thickness.get();
        let split_space = (length - bar_thickness).max(0.0);
        if split_space <= f64::EPSILON {
            return;
        }

        let step = (split_space / 100.0).max(1.0);
        let big_step = step * 10.0;
        let delta = if key_event.modifiers.shift() {
            big_step
        } else {
            step
        };

        let mut child1_len = split_space * self.split_point_effective;
        match key_event.key {
            Key::Named(NamedKey::ArrowLeft) if self.split_axis == Axis::Horizontal => {
                child1_len -= delta;
            }
            Key::Named(NamedKey::ArrowRight) if self.split_axis == Axis::Horizontal => {
                child1_len += delta;
            }
            Key::Named(NamedKey::ArrowUp) if self.split_axis == Axis::Vertical => {
                child1_len -= delta;
            }
            Key::Named(NamedKey::ArrowDown) if self.split_axis == Axis::Vertical => {
                child1_len += delta;
            }
            Key::Named(NamedKey::Home) => {
                child1_len = self.split_side_limits(split_space).0;
            }
            Key::Named(NamedKey::End) => {
                child1_len = self.split_side_limits(split_space).1;
            }
            _ => return,
        }

        self.set_chosen_from_child1_len(split_space, child1_len);
        ctx.request_layout();
    }

    fn on_access_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &AccessEvent,
    ) {
        if ctx.is_disabled() || !self.draggable {
            return;
        }

        let length = ctx.content_box().size().get_coord(self.split_axis);
        let bar_thickness = self.bar_thickness.get();
        let split_space = (length - bar_thickness).max(0.0);
        if split_space <= f64::EPSILON {
            return;
        }

        let step = (split_space / 100.0).max(1.0);
        let mut child1_len = split_space * self.split_point_effective;

        match event.action {
            masonry::accesskit::Action::Increment => child1_len += step,
            masonry::accesskit::Action::Decrement => child1_len -= step,
            masonry::accesskit::Action::SetValue => match &event.data {
                Some(ActionData::NumericValue(value)) => child1_len = *value,
                Some(ActionData::Value(value)) => {
                    if let Ok(value) = value.parse() {
                        child1_len = value;
                    }
                }
                _ => return,
            },
            _ => return,
        }

        self.set_chosen_from_child1_len(split_space, child1_len);
        ctx.request_layout();
    }

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child1);
        ctx.register_child(&mut self.child2);
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        match event {
            Update::FocusChanged(_)
            | Update::HoveredChanged(_)
            | Update::ActiveChanged(_)
            | Update::DisabledChanged(_) => {
                ctx.request_paint_only();
            }
            _ => {}
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross_length: Option<Length>,
    ) -> Length {
        if let LenReq::FitContent(space) = len_req {
            // We always want to use up all offered space
            if axis == self.split_axis {
                // Don't go below the bar thickness, which we always want to paint.
                return space.max(self.bar_thickness);
            }
            return space;
        }
        // Both children can share the same auto length, because it'll be either Min or MaxContent.
        let auto_length = len_req.into();

        let cross = axis.cross();
        let (child1_cross_space, child2_cross_space) = cross_length
            .map(|cross_length| {
                // We need to split the cross length if it's our split axis
                if cross == self.split_axis {
                    let cross_space = cross_length.saturating_sub(self.bar_thickness);
                    let split_point = self.calc_effective_split_point(cross_space.get());
                    let child1_cross_space = (cross_space.get() * split_point).px();
                    (
                        child1_cross_space,
                        cross_space.saturating_sub(child1_cross_space),
                    )
                } else {
                    (cross_length, cross_length)
                }
            })
            .unzip();
        let child1_context_size = LayoutSize::maybe(cross, child1_cross_space);
        let child2_context_size = LayoutSize::maybe(cross, child2_cross_space);

        let child1_length = ctx.compute_length(
            &mut self.child1,
            auto_length,
            child1_context_size,
            axis,
            child1_cross_space,
        );
        let child2_length = ctx.compute_length(
            &mut self.child2,
            auto_length,
            child2_context_size,
            axis,
            child2_cross_space,
        );

        if axis == self.split_axis {
            child1_length
                .saturating_add(child2_length)
                .saturating_add(self.bar_thickness)
        } else {
            child1_length.max(child2_length)
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let bar_thickness = self.bar_thickness.get();
        let split_space = (size.get_coord(self.split_axis) - bar_thickness).max(0.);
        let cross_space = size.get_coord(self.split_axis.cross());

        // Update our effective split point to respect our size
        self.split_point_effective = self.calc_effective_split_point(split_space);

        let child1_split_space = (split_space * self.split_point_effective).max(0.);
        let child2_split_space = (split_space - child1_split_space).max(0.);

        let child1_size = self.split_axis.pack_size(child1_split_space, cross_space);
        let child2_size = self.split_axis.pack_size(child2_split_space, cross_space);

        ctx.run_layout(&mut self.child1, child1_size);
        ctx.run_layout(&mut self.child2, child2_size);

        // Top-left align both children.
        let child1_origin = Point::ORIGIN;
        let child2_origin = self
            .split_axis
            .pack_point(child1_split_space + bar_thickness, 0.);
        ctx.place_child(&mut self.child1, child1_origin);
        ctx.place_child(&mut self.child2, child2_origin);
    }

    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn get_cursor(&self, ctx: &QueryCtx<'_>, pos: Point) -> CursorIcon {
        let length = ctx.content_box().size().get_coord(self.split_axis);
        let local_pos = ctx.to_local(pos).get_coord(self.split_axis);
        let is_bar_area_hovered = self.bar_area_hit_test(length, local_pos);

        if self.draggable && (ctx.is_active() || is_bar_area_hovered) {
            match self.split_axis {
                Axis::Horizontal => CursorIcon::EwResize,
                Axis::Vertical => CursorIcon::NsResize,
            }
        } else {
            CursorIcon::Default
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Splitter
    }

    fn accessibility(
        &mut self,
        ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        let length = ctx.content_box().size().get_coord(self.split_axis);
        let bar_thickness = self.bar_thickness.get();
        let split_space = (length - bar_thickness).max(0.0);
        let (min_limit, max_limit) = self.split_side_limits(split_space);
        let child1_len = split_space * self.split_point_effective;

        node.set_orientation(match self.split_axis {
            Axis::Horizontal => masonry::accesskit::Orientation::Horizontal,
            Axis::Vertical => masonry::accesskit::Orientation::Vertical,
        });
        node.set_value(child1_len.to_string());
        node.set_numeric_value(child1_len);
        node.set_min_numeric_value(min_limit);
        node.set_max_numeric_value(max_limit);
        node.set_numeric_value_step((split_space / 100.0).max(1.0));

        if self.draggable && !ctx.is_disabled() {
            node.add_action(masonry::accesskit::Action::SetValue);
            node.add_action(masonry::accesskit::Action::Increment);
            node.add_action(masonry::accesskit::Action::Decrement);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child1.id(), self.child2.id()])
    }
}

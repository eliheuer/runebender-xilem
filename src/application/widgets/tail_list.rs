// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A list that grows up from its bottom edge and shows whole rows only.
//!
//! A conversation is read from the end: the newest row sits on the bottom
//! edge, and older rows stack above it. A row that does not fit entirely
//! in the list is not shown at all, instead of being cut by the edge. The
//! wheel scrolls the list up through older rows, and the scrolled distance
//! is kept from the end, so a new row keeps the list on the end.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, CollectionWidget, EventCtx, FromDynWidget, LayoutCtx, MeasureCtx,
    NewWidget, PaintCtx, PointerEvent, PointerScrollEvent, PropertiesMut, PropertiesRef,
    RegisterCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::dpi::{LogicalPosition, PhysicalPosition};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Point, Rect, Size};
use masonry::layout::{LayoutSize, LenDef, LenReq, Length};
use xilem::core::{
    AppendVec, ElementSplice, MessageCtx, MessageResult, Mut, SuperElement, View, ViewElement,
    ViewMarker, ViewSequence,
};
use xilem::{Pod, ViewCtx};

/// Where a row that is not shown is placed: far above the list, outside
/// its clip, so it is neither painted nor under the pointer.
const OUT_OF_VIEW: f64 = -1.0e6;

/// One wheel line, in logical pixels, as in Masonry's own portal.
const LINE_PX: f64 = 120.0;

pub(crate) struct TailListWidget {
    rows: Vec<WidgetPod<dyn Widget>>,
    gap: f64,
    /// How far the list is scrolled up from its end, in logical pixels.
    scrolled: f64,
    /// The rows' total height at the last layout, gaps included.
    content_height: f64,
}

impl TailListWidget {
    fn new(gap: f64) -> Self {
        Self {
            rows: Vec::new(),
            gap,
            scrolled: 0.0,
            content_height: 0.0,
        }
    }

    fn set_gap(this: &mut WidgetMut<'_, Self>, gap: f64) {
        if this.widget.gap != gap {
            this.widget.gap = gap;
            this.ctx.request_layout();
        }
    }
}

impl CollectionWidget<()> for TailListWidget {
    fn len(&self) -> usize {
        self.rows.len()
    }

    fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    fn get_mut<'t>(this: &'t mut WidgetMut<'_, Self>, idx: usize) -> WidgetMut<'t, dyn Widget> {
        this.ctx.get_mut(&mut this.widget.rows[idx])
    }

    fn add(
        this: &mut WidgetMut<'_, Self>,
        child: NewWidget<impl Widget + ?Sized>,
        _: impl Into<()>,
    ) {
        this.widget.rows.push(child.erased().to_pod());
        this.ctx.children_changed();
    }

    fn insert(
        this: &mut WidgetMut<'_, Self>,
        idx: usize,
        child: NewWidget<impl Widget + ?Sized>,
        _: impl Into<()>,
    ) {
        this.widget.rows.insert(idx, child.erased().to_pod());
        this.ctx.children_changed();
    }

    fn set(
        this: &mut WidgetMut<'_, Self>,
        idx: usize,
        child: NewWidget<impl Widget + ?Sized>,
        _: impl Into<()>,
    ) {
        let old = std::mem::replace(&mut this.widget.rows[idx], child.erased().to_pod());
        this.ctx.remove_child(old);
        this.ctx.children_changed();
    }

    fn set_params(_: &mut WidgetMut<'_, Self>, _: usize, _: impl Into<()>) {}

    fn swap(this: &mut WidgetMut<'_, Self>, a: usize, b: usize) {
        this.widget.rows.swap(a, b);
        this.ctx.children_changed();
    }

    fn remove(this: &mut WidgetMut<'_, Self>, idx: usize) {
        let old = this.widget.rows.remove(idx);
        this.ctx.remove_child(old);
        this.ctx.children_changed();
    }

    fn clear(this: &mut WidgetMut<'_, Self>) {
        for old in this.widget.rows.drain(..) {
            this.ctx.remove_child(old);
        }
        this.ctx.children_changed();
    }
}

impl Widget for TailListWidget {
    type Action = ();

    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        for row in &mut self.rows {
            ctx.register_child(row);
        }
    }

    fn children_ids(&self) -> ChildrenIds {
        self.rows.iter().map(WidgetPod::id).collect()
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        let PointerEvent::Scroll(PointerScrollEvent { delta, .. }) = event else {
            return;
        };
        let scale = ctx.scale_factor();
        let size = ctx.content_box().size();
        let line = PhysicalPosition {
            x: LINE_PX * scale,
            y: LINE_PX * scale,
        };
        let page = PhysicalPosition {
            x: size.width * scale,
            y: size.height * scale,
        };
        let LogicalPosition { y, .. } = delta.to_pixel_delta(line, page).to_logical::<f64>(scale);
        let limit = (self.content_height - size.height).max(0.0);
        let scrolled = (self.scrolled + y).clamp(0.0, limit);
        if (scrolled - self.scrolled).abs() > f64::EPSILON {
            self.scrolled = scrolled;
            ctx.request_layout();
            ctx.set_handled();
        }
    }

    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross: Option<Length>,
    ) -> Length {
        let auto: LenDef = len_req.into();
        let context = LayoutSize::maybe(axis.cross(), cross);
        let mut total = 0.0_f64;
        for row in &mut self.rows {
            let length = ctx.compute_length(row, auto, context, axis, cross).get();
            total = match axis {
                Axis::Vertical => total + length,
                Axis::Horizontal => total.max(length),
            };
        }
        if axis == Axis::Vertical && self.rows.len() > 1 {
            // Precision loss is fine for a count of rows.
            #[expect(clippy::cast_precision_loss, reason = "row counts are small")]
            let gaps = (self.rows.len() - 1) as f64;
            total += self.gap * gaps;
        }
        Length::px(total)
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        let width = Length::px(size.width);
        let mut heights = Vec::with_capacity(self.rows.len());
        for row in &mut self.rows {
            let height = ctx
                .compute_length(
                    row,
                    LenDef::MaxContent,
                    size.into(),
                    Axis::Vertical,
                    Some(width),
                )
                .get();
            ctx.run_layout(row, Size::new(size.width, height));
            heights.push(height);
        }
        let gaps = heights.len().saturating_sub(1);
        // Precision loss is fine for a count of rows.
        #[expect(clippy::cast_precision_loss, reason = "row counts are small")]
        let content = heights.iter().sum::<f64>() + self.gap * gaps as f64;
        self.content_height = content;
        self.scrolled = self.scrolled.clamp(0.0, (content - size.height).max(0.0));

        // From the end upward: the newest row's bottom sits on the edge,
        // moved up by what is scrolled. A row that is cut by either edge is
        // put out of view instead.
        let mut bottom = size.height + self.scrolled;
        for (row, height) in self.rows.iter_mut().zip(heights).rev() {
            let top = bottom - height;
            let whole = top >= -0.5 && bottom <= size.height + 0.5;
            let origin = if whole {
                Point::new(0.0, top)
            } else {
                Point::new(0.0, OUT_OF_VIEW)
            };
            ctx.place_child(row, origin);
            bottom = top - self.gap;
        }
        ctx.set_clip_path(Rect::from_origin_size(Point::ORIGIN, size));
        ctx.clear_baselines();
    }

    fn paint(&mut self, _: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, _: &mut Painter<'_>) {}

    fn accessibility_role(&self) -> Role {
        Role::Log
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
}

// --- MARK: View

/// The rows as a list that grows up from its bottom edge.
pub(crate) fn tail_list<State: 'static, Action, Seq: TailListSequence<State, Action>>(
    rows: Seq,
    gap: f64,
) -> TailList<Seq> {
    TailList { rows, gap }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub(crate) struct TailList<Seq> {
    rows: Seq,
    gap: f64,
}

pub(crate) struct TailListState<SeqState> {
    seq_state: SeqState,
    scratch: AppendVec<TailListElement>,
}

impl<Seq> ViewMarker for TailList<Seq> {}
impl<State, Action, Seq> View<State, Action, ViewCtx> for TailList<Seq>
where
    State: 'static,
    Action: 'static,
    Seq: TailListSequence<State, Action>,
{
    type Element = Pod<TailListWidget>;
    type ViewState = TailListState<Seq::SeqState>;

    fn build(&self, ctx: &mut ViewCtx, app: &mut State) -> (Self::Element, Self::ViewState) {
        let mut elements = AppendVec::default();
        let mut widget = TailListWidget::new(self.gap);
        let seq_state = self.rows.seq_build(ctx, &mut elements, app);
        for row in elements.drain() {
            widget.rows.push(row.widget.new_widget.to_pod());
        }
        (
            ctx.create_pod(widget),
            TailListState {
                seq_state,
                scratch: elements,
            },
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        TailListState { seq_state, scratch }: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut State,
    ) {
        if self.gap != prev.gap {
            TailListWidget::set_gap(&mut element, self.gap);
        }
        let mut splice = TailListSplice::new(element, scratch);
        self.rows
            .seq_rebuild(&prev.rows, seq_state, ctx, &mut splice, app);
        debug_assert!(scratch.is_empty(), "every built row was inserted");
    }

    fn teardown(
        &self,
        TailListState { seq_state, scratch }: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
    ) {
        let mut splice = TailListSplice::new(element, scratch);
        self.rows.seq_teardown(seq_state, ctx, &mut splice);
        debug_assert!(scratch.is_empty(), "every built row was inserted");
    }

    fn message(
        &self,
        TailListState { seq_state, scratch }: &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        app: &mut State,
    ) -> MessageResult<Action> {
        let mut splice = TailListSplice::new(element, scratch);
        let result = self.rows.seq_message(seq_state, message, &mut splice, app);
        debug_assert!(scratch.is_empty(), "every built row was inserted");
        result
    }
}

// --- MARK: Element

pub(crate) struct TailListElement {
    widget: Pod<dyn Widget>,
}

pub(crate) struct TailListElementMut<'w> {
    parent: WidgetMut<'w, TailListWidget>,
    idx: usize,
}

impl ViewElement for TailListElement {
    type Mut<'a> = TailListElementMut<'a>;
}

impl SuperElement<Self, ViewCtx> for TailListElement {
    fn upcast(_: &mut ViewCtx, child: Self) -> Self {
        child
    }

    fn with_downcast_val<R>(
        mut this: Mut<'_, Self>,
        f: impl FnOnce(Mut<'_, Self>) -> R,
    ) -> (Self::Mut<'_>, R) {
        let r = {
            let parent = this.parent.reborrow_mut();
            f(TailListElementMut {
                idx: this.idx,
                parent,
            })
        };
        (this, r)
    }
}

impl<W: Widget + FromDynWidget + ?Sized> SuperElement<Pod<W>, ViewCtx> for TailListElement {
    fn upcast(_: &mut ViewCtx, child: Pod<W>) -> Self {
        Self {
            widget: child.erased(),
        }
    }

    fn with_downcast_val<R>(
        mut this: Mut<'_, Self>,
        f: impl FnOnce(Mut<'_, Pod<W>>) -> R,
    ) -> (Self::Mut<'_>, R) {
        let ret = {
            let mut child = TailListWidget::get_mut(&mut this.parent, this.idx);
            f(child.downcast())
        };
        (this, ret)
    }
}

pub(crate) trait TailListSequence<State: 'static, Action = ()>:
    ViewSequence<State, Action, ViewCtx, TailListElement>
{
}

impl<Seq, State, Action> TailListSequence<State, Action> for Seq
where
    Seq: ViewSequence<State, Action, ViewCtx, TailListElement>,
    State: 'static,
{
}

// --- MARK: Splice

struct TailListSplice<'w, 's> {
    idx: usize,
    element: WidgetMut<'w, TailListWidget>,
    scratch: &'s mut AppendVec<TailListElement>,
}

impl<'w, 's> TailListSplice<'w, 's> {
    fn new(
        element: WidgetMut<'w, TailListWidget>,
        scratch: &'s mut AppendVec<TailListElement>,
    ) -> Self {
        Self {
            idx: 0,
            element,
            scratch,
        }
    }
}

impl ElementSplice<TailListElement> for TailListSplice<'_, '_> {
    fn with_scratch<R>(&mut self, f: impl FnOnce(&mut AppendVec<TailListElement>) -> R) -> R {
        let ret = f(self.scratch);
        for element in self.scratch.drain() {
            TailListWidget::insert(&mut self.element, self.idx, element.widget.new_widget, ());
            self.idx += 1;
        }
        ret
    }

    fn insert(&mut self, element: TailListElement) {
        TailListWidget::insert(&mut self.element, self.idx, element.widget.new_widget, ());
        self.idx += 1;
    }

    fn mutate<R>(&mut self, f: impl FnOnce(Mut<'_, TailListElement>) -> R) -> R {
        let child = TailListElementMut {
            parent: self.element.reborrow_mut(),
            idx: self.idx,
        };
        let ret = f(child);
        self.idx += 1;
        ret
    }

    fn skip(&mut self, n: usize) {
        self.idx += n;
    }

    fn index(&self) -> usize {
        self.idx
    }

    fn delete<R>(&mut self, f: impl FnOnce(Mut<'_, TailListElement>) -> R) -> R {
        let ret = {
            let child = TailListElementMut {
                parent: self.element.reborrow_mut(),
                idx: self.idx,
            };
            f(child)
        };
        TailListWidget::remove(&mut self.element, self.idx);
        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::core::WidgetRef;
    use masonry_testing::TestHarness;
    use xilem::style::Style as _;
    use xilem::view::{label, sized_box};

    fn rows(n: usize) -> Vec<Box<xilem::AnyWidgetView<()>>> {
        (0..n)
            .map(|i| {
                Box::new(
                    sized_box(label(format!("row {i}")))
                        .padding(crate::application::design::Space::Md),
                ) as Box<xilem::AnyWidgetView<()>>
            })
            .collect()
    }

    fn window_rect(widget: &WidgetRef<'_, dyn Widget>) -> Rect {
        widget
            .ctx()
            .window_transform()
            .transform_rect_bbox(widget.ctx().border_box())
    }

    /// The rows' top edges inside the list, in order; `None` for a row
    /// that is out of view.
    fn tops(root: &WidgetRef<'_, dyn Widget>) -> Vec<Option<f64>> {
        let origin = window_rect(root).y0;
        root.children()
            .into_iter()
            .map(|row| {
                let top = window_rect(&row).y0 - origin;
                (top > OUT_OF_VIEW / 2.0).then_some(top)
            })
            .collect()
    }

    #[test]
    fn the_newest_row_sits_on_the_bottom_and_a_cut_row_is_not_shown() {
        let view = tail_list(rows(20), 8.0);
        let mut ctx = crate::application::view::render::panel_resize_tests_support::context();
        let (pod, _) = View::<(), (), ViewCtx>::build(&view, &mut ctx, &mut ());
        let harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            pod.new_widget,
            (200, 150),
        );
        let root = harness.root_widget().as_dyn();
        let height = window_rect(&root).height();
        let tops = tops(&root);
        let shown: Vec<f64> = tops.iter().flatten().copied().collect();
        assert!(
            !shown.is_empty() && shown.len() < 20,
            "some rows fit, some do not: {tops:?}"
        );
        let last = *tops.last().unwrap();
        let row_height = root
            .children()
            .last()
            .map(|row| window_rect(row).height())
            .unwrap();
        assert_eq!(
            last,
            Some(height - row_height),
            "the newest row ends on the bottom edge"
        );
        assert!(
            shown.iter().all(|top| *top >= 0.0),
            "no shown row starts above the top: {shown:?}"
        );
        assert!(
            tops[..tops.len() - shown.len()].iter().all(Option::is_none),
            "the rows that do not fit are the oldest: {tops:?}"
        );
    }
}

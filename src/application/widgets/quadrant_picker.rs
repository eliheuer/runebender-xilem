// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The connected rule layer behind the coordinate-reference picker buttons.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerEvent, PropertiesMut,
    PropertiesRef, RegisterCtx, Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Line, Size, Stroke};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::application::view::design::{
    COORD_PICKER_EDGE, COORD_PICKER_GAP, COORD_PICKER_INSET, ControlSize,
};

const GRID_START: f64 = COORD_PICKER_INSET + ControlSize::Dot.px() / 2.0;
const GRID_STEP: f64 = ControlSize::Dot.px() + COORD_PICKER_GAP;

pub(crate) struct QuadrantGridWidget {
    color: Color,
    size: Size,
}

impl Widget for QuadrantGridWidget {
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
        Length::px(COORD_PICKER_EDGE)
    }

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        for index in 0..3 {
            let at = GRID_START + index as f64 * GRID_STEP;
            painter
                .stroke(
                    Line::new((GRID_START, at), (GRID_START + GRID_STEP * 2.0, at)),
                    &Stroke::new(1.0),
                    self.color,
                )
                .draw();
            painter
                .stroke(
                    Line::new((at, GRID_START), (at, GRID_START + GRID_STEP * 2.0)),
                    &Stroke::new(1.0),
                    self.color,
                )
                .draw();
        }
    }

    fn on_pointer_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &PointerEvent,
    ) {
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
}

pub(crate) struct QuadrantGrid {
    color: Color,
}

pub(crate) fn quadrant_grid(color: Color) -> QuadrantGrid {
    QuadrantGrid { color }
}

impl ViewMarker for QuadrantGrid {}

impl<State: 'static> View<State, (), ViewCtx> for QuadrantGrid {
    type Element = Pod<QuadrantGridWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        (
            ctx.create_pod(QuadrantGridWidget {
                color: self.color,
                size: Size::ZERO,
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.color != prev.color {
            element.widget.color = self.color;
            element.ctx.request_render();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        _message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        _state: &mut State,
    ) -> MessageResult<()> {
        MessageResult::Stale
    }
}

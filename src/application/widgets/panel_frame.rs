// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A decorative panel frame that leaves pointer input to the content underneath.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, PaintCtx, PropertiesRef, RegisterCtx, Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Insets, Rect, Shape, Size, Stroke, Vec2};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::application::view::design;

pub(crate) struct PanelFrameWidget {
    outline: Color,
    radius: f64,
    shadow: Option<Color>,
}

impl Widget for PanelFrameWidget {
    type Action = ();

    fn accepts_pointer_interaction(&self) -> bool {
        false
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        len_req: LenReq,
        _cross: Option<Length>,
    ) -> Length {
        match len_req {
            LenReq::FitContent(space) => space,
            _ => Length::ZERO,
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        let extent = if self.shadow.is_some() {
            design::PANEL_SHADOW_OFFSET
        } else {
            0.0
        };
        ctx.set_paint_insets(Insets::new(extent, 0.0, 0.0, extent));
        ctx.set_clip_path(Rect::new(-extent, 0.0, size.width, size.height + extent));
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let rect = ctx.content_box();
        if rect.width() <= 1.0 || rect.height() <= 1.0 {
            return;
        }
        let radius = self
            .radius
            .clamp(0.0, rect.width().min(rect.height()) / 2.0);
        if let Some(shadow) = self.shadow {
            let offset = design::PANEL_SHADOW_OFFSET;
            let mut outside =
                Rect::new(rect.x0 - offset, rect.y0, rect.x1, rect.y1 + offset).to_path(0.1);
            outside.extend(
                design::rounded_rect_path(rect, radius)
                    .reverse_subpaths()
                    .elements()
                    .iter()
                    .copied(),
            );
            // Paint only outside the panel face, including its rounded corner cutouts.
            painter.with_fill_clip(outside, |painter| {
                let shadow_rect = rect + Vec2::new(-offset, offset);
                painter
                    .fill(design::rounded_rect_path(shadow_rect, radius), shadow)
                    .draw();
            });
        }
        let edge = design::rounded_rect_path(rect.inset(-0.5), (radius - 0.5).max(0.0));
        painter
            .stroke(&edge, &Stroke::new(1.0), self.outline)
            .draw();
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

/// An input-transparent outline and shadow outside a separately clipped panel.
pub(crate) struct PanelFrame {
    outline: Color,
    radius: f64,
    shadow: Option<Color>,
}

pub(crate) fn panel_frame(outline: Color, radius: f64, shadow: Option<Color>) -> PanelFrame {
    PanelFrame {
        outline,
        radius,
        shadow,
    }
}

impl ViewMarker for PanelFrame {}

impl<State: 'static> View<State, (), ViewCtx> for PanelFrame {
    type Element = Pod<PanelFrameWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, Self::ViewState) {
        (
            ctx.create_pod(PanelFrameWidget {
                outline: self.outline,
                radius: self.radius,
                shadow: self.shadow,
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
        if self.outline != prev.outline || self.radius != prev.radius || self.shadow != prev.shadow
        {
            element.widget.outline = self.outline;
            element.widget.radius = self.radius;
            element.widget.shadow = self.shadow;
            if self.shadow.is_some() != prev.shadow.is_some() {
                element.ctx.request_layout();
            }
            element.ctx.request_render();
        }
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

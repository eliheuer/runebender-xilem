// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The letters of a labeled sample as a row of chips, in reading order.
//!
//! A chip is hollow until its letter has ink, then it takes the letter's color. The active
//! chip is larger. A click makes a letter active; the pointer over a chip lights the letter's
//! ink on the canvas. Words are separated by a wider gap.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx,
    Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Axis, Circle, Point, Size, Stroke};
use masonry::layout::{LenReq, Length};
use std::sync::Arc;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::application::widgets::text_label::{self, Anchor};

/// One letter of the sample.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Chip {
    pub character: char,
    pub color: Color,
    /// The letter owns some ink.
    pub done: bool,
    /// A space in the text comes before this letter.
    pub word_start: bool,
}

/// What the chips report upward.
#[derive(Debug)]
pub(crate) enum ChipEvent {
    Pick(usize),
    Hover(Option<usize>),
}

const CHIP: f64 = 28.0;
const GAP: f64 = 4.0;
const WORD_GAP: f64 = 14.0;
const ROW: f64 = CHIP + 6.0;

/// The colors a chip is drawn with, from the theme's mark treatment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChipInks {
    /// The letter on a colored chip.
    pub ink: Color,
    /// The letter and edge of a chip whose letter has no ink yet.
    pub hollow: Color,
    /// The face of such a chip.
    pub face: Color,
    /// The edge of every chip.
    pub outline: Color,
    /// The ring around the active chip.
    pub ring: Color,
}

pub(crate) struct LetterChipsWidget {
    chips: Arc<Vec<Chip>>,
    active: usize,
    inks: ChipInks,
    size: Size,
    hovered: Option<usize>,
}

impl LetterChipsWidget {
    /// The center of every chip at the current width, laid right to left, wrapping.
    fn centers(&self, width: f64) -> Vec<Point> {
        let mut centers = Vec::with_capacity(self.chips.len());
        let (mut x, mut y) = (width - CHIP / 2.0, ROW / 2.0);
        for (index, chip) in self.chips.iter().enumerate() {
            if index > 0 {
                x -= CHIP + if chip.word_start { WORD_GAP } else { GAP };
            }
            if x < CHIP / 2.0 {
                x = width - CHIP / 2.0;
                y += ROW;
            }
            centers.push(Point::new(x, y));
        }
        centers
    }

    fn rows(&self, width: f64) -> usize {
        let mut rows = 1;
        let mut last_y = ROW / 2.0;
        for center in self.centers(width) {
            if center.y > last_y {
                rows += 1;
                last_y = center.y;
            }
        }
        rows
    }

    fn chip_at(&self, at: Point) -> Option<usize> {
        self.centers(self.size.width)
            .iter()
            .position(|center| center.distance(at) <= CHIP / 2.0 + 2.0)
    }
}

impl Widget for LetterChipsWidget {
    type Action = ChipEvent;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        axis: Axis,
        len_req: LenReq,
        cross: Option<Length>,
    ) -> Length {
        match axis {
            Axis::Horizontal => match len_req {
                LenReq::FitContent(space) => space,
                _ => Length::px(CHIP),
            },
            Axis::Vertical => {
                let width = cross.map_or(240.0, Length::get);
                Length::px(ROW * self.rows(width) as f64)
            }
        }
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
        let centers = self.centers(self.size.width);
        for (index, (chip, center)) in self.chips.iter().zip(&centers).enumerate() {
            let active = index == self.active;
            let hovered = self.hovered == Some(index);
            let radius = CHIP / 2.0 - 3.0;
            let face = Circle::new(*center, radius);
            // Like the grid's mark swatches: a filled disc with a thin edge.
            if chip.done {
                painter.fill(face, chip.color).draw();
                painter
                    .stroke(face, &Stroke::new(1.0), self.inks.outline)
                    .draw();
            } else {
                painter.fill(face, self.inks.face).draw();
                painter
                    .stroke(face, &Stroke::new(1.0), self.inks.hollow)
                    .draw();
            }
            if active || hovered {
                painter
                    .stroke(
                        Circle::new(*center, radius + 3.0),
                        &Stroke::new(if active { 1.5 } else { 1.0 }),
                        self.inks.ring,
                    )
                    .draw();
            }
            let ink = if chip.done {
                self.inks.ink
            } else {
                self.inks.hollow
            };
            // Small, and a little above center: Arabic letters sit low in their box.
            text_label::draw(
                painter,
                Point::new(center.x, center.y - 1.0),
                &chip.character.to_string(),
                13.0,
                ink,
                Anchor::Middle,
            );
        }
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
                if let Some(index) = self.chip_at(ctx.local_position(state.position)) {
                    ctx.submit_action::<ChipEvent>(ChipEvent::Pick(index));
                    ctx.set_handled();
                }
            }
            PointerEvent::Move(update) => {
                let hovered = self.chip_at(ctx.local_position(update.current.position));
                if hovered != self.hovered {
                    self.hovered = hovered;
                    ctx.submit_action::<ChipEvent>(ChipEvent::Hover(hovered));
                    ctx.request_render();
                }
            }
            PointerEvent::Leave(_) if self.hovered.is_some() => {
                self.hovered = None;
                ctx.submit_action::<ChipEvent>(ChipEvent::Hover(None));
                ctx.request_render();
            }
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _props: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::HoveredChanged(false)) && self.hovered.is_some() {
            self.hovered = None;
            ctx.request_render();
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::ListBox
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

pub(crate) struct LetterChips<F> {
    chips: Arc<Vec<Chip>>,
    active: usize,
    inks: ChipInks,
    on_event: F,
}

pub(crate) fn letter_chips<F, Workspace: 'static>(
    chips: Arc<Vec<Chip>>,
    active: usize,
    inks: ChipInks,
    on_event: F,
) -> LetterChips<F>
where
    F: Fn(&mut Workspace, ChipEvent) + 'static,
{
    LetterChips {
        chips,
        active,
        inks,
        on_event,
    }
}

impl<F> ViewMarker for LetterChips<F> {}

impl<F, Workspace: 'static> View<Workspace, (), ViewCtx> for LetterChips<F>
where
    F: Fn(&mut Workspace, ChipEvent) + 'static,
{
    type Element = Pod<LetterChipsWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut Workspace) -> (Self::Element, Self::ViewState) {
        let widget = LetterChipsWidget {
            chips: self.chips.clone(),
            active: self.active,
            inks: self.inks,
            size: Size::ZERO,
            hovered: None,
        };
        (ctx.with_action_widget(|ctx| ctx.create_pod(widget)), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut Workspace,
    ) {
        let mut changed = false;
        if !Arc::ptr_eq(&self.chips, &prev.chips) && *self.chips != *prev.chips {
            element.widget.chips = self.chips.clone();
            element.ctx.request_layout();
            changed = true;
        }
        if self.active != prev.active {
            element.widget.active = self.active;
            changed = true;
        }
        if self.inks != prev.inks {
            element.widget.inks = self.inks;
            changed = true;
        }
        if changed {
            element.ctx.request_render();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        _element: Mut<'_, Self::Element>,
        app: &mut Workspace,
    ) -> MessageResult<()> {
        match message.take_message::<ChipEvent>() {
            Some(event) => {
                (self.on_event)(app, *event);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

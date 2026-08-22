// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The glyph grid: one canvas island that paints every visible cell.
//!
//! Following runebender-gpui's lesson, this is one widget that paints all
//! cells into one scene, not a widget per cell. It owns scroll offset and
//! selection, and reports open/select events to the app.

use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PointerScrollEvent, PropertiesMut,
    PropertiesRef, RegisterCtx, ScrollDelta, Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, Point, Rect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::model::FontModel;
use crate::theme::Palette;

const CELL: f64 = 84.0;
const GAP: f64 = 8.0;
const PAD: f64 = 12.0;

/// One drawable cell: what the grid needs without touching the font model.
#[derive(Clone)]
pub struct Cell {
    pub index: usize,
    pub name: Arc<str>,
    pub codepoint: Option<char>,
    pub outline: Arc<masonry::kurbo::BezPath>,
    pub ink: Rect,
    pub advance: f64,
    pub mark: Option<Color>,
}

/// The vertical metrics the cell preview is scaled against.
#[derive(Clone, Copy)]
pub struct CellMetrics {
    pub ascender: f64,
    pub descender: f64,
    pub upm: f64,
}

pub fn cells_of(font: &FontModel, palette: &Palette) -> Vec<Cell> {
    font.glyphs
        .iter()
        .enumerate()
        .map(|(index, g)| Cell {
            index,
            name: Arc::from(g.name.as_str()),
            codepoint: g.codepoint,
            outline: g.outline.clone(),
            ink: g.ink,
            advance: g.advance,
            mark: g.mark.as_deref().and_then(|m| palette.mark(m)),
        })
        .collect()
}

/// What the grid reports upward.
#[derive(Debug)]
pub enum GridEvent {
    Selected(usize),
    Open(usize),
}

pub struct GridWidget {
    cells: Arc<Vec<Cell>>,
    metrics: CellMetrics,
    palette: Arc<Palette>,
    selected: Option<usize>,
    scroll: f64,
    size: Size,
}

impl GridWidget {
    fn columns(&self) -> usize {
        (((self.size.width - 2.0 * PAD + GAP) / (CELL + GAP)).floor() as usize).max(1)
    }

    fn rows(&self) -> usize {
        self.cells.len().div_ceil(self.columns())
    }

    fn content_height(&self) -> f64 {
        2.0 * PAD + self.rows() as f64 * (CELL + GAP) - GAP
    }

    fn max_scroll(&self) -> f64 {
        (self.content_height() - self.size.height).max(0.0)
    }

    fn cell_rect(&self, index: usize) -> Rect {
        let cols = self.columns();
        let (col, row) = (index % cols, index / cols);
        let x = PAD + col as f64 * (CELL + GAP);
        let y = PAD + row as f64 * (CELL + GAP) - self.scroll;
        Rect::new(x, y, x + CELL, y + CELL)
    }

    fn cell_at(&self, p: Point) -> Option<usize> {
        let cols = self.columns();
        if p.x < PAD || p.x > self.size.width - PAD {
            return None;
        }
        let col = ((p.x - PAD) / (CELL + GAP)).floor() as usize;
        if col >= cols {
            return None;
        }
        let row = ((p.y + self.scroll - PAD) / (CELL + GAP)).floor() as usize;
        let index = row * cols + col;
        if index < self.cells.len() && self.cell_rect(index).contains(p) {
            Some(index)
        } else {
            None
        }
    }
}

impl Widget for GridWidget {
    type Action = GridEvent;

    fn accepts_focus(&self) -> bool {
        true
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _ctx: &mut MeasureCtx<'_>,
        _props: &PropertiesRef<'_>,
        _axis: Axis,
        len_req: LenReq,
        _cross_length: Option<Length>,
    ) -> Length {
        match len_req {
            LenReq::FitContent(space) => space,
            _ => Length::px(200.0),
        }
    }

    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        self.size = size;
        self.scroll = self.scroll.clamp(0.0, self.max_scroll());
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let pal = &self.palette;
        painter.fill_rect(self.size.to_rect(), pal.app);

        let cols = self.columns();
        let first_row = ((self.scroll - PAD) / (CELL + GAP)).floor().max(0.0) as usize;
        let last_row = ((self.scroll + self.size.height - PAD) / (CELL + GAP)).ceil() as usize;
        let cell_border = pal.role("readonlyPoint");
        let glyph_fill = pal.text;

        for row in first_row..=last_row {
            for col in 0..cols {
                let index = row * cols + col;
                let Some(cell) = self.cells.get(index) else {
                    continue;
                };
                let rect = self.cell_rect(index);
                let selected = self.selected == Some(index);

                // Cell background and border.
                painter.fill(rounded(rect, 6.0), pal.panel).draw();
                let border = if selected {
                    pal.role("gridSelected")
                } else {
                    cell.mark.unwrap_or(cell_border)
                };
                painter
                    .stroke(rounded(rect, 6.0), &Stroke::new(if selected { 2.0 } else { 1.0 }), border)
                    .draw();

                // Glyph preview: fit the em box into the upper part of the cell.
                if !cell.outline.elements().is_empty() {
                    let preview = fit_transform(rect, cell.advance, &self.metrics);
                    let outline = preview * (*cell.outline).clone();
                    painter.fill(&outline, glyph_fill.with_alpha(0.9)).draw();
                }
            }
        }

        // Scrollbar.
        let max = self.max_scroll();
        if max > 0.0 {
            let track_h = self.size.height;
            let thumb_h = (track_h * track_h / self.content_height()).max(24.0);
            let thumb_y = (self.scroll / max) * (track_h - thumb_h);
            let x = self.size.width - 6.0;
            painter
                .fill(rounded(Rect::new(x, thumb_y, x + 4.0, thumb_y + thumb_h), 2.0), pal.text_muted.with_alpha(0.5))
                .draw();
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
                ctx.request_focus();
                let at = ctx.local_position(state.position);
                if let Some(index) = self.cell_at(at) {
                    let reopen = self.selected == Some(index);
                    self.selected = Some(index);
                    ctx.submit_action::<GridEvent>(GridEvent::Selected(index));
                    if reopen {
                        ctx.submit_action::<GridEvent>(GridEvent::Open(index));
                    }
                    ctx.request_render();
                }
                ctx.set_handled();
            }
            PointerEvent::Scroll(PointerScrollEvent { delta, .. }) => {
                let dy = match delta {
                    ScrollDelta::PixelDelta(p) => p.y,
                    ScrollDelta::LineDelta(_, y) => f64::from(*y) * (CELL + GAP),
                    _ => 0.0,
                };
                let next = (self.scroll - dy).clamp(0.0, self.max_scroll());
                if next != self.scroll {
                    self.scroll = next;
                    ctx.request_render();
                }
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn accessibility(&mut self, _ctx: &mut AccessCtx<'_>, _props: &PropertiesRef<'_>, node: &mut Node) {
        node.set_description(format!("Glyph grid, {} glyphs", self.cells.len()));
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

fn rounded(rect: Rect, r: f64) -> masonry::kurbo::RoundedRect {
    rect.to_rounded_rect(r)
}

/// Map design space into a cell: fit the em box (advance wide, ascender..descender tall)
/// with a margin, Y-flipped, biased toward the top so descenders have room.
fn fit_transform(cell: Rect, advance: f64, m: &CellMetrics) -> Affine {
    let margin = 12.0;
    let inner = Rect::new(
        cell.x0 + margin,
        cell.y0 + margin,
        cell.x1 - margin,
        cell.y1 - margin,
    );
    let em_w = advance.max(m.upm * 0.5);
    let em_h = m.ascender - m.descender;
    let scale = (inner.width() / em_w).min(inner.height() / em_h);
    let baseline_y = inner.y0 + (m.ascender / em_h) * inner.height();
    let x0 = inner.x0 + (inner.width() - em_w * scale) / 2.0;
    Affine::new([scale, 0.0, 0.0, -scale, x0, baseline_y])
}

// ---------------------------------------------------------------------------
// View wrapper.

pub struct GridView<F> {
    cells: Arc<Vec<Cell>>,
    metrics: CellMetrics,
    palette: Arc<Palette>,
    selected: Option<usize>,
    on_event: F,
}

pub fn grid<F, App: 'static>(
    cells: Arc<Vec<Cell>>,
    metrics: CellMetrics,
    palette: Arc<Palette>,
    selected: Option<usize>,
    on_event: F,
) -> GridView<F>
where
    F: Fn(&mut App, GridEvent) + 'static,
{
    GridView {
        cells,
        metrics,
        palette,
        selected,
        on_event,
    }
}

impl<F> ViewMarker for GridView<F> {}
impl<F, App: 'static> View<App, (), ViewCtx> for GridView<F>
where
    F: Fn(&mut App, GridEvent) + 'static,
{
    type Element = Pod<GridWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut App) -> (Self::Element, Self::ViewState) {
        let widget = GridWidget {
            cells: self.cells.clone(),
            metrics: self.metrics,
            palette: self.palette.clone(),
            selected: self.selected,
            scroll: 0.0,
            size: Size::ZERO,
        };
        (ctx.with_action_widget(|ctx| ctx.create_pod(widget)), ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        (): &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut App,
    ) {
        let mut changed = false;
        if !Arc::ptr_eq(&self.cells, &prev.cells) {
            element.widget.cells = self.cells.clone();
            element.widget.scroll = 0.0;
            changed = true;
        }
        if self.selected != prev.selected {
            element.widget.selected = self.selected;
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
        app: &mut App,
    ) -> MessageResult<()> {
        match message.take_message::<GridEvent>() {
            Some(event) => {
                (self.on_event)(app, *event);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

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
use crate::text_label::{self, Anchor};
use crate::theme::Palette;

const GAP: f64 = 8.0;
const PAD: f64 = 12.0;

/// Column span for a glyph, from name length and advance/upm (matches gpui).
fn column_span(name: &str, advance: f64, upm: f64) -> usize {
    let name_span = match name.chars().count() {
        0..=14 => 1,
        15..=26 => 2,
        _ => 3,
    };
    let ratio = if upm > 0.0 { advance / upm } else { 0.0 };
    let width_span = if ratio <= 1.5 { 1 } else if ratio <= 2.8 { 2 } else if ratio <= 4.0 { 3 } else { 4 };
    name_span.max(width_span)
}

/// Pack (cell-index, span) items into rows of `cols` columns; the last cell
/// of each row grows to fill the remainder (matches gpui `pack_spans`).
fn pack_spans(spans: &[(usize, usize)], cols: usize) -> Vec<Vec<(usize, usize)>> {
    let cols = cols.max(1);
    let mut rows: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut row: Vec<(usize, usize)> = Vec::new();
    let mut used = 0usize;
    for &(item, span) in spans {
        let span = span.clamp(1, cols);
        if used + span > cols && !row.is_empty() {
            if let Some(last) = row.last_mut() {
                last.1 += cols - used;
            }
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        row.push((item, span));
        used += span;
        if used == cols {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
    }
    if !row.is_empty() {
        if let Some(last) = row.last_mut() {
            last.1 += cols - used;
        }
        rows.push(row);
    }
    rows
}

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
    /// Target cell edge length in px.
    pub cell: f64,
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
    Selected { index: usize, cmd: bool, shift: bool },
    Open(usize),
}

pub struct GridWidget {
    cells: Arc<Vec<Cell>>,
    metrics: CellMetrics,
    palette: Arc<Palette>,
    selected: Option<usize>,
    multi: std::sync::Arc<std::collections::HashSet<usize>>,
    scroll: f64,
    size: Size,
}

impl GridWidget {
    fn columns(&self) -> usize {
        (((self.size.width - 2.0 * PAD + GAP) / (self.metrics.cell + GAP)).floor() as usize).max(1)
    }

    fn cell_width(&self, span: usize) -> f64 {
        self.metrics.cell * span as f64 + GAP * (span.saturating_sub(1)) as f64
    }

    /// Packed rows of (cell-index-in-self.cells, span).
    fn packed(&self) -> Vec<Vec<(usize, usize)>> {
        let spans: Vec<(usize, usize)> = self
            .cells
            .iter()
            .enumerate()
            .map(|(i, c)| (i, column_span(&c.name, c.advance, self.metrics.upm)))
            .collect();
        pack_spans(&spans, self.columns())
    }

    fn row_pitch(&self) -> f64 {
        self.metrics.cell + GAP
    }

    fn content_height(&self, rows: usize) -> f64 {
        2.0 * PAD + rows as f64 * self.row_pitch() - GAP
    }

    fn max_scroll(&self, rows: usize) -> f64 {
        (self.content_height(rows) - self.size.height).max(0.0)
    }
}

impl GridWidget {
    fn cell_index_at(&self, p: Point) -> Option<usize> {
        if p.x < PAD || p.y < 0.0 {
            return None;
        }
        let pitch = self.row_pitch();
        let r = ((p.y + self.scroll - PAD) / pitch).floor();
        if r < 0.0 {
            return None;
        }
        let rows = self.packed();
        let row = rows.get(r as usize)?;
        let row_y = PAD + r as usize as f64 * pitch - self.scroll;
        if p.y > row_y + self.metrics.cell {
            return None;
        }
        let mut x = PAD;
        for &(ci, span) in row {
            let w = self.cell_width(span);
            if p.x >= x && p.x <= x + w {
                return self.cells.get(ci).map(|c| c.index);
            }
            x += w + GAP;
        }
        None
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
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let pal = &self.palette;
        painter.fill_rect(self.size.to_rect(), pal.app);

        let rows = self.packed();
        let total = rows.len();
        self.scroll = self.scroll.clamp(0.0, self.max_scroll(total));
        let pitch = self.row_pitch();
        let cell_border = pal.role("readonlyPoint");
        let glyph_fill = pal.text;

        for (r, row) in rows.iter().enumerate() {
            let y = PAD + r as f64 * pitch - self.scroll;
            if y + self.metrics.cell < 0.0 || y > self.size.height {
                continue;
            }
            let mut x = PAD;
            for &(ci, span) in row {
                let w = self.cell_width(span);
                let rect = Rect::new(x, y, x + w, y + self.metrics.cell);
                x += w + GAP;
                let Some(cell) = self.cells.get(ci) else { continue };
                let selected = self.selected == Some(cell.index);
                let multi = self.multi.contains(&cell.index);

                let bg = if multi { pal.role("gridSelected").with_alpha(0.18) } else { pal.panel };
                painter.fill(rect.to_rounded_rect(6.0), bg).draw();
                let border = if selected || multi { pal.role("gridSelected") } else { cell.mark.unwrap_or(cell_border) };
                painter.stroke(rect.to_rounded_rect(6.0), &Stroke::new(if selected || multi { 2.0 } else { 1.0 }), border).draw();

                let preview_rect = Rect::new(rect.x0, rect.y0, rect.x1, rect.y1 - 30.0);
                if !cell.outline.elements().is_empty() {
                    let preview = fit_transform(preview_rect, cell.advance, &self.metrics);
                    let outline = preview * (*cell.outline).clone();
                    // Fill the glyph with its mark colour (gpui), so the grid
                    // reads by category; selected cells use the ring colour,
                    // unmarked glyphs the default glyph fill.
                    let glyph_color = if selected || multi {
                        pal.role("gridSelected")
                    } else {
                        cell.mark.unwrap_or(glyph_fill)
                    };
                    painter.fill(&outline, glyph_color).draw();
                }
                // Two stacked, left-aligned lines (gpui's cell-labels box):
                // the glyph name on top, its U+XXXX below in muted text.
                let muted = self.palette.text_muted;
                let name_color = if selected || multi {
                    pal.role("gridSelected")
                } else {
                    cell.mark.unwrap_or(self.palette.text)
                };
                let has_uni = cell.codepoint.is_some();
                let name_y = if has_uni { rect.y1 - 20.0 } else { rect.y1 - 9.0 };
                text_label::draw(painter, Point::new(rect.x0 + 8.0, name_y), &cell.name, 10.0, name_color, Anchor::Start);
                if let Some(cp) = cell.codepoint {
                    text_label::draw(painter, Point::new(rect.x0 + 8.0, rect.y1 - 8.0), &format!("U+{:04X}", cp as u32), 10.0, muted, Anchor::Start);
                }
            }
        }

        let max = self.max_scroll(total);
        if max > 0.0 {
            let track_h = self.size.height;
            let thumb_h = (track_h * track_h / self.content_height(total)).max(24.0);
            let thumb_y = (self.scroll / max) * (track_h - thumb_h);
            let sx = self.size.width - 6.0;
            painter.fill(Rect::new(sx, thumb_y, sx + 4.0, thumb_y + thumb_h).to_rounded_rect(2.0), pal.text_muted.with_alpha(0.5)).draw();
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
                if let Some(index) = self.cell_index_at(at) {
                    let cmd = state.modifiers.meta() || state.modifiers.ctrl();
                    let shift = state.modifiers.shift();
                    let reopen = self.selected == Some(index) && !cmd && !shift;
                    self.selected = Some(index);
                    ctx.submit_action::<GridEvent>(GridEvent::Selected { index, cmd, shift });
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
                    ScrollDelta::LineDelta(_, y) => f64::from(*y) * (self.metrics.cell + GAP),
                    _ => 0.0,
                };
                let total = self.packed().len();
                let next = (self.scroll - dy).clamp(0.0, self.max_scroll(total));
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
    multi: std::sync::Arc<std::collections::HashSet<usize>>,
    on_event: F,
}

pub fn grid<F, App: 'static>(
    cells: Arc<Vec<Cell>>,
    metrics: CellMetrics,
    palette: Arc<Palette>,
    selected: Option<usize>,
    multi: std::sync::Arc<std::collections::HashSet<usize>>,
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
        multi,
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
            multi: self.multi.clone(),
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
        if !Arc::ptr_eq(&self.multi, &prev.multi) {
            element.widget.multi = self.multi.clone();
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

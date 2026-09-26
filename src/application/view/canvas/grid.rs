// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The glyph grid: one canvas island that paints every visible cell.
//!
//! This is one widget that paints all
//! cells into one scene, not a widget per cell. It owns scroll offset and
//! selection, and reports open/select events to the app.

use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayoutCtx, MeasureCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PointerScrollEvent, PropertiesMut, PropertiesRef,
    RegisterCtx, ScrollDelta, TextEvent, Widget,
};
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, Axis, Point, Rect, Shape, Size, Stroke};
use masonry::layout::{LenReq, Length};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Color, Pod, ViewCtx};

use crate::application::font_model::FontModel;
use crate::application::view::design::{
    GRID_CELL_SELECTED_SHADOW_OFFSET, GRID_CELL_SHADOW_OFFSET, Stroke as DesignStroke,
};
use crate::application::view::render::px32;
use crate::application::view::theme::Palette;
use crate::application::widgets::text_label::{self, Anchor};
use runebender::outline::glyph_paths::round_units;

const GAP: f64 = 8.0;
/// The label block has a little air over the first line, close line spacing,
/// and the same inset below as at the sides.
const LABEL_TOP: f64 = 6.0;
const LABEL_BOTTOM: f64 = 4.0;
const LABEL_GAP: f64 = 0.0;

/// Caption size, line count, and total height for a base-width cell.
///
/// This is intentionally independent of a particular glyph: GPUI reserves
/// the same two-line block for every full-size cell, leaving the Unicode line
/// empty for an unencoded glyph, so outlines do not jump between neighbours.
fn cell_label_metrics(width: f64, captions: bool, detail: bool) -> (f64, usize, f64) {
    let (size, lines) = if !captions || width < 48.0 {
        (0.0, 0)
    } else if width < 90.0 {
        (13.0, 1)
    } else {
        (13.0, if detail { 3 } else { 2 })
    };
    let line = (size * 1.10_f64).ceil();
    let height = if lines == 0 {
        0.0
    } else {
        LABEL_TOP + line * lines as f64 + LABEL_GAP * (lines - 1) as f64 + LABEL_BOTTOM
    };
    (size, lines, height)
}

/// Column span for a glyph, from name length and advance/upm (matches gpui).
fn column_span(name: &str, advance: f64, upm: f64) -> usize {
    let name_span = match name.chars().count() {
        0..=14 => 1,
        15..=26 => 2,
        _ => 3,
    };
    let ratio = if upm > 0.0 { advance / upm } else { 0.0 };
    let width_span = if ratio <= 1.5 {
        1
    } else if ratio <= 2.8 {
        2
    } else if ratio <= 4.0 {
        3
    } else {
        4
    };
    name_span.max(width_span)
}

/// Pack (cell-index, span) items into rows of `cols` columns; the last cell
/// of each row grows to fill the remainder (matches gpui `pack_spans`).
fn pack_spans(spans: &[(usize, usize)], cols: usize) -> Vec<Vec<(usize, usize)>> {
    let cols = cols.max(1);
    let mut rows: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut row: Vec<(usize, usize)> = Vec::new();
    let mut used = 0_usize;
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
pub(crate) struct Cell {
    pub index: usize,
    pub name: Arc<str>,
    pub codepoint: Option<char>,
    pub outline: Arc<kurbo::BezPath>,
    pub advance: f64,
    pub mark: Option<Color>,
}

/// The vertical metrics the cell preview is scaled against.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct CellMetrics {
    /// Target cell edge length in px.
    pub cell: f64,
    /// Insets around this grid. The editor rail is intentionally denser
    /// than the overview, matching GPUI's compact thumbnail index.
    pub padding: f64,
    /// Vertical grid inset, independent from the horizontal rail inset.
    pub padding_y: f64,
    /// Whether each row reserves a caption band below its thumbnail.
    pub captions_below: bool,
    pub ascender: f64,
    pub descender: f64,
    pub upm: f64,
    /// Detail view: a third line under the name with the category and advance.
    ///
    /// This rides in the metrics rather than being its own parameter
    /// because adding one field to the widget means editing the widget
    /// struct, the view struct, `build`, `rebuild` and the constructor's
    /// signature, in five places, for a bool.
    pub detail: bool,
}

pub(crate) fn cells_of(font: &FontModel, palette: &Palette) -> Vec<Cell> {
    font.glyphs
        .iter()
        .enumerate()
        .map(|(index, g)| Cell {
            index,
            name: Arc::from(g.name.as_str()),
            codepoint: g.codepoint,
            outline: g.outline.clone(),
            advance: g.advance,
            mark: g.mark.as_deref().and_then(|m| palette.mark(m)),
        })
        .collect()
}

/// What the grid reports upward.
#[derive(Debug)]
pub(crate) enum GridEvent {
    Selected {
        index: usize,
        cmd: bool,
        shift: bool,
    },
    Open(usize),
}

pub(crate) struct GridWidget {
    cells: Arc<Vec<Cell>>,
    metrics: CellMetrics,
    palette: Arc<Palette>,
    selected: Option<usize>,
    multi: Arc<std::collections::HashSet<usize>>,
    scroll: f64,
    /// Unconsumed logical-pixel travel from a high-resolution trackpad.
    ///
    /// Pixel scroll events arrive much more frequently than mouse-wheel line
    /// events. Accumulating them prevents every tiny event from becoming a
    /// complete row jump while retaining the grid's row-snapped viewport.
    pixel_scroll_remainder: f64,
    size: Size,
    /// The pointer is over the grid, which is when the scroll thumb shows.
    hovered: bool,
}

impl GridWidget {
    fn columns(&self) -> usize {
        let ideal =
            (self.size.width - 2.0 * self.metrics.padding + GAP) / (self.metrics.cell + GAP);
        // The compact rail fits the nearest count instead of dropping a column
        // whenever its target size is a few pixels larger than the fitted size.
        let count = if self.metrics.captions_below {
            ideal.floor()
        } else {
            ideal.round()
        };
        usize::try_from(round_units(count)).unwrap_or(1).max(1)
    }

    fn cell_width(&self, span: usize) -> f64 {
        // Distribute the remainder across columns in both overview and rail.
        // Painting and pointer hit testing use this same fitted width.
        let columns = self.columns() as f64;
        let edge = ((self.size.width - 2.0 * self.metrics.padding - GAP * (columns - 1.0))
            / columns)
            .max(1.0);
        let edge = if self.metrics.captions_below {
            edge
        } else {
            edge.floor()
        };
        edge * span as f64 + GAP * (span.saturating_sub(1)) as f64
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
        self.cell_height() + GAP
    }

    /// The overview reserves its two-line name/codepoint caption below
    /// the thumbnail, as GPUI's grid fit does. The editor rail is a
    /// thumbnail index, so its short cells do not carry that band.
    fn cell_height(&self) -> f64 {
        let caption = cell_label_metrics(
            self.cell_width(1),
            self.metrics.captions_below,
            self.metrics.detail,
        )
        .2;
        let target = self.cell_width(1) + caption;
        let available = (self.size.height - 2.0 * self.metrics.padding_y).max(target);
        let ideal_rows = (available + GAP) / (target + GAP);
        let rows = if self.metrics.captions_below {
            ideal_rows.floor()
        } else {
            ideal_rows.round()
        }
        .max(1.0);
        ((available - GAP * (rows - 1.0)) / rows).floor().max(1.0)
    }

    fn inset_x(&self) -> f64 {
        if self.metrics.captions_below {
            return self.metrics.padding;
        }
        let width = self.columns() as f64 * (self.cell_width(1) + GAP) - GAP;
        ((self.size.width - width) / 2.0).floor().max(0.0)
    }

    fn inset_y(&self) -> f64 {
        if self.metrics.captions_below {
            return self.metrics.padding_y;
        }
        let rows = ((self.size.height - 2.0 * self.metrics.padding_y + GAP) / self.row_pitch())
            .round()
            .max(1.0);
        ((self.size.height - (rows * self.row_pitch() - GAP)) / 2.0)
            .floor()
            .max(0.0)
    }

    fn content_height(&self, rows: usize) -> f64 {
        2.0 * self.inset_y() + rows as f64 * self.row_pitch() - GAP
    }

    fn visible_rows(&self) -> usize {
        let available = (self.size.height - 2.0 * self.inset_y()).max(0.0);
        usize::try_from(round_units(((available + GAP) / self.row_pitch()).floor()))
            .unwrap_or(1)
            .max(1)
    }

    fn scroll_row(&self) -> usize {
        usize::try_from(round_units(self.scroll / self.row_pitch())).unwrap_or(0)
    }

    fn max_scroll_row(&self, rows: usize) -> usize {
        rows.saturating_sub(self.visible_rows())
    }

    fn max_scroll(&self, rows: usize) -> f64 {
        self.max_scroll_row(rows) as f64 * self.row_pitch()
    }

    fn set_scroll_row(&mut self, row: usize, rows: usize) {
        self.scroll = row.min(self.max_scroll_row(rows)) as f64 * self.row_pitch();
    }

    /// Move by complete rows, matching GPUI's row-quantized wheel behavior.
    fn scroll_rows(&mut self, delta_y: f64, rows: usize) -> bool {
        if delta_y == 0.0 {
            return false;
        }
        let visible = self.visible_rows();
        let step = usize::try_from(round_units(
            (delta_y / self.row_pitch()).abs().ceil().max(1.0),
        ))
        .unwrap_or(1)
        .clamp(1, visible);
        let current = self.scroll_row();
        let next = if delta_y > 0.0 {
            current.saturating_sub(step)
        } else {
            current.saturating_add(step).min(self.max_scroll_row(rows))
        };
        self.set_scroll_row(next, rows);
        next != current
    }

    /// Accumulate high-resolution trackpad travel before moving complete rows.
    fn scroll_pixels(&mut self, delta_y: f64, rows: usize) -> bool {
        if !delta_y.is_finite() || delta_y == 0.0 {
            return false;
        }
        // Do not make a new gesture in the opposite direction first cancel a
        // stale fraction of a row from the previous gesture.
        if self.pixel_scroll_remainder != 0.0
            && self.pixel_scroll_remainder.signum() != delta_y.signum()
        {
            self.pixel_scroll_remainder = 0.0;
        }
        self.pixel_scroll_remainder += delta_y;

        let pitch = self.row_pitch().max(1.0);
        let step = usize::try_from(round_units(
            (self.pixel_scroll_remainder.abs() / pitch).floor(),
        ))
        .unwrap_or(0)
        .min(self.visible_rows());
        if step == 0 {
            return false;
        }

        let consumed = self.pixel_scroll_remainder.signum() * step as f64 * pitch;
        self.pixel_scroll_remainder -= consumed;
        let changed = self.scroll_rows(consumed, rows);
        if !changed {
            // Momentum at an edge must not leave a large hidden balance that
            // makes the next gesture in the other direction feel unresponsive.
            self.pixel_scroll_remainder = 0.0;
        }
        changed
    }

    /// Replace the display cells, resetting the viewport only when their order changes.
    ///
    /// `filtered_cells` can return a new `Arc` on a Xilem rebuild. Pointer identity
    /// therefore cannot distinguish a routine rebuild from a changed filter, and using
    /// it as that distinction made every selection update snap the grid back to the top.
    fn update_cells(&mut self, cells: Arc<Vec<Cell>>) {
        let order_changed = self.cells.len() != cells.len()
            || self
                .cells
                .iter()
                .zip(cells.iter())
                .any(|(old, new)| old.index != new.index);
        self.cells = cells;
        if order_changed {
            self.scroll = 0.0;
            self.pixel_scroll_remainder = 0.0;
        }
    }

    /// Move the primary selection in display order and keep it fully visible.
    fn step_selection(&mut self, offset: isize) -> Option<usize> {
        let current = self
            .selected
            .and_then(|selected| self.cells.iter().position(|cell| cell.index == selected))?;
        let target = current
            .saturating_add_signed(offset)
            .min(self.cells.len().saturating_sub(1));
        if target == current {
            return None;
        }
        self.pixel_scroll_remainder = 0.0;
        let index = self.cells[target].index;
        self.selected = Some(index);
        self.multi = Arc::default();

        let rows = self.packed();
        if let Some(row) = rows
            .iter()
            .position(|packed| packed.iter().any(|(cell, _)| *cell == target))
        {
            let first = self.scroll_row();
            let visible = self.visible_rows();
            if row < first {
                self.set_scroll_row(row, rows.len());
            } else if row >= first + visible {
                self.set_scroll_row(row + 1 - visible, rows.len());
            }
        }
        Some(index)
    }
}

impl GridWidget {
    fn cell_index_at(&self, p: Point) -> Option<usize> {
        let outside_rows = p.y < self.inset_y() || p.y >= self.size.height - self.inset_y();
        if p.x < self.inset_x() || outside_rows {
            return None;
        }
        let pitch = self.row_pitch();
        let r = ((p.y + self.scroll - self.inset_y()) / pitch).floor();
        if r < 0.0 {
            return None;
        }
        let rows = self.packed();
        let row_index = usize::try_from(round_units(r)).ok()?;
        let row = rows.get(row_index)?;
        let row_y = self.inset_y() + r * pitch - self.scroll;
        if p.y > row_y + self.cell_height() {
            return None;
        }
        let mut x = self.inset_x();
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

    fn layout(&mut self, _ctx: &mut LayoutCtx<'_>, _props: &PropertiesRef<'_>, size: Size) {
        if self.size != size {
            self.pixel_scroll_remainder = 0.0;
        }
        let scroll_row = self.scroll_row();
        self.size = size;
        self.set_scroll_row(scroll_row, self.packed().len());
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let rows = self.packed();
        let total = rows.len();
        self.set_scroll_row(self.scroll_row(), total);
        let pal = &self.palette;
        painter.fill_rect(self.size.to_rect(), pal.grid_bg());

        // Keep cell paint inside the fitted-row area without clipping the
        // grid's own ground. A widget-level clip also clipped that ground,
        // letting the parent panel show as mismatched strips above and below.
        let inset = self.inset_y();
        painter.push_fill_clip(Rect::new(
            0.0,
            inset,
            self.size.width,
            (self.size.height - inset).max(inset),
        ));

        let pitch = self.row_pitch();
        // A marked cell is filled with its mark and keylined; its glyph and labels are drawn in the
        // theme's mark ink. A selected cell inverts.
        let cell_border = pal.outline;
        let glyph_fill = pal.text;
        let mark_outline = pal.mark_outline.unwrap_or(cell_border);
        let mark_ink = pal.mark_ink.unwrap_or(glyph_fill);

        for (r, row) in rows.iter().enumerate() {
            let y = self.inset_y() + r as f64 * pitch - self.scroll;
            if y + self.cell_height() < 0.0 || y > self.size.height {
                continue;
            }
            let mut x = self.inset_x();
            for &(ci, span) in row {
                let w = self.cell_width(span);
                let rect = Rect::new(x, y, x + w, y + self.cell_height());
                x += w + GAP;
                let Some(cell) = self.cells.get(ci) else {
                    continue;
                };
                let selected = self.selected == Some(cell.index);
                let multi = self.multi.contains(&cell.index);

                // Selected and multi-selected read the same: one fill, one
                // ring, and one width.
                let picked = selected || multi;
                let bg = if picked {
                    pal.selected_bg()
                } else {
                    cell.mark.unwrap_or(pal.panel)
                };
                let ink = if picked {
                    pal.selected_content_ink()
                } else if cell.mark.is_some() {
                    mark_ink
                } else {
                    glyph_fill
                };
                // A hard lower-left shadow lifts each tile from the recessed
                // grid ground. Selection gets one extra pixel without changing
                // the tile's layout or hit geometry.
                let shadow_offset = if picked {
                    GRID_CELL_SELECTED_SHADOW_OFFSET
                } else {
                    GRID_CELL_SHADOW_OFFSET
                };
                painter
                    .fill(
                        rect + kurbo::Vec2::new(-shadow_offset, shadow_offset),
                        pal.cell_shadow(),
                    )
                    .draw();
                // The reference cells are square. Encoding the square as a
                // zero-radius `RoundedRect` made Vello CPU lose later
                // same-colour outline/text draws in the Gray theme.
                painter.fill(rect, bg).draw();
                let border = if picked {
                    pal.selected_bg()
                } else if cell.mark.is_some() {
                    mark_outline
                } else {
                    cell_border
                };
                // Keep the keyline inside the cell, like a layout border.
                // A centered stroke at the edge blurs into the surrounding gap.
                let width = DesignStroke::Hairline.px();
                let half = width / 2.0;
                let keyline = Rect::new(
                    rect.x0 + half,
                    rect.y0 + half,
                    rect.x1 - half,
                    rect.y1 - half,
                );
                painter.stroke(keyline, &Stroke::new(width), border).draw();

                // The label block is sized from what it draws: under 34px wide a cell is a thumbnail
                // with no text, under 90px it carries its name only, and
                // above that the name and the codepoint.
                // One type size, the interface's: a cell too narrow to
                // carry a name at it carries none.
                let (label_size, label_lines, block) = cell_label_metrics(
                    self.cell_width(1),
                    self.metrics.captions_below,
                    self.metrics.detail,
                );
                let line = (label_size * 1.10).ceil();

                let preview_rect = Rect::new(rect.x0, rect.y0, rect.x1, rect.y1 - block);
                if !cell.outline.elements().is_empty() {
                    let preview =
                        fit_transform(preview_rect, cell.outline.bounding_box(), self.metrics.upm);
                    let outline = preview * (*cell.outline).clone();
                    // Fill the glyph with its mark colour (gpui), so the grid
                    // reads by category; selected cells use the ring colour,
                    // unmarked glyphs the default glyph fill.
                    painter.fill(&outline, ink).draw();
                }
                if label_lines == 0 {
                    continue;
                }
                let muted = if picked || cell.mark.is_some() {
                    ink
                } else {
                    self.palette.text_muted
                };
                painter.push_fill_clip(Rect::new(
                    rect.x0 + 1.0,
                    rect.y0,
                    rect.x1 - 1.0,
                    rect.y1 - 1.0,
                ));
                let name_color = ink;
                let top = rect.y1 - block + LABEL_TOP;
                // Baseline inside its own line box, not the box edge.
                let baseline = |n: f64| top + (line + LABEL_GAP) * n + line * 0.5;
                text_label::draw(
                    painter,
                    Point::new(rect.x0 + LABEL_TOP, baseline(0.0)),
                    &cell.name,
                    px32(label_size),
                    name_color,
                    Anchor::Start,
                );
                if label_lines > 1
                    && let Some(cp) = cell.codepoint
                {
                    text_label::draw(
                        painter,
                        Point::new(rect.x0 + LABEL_TOP, baseline(1.0)),
                        &format!("U+{:04X}", cp as u32),
                        px32(label_size),
                        muted,
                        Anchor::Start,
                    );
                }
                if label_lines > 2 {
                    let category = cell
                        .codepoint
                        .map(|c| {
                            runebender::analysis::category::GlyphCategory::from_codepoint(c)
                                .display_name()
                        })
                        .unwrap_or("Unencoded");
                    text_label::draw(
                        painter,
                        Point::new(rect.x0 + LABEL_TOP, baseline(2.0)),
                        &format!("{category} \u{00b7} {:.0}", cell.advance),
                        px32(label_size),
                        muted,
                        Anchor::Start,
                    );
                }
                painter.pop_clip();
            }
        }

        // The thumb shows while the pointer is over the grid, the way
        // the portals' bars do now, and the GPUI grid has none at all.
        let max = self.max_scroll(total);
        if max > 0.0 && self.hovered {
            let track_h = self.size.height;
            let thumb_h = (track_h * track_h / self.content_height(total)).max(24.0);
            let thumb_y = (self.scroll / max) * (track_h - thumb_h);
            let sx = self.size.width - 6.0;
            painter
                .fill(
                    Rect::new(sx, thumb_y, sx + 4.0, thumb_y + thumb_h).to_rounded_rect(2.0),
                    pal.text_muted.with_alpha(0.5),
                )
                .draw();
        }
        painter.pop_clip();
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Enter(_) => {
                self.hovered = true;
                ctx.request_render();
            }
            PointerEvent::Leave(_) => {
                self.hovered = false;
                ctx.request_render();
            }
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
                let total = self.packed().len();
                let changed = match delta {
                    // ui-events specifies these in physical pixels. Grid
                    // geometry is logical, so normalize for Retina displays
                    // before accumulating the gesture.
                    ScrollDelta::PixelDelta(p) => {
                        self.scroll_pixels(p.y / ctx.scale_factor(), total)
                    }
                    ScrollDelta::LineDelta(_, y) => {
                        self.pixel_scroll_remainder = 0.0;
                        self.scroll_rows(f64::from(*y) * self.row_pitch(), total)
                    }
                    _ => false,
                };
                if changed {
                    ctx.request_render();
                }
                ctx.set_handled();
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down
            || key.modifiers.meta()
            || key.modifiers.ctrl()
            || key.modifiers.alt()
            || key.modifiers.shift()
        {
            return;
        }
        let columns = isize::try_from(self.columns()).unwrap_or(isize::MAX);
        let offset = match key.key {
            Key::Named(NamedKey::ArrowLeft) => -1,
            Key::Named(NamedKey::ArrowRight) => 1,
            Key::Named(NamedKey::ArrowUp) => -columns,
            Key::Named(NamedKey::ArrowDown) => columns,
            _ => return,
        };
        if let Some(index) = self.step_selection(offset) {
            ctx.submit_action::<GridEvent>(GridEvent::Selected {
                index,
                cmd: false,
                shift: false,
            });
            ctx.request_render();
        }
        // Once the grid owns focus, arrows belong to its spatial selection even
        // at an outer edge; do not let an edge press trigger a parent shortcut.
        ctx.set_handled();
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_description(format!("Glyph grid, {} glyphs", self.cells.len()));
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

/// GPUI thumbnail placement: retain em-relative sizes while centering ink.
/// Expand the em window for tall marks rather than clipping their outlines.
fn fit_transform(cell: Rect, ink: Rect, upm: f64) -> Affine {
    const EM_FILL: f64 = 0.65;
    const BASELINE_FROM_TOP: f64 = 0.8;
    const THUMBNAIL_FILL: f64 = 0.92;
    let em_height = upm.max(1.0) / EM_FILL;
    let em_top = -BASELINE_FROM_TOP * em_height;
    let top = em_top.min(-ink.y1);
    let bottom = (em_top + em_height).max(-ink.y0);
    let scale = (cell.width() / ink.width().max(1.0)).min(cell.height() / (bottom - top).max(1.0))
        * THUMBNAIL_FILL;
    let x = cell.x0 + (cell.width() - ink.width() * scale) / 2.0 - ink.x0 * scale;
    let y = cell.y0 + (cell.height() - ink.height() * scale) / 2.0 + ink.y1 * scale;
    Affine::new([scale, 0.0, 0.0, -scale, x, y])
}

// ---------------------------------------------------------------------------
// View wrapper.

pub(crate) struct GridView<F> {
    cells: Arc<Vec<Cell>>,
    metrics: CellMetrics,
    palette: Arc<Palette>,
    selected: Option<usize>,
    multi: Arc<std::collections::HashSet<usize>>,
    on_event: F,
}

pub(crate) fn grid<F, Workspace: 'static>(
    cells: Arc<Vec<Cell>>,
    metrics: CellMetrics,
    palette: Arc<Palette>,
    selected: Option<usize>,
    multi: Arc<std::collections::HashSet<usize>>,
    on_event: F,
) -> GridView<F>
where
    F: Fn(&mut Workspace, GridEvent) + 'static,
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
impl<F, Workspace: 'static> View<Workspace, (), ViewCtx> for GridView<F>
where
    F: Fn(&mut Workspace, GridEvent) + 'static,
{
    type Element = Pod<GridWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut Workspace) -> (Self::Element, Self::ViewState) {
        let widget = GridWidget {
            cells: self.cells.clone(),
            metrics: self.metrics,
            palette: self.palette.clone(),
            selected: self.selected,
            multi: self.multi.clone(),
            scroll: 0.0,
            pixel_scroll_remainder: 0.0,
            size: Size::ZERO,
            hovered: false,
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
        if self.metrics != prev.metrics {
            element.widget.metrics = self.metrics;
            element.widget.scroll = 0.0;
            element.widget.pixel_scroll_remainder = 0.0;
            // The rail's clip inset depends on the fitted thumbnail size.
            element.ctx.request_layout();
            changed = true;
        }
        if !Arc::ptr_eq(&self.palette, &prev.palette) {
            element.widget.palette = self.palette.clone();
            changed = true;
        }
        if !Arc::ptr_eq(&self.cells, &prev.cells) {
            element.widget.update_cells(self.cells.clone());
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
        app: &mut Workspace,
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

#[cfg(test)]
mod thumbnail_tests {
    use super::*;

    fn rail() -> GridWidget {
        GridWidget {
            cells: Arc::new(
                (0..100)
                    .map(|index| Cell {
                        index,
                        name: Arc::from(format!("glyph{index}")),
                        codepoint: None,
                        outline: Arc::new(kurbo::BezPath::new()),
                        advance: 500.0,
                        mark: None,
                    })
                    .collect(),
            ),
            metrics: CellMetrics {
                cell: crate::application::view::design::RAIL_CELL_SIZE,
                padding: crate::application::view::design::RAIL_GRID_INSET,
                padding_y: crate::application::view::design::RAIL_GRID_INSET,
                captions_below: false,
                ascender: 800.0,
                descender: -200.0,
                upm: 1000.0,
                detail: false,
            },
            palette: Arc::new(Palette::load("gray")),
            selected: None,
            multi: Arc::default(),
            scroll: 0.0,
            pixel_scroll_remainder: 0.0,
            size: Size::new(246.0, 538.0),
            hovered: false,
        }
    }

    #[test]
    fn compact_rail_fits_five_columns_and_hit_tests_every_visible_cell() {
        let grid = rail();
        assert_eq!(grid.columns(), 5);
        assert_eq!(grid.cell_width(1), 40.0);
        assert_eq!(grid.row_pitch(), 48.0);
        assert_eq!((grid.inset_x(), grid.inset_y()), (7.0, 9.0));
        for row in 0..11 {
            for col in 0..5 {
                let p = Point::new(27.0 + f64::from(col) * 48.0, 29.0 + f64::from(row) * 48.0);
                assert_eq!(grid.cell_index_at(p), Some((row * 5 + col) as usize));
            }
        }
        assert_eq!(grid.cell_index_at(Point::new(51.0, 29.0)), None);
        assert_eq!(grid.cell_index_at(Point::new(27.0, 53.0)), None);
        assert_eq!(grid.cell_index_at(Point::new(27.0, 535.0)), None);
    }

    #[test]
    fn rail_size_and_scrolling_keep_painted_cells_clickable() {
        let mut grid = rail();
        for target in [24.0, 44.0, 96.0] {
            grid.metrics.cell = target;
            grid.scroll = grid.max_scroll(grid.packed().len());
            let rows = grid.packed();
            let mut x = grid.inset_x();
            let y = grid.inset_y() + (rows.len() - 1) as f64 * grid.row_pitch() - grid.scroll;
            for &(ci, span) in rows.last().unwrap() {
                let width = grid.cell_width(span);
                assert_eq!(
                    grid.cell_index_at(Point::new(x + width / 2.0, y + grid.cell_height() / 2.0)),
                    Some(ci)
                );
                x += width + GAP;
            }
        }
    }

    #[test]
    fn trackpad_scroll_accumulates_before_moving_a_row_and_survives_rebuild() {
        use masonry::core::pointer::{PointerId, PointerInfo, PointerState, PointerType};
        use masonry::core::{NewWidget, PointerEvent, PointerScrollEvent, ScrollDelta};
        use masonry::dpi::PhysicalPosition;
        use masonry_testing::TestHarness;

        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            NewWidget::new(rail()),
            (246, 538),
        );
        let state = PointerState {
            position: PhysicalPosition::new(100.0, 100.0),
            ..PointerState::default()
        };
        let pitch = harness.edit_root_widget(|root| root.widget.row_pitch());
        for event in 0..3 {
            harness.process_pointer_event(PointerEvent::Scroll(PointerScrollEvent {
                pointer: PointerInfo {
                    pointer_id: Some(PointerId::PRIMARY),
                    persistent_device_id: None,
                    pointer_type: PointerType::Mouse,
                },
                delta: ScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -17.0)),
                state: state.clone(),
            }));
            if event < 2 {
                assert_eq!(
                    harness.edit_root_widget(|root| root.widget.scroll),
                    0.0,
                    "a sub-row trackpad delta must not become a whole-row jump"
                );
            }
        }
        assert_eq!(harness.edit_root_widget(|root| root.widget.scroll), pitch);

        harness.edit_root_widget(|root| {
            let equivalent = Arc::new(root.widget.cells.as_ref().clone());
            root.widget.update_cells(equivalent);
        });
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.scroll),
            pitch,
            "a routine Xilem rebuild must not snap the trackpad viewport to the top"
        );
    }

    #[test]
    fn mouse_wheel_lines_still_move_immediately() {
        use masonry::core::pointer::{PointerId, PointerInfo, PointerState, PointerType};
        use masonry::core::{NewWidget, PointerEvent, PointerScrollEvent, ScrollDelta};
        use masonry_testing::TestHarness;

        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            NewWidget::new(rail()),
            (246, 538),
        );
        harness.process_pointer_event(PointerEvent::Scroll(PointerScrollEvent {
            pointer: PointerInfo {
                pointer_id: Some(PointerId::PRIMARY),
                persistent_device_id: None,
                pointer_type: PointerType::Mouse,
            },
            delta: ScrollDelta::LineDelta(0.0, -1.0),
            state: PointerState::default(),
        }));
        let pitch = harness.edit_root_widget(|root| root.widget.row_pitch());
        assert_eq!(harness.edit_root_widget(|root| root.widget.scroll), pitch);
    }

    #[test]
    fn focused_grid_arrows_move_selection_and_reveal_the_next_row() {
        use masonry::core::keyboard::{Key, NamedKey};
        use masonry::core::{NewWidget, TextEvent};
        use masonry_testing::TestHarness;

        let mut widget = rail();
        widget.selected = Some(50);
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            NewWidget::new(widget),
            (246, 538),
        );
        let id = harness.root_widget().id();
        harness.focus_on(Some(id));

        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        let (action, action_id) = harness.pop_action::<GridEvent>().expect("selection action");
        assert_eq!(action_id, id);
        assert!(matches!(
            action,
            GridEvent::Selected {
                index: 55,
                cmd: false,
                shift: false
            }
        ));
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.selected),
            Some(55)
        );
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.scroll),
            harness.edit_root_widget(|root| root.widget.row_pitch()),
            "keyboard reveal must also land on a complete row"
        );

        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowLeft)));
        assert!(matches!(
            harness.pop_action::<GridEvent>().map(|pair| pair.0),
            Some(GridEvent::Selected { index: 54, .. })
        ));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowUp)));
        assert!(matches!(
            harness.pop_action::<GridEvent>().map(|pair| pair.0),
            Some(GridEvent::Selected { index: 49, .. })
        ));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowRight)));
        assert!(matches!(
            harness.pop_action::<GridEvent>().map(|pair| pair.0),
            Some(GridEvent::Selected { index: 50, .. })
        ));
    }

    #[test]
    fn grid_ground_covers_fitted_top_and_bottom_margins() {
        use masonry::core::NewWidget;
        use masonry_testing::TestHarness;

        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            NewWidget::new(rail()),
            (246, 538),
        );
        let rendered = harness.render();
        // x=1 stays outside the fitted cells, so all three samples are the
        // grid ground. A widget-level clip used to expose the parent surface
        // at the first and last samples.
        let interior = rendered.get_pixel(1, 20);
        assert_eq!(rendered.get_pixel(1, 1), interior);
        assert_eq!(rendered.get_pixel(1, 536), interior);
    }

    #[test]
    fn thumbnail_ink_is_centered_and_tall_marks_stay_inside() {
        let cell = Rect::new(10.0, 20.0, 110.0, 120.0);
        for ink in [
            Rect::new(80.0, 0.0, 680.0, 700.0),
            Rect::new(-100.0, 900.0, 100.0, 2400.0),
        ] {
            let transform = fit_transform(cell, ink, 1000.0);
            let drawn = transform.transform_rect_bbox(ink);
            assert!((drawn.center() - cell.center()).hypot() < 1e-9);
            assert!(cell.contains(drawn.origin()));
            assert!(drawn.x1 <= cell.x1 && drawn.y1 <= cell.y1);
        }
        let period = fit_transform(cell, Rect::new(0.0, 0.0, 100.0, 100.0), 1000.0)
            .transform_rect_bbox(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert!(period.height() < cell.height() * 0.1);
    }

    #[test]
    fn caption_thresholds_match_the_gpui_grid() {
        assert_eq!(cell_label_metrics(47.0, true, false), (0.0, 0, 0.0));
        assert_eq!(cell_label_metrics(48.0, true, false), (13.0, 1, 25.0));
        assert_eq!(cell_label_metrics(89.0, true, false), (13.0, 1, 25.0));
        assert_eq!(cell_label_metrics(90.0, true, false), (13.0, 2, 40.0));
        assert_eq!(cell_label_metrics(90.0, true, true), (13.0, 3, 55.0));
        assert_eq!(cell_label_metrics(200.0, false, true), (0.0, 0, 0.0));
    }

    #[test]
    fn spanning_and_row_packing_match_the_reference_rules() {
        assert_eq!(column_span("short", 500.0, 1000.0), 1);
        assert_eq!(column_span("fifteen-letters!", 500.0, 1000.0), 2);
        assert_eq!(column_span("short", 3000.0, 1000.0), 3);
        assert_eq!(column_span("short", 5000.0, 1000.0), 4);
        assert_eq!(
            pack_spans(&[(0, 1), (1, 2), (2, 2), (3, 1)], 4),
            vec![vec![(0, 1), (1, 3)], vec![(2, 2), (3, 2)]]
        );
    }
}

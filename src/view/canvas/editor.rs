// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The glyph editor island: a canvas widget that owns the edit session
//! and gesture state, and the view that hosts it.

use std::sync::Arc;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, EventCtx, LayerType, LayoutCtx, MeasureCtx, NewWidget, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PointerScrollEvent, PointerUpdate,
    PropertiesMut, PropertiesRef, RegisterCtx, ScrollDelta, TextEvent, Widget, WidgetId,
};
use masonry::imaging::Painter;
use masonry::kurbo;
use masonry::kurbo::{Axis, Circle, Line, Point, Rect, Size, Stroke};
use masonry::layout::{LenReq, Length};
use runebender_core::outline::glyph_ops::PointId;
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::Tool;
use crate::Workspace;
use crate::edit::session::Session;
use crate::view::theme::Palette;
use crate::widgets::context_menu::{ContextMenu, MenuAction, MenuRow, MenuTarget};
use crate::widgets::text_label::{self, Anchor};

use crate::view::design::{
    ANCHOR_DIAMOND_SCALE, METRICS_CARD_BOTTOM, METRICS_CARD_HEADER as PANEL_HEADER,
    METRICS_CARD_HEIGHT as PANEL_HEIGHT, METRICS_CARD_INSET as PANEL_PAD,
    METRICS_CARD_WIDTH as PANEL_WIDTH, METRICS_FIELD_GAP, METRICS_FIELD_HEIGHT as PANEL_ROW,
    METRICS_FIELD_START, METRICS_FIELD_WIDTH, POINT_CORNER_RADIUS, POINT_CURVE_RADIUS,
    POINT_HALO_EXTRA, POINT_RING_WIDTH, POINT_SELECTED_GROW, Radius, START_ARROW_OFFSET,
    START_ARROW_RADIUS, Stroke as DesignStroke, TextSize, point_marker_scale,
};

const HIT_RADIUS_PX: f64 = 8.0;

/// Context-menu items: (label, op). Op returns whether the glyph changed.
/// The right-click menu's rows. Shared with the layer that draws them.
const MENU_ITEMS: &[MenuRow] = &[
    MenuRow {
        label: std::borrow::Cow::Borrowed("Add Anchor"),
        action: MenuAction::AddAnchor,
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Set Start Point"),
        action: MenuAction::Op(|s| s.set_start()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Round Corners"),
        action: MenuAction::Op(|s| s.round_corners()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Reverse Contours"),
        action: MenuAction::Op(|s| s.reverse()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Remove Overlap"),
        action: MenuAction::Op(|s| s.remove_overlap()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Flip Horizontal"),
        action: MenuAction::Op(|s| s.flip_horizontal()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Flip Vertical"),
        action: MenuAction::Op(|s| s.flip_vertical()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Rotate 90"),
        action: MenuAction::Op(|s| s.rotate_90()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Duplicate"),
        action: MenuAction::Op(|s| s.duplicate()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Harmonize"),
        action: MenuAction::Op(|s| s.harmonize()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Balance"),
        action: MenuAction::Op(|s| s.balance()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Optimize"),
        action: MenuAction::Op(|s| s.optimize()),
    },
    MenuRow {
        label: std::borrow::Cow::Borrowed("Decompose"),
        action: MenuAction::Op(|s| s.decompose()),
    },
];

impl EditorWidget {
    /// Apply a context-menu choice. Called from the menu layer, which is
    /// not a child of this widget and so cannot reach it with an action.
    pub(crate) fn apply_menu_choice(
        this: &mut masonry::core::WidgetMut<'_, Self>,
        action: MenuAction,
        at: Point,
    ) {
        let changed = match action {
            MenuAction::AddAnchor => {
                this.widget.session.add_anchor(at.x.round(), at.y.round());
                true
            }
            MenuAction::Op(op) => op(&mut this.widget.session),
            // Not this canvas's menu.
            MenuAction::AddNode(_) => false,
        };
        this.widget.menu = None;
        if changed {
            this.ctx.submit_action::<EditorEvent>(EditorEvent::Edited);
        }
        this.ctx.request_render();
    }

    /// The menu closed without a choice.
    pub(crate) fn forget_menu(this: &mut masonry::core::WidgetMut<'_, Self>) {
        this.widget.menu = None;
    }
}

/// What the editor reports upward.
#[derive(Debug)]
pub(crate) enum EditorEvent {
    /// The glyph changed; the app should refresh its cached preview.
    Edited,
    /// Selection changed; carries how many points are selected.
    Selection(usize),
    /// The text tool activated a sort: open that glyph for editing.
    EditGlyph(String),
    /// The committed logical text changed; park it with the active tab.
    TextChanged(String),
    /// Cmd+Z: the app undoes on the master's pile.
    Undo,
    /// Cmd+Shift+Z or Cmd+Y.
    Redo,
}

enum Drag {
    None,
    Points {
        start: Point,
    },
    Pan {
        last: Point,
    },
    /// Pen mouse-down at `origin` (design space); becomes handle-drag past a threshold.
    Pen {
        origin: Point,
        dragging: bool,
    },
    /// Rubber-band selection in screen space.
    Marquee {
        start: Point,
        current: Point,
        additive: bool,
    },
    /// Drawing a shape; endpoints in design space.
    Shape {
        start: Point,
        current: Point,
    },
    /// Dragging an anchor by index.
    Anchor {
        idx: usize,
    },
    /// Dragging one top-level component; `last` is in design space.
    Component {
        last: Point,
    },
    /// Dragging the advance (right sidebearing) line.
    AdvanceLine,
    /// Dragging the left sidebearing line; carries the last cursor x (screen).
    LeftLine {
        last_x: f64,
    },
}

pub(crate) struct EditorWidget {
    session: Session,
    palette: Arc<Palette>,
    /// The glyph's kerning groups, shown in the floating metrics card.
    groups: (String, String),
    /// A glyph mark colors the card header, as it does in GPUI.
    mark: Option<xilem::Color>,
    tool: Tool,
    ghosts: Arc<Vec<kurbo::BezPath>>,
    /// Read-only interpolated instance overlay at the current axis location.
    interp: Option<Arc<kurbo::BezPath>>,
    /// Background layer and reference glyph, drawn under everything.
    underlay: Underlay,
    /// The text tool's buffer, present only while that tool is in hand.
    text: Option<crate::edit::text_tool::TextState>,
    /// The master the buffer was built from.
    text_inputs: Option<crate::edit::text_tool::TextInputs>,
    size: Size,
    drag: Drag,
    /// Last cursor position in design space, for the pen preview segment.
    hover: Option<Point>,
    /// The open context-menu layer, if there is one.
    menu: Option<WidgetId>,
    view: ViewOptions,
    /// The metric box being typed into, if any, and what has been typed.
    ///
    /// This is a hand-written text field. The panel it lives in is
    /// painted rather than composed, so it cannot hold Xilem's
    /// `text_input`: there is no view for a floating panel, and the
    /// `zstack` that would have composed one never finished compiling.
    /// Three numbers over the drawing therefore cost an editing mode.
    field: Option<MetricField>,
    field_buf: String,
}

/// Which number in the metrics panel is being typed into.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricField {
    Lsb,
    Width,
    Rsb,
}

impl EditorWidget {
    /// The metrics panel that floats over the drawing, centered at its bottom.
    ///
    /// The GPUI build has this, and it is a large part of why that
    /// editor reads better: the numbers you are working on sit with the
    /// drawing instead of in a column at the side.
    ///
    /// This is painted rather than composed. The view-land way is a
    /// `zstack` around the editor pane, and that one extra container
    /// around an application-sized view tree took the build from under a
    /// minute to over thirty-five, at which point it was killed rather
    /// than finished. Painting it here costs one method and no types.
    /// Where the three metric boxes are, in widget coordinates.
    ///
    /// Paint and hit testing both read this, so the box you click is the
    /// box you can see. Writing it twice is how a painted control drifts.
    fn metric_boxes(&self) -> Option<[(MetricField, Rect); 3]> {
        self.session.side_bearings()?;
        let (left, top) = self.metrics_panel_origin()?;
        let y = top + PANEL_HEADER + PANEL_PAD + DesignStroke::Hairline.px();
        let box_at = |column: f64| {
            let x = left + METRICS_FIELD_START + column * (METRICS_FIELD_WIDTH + METRICS_FIELD_GAP);
            Rect::new(x, y, x + METRICS_FIELD_WIDTH, y + PANEL_ROW)
        };
        Some([
            (MetricField::Lsb, box_at(0.0)),
            (MetricField::Width, box_at(1.0)),
            (MetricField::Rsb, box_at(2.0)),
        ])
    }

    /// The panel's top left corner, or `None` when it does not fit.
    fn metrics_panel_origin(&self) -> Option<(f64, f64)> {
        let top = self.size.height - PANEL_HEIGHT - METRICS_CARD_BOTTOM;
        if top < 0.0 || PANEL_WIDTH + PANEL_PAD * 2.0 > self.size.width {
            return None;
        }
        Some(((self.size.width - PANEL_WIDTH) / 2.0, top))
    }

    /// Type a key into the focused metric box. Returns whether the glyph
    /// changed, and whether the key was ours.
    fn metric_key(&mut self, key: &masonry::core::KeyboardEvent) -> (bool, bool) {
        let Some(field) = self.field else {
            return (false, false);
        };
        match &key.key {
            Key::Character(typed) => {
                for character in typed.chars() {
                    if character.is_ascii_digit() || character == '-' {
                        self.field_buf.push(character);
                    }
                }
                (false, true)
            }
            Key::Named(NamedKey::Backspace) => {
                self.field_buf.pop();
                (false, true)
            }
            Key::Named(NamedKey::Escape) => {
                self.field = None;
                (false, true)
            }
            Key::Named(NamedKey::Tab) => {
                let next = match field {
                    MetricField::Lsb => MetricField::Width,
                    MetricField::Width => MetricField::Rsb,
                    MetricField::Rsb => MetricField::Lsb,
                };
                let edited = self.commit_metric();
                self.focus_metric(next);
                (edited, true)
            }
            Key::Named(NamedKey::Enter) => {
                let edited = self.commit_metric();
                self.field = None;
                (edited, true)
            }
            _ => (false, false),
        }
    }

    /// Put the caret in a box and seed it with the value it shows.
    fn focus_metric(&mut self, field: MetricField) {
        let Some(sb) = self.session.side_bearings() else {
            return;
        };
        self.field_buf = match field {
            MetricField::Lsb => sb.lsb.to_string(),
            MetricField::Width => format!("{:.0}", sb.advance),
            MetricField::Rsb => sb.rsb.to_string(),
        };
        self.field = Some(field);
    }

    /// Apply what was typed. The sidebearing rules are the ones the
    /// inspector's fields use: the left one moves the outline, the right
    /// one moves the advance.
    fn commit_metric(&mut self) -> bool {
        let Some(field) = self.field else {
            return false;
        };
        let Ok(value) = self.field_buf.trim().parse::<f64>() else {
            return false;
        };
        let Some(sb) = self.session.side_bearings() else {
            return false;
        };
        match field {
            MetricField::Lsb => self.session.shift_glyph(value - sb.min_x),
            MetricField::Width => self.session.set_advance(value),
            MetricField::Rsb => self.session.set_advance(sb.max_x + value),
        }
        true
    }

    fn paint_metrics(&self, painter: &mut Painter<'_>) {
        const PAD: f64 = PANEL_PAD;
        let pal = &self.palette;
        let bearings = self.session.side_bearings();
        // Keep read-only group labels inside their end columns. Full names
        // remain available in the inspector's kerning group fields.
        let group_label = |name: &str| {
            let name = name
                .strip_prefix("public.kern1.")
                .or_else(|| name.strip_prefix("public.kern2."))
                .unwrap_or(name);
            if name.chars().count() > 6 {
                format!("{}…", name.chars().take(5).collect::<String>())
            } else {
                name.to_owned()
            }
        };
        let Some((left, top)) = self.metrics_panel_origin() else {
            return;
        };
        let width = PANEL_WIDTH;
        let frame =
            Rect::new(left, top, left + width, top + PANEL_HEIGHT).to_rounded_rect(Radius::Sm.px());
        painter.fill(frame, pal.panel).draw();
        painter
            .stroke(
                frame,
                &Stroke::new(DesignStroke::Hairline.px()),
                pal.outline,
            )
            .draw();
        // The reference's neutral header keeps glyph marks in the navigation rail.
        painter
            .stroke(
                Line::new(
                    (left, top + PANEL_HEADER),
                    (left + width, top + PANEL_HEADER),
                ),
                &Stroke::new(DesignStroke::Hairline.px()),
                pal.outline,
            )
            .draw();

        let header_text = |painter: &mut Painter<'_>, x: f64, s: &str, size: f32, color, anchor| {
            text_label::draw(
                painter,
                Point::new(left + x, top + PANEL_HEADER / 2.0),
                s,
                size,
                color,
                anchor,
            );
        };
        header_text(
            painter,
            PAD,
            &self.session.glyph_name,
            TextSize::Body.px(),
            pal.text,
            Anchor::Start,
        );
        // The codepoint, right aligned on the same line, as the GPUI
        // build has it. This one is free because the session carries the
        // glyph; the kerning groups it also shows are not, because they
        // live in the font model, and reaching them means threading two
        // strings through the widget, the view, `build`, `rebuild` and
        // the constructor.
        if let Some(codepoint) = self.session.glyph.codepoints.iter().next() {
            header_text(
                painter,
                width - PAD,
                &format!("{:04X}", codepoint as u32),
                TextSize::Body.px(),
                pal.text,
                Anchor::End,
            );
        }
        if let Some(sb) = &bearings {
            // Three boxes you can type in, like the GPUI build's. Each
            // one is drawn here and hit tested from the same rectangles,
            // because a painted control that computes its geometry twice
            // will drift the moment either copy is edited.
            if let Some(boxes) = self.metric_boxes() {
                for (field, rect) in boxes {
                    let focused = self.field == Some(field);
                    let value = if focused {
                        self.field_buf.clone()
                    } else {
                        match field {
                            MetricField::Lsb => sb.lsb.to_string(),
                            MetricField::Width => format!("{:.0}", sb.advance),
                            MetricField::Rsb => sb.rsb.to_string(),
                        }
                    };
                    let border = if focused { pal.text } else { pal.field_outline };
                    painter.fill(rect, pal.field()).draw();
                    // Like the native inputs, the field's keyline stays inside
                    // its bounds. A centered exterior stroke blurs the edge.
                    let width = DesignStroke::Hairline.px();
                    let half = width / 2.0;
                    let keyline = Rect::new(
                        rect.x0 + half,
                        rect.y0 + half,
                        rect.x1 - half,
                        rect.y1 - half,
                    );
                    painter.stroke(keyline, &Stroke::new(width), border).draw();
                    let baseline = rect.center().y;
                    text_label::draw(
                        painter,
                        Point::new(
                            rect.x0 + crate::view::design::INPUT_HORIZONTAL_INSET,
                            baseline,
                        ),
                        &value,
                        TextSize::Body.px(),
                        pal.text,
                        Anchor::Start,
                    );
                    if focused {
                        // A caret, drawn by hand, because this is a text
                        // field drawn by hand.
                        let caret =
                            Rect::new(rect.x1 - 4.0, rect.y0 + 3.0, rect.x1 - 3.0, rect.y1 - 3.0);
                        painter.fill(caret, pal.role("textCursor")).draw();
                    }
                }
            }
            if let Some(boxes) = self.metric_boxes() {
                let first = boxes[0].1;
                let last = boxes[2].1;
                for (x, label, anchor) in [
                    (left + PAD, "LSB", Anchor::Start),
                    (left + width - PAD, "RSB", Anchor::End),
                ] {
                    text_label::draw(
                        painter,
                        Point::new(x, first.center().y),
                        label,
                        TextSize::Body.px(),
                        pal.text_muted,
                        anchor,
                    );
                }
                // Group names retain their own row rather than replacing the
                // sidebearing labels. Full names remain in the inspector.
                for (rect, group) in [(first, &self.groups.0), (last, &self.groups.1)] {
                    text_label::draw(
                        painter,
                        Point::new(
                            rect.center().x,
                            rect.y1 + METRICS_FIELD_GAP + PANEL_ROW / 2.0,
                        ),
                        &group_label(group),
                        TextSize::Body.px(),
                        pal.text_muted,
                        Anchor::Middle,
                    );
                }
            }
        }
    }

    fn screen_points(&self) -> Vec<(PointId, Point, bool, bool, bool)> {
        let affine = self.session.viewport.affine();
        self.session
            .points()
            .into_iter()
            .map(|p| (p.id, affine * p.point, p.on_curve, p.smooth, p.start))
            .collect()
    }

    /// Closed contours expose their first on-curve point and outgoing direction.
    /// Open paths and contours without an on-curve point have no start marker.
    fn start_markers(&self) -> Vec<(PointId, Point, Point)> {
        let affine = self.session.viewport.affine();
        self.session
            .glyph
            .contours
            .iter()
            .enumerate()
            .filter_map(|(ci, contour)| {
                if contour.points.first()?.typ == norad::PointType::Move {
                    return None;
                }
                let pi = contour
                    .points
                    .iter()
                    .position(|p| p.typ != norad::PointType::OffCurve)?;
                let start = &contour.points[pi];
                let next = &contour.points[(pi + 1) % contour.points.len()];
                let from = affine * Point::new(start.x, start.y);
                let to = affine * Point::new(next.x, next.y);
                (from.distance(to) > 0.001).then_some(((ci, pi), from, to))
            })
            .collect()
    }

    fn hit_point(&self, at: Point) -> Option<PointId> {
        self.screen_points()
            .into_iter()
            .filter(|(_, sp, _, _, _)| sp.distance(at) <= HIT_RADIUS_PX)
            .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
            .map(|(id, _, _, _, _)| id)
    }

    fn fit(&mut self) {
        let m = self.session.metrics;
        self.session.viewport.fit_to_canvas(
            self.size.width,
            self.size.height,
            self.session.advance(),
            m.ascender,
            m.descender,
            0.62,
        );
        self.session.fitted = true;
    }

    /// Frame the whole text line rather than one glyph.
    ///
    /// The editor's normal fit is one advance wide, which shows about two
    /// letters of a word. Typing is for judging spacing, so the view has
    /// to hold the line.
    fn fit_text(&mut self) {
        let Some(text) = &self.text else { return };
        let m = self.session.metrics;
        let width: f64 = text
            .placed()
            .iter()
            .map(|sort| sort.origin.x + sort.advance)
            .fold(self.session.advance(), f64::max);
        // Width first, and only then height. `fit_to_canvas` sizes to the
        // em, which is right for one glyph and wrong for a word: a line
        // of fifteen letters would fit vertically and run off both sides.
        let width = width.max(m.upm);
        let design_height = (m.ascender - m.descender).max(1.0);
        let zoom = ((self.size.width * 0.9) / width).min((self.size.height * 0.8) / design_height);
        self.session.viewport.zoom = zoom.max(0.001);
        let center_y = (m.ascender + m.descender) / 2.0;
        self.session.viewport.offset = kurbo::Vec2::new(
            (self.size.width - width * self.session.viewport.zoom) / 2.0,
            self.size.height / 2.0 + center_y * self.session.viewport.zoom,
        );
        self.session.fitted = true;
    }

    fn emit(&self, ctx: &mut EventCtx<'_>, edited: bool) {
        if edited {
            ctx.submit_action::<EditorEvent>(EditorEvent::Edited);
        }
        ctx.submit_action::<EditorEvent>(EditorEvent::Selection(self.session.selection.len()));
        ctx.request_render();
    }

    fn emit_text_changed(&self, ctx: &mut EventCtx<'_>) {
        if let Some(text) = &self.text {
            ctx.submit_action::<EditorEvent>(EditorEvent::TextChanged(text.buffer.text()));
        }
    }
}

/// A grid line index from an already rounded coordinate. The visible
/// canvas is a few thousand units at most, so the cast cannot truncate.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the visible canvas spans a few thousand units"
)]
fn grid_index(v: f64) -> i64 {
    v as i64
}

impl Widget for EditorWidget {
    type Action = EditorEvent;

    fn accepts_focus(&self) -> bool {
        true
    }

    fn accepts_text_input(&self) -> bool {
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
        if self.size != size {
            self.size = size;
            if !self.session.fitted {
                // The text tool frames the line; everything else frames
                // the glyph.
                if self.tool == Tool::Text && self.text.is_some() {
                    self.fit_text();
                } else {
                    self.fit();
                }
            }
        }
        ctx.set_clip_path(size.to_rect());
    }

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let pal = self.palette.clone();
        let affine = self.session.viewport.affine();
        painter.fill_rect(self.size.to_rect(), pal.canvas);

        // The text tool draws a line of glyphs rather than one glyph.
        // The active sort is the one being edited, so it keeps the
        // editing colour and the rest are quiet.
        if self.tool == Tool::Text
            && let Some(text) = &self.text
        {
            let m = &self.session.metrics;
            let ink = pal.text;
            let quiet = pal.text.with_alpha(0.55);
            for sort in text.placed() {
                let box_ = Rect::from_points(
                    affine * Point::new(sort.origin.x, m.descender + sort.origin.y),
                    affine * Point::new(sort.origin.x + sort.advance, m.ascender + sort.origin.y),
                );
                if sort.selected {
                    painter
                        .fill(box_, pal.role("selection").with_alpha(0.32))
                        .draw();
                }
                let color = if sort.active { ink } else { quiet };
                painter.fill(&(affine * sort.path), color).draw();
                if sort.active {
                    painter
                        .stroke(box_, &Stroke::new(1.0), pal.role("metricQuiet"))
                        .draw();
                }
            }
            // The caret, full em height, so it reads as a text cursor
            // rather than a mark on the baseline.
            let caret = text.caret();
            let top = affine * Point::new(caret.x, caret.y + m.ascender);
            let bottom = affine * Point::new(caret.x, caret.y + m.descender);
            painter
                .stroke(
                    Line::new(top, bottom),
                    &Stroke::new(1.5),
                    pal.role("textCursor"),
                )
                .draw();
            return;
        }

        // Underlay, drawn first so everything else sits on top of it. The
        // reference glyph is a quiet fill (it is a shape to match), the
        // background layer is a quiet outline (it is a trace to follow).
        if let Some(reference) = &self.underlay.reference {
            painter
                .fill(
                    &(affine * (**reference).clone()),
                    pal.text_muted.with_alpha(0.18),
                )
                .draw();
        }
        if let Some(background) = &self.underlay.background {
            painter
                .stroke(
                    &(affine * (**background).clone()),
                    &Stroke::new(1.0),
                    pal.text_muted.with_alpha(0.5),
                )
                .draw();
        }
        for mark in &self.underlay.mark_cloud {
            painter
                .fill(
                    &(affine * (**mark).clone()),
                    pal.role("component").with_alpha(0.10),
                )
                .draw();
        }
        if let Some(proposal) = &self.underlay.proposal {
            let path = affine * (**proposal).clone();
            painter
                .fill(&path, pal.role("warning").with_alpha(0.12))
                .draw();
            painter
                .stroke(
                    &path,
                    &Stroke::new(1.75),
                    pal.role("warning").with_alpha(0.95),
                )
                .draw();
        }

        // Interpolation ghosts: the other masters' outlines, faint.
        for ghost in self.ghosts.iter() {
            painter
                .stroke(
                    &(affine * ghost.clone()),
                    &Stroke::new(1.0),
                    pal.role("reference").with_alpha(0.55),
                )
                .draw();
        }

        // The design grid, as dots or lines according to View > Grid.
        // 8-unit intersection once the zoom passes 0.8x, and a finer
        // 2-unit dot past 8x. The dots grow with the pitch, a fifth of
        // the coarse one and an eighth of the fine one, within limits.
        {
            let zoom = self.session.viewport.zoom;
            let smoothstep = |t: f64| {
                let t = t.clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            let mid = smoothstep((zoom - 0.8) / 0.8);
            if mid > 0.0 {
                let inv = affine.inverse();
                let a = inv * Point::new(0.0, 0.0);
                let b = inv * Point::new(self.size.width, self.size.height);
                let (min_x, max_x) = (a.x.min(b.x), a.x.max(b.x));
                let (min_y, max_y) = (a.y.min(b.y), a.y.max(b.y));
                let mut level = |spacing: f64, skip_every: i64, size: f64, alpha: f64| {
                    let mut marks = kurbo::BezPath::new();
                    let h = size / 2.0;
                    let (ix0, ix1) = (
                        grid_index((min_x / spacing).floor()),
                        grid_index((max_x / spacing).ceil()),
                    );
                    let (iy0, iy1) = (
                        grid_index((min_y / spacing).floor()),
                        grid_index((max_y / spacing).ceil()),
                    );
                    if self.view.grid_lines {
                        for ix in ix0..=ix1 {
                            if skip_every == 0 || ix % skip_every != 0 {
                                let x = (affine * Point::new(ix as f64 * spacing, 0.0)).x;
                                marks.move_to(Point::new(x, 0.0));
                                marks.line_to(Point::new(x, self.size.height));
                            }
                        }
                        for iy in iy0..=iy1 {
                            if skip_every == 0 || iy % skip_every != 0 {
                                let y = (affine * Point::new(0.0, iy as f64 * spacing)).y;
                                marks.move_to(Point::new(0.0, y));
                                marks.line_to(Point::new(self.size.width, y));
                            }
                        }
                    } else {
                        for ix in ix0..=ix1 {
                            for iy in iy0..=iy1 {
                                if skip_every > 0 && ix % skip_every == 0 && iy % skip_every == 0 {
                                    continue;
                                }
                                let at =
                                    affine * Point::new(ix as f64 * spacing, iy as f64 * spacing);
                                marks.extend(kurbo::Shape::to_path(
                                    &Rect::new(at.x - h, at.y - h, at.x + h, at.y + h),
                                    0.1,
                                ));
                            }
                        }
                    }
                    let color = pal
                        .role("designGridCoarse")
                        .with_alpha(crate::view::render::px32(alpha));
                    if self.view.grid_lines {
                        painter.stroke(&marks, &Stroke::new(0.5), color).draw();
                    } else {
                        painter.fill(&marks, color).draw();
                    }
                };
                let coarse = (8.0 * zoom * 0.2).clamp(1.5, 5.0);
                level(8.0, 0, coarse, mid);
                let close = smoothstep((zoom - 8.0) / 8.0);
                if close > 0.0 {
                    let fine = (2.0 * zoom * 0.125).clamp(1.0, 3.5);
                    level(2.0, 4, fine, close);
                }
            }
        }

        let m = &self.session.metrics;
        // Metrics belong to this glyph's advance, not the whole workspace.
        // Include the full em even when its top lies above the ascender.
        let frame = pal.metrics_line();
        let rule = DesignStroke::Hairline.px();
        let box_top = m.upm.max(m.ascender);
        let x0 = (affine * Point::new(0.0, 0.0)).x;
        let x1 = (affine * Point::new(self.session.advance(), 0.0)).x;
        let mut levels = vec![
            0.0,
            m.upm,
            m.ascender,
            m.descender,
            m.x_height,
            m.cap_height,
        ];
        levels.retain(|y| y.is_finite());
        levels.sort_by(f64::total_cmp);
        levels.dedup_by(|a, b| (*a - *b).abs() < 0.001);
        for y in levels {
            let sy = (affine * Point::new(0.0, y)).y;
            painter.fill_rect(Rect::new(x0.min(x1), sy, x0.max(x1), sy + rule), frame);
        }
        let top = (affine * Point::new(0.0, box_top)).y;
        let bottom = (affine * Point::new(0.0, m.descender)).y;
        for x in [x0, x1] {
            painter.fill_rect(
                Rect::new(x, top.min(bottom), x + rule, top.max(bottom)),
                frame,
            );
        }

        // Editing affordances only render on a master. Off a master the view
        // shows the read-only interpolated instance instead (web/Glyphs
        // behavior): swap the outline, don't ghost it behind an editable one.
        if self.interp.is_none() {
            if !self.session.components.elements().is_empty() {
                painter
                    .fill(
                        &(affine * self.session.components.clone()),
                        pal.role("component").with_alpha(0.5),
                    )
                    .draw();
                if let Some(selected) = self.session.selected_component_path() {
                    painter
                        .stroke(
                            &(affine * selected.clone()),
                            &Stroke::new(2.0),
                            pal.role("selection"),
                        )
                        .draw();
                }
            }
            let outline = affine * self.session.outline();
            painter
                // The outline fill role, opaque, as the GPUI build: a mid
                // tone under the stroke so the shape reads at a glance.
                .fill(&outline, pal.role("outlineFill"))
                .draw();
            painter
                .stroke(&outline, &Stroke::new(1.0), pal.role("pathStroke"))
                .draw();

            let handle = Stroke::new(1.0);
            for contour in &self.session.glyph.contours {
                let n = contour.points.len();
                for i in 0..n {
                    if !matches!(contour.points[i].typ, norad::PointType::OffCurve) {
                        continue;
                    }
                    let off = affine * Point::new(contour.points[i].x, contour.points[i].y);
                    for j in [(i + n - 1) % n, (i + 1) % n] {
                        if !matches!(contour.points[j].typ, norad::PointType::OffCurve) {
                            let on = affine * Point::new(contour.points[j].x, contour.points[j].y);
                            painter
                                .stroke(Line::new(off, on), &handle, pal.handle_line)
                                .draw();
                        }
                    }
                }
            }

            let marker_scale = point_marker_scale(self.session.viewport.zoom);
            let ring_width = (POINT_RING_WIDTH * marker_scale).max(DesignStroke::Hairline.px());
            let halo_width = ring_width + POINT_HALO_EXTRA;
            for (id, sp, on_curve, smooth, _) in self.screen_points() {
                let selected = self.session.selection.contains(&id);
                let hue = if !on_curve {
                    pal.role("pointOffcurve")
                } else if smooth {
                    pal.role("pointSmooth")
                } else {
                    pal.role("pointCorner")
                };
                // The hue is the ring or the interior, by the theme's
                // recipe: a ring on a dark interior where the ground
                // is far from mid grey, a hue fill with one keyline
                // where it is not (the mark cells' treatment).
                let (fill, interior) = if selected {
                    (
                        pal.point_outline.unwrap_or(pal.text),
                        pal.role("pointSelected"),
                    )
                } else if pal.points_filled {
                    (pal.point_outline.unwrap_or(pal.text), hue)
                } else {
                    (hue, pal.app)
                };
                // A point is a dark window with a coloured ring, which is
                // the GPUI build's recipe and the web editor's before it: a
                // halo so the point keeps an edge over the outline, an
                // interior that masks what runs under it, then a
                // constant-width ring. A solid dot loses its shape against
                // the curve it sits on.
                let square = on_curve && !smooth;
                let radius = if square {
                    POINT_CORNER_RADIUS
                } else {
                    POINT_CURVE_RADIUS
                };
                let r = (radius + if selected { POINT_SELECTED_GROW } else { 0.0 }) * marker_scale;
                let halo = pal.app.with_alpha(0.85);
                let ring = Stroke::new(ring_width);
                if square {
                    let shape = Rect::new(sp.x - r, sp.y - r, sp.x + r, sp.y + r);
                    if pal.point_halo {
                        painter.stroke(shape, &Stroke::new(halo_width), halo).draw();
                    }
                    painter.fill(shape, interior).draw();
                    painter.stroke(shape, &ring, fill).draw();
                } else {
                    let shape = Circle::new(sp, r);
                    if pal.point_halo {
                        painter.stroke(shape, &Stroke::new(halo_width), halo).draw();
                    }
                    painter.fill(shape, interior).draw();
                    painter.stroke(shape, &ring, fill).draw();
                }
            }

            // A separate direction arrow preserves the ordinary point shape
            // and its corner/smooth hue, as in the inspected reference bundle.
            for (id, from, to) in self.start_markers() {
                let selected = self.session.selection.contains(&id);
                let size = (START_ARROW_RADIUS + if selected { POINT_SELECTED_GROW } else { 0.0 })
                    * marker_scale;
                let forward = (to - from).normalize();
                let side = kurbo::Vec2::new(-forward.y, forward.x);
                let center = from + side * START_ARROW_OFFSET * marker_scale;
                let tip = center + forward * size;
                let base = center - forward * size * 0.5;
                let mut arrow = kurbo::BezPath::new();
                arrow.move_to(tip);
                arrow.line_to(base + side * size * 0.5);
                arrow.line_to(base - side * size * 0.5);
                arrow.close_path();
                let color = pal.role(if selected {
                    "pointSelected"
                } else {
                    "pointSmooth"
                });
                painter.fill(&arrow, color).draw();
            }

            // Anchors have a dark interior and a distinct pink keyline in
            // the inspected reference; selection retains the shared node palette.
            let anchor_color = pal.mark("pink").unwrap_or_else(|| pal.role("danger"));
            for (ai, anchor) in self.session.glyph.anchors.iter().enumerate() {
                let p = affine * Point::new(anchor.x, anchor.y);
                let selected = self.session.selected_anchor == Some(ai);
                let (ring, inner) = if selected {
                    (
                        pal.point_outline.unwrap_or(pal.text),
                        pal.role("pointSelected"),
                    )
                } else {
                    (anchor_color, pal.role("pointInner"))
                };
                let r = (POINT_CURVE_RADIUS + if selected { POINT_SELECTED_GROW } else { 0.0 })
                    * marker_scale
                    * ANCHOR_DIAMOND_SCALE;
                let diamond = kurbo::BezPath::from_vec(vec![
                    kurbo::PathEl::MoveTo(Point::new(p.x, p.y - r)),
                    kurbo::PathEl::LineTo(Point::new(p.x + r, p.y)),
                    kurbo::PathEl::LineTo(Point::new(p.x, p.y + r)),
                    kurbo::PathEl::LineTo(Point::new(p.x - r, p.y)),
                    kurbo::PathEl::ClosePath,
                ]);
                if pal.point_halo {
                    painter
                        .stroke(&diamond, &Stroke::new(halo_width), pal.role("halo"))
                        .draw();
                }
                painter.fill(&diamond, inner).draw();
                painter
                    .stroke(&diamond, &Stroke::new(ring_width), ring)
                    .draw();
            }
        } else if let Some(interp) = &self.interp {
            // Read-only interpolated instance in warm amber, filled and
            // stroked, standing in for the editable outline.
            let path = affine * (**interp).clone();
            painter
                .fill(&path, pal.role("warning").with_alpha(0.14))
                .draw();
            painter
                .stroke(
                    &path,
                    &Stroke::new(1.75),
                    pal.role("warning").with_alpha(0.95),
                )
                .draw();
        }

        // Curvature comb: a strip pushed out along the normal of every
        // curved segment, so the shape of the curvature is visible.
        if self.view.comb && self.interp.is_none() {
            let strips = self.session.curvature_comb();
            let maxk = strips
                .iter()
                .flatten()
                .map(|sample| sample.kappa.abs())
                .fold(0.0, f64::max);
            for strip in strips {
                for pair in strip.windows(2) {
                    let mut quad = kurbo::BezPath::new();
                    quad.move_to(affine * pair[0].on);
                    quad.line_to(affine * pair[1].on);
                    quad.line_to(affine * pair[1].outer);
                    quad.line_to(affine * pair[0].outer);
                    quad.close_path();
                    let k = if maxk > 1e-12 {
                        (pair[0].kappa.abs() + pair[1].kappa.abs()) * 0.5 / maxk
                    } else {
                        0.0
                    };
                    painter
                        .stroke(
                            &quad,
                            &Stroke::new(2.0),
                            pal.point_outline.unwrap_or(pal.text),
                        )
                        .draw();
                    painter.fill(&quad, pal.comb_gradient(k)).draw();
                }
            }
        }

        // Continuity rings match GPUI and preserve the underlying point shape.
        if self.view.continuity && self.interp.is_none() {
            use runebender_core::analysis::curve::GLevel;
            const CONTINUITY_RADIUS: f64 = 4.5 * 1.9;
            let color = pal.mark("green").unwrap_or(pal.text);
            let outline = pal.point_outline.unwrap_or(pal.text);
            for node in self.session.continuity() {
                if matches!(node.level, GLevel::Corner) {
                    continue;
                }
                let ring = Circle::new(affine * node.at, CONTINUITY_RADIUS);
                painter.stroke(ring, &Stroke::new(3.0), outline).draw();
                painter.stroke(ring, &Stroke::new(1.5), color).draw();
            }
        }

        // Colorize: tint the outline and its handles by segment length,
        // the web editor's mode for spotting odd measurements.
        if self.view.colorize && self.interp.is_none() {
            for stroke in self.session.colored_strokes() {
                let color = pal.popcount(stroke.popcount);
                let width = if stroke.wide { 2.0 } else { 1.0 };
                painter
                    .stroke(&(affine * stroke.path), &Stroke::new(width), color)
                    .draw();
            }
        }

        // Measure overlay: segment and handle lengths, and side bearings.
        if self.view.measures() && self.interp.is_none() {
            let zoom = self.session.viewport.zoom;
            for m in self.session.measurements() {
                use runebender_core::analysis::measure::MeasureKind;
                let wanted = match m.kind {
                    MeasureKind::Handle => self.view.handles,
                    MeasureKind::Segment => self.view.segments,
                    MeasureKind::Horizontal | MeasureKind::Vertical => self.view.spans,
                };
                if !wanted {
                    continue;
                }
                let a = affine * m.a;
                let b = affine * m.b;
                let color = match m.kind {
                    MeasureKind::Handle => pal.role("pointOffcurve"),
                    MeasureKind::Segment => pal.tool_feedback(),
                    _ => pal.role("selection"),
                };
                painter
                    .stroke(Line::new(a, b), &Stroke::new(1.0), color)
                    .draw();
                let mid = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
                let text = self.view.label(m.length);
                text_label::draw(painter, mid, &text, 11.0, color, Anchor::Middle);
            }
            if self.view.sizes {
                use kurbo::Shape as _;
                for hit in runebender_core::outline::segment_ops::segments(&self.session.glyph) {
                    let bounds = hit.seg.bounding_box();
                    if bounds.width() < 1.0 && bounds.height() < 1.0 {
                        continue;
                    }
                    let a = affine * Point::new(bounds.x0, bounds.y0);
                    let b = affine * Point::new(bounds.x1, bounds.y1);
                    let screen = Rect::from_points(a, b);
                    painter
                        .stroke(screen, &Stroke::new(1.0), pal.role("metricQuiet"))
                        .draw();
                    let label = format!("{:.0}×{:.0}", bounds.width(), bounds.height());
                    text_label::draw(
                        painter,
                        screen.center(),
                        &label,
                        11.0,
                        pal.text,
                        Anchor::Middle,
                    );
                }
            }
            if let Some(sb) = self.session.side_bearings().filter(|_| self.view.bearings) {
                let quiet = pal.role("metricQuiet");
                let y = (affine
                    * Point::new(0.0, sb.y_left.min(sb.y_right) - 40.0 / zoom.max(0.001)))
                .y;
                let l = affine * Point::new(0.0, 0.0);
                let ink_l = affine * Point::new(sb.min_x, 0.0);
                let ink_r = affine * Point::new(sb.max_x, 0.0);
                let adv = affine * Point::new(sb.advance, 0.0);
                painter
                    .stroke(Line::new((l.x, y), (ink_l.x, y)), &Stroke::new(1.0), quiet)
                    .draw();
                painter
                    .stroke(
                        Line::new((ink_r.x, y), (adv.x, y)),
                        &Stroke::new(1.0),
                        quiet,
                    )
                    .draw();
                let (lsb, rsb) = (self.view.label(sb.lsb), self.view.label(sb.rsb));
                text_label::draw(
                    painter,
                    Point::new((l.x + ink_l.x) / 2.0, y - 8.0),
                    &lsb,
                    11.0,
                    quiet,
                    Anchor::Middle,
                );
                text_label::draw(
                    painter,
                    Point::new((ink_r.x + adv.x) / 2.0, y - 8.0),
                    &rsb,
                    11.0,
                    quiet,
                    Anchor::Middle,
                );
            }
        }

        // Marquee rectangle.
        if let Drag::Marquee { start, current, .. } = &self.drag {
            let rect = Rect::from_points(*start, *current);
            painter
                .fill(rect, pal.role("selection").with_alpha(0.15))
                .draw();
            painter
                .stroke(
                    rect,
                    &Stroke::new(1.0),
                    pal.role("selection").with_alpha(0.8),
                )
                .draw();
        }

        // Shape preview.
        if let Drag::Shape { start, current } = &self.drag {
            let p0 = affine * *start;
            let p1 = affine * *current;
            let accent = pal.tool_feedback();
            match self.tool {
                Tool::Knife => {
                    let danger = pal.role("danger");
                    painter
                        .stroke(Line::new(p0, p1), &Stroke::new(1.0), danger)
                        .draw();
                    for hit in self.session.knife_hits(*start, *current) {
                        let sp = affine * hit;
                        painter.fill(Circle::new(sp, 3.5), danger).draw();
                    }
                }
                Tool::Ellipse => {
                    let c = ((p0.x + p1.x) / 2.0, (p0.y + p1.y) / 2.0);
                    let rr = ((p1.x - p0.x).abs() / 2.0, (p1.y - p0.y).abs() / 2.0);
                    let e = kurbo::Ellipse::new(c, rr, 0.0);
                    painter.stroke(e, &Stroke::new(1.0), accent).draw();
                }
                _ => {
                    painter
                        .stroke(Rect::from_points(p0, p1), &Stroke::new(1.0), accent)
                        .draw();
                }
            }
        }

        self.paint_metrics(painter);
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Off a master the view is a read-only instance: swallow edit clicks.
        if self.interp.is_some()
            && let PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary | PointerButton::Secondary),
                ..
            }) = event
        {
            ctx.set_handled();
            return;
        }
        // A click in a metric box goes to the box, not to the drawing
        // underneath it. The panel is painted, so nothing else will do
        // this for us: a composed panel would have taken the click by
        // being in front.
        if let PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            state,
            ..
        }) = event
        {
            let at = ctx.local_position(state.position);
            if let Some(boxes) = self.metric_boxes()
                && let Some((field, _)) = boxes.iter().find(|(_, rect)| rect.contains(at))
            {
                let edited = self.commit_metric();
                self.focus_metric(*field);
                ctx.request_focus();
                self.emit(ctx, edited);
                ctx.set_handled();
                return;
            }
            // A click anywhere else puts the value away.
            if self.field.is_some() {
                let edited = self.commit_metric();
                self.field = None;
                self.emit(ctx, edited);
            }
        }
        match event {
            PointerEvent::Down(PointerButtonEvent { button, state, .. }) => {
                ctx.request_focus();
                let at = ctx.local_position(state.position);
                // Text tool: a click is a caret placement, and a click on
                // a sort makes that glyph the one being edited.
                if self.tool == Tool::Text
                    && let Some(text) = self.text.as_mut()
                    && *button == Some(PointerButton::Primary)
                {
                    let design = self.session.viewport.screen_to_design(at);
                    if let Some(index) = text.click(design)
                        && let Some(glyph) = text.activate(index)
                    {
                        ctx.submit_action::<EditorEvent>(EditorEvent::EditGlyph(glyph));
                    }
                    ctx.request_render();
                    ctx.set_handled();
                    return;
                }
                if *button == Some(PointerButton::Secondary) {
                    // A layer, not a rectangle painted into this canvas:
                    // it is rooted in window space, so it can hang past
                    // the editor's edge like a menu should.
                    if self.menu.is_none() {
                        let design = self.session.viewport.screen_to_design(at);
                        let menu = ContextMenu::new(
                            ctx.widget_id(),
                            MenuTarget::Editor,
                            MENU_ITEMS.to_vec(),
                            self.palette.clone(),
                            design,
                        );
                        let menu = NewWidget::new(menu);
                        self.menu = Some(menu.id());
                        ctx.create_layer(LayerType::Other, menu, ctx.to_window(at));
                    }
                    ctx.set_handled();
                    return;
                }
                ctx.capture_pointer();
                match button {
                    Some(PointerButton::Primary) if self.tool == Tool::HyperPen => {
                        let affine = self.session.viewport.affine();
                        let near_first = self
                            .session
                            .first_contour_point()
                            .map(|p| (affine * p).distance(at) <= HIT_RADIUS_PX)
                            .unwrap_or(false);
                        let corner = state.modifiers.alt();
                        if near_first && self.session.hyper_is_active() {
                            self.session.hyper_close();
                        } else {
                            let d = self.session.viewport.screen_to_design(at);
                            self.session.hyper_add(d.x, d.y, corner);
                        }
                        self.emit(ctx, true);
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary)
                        if matches!(self.tool, Tool::Rect | Tool::Ellipse | Tool::Knife) =>
                    {
                        ctx.request_focus();
                        ctx.capture_pointer();
                        let d = self.session.viewport.screen_to_design(at);
                        self.drag = Drag::Shape {
                            start: d,
                            current: d,
                        };
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Pen => {
                        let affine = self.session.viewport.affine();
                        let near_first = self
                            .session
                            .pen_first_point()
                            .map(|p| (affine * p).distance(at) <= HIT_RADIUS_PX)
                            .unwrap_or(false);
                        if near_first && self.session.pen_is_active() {
                            self.session.pen_close();
                            self.drag = Drag::None;
                            self.emit(ctx, true);
                        } else {
                            let origin = self.session.viewport.screen_to_design(at);
                            self.drag = Drag::Pen {
                                origin,
                                dragging: false,
                            };
                        }
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) => {
                        let shift = state.modifiers.shift();
                        // Sidebearing lines (only when not near a point).
                        let affine = self.session.viewport.affine();
                        let adv_x = (affine * Point::new(self.session.advance(), 0.0)).x;
                        let lsb_x = (affine * Point::new(0.0, 0.0)).x;
                        if self.hit_point(at).is_none() {
                            if (at.x - adv_x).abs() <= 4.0 {
                                self.drag = Drag::AdvanceLine;
                                ctx.set_handled();
                                return;
                            }
                            if (at.x - lsb_x).abs() <= 4.0 {
                                self.drag = Drag::LeftLine { last_x: at.x };
                                ctx.set_handled();
                                return;
                            }
                        }
                        // Anchor hit takes priority over points.
                        if let Some(ai) = self.session.anchor_at(
                            self.session.viewport.screen_to_design(at),
                            HIT_RADIUS_PX / self.session.viewport.zoom,
                        ) {
                            self.session.selected_anchor = Some(ai);
                            self.session.selected_component = None;
                            self.session.selection.clear();
                            self.drag = Drag::Anchor { idx: ai };
                            self.emit(ctx, false);
                            ctx.set_handled();
                            return;
                        }
                        self.session.selected_anchor = None;
                        match self.hit_point(at) {
                            Some(id) => {
                                if shift {
                                    if !self.session.selection.remove(&id) {
                                        self.session.selection.insert(id);
                                    }
                                } else if !self.session.selection.contains(&id) {
                                    self.session.selection.clear();
                                    self.session.selection.insert(id);
                                }
                                self.session.selected_component = None;
                                self.session.begin_point_drag();
                                self.drag = Drag::Points { start: at };
                                self.emit(ctx, false);
                            }
                            None => {
                                let design = self.session.viewport.screen_to_design(at);
                                if let Some(index) = self.session.component_at(design) {
                                    self.session.select_component(index);
                                    self.drag = Drag::Component { last: design };
                                    self.emit(ctx, false);
                                    ctx.set_handled();
                                    return;
                                }
                                if !shift {
                                    self.session.selection.clear();
                                    self.session.selected_component = None;
                                    self.emit(ctx, false);
                                }
                                self.drag = Drag::Marquee {
                                    start: at,
                                    current: at,
                                    additive: shift,
                                };
                            }
                        }
                    }
                    _ => self.drag = Drag::Pan { last: at },
                }
                ctx.set_handled();
            }
            PointerEvent::Move(PointerUpdate { current, .. }) => {
                let at = ctx.local_position(current.position);
                if matches!(self.tool, Tool::Pen | Tool::HyperPen) {
                    self.hover = Some(self.session.viewport.screen_to_design(at));
                    if self.session.active_contour.is_some() {
                        ctx.request_render();
                    }
                }
                match &mut self.drag {
                    Drag::AdvanceLine => {
                        let d = self.session.viewport.screen_to_design(at);
                        self.session.drag_advance(d.x.round());
                        ctx.request_render();
                    }
                    Drag::LeftLine { last_x } => {
                        let dx_screen = at.x - *last_x;
                        *last_x = at.x;
                        let dx = (dx_screen / self.session.viewport.zoom).round();
                        if dx != 0.0 {
                            self.session.shift_glyph(dx);
                            ctx.request_render();
                        }
                    }
                    Drag::Anchor { idx } => {
                        let idx = *idx;
                        let d = self.session.viewport.screen_to_design(at);
                        self.session.move_anchor(idx, d.x.round(), d.y.round());
                        ctx.request_render();
                    }
                    Drag::Component { last } => {
                        let design = self.session.viewport.screen_to_design(at);
                        let delta = design - *last;
                        *last = design;
                        if self.session.drag_component_by(delta.x, delta.y) {
                            ctx.request_render();
                        }
                    }
                    Drag::Pen { origin, dragging } => {
                        let origin = *origin;
                        let to = self.session.viewport.screen_to_design(at);
                        let moved_px = (self.session.viewport.affine() * origin).distance(at);
                        if *dragging {
                            self.session.pen_smooth_drag(origin, to);
                            ctx.request_render();
                        } else if moved_px > 4.0 {
                            if let Drag::Pen { dragging, .. } = &mut self.drag {
                                *dragging = true;
                            }
                            self.session.pen_smooth_begin(origin, to);
                            ctx.request_render();
                        }
                    }
                    Drag::Points { start } => {
                        let zoom = self.session.viewport.zoom;
                        let total = ((at.x - start.x) / zoom, -(at.y - start.y) / zoom);
                        if self.session.drag_points_to(total) {
                            ctx.request_render();
                        }
                    }
                    Drag::Pan { last } => {
                        let d = at - *last;
                        *last = at;
                        self.session.viewport.pan(d.x, d.y);
                        ctx.request_render();
                    }
                    Drag::Marquee { current, .. } => {
                        *current = at;
                        ctx.request_render();
                    }
                    Drag::Shape { current, .. } => {
                        *current = self.session.viewport.screen_to_design(at);
                        ctx.request_render();
                    }
                    Drag::None => {}
                }
            }
            PointerEvent::Up(_) | PointerEvent::Cancel(_) => match &self.drag {
                Drag::Points { .. } => {
                    self.session.end_point_drag();
                    self.drag = Drag::None;
                    self.emit(ctx, true);
                }
                Drag::Pen { origin, dragging } => {
                    if !dragging {
                        self.session.pen_corner(origin.x, origin.y);
                    }
                    self.drag = Drag::None;
                    self.emit(ctx, true);
                }
                Drag::Marquee {
                    start,
                    current,
                    additive,
                } => {
                    let rect = Rect::from_points(*start, *current);
                    let additive = *additive;
                    if !additive {
                        self.session.selection.clear();
                    }
                    for (id, sp, _, _, _) in self.screen_points() {
                        if rect.contains(sp) {
                            self.session.selection.insert(id);
                        }
                    }
                    self.drag = Drag::None;
                    self.emit(ctx, false);
                }
                Drag::Anchor { .. } | Drag::AdvanceLine | Drag::LeftLine { .. } => {
                    self.session.end_metric_drag();
                    self.drag = Drag::None;
                    self.emit(ctx, true);
                }
                Drag::Component { .. } => {
                    self.session.end_component_drag();
                    self.drag = Drag::None;
                    self.emit(ctx, true);
                }
                Drag::Shape { start, current } => {
                    let (s0, c0) = (*start, *current);
                    match self.tool {
                        Tool::Rect => self.session.add_rect(s0.x, s0.y, c0.x, c0.y),
                        Tool::Ellipse => self.session.add_ellipse(s0.x, s0.y, c0.x, c0.y),
                        Tool::Knife => {
                            self.session.knife_cut(s0, c0);
                        }
                        _ => {}
                    }
                    self.drag = Drag::None;
                    self.emit(ctx, true);
                }
                Drag::Pan { .. } => self.drag = Drag::None,
                Drag::None => {}
            },
            PointerEvent::Scroll(PointerScrollEvent { delta, state, .. }) => {
                let at = ctx.local_position(state.position);
                let dy = match delta {
                    ScrollDelta::PixelDelta(p) => p.y,
                    ScrollDelta::LineDelta(_, y) => f64::from(*y) * 20.0,
                    _ => 0.0,
                };
                let factor = (dy / 300.0).exp();
                self.session.viewport.zoom_about(at, factor, 0.02, 64.0);
                ctx.request_render();
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
        if self.tool == Tool::Text
            && let Some(text) = self.text.as_mut()
            && let TextEvent::Ime(ime) = event
        {
            let changed = match ime {
                masonry::core::Ime::Preedit(value, _) => {
                    text.set_preedit(value.clone());
                    false
                }
                masonry::core::Ime::Commit(value) => text.commit_preedit(value),
                masonry::core::Ime::Disabled => {
                    text.set_preedit(String::new());
                    false
                }
                masonry::core::Ime::Enabled => false,
            };
            if changed {
                self.emit_text_changed(ctx);
            }
            // Composition can change the visible run even when the commit
            // contains no glyph the document can place. Refit after every
            // IME transition so cancelling or rejecting a preedit cannot
            // leave the viewport sized for stale composition text.
            self.fit_text();
            ctx.request_render();
            ctx.set_handled();
            return;
        }
        if self.tool == Tool::Text
            && let Some(text) = self.text.as_mut()
            && let TextEvent::ClipboardPaste(value) = event
        {
            if text.commit_preedit(value) {
                self.fit_text();
                ctx.request_render();
                self.emit_text_changed(ctx);
            }
            ctx.set_handled();
            return;
        }
        let TextEvent::Keyboard(key) = event else {
            return;
        };
        if key.state != KeyState::Down {
            return;
        }
        // Read-only instance: don't handle edit keys, but let Escape bubble to
        // the shortcut host (so it still leaves the editor).
        if self.interp.is_some() {
            return;
        }
        let cmd = key.modifiers.meta() || key.modifiers.ctrl();
        let shift = key.modifiers.shift();
        let step = if shift { 10.0 } else { 1.0 };

        if self.tool == Tool::Text
            && let Some(text) = self.text.as_mut()
            && cmd
            && let Key::Character(character) = &key.key
        {
            if character.eq_ignore_ascii_case("c") || character.eq_ignore_ascii_case("x") {
                if let Some(selected) = text.buffer.selected_text().filter(|text| !text.is_empty())
                {
                    ctx.set_clipboard(selected);
                    if character.eq_ignore_ascii_case("x") {
                        text.buffer.delete_after_cursor();
                        text.buffer.shape_arabic_if_rtl();
                        self.fit_text();
                        ctx.request_render();
                        self.emit_text_changed(ctx);
                    }
                }
                ctx.set_handled();
                return;
            }
            if character.eq_ignore_ascii_case("a") {
                text.buffer.select_range(0, text.buffer.len());
                ctx.request_render();
                ctx.set_handled();
                return;
            }
        }

        // A focused metric box takes the keys first. Everything below
        // this point would otherwise read a digit as a nudge or a tool.
        if self.field.is_some() && !cmd {
            let (edited, handled) = self.metric_key(key);
            if handled {
                self.emit(ctx, edited);
                ctx.set_handled();
                return;
            }
        }

        // The text tool types. Everything a key would otherwise do to
        // the outline is off while it is in hand, because a person
        // typing "n" means the letter, not the pen.
        if self.tool == Tool::Text
            && let Some(text) = self.text.as_mut()
            && !cmd
        {
            let (handled, changed_text) = match &key.key {
                // Text arrives through `Ime::Commit`, including ordinary
                // keyboard input. Consume the logical character key here so
                // it neither inserts twice nor triggers an application tool.
                Key::Character(_) => {
                    ctx.set_handled();
                    return;
                }
                Key::Named(NamedKey::Backspace) => {
                    let changed = text.buffer.delete_before_cursor().is_some();
                    if changed {
                        text.buffer.shape_arabic_if_rtl();
                    }
                    (changed, changed)
                }
                Key::Named(NamedKey::Delete) => {
                    let changed = text.buffer.delete_after_cursor().is_some();
                    if changed {
                        text.buffer.shape_arabic_if_rtl();
                    }
                    (changed, changed)
                }
                Key::Named(NamedKey::Enter) => {
                    text.buffer.insert_line_break();
                    (true, true)
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    if shift {
                        text.buffer.extend_selection_visual_left();
                    } else {
                        text.buffer.move_cursor_visual_left();
                    }
                    (true, false)
                }
                Key::Named(NamedKey::ArrowRight) => {
                    if shift {
                        text.buffer.extend_selection_visual_right();
                    } else {
                        text.buffer.move_cursor_visual_right();
                    }
                    (true, false)
                }
                Key::Named(NamedKey::ArrowUp) => {
                    if shift {
                        text.buffer
                            .extend_selection_vertically(-1, text.line_height);
                    } else {
                        text.buffer.move_cursor_vertically(-1, text.line_height);
                    }
                    (true, false)
                }
                Key::Named(NamedKey::ArrowDown) => {
                    if shift {
                        text.buffer.extend_selection_vertically(1, text.line_height);
                    } else {
                        text.buffer.move_cursor_vertically(1, text.line_height);
                    }
                    (true, false)
                }
                Key::Named(NamedKey::Home) => {
                    if shift {
                        text.buffer.extend_selection_to_line_edge(false);
                    } else {
                        text.buffer.move_cursor_to_line_edge(false);
                    }
                    (true, false)
                }
                Key::Named(NamedKey::End) => {
                    if shift {
                        text.buffer.extend_selection_to_line_edge(true);
                    } else {
                        text.buffer.move_cursor_to_line_edge(true);
                    }
                    (true, false)
                }
                _ => (false, false),
            };
            if handled {
                // A longer line needs more room; refit only while the
                // caret is at the end, so it does not fight a person who
                // has zoomed in on a pair.
                let at_end = text.buffer.cursor() == text.buffer.len();
                if at_end {
                    self.fit_text();
                }
                if changed_text {
                    self.emit_text_changed(ctx);
                }
                ctx.request_render();
                ctx.set_handled();
            }
            return;
        }
        let (edited, handled) = match &key.key {
            Key::Named(NamedKey::Escape) => {
                if self.session.pen_is_active() || self.session.active_contour.is_some() {
                    self.session.pen_cancel();
                    self.emit(ctx, false);
                    ctx.set_handled();
                }
                // Otherwise do not handle: the shortcut host turns Escape into
                // "back to overview".
                return;
            }
            Key::Named(NamedKey::ArrowLeft) => (self.session.nudge(-step, 0.0), true),
            Key::Named(NamedKey::ArrowRight) => (self.session.nudge(step, 0.0), true),
            Key::Named(NamedKey::ArrowUp) => (self.session.nudge(0.0, step), true),
            Key::Named(NamedKey::ArrowDown) => (self.session.nudge(0.0, -step), true),
            Key::Named(NamedKey::Delete) | Key::Named(NamedKey::Backspace) => {
                if self.session.selected_anchor.is_some() {
                    (self.session.delete_selected_anchor(), true)
                } else {
                    (self.session.delete_selected(), true)
                }
            }
            Key::Character(c) if cmd && c.eq_ignore_ascii_case("a") => {
                self.session.select_all();
                (false, true)
            }
            Key::Character(c) if cmd && !shift && c.eq_ignore_ascii_case("z") => {
                ctx.submit_action::<EditorEvent>(EditorEvent::Undo);
                ctx.set_handled();
                return;
            }
            Key::Character(c) if cmd && (c == "y" || (shift && c.eq_ignore_ascii_case("z"))) => {
                ctx.submit_action::<EditorEvent>(EditorEvent::Redo);
                ctx.set_handled();
                return;
            }
            _ => return,
        };
        if handled {
            self.emit(ctx, edited);
            ctx.set_handled();
        }
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
        node.set_description(format!("Glyph editor: {}", self.session.glyph_name));
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

// ---------------------------------------------------------------------------
/// Cheap equality for the interpolation overlay: same Arc, or both absent.
fn interp_eq(a: &Option<Arc<kurbo::BezPath>>, b: &Option<Arc<kurbo::BezPath>>) -> bool {
    match (a, b) {
        (Some(x), Some(y)) => Arc::ptr_eq(x, y),
        (None, None) => true,
        _ => false,
    }
}

// View wrapper.

pub(crate) struct EditorView<F> {
    session: Arc<Session>,
    palette: Arc<Palette>,
    groups: (String, String),
    mark: Option<xilem::Color>,
    tool: Tool,
    view: ViewOptions,
    ghosts: Arc<Vec<kurbo::BezPath>>,
    interp: Option<Arc<kurbo::BezPath>>,
    underlay: Underlay,
    text: Option<crate::edit::text_tool::TextInputs>,
    on_event: F,
}

// The editor takes every input it draws from as its own argument, so the
// call site reads as a list of what the view depends on.
#[expect(
    clippy::too_many_arguments,
    reason = "one argument per layer this paint pass draws"
)]
pub(crate) fn editor<F: Fn(&mut Workspace, EditorEvent) + 'static>(
    session: Arc<Session>,
    palette: Arc<Palette>,
    groups: (String, String),
    mark: Option<xilem::Color>,
    tool: Tool,
    view: ViewOptions,
    ghosts: Arc<Vec<kurbo::BezPath>>,
    interp: Option<Arc<kurbo::BezPath>>,
    underlay: Underlay,
    text: Option<crate::edit::text_tool::TextInputs>,
    on_event: F,
) -> EditorView<F> {
    EditorView {
        session,
        palette,
        groups,
        mark,
        tool,
        view,
        ghosts,
        interp,
        underlay,
        text,
        on_event,
    }
}

/// The analysis overlays: what the editor draws on top of the outline
/// besides the points.
///
/// These match the GPUI build's Measure and Curves options, and all of
/// them read from `runebender-core`, so the three editors agree about
/// what a kink or a stem is.
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct ViewOptions {
    /// Draw design-grid lines instead of dots.
    pub grid_lines: bool,
    /// The curvature comb.
    pub comb: bool,
    /// A dot per on-curve node, colored by continuity level.
    pub continuity: bool,
    /// Tint the outline and handles by segment length.
    pub colorize: bool,
    /// Label handle lengths.
    pub handles: bool,
    /// Label straight segment lengths.
    pub segments: bool,
    /// Draw bounding boxes and width×height labels for every segment.
    pub sizes: bool,
    /// Draw horizontal and vertical stem/counter spans.
    pub spans: bool,
    /// Draw and label the side bearings.
    pub bearings: bool,
    /// Spell lengths as sums of powers of two: 96 = 64+32.
    pub popcount: bool,
}

impl ViewOptions {
    /// What the Measure tool turns on when it is picked.
    pub(crate) fn measuring() -> Self {
        Self {
            handles: true,
            segments: true,
            bearings: true,
            popcount: true,
            ..Self::default()
        }
    }

    /// Whether anything in the measure group is on.
    pub(crate) fn measures(self) -> bool {
        self.colorize || self.handles || self.segments || self.sizes || self.spans || self.bearings
    }

    /// A length, spelled the way the options ask for.
    fn label(self, value: i64) -> String {
        if self.popcount {
            runebender_core::analysis::measure::label(value)
        } else {
            value.to_string()
        }
    }
}

/// What is drawn under the outline: the UFO background layer, a
/// reference glyph, and an optional proposal comparison. All are
/// read-only; the warm proposal is stronger so it remains legible
/// against the editable foreground.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct Underlay {
    /// The glyph's contours in the UFO's background layer.
    pub background: Option<Arc<kurbo::BezPath>>,
    /// Another glyph, shown behind this one.
    pub reference: Option<Arc<kurbo::BezPath>>,
    /// Marks positioned by matching `_anchor`/`anchor` pairs.
    pub mark_cloud: Vec<Arc<kurbo::BezPath>>,
    /// A waiting proposal for this glyph, shown in warm amber.
    pub proposal: Option<Arc<kurbo::BezPath>>,
}

impl<F> ViewMarker for EditorView<F> {}
impl<F: Fn(&mut Workspace, EditorEvent) + 'static> View<Workspace, (), ViewCtx> for EditorView<F> {
    type Element = Pod<EditorWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut Workspace) -> (Self::Element, Self::ViewState) {
        let mut session = (*self.session).clone();
        if self.text.is_some() {
            // Framing is decided in layout, and the text line needs a
            // different frame than one glyph.
            session.fitted = false;
        }
        let widget = EditorWidget {
            session,
            palette: self.palette.clone(),
            groups: self.groups.clone(),
            mark: self.mark,
            tool: self.tool,
            ghosts: self.ghosts.clone(),
            interp: self.interp.clone(),
            underlay: self.underlay.clone(),
            text: self
                .text
                .as_ref()
                .map(crate::edit::text_tool::TextState::new),
            text_inputs: self.text.clone(),
            size: Size::ZERO,
            drag: Drag::None,
            hover: None,
            menu: None,
            view: self.view,
            field: None,
            field_buf: String::new(),
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
        let mut dirty = false;
        if !Arc::ptr_eq(&self.session, &prev.session) {
            let viewport = element.widget.session.viewport.clone();
            let fitted = element.widget.session.fitted;
            element.widget.session = (*self.session).clone();
            element.widget.session.viewport = viewport;
            element.widget.session.fitted = fitted;
            dirty = true;
        }
        if self.tool != prev.tool {
            element.widget.tool = self.tool;
            if self.tool != Tool::Pen {
                element.widget.session.pen_cancel();
            }
            dirty = true;
        }
        if self.groups != prev.groups {
            element.widget.groups = self.groups.clone();
            dirty = true;
        }
        if self.mark != prev.mark {
            element.widget.mark = self.mark;
            dirty = true;
        }
        if self.view != prev.view {
            element.widget.view = self.view;
            dirty = true;
        }
        if !Arc::ptr_eq(&self.ghosts, &prev.ghosts) {
            element.widget.ghosts = self.ghosts.clone();
            dirty = true;
        }
        if !interp_eq(&self.interp, &prev.interp) {
            element.widget.interp = self.interp.clone();
            dirty = true;
        }
        if self.underlay != prev.underlay {
            element.widget.underlay = self.underlay.clone();
            dirty = true;
        }
        // The buffer is the widget's while the tool is in hand, so this
        // only replaces it when the app supplies a different one: a new
        // master, a reopened glyph, or the tool being picked up.
        if self.text != element.widget.text_inputs {
            match (&self.text, element.widget.text.as_mut()) {
                // A document/tab switch restores that tab's parked text rather
                // than carrying the previous widget-owned buffer across.
                (Some(inputs), Some(_))
                    if element
                        .widget
                        .text_inputs
                        .as_ref()
                        .is_some_and(|old| !inputs.same_context(old)) =>
                {
                    element.widget.text = Some(crate::edit::text_tool::TextState::new(inputs));
                    element.widget.fit_text();
                    element.ctx.request_layout();
                }
                // Same tool, new master or edited glyph: keep what has
                // been typed and re-read the metrics.
                (Some(inputs), Some(state)) => state.refresh(inputs),
                (Some(inputs), None) => {
                    element.widget.text = Some(crate::edit::text_tool::TextState::new(inputs));
                    element.widget.fit_text();
                    element.ctx.request_layout();
                }
                // Park the buffer between tool changes; only Text handles typing.
                (None, _) => {}
            }
            element.widget.text_inputs = self.text.clone();
            dirty = true;
        }
        if dirty {
            element.ctx.request_render();
        }
    }

    fn teardown(&self, (): &mut Self::ViewState, _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        (): &mut Self::ViewState,
        message: &mut MessageCtx,
        element: Mut<'_, Self::Element>,
        app: &mut Workspace,
    ) -> MessageResult<()> {
        match message.take_message::<EditorEvent>() {
            Some(event) => {
                // The island is the live source of truth while editing. Pull its
                // session back into the app before the callback runs, so save and
                // the grid preview see the edits (the widget edits its own clone).
                app.sync_session_from(&mut element.widget.session);
                (self.on_event)(app, *event);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::theme::default_property_set;
    use masonry_testing::TestHarness;

    fn session() -> Session {
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
        let mut contour = norad::Contour::default();
        for (x, y) in [(0.0, 0.0), (400.0, 0.0), (400.0, 700.0), (0.0, 700.0)] {
            contour.points.push(norad::ContourPoint::new(
                x,
                y,
                norad::PointType::Line,
                false,
                None,
                None,
            ));
        }
        glyph.contours.push(contour);
        font.default_layer_mut().insert_glyph(glyph);
        Session::new(&font, "A").expect("the glyph is there")
    }

    fn widget() -> EditorWidget {
        EditorWidget {
            session: session(),
            palette: Arc::new(Palette::load("dark")),
            groups: (String::new(), String::new()),
            mark: None,
            tool: Tool::Select,
            ghosts: Arc::new(Vec::new()),
            interp: None,
            underlay: Underlay::default(),
            text: None,
            text_inputs: None,
            size: Size::ZERO,
            drag: Drag::None,
            hover: None,
            menu: None,
            view: ViewOptions::default(),
            field: None,
            field_buf: String::new(),
        }
    }

    #[test]
    fn start_markers_use_closed_contours_and_first_on_curve_direction() {
        let mut editor = widget();
        let affine = editor.session.viewport.affine();
        let initial = editor.start_markers();
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].0, (0, 0));
        assert_eq!(initial[0].1, affine * Point::new(0.0, 0.0));
        assert_eq!(initial[0].2, affine * Point::new(400.0, 0.0));
        editor.session.glyph.contours[0].points[0].typ = norad::PointType::OffCurve;
        assert_eq!(editor.start_markers()[0].0, (0, 1), "skip leading handles");
        editor.session.glyph.contours[0].points[0].typ = norad::PointType::Move;
        assert!(
            editor.start_markers().is_empty(),
            "open paths have no marker"
        );
        editor.session.glyph.contours[0].points.clear();
        assert!(
            editor.start_markers().is_empty(),
            "empty contours are harmless"
        );
    }

    #[test]
    fn parked_text_does_not_consume_outline_tool_typing() {
        use masonry::core::Ime;
        use masonry::core::keyboard::{Code, KeyboardEvent, Modifiers};
        let mut editor = widget();
        editor.text = Some(crate::edit::text_tool::TextState::test_buffer());
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        harness.focus_on(Some(harness.root_id()));
        let typed = || {
            TextEvent::Keyboard(KeyboardEvent {
                state: KeyState::Down,
                key: Key::Character("B".into()),
                code: Code::Unidentified,
                ..KeyboardEvent::default()
            })
        };
        let named = |key| {
            TextEvent::Keyboard(KeyboardEvent {
                state: KeyState::Down,
                key: Key::Named(key),
                code: Code::Unidentified,
                ..KeyboardEvent::default()
            })
        };
        let shifted = |key| {
            TextEvent::Keyboard(KeyboardEvent {
                state: KeyState::Down,
                key: Key::Named(key),
                code: Code::Unidentified,
                modifiers: Modifiers::SHIFT,
                ..KeyboardEvent::default()
            })
        };
        let command = |character: &'static str| {
            TextEvent::Keyboard(KeyboardEvent {
                state: KeyState::Down,
                key: Key::Character(character.into()),
                code: Code::Unidentified,
                modifiers: Modifiers::META,
                ..KeyboardEvent::default()
            })
        };
        harness.process_text_event(typed());
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
            1
        );
        harness.edit_root_widget(|root| root.widget.tool = Tool::Text);
        harness.process_text_event(typed());
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
            1,
            "logical key text waits for the IME commit"
        );
        harness.process_text_event(TextEvent::Ime(Ime::Preedit("B".into(), Some((0, 1)))));
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
            1,
            "preedit is visible state, not committed text"
        );
        harness.process_text_event(TextEvent::Ime(Ime::Preedit(String::new(), None)));
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().preedit.clone()),
            "",
            "empty preedit cancels composition"
        );
        harness.process_text_event(TextEvent::Ime(Ime::Preedit("B".into(), Some((0, 1)))));
        harness.process_text_event(TextEvent::Ime(Ime::Commit("B".into())));
        let (event, _) = harness
            .pop_action::<EditorEvent>()
            .expect("a committed edit reports its logical text");
        assert!(
            matches!(event, EditorEvent::TextChanged(text) if text == "AB"),
            "the view receives the complete committed buffer"
        );
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
            2
        );
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().preedit.clone()),
            "",
            "commit clears the composition preview"
        );
        harness.process_text_event(named(NamedKey::Enter));
        harness.process_text_event(TextEvent::Ime(Ime::Commit("A".into())));
        harness.process_text_event(named(NamedKey::ArrowUp));
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.cursor()),
            1,
            "up preserves the caret column on the preceding line"
        );
        harness.process_text_event(named(NamedKey::End));
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.cursor()),
            2
        );
        harness.process_text_event(named(NamedKey::Home));
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.cursor()),
            0
        );
        harness.process_text_event(named(NamedKey::ArrowDown));
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.cursor()),
            3,
            "down reaches the same column on the following line"
        );
        harness.process_text_event(shifted(NamedKey::End));
        assert_eq!(
            harness.edit_root_widget(|root| root
                .widget
                .text
                .as_ref()
                .unwrap()
                .buffer
                .selection_range()),
            Some(3..4),
            "Shift+End selects the final sort"
        );
        harness.process_text_event(named(NamedKey::Backspace));
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
            3,
            "Backspace removes the selection"
        );
        assert_eq!(
            harness.edit_root_widget(|root| root
                .widget
                .text
                .as_ref()
                .unwrap()
                .buffer
                .selection_range()),
            None
        );
        harness.process_text_event(TextEvent::Ime(Ime::Commit("A".into())));
        harness.edit_root_widget(|root| root.widget.tool = Tool::Select);
        harness.process_text_event(typed());
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
            4
        );
        harness.edit_root_widget(|root| {
            root.widget.tool = Tool::Text;
            let buffer = &mut root.widget.text.as_mut().unwrap().buffer;
            buffer.select_range(0, buffer.len());
        });
        harness.process_text_event(command("c"));
        assert_eq!(harness.clipboard_contents(), "AB\nA");
        harness.process_text_event(command("x"));
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
            0,
            "cut removes exactly the copied logical selection"
        );
        harness.process_text_event(TextEvent::ClipboardPaste("A\r\nB".into()));
        assert_eq!(
            harness.edit_root_widget(|root| {
                let buffer = &mut root.widget.text.as_mut().unwrap().buffer;
                buffer.select_range(0, buffer.len());
                buffer.selected_text()
            }),
            Some("A\nB".into()),
            "paste normalizes platform newlines and preserves text"
        );
    }

    /// Typing in the width box changes the advance, and only on Enter.
    #[test]
    fn metric_box_commits_on_enter() {
        let mut widget = widget();
        widget.size = Size::new(600.0, 400.0);
        let before = widget.session.advance();
        widget.focus_metric(MetricField::Width);
        widget.field_buf.clear();
        widget.field_buf.push_str("900");
        assert_eq!(widget.session.advance(), before, "not until Enter");
        assert!(widget.commit_metric());
        assert_eq!(widget.session.advance(), 900.0);
    }

    /// The boxes are where the centered panel paints them.
    #[test]
    fn metric_boxes_sit_in_the_panel() {
        let mut widget = widget();
        widget.size = Size::new(600.0, 400.0);
        let boxes = widget.metric_boxes().expect("the panel fits");
        let (_, origin) = (0, widget.metrics_panel_origin().expect("it fits"));
        for (_, rect) in boxes {
            assert!(rect.x0 >= origin.0, "inside the panel's left edge");
            assert!(rect.x1 <= origin.0 + PANEL_WIDTH, "inside its right edge");
            assert!(rect.y0 >= origin.1, "below its top");
        }
    }

    #[test]
    fn moved_metrics_fields_receive_clicks_and_commit_through_keyboard_events() {
        use masonry::core::keyboard::KeyboardEvent;
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget().prepare(), (786, 510));
        let boxes = harness.edit_root_widget(|root| root.widget.metric_boxes().unwrap());
        for (field, rect) in boxes {
            harness.mouse_move(rect.center());
            harness.mouse_button_press(Some(PointerButton::Primary));
            assert!(harness.edit_root_widget(|root| root.widget.field == Some(field)));
        }
        let width = boxes[1].1;
        harness.mouse_move(width.center());
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.edit_root_widget(|root| root.widget.field_buf = "900".into());
        harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Named(NamedKey::Enter),
            ..KeyboardEvent::default()
        }));
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.session.advance()),
            900.0
        );
        assert!(harness.edit_root_widget(|root| root.widget.field.is_none()));
    }

    #[test]
    fn metrics_card_keeps_its_bottom_clearance_and_hides_when_it_cannot_fit() {
        let mut widget = widget();
        for width in [606.0, 786.0] {
            widget.size = Size::new(width, 510.0);
            let (left, top) = widget.metrics_panel_origin().unwrap();
            assert_eq!(left + PANEL_WIDTH / 2.0, width / 2.0);
            assert_eq!(top + PANEL_HEIGHT, 498.0);
        }
        widget.size = Size::new(280.0, 510.0);
        assert!(widget.metric_boxes().is_none());
        widget.size = Size::new(786.0, 90.0);
        assert!(widget.metric_boxes().is_none());
    }

    /// A right click opens the menu as a layer, and the editor remembers
    /// which layer so a second right click does not stack another one.
    #[test]
    fn right_click_opens_one_menu_layer() {
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget().prepare(), (600, 400));
        harness.mouse_move(Point::new(300.0, 200.0));
        harness.mouse_button_press(Some(PointerButton::Secondary));
        let first = harness.edit_root_widget(|root| root.widget.menu);
        assert!(first.is_some(), "the menu layer was created");

        harness.mouse_button_press(Some(PointerButton::Secondary));
        let second = harness.edit_root_widget(|root| root.widget.menu);
        assert_eq!(first, second, "a second right click did not stack a layer");
    }

    /// A left click still edits rather than being eaten by menu handling.
    #[test]
    fn left_click_is_not_swallowed() {
        let mut harness =
            TestHarness::create_with_size(default_property_set(), widget().prepare(), (600, 400));
        harness.mouse_move(Point::new(300.0, 200.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        let menu = harness.edit_root_widget(|root| root.widget.menu);
        assert!(menu.is_none(), "a left click does not open the menu");
    }
}

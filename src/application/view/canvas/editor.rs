// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The glyph editor island: a canvas widget that owns the edit session
//! and gesture state, and the view that hosts it.

use std::sync::Arc;
use std::sync::Mutex;

use masonry::accesskit::{Node, Role};
use masonry::core::keyboard::{Key, KeyState, NamedKey};
use masonry::core::{
    AccessCtx, ChildrenIds, CursorIcon, EventCtx, LayerType, LayoutCtx, MeasureCtx, NewWidget,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent, PointerScrollEvent, PointerUpdate,
    PropertiesMut, PropertiesRef, QueryCtx, RegisterCtx, ScrollDelta, TextEvent, UpdateCtx, Widget,
    WidgetId,
};
use masonry::imaging::Painter;
use masonry::kurbo;
use masonry::kurbo::{Affine, Axis, Circle, Line, Point, Rect, Shape as _, Size, Stroke};
use masonry::layout::{LenReq, Length};
use masonry::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};
use runebender::font::{AnchorId, ContourId, PointId};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

use crate::application::editor::session::{Session, SessionSyncOutcome};
use crate::application::editor::tools::label::LABEL_GRID;
use crate::application::editor::tools::sketch::SketchLayer;
use crate::application::view::theme::Palette;
use crate::application::widgets::context_menu::{ContextMenu, MenuAction, MenuRow, MenuTarget};
use crate::application::widgets::text_label::{self, Anchor};
use crate::application::workspace::Tool;
use crate::application::workspace::Workspace;

use crate::application::view::design::{
    ANCHOR_DIAMOND_SCALE, METRICS_CARD_BOTTOM, METRICS_CARD_HEADER as PANEL_HEADER,
    METRICS_CARD_HEIGHT as PANEL_HEIGHT, METRICS_CARD_INSET as PANEL_PAD,
    METRICS_CARD_WIDTH as PANEL_WIDTH, METRICS_FIELD_GAP, METRICS_FIELD_HEIGHT as PANEL_ROW,
    METRICS_FIELD_START, METRICS_FIELD_WIDTH, POINT_CORNER_RADIUS, POINT_CURVE_RADIUS,
    POINT_GRID_COARSE_LINE_WIDTH, POINT_GRID_FINE_LINE_WIDTH, POINT_HALO_EXTRA, POINT_RING_WIDTH,
    POINT_SELECTED_GROW, START_MARKER_BACK, START_MARKER_HALF_WIDTH, START_MARKER_SCALE,
    START_MARKER_SMOOTH_CUT, START_MARKER_TIP, Stroke as DesignStroke, TEXT_CURSOR_CAP_FRACTION,
    TEXT_CURSOR_CAP_MAX, TEXT_CURSOR_CAP_MIN, TextSize, point_marker_scale,
};

const HIT_RADIUS_PX: f64 = 8.0;
const CURSOR_BLINK_HALF_CYCLE_NS: u64 = 500_000_000;
const CURSOR_BLINK_CYCLE_NS: u64 = CURSOR_BLINK_HALF_CYCLE_NS * 2;

fn cursor_visible_at(elapsed_ns: u64) -> bool {
    elapsed_ns.rem_euclid(CURSOR_BLINK_CYCLE_NS) <= CURSOR_BLINK_HALF_CYCLE_NS
}

/// Append one round design-grid dot in screen coordinates to `marks`.
///
/// The visible grid holds thousands of dots, so each one is appended to the shared path
/// rather than built as a path of its own.
fn push_grid_dot(marks: &mut kurbo::BezPath, at: Point, diameter: f64) {
    marks.extend(Circle::new(at, diameter / 2.0).path_elements(0.1));
}

/// A horizontal rule whose visual centre stays on the supplied coordinate.
fn horizontal_rule_rect(x0: f64, x1: f64, y: f64, width: f64) -> Rect {
    let half = width / 2.0;
    Rect::new(x0.min(x1), y - half, x0.max(x1), y + half)
}

/// A vertical rule whose visual centre stays on the supplied coordinate.
fn vertical_rule_rect(x: f64, y0: f64, y1: f64, width: f64) -> Rect {
    let half = width / 2.0;
    Rect::new(x - half, y0.min(y1), x + half, y0.max(y1))
}

/// Hermite ease from zero to one.
fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The design grid's coarse and fine opacity at this zoom.
fn grid_alphas(zoom: f64) -> (f64, f64) {
    (
        smoothstep((zoom - 0.8) / 0.8),
        smoothstep((zoom - 8.0) / 8.0),
    )
}

/// The design grid's coarse and fine dot diameters at this zoom.
fn grid_dot_sizes(zoom: f64) -> (f64, f64) {
    (
        (8.0 * zoom * 0.2).clamp(1.5, 5.0),
        (2.0 * zoom * 0.125).clamp(1.0, 3.5),
    )
}

/// Build the part of one design-grid level visible through a point marker.
fn point_grid_marks(
    affine: Affine,
    center: Point,
    radius: f64,
    square: bool,
    spacing: f64,
    dot_size: f64,
    grid_lines: bool,
) -> kurbo::BezPath {
    let inv = affine.inverse();
    let a = (inv * Point::new(center.x - radius, center.y)).x;
    let b = (inv * Point::new(center.x + radius, center.y)).x;
    let (lo_x, hi_x) = (a.min(b), a.max(b));
    let a = (inv * Point::new(center.x, center.y - radius)).y;
    let b = (inv * Point::new(center.x, center.y + radius)).y;
    let (lo_y, hi_y) = (a.min(b), a.max(b));
    let xs = grid_index((lo_x / spacing).ceil())..=grid_index((hi_x / spacing).floor());
    let ys = grid_index((lo_y / spacing).ceil())..=grid_index((hi_y / spacing).floor());
    let mut marks = kurbo::BezPath::new();

    if grid_lines {
        let half_at = |distance: f64| {
            if square {
                radius
            } else {
                (radius * radius - distance * distance).max(0.0).sqrt()
            }
        };
        for x_index in xs.clone() {
            let x = (affine * Point::new(x_index as f64 * spacing, 0.0)).x;
            let half = half_at(x - center.x);
            if half > 0.2 {
                marks.move_to(Point::new(x, center.y - half));
                marks.line_to(Point::new(x, center.y + half));
            }
        }
        for y_index in ys {
            let y = (affine * Point::new(0.0, y_index as f64 * spacing)).y;
            let half = half_at(y - center.y);
            if half > 0.2 {
                marks.move_to(Point::new(center.x - half, y));
                marks.line_to(Point::new(center.x + half, y));
            }
        }
    } else {
        for x_index in xs {
            for y_index in ys.clone() {
                let at = affine * Point::new(x_index as f64 * spacing, y_index as f64 * spacing);
                let dx = at.x - center.x;
                let dy = at.y - center.y;
                let inside = if square {
                    dx.abs() <= radius && dy.abs() <= radius
                } else {
                    dx * dx + dy * dy <= radius * radius
                };
                if inside {
                    push_grid_dot(&mut marks, at, dot_size);
                }
            }
        }
    }
    marks
}

fn point_marker_shape(center: Point, radius: f64, square: bool) -> kurbo::BezPath {
    if square {
        kurbo::Shape::to_path(
            &Rect::new(
                center.x - radius,
                center.y - radius,
                center.x + radius,
                center.y + radius,
            ),
            0.1,
        )
    } else {
        kurbo::Shape::to_path(&Circle::new(center, radius), 0.15)
    }
}

/// Replace a closed contour's first node with the GPUI direction wedge.
fn direction_marker_shape(
    center: Point,
    toward: Point,
    radius: f64,
    smooth: bool,
) -> Option<kurbo::BezPath> {
    let direction = toward - center;
    let length = direction.hypot();
    if length < f64::EPSILON {
        return None;
    }
    let forward = direction / length;
    let side = kurbo::Vec2::new(-forward.y, forward.x);
    let radius = radius * START_MARKER_SCALE;
    let tip = center + forward * radius * START_MARKER_TIP;
    let left =
        center - forward * radius * START_MARKER_BACK + side * radius * START_MARKER_HALF_WIDTH;
    let right =
        center - forward * radius * START_MARKER_BACK - side * radius * START_MARKER_HALF_WIDTH;

    let mut path = kurbo::BezPath::new();
    if smooth {
        let toward = |a: Point, b: Point| {
            Point::new(
                a.x + (b.x - a.x) * START_MARKER_SMOOTH_CUT,
                a.y + (b.y - a.y) * START_MARKER_SMOOTH_CUT,
            )
        };
        path.move_to(toward(tip, right));
        path.quad_to(tip, toward(tip, left));
        path.line_to(toward(left, tip));
        path.quad_to(left, toward(left, right));
        path.line_to(toward(right, left));
        path.quad_to(right, toward(right, tip));
    } else {
        path.move_to(tip);
        path.line_to(left);
        path.line_to(right);
    }
    path.close_path();
    Some(path)
}

/// Metric heights shared by every text sort, deduplicated so equal font
/// metrics do not paint darker than their neighbours.
fn text_sort_metric_ys(metrics: &crate::application::editor::session::Metrics) -> Vec<f64> {
    let mut ys = vec![
        metrics.descender,
        0.0,
        metrics.ascender,
        metrics.upm.max(metrics.ascender),
        metrics.x_height,
        metrics.cap_height,
    ];
    ys.retain(|y| y.is_finite());
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    ys
}

/// Draw dark marks inward from each intersection of a sort edge and metric line.
/// Paint these after the quiet metric rules so neighbouring sorts cannot cover them.
fn paint_metric_crosses(
    painter: &mut Painter<'_>,
    xs: [f64; 2],
    y_bounds: [f64; 2],
    ys: impl Clone + Iterator<Item = f64>,
    mark: f64,
    color: xilem::Color,
) {
    if mark < 3.0 {
        return;
    }
    let mark = mark * 0.5;
    let left = xs[0].min(xs[1]);
    let right = xs[0].max(xs[1]);
    let top = y_bounds[0].min(y_bounds[1]);
    let bottom = y_bounds[0].max(y_bounds[1]);
    let inward = mark.min((right - left) / 2.0);
    let rule = Stroke::new(DesignStroke::Hairline.px());
    for (x, inner_x) in [(left, left + inward), (right, right - inward)] {
        for y in ys.clone() {
            painter
                .stroke(Line::new((x, y), (inner_x, y)), &rule, color)
                .draw();
            painter
                .stroke(
                    Line::new((x, (y - mark).max(top)), (x, (y + mark).min(bottom))),
                    &rule,
                    color,
                )
                .draw();
        }
    }
}

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
            MenuAction::ToggleComponentAlignment => {
                this.ctx
                    .submit_action::<EditorEvent>(EditorEvent::ToggleComponentAlignment);
                false
            }
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
    /// Change manual placement for the component selected by the context click.
    ToggleComponentAlignment,
    /// A composed sort was activated: open that glyph without losing the line,
    /// and keep the tool that established the interaction.
    EditGlyph { name: String, tool: Tool },
    /// The committed logical text changed; park it with the active tab.
    TextChanged(String),
    /// Widget-owned selected shaped occurrence and its proof settings.
    TextProofSelection(Result<runebender::text::buffer::TextProofSelection, String>),
    /// Session-only brush ink changed; the canonical glyph did not.
    SketchChanged,
    /// Cmd+Z: the app undoes on the master's pile.
    Undo,
    /// Cmd+Shift+Z or Cmd+Y.
    Redo,
}

fn dispatch_editor_event(
    app: &mut Workspace,
    session: &mut Session,
    event: EditorEvent,
    on_event: impl FnOnce(&mut Workspace, EditorEvent),
) {
    if matches!(&event, EditorEvent::SketchChanged) {
        on_event(app, event);
        return;
    }
    match app.sync_session_from(session) {
        SessionSyncOutcome::Changed => on_event(app, event),
        SessionSyncOutcome::Unchanged if !matches!(event, EditorEvent::Edited) => {
            on_event(app, event);
        }
        SessionSyncOutcome::Unchanged | SessionSyncOutcome::Rejected => {}
    }
}

/// One color per letter for the label tool, cycling through the theme's mark colors.
/// Ten hues for labeled letters, chosen to read on the gray canvas in both themes. Letters
/// within three places of each other in the text never share one.
pub(crate) fn label_colors(_pal: &Palette) -> Vec<xilem::Color> {
    const HUES: [u32; 10] = [
        0xD94F3D, 0x3A7BD5, 0xE0A826, 0x3C9D5E, 0x8A5BD6, 0xE6702E, 0xD9529C, 0x2A9DA8, 0x9C7A2E,
        0x5E6BD8,
    ];
    let color = |value: u32| {
        xilem::Color::from_rgba8(
            ((value >> 16) & 0xFF) as u8,
            ((value >> 8) & 0xFF) as u8,
            (value & 0xFF) as u8,
            255,
        )
    };
    (0..HUES.len())
        .map(|index| color(HUES[index * 3 % HUES.len()]))
        .collect()
}

enum Drag {
    Metaballs {
        last: Point,
        changed: bool,
    },
    None,
    Points {
        start: Point,
    },
    Pan {
        last: Point,
    },
    Sketch {
        last: Point,
        changed: bool,
    },
    /// Pen mouse-down at `origin` (design space); becomes handle-drag past a threshold.
    Pen {
        origin: Point,
        dragging: bool,
        point_count: usize,
        active_contour: Option<ContourId>,
    },
    /// Rubber-band selection in screen space.
    Marquee {
        start: Point,
        current: Point,
        additive: bool,
    },
    /// Freehand selection boundary in screen space.
    Lasso {
        points: Vec<Point>,
        additive: bool,
    },
    /// Moving the layer's picture, or resizing it from a corner. `start` is in design space,
    /// `origin` is the picture's transform at the press, and `anchor` is the corner that stays
    /// put while the opposite one, `corner`, follows the pointer.
    Image {
        start: Point,
        origin: Affine,
        anchor: Option<Point>,
        corner: Point,
    },
    /// Label tool: a press in screen space. It becomes a freehand region when it travels,
    /// and a polygon corner when it does not.
    Label {
        points: Vec<Point>,
        /// Option was held at the press: a click unpaints instead of painting.
        erase: bool,
    },
    /// A polygon corner of a label region follows the pointer; the session holds where.
    LabelCorner {
        region: usize,
        /// Option is held: the corner is being pulled away from others, so it does not snap
        /// back onto a neighbor's corner.
        detach: bool,
    },
    /// Shift-drag in the label tool: a box that selects the corners inside it, in design
    /// space.
    LabelMarquee {
        start: Point,
        current: Point,
    },
    /// Several label corners follow the pointer; the session holds where.
    LabelCorners {
        start: Point,
    },
    /// A whole lasso polygon follows the pointer; the session holds where.
    LabelRegion {
        region: usize,
        start: Point,
    },
    /// A cut, or one end of it, follows the pointer; the session holds where.
    LabelCut {
        cut: usize,
        end: Option<usize>,
        start: Point,
    },
    /// Drawing a shape; endpoints in design space.
    Shape {
        start: Point,
        current: Point,
    },
    /// Dragging an anchor by stable document identity.
    Anchor {
        id: AnchorId,
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
    sketch: Arc<Mutex<SketchLayer>>,
    sketch_source: usize,
    /// The sketch ink last rasterized for the canvas, by sketch revision and ink alpha,
    /// so a zoom or hover repaint does not rebuild and re-upload a 512 px image.
    sketch_image: Option<(u64, u32, ImageData)>,
    /// Space is held: pan while showing only the filled design.
    preview_mode: bool,
    ghosts: Arc<Vec<Arc<kurbo::BezPath>>>,
    /// Read-only interpolated instance overlay at the current axis location.
    interp: Option<Arc<kurbo::BezPath>>,
    /// Background layer and reference glyph, drawn under everything.
    underlay: Underlay,
    /// The tab's text composition, kept while outline tools edit one sort.
    text: Option<crate::application::editor::tools::text::TextState>,
    /// The master the buffer was built from.
    text_inputs: Option<crate::application::editor::tools::text::TextInputs>,
    size: Size,
    /// The picture's transform while it is dragged, drawn before it is committed.
    image_preview: Option<Affine>,
    drag: Drag,
    /// Last cursor position in design space, for the pen preview segment.
    hover: Option<Point>,
    /// Where a pen click would add a point to an existing segment, and
    /// whether that point splits a curve.
    insert_preview: Option<(Point, bool)>,
    /// What the label tool's pointer is over, for the glow, the cursor and the caption.
    label_hover: Option<crate::application::editor::tools::label::LabelHit>,
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
    /// Text caret blink phase; reset whenever the person moves or edits it.
    cursor_blink_elapsed_ns: u64,
    cursor_visible: bool,
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
    /// The numbers being edited sit with the drawing instead of in a column at
    /// the side, which keeps the relationship visible.
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
        if self.preview_mode || self.session.neural {
            return None;
        }
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
        self.field_buf = match (field, self.session.side_bearings()) {
            (MetricField::Lsb, Some(sb)) => sb.lsb.to_string(),
            (MetricField::Width, Some(sb)) => format!("{:.0}", sb.advance),
            (MetricField::Rsb, Some(sb)) => sb.rsb.to_string(),
            // A blank glyph has no ink bounds, so its sidebearings are
            // undefined. Keep the controls usable: width remains its stored
            // advance, while a right sidebearing is the full advance from the
            // origin to the right margin.
            (MetricField::Lsb, None) => "0".into(),
            (MetricField::Width | MetricField::Rsb, None) => {
                format!("{:.0}", self.session.advance())
            }
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
        match field {
            MetricField::Lsb => {
                if let Some(sb) = self.session.side_bearings() {
                    self.session.shift_glyph(value - sb.min_x);
                } else {
                    return false;
                }
            }
            MetricField::Width => self.session.set_advance(value),
            MetricField::Rsb => {
                let width = self
                    .session
                    .side_bearings()
                    .map_or(value, |sb| sb.max_x + value);
                self.session.set_advance(width);
            }
        }
        true
    }

    fn paint_metrics(&self, painter: &mut Painter<'_>) {
        const PAD: f64 = PANEL_PAD;
        // A neural item has no advance or sidebearings to show.
        if self.session.neural {
            return;
        }
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
        let frame = Rect::new(left, top, left + width, top + PANEL_HEIGHT);
        let rounded = crate::application::view::design::rounded_rect_path;
        let radius = pal.corner_radius;
        painter
            .fill(
                rounded(frame + kurbo::Vec2::new(-4.0, 4.0), radius),
                pal.cell_shadow().with_alpha(0.5),
            )
            .draw();
        painter.fill(rounded(frame, radius), pal.panel).draw();
        let (header_bg, header_ink) = pal.floating_header_colors(self.mark);
        painter.with_fill_clip(rounded(frame, radius), |painter| {
            painter
                .fill(
                    Rect::new(left, top, left + width, top + PANEL_HEADER),
                    header_bg,
                )
                .draw();
        });
        painter
            .stroke(
                rounded(frame, radius),
                &Stroke::new(DesignStroke::Hairline.px()),
                pal.outline,
            )
            .draw();
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

        let header_text = |painter: &mut Painter<'_>, x: f64, s: &str, size: f32, anchor| {
            text_label::draw(
                painter,
                Point::new(left + x, top + PANEL_HEADER / 2.0),
                s,
                size,
                header_ink,
                anchor,
            );
        };
        header_text(
            painter,
            PAD,
            &self.session.glyph_name,
            TextSize::Body.px(),
            Anchor::Start,
        );
        // The codepoint, right aligned on the same line, as the GPUI
        // build has it. This one is free because the session carries the
        // glyph; the kerning groups it also shows are not, because they
        // live in the font model, and reaching them means threading two
        // strings through the widget, the view, `build`, `rebuild` and
        // the constructor.
        if let Some(codepoint) = self.session.codepoint() {
            header_text(
                painter,
                width - PAD,
                &format!("{:04X}", codepoint as u32),
                TextSize::Body.px(),
                Anchor::End,
            );
        }
        // Three boxes you can type in. Each one is drawn here and hit tested
        // from the same rectangles, because a painted control that computes
        // its geometry twice will drift the moment either copy is edited.
        //
        // The card is glyph chrome, not an outline measurement. Keep its
        // inputs present when the final point is deleted. An empty glyph has
        // no measured ink bounds, so the left value is the origin and the
        // right value is its full advance.
        if let Some(boxes) = self.metric_boxes() {
            let group_box = |column: f64| {
                let x = left + PANEL_PAD + column * (METRICS_FIELD_WIDTH + METRICS_FIELD_GAP);
                Rect::new(x, boxes[0].1.y0, x + METRICS_FIELD_WIDTH, boxes[0].1.y1)
            };
            for (rect, group) in [
                (group_box(0.0), &self.groups.0),
                (group_box(4.0), &self.groups.1),
            ] {
                painter
                    .fill(rounded(rect, pal.control_radius), pal.field())
                    .draw();
                let line = DesignStroke::Hairline.px();
                let half = line / 2.0;
                painter
                    .stroke(
                        rounded(
                            Rect::new(
                                rect.x0 + half,
                                rect.y0 + half,
                                rect.x1 - half,
                                rect.y1 - half,
                            ),
                            (pal.control_radius - half).max(0.0),
                        ),
                        &Stroke::new(line),
                        pal.field_outline,
                    )
                    .draw();
                text_label::draw(
                    painter,
                    rect.center(),
                    &group_label(group),
                    TextSize::Body.px(),
                    pal.text,
                    Anchor::Middle,
                );
            }
            for (field, rect) in boxes {
                let focused = self.field == Some(field);
                let value = if focused {
                    self.field_buf.clone()
                } else {
                    match (field, bearings) {
                        (MetricField::Lsb, Some(sb)) => sb.lsb.to_string(),
                        (MetricField::Width, Some(sb)) => format!("{:.0}", sb.advance),
                        (MetricField::Rsb, Some(sb)) => sb.rsb.to_string(),
                        (MetricField::Lsb, None) => "0".into(),
                        (MetricField::Width | MetricField::Rsb, None) => {
                            format!("{:.0}", self.session.advance())
                        }
                    }
                };
                let border = if focused { pal.text } else { pal.field_outline };
                painter
                    .fill(rounded(rect, pal.control_radius), pal.field())
                    .draw();
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
                painter
                    .stroke(
                        rounded(keyline, (pal.control_radius - half).max(0.0)),
                        &Stroke::new(width),
                        border,
                    )
                    .draw();
                let baseline = rect.center().y;
                text_label::draw(
                    painter,
                    Point::new(
                        rect.x0 + crate::application::view::design::INPUT_HORIZONTAL_INSET,
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
                    painter.fill(caret, pal.outline).draw();
                }
            }
        }
    }

    fn screen_points(&self) -> Vec<(PointId, Point, bool, bool, bool)> {
        let affine = self.glyph_affine();
        self.session
            .points()
            .into_iter()
            .map(|p| (p.id, affine * p.point, p.on_curve, p.smooth, p.start))
            .collect()
    }

    /// Closed contours expose their first on-curve point and outgoing direction.
    /// Open paths and contours without an on-curve point have no start marker.
    fn start_markers(&self) -> Vec<(PointId, Point, Point)> {
        let affine = self.glyph_affine();
        self.session
            .start_markers()
            .into_iter()
            .map(|(id, from, to)| (id, affine * from, affine * to))
            .collect()
    }

    fn hit_point(&self, at: Point) -> Option<PointId> {
        self.screen_points()
            .into_iter()
            .filter(|(_, sp, _, _, _)| sp.distance(at) <= HIT_RADIUS_PX)
            .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
            .map(|(id, _, _, _, _)| id)
    }

    /// The active sort's position in the composed line, or the origin when
    /// this tab has no text composition.
    fn active_sort_origin(&self) -> Point {
        self.text
            .as_ref()
            .and_then(crate::application::editor::tools::text::TextState::active_origin)
            .unwrap_or(Point::ORIGIN)
    }

    /// Design-to-screen transform for the glyph currently being edited.
    fn glyph_affine(&self) -> Affine {
        self.session.viewport.affine() * Affine::translate(self.active_sort_origin().to_vec2())
    }

    /// Convert a screen point into the active sort's glyph-local design space.
    fn screen_to_glyph_design(&self, at: Point) -> Point {
        self.session.viewport.screen_to_design(at) - self.active_sort_origin().to_vec2()
    }

    fn fit(&mut self) {
        // A neural canvas has no advance or metrics box to frame, and its
        // ink sits wherever the calligraphy was traced. Frame the ink.
        if self.session.neural && self.fit_ink() {
            return;
        }
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

    /// Center the outline's bounds in the canvas. Returns false when the
    /// layer has no outline, so the caller can fall back to the metrics box.
    fn fit_ink(&mut self) -> bool {
        let Some(ink) = self
            .session
            .segment_bounds()
            .into_iter()
            .reduce(|a, b| a.union(b))
        else {
            return false;
        };
        let (width, height) = (ink.width().max(1.0), ink.height().max(1.0));
        let zoom = ((self.size.width * 0.7) / width).min((self.size.height * 0.7) / height);
        let viewport = &mut self.session.viewport;
        viewport.zoom = zoom.max(0.001);
        let center = ink.center();
        viewport.offset = kurbo::Vec2::new(
            self.size.width / 2.0 - center.x * viewport.zoom,
            self.size.height / 2.0 + center.y * viewport.zoom,
        );
        self.session.fitted = true;
        true
    }

    /// Frame the whole text line rather than one glyph.
    ///
    /// The editor's normal fit is one advance wide, which shows about two
    /// letters of a word. Typing is for judging spacing, so the view has
    /// to hold the line.
    fn fit_text(&mut self) {
        let Some(text) = &self.text else {
            return;
        };
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
        // Text lines use the full sort box, not merely ascender to
        // descender. Virtua's upm is above its ascender; fitting the shorter
        // range made the top metric cross and caret cap collide with the
        // title bar while the glyphs looked too large.
        let sort_top = m.upm.max(m.ascender);
        let design_height = (sort_top - m.descender).max(1.0);
        // Leave the same breathing room as the mature editors: roughly ten
        // percent at each horizontal edge, and reserve the floating metrics
        // card instead of centering the run behind it.
        let available_height =
            (self.size.height - PANEL_HEIGHT - METRICS_CARD_BOTTOM).max(design_height * 0.001);
        let zoom = ((self.size.width * 0.8) / width).min((available_height * 0.8) / design_height);
        self.session.viewport.zoom = zoom.max(0.001);
        let center_y = (sort_top + m.descender) / 2.0;
        self.session.viewport.offset = kurbo::Vec2::new(
            (self.size.width - width * self.session.viewport.zoom) / 2.0,
            available_height / 2.0 + center_y * self.session.viewport.zoom,
        );
        self.session.fitted = true;
    }

    /// The label tool's view of the glyph: each letter's ink in that letter's color, the regions
    /// of the active letter, and the polygon being placed. Ink no letter owns keeps the plain
    /// outline, so what is left to label is what is not colored.
    fn paint_labels(&self, painter: &mut Painter<'_>, affine: Affine) {
        use crate::application::editor::tools::label::{LabelHit, LabelSelection};
        let pal = &self.palette;
        let colors = label_colors(pal);
        let outline = affine * self.session.outline();
        let selected = self.session.label.sample;
        let active = self.session.label.active;
        let lit_letter = self.session.label.hover_letter;
        // Every sample's boundary, the selected one strongest.
        for (position, sample) in self.session.neural_item().samples.iter().enumerate() {
            let (width, alpha) = if Some(position) == selected {
                (1.0, 0.6)
            } else {
                (1.0, 0.3)
            };
            painter
                .stroke(
                    affine * sample.boundary_path(),
                    &Stroke::new(width),
                    pal.text_muted.with_alpha(alpha),
                )
                .draw();
        }
        // Labeled ink in its letter's color. Shared ink shows both colors, one over the other.
        for area in self.session.label_areas() {
            let color = colors[area.letter % colors.len()];
            let lit = Some(area.sample) == selected && lit_letter == Some(area.letter);
            let path = affine * area.area;
            painter.with_fill_clip(&path, |painter| {
                painter.fill(&outline, color.with_alpha(0.85)).draw();
                if lit {
                    painter.fill(&outline, pal.canvas.with_alpha(0.35)).draw();
                }
            });
        }
        // The piece under the pointer glows.
        if let Some(LabelHit::Piece(piece)) = &self.label_hover {
            painter
                .fill(&(affine * piece.clone()), pal.text.with_alpha(0.18))
                .draw();
        }
        // Cuts: thin lines; the selected or hovered one carries its end handles.
        let picked_cut = match (&self.session.label.selected, &self.label_hover) {
            (Some(LabelSelection::Cut(cut)), _) => Some(*cut),
            (_, Some(LabelHit::Cut { cut } | LabelHit::CutEnd { cut, .. })) => Some(*cut),
            _ => None,
        };
        for (index, cut) in self.session.label_cuts().into_iter().enumerate() {
            let picked = picked_cut == Some(index);
            let line = Line::new(affine * cut.p0, affine * cut.p1);
            painter
                .stroke(
                    line,
                    &Stroke::new(if picked { 1.5 } else { 1.0 }),
                    pal.text.with_alpha(if picked { 0.9 } else { 0.5 }),
                )
                .draw();
            if picked {
                for end in [line.p0, line.p1] {
                    let handle = Circle::new(end, 3.5);
                    painter.fill(handle, pal.canvas).draw();
                    painter.stroke(handle, &Stroke::new(1.5), pal.text).draw();
                }
            }
        }
        // Every lasso polygon: a translucent fill in its letter's color and the same dark
        // outline as the drawing. The active letter's and the selected one carry colored
        // corner handles.
        let picked_corners = self.session.selected_corners();
        let shared = self.session.shared_corners();
        let picked_region = match &self.session.label.selected {
            Some(LabelSelection::Region(region)) => Some(*region),
            _ => None,
        };
        for polygon in self.session.label_polygons() {
            let color = colors[polygon.letter % colors.len()];
            let mut path = kurbo::BezPath::new();
            for (index, corner) in polygon.corners.iter().enumerate() {
                let at = affine * *corner;
                if index == 0 {
                    path.move_to(at);
                } else {
                    path.line_to(at);
                }
            }
            path.close_path();
            painter.fill(&path, color.with_alpha(0.14)).draw();
            painter
                .stroke(&path, &Stroke::new(1.0), pal.role("pathStroke"))
                .draw();
            // Every shape shows its corners; the active letter's and the picked ones are
            // larger.
            let strong = polygon.letter == active
                || picked_region == Some(polygon.region)
                || picked_corners
                    .iter()
                    .any(|(region, _)| *region == polygon.region);
            for (index, corner) in polygon.corners.iter().enumerate() {
                if shared
                    .iter()
                    .any(|(at, _)| at.distance(*corner) <= LABEL_GRID / 2.0)
                {
                    continue;
                }
                let picked = picked_corners.contains(&(polygon.region, index));
                let radius = if picked {
                    5.0
                } else if strong {
                    3.5
                } else {
                    2.5
                };
                let handle = Circle::new(affine * *corner, radius);
                painter.fill(handle, color).draw();
                painter
                    .stroke(handle, &Stroke::new(1.0), pal.role("pathStroke"))
                    .draw();
            }
            // A corner would go here: the edge under the pointer shows it.
            if let Some(LabelHit::Edge { region, at, .. }) = &self.label_hover
                && *region == polygon.region
            {
                let ghost = Circle::new(affine * *at, 3.5);
                painter.fill(ghost, color.with_alpha(0.5)).draw();
                painter
                    .stroke(ghost, &Stroke::new(1.0), pal.role("pathStroke"))
                    .draw();
            }
        }
        // Where corners of different shapes meet: one node in wedges, one color each.
        for (at, letters) in &shared {
            let picked = picked_corners.iter().any(|(region, corner)| {
                self.session
                    .corner_point(*region, *corner)
                    .is_some_and(|p| p.distance(*at) <= LABEL_GRID / 2.0)
            });
            let center = affine * *at;
            let radius = if picked { 6.0 } else { 4.5 };
            let step = std::f64::consts::TAU / letters.len() as f64;
            for (slice, letter) in letters.iter().enumerate() {
                let start = -std::f64::consts::FRAC_PI_2 + step * slice as f64;
                // From the center to the rim, around the arc, and back.
                let arc = kurbo::Arc::new(center, (radius, radius), start, step, 0.0);
                let mut wedge = kurbo::BezPath::new();
                wedge.move_to(center);
                wedge.line_to(center + kurbo::Vec2::new(start.cos(), start.sin()) * radius);
                wedge.extend(arc.append_iter(0.1));
                wedge.close_path();
                painter.fill(&wedge, colors[*letter % colors.len()]).draw();
            }
            painter
                .stroke(
                    Circle::new(center, radius),
                    &Stroke::new(1.0),
                    pal.role("pathStroke"),
                )
                .draw();
        }
        if let Drag::LabelMarquee { start, current } = &self.drag {
            let rect = affine.transform_rect_bbox(Rect::from_points(*start, *current));
            painter
                .fill(rect, pal.role("selection").with_alpha(0.1))
                .draw();
            painter
                .stroke(rect, &Stroke::new(1.0), pal.role("selection"))
                .draw();
        }
        // A freehand drag in progress: a cut is a straight line, a lasso a loop.
        if let Drag::Label { points, .. } = &self.drag
            && points.len() >= 2
        {
            let color = if selected.is_some() {
                colors[active % colors.len()]
            } else {
                pal.tool_feedback()
            };
            let design: Vec<Point> = points
                .iter()
                .map(|p| self.screen_to_glyph_design(*p))
                .collect();
            let is_cut = selected.is_some()
                && crate::application::editor::tools::label::drag_is(
                    &design,
                    &self.session.label_contours(),
                    12.0 / self.session.viewport.zoom,
                ) == crate::application::editor::tools::label::DragKind::Cut;
            let mut path = kurbo::BezPath::new();
            if is_cut {
                path.move_to(points[0]);
                path.line_to(points[points.len() - 1]);
            } else {
                path.move_to(points[0]);
                for point in &points[1..] {
                    path.line_to(*point);
                }
                path.close_path();
                painter.fill(&path, color.with_alpha(0.12)).draw();
            }
            painter.stroke(&path, &Stroke::new(1.5), color).draw();
        }
        // What a click does, in the corner of the canvas.
        if let Some(caption) = self.label_caption() {
            text_label::draw(
                painter,
                Point::new(PANEL_PAD + 2.0, self.size.height - PANEL_PAD - 10.0),
                &caption,
                12.0,
                pal.text_muted,
                Anchor::Start,
            );
        }
    }

    /// What a click would do where the label tool's pointer is.
    fn label_caption(&self) -> Option<String> {
        use crate::application::editor::tools::label::LabelHit;
        if self.session.label.sample.is_none() {
            return Some(match self.label_hover {
                Some(LabelHit::Piece(_)) => {
                    "Click a sample to label it. Drag a loop around writing for a new one".into()
                }
                _ => "Drag a loop around writing to make a sample".into(),
            });
        }
        let letter = self.session.active_letter().map(|(_, c)| c);
        let Some(letter) = letter else {
            return Some("Type the sample's text in the panel".into());
        };
        Some(match &self.label_hover {
            Some(LabelHit::Piece(_)) => {
                format!("Click: paint {letter}. Option-click: unpaint. Drag across: cut")
            }
            Some(LabelHit::Corner { .. }) => {
                "Drag: move the corner with any that meet it. Option-drag: only this one".into()
            }
            Some(LabelHit::Edge { .. }) => "Click: add a corner".into(),
            Some(LabelHit::CutEnd { .. }) => "Drag: move the end of the cut".into(),
            Some(LabelHit::Cut { .. }) => "Drag: move the cut. Delete: remove it".into(),
            Some(LabelHit::Lasso { .. }) => "Click: pick this shape. Drag: move it".into(),
            _ => format!("Painting {letter}. Drag a loop: lasso. Enter: next letter"),
        })
    }

    /// The layer's picture as shown: its transform and its size in pixels. None while it is
    /// hidden, missing or (unless `locked_too`) locked.
    fn image_frame(&self, locked_too: bool) -> Option<(Affine, f64, f64)> {
        if !locked_too && self.session.image_locked() {
            return None;
        }
        let shown = self.underlay.image.as_ref()?;
        let transform = self.session.layer_image()?.transform();
        Some((
            transform,
            f64::from(shown.data.width),
            f64::from(shown.data.height),
        ))
    }

    /// The picture's corners in design space, each with the corner opposite it.
    fn image_corners(transform: Affine, width: f64, height: f64) -> [(Point, Point); 4] {
        let corner = |x: f64, y: f64| transform * Point::new(x, y);
        let (a, b, c, d) = (
            corner(0.0, 0.0),
            corner(width, 0.0),
            corner(width, height),
            corner(0.0, height),
        );
        [(a, c), (b, d), (c, a), (d, b)]
    }

    /// What a Select press at `design` does to the picture: resize it from a corner handle
    /// when it is selected, or move it when the press lands on it.
    fn image_press(&self, design: Point) -> Option<Drag> {
        let (transform, width, height) = self.image_frame(false)?;
        let reach = HIT_RADIUS_PX / self.session.viewport.zoom;
        if self.session.image_selected {
            for (corner, anchor) in Self::image_corners(transform, width, height) {
                if corner.distance(design) <= reach {
                    return Some(Drag::Image {
                        start: design,
                        origin: transform,
                        anchor: Some(anchor),
                        corner,
                    });
                }
            }
        }
        let bounds = (transform * Rect::new(0.0, 0.0, width, height).to_path(0.1)).bounding_box();
        bounds.contains(design).then_some(Drag::Image {
            start: design,
            origin: transform,
            anchor: None,
            corner: design,
        })
    }

    /// The transform an image drag gives the picture with the pointer at `design`.
    fn image_drag_transform(
        start: Point,
        origin: Affine,
        anchor: Option<Point>,
        corner: Point,
        design: Point,
    ) -> Affine {
        let Some(anchor) = anchor else {
            return Affine::translate(design - start) * origin;
        };
        // Proportional: project the pointer onto the diagonal through the anchor.
        let diagonal = corner - anchor;
        let length = diagonal.hypot2();
        if length <= f64::EPSILON {
            return origin;
        }
        let scale = ((design - anchor).dot(diagonal) / length).max(0.02);
        Affine::translate(anchor.to_vec2())
            * Affine::scale(scale)
            * Affine::translate(-anchor.to_vec2())
            * origin
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
            self.emit_text_proof_selection(ctx);
        }
    }

    fn emit_text_proof_selection(&self, ctx: &mut EventCtx<'_>) {
        if let Some(text) = &self.text {
            ctx.submit_action::<EditorEvent>(EditorEvent::TextProofSelection(
                text.buffer.proof_selection(),
            ));
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
                // An open composition frames the line even while Select edits
                // one sort within it.
                if self.text.is_some() {
                    self.fit_text();
                } else {
                    self.fit();
                }
                // Deterministic close-up evidence for zoom-dependent grid
                // rendering. This is deliberately gated to headless capture.
                if std::env::var("RUNEBENDER_SCREENSHOT").is_ok()
                    && let Ok(zoom) = std::env::var("RUNEBENDER_EDITOR_ZOOM")
                    && let Ok(zoom) = zoom.parse::<f64>()
                    && zoom.is_finite()
                    && zoom > 0.0
                {
                    let center = self.size.to_rect().center();
                    self.session.viewport.zoom_about(
                        center,
                        zoom / self.session.viewport.zoom,
                        zoom,
                        zoom,
                    );
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
        let view_affine = self.session.viewport.affine();
        painter.fill_rect(self.size.to_rect(), pal.canvas);

        // Holding Space is both the temporary Hand tool and the standard font
        // editor preview: show the filled design without nodes, handles, grid,
        // metrics, analyses, underlays, or other editing chrome. GPUI and Web
        // use this same press/release lifecycle, so panning never changes the
        // persistent tool and releasing Space restores the normal drawing.
        if self.preview_mode {
            if let Some(text) = &self.text {
                for sort in text.placed() {
                    painter
                        .fill(&(view_affine * sort.path), pal.editor_ink())
                        .draw();
                }
            } else if let Some(interp) = &self.interp {
                painter
                    .fill(&**interp, pal.editor_ink())
                    .transform(view_affine)
                    .draw();
            } else {
                painter
                    .fill(self.session.outline(), pal.editor_ink())
                    .transform(view_affine)
                    .draw();
                if !self.session.components.elements().is_empty() {
                    painter
                        .fill(&self.session.components, pal.editor_ink())
                        .transform(view_affine)
                        .draw();
                }
            }
            return;
        }

        // A text composition outlives the Text tool. With Text active every
        // sort is a fill and the caret is visible; with an outline tool the
        // active sort is omitted here and the editable glyph chrome below is
        // drawn at that sort's origin. This is the GPUI/Web state model.
        if let Some(text) = &self.text {
            let m = &self.session.metrics;
            let ink = pal.editor_ink();
            let sort_top = m.upm.max(m.ascender);
            let sort_bottom = m.descender;
            let sort_height_px = ((sort_top - sort_bottom) * self.session.viewport.zoom).abs();
            let mark = (sort_height_px * 0.05).clamp(1.5, 24.0);
            let metric_ys = text_sort_metric_ys(m);
            let mut placed = text.placed();
            let rule = Stroke::new(DesignStroke::Hairline.px());
            for sort in &mut placed {
                let box_ = Rect::from_points(
                    view_affine * Point::new(sort.origin.x, sort_bottom + sort.origin.y),
                    view_affine
                        * Point::new(sort.origin.x + sort.advance, sort_top + sort.origin.y),
                );
                if self.tool == Tool::Text && sort.selected {
                    painter
                        .fill(box_, pal.role("selection").with_alpha(0.32))
                        .draw();
                }
                if mark >= 3.0 && !sort.active {
                    let quiet = pal.role("metricQuiet");
                    for x in [box_.x0, box_.x1] {
                        painter
                            .stroke(Line::new((x, box_.y0), (x, box_.y1)), &rule, quiet)
                            .draw();
                    }
                    for &y in &metric_ys {
                        let sy = (view_affine * Point::new(sort.origin.x, sort.origin.y + y)).y;
                        painter
                            .stroke(Line::new((box_.x0, sy), (box_.x1, sy)), &rule, quiet)
                            .draw();
                    }
                }
                if !sort.active || self.tool == Tool::Text {
                    painter
                        .fill(&(view_affine * std::mem::take(&mut sort.path)), ink)
                        .draw();
                }
            }
            // Draw crosses over every sort's quiet rules, including shared
            // boundaries. The active sort's marks go last when Text owns it;
            // outline tools draw them with the active glyph frame below.
            if mark >= 3.0 {
                for sort in placed.iter().filter(|sort| !sort.active).chain(
                    placed
                        .iter()
                        .filter(|sort| sort.active && self.tool == Tool::Text),
                ) {
                    let x0 = (view_affine * sort.origin).x;
                    let x1 =
                        (view_affine * Point::new(sort.origin.x + sort.advance, sort.origin.y)).x;
                    let ys = metric_ys
                        .iter()
                        .map(|&y| (view_affine * Point::new(sort.origin.x, sort.origin.y + y)).y);
                    let top = (view_affine * Point::new(0.0, sort.origin.y + sort_top)).y;
                    let bottom = (view_affine * Point::new(0.0, sort.origin.y + sort_bottom)).y;
                    paint_metric_crosses(painter, [x0, x1], [top, bottom], ys, mark, pal.outline);
                }
            }
            if self.tool == Tool::Text && self.cursor_visible {
                // The caret follows GPUI and Web: a full sort-height rule with
                // inward triangular caps scaled from its on-screen height. It
                // uses the same single outline stroke as the sort corners.
                let caret = text.caret();
                let top = view_affine * Point::new(caret.x, caret.y + sort_top);
                let bottom = view_affine * Point::new(caret.x, caret.y + sort_bottom);
                let cursor = pal.outline;
                painter
                    .stroke(
                        Line::new(top, bottom),
                        &Stroke::new(DesignStroke::Hairline.px()),
                        cursor,
                    )
                    .draw();
                let triangle_width = (sort_height_px * TEXT_CURSOR_CAP_FRACTION)
                    .clamp(TEXT_CURSOR_CAP_MIN, TEXT_CURSOR_CAP_MAX);
                let triangle_height = triangle_width * (2.0 / 3.0);
                let mut top_cap = kurbo::BezPath::new();
                top_cap.move_to((top.x - triangle_width / 2.0, top.y));
                top_cap.line_to((top.x + triangle_width / 2.0, top.y));
                top_cap.line_to((top.x, top.y + triangle_height));
                top_cap.close_path();
                painter.fill(&top_cap, cursor).draw();
                let mut bottom_cap = kurbo::BezPath::new();
                bottom_cap.move_to((bottom.x - triangle_width / 2.0, bottom.y));
                bottom_cap.line_to((bottom.x + triangle_width / 2.0, bottom.y));
                bottom_cap.line_to((bottom.x, bottom.y - triangle_height));
                bottom_cap.close_path();
                painter.fill(&bottom_cap, cursor).draw();

                self.paint_metrics(painter);
                return;
            }
            if self.tool == Tool::Text {
                self.paint_metrics(painter);
                return;
            }
            if text.active_origin().is_none() {
                return;
            }
        }

        let affine = self.glyph_affine();

        // Underlay, drawn first so everything else sits on top of it. The
        // reference glyph is a quiet fill (it is a shape to match), the
        // background layer is a quiet outline (it is a trace to follow).
        if let Some(image) = &self.underlay.image {
            let height = f64::from(image.data.height);
            let to_glyph = self.image_preview.map_or(image.to_glyph, |transform| {
                transform * Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, height])
            });
            painter.draw_image(&image.data, affine * to_glyph);
            if self.session.image_selected
                && let Some((transform, width, height)) = self.image_frame(true)
            {
                let locked = self.session.image_locked();
                let transform = self.image_preview.unwrap_or(transform);
                let frame = affine * transform * Rect::new(0.0, 0.0, width, height).to_path(0.1);
                // A locked picture shows that it is selected, but offers no handles.
                let selection = if locked {
                    pal.text_muted
                } else {
                    pal.role("selection")
                };
                painter.stroke(&frame, &Stroke::new(1.0), selection).draw();
                for (corner, _) in Self::image_corners(transform, width, height)
                    .into_iter()
                    .filter(|_| !locked)
                {
                    let at = affine * corner;
                    painter
                        .fill(Rect::from_center_size(at, (7.0, 7.0)), pal.canvas)
                        .draw();
                    painter
                        .stroke(
                            Rect::from_center_size(at, (7.0, 7.0)),
                            &Stroke::new(1.0),
                            selection,
                        )
                        .draw();
                }
            }
        }
        if let Some(reference) = &self.underlay.reference {
            painter
                .fill(&**reference, pal.text_muted.with_alpha(0.18))
                .transform(affine)
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
                .fill(&**mark, pal.role("component").with_alpha(0.10))
                .transform(affine)
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
        if let Some(candidate) = &self.underlay.brush_candidate {
            let path = affine * (**candidate).clone();
            painter
                .fill(&path, pal.role("warning").with_alpha(0.35))
                .draw();
            painter
                .stroke(&path, &Stroke::new(2.0), pal.role("warning"))
                .draw();
        }

        // Interpolation ghosts: the other masters' outlines, faint.
        for ghost in self.ghosts.iter() {
            painter
                .stroke(
                    &(affine * (**ghost).clone()),
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
            let (mid, close) = grid_alphas(zoom);
            if mid > 0.0 {
                let inv = affine.inverse();
                let a = inv * Point::new(0.0, 0.0);
                let b = inv * Point::new(self.size.width, self.size.height);
                let (min_x, max_x) = (a.x.min(b.x), a.x.max(b.x));
                let (min_y, max_y) = (a.y.min(b.y), a.y.max(b.y));
                let mut level = |spacing: f64, skip_every: i64, size: f64, alpha: f64| {
                    let mut marks = kurbo::BezPath::new();
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
                        let columns = usize::try_from(ix1 - ix0 + 1).unwrap_or_default();
                        let rows = usize::try_from(iy1 - iy0 + 1).unwrap_or_default();
                        // Six elements per dot: a move, four curves, and a close.
                        marks = kurbo::BezPath::with_capacity(columns * rows * 6);
                        for ix in ix0..=ix1 {
                            for iy in iy0..=iy1 {
                                if skip_every > 0 && ix % skip_every == 0 && iy % skip_every == 0 {
                                    continue;
                                }
                                let at =
                                    affine * Point::new(ix as f64 * spacing, iy as f64 * spacing);
                                push_grid_dot(&mut marks, at, size);
                            }
                        }
                    }
                    let color = pal
                        .role("designGridCoarse")
                        .with_alpha(crate::application::view::render::px32(alpha));
                    if self.view.grid_lines {
                        painter.stroke(&marks, &Stroke::new(0.5), color).draw();
                    } else {
                        painter.fill(&marks, color).draw();
                    }
                };
                let (coarse, fine) = grid_dot_sizes(zoom);
                let spacing = runebender::outline::point_ops::DOT_GRID_SPACING;
                level(spacing, 0, coarse, mid);
                if close > 0.0 {
                    level(spacing / 4.0, 4, fine, close);
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
        let levels = text_sort_metric_ys(m);
        // A neural item is not set in a box: no advance, ascender, or descender.
        // One baseline at y = 0 runs across the whole canvas as the anchor
        // between positive and negative heights.
        let neural = self.session.neural;
        if neural {
            let sy = (affine * Point::new(0.0, 0.0)).y;
            painter.fill_rect(horizontal_rule_rect(0.0, self.size.width, sy, rule), frame);
        }
        for &y in levels.iter().filter(|_| !neural) {
            let sy = (affine * Point::new(0.0, y)).y;
            painter.fill_rect(horizontal_rule_rect(x0, x1, sy, rule), frame);
        }
        let top = (affine * Point::new(0.0, box_top)).y;
        let bottom = (affine * Point::new(0.0, m.descender)).y;
        for x in [x0, x1].into_iter().filter(|_| !neural) {
            painter.fill_rect(vertical_rule_rect(x, top, bottom, rule), frame);
        }
        let mark =
            (((box_top - m.descender) * self.session.viewport.zoom).abs() * 0.05).clamp(1.5, 24.0);
        if !neural {
            paint_metric_crosses(
                painter,
                [x0, x1],
                [top, bottom],
                levels.iter().map(|&y| (affine * Point::new(0.0, y)).y),
                mark,
                pal.outline,
            );
        }

        // Editing affordances only render on a master. Off a master the view
        // shows the read-only interpolated instance instead (web/Glyphs
        // behavior): swap the outline, don't ghost it behind an editable one.
        if self.interp.is_none() {
            if !self.session.components.elements().is_empty() {
                painter
                    .fill(
                        &self.session.components,
                        pal.role("component").with_alpha(0.5),
                    )
                    .transform(affine)
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

            // Curvature comb: paint the coloured strip before the editable
            // outline, handles, and points. This is the GPUI layer order: the
            // comb remains vivid, while every editing affordance stays clear
            // and selectable above it.
            if self.view.comb {
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

            let outline = affine * self.session.outline();
            painter
                // GPUI keeps the edit fill at 70% opacity so the glyph reads
                // as a shape without hiding the design grid and metrics.
                .fill(&outline, pal.outline_fill())
                .draw();
            painter
                .stroke(&outline, &Stroke::new(1.0), pal.role("pathStroke"))
                .draw();

            // Labeling reads the ink as a whole; points and handles would only hide it.
            let point_chrome = self.tool != Tool::Label;
            let handle = Stroke::new(1.0);
            for line in self
                .session
                .handle_lines()
                .into_iter()
                .filter(|_| point_chrome)
            {
                painter
                    .stroke(affine * line, &handle, pal.handle_line)
                    .draw();
            }

            let marker_scale = point_marker_scale(self.session.viewport.zoom);
            let ring_width = (POINT_RING_WIDTH * marker_scale).max(DesignStroke::Hairline.px());
            let halo_width = ring_width + POINT_HALO_EXTRA;
            let start_markers = self.start_markers();
            for (id, sp, on_curve, smooth, _) in
                self.screen_points().into_iter().filter(|_| point_chrome)
            {
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
                    (hue, pal.canvas)
                };
                // A point is a dark window with a coloured ring, shared with
                // the web editor: a
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
                let shape = start_markers
                    .iter()
                    .find(|(start_id, _, _)| *start_id == id)
                    .and_then(|(_, from, to)| direction_marker_shape(*from, *to, r, smooth))
                    .unwrap_or_else(|| point_marker_shape(sp, r, square));
                let halo = pal.canvas.with_alpha(0.85);
                let ring = Stroke::new(ring_width);
                if pal.point_halo {
                    painter
                        .stroke(&shape, &Stroke::new(halo_width), halo)
                        .draw();
                }
                painter.fill(&shape, interior).draw();

                // A point is also a window onto the design grid. GPUI
                // redraws the dots or line chords which fall inside the
                // marker after its interior and before its ring. This makes
                // exact grid alignment readable without weakening the point.
                let (coarse_alpha, fine_alpha) = grid_alphas(self.session.viewport.zoom);
                let (coarse_dot, fine_dot) = grid_dot_sizes(self.session.viewport.zoom);
                let grid_color = if selected || pal.points_filled {
                    fill
                } else {
                    hue
                };
                for (spacing, alpha, dot_size, line_width) in [
                    (8.0, coarse_alpha, coarse_dot, POINT_GRID_COARSE_LINE_WIDTH),
                    (2.0, fine_alpha, fine_dot, POINT_GRID_FINE_LINE_WIDTH),
                ] {
                    if alpha <= 0.0 {
                        continue;
                    }
                    let marks = point_grid_marks(
                        affine,
                        sp,
                        r,
                        square,
                        spacing,
                        dot_size,
                        self.view.grid_lines,
                    );
                    if marks.is_empty() {
                        continue;
                    }
                    let color =
                        grid_color.with_alpha(crate::application::view::render::px32(alpha));
                    if self.view.grid_lines {
                        painter
                            .stroke(&marks, &Stroke::new(line_width), color)
                            .draw();
                    } else {
                        painter.fill(&marks, color).draw();
                    }
                }
                painter.stroke(&shape, &ring, fill).draw();
            }

            // The ordinary pen's initial incoming and current outgoing
            // handles remain session-local until close or the next segment
            // commits them. Paint their nodes separately so smooth points
            // retain the same visible feedback as committed off-curves.
            for (_, handle) in self.session.pen_handle_previews() {
                let hue = pal.role("pointOffcurve");
                let (fill, interior) = if pal.points_filled {
                    (pal.point_outline.unwrap_or(pal.text), hue)
                } else {
                    (hue, pal.canvas)
                };
                let shape =
                    point_marker_shape(affine * handle, POINT_CURVE_RADIUS * marker_scale, false);
                if pal.point_halo {
                    painter
                        .stroke(
                            &shape,
                            &Stroke::new(halo_width),
                            pal.canvas.with_alpha(0.85),
                        )
                        .draw();
                }
                painter.fill(&shape, interior).draw();
                painter
                    .stroke(&shape, &Stroke::new(ring_width), fill)
                    .draw();
            }

            // The point a pen click would add to the segment under the
            // cursor: a faint corner on a line, a faint smooth point on a curve.
            if let Some((position, curve)) = self.insert_preview.filter(|_| self.tool == Tool::Pen)
            {
                let hue = pal.role(if curve { "pointSmooth" } else { "pointCorner" });
                let (fill, interior) = if pal.points_filled {
                    (pal.point_outline.unwrap_or(pal.text), hue)
                } else {
                    (hue, pal.canvas)
                };
                let radius = if curve {
                    POINT_CURVE_RADIUS
                } else {
                    POINT_CORNER_RADIUS
                };
                let shape = point_marker_shape(affine * position, radius * marker_scale, !curve);
                painter.fill(&shape, interior.with_alpha(0.5)).draw();
                painter
                    .stroke(&shape, &Stroke::new(ring_width), fill.with_alpha(0.6))
                    .draw();
            }

            // Anchors use the same point construction, with solid pink inside
            // the shared dark keyline. Selection retains the node palette.
            let anchor_color = pal.mark("pink").unwrap_or_else(|| pal.role("danger"));
            for (anchor, position) in self.session.anchor_points() {
                let p = affine * position;
                let selected = self.session.selected_anchor == Some(anchor);
                let (ring, inner) = if selected {
                    (
                        pal.point_outline.unwrap_or(pal.text),
                        pal.role("pointSelected"),
                    )
                } else {
                    (pal.point_outline.unwrap_or(pal.text), anchor_color)
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

        // Continuity rings match GPUI and preserve the underlying point shape.
        if self.view.continuity && self.interp.is_none() {
            use runebender::analysis::curve::GLevel;
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
                use runebender::analysis::measure::MeasureKind;
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
                for bounds in self.session.segment_bounds() {
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

        if matches!(self.tool, Tool::Select | Tool::Metaball)
            && let Ok(source) = self.session.metaball_data()
        {
            let marker_scale = point_marker_scale(self.session.viewport.zoom);
            let ring_width = (POINT_RING_WIDTH * marker_scale).max(DesignStroke::Hairline.px());
            let halo_width = ring_width + POINT_HALO_EXTRA;
            for group in source.groups {
                for ball in group.balls {
                    let center = affine * Point::new(ball.x, ball.y);
                    let selected = self
                        .session
                        .metaballs
                        .selected
                        .contains(&(group.id, ball.id));
                    let (marker_ring, interior) = if selected {
                        (
                            pal.point_outline.unwrap_or(pal.text),
                            pal.role("pointSelected"),
                        )
                    } else {
                        (pal.editor_control_ink(), pal.canvas)
                    };
                    let support = Circle::new(
                        center,
                        ball.radius * ball.reach * self.session.viewport.zoom,
                    );
                    if selected {
                        let keyline = pal.mark("orange").unwrap_or(marker_ring);
                        painter
                            .stroke(
                                support,
                                &Stroke::new(DesignStroke::Hairline.px() * 3.0),
                                keyline,
                            )
                            .draw();
                        painter
                            .stroke(
                                support,
                                &Stroke::new(DesignStroke::Hairline.px()),
                                pal.role("pointSelected"),
                            )
                            .draw();
                    } else {
                        painter
                            .stroke(
                                support,
                                &Stroke::new(DesignStroke::Hairline.px()),
                                marker_ring.with_alpha(0.35),
                            )
                            .draw();
                    }
                    let radius = (POINT_CURVE_RADIUS
                        + if selected { POINT_SELECTED_GROW } else { 0.0 })
                        * marker_scale;
                    let marker = Circle::new(center, radius);
                    if pal.point_halo {
                        painter
                            .stroke(
                                marker,
                                &Stroke::new(halo_width),
                                pal.canvas.with_alpha(0.85),
                            )
                            .draw();
                    }
                    painter.fill(marker, interior).draw();
                    painter
                        .stroke(marker, &Stroke::new(ring_width), marker_ring)
                        .draw();
                }
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

        if self.tool == Tool::Label {
            self.paint_labels(painter, affine);
        }

        if let Drag::Lasso { points, .. } | Drag::Label { points, .. } = &self.drag
            && let Some(first) = points.first()
        {
            let mut path = kurbo::BezPath::new();
            path.move_to(*first);
            for point in &points[1..] {
                path.line_to(*point);
            }
            if points.len() >= 3 {
                path.close_path();
                painter
                    .fill(&path, pal.role("selection").with_alpha(0.15))
                    .draw();
            }
            painter
                .stroke(
                    &path,
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

        if self.tool == Tool::Sketch {
            let sketch = self
                .sketch
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if sketch.matches(&self.session.glyph_name, self.sketch_source) && sketch.has_ink() {
                let alpha: f32 = if self.underlay.brush_candidate.is_some() {
                    0.18
                } else {
                    0.55
                };
                let key = (sketch.revision(), alpha.to_bits());
                let image = match &self.sketch_image {
                    Some((revision, alpha_bits, image)) if (*revision, *alpha_bits) == key => {
                        image.clone()
                    }
                    _ => {
                        let image = ImageData {
                            data: Blob::new(Arc::new(
                                sketch.display_rgba(pal.tool_feedback().with_alpha(alpha)),
                            )),
                            format: ImageFormat::Rgba8,
                            alpha_type: ImageAlphaType::Alpha,
                            width: 512,
                            height: 512,
                        };
                        self.sketch_image = Some((key.0, key.1, image.clone()));
                        image
                    }
                };
                let pixel_to_glyph = Affine::new([2.0, 0.0, 0.0, -2.0, sketch.left(), 800.0]);
                painter.draw_image(&image, self.glyph_affine() * pixel_to_glyph);
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
                // GPUI and Web let any editing tool follow a composed sort on
                // double-click. Activating it before replacing the live glyph
                // session is what keeps the surrounding word on the canvas.
                if state.count >= 2
                    && *button == Some(PointerButton::Primary)
                    && let Some(text) = self.text.as_mut()
                {
                    let design = self.session.viewport.screen_to_design(at);
                    if let Some(glyph) = text.activate_at(design) {
                        ctx.submit_action::<EditorEvent>(EditorEvent::EditGlyph {
                            name: glyph,
                            tool: self.tool,
                        });
                        self.emit_text_proof_selection(ctx);
                        self.drag = Drag::None;
                        ctx.request_render();
                        ctx.set_handled();
                        return;
                    }
                }
                // Text tool: a click is a caret placement, and a click on
                // a sort makes that glyph the one being edited.
                if self.tool == Tool::Text
                    && let Some(text) = self.text.as_mut()
                    && *button == Some(PointerButton::Primary)
                {
                    self.cursor_blink_elapsed_ns = 0;
                    self.cursor_visible = true;
                    ctx.request_anim_frame();
                    let design = self.session.viewport.screen_to_design(at);
                    if let Some(index) = text.click(design)
                        && let Some(glyph) = text.activate(index)
                    {
                        ctx.submit_action::<EditorEvent>(EditorEvent::EditGlyph {
                            name: glyph,
                            tool: Tool::Text,
                        });
                    }
                    self.emit_text_proof_selection(ctx);
                    ctx.request_render();
                    ctx.set_handled();
                    return;
                }
                if *button == Some(PointerButton::Secondary) {
                    // A layer, not a rectangle painted into this canvas:
                    // it is rooted in window space, so it can hang past
                    // the editor's edge like a menu should.
                    if self.menu.is_none() {
                        let design = self.screen_to_glyph_design(at);
                        let mut rows = MENU_ITEMS.to_vec();
                        let on_image = self.image_frame(true).is_some_and(|(t, w, h)| {
                            (t * Rect::new(0.0, 0.0, w, h).to_path(0.1))
                                .bounding_box()
                                .contains(design)
                        });
                        if on_image {
                            let locked = self.session.image_locked();
                            rows.splice(
                                0..0,
                                [
                                    MenuRow {
                                        label: std::borrow::Cow::Borrowed(if locked {
                                            "Unlock Image"
                                        } else {
                                            "Lock Image"
                                        }),
                                        action: MenuAction::Op(|s| s.toggle_image_lock()),
                                    },
                                    MenuRow {
                                        label: std::borrow::Cow::Borrowed("Remove Image"),
                                        action: MenuAction::Op(|s| s.remove_image()),
                                    },
                                ],
                            );
                        }
                        if let Some(component) = self.session.component_at(design) {
                            self.session.select_component_id(component);
                            self.emit(ctx, false);
                            let aligned = self.session.selected_component_aligned() == Some(true);
                            rows.insert(
                                0,
                                MenuRow {
                                    label: std::borrow::Cow::Borrowed(if aligned {
                                        "Disable Automatic Alignment"
                                    } else {
                                        "Enable Automatic Alignment"
                                    }),
                                    action: MenuAction::ToggleComponentAlignment,
                                },
                            );
                        }
                        let menu = ContextMenu::new(
                            ctx.widget_id(),
                            MenuTarget::Editor,
                            rows,
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
                    Some(PointerButton::Primary) if self.tool == Tool::Sketch => {
                        let design = self.screen_to_glyph_design(at);
                        let mut sketch = self
                            .sketch
                            .lock()
                            .unwrap_or_else(|error| error.into_inner());
                        if !sketch.matches(&self.session.glyph_name, self.sketch_source) {
                            *sketch = sketch.for_glyph(
                                self.session.glyph_name.clone(),
                                self.sketch_source,
                                self.session.advance(),
                            );
                        }
                        let changed = sketch.stroke((design.x, design.y), (design.x, design.y));
                        self.drag = Drag::Sketch {
                            last: design,
                            changed,
                        };
                        ctx.request_render();
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Metaball => {
                        ctx.request_focus();
                        let at = self.screen_to_glyph_design(at);
                        let changed = self.session.metaball_click(
                            at,
                            HIT_RADIUS_PX / self.session.viewport.zoom,
                            state.modifiers.shift(),
                        );
                        self.drag = Drag::Metaballs { last: at, changed };
                        self.emit(ctx, false);
                        ctx.request_render();
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Label => {
                        use crate::application::editor::tools::label::{
                            LabelHit, LabelMoving, LabelSelection,
                        };
                        ctx.request_focus();
                        let design = self.screen_to_glyph_design(at);
                        let reach = HIT_RADIUS_PX / self.session.viewport.zoom;
                        match self.session.label_hit(design, reach) {
                            LabelHit::Corner { region, corner } => {
                                self.session.activate_label_region(region);
                                let shift = state.modifiers.shift();
                                let mut picked = self.session.selected_corners();
                                if shift {
                                    // Shift adds a corner to the set, or takes it out.
                                    match picked.iter().position(|c| *c == (region, corner)) {
                                        Some(at) => {
                                            picked.remove(at);
                                        }
                                        None => picked.push((region, corner)),
                                    }
                                    self.session.label.selected =
                                        Some(LabelSelection::Corners(picked));
                                    self.emit(ctx, false);
                                } else {
                                    if state.modifiers.alt() || !picked.contains(&(region, corner))
                                    {
                                        // Corners that meet here move as one; Option takes
                                        // only the one on top, to pull it away.
                                        let at = self.session.corner_point(region, corner);
                                        picked = if state.modifiers.alt() {
                                            Vec::new()
                                        } else {
                                            at.map(|at| self.session.corners_at(at))
                                                .unwrap_or_default()
                                        };
                                        if picked.is_empty() {
                                            picked.push((region, corner));
                                        }
                                        self.session.label.selected =
                                            Some(LabelSelection::Corners(picked.clone()));
                                    }
                                    if picked.len() == 1 {
                                        self.session.label.moving = Some(LabelMoving::Corner {
                                            region,
                                            corner,
                                            to: design,
                                        });
                                        self.drag = Drag::LabelCorner {
                                            region,
                                            detach: state.modifiers.alt(),
                                        };
                                    } else {
                                        self.session.label.moving = Some(LabelMoving::Corners {
                                            by: kurbo::Vec2::ZERO,
                                        });
                                        self.drag = Drag::LabelCorners { start: design };
                                    }
                                    ctx.capture_pointer();
                                    self.emit(ctx, false);
                                }
                            }
                            LabelHit::Edge {
                                region,
                                after,
                                at: on,
                            } => {
                                // A corner appears on the edge and follows the pointer.
                                self.session.activate_label_region(region);
                                if let Some(corner) =
                                    self.session.add_label_corner(region, after, on)
                                {
                                    self.session.label.selected =
                                        Some(LabelSelection::Corners(vec![(region, corner)]));
                                    self.session.label.moving = Some(LabelMoving::Corner {
                                        region,
                                        corner,
                                        to: on,
                                    });
                                    self.drag = Drag::LabelCorner {
                                        region,
                                        detach: false,
                                    };
                                    ctx.capture_pointer();
                                    self.emit(ctx, true);
                                }
                            }
                            LabelHit::CutEnd { cut, end } => {
                                self.session.label.selected = Some(LabelSelection::Cut(cut));
                                self.session.label.moving = Some(LabelMoving::CutEnd {
                                    cut,
                                    end,
                                    to: design,
                                });
                                self.drag = Drag::LabelCut {
                                    cut,
                                    end: Some(end),
                                    start: design,
                                };
                                ctx.capture_pointer();
                                self.emit(ctx, false);
                            }
                            LabelHit::Cut { cut } => {
                                self.session.label.selected = Some(LabelSelection::Cut(cut));
                                self.session.label.moving = Some(LabelMoving::Cut {
                                    cut,
                                    by: kurbo::Vec2::ZERO,
                                });
                                self.drag = Drag::LabelCut {
                                    cut,
                                    end: None,
                                    start: design,
                                };
                                ctx.capture_pointer();
                                self.emit(ctx, false);
                            }
                            LabelHit::Lasso { region } => {
                                // A press inside a lasso picks it up: its letter becomes
                                // active and its corners show. A drag moves the whole shape.
                                self.session.activate_label_region(region);
                                self.session.label.selected = Some(LabelSelection::Region(region));
                                self.session.label.moving = Some(LabelMoving::Region {
                                    region,
                                    by: kurbo::Vec2::ZERO,
                                });
                                self.drag = Drag::LabelRegion {
                                    region,
                                    start: design,
                                };
                                ctx.capture_pointer();
                                self.emit(ctx, false);
                            }
                            LabelHit::Piece(_) | LabelHit::Nothing
                                if state.modifiers.shift()
                                    && self.session.label.sample.is_some() =>
                            {
                                // Shift-drag: a box that selects corners.
                                self.drag = Drag::LabelMarquee {
                                    start: design,
                                    current: design,
                                };
                                ctx.capture_pointer();
                            }
                            LabelHit::Piece(_) | LabelHit::Nothing => {
                                if self.session.label.sample.is_none()
                                    && let Some(sample) = self.session.sample_at(design)
                                {
                                    // A click on a sample selects it.
                                    self.session.select_sample(Some(sample));
                                    self.emit(ctx, false);
                                } else {
                                    self.drag = Drag::Label {
                                        points: vec![at],
                                        erase: state.modifiers.alt(),
                                    };
                                }
                            }
                        }
                        ctx.request_render();
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Hand => {
                        self.drag = Drag::Pan { last: at };
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Lasso => {
                        ctx.request_focus();
                        self.drag = Drag::Lasso {
                            points: vec![at],
                            additive: state.modifiers.shift(),
                        };
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::HyperPen => {
                        let affine = self.glyph_affine();
                        let near_first = self
                            .session
                            .first_contour_point()
                            .map(|p| (affine * p).distance(at) <= HIT_RADIUS_PX)
                            .unwrap_or(false);
                        let corner = state.modifiers.alt();
                        if near_first && self.session.hyper_is_active() {
                            self.session.hyper_close();
                        } else {
                            let d = self.screen_to_glyph_design(at);
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
                        let d = self.screen_to_glyph_design(at);
                        self.drag = Drag::Shape {
                            start: d,
                            current: d,
                        };
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) if self.tool == Tool::Pen => {
                        let affine = self.glyph_affine();
                        let near_first = self
                            .session
                            .pen_first_point()
                            .map(|p| (affine * p).distance(at) <= HIT_RADIUS_PX)
                            .unwrap_or(false);
                        let on_segment = !self.session.pen_is_active()
                            && self.hit_point(at).is_none()
                            && self.session.insert_point_at(
                                self.screen_to_glyph_design(at),
                                HIT_RADIUS_PX / self.session.viewport.zoom,
                            );
                        if near_first && self.session.pen_is_active() {
                            self.session.pen_close();
                            self.drag = Drag::None;
                            self.emit(ctx, true);
                        } else if on_segment {
                            self.insert_preview = None;
                            // A click on an outline adds a point to that
                            // segment rather than starting a new contour.
                            self.drag = Drag::None;
                            self.emit(ctx, true);
                            ctx.request_render();
                        } else {
                            let origin = self.screen_to_glyph_design(at);
                            let (point_count, active_contour) = self.session.pen_checkpoint();
                            self.drag = Drag::Pen {
                                origin,
                                dragging: false,
                                point_count,
                                active_contour,
                            };
                        }
                        ctx.set_handled();
                        return;
                    }
                    Some(PointerButton::Primary) => {
                        let shift = state.modifiers.shift();
                        if self.tool == Tool::Select {
                            let design = self.screen_to_glyph_design(at);
                            if self.session.select_metaball_at(
                                design,
                                HIT_RADIUS_PX / self.session.viewport.zoom,
                                shift,
                            ) {
                                ctx.request_focus();
                                self.drag = Drag::Metaballs {
                                    last: design,
                                    changed: false,
                                };
                                self.emit(ctx, false);
                                ctx.request_render();
                                ctx.set_handled();
                                return;
                            }
                        }
                        // Sidebearing lines (only when not near a point).
                        let affine = self.glyph_affine();
                        let adv_x = (affine * Point::new(self.session.advance(), 0.0)).x;
                        let lsb_x = (affine * Point::new(0.0, 0.0)).x;
                        if self.hit_point(at).is_none() && !self.session.neural {
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
                        if let Some(anchor) = self.session.anchor_at(
                            self.screen_to_glyph_design(at),
                            HIT_RADIUS_PX / self.session.viewport.zoom,
                        ) {
                            self.session.metaballs.selected.clear();
                            self.session.selected_anchor = Some(anchor);
                            self.session.selected_component = None;
                            self.session.selection.clear();
                            self.drag = Drag::Anchor { id: anchor };
                            self.emit(ctx, false);
                            ctx.set_handled();
                            return;
                        }
                        self.session.selected_anchor = None;
                        // Option-click on a segment gives it two curve handles, as in Glyphs.
                        if self.tool == Tool::Select
                            && state.modifiers.alt()
                            && self.hit_point(at).is_none()
                            && self.session.add_segment_handles_at(
                                self.screen_to_glyph_design(at),
                                HIT_RADIUS_PX / self.session.viewport.zoom,
                            )
                        {
                            self.emit(ctx, true);
                            ctx.set_handled();
                            return;
                        }
                        match self.hit_point(at) {
                            // A double click on an on-curve point switches it between corner
                            // and smooth, as in Glyphs.
                            Some(id) if state.count >= 2 && self.session.toggle_smooth(id) => {
                                self.session.selection = std::collections::HashSet::from([id]);
                                self.emit(ctx, true);
                            }
                            Some(id) => {
                                self.session.metaballs.selected.clear();
                                self.session.image_selected = false;
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
                                let design = self.screen_to_glyph_design(at);
                                if let Some(component) = self.session.component_at(design) {
                                    self.session.metaballs.selected.clear();
                                    self.session.select_component_id(component);
                                    self.drag = Drag::Component { last: design };
                                    self.emit(ctx, false);
                                    ctx.set_handled();
                                    return;
                                }
                                if !shift && let Some(drag) = self.image_press(design) {
                                    self.session.metaballs.selected.clear();
                                    self.session.selection.clear();
                                    self.session.selected_component = None;
                                    self.session.image_selected = true;
                                    self.drag = drag;
                                    self.emit(ctx, false);
                                    ctx.request_render();
                                    ctx.set_handled();
                                    return;
                                }
                                if !shift {
                                    self.session.metaballs.selected.clear();
                                    self.session.selection.clear();
                                    self.session.selected_component = None;
                                    self.session.image_selected = false;
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
                let active_origin = self.active_sort_origin().to_vec2();
                let glyph_design = self.session.viewport.screen_to_design(at) - active_origin;
                let glyph_affine =
                    self.session.viewport.affine() * Affine::translate(active_origin);
                if matches!(self.tool, Tool::Pen | Tool::HyperPen) {
                    self.hover = Some(glyph_design);
                    if self.session.contour_drawing_is_active() {
                        ctx.request_render();
                    }
                }
                let insert_preview = (self.tool == Tool::Pen
                    && matches!(self.drag, Drag::None)
                    && !self.session.pen_is_active()
                    && self.hit_point(at).is_none())
                .then(|| {
                    self.session
                        .segment_insert_at(glyph_design, HIT_RADIUS_PX / self.session.viewport.zoom)
                })
                .flatten()
                .map(|insert| (insert.point, insert.curve));
                if insert_preview != self.insert_preview {
                    self.insert_preview = insert_preview;
                    ctx.request_render();
                }
                if self.tool == Tool::Label && matches!(self.drag, Drag::None) {
                    use crate::application::editor::tools::label::LabelHit;
                    let hit = self
                        .session
                        .label_hit(glyph_design, HIT_RADIUS_PX / self.session.viewport.zoom);
                    let hover = (hit != LabelHit::Nothing).then_some(hit);
                    if hover != self.label_hover {
                        self.label_hover = hover;
                        ctx.request_render();
                    }
                }
                match &mut self.drag {
                    Drag::Sketch { last, changed } => {
                        let mut sketch = self
                            .sketch
                            .lock()
                            .unwrap_or_else(|error| error.into_inner());
                        *changed |=
                            sketch.stroke((last.x, last.y), (glyph_design.x, glyph_design.y));
                        *last = glyph_design;
                        ctx.request_render();
                    }
                    Drag::Metaballs { last, changed } => {
                        let moved = self.session.move_metaballs(glyph_design - *last, true);
                        *changed |= moved;
                        *last = glyph_design;
                        if moved {
                            // Publish the live draft to the proof strip and inspector.
                            // Only release emits Edited and commits this gesture.
                            self.emit(ctx, false);
                            ctx.request_render();
                        }
                    }
                    Drag::Image {
                        start,
                        origin,
                        anchor,
                        corner,
                    } => {
                        self.image_preview = Some(Self::image_drag_transform(
                            *start,
                            *origin,
                            *anchor,
                            *corner,
                            glyph_design,
                        ));
                        ctx.request_render();
                    }
                    Drag::AdvanceLine => {
                        let d = glyph_design;
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
                    Drag::Anchor { id } => {
                        let id = *id;
                        let d = glyph_design;
                        self.session.move_anchor(id, d.x.round(), d.y.round());
                        ctx.request_render();
                    }
                    Drag::Component { last } => {
                        let design = glyph_design;
                        let delta = design - *last;
                        *last = design;
                        if self.session.drag_component_by(delta.x, delta.y) {
                            ctx.request_render();
                        }
                    }
                    Drag::Pen {
                        origin, dragging, ..
                    } => {
                        let origin = *origin;
                        let to = glyph_design;
                        let moved_px = (glyph_affine * origin).distance(at);
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
                        // Option moves on-curve points without their handles.
                        let independent = current.modifiers.alt();
                        if self.session.drag_points_to(total, independent) {
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
                    Drag::LabelCorner { region, detach } => {
                        use crate::application::editor::tools::label::{LabelMoving, on_grid};
                        let (region, detach) = (*region, *detach);
                        if let Some(LabelMoving::Corner { corner, .. }) = self.session.label.moving
                        {
                            let to = if detach {
                                on_grid(glyph_design)
                            } else {
                                self.session.snapped_label_corner(
                                    glyph_design,
                                    HIT_RADIUS_PX / self.session.viewport.zoom,
                                    Some(region),
                                )
                            };
                            self.session.label.moving =
                                Some(LabelMoving::Corner { region, corner, to });
                            ctx.request_render();
                        }
                    }
                    Drag::LabelCorners { start } => {
                        use crate::application::editor::tools::label::LabelMoving;
                        let start = *start;
                        self.session.label.moving = Some(LabelMoving::Corners {
                            by: glyph_design - start,
                        });
                        ctx.request_render();
                    }
                    Drag::LabelMarquee { current, .. } => {
                        *current = glyph_design;
                        ctx.request_render();
                    }
                    Drag::LabelRegion { region, start } => {
                        use crate::application::editor::tools::label::LabelMoving;
                        let (region, start) = (*region, *start);
                        self.session.label.moving = Some(LabelMoving::Region {
                            region,
                            by: glyph_design - start,
                        });
                        ctx.request_render();
                    }
                    Drag::LabelCut { cut, end, start } => {
                        use crate::application::editor::tools::label::LabelMoving;
                        let (cut, end, start) = (*cut, *end, *start);
                        self.session.label.moving = Some(match end {
                            Some(end) => LabelMoving::CutEnd {
                                cut,
                                end,
                                to: glyph_design,
                            },
                            None => LabelMoving::Cut {
                                cut,
                                by: glyph_design - start,
                            },
                        });
                        ctx.request_render();
                    }
                    Drag::Lasso { points, .. } | Drag::Label { points, .. } => {
                        if points.last().is_none_or(|last| last.distance(at) >= 2.0) {
                            points.push(at);
                            ctx.request_render();
                        }
                    }
                    Drag::Shape { current, .. } => {
                        *current = glyph_design;
                        ctx.request_render();
                    }
                    Drag::None => {}
                }
            }
            event @ (PointerEvent::Up(_) | PointerEvent::Cancel(_)) => {
                let cancelled = matches!(event, PointerEvent::Cancel(_));
                match &self.drag {
                    Drag::Sketch { changed, .. } => {
                        if *changed {
                            ctx.submit_action::<EditorEvent>(EditorEvent::SketchChanged);
                        }
                        self.drag = Drag::None;
                    }
                    Drag::Metaballs { changed, .. } => {
                        let changed = *changed;
                        if cancelled {
                            self.session.cancel_metaball_drag();
                        } else {
                            self.session.end_metaball_drag();
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, changed && !cancelled);
                    }
                    Drag::Points { .. } => {
                        if cancelled {
                            self.session.cancel_point_drag();
                        } else {
                            self.session.end_point_drag();
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, !cancelled);
                    }
                    Drag::Pen {
                        origin,
                        dragging,
                        point_count,
                        active_contour,
                    } => {
                        if cancelled {
                            self.session
                                .cancel_pen_gesture(*point_count, *active_contour);
                        } else if !dragging {
                            self.session.pen_corner(origin.x, origin.y);
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, !cancelled);
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
                        // A locked picture cannot be moved, so a drag over it is a marquee;
                        // a click on it still selects it, to inspect or unlock it.
                        let clicked = rect.width().max(rect.height()) < 3.0;
                        if clicked && !additive && self.session.selection.is_empty() {
                            let design = self.screen_to_glyph_design(*start);
                            self.session.image_selected =
                                self.image_frame(true).is_some_and(|(t, w, h)| {
                                    (t * Rect::new(0.0, 0.0, w, h).to_path(0.1))
                                        .bounding_box()
                                        .contains(design)
                                });
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, false);
                    }
                    Drag::Image { origin, .. } => {
                        let origin = *origin;
                        let moved = self.image_preview.take();
                        let changed = !cancelled
                            && moved.is_some_and(|transform| {
                                transform != origin && self.session.set_image_transform(transform)
                            });
                        self.drag = Drag::None;
                        self.emit(ctx, changed);
                    }
                    Drag::LabelMarquee { start, current } => {
                        use crate::application::editor::tools::label::LabelSelection;
                        let rect = Rect::from_points(*start, *current);
                        if !cancelled {
                            let picked = self.session.corners_in(rect);
                            self.session.label.selected = if picked.is_empty() {
                                None
                            } else {
                                Some(LabelSelection::Corners(picked))
                            };
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, false);
                    }
                    Drag::LabelCorner { .. }
                    | Drag::LabelCorners { .. }
                    | Drag::LabelCut { .. }
                    | Drag::LabelRegion { .. } => {
                        let changed = self.session.finish_label_move(!cancelled);
                        self.drag = Drag::None;
                        self.emit(ctx, changed);
                    }
                    Drag::Label { points, erase } => {
                        let erase = *erase;
                        let mut changed = false;
                        if !cancelled {
                            let design: Vec<Point> = points
                                .iter()
                                .map(|p| self.screen_to_glyph_design(*p))
                                .collect();
                            let travelled = points
                                .iter()
                                .fold(Rect::from_points(points[0], points[0]), |r, p| {
                                    r.union_pt(*p)
                                });
                            let moved = travelled.width().max(travelled.height()) > 4.0;
                            let contours = self.session.label_contours();
                            let near = 12.0 / self.session.viewport.zoom;
                            if !moved {
                                // A click: paint the piece, or with Option, unpaint it.
                                if self.session.label.sample.is_some() {
                                    self.session.label.selected = None;
                                    changed = self.session.paint_piece(design[0], erase);
                                }
                            } else {
                                use crate::application::editor::tools::label::{DragKind, drag_is};
                                let kind = drag_is(&design, &contours, near);
                                let sample = self.session.label.sample.is_some();
                                match kind {
                                    DragKind::Cut if sample => {
                                        changed = self
                                            .session
                                            .add_cut(design[0], design[design.len() - 1]);
                                    }
                                    DragKind::Lasso => {
                                        changed = self.session.add_label_lasso(&design);
                                    }
                                    _ => {
                                        self.session.label.error = Some(if sample {
                                            "Drag across a stroke to cut it, or a loop to lasso"
                                                .into()
                                        } else {
                                            "Drag a loop around writing to make a sample".into()
                                        });
                                    }
                                }
                            }
                        }
                        self.drag = Drag::None;
                        self.label_hover = None;
                        self.emit(ctx, changed);
                    }
                    Drag::Lasso { points, additive } => {
                        if !cancelled {
                            let additive = *additive;
                            let mut path = kurbo::BezPath::new();
                            if let Some(first) = points.first() {
                                path.move_to(*first);
                                for point in &points[1..] {
                                    path.line_to(*point);
                                }
                                path.close_path();
                            }
                            let chosen: Vec<_> = if points.len() >= 3 {
                                self.screen_points()
                                    .into_iter()
                                    .filter(|(_, point, _, _, _)| path.winding(*point) != 0)
                                    .map(|(id, _, _, _, _)| id)
                                    .collect()
                            } else {
                                points
                                    .last()
                                    .and_then(|point| self.hit_point(*point))
                                    .into_iter()
                                    .collect()
                            };
                            if !additive {
                                self.session.selection.clear();
                            }
                            self.session.selection.extend(chosen);
                            self.emit(ctx, false);
                        }
                        self.drag = Drag::None;
                        ctx.request_render();
                    }
                    Drag::Anchor { .. } => {
                        if cancelled {
                            self.session.cancel_anchor_drag();
                        } else {
                            self.session.end_anchor_drag();
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, !cancelled);
                    }
                    Drag::AdvanceLine | Drag::LeftLine { .. } => {
                        if cancelled {
                            self.session.cancel_metric_drag();
                        } else {
                            self.session.end_metric_drag();
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, !cancelled);
                    }
                    Drag::Component { .. } => {
                        if cancelled {
                            self.session.cancel_component_drag();
                        } else {
                            self.session.end_component_drag();
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, !cancelled);
                    }
                    Drag::Shape { start, current } => {
                        if !cancelled {
                            let (s0, c0) = (*start, *current);
                            match self.tool {
                                Tool::Rect => self.session.add_rect(s0.x, s0.y, c0.x, c0.y),
                                Tool::Ellipse => {
                                    self.session.add_ellipse(s0.x, s0.y, c0.x, c0.y);
                                }
                                Tool::Knife => {
                                    self.session.knife_cut(s0, c0);
                                }
                                _ => {}
                            }
                        }
                        self.drag = Drag::None;
                        self.emit(ctx, !cancelled);
                    }
                    Drag::Pan { .. } => self.drag = Drag::None,
                    Drag::None => {}
                }
            }
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
        if self.tool == Tool::Text && self.text.is_some() {
            self.cursor_blink_elapsed_ns = 0;
            self.cursor_visible = true;
            ctx.request_anim_frame();
        }
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
        let alt = key.modifiers.alt();
        // From the config file; by default one grid step, so a nudge cannot round back to
        // where it started.
        // Command and Shift together make large moves.
        let step = crate::application::platform::config::nudge().for_modifiers(shift, cmd);

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
                self.emit_text_proof_selection(ctx);
                ctx.request_render();
                ctx.set_handled();
                return;
            }
        }

        if self.tool == Tool::Select
            && self.session.image_selected
            && self.session.selection.is_empty()
            && self.field.is_none()
            && !cmd
            && let Some(transform) = self.session.layer_image().map(|image| image.transform())
        {
            let nudge = |dx: f64, dy: f64| Affine::translate((dx, dy)) * transform;
            let locked = self.session.image_locked();
            let (handled, edited) = match &key.key {
                Key::Named(NamedKey::Escape) => {
                    self.session.image_selected = false;
                    (true, false)
                }
                // A locked picture can be selected and nothing more.
                _ if locked => (false, false),
                Key::Named(NamedKey::ArrowLeft) => {
                    (true, self.session.set_image_transform(nudge(-step, 0.0)))
                }
                Key::Named(NamedKey::ArrowRight) => {
                    (true, self.session.set_image_transform(nudge(step, 0.0)))
                }
                Key::Named(NamedKey::ArrowUp) => {
                    (true, self.session.set_image_transform(nudge(0.0, step)))
                }
                Key::Named(NamedKey::ArrowDown) => {
                    (true, self.session.set_image_transform(nudge(0.0, -step)))
                }
                Key::Named(NamedKey::Backspace | NamedKey::Delete) => {
                    (true, self.session.remove_image())
                }
                _ => (false, false),
            };
            if handled {
                self.emit(ctx, edited);
                ctx.request_render();
                ctx.set_handled();
                return;
            }
        }

        if self.tool == Tool::Label && self.field.is_none() && !cmd {
            let mut edited = false;
            let handled = match &key.key {
                Key::Named(NamedKey::Tab) if shift => {
                    self.session.step_label_letter(-1);
                    true
                }
                Key::Named(NamedKey::Tab | NamedKey::Enter) => {
                    self.session.next_label_letter();
                    true
                }
                Key::Named(NamedKey::Backspace | NamedKey::Delete) => {
                    edited = self.session.delete_label_selection();
                    true
                }
                Key::Named(NamedKey::Escape) if self.session.label.selected.is_some() => {
                    self.session.label.selected = None;
                    true
                }
                Key::Named(NamedKey::Escape) if self.session.label.sample.is_some() => {
                    self.session.select_sample(None);
                    true
                }
                _ => false,
            };
            if handled {
                self.emit(ctx, edited);
                ctx.request_render();
                ctx.set_handled();
                return;
            }
        }

        let edits_metaballs = self.tool == Tool::Metaball
            || (self.tool == Tool::Select
                && !self.session.metaballs.selected.is_empty()
                && self.session.selection.is_empty()
                && self.session.selected_anchor.is_none()
                && self.session.selected_component.is_none());
        if edits_metaballs && self.field.is_none() {
            let mut edited = false;
            let handled = match &key.key {
                Key::Character(c) if cmd && c.eq_ignore_ascii_case("a") => {
                    self.session.select_all_metaballs();
                    true
                }
                Key::Named(NamedKey::Backspace | NamedKey::Delete) => {
                    edited = self.session.delete_metaballs();
                    true
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    edited = self
                        .session
                        .move_metaballs(kurbo::Vec2::new(-step, 0.0), false);
                    true
                }
                Key::Named(NamedKey::ArrowRight) => {
                    edited = self
                        .session
                        .move_metaballs(kurbo::Vec2::new(step, 0.0), false);
                    true
                }
                Key::Named(NamedKey::ArrowUp) => {
                    edited = self
                        .session
                        .move_metaballs(kurbo::Vec2::new(0.0, step), false);
                    true
                }
                Key::Named(NamedKey::ArrowDown) => {
                    edited = self
                        .session
                        .move_metaballs(kurbo::Vec2::new(0.0, -step), false);
                    true
                }
                Key::Named(NamedKey::Escape) => {
                    self.session.metaballs =
                        crate::application::editor::tools::metaballs::MetaballSelection::default();
                    true
                }
                _ => false,
            };
            if handled {
                self.emit(ctx, edited);
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
                // Masonry, GPUI, and Web all deliver ordinary typing as a
                // logical character key. IME commits remain a separate path
                // for composed text.
                Key::Character(value) => {
                    let value: String = value.chars().filter(|c| !c.is_control()).collect();
                    let changed = !value.is_empty() && text.commit_preedit(&value);
                    (true, changed)
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
                } else {
                    self.emit_text_proof_selection(ctx);
                }
                ctx.request_render();
                ctx.set_handled();
            }
            return;
        }
        let (edited, handled) = match &key.key {
            Key::Named(NamedKey::Escape) => {
                if self.session.pen_is_active() || self.session.contour_drawing_is_active() {
                    self.session.pen_cancel();
                    self.emit(ctx, false);
                    ctx.set_handled();
                }
                // Otherwise do not handle: the shortcut host turns Escape into
                // "back to overview".
                return;
            }
            // Option nudges on-curve points without their handles.
            Key::Named(NamedKey::ArrowLeft) => (self.session.nudge_with(-step, 0.0, alt), true),
            Key::Named(NamedKey::ArrowRight) => (self.session.nudge_with(step, 0.0, alt), true),
            Key::Named(NamedKey::ArrowUp) => (self.session.nudge_with(0.0, step, alt), true),
            Key::Named(NamedKey::ArrowDown) => (self.session.nudge_with(0.0, -step, alt), true),
            // Tab and Shift-Tab walk the selection along its contour, as in Glyphs.
            Key::Named(NamedKey::Tab) if !cmd => {
                self.session.select_adjacent_point(!shift);
                ctx.request_render();
                (false, true)
            }
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

    fn on_anim_frame(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        interval: u64,
    ) {
        if self.tool != Tool::Text || self.text.is_none() {
            return;
        }

        self.cursor_blink_elapsed_ns = self
            .cursor_blink_elapsed_ns
            .saturating_add(interval)
            .rem_euclid(CURSOR_BLINK_CYCLE_NS);
        // Keep the exact half-cycle boundary visible. Headless proof renders
        // after a 500 ms settle frame, while a real display crosses the
        // boundary on its following frame.
        let visible = cursor_visible_at(self.cursor_blink_elapsed_ns);
        if visible != self.cursor_visible {
            self.cursor_visible = visible;
            ctx.request_paint_only();
        }
        ctx.request_anim_frame();
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn get_cursor(&self, _ctx: &QueryCtx<'_>, _pos: Point) -> CursorIcon {
        if self.tool == Tool::Hand {
            if matches!(self.drag, Drag::Pan { .. }) {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            }
        } else {
            CursorIcon::Default
        }
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
    sketch: Arc<Mutex<SketchLayer>>,
    sketch_source: usize,
    sketch_revision: u64,
    preview_mode: bool,
    view: ViewOptions,
    ghosts: Arc<Vec<Arc<kurbo::BezPath>>>,
    interp: Option<Arc<kurbo::BezPath>>,
    underlay: Underlay,
    text: Option<crate::application::editor::tools::text::TextInputs>,
    focus_target: Arc<Mutex<Option<WidgetId>>>,
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
    sketch: Arc<Mutex<SketchLayer>>,
    sketch_source: usize,
    preview_mode: bool,
    view: ViewOptions,
    ghosts: Arc<Vec<Arc<kurbo::BezPath>>>,
    interp: Option<Arc<kurbo::BezPath>>,
    underlay: Underlay,
    text: Option<crate::application::editor::tools::text::TextInputs>,
    focus_target: Arc<Mutex<Option<WidgetId>>>,
    on_event: F,
) -> EditorView<F> {
    let sketch_revision = sketch
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .revision();
    EditorView {
        session,
        palette,
        groups,
        mark,
        tool,
        sketch,
        sketch_source,
        sketch_revision,
        preview_mode,
        view,
        ghosts,
        interp,
        underlay,
        text,
        focus_target,
        on_event,
    }
}

/// The analysis overlays: what the editor draws on top of the outline
/// besides the points.
///
/// They read from Runebender's analysis modules so results remain consistent across
/// interfaces.
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
            runebender::analysis::measure::label(value)
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
    /// A completed, unapplied Brush draft rendered without compiling the font.
    pub brush_candidate: Option<Arc<kurbo::BezPath>>,
    /// The picture placed behind the glyph, such as a page of calligraphy to trace.
    pub image: Option<PlacedImage>,
}

/// A decoded picture and where it sits on the canvas.
#[derive(Clone, PartialEq)]
pub(crate) struct PlacedImage {
    /// The pixels, already faded to sit behind the outline.
    pub data: ImageData,
    /// Maps pixel space (origin at the top left, y down) to font units.
    pub to_glyph: Affine,
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
            sketch: self.sketch.clone(),
            sketch_source: self.sketch_source,
            sketch_image: None,
            preview_mode: self.preview_mode,
            ghosts: self.ghosts.clone(),
            interp: self.interp.clone(),
            underlay: self.underlay.clone(),
            image_preview: None,
            text: self
                .text
                .as_ref()
                .map(crate::application::editor::tools::text::TextState::new),
            text_inputs: self.text.clone(),
            size: Size::ZERO,
            drag: Drag::None,
            hover: None,
            insert_preview: None,
            label_hover: None,
            menu: None,
            view: self.view,
            field: None,
            field_buf: String::new(),
            cursor_blink_elapsed_ns: 0,
            cursor_visible: true,
        };
        let pod = ctx.with_action_widget(|ctx| ctx.create_pod(widget));
        *self
            .focus_target
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(pod.new_widget.id());
        (pod, ())
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
        if !Arc::ptr_eq(&self.palette, &prev.palette) {
            element.widget.palette = self.palette.clone();
            dirty = true;
        }
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
            element.widget.cursor_blink_elapsed_ns = 0;
            element.widget.cursor_visible = true;
            if self.tool == Tool::Text {
                element.ctx.request_anim_frame();
            }
            if self.tool != Tool::Pen {
                element.widget.session.pen_cancel();
            }
            dirty = true;
        }
        if self.sketch_source != prev.sketch_source {
            element.widget.sketch_source = self.sketch_source;
            dirty = true;
        }
        if self.sketch_revision != prev.sketch_revision {
            dirty = true;
        }
        if self.preview_mode != prev.preview_mode {
            element.widget.preview_mode = self.preview_mode;
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
        // The pane allocates the ghost list on every rebuild; the paths inside it are shared
        // with the font, so the same set of them means the same ghosts.
        let same_ghosts = self.ghosts.len() == prev.ghosts.len()
            && self
                .ghosts
                .iter()
                .zip(prev.ghosts.iter())
                .all(|(a, b)| Arc::ptr_eq(a, b));
        if !same_ghosts {
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
                    element.widget.text = Some(
                        crate::application::editor::tools::text::TextState::new(inputs),
                    );
                    element.widget.fit_text();
                    element.ctx.request_layout();
                }
                // A live client can replace the line without a widget input event.
                // Ordinary typing already changed the widget buffer, so do not
                // reconstruct it (and lose its caret) when the app catches up.
                (Some(inputs), Some(state))
                    if element.widget.text_inputs.as_ref().is_some_and(|old| {
                        inputs.replaced_text(old) && state.buffer.text() != inputs.initial_text()
                    }) =>
                {
                    *state = crate::application::editor::tools::text::TextState::new(inputs);
                    element.widget.fit_text();
                    element.ctx.request_layout();
                }
                // Same tool, new master or edited glyph: keep what has
                // been typed and re-read the metrics.
                (Some(inputs), Some(state)) => state.refresh(inputs),
                (Some(inputs), None) => {
                    element.widget.text = Some(
                        crate::application::editor::tools::text::TextState::new(inputs),
                    );
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
                dispatch_editor_event(app, &mut element.widget.session, *event, &self.on_event);
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    use masonry::dpi::PhysicalPosition;
    use masonry::theme::default_property_set;
    use masonry::ui_events::pointer::PointerState;
    use masonry_testing::{PRIMARY_MOUSE, TestHarness};

    fn projected_glyph(session: &Session) -> norad::Glyph {
        session
            .projected_glyph()
            .expect("an editor session has a canonical layer")
    }

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

    fn empty_session() -> Session {
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
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
            sketch: Arc::new(Mutex::new(SketchLayer::new("A".into(), 0, 500.0))),
            sketch_source: 0,
            sketch_image: None,
            preview_mode: false,
            ghosts: Arc::new(Vec::new()),
            interp: None,
            underlay: Underlay::default(),
            image_preview: None,
            text: None,
            text_inputs: None,
            size: Size::ZERO,
            drag: Drag::None,
            hover: None,
            insert_preview: None,
            label_hover: None,
            menu: None,
            view: ViewOptions::default(),
            field: None,
            field_buf: String::new(),
            cursor_blink_elapsed_ns: 0,
            cursor_visible: true,
        }
    }

    #[test]
    fn design_grid_dot_is_round_instead_of_a_square_tile() {
        let mut dot = kurbo::BezPath::new();
        push_grid_dot(&mut dot, Point::new(10.0, 10.0), 4.0);
        assert_eq!(dot.bounding_box(), Rect::new(8.0, 8.0, 12.0, 12.0));
        assert_ne!(dot.winding(Point::new(10.0, 10.0)), 0);
        assert_eq!(
            dot.winding(Point::new(8.2, 8.2)),
            0,
            "a rounded dot does not fill its bounding-box corner"
        );
    }

    #[test]
    fn metric_rules_are_centered_on_their_design_coordinates() {
        let horizontal = horizontal_rule_rect(10.0, 30.0, 12.25, 1.0);
        assert_eq!(horizontal.center().y, 12.25);
        assert_eq!(horizontal.height(), 1.0);

        let vertical = vertical_rule_rect(18.75, 40.0, 5.0, 1.0);
        assert_eq!(vertical.center().x, 18.75);
        assert_eq!(vertical.width(), 1.0);
    }

    #[test]
    fn point_window_repaints_the_grid_intersection_under_its_center() {
        let marks = point_grid_marks(
            Affine::IDENTITY,
            Point::new(8.0, 8.0),
            4.5,
            false,
            8.0,
            2.0,
            false,
        );
        assert_ne!(marks.winding(Point::new(8.0, 8.0)), 0);
        assert_eq!(marks.bounding_box(), Rect::new(7.0, 7.0, 9.0, 9.0));
    }

    #[test]
    fn point_window_preserves_an_off_center_grid_dot() {
        let marks = point_grid_marks(
            Affine::IDENTITY,
            Point::new(10.0, 8.0),
            4.5,
            false,
            8.0,
            2.0,
            false,
        );
        assert_ne!(marks.winding(Point::new(8.0, 8.0)), 0);
        assert_eq!(marks.winding(Point::new(10.0, 8.0)), 0);
    }

    #[test]
    fn point_window_clips_line_grid_to_the_marker() {
        let marks = point_grid_marks(
            Affine::IDENTITY,
            Point::new(8.0, 8.0),
            4.5,
            false,
            8.0,
            2.0,
            true,
        );
        assert_eq!(marks.bounding_box(), Rect::new(3.5, 3.5, 12.5, 12.5));
    }

    #[test]
    fn design_grid_levels_fade_in_at_the_gpui_thresholds() {
        assert_eq!(grid_alphas(0.8), (0.0, 0.0));
        assert_eq!(grid_alphas(1.6), (1.0, 0.0));
        assert_eq!(grid_alphas(8.0), (1.0, 0.0));
        assert_eq!(grid_alphas(16.0), (1.0, 1.0));
    }

    #[test]
    fn text_cursor_blinks_on_a_one_second_cycle() {
        assert!(cursor_visible_at(0));
        assert!(cursor_visible_at(CURSOR_BLINK_HALF_CYCLE_NS));
        assert!(!cursor_visible_at(CURSOR_BLINK_HALF_CYCLE_NS + 1));
        assert!(!cursor_visible_at(CURSOR_BLINK_CYCLE_NS - 1));
        assert!(cursor_visible_at(CURSOR_BLINK_CYCLE_NS));
    }

    #[test]
    fn text_cursor_animation_hides_then_resets_on_input() {
        use masonry::core::keyboard::{Code, KeyboardEvent};

        let mut editor = widget();
        editor.tool = Tool::Text;
        editor.text = Some(crate::application::editor::tools::text::TextState::test_buffer());
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        harness.focus_on(Some(harness.root_id()));

        harness.animate_ms(501);
        assert!(!harness.edit_root_widget(|root| root.widget.cursor_visible));

        harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Named(NamedKey::ArrowLeft),
            code: Code::ArrowLeft,
            ..KeyboardEvent::default()
        }));
        assert!(harness.edit_root_widget(|root| root.widget.cursor_visible));
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.cursor_blink_elapsed_ns),
            0
        );
    }

    #[test]
    fn start_markers_use_closed_contours_and_first_on_curve_direction() {
        let mut editor = widget();
        let affine = editor.session.viewport.affine();
        let initial = editor.start_markers();
        assert_eq!(initial.len(), 1);
        assert_eq!(initial[0].0, editor.session.point_id_at(0, 0).unwrap());
        assert_eq!(initial[0].1, affine * Point::new(0.0, 0.0));
        assert_eq!(initial[0].2, affine * Point::new(400.0, 0.0));
        let first = editor.session.point_id_at(0, 0).unwrap();
        assert!(
            editor
                .session
                .stage_canonical_string_edit("test point kind", |draft| {
                    draft
                        .set_point_type(first, runebender::font::LayerPointType::OffCurve)
                        .map_err(|error| error.to_string())
                })
                .unwrap()
        );
        assert_eq!(
            editor.start_markers()[0].0,
            editor.session.point_id_at(0, 1).unwrap(),
            "skip leading handles"
        );
        let first = editor.session.point_id_at(0, 0).unwrap();
        assert!(
            editor
                .session
                .stage_canonical_string_edit("test open path", |draft| {
                    draft
                        .set_point_type(first, runebender::font::LayerPointType::Move)
                        .map_err(|error| error.to_string())
                })
                .unwrap()
        );
        assert!(
            editor.start_markers().is_empty(),
            "open paths have no marker"
        );
        let points = (0..)
            .map_while(|index| editor.session.point_id_at(0, index))
            .collect::<Vec<_>>();
        assert!(
            editor
                .session
                .stage_canonical_string_edit("test empty path", |draft| {
                    draft
                        .delete_points(&points)
                        .map_err(|error| error.to_string())
                })
                .unwrap()
        );
        assert!(
            editor.start_markers().is_empty(),
            "empty contours are harmless"
        );
    }

    #[test]
    fn start_marker_replaces_the_node_and_softens_only_smooth_starts() {
        let center = Point::new(20.0, 20.0);
        let toward = Point::new(30.0, 20.0);
        let sharp = direction_marker_shape(center, toward, POINT_CORNER_RADIUS, false).unwrap();
        let smooth = direction_marker_shape(center, toward, POINT_CURVE_RADIUS, true).unwrap();

        assert_eq!(
            sharp.elements().len(),
            4,
            "a corner start is a crisp triangle"
        );
        assert_eq!(
            smooth
                .elements()
                .iter()
                .filter(|element| matches!(element, kurbo::PathEl::QuadTo(_, _)))
                .count(),
            3,
            "a smooth start rounds each wedge corner"
        );
        assert!(
            direction_marker_shape(center, center, POINT_CURVE_RADIUS, true).is_none(),
            "a zero-length direction falls back to the ordinary node"
        );
    }

    #[test]
    fn outline_tools_use_the_active_sorts_origin() {
        let mut editor = widget();
        let mut text = crate::application::editor::tools::text::TextState::test_buffer();
        assert!(text.buffer.insert_character('B'));
        assert!(text.buffer.activate_sort(1));
        editor.text = Some(text);

        let origin = editor.active_sort_origin();
        assert_eq!(origin, Point::new(500.0, 0.0));
        assert_eq!(
            editor.glyph_affine() * Point::ORIGIN,
            editor.session.viewport.affine() * origin,
            "the editable glyph is drawn where its active sort sits"
        );
        let screen = editor.glyph_affine() * Point::new(40.0, 60.0);
        let design = editor.screen_to_glyph_design(screen);
        assert!((design.x - 40.0).abs() < 0.001);
        assert!((design.y - 60.0).abs() < 0.001);
    }

    #[test]
    fn parked_text_does_not_consume_outline_tool_typing() {
        use masonry::core::Ime;
        use masonry::core::keyboard::{Code, KeyboardEvent, Modifiers};
        let mut editor = widget();
        editor.text = Some(crate::application::editor::tools::text::TextState::test_buffer());
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
        let (event, _) = harness
            .pop_action::<EditorEvent>()
            .expect("ordinary keyboard text reports the complete buffer");
        assert!(
            matches!(event, EditorEvent::TextChanged(text) if text == "AB"),
            "logical character keys type directly, as Masonry, GPUI, and Web do"
        );
        let (event, _) = harness
            .pop_action::<EditorEvent>()
            .expect("typing reports the selected proof context separately");
        assert!(matches!(event, EditorEvent::TextProofSelection(Err(_))));
        assert_eq!(
            harness.edit_root_widget(|root| root.widget.text.as_ref().unwrap().buffer.len()),
            2,
            "ordinary keyboard input does not depend on an IME commit"
        );
        harness.process_text_event(named(NamedKey::Backspace));
        let (event, _) = harness
            .pop_action::<EditorEvent>()
            .expect("deleting the direct-key test glyph reports the restored buffer");
        assert!(matches!(event, EditorEvent::TextChanged(text) if text == "A"));
        let (event, _) = harness
            .pop_action::<EditorEvent>()
            .expect("deleting refreshes the selected proof context");
        assert!(matches!(event, EditorEvent::TextProofSelection(Err(_))));
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
        let (event, _) = harness
            .pop_action::<EditorEvent>()
            .expect("IME commit refreshes the selected proof context");
        assert!(matches!(event, EditorEvent::TextProofSelection(Err(_))));
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

    #[test]
    fn text_tool_consumes_space_as_text() {
        use masonry::core::keyboard::{Code, KeyboardEvent};
        let mut editor = widget();
        editor.tool = Tool::Text;
        editor.text = Some(crate::application::editor::tools::text::TextState::test_buffer());
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        harness.focus_on(Some(harness.root_id()));
        harness.process_text_event(TextEvent::Keyboard(KeyboardEvent {
            state: KeyState::Down,
            key: Key::Character(" ".into()),
            code: Code::Space,
            ..KeyboardEvent::default()
        }));

        let (event, _) = harness
            .pop_action::<EditorEvent>()
            .expect("the Text tool consumes Space and reports the buffer");
        assert!(matches!(event, EditorEvent::TextChanged(text) if text == "A "));
        let (event, _) = harness
            .pop_action::<EditorEvent>()
            .expect("space refreshes the selected proof context");
        assert!(matches!(event, EditorEvent::TextProofSelection(Err(_))));
    }

    #[test]
    fn select_double_click_activates_the_composed_sort() {
        let mut editor = widget();
        let mut text = crate::application::editor::tools::text::TextState::test_buffer();
        assert!(text.buffer.insert_character('B'));
        editor.text = Some(text);
        editor.tool = Tool::Select;
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        let at = harness.edit_root_widget(|root| {
            let editor = &root.widget;
            let text = editor.text.as_ref().unwrap();
            let layout = text.buffer.layout(text.line_height);
            let sort = &layout.items[1];
            editor.session.viewport.affine()
                * Point::new(sort.x + sort.advance_width / 2.0, sort.y + 300.0)
        });
        let state = PointerState {
            position: PhysicalPosition::new(at.x, at.y),
            count: 2,
            ..PointerState::default()
        };
        harness.process_pointer_event(PointerEvent::Down(PointerButtonEvent {
            pointer: PRIMARY_MOUSE,
            button: Some(PointerButton::Primary),
            state,
        }));

        let (event, _) = harness
            .pop_action::<EditorEvent>()
            .expect("double-clicking a sort reports the glyph to edit");
        assert!(matches!(
            event,
            EditorEvent::EditGlyph {
                name,
                tool: Tool::Select
            } if name == "B"
        ));
        let (event, _) = harness
            .pop_action::<EditorEvent>()
            .expect("double-click reports the shaped proof context after activating the glyph");
        assert!(matches!(event, EditorEvent::TextProofSelection(Err(_))));
        assert_eq!(
            harness.edit_root_widget(|root| root
                .widget
                .text
                .as_ref()
                .unwrap()
                .buffer
                .active_sort()),
            Some(1),
            "the clicked sort becomes the editable one"
        );
    }

    #[test]
    fn coincident_metric_heights_have_one_intersection() {
        let metrics = crate::application::editor::session::Metrics {
            upm: 750.0,
            ascender: 750.0,
            descender: -250.0,
            x_height: 500.0,
            cap_height: 500.0,
        };
        assert_eq!(
            text_sort_metric_ys(&metrics),
            vec![-250.0, 0.0, 500.0, 750.0]
        );
    }

    #[test]
    fn space_preview_hides_metrics_hit_targets() {
        let mut widget = widget();
        widget.size = Size::new(600.0, 400.0);
        assert!(widget.metric_boxes().is_some());
        widget.preview_mode = true;
        assert!(
            widget.metric_boxes().is_none(),
            "hidden preview chrome cannot intercept a pan gesture"
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

    #[test]
    fn empty_glyph_keeps_metrics_inputs_and_can_change_its_advance() {
        let mut widget = widget();
        widget.session = empty_session();
        widget.size = Size::new(600.0, 400.0);

        assert!(widget.session.side_bearings().is_none());
        assert_eq!(widget.metric_boxes().expect("the panel fits").len(), 3);

        widget.focus_metric(MetricField::Width);
        assert_eq!(widget.field_buf, "500");
        widget.field_buf = "720".into();
        assert!(widget.commit_metric());
        assert_eq!(widget.session.advance(), 720.0);

        widget.focus_metric(MetricField::Rsb);
        assert_eq!(widget.field_buf, "720");
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
            let boxes = widget.metric_boxes().unwrap();
            assert_eq!(boxes[0].1.x0, left + 70.0);
            assert_eq!(boxes[1].1.x0, left + 132.0);
            assert_eq!(boxes[2].1.x0, left + 194.0);
        }
        widget.size = Size::new(280.0, 510.0);
        assert!(widget.metric_boxes().is_none());
        widget.size = Size::new(786.0, 60.0);
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

    #[test]
    fn right_click_selects_the_component_under_the_menu() {
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(projected_glyph(&session()));
        let mut composite = norad::Glyph::new("composite");
        composite.width = 500.0;
        composite.components.push(norad::Component::new(
            norad::Name::new("A").expect("valid glyph name"),
            norad::AffineTransform::default(),
            None,
        ));
        font.default_layer_mut().insert_glyph(composite);
        let mut editor = widget();
        editor.session = Session::new(&font, "composite").expect("composite resolves");
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        let at =
            harness.edit_root_widget(|root| root.widget.glyph_affine() * Point::new(200.0, 350.0));
        harness.mouse_move(at);
        harness.mouse_button_press(Some(PointerButton::Secondary));
        let menu = harness.edit_root_widget(|root| {
            assert!(root.widget.session.selected_component.is_some());
            root.widget.menu.expect("component menu opens")
        });
        let menu_origin = harness
            .get_widget_with_id(menu)
            .ctx()
            .to_window(Point::ORIGIN);
        harness.mouse_move(menu_origin + (20.0, 16.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        let mut toggled = false;
        while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
            toggled |= matches!(event, EditorEvent::ToggleComponentAlignment);
        }
        assert!(toggled, "the component menu submits the alignment command");
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

    #[test]
    fn a_picture_is_selected_moved_and_resized_from_a_corner() {
        let mut editor = widget();
        let placed = runebender::font::LayerImage::new(
            std::path::PathBuf::from("page.png"),
            None,
            Affine::new([2.0, 0.0, 0.0, 2.0, 100.0, 0.0]),
        )
        .unwrap();
        assert!(editor.session.set_image(Some(placed)));
        editor.underlay.image = Some(PlacedImage {
            data: ImageData {
                data: Blob::new(Arc::new(vec![0; 100 * 100 * 4])),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
                width: 100,
                height: 100,
            },
            to_glyph: Affine::new([2.0, 0.0, 0.0, -2.0, 100.0, 200.0]),
        });
        let original = projected_glyph(&editor.session).contours;
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        // the picture spans x 100..300 and y 0..200 in design space; press in the middle
        let (inside, delta) = harness.edit_root_widget(|root| {
            let affine = root.widget.glyph_affine();
            (
                affine * Point::new(200.0, 100.0),
                affine * Point::new(230.0, 140.0),
            )
        });
        harness.mouse_move(inside);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_move(delta);
        harness.mouse_button_release(Some(PointerButton::Primary));
        let corner = harness.edit_root_widget(|root| {
            let session = &root.widget.session;
            assert!(session.image_selected);
            let [_, _, _, _, x, y] = session.layer_image().unwrap().transform().as_coeffs();
            assert!(
                (x - 130.0).abs() < 1.0 && (y - 40.0).abs() < 1.0,
                "moved to {x}, {y}"
            );
            assert_eq!(projected_glyph(session).contours, original);
            root.widget.glyph_affine() * Point::new(330.0, 240.0)
        });
        // the top right corner, pulled out to twice the size about the lower left
        let pulled =
            harness.edit_root_widget(|root| root.widget.glyph_affine() * Point::new(530.0, 440.0));
        harness.mouse_move(corner);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_move(pulled);
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|root| {
            let [scale, _, _, _, x, y] = root
                .widget
                .session
                .layer_image()
                .unwrap()
                .transform()
                .as_coeffs();
            assert!((scale - 4.0).abs() < 0.05, "scaled to {scale}");
            assert!(
                (x - 130.0).abs() < 1.0 && (y - 40.0).abs() < 1.0,
                "anchor kept at {x}, {y}"
            );
        });
    }

    #[test]
    fn a_locked_picture_can_be_selected_but_not_changed() {
        // a locked picture as it comes from a source, beside the glyph's own ink
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
        glyph.image = Some(
            norad::Image::new(
                std::path::PathBuf::from("page.png"),
                None,
                norad::AffineTransform {
                    x_scale: 2.0,
                    xy_scale: 0.0,
                    yx_scale: 0.0,
                    y_scale: 2.0,
                    x_offset: 450.0,
                    y_offset: 0.0,
                },
            )
            .unwrap(),
        );
        glyph.lib.insert(
            "com.runebender.imageLocked".into(),
            plist::Value::Boolean(true),
        );
        font.default_layer_mut().insert_glyph(glyph);
        let mut editor = widget();
        editor.session = Session::new(&font, "A").unwrap();
        let placed = editor.session.layer_image().unwrap();
        assert!(editor.session.image_locked());
        editor.underlay.image = Some(PlacedImage {
            data: ImageData {
                data: Blob::new(Arc::new(vec![0; 100 * 100 * 4])),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
                width: 100,
                height: 100,
            },
            to_glyph: Affine::new([2.0, 0.0, 0.0, -2.0, 450.0, 200.0]),
        });
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        let (inside, beside, outside) = harness.edit_root_widget(|root| {
            let affine = root.widget.glyph_affine();
            (
                affine * Point::new(550.0, 100.0),
                affine * Point::new(610.0, 160.0),
                affine * Point::new(800.0, 100.0),
            )
        });
        // beside the glyph's own ink, which a click would select first: a click selects it
        harness.mouse_move(inside);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|root| assert!(root.widget.session.image_selected));
        // a drag over it does not move it, and Delete does not remove it
        harness.mouse_move(inside);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_move(beside);
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.mouse_move(inside);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::Delete)));
        harness.edit_root_widget(|root| {
            let session = &mut root.widget.session;
            assert_eq!(
                session.layer_image().unwrap().transform(),
                placed.transform()
            );
            assert!(!session.remove_image() && !session.set_image_transform(Affine::IDENTITY));
        });
        // a click beside it deselects it
        harness.mouse_move(outside);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|root| assert!(!root.widget.session.image_selected));
    }

    #[test]
    fn every_arrow_nudges_a_point_the_same_distance() {
        // A nudge smaller than the grid used to round back to the start in two directions and
        // jump double in the others.
        let step = crate::application::platform::config::nudge().step;
        for (key, delta) in [
            (NamedKey::ArrowRight, (step, 0.0)),
            (NamedKey::ArrowLeft, (-step, 0.0)),
            (NamedKey::ArrowUp, (0.0, step)),
            (NamedKey::ArrowDown, (0.0, -step)),
        ] {
            let mut editor = widget();
            let (id, before) = {
                let point = editor.session.points()[0];
                (point.id, point.point)
            };
            editor.session.selection = HashSet::from([id]);
            let mut harness =
                TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
            harness.focus_on(Some(harness.root_id()));
            harness.process_text_event(TextEvent::key_down(Key::Named(key)));
            harness.edit_root_widget(|root| {
                let after = root
                    .widget
                    .session
                    .points()
                    .into_iter()
                    .find(|point| point.id == id)
                    .unwrap()
                    .point;
                assert_eq!(after - before, kurbo::Vec2::from(delta), "{key:?}");
            });
        }
    }

    #[test]
    fn a_double_click_switches_a_point_between_corner_and_smooth() {
        let editor = widget();
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        let (id, at, smooth) = harness.edit_root_widget(|root| {
            let (id, at, _, smooth, _) = root.widget.screen_points()[0];
            (id, at, smooth)
        });
        let state = PointerState {
            position: PhysicalPosition::new(at.x, at.y),
            count: 2,
            ..PointerState::default()
        };
        harness.process_pointer_event(PointerEvent::Down(PointerButtonEvent {
            pointer: PRIMARY_MOUSE,
            button: Some(PointerButton::Primary),
            state,
        }));
        harness.edit_root_widget(|root| {
            let point = root
                .widget
                .session
                .points()
                .into_iter()
                .find(|point| point.id == id)
                .unwrap();
            assert_eq!(point.smooth, !smooth, "the double click switched it");
        });
    }

    #[test]
    fn a_pen_click_on_a_line_adds_a_corner_point_to_it() {
        let mut editor = widget();
        editor.tool = Tool::Pen;
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        let at =
            harness.edit_root_widget(|root| root.widget.glyph_affine() * Point::new(200.0, 0.0));
        harness.mouse_move(at);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|root| {
            let session = &root.widget.session;
            let points = session.points();
            assert_eq!(
                points.len(),
                5,
                "one point joined the square, no new contour"
            );
            assert!(!session.pen_is_active());
            let added = points
                .iter()
                .find(|point| point.point == Point::new(200.0, 0.0))
                .expect("the point sits on the bottom edge");
            assert!(added.on_curve && !added.smooth);
            assert_eq!(session.selection, HashSet::from([added.id]));
        });
    }

    #[test]
    fn a_pen_hover_on_a_line_previews_the_new_point() {
        let mut editor = widget();
        editor.tool = Tool::Pen;
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        let (on_edge, away) = harness.edit_root_widget(|root| {
            let affine = root.widget.glyph_affine();
            (
                affine * Point::new(200.0, 0.0),
                affine * Point::new(200.0, 350.0),
            )
        });
        harness.mouse_move(on_edge);
        harness.edit_root_widget(|root| {
            assert_eq!(
                root.widget.insert_preview,
                Some((Point::new(200.0, 0.0), false))
            );
        });
        harness.mouse_move(away);
        harness.edit_root_widget(|root| assert_eq!(root.widget.insert_preview, None));

        // While a new contour is being drawn, the pen only draws.
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.mouse_move(on_edge);
        harness.edit_root_widget(|root| {
            assert!(root.widget.session.pen_is_active());
            assert_eq!(root.widget.insert_preview, None);
        });
    }

    /// Two curves meeting at a smooth point, the fourth point, at (300, 300).
    fn smooth_curve_session() -> Session {
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
        let mut contour = norad::Contour::default();
        for (x, y, typ, smooth) in [
            (0.0, 0.0, norad::PointType::Line, false),
            (0.0, 200.0, norad::PointType::OffCurve, false),
            (200.0, 300.0, norad::PointType::OffCurve, false),
            (300.0, 300.0, norad::PointType::Curve, true),
            (400.0, 300.0, norad::PointType::OffCurve, false),
            (500.0, 200.0, norad::PointType::OffCurve, false),
            (500.0, 0.0, norad::PointType::Curve, false),
        ] {
            contour
                .points
                .push(norad::ContourPoint::new(x, y, typ, smooth, None, None));
        }
        glyph.contours.push(contour);
        font.default_layer_mut().insert_glyph(glyph);
        Session::new(&font, "A").expect("the glyph is there")
    }

    #[test]
    fn an_option_nudge_moves_a_smooth_point_without_its_handles() {
        use masonry::core::keyboard::{Code, KeyboardEvent, Modifiers};
        let nudge = crate::application::platform::config::nudge();
        let (step, shift_step, command_step) = (nudge.step, nudge.shift, nudge.command);
        let mut editor = widget();
        editor.session = smooth_curve_session();
        let smooth = editor.session.points()[3].id;
        editor.session.selection = HashSet::from([smooth]);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        harness.focus_on(Some(harness.root_id()));
        let arrow_up = |modifiers| {
            TextEvent::Keyboard(KeyboardEvent {
                state: KeyState::Down,
                key: Key::Named(NamedKey::ArrowUp),
                code: Code::Unidentified,
                modifiers,
                ..KeyboardEvent::default()
            })
        };
        let positions = |harness: &mut TestHarness<EditorWidget>| {
            harness.edit_root_widget(|root| {
                root.widget.session.points()[2..=4]
                    .iter()
                    .map(|point| point.point)
                    .collect::<Vec<_>>()
            })
        };
        // Without a workspace, each nudge stages from the same committed
        // state, so every press below starts from the original outline.
        for (modifiers, rise, handles_follow) in [
            (Modifiers::ALT, step, false),
            (Modifiers::ALT | Modifiers::SHIFT, shift_step, false),
            (Modifiers::empty(), step, true),
            (Modifiers::META | Modifiers::SHIFT, command_step, true),
        ] {
            harness.process_text_event(arrow_up(modifiers));
            let handle_y = if handles_follow { 300.0 + rise } else { 300.0 };
            assert_eq!(
                positions(&mut harness),
                [
                    Point::new(200.0, handle_y),
                    Point::new(300.0, 300.0 + rise),
                    Point::new(400.0, handle_y),
                ],
                "{modifiers:?}"
            );
        }
    }

    #[test]
    fn an_option_drag_moves_a_smooth_point_without_its_handles() {
        let mut session = smooth_curve_session();
        let smooth = session.points()[3].id;
        session.selection.insert(smooth);
        let positions = |session: &Session| {
            session.points()[2..=4]
                .iter()
                .map(|point| point.point)
                .collect::<Vec<_>>()
        };
        session.begin_point_drag();
        assert!(session.drag_points_to((0.0, 20.0), true));
        assert_eq!(
            positions(&session),
            [
                Point::new(200.0, 300.0),
                Point::new(300.0, 320.0),
                Point::new(400.0, 300.0),
            ],
            "Option leaves both handles in place"
        );
        assert!(session.drag_points_to((0.0, 20.0), false));
        assert_eq!(
            positions(&session),
            [
                Point::new(200.0, 320.0),
                Point::new(300.0, 320.0),
                Point::new(400.0, 320.0),
            ],
            "without Option the handles follow"
        );
        assert!(session.drag_points_to((0.0, 20.0), true));
        assert_eq!(
            positions(&session)[0],
            Point::new(200.0, 300.0),
            "pressing Option mid-drag returns the handles"
        );
        session.end_point_drag();
    }

    #[test]
    fn a_pen_click_on_a_curve_adds_a_smooth_point_with_two_handles() {
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 500.0;
        let mut contour = norad::Contour::default();
        for (x, y, typ) in [
            (0.0, 0.0, norad::PointType::Line),
            (0.0, 200.0, norad::PointType::OffCurve),
            (100.0, 300.0, norad::PointType::OffCurve),
            (300.0, 300.0, norad::PointType::Curve),
            (300.0, 0.0, norad::PointType::Line),
        ] {
            contour
                .points
                .push(norad::ContourPoint::new(x, y, typ, false, None, None));
        }
        glyph.contours.push(contour);
        font.default_layer_mut().insert_glyph(glyph);
        let mut session = Session::new(&font, "A").expect("the glyph is there");
        let curve = kurbo::CubicBez::new((0.0, 0.0), (0.0, 200.0), (100.0, 300.0), (300.0, 300.0));
        let before = kurbo::ParamCurve::eval(&curve, 0.5);
        assert!(session.insert_point_at(before, 4.0));
        let points = session.points();
        assert_eq!(
            points.len(),
            8,
            "a smooth point and a new handle on each side"
        );
        let index = points
            .iter()
            .position(|point| session.selection.contains(&point.id))
            .expect("the new point is selected");
        let added = &points[index];
        assert!(added.on_curve && added.smooth);
        assert!(
            added.point.distance(before) <= 1.0,
            "the curve keeps its shape"
        );
        assert!(!points[index - 1].on_curve && !points[index + 1].on_curve);
        assert!(
            !session.insert_point_at(Point::new(150.0, 100.0), 4.0),
            "empty space is a miss"
        );
    }

    fn session_with(points: &[(f64, f64, norad::PointType)]) -> Session {
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("A");
        glyph.width = 600.0;
        let mut contour = norad::Contour::default();
        for &(x, y, typ) in points {
            contour
                .points
                .push(norad::ContourPoint::new(x, y, typ, false, None, None));
        }
        glyph.contours.push(contour);
        font.default_layer_mut().insert_glyph(glyph);
        Session::new(&font, "A").expect("the glyph is there")
    }

    #[test]
    fn option_click_gives_lines_and_single_control_curves_two_handles() {
        use norad::PointType::{Curve, Line, OffCurve, QCurve};
        let contour = [
            (0.0, 0.0, Line),
            (0.0, 300.0, Line),
            (150.0, 450.0, OffCurve),
            (300.0, 300.0, QCurve),
            (300.0, 0.0, Line),
        ];
        let handles = |session: &Session| {
            let mut found: Vec<_> = session
                .points()
                .iter()
                .filter(|point| session.selection.contains(&point.id))
                .map(|point| (point.point.x, point.point.y, point.on_curve))
                .collect();
            found.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
            found
        };

        // A straight segment gets handles at its thirds.
        let mut session = session_with(&contour);
        assert!(session.add_segment_handles_at(Point::new(0.0, 150.0), 4.0));
        assert_eq!(
            handles(&session),
            [(0.0, 100.0, false), (0.0, 200.0, false)]
        );
        assert_eq!(session.points().len(), 7);

        // A one-control curve keeps its shape: its midpoint stays at (150, 375).
        let mut session = session_with(&contour);
        assert!(session.add_segment_handles_at(Point::new(150.0, 375.0), 4.0));
        assert_eq!(
            handles(&session),
            [(100.0, 400.0, false), (200.0, 400.0, false)]
        );
        let cubic =
            kurbo::CubicBez::new((0.0, 300.0), (100.0, 400.0), (200.0, 400.0), (300.0, 300.0));
        assert_eq!(
            kurbo::ParamCurve::eval(&cubic, 0.5),
            Point::new(150.0, 375.0)
        );
        assert_eq!(session.points().len(), 6);

        // A segment that already has two handles is left alone.
        let mut session = session_with(&[
            (0.0, 300.0, Line),
            (100.0, 400.0, OffCurve),
            (200.0, 400.0, OffCurve),
            (300.0, 300.0, Curve),
            (300.0, 0.0, Line),
        ]);
        assert!(!session.add_segment_handles_at(Point::new(150.0, 375.0), 4.0));
    }

    #[test]
    fn making_a_point_smooth_lines_up_its_handles() {
        use norad::PointType::{Curve, Line, OffCurve};
        let contour = [
            (0.0, 0.0, Line),
            (100.0, 100.0, OffCurve),
            (150.0, 100.0, OffCurve),
            (200.0, 0.0, Curve),
            (250.0, 100.0, OffCurve),
            (300.0, 100.0, OffCurve),
            (400.0, 0.0, Curve),
            (200.0, -200.0, Line),
        ];
        let id_at = |session: &Session, x: f64, y: f64| {
            session
                .points()
                .iter()
                .find(|point| point.point == Point::new(x, y))
                .map(|point| point.id)
                .expect("point exists")
        };
        let at = |session: &Session, index: usize| session.points()[index].point;

        // Handles on both sides turn onto the line that bisects their directions.
        let mut session = session_with(&contour);
        let corner = id_at(&session, 200.0, 0.0);
        assert!(session.toggle_smooth(corner));
        let (before, after) = (at(&session, 2), at(&session, 4));
        assert_eq!((before.y, after.y), (0.0, 0.0), "both handles lie on y = 0");
        assert!(before.x < 200.0 && after.x > 200.0);
        assert!((Point::new(200.0, 0.0).distance(before) - 50_f64.hypot(100.0)).abs() < 1.0);
        assert!((Point::new(200.0, 0.0).distance(after) - 50_f64.hypot(100.0)).abs() < 1.0);

        // Beside a straight segment, the handle continues that segment.
        let mut session = session_with(&contour);
        let joint = id_at(&session, 400.0, 0.0);
        assert!(session.toggle_smooth(joint));
        let handle = at(&session, 5);
        assert!(
            (handle.y - (handle.x - 400.0)).abs() <= 1.0,
            "the handle lies on the line through (200, -200) and (400, 0): {handle:?}"
        );
        assert!(handle.x > 400.0);
    }

    #[test]
    fn tab_and_shift_tab_walk_the_selection_around_the_contour() {
        use masonry::core::keyboard::{Code, KeyboardEvent, Modifiers};
        use norad::PointType::{Curve, Line, OffCurve};
        let mut editor = widget();
        editor.session = session_with(&[
            (0.0, 0.0, Line),
            (0.0, 200.0, OffCurve),
            (100.0, 300.0, OffCurve),
            (300.0, 300.0, Curve),
            (300.0, 0.0, Line),
        ]);
        let ids: Vec<_> = editor
            .session
            .points()
            .iter()
            .map(|point| point.id)
            .collect();
        editor.session.selection = HashSet::from([ids[0]]);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        harness.focus_on(Some(harness.root_id()));
        let tab = |modifiers| {
            TextEvent::Keyboard(KeyboardEvent {
                state: KeyState::Down,
                key: Key::Named(NamedKey::Tab),
                code: Code::Tab,
                modifiers,
                ..KeyboardEvent::default()
            })
        };
        let mut press = |modifiers| {
            harness.process_text_event(tab(modifiers));
            harness.edit_root_widget(|root| root.widget.session.selection.clone())
        };
        // Forward follows the contour, handles included.
        assert_eq!(press(Modifiers::empty()), HashSet::from([ids[1]]));
        assert_eq!(press(Modifiers::empty()), HashSet::from([ids[2]]));
        // Shift-Tab goes back, and wraps past the start of a closed contour.
        assert_eq!(press(Modifiers::SHIFT), HashSet::from([ids[1]]));
        assert_eq!(press(Modifiers::SHIFT), HashSet::from([ids[0]]));
        assert_eq!(press(Modifiers::SHIFT), HashSet::from([ids[4]]));
        assert_eq!(press(Modifiers::empty()), HashSet::from([ids[0]]));
        assert_eq!(harness.focused_widget_id(), Some(harness.root_id()));
    }

    #[test]
    fn a_label_loop_makes_a_sample_and_hovered_ink_glows() {
        // The harness holds no document, so each edit stages from the same base; the
        // painting and cutting gestures are covered by the session's own tests.
        let mut editor = widget();
        editor.tool = Tool::Label;
        let original = projected_glyph(&editor.session);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        let affine = harness.edit_root_widget(|root| root.widget.glyph_affine());
        let screen = |x: f64, y: f64| affine * Point::new(x, y);

        // hovering the square's ink glows and the caption says what a click does
        harness.mouse_move(screen(200.0, 350.0));
        harness.edit_root_widget(|root| {
            assert!(matches!(
                root.widget.label_hover,
                Some(crate::application::editor::tools::label::LabelHit::Piece(_))
            ));
            assert!(
                root.widget
                    .label_caption()
                    .unwrap()
                    .starts_with("Click a sample")
            );
        });

        // a loop around the square, with no sample selected: a new sample
        harness.mouse_move(screen(660.0, 350.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        for step in 1..=16 {
            let turn = f64::from(step) * std::f64::consts::TAU / 16.0;
            harness.mouse_move(screen(
                200.0 + 460.0 * turn.cos(),
                350.0 + 520.0 * turn.sin(),
            ));
        }
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|root| {
            let session = &root.widget.session;
            assert_eq!(session.neural_item().samples.len(), 1);
            assert_eq!(session.label.sample, Some(0), "the new sample is selected");
            assert!(
                session.neural_item().samples[0].boundary.len() <= 16,
                "the loop became a polygon with few corners"
            );
            assert_eq!(
                projected_glyph(session).contours,
                original.contours,
                "labeling never edits the outline"
            );
        });
    }

    #[test]
    fn label_clicks_through_the_workspace_paint_cut_and_keep_the_sample() {
        use crate::application::editor::tools::label::LabelSelection;
        let path = std::path::Path::new("assets/font-sources/neural-fonts/NastaliqDemo.nufo");
        let mut workspace = Workspace::open(path).expect("the demo source opens");
        workspace.open_glyph(workspace.font.index_of("bismillah-001").unwrap());
        workspace.select_tool(Tool::Label);
        // the panel's row click
        Arc::make_mut(&mut workspace.session).select_sample(Some(0));
        let mut editor = widget();
        editor.session = (*workspace.session).clone();
        editor.tool = Tool::Label;
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (1200, 700));
        let dispatch = |harness: &mut TestHarness<EditorWidget>, workspace: &mut Workspace| {
            while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
                harness.edit_root_widget(|root| {
                    dispatch_editor_event(
                        workspace,
                        &mut root.widget.session,
                        event,
                        |app, event| match event {
                            EditorEvent::Edited => app.finish_open_glyph_refresh(),
                            EditorEvent::Selection(count) => app.selected_points = count,
                            _ => {}
                        },
                    );
                });
            }
        };
        let affine = harness.edit_root_widget(|root| root.widget.glyph_affine());
        let screen = |x: f64, y: f64| affine * Point::new(x, y);

        // a click on the ب tooth paints it with the active letter
        harness.mouse_move(screen(1100.0, 450.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        dispatch(&mut harness, &mut workspace);
        assert_eq!(workspace.session.label.error, None);
        assert_eq!(workspace.session.label.sample, Some(0));
        assert!(
            workspace.session.selected_sample().is_some(),
            "the sample stays readable"
        );
        assert!(workspace.session.active_letter().is_some());

        // a drag across the long stroke is a cut
        harness.mouse_move(screen(300.0, 900.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        for y in [700.0, 500.0, 300.0, 100.0, -100.0, -300.0] {
            harness.mouse_move(screen(300.0, y));
        }
        harness.mouse_button_release(Some(PointerButton::Primary));
        dispatch(&mut harness, &mut workspace);
        assert_eq!(workspace.session.label.error, None);
        assert_eq!(workspace.session.neural_item().samples[0].cuts.len(), 1);
        assert_eq!(
            workspace.session.label.selected,
            Some(LabelSelection::Cut(0))
        );
        assert!(workspace.session.selected_sample().is_some());

        // a click inside one of the file's lasso polygons picks it up
        harness.mouse_move(screen(-1280.0, 300.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        dispatch(&mut harness, &mut workspace);
        assert!(matches!(
            workspace.session.label.selected,
            Some(LabelSelection::Region(_))
        ));
        assert_eq!(workspace.session.label.error, None);

        // a short drag that stays on the ink is neither a cut nor a lasso: a hint, no change
        harness.mouse_move(screen(-1480.0, 400.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        for x in [-1460.0, -1440.0, -1420.0] {
            harness.mouse_move(screen(x, 400.0));
        }
        harness.mouse_button_release(Some(PointerButton::Primary));
        dispatch(&mut harness, &mut workspace);
        assert_eq!(workspace.session.neural_item().samples[0].cuts.len(), 1);
    }

    #[test]
    fn lasso_selects_enclosed_point_without_editing_outline() {
        let mut editor = widget();
        editor.tool = Tool::Lasso;
        let original = projected_glyph(&editor.session);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        let (id, point) = harness.edit_root_widget(|root| {
            let (id, point, _, _, _) = root.widget.screen_points()[0];
            (id, point)
        });
        for (index, (x, y)) in [(-14.0, -14.0), (14.0, -14.0), (14.0, 14.0), (-14.0, 14.0)]
            .into_iter()
            .enumerate()
        {
            harness.mouse_move(point + kurbo::Vec2::new(x, y));
            if index == 0 {
                harness.mouse_button_press(Some(PointerButton::Primary));
            }
        }
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|root| {
            assert_eq!(root.widget.session.selection, HashSet::from([id]));
            assert_eq!(projected_glyph(&root.widget.session), original);
        });
    }

    #[test]
    fn metaball_pointer_gesture_adds_live_source_with_one_undo_step() {
        let mut editor = widget();
        editor.tool = Tool::Metaball;
        let original = projected_glyph(&editor.session);
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        harness.mouse_move(Point::new(300.0, 200.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_move(Point::new(330.0, 220.0));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|root| {
            let session = &root.widget.session;
            let source = session.metaball_data().unwrap();
            assert_eq!(source.groups.len(), 1);
            assert_eq!(source.groups[0].balls.len(), 1);
            assert_eq!(
                projected_glyph(session).contours,
                original.contours,
                "live metaballs do not create font points"
            );
            assert!(session.pending_canonical.is_some());
            assert!(!session.gesture_in_progress());
            assert!(!session.metaball_preview.elements().is_empty());
        });
    }

    #[test]
    fn select_tool_moves_an_existing_metaball_without_creating_one() {
        let mut editor = widget();
        editor.tool = Tool::Metaball;
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));

        let center = Point::new(300.0, 200.0);
        harness.mouse_move(center);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        let (id, original) = harness.edit_root_widget(|root| {
            root.widget.tool = Tool::Select;
            root.widget.session.metaballs.selected.clear();
            let source = root.widget.session.metaball_data().unwrap();
            let ball = &source.groups[0].balls[0];
            ((source.groups[0].id, ball.id), Point::new(ball.x, ball.y))
        });

        harness.mouse_move(center);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_move(Point::new(330.0, 220.0));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|root| {
            let source = root.widget.session.metaball_data().unwrap();
            assert_eq!(source.groups.len(), 1);
            assert_eq!(source.groups[0].balls.len(), 1);
            assert!(root.widget.session.metaballs.selected.contains(&id));
            let ball = &source.groups[0].balls[0];
            assert_ne!(Point::new(ball.x, ball.y), original);
        });

        harness.mouse_move(Point::new(20.0, 20.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        harness.edit_root_widget(|root| {
            let source = root.widget.session.metaball_data().unwrap();
            assert_eq!(source.groups.len(), 1);
            assert_eq!(source.groups[0].balls.len(), 1);
            assert!(root.widget.session.metaballs.selected.is_empty());
        });
    }

    #[test]
    fn brush_pointer_event_changes_only_session_ink() {
        let path = std::env::temp_dir().join(format!(
            "runebender-brush-event-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(projected_glyph(&session()));
        font.save(&path).expect("the fixture saves");
        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        workspace.open_glyph(0);
        let original = projected_glyph(&workspace.session);
        let revision = workspace.font.project.document_revision();
        let undo_depth = workspace.metadata_undo.len();
        let mut editor = widget();
        editor.session = (*workspace.session).clone();
        editor.tool = Tool::Sketch;
        editor.sketch = workspace.sketch.clone();
        editor.sketch_source = workspace.font.active();
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        let (from, to) = harness.edit_root_widget(|root| {
            let affine = root.widget.glyph_affine();
            (
                affine * Point::new(220.0, 180.0),
                affine * Point::new(280.0, 240.0),
            )
        });
        harness.mouse_move(from);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_move(to);
        harness.mouse_button_release(Some(PointerButton::Primary));
        let mut sketch_events = 0;
        while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
            harness.edit_root_widget(|root| {
                dispatch_editor_event(
                    &mut workspace,
                    &mut root.widget.session,
                    event,
                    |app, event| {
                        assert!(matches!(event, EditorEvent::SketchChanged));
                        sketch_events += 1;
                        app.note = "Sketch ink changed".into();
                    },
                );
            });
        }
        assert_eq!(sketch_events, 1);
        assert!(workspace.sketch.lock().unwrap().has_ink());
        assert_eq!(projected_glyph(&workspace.session), original);
        assert_eq!(workspace.font.project.document_revision(), revision);
        assert_eq!(workspace.metadata_undo.len(), undo_depth);
        assert!(!workspace.modified);
        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    fn check_metaball_drag_preview(cancel: bool) {
        let path = std::env::temp_dir().join(format!(
            "runebender-metaball-live-proof-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(projected_glyph(&session()));
        font.save(&path).expect("the fixture saves");
        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        workspace.open_glyph(0);
        let original = workspace.session.outline_arc();
        let revision = workspace.font.project.document_revision();
        let undo_depth = workspace.metadata_undo.len();
        let redo_depth = workspace.metadata_redo.len();
        let mut editor = widget();
        editor.session = (*workspace.session).clone();
        editor.tool = Tool::Metaball;
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        let dispatch = |harness: &mut TestHarness<EditorWidget>, workspace: &mut Workspace| {
            let mut edited = 0;
            while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
                harness.edit_root_widget(|root| {
                    dispatch_editor_event(
                        workspace,
                        &mut root.widget.session,
                        event,
                        |app, event| match event {
                            EditorEvent::Edited => {
                                edited += 1;
                                app.finish_open_glyph_refresh();
                            }
                            EditorEvent::Selection(count) => app.selected_points = count,
                            _ => panic!("unexpected metaball gesture event"),
                        },
                    );
                });
            }
            edited
        };

        harness.mouse_move(Point::new(300.0, 200.0));
        harness.mouse_button_press(Some(PointerButton::Primary));
        assert_eq!(dispatch(&mut harness, &mut workspace), 0);
        let mut previous = workspace.session.outline_arc();
        assert_ne!(previous, original, "placing a center updates the proof");
        for at in [Point::new(320.0, 210.0), Point::new(340.0, 230.0)] {
            harness.mouse_move(at);
            assert_eq!(dispatch(&mut harness, &mut workspace), 0);
            let proof = workspace.session.outline_arc();
            assert_ne!(proof, previous, "the proof follows each pointer move");
            harness.edit_root_widget(|root| {
                assert_eq!(proof, root.widget.session.outline_arc());
                assert!(root.widget.session.gesture_in_progress());
                assert!(root.widget.session.pending_canonical.is_none());
            });
            assert_eq!(workspace.font.project.document_revision(), revision);
            assert_eq!(workspace.metadata_undo.len(), undo_depth);
            assert_eq!(workspace.metadata_redo.len(), redo_depth);
            assert!(!workspace.modified);
            previous = proof;
        }

        if cancel {
            harness.process_pointer_event(PointerEvent::Cancel(PRIMARY_MOUSE));
            assert_eq!(dispatch(&mut harness, &mut workspace), 0);
            assert_eq!(workspace.session.outline_arc(), original);
            assert_eq!(workspace.font.project.document_revision(), revision);
            assert_eq!(workspace.metadata_undo.len(), undo_depth);
            assert_eq!(workspace.metadata_redo.len(), redo_depth);
            assert!(!workspace.modified);
        } else {
            harness.mouse_button_release(Some(PointerButton::Primary));
            assert_eq!(dispatch(&mut harness, &mut workspace), 1);
            assert_eq!(workspace.session.outline_arc(), previous);
            assert_eq!(workspace.font.project.document_revision(), revision + 1);
            assert_eq!(workspace.metadata_undo.len(), undo_depth + 1);
            workspace.undo_open_glyph(false);
            assert_eq!(workspace.session.outline_arc(), original);
        }
        assert!(!workspace.session.gesture_in_progress());
        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn metaball_drag_updates_the_proof_before_one_release_commit() {
        check_metaball_drag_preview(false);
    }

    #[test]
    fn cancelled_metaball_drag_restores_the_live_proof_without_committing() {
        check_metaball_drag_preview(true);
    }

    #[test]
    fn an_outline_edit_refreshes_cached_outlines_for_the_glyph_and_its_composites() {
        use kurbo::Shape as _;
        let path = std::env::temp_dir().join(format!(
            "runebender-editor-refresh-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(projected_glyph(&session()));
        let mut composite = norad::Glyph::new("B");
        composite.width = 500.0;
        composite.components.push(norad::Component::new(
            norad::Name::new("A").expect("a valid glyph name"),
            norad::AffineTransform::default(),
            None,
        ));
        font.default_layer_mut().insert_glyph(composite);
        font.save(&path).expect("the fixture saves");
        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let (a, b) = (
            workspace.font.index_of("A").expect("A exists"),
            workspace.font.index_of("B").expect("B exists"),
        );
        let right_edge = |workspace: &Workspace, index: usize| {
            (
                workspace.font.glyphs[index].outline.bounding_box().x1,
                workspace.cells[index].outline.bounding_box().x1,
            )
        };
        assert_eq!(right_edge(&workspace, a), (400.0, 400.0));
        assert_eq!(right_edge(&workspace, b), (400.0, 400.0));

        workspace.open_glyph(a);
        let point = workspace.session.point_id_at(0, 1).unwrap();
        let mut changed = (*workspace.session).clone();
        changed.selection = HashSet::from([point]);
        changed.begin_point_drag();
        assert!(changed.drag_points_to((20.0, 0.0), false));
        changed.end_point_drag();
        dispatch_editor_event(
            &mut workspace,
            &mut changed,
            EditorEvent::Edited,
            |app, _| app.finish_open_glyph_refresh(),
        );
        // The grid, rail and previews read these caches, not the project.
        assert_eq!(right_edge(&workspace, a), (420.0, 420.0));
        assert_eq!(
            right_edge(&workspace, b),
            (420.0, 420.0),
            "a composite using the edited glyph refreshes too"
        );

        // Undo refreshes the same caches back.
        workspace.undo_open_glyph(false);
        assert_eq!(right_edge(&workspace, a), (400.0, 400.0));
        assert_eq!(right_edge(&workspace, b), (400.0, 400.0));
        let _ = std::fs::remove_dir_all(&path);
    }

    #[test]
    fn unchanged_editor_release_skips_the_real_edited_callback_and_retains_redo() {
        let path = std::env::temp_dir().join(format!(
            "runebender-editor-noop-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let session = session();
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(projected_glyph(&session));
        font.save(&path).expect("the fixture saves");
        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        workspace.open_glyph(0);
        let point = workspace.session.point_id_at(0, 0).unwrap();

        let mut changed = (*workspace.session).clone();
        changed.selection.insert(point);
        changed.begin_point_drag();
        assert!(changed.drag_points_to((20.0, 0.0), false));
        changed.end_point_drag();
        dispatch_editor_event(
            &mut workspace,
            &mut changed,
            EditorEvent::Edited,
            |app, _| app.finish_open_glyph_refresh(),
        );
        workspace.undo_open_glyph(false);
        let address = workspace.font.active_layer_address("A").unwrap();
        assert!(workspace.font.project.can_replay_document_layer_history(
            &address,
            runebender::font::history::HistoryDirection::Redo,
        ));

        workspace.modified = false;
        let revision = workspace.font.project.document_revision();
        let undo = workspace.metadata_undo.len();
        let redo = workspace.metadata_redo.len();
        let callbacks = std::cell::Cell::new(0);

        let mut unchanged = (*workspace.session).clone();
        unchanged.selection.insert(point);
        unchanged.begin_point_drag();
        unchanged.end_point_drag();
        dispatch_editor_event(
            &mut workspace,
            &mut unchanged,
            EditorEvent::Edited,
            |app, _| {
                callbacks.set(callbacks.get() + 1);
                app.finish_open_glyph_refresh();
            },
        );

        let mut out_and_back = (*workspace.session).clone();
        out_and_back.selection.insert(point);
        out_and_back.begin_point_drag();
        assert!(out_and_back.drag_points_to((20.0, 0.0), false));
        assert!(out_and_back.drag_points_to((0.0, 0.0), false));
        out_and_back.end_point_drag();
        dispatch_editor_event(
            &mut workspace,
            &mut out_and_back,
            EditorEvent::Edited,
            |app, _| {
                callbacks.set(callbacks.get() + 1);
                app.finish_open_glyph_refresh();
            },
        );

        assert_eq!(callbacks.get(), 0);
        assert!(!workspace.modified);
        assert_eq!(workspace.font.project.document_revision(), revision);
        assert_eq!(workspace.metadata_undo.len(), undo);
        assert_eq!(workspace.metadata_redo.len(), redo);
        assert!(workspace.font.project.can_replay_document_layer_history(
            &address,
            runebender::font::history::HistoryDirection::Redo,
        ));
        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn pointer_cancel_restores_pen_preview_and_preserves_canonical_redo() {
        let path = std::env::temp_dir().join(format!(
            "runebender-editor-pen-cancel-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(projected_glyph(&session()));
        font.save(&path).expect("the fixture saves");
        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        workspace.open_glyph(0);

        let mut pen_session = (*workspace.session).clone();
        pen_session.pen_corner(100.0, 100.0);
        assert_eq!(
            workspace.sync_session_from(&mut pen_session),
            SessionSyncOutcome::Changed
        );
        workspace.finish_open_glyph_refresh();
        let selected = workspace.session.point_id_at(0, 0).unwrap();
        Arc::make_mut(&mut workspace.session)
            .selection
            .insert(selected);
        workspace.apply_op(|session| session.nudge(20.0, 0.0));
        workspace.undo_open_glyph(false);

        workspace.modified = false;
        let baseline = projected_glyph(&workspace.session);
        let selection = workspace.session.selection.clone();
        let revision = workspace.font.project.document_revision();
        let undo = workspace.metadata_undo.len();
        let redo = workspace.metadata_redo.len();
        let address = workspace.font.active_layer_address("A").unwrap();
        assert!(workspace.font.project.can_replay_document_layer_history(
            &address,
            runebender::font::history::HistoryDirection::Redo,
        ));

        let mut editor = widget();
        editor.session = (*workspace.session).clone();
        editor.tool = Tool::Pen;
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        let (from, to) = harness.edit_root_widget(|root| {
            let affine = root.widget.glyph_affine();
            (
                affine * Point::new(220.0, 180.0),
                affine * Point::new(280.0, 240.0),
            )
        });
        harness.mouse_move(from);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_move(to);
        harness.process_pointer_event(PointerEvent::Cancel(PRIMARY_MOUSE));

        let mut events = Vec::new();
        while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
            events.push(event);
        }
        assert!(
            events
                .iter()
                .all(|event| matches!(event, EditorEvent::Selection(_))),
            "a cancelled pen preview never emits Edited"
        );
        for event in events {
            harness.edit_root_widget(|root| {
                dispatch_editor_event(
                    &mut workspace,
                    &mut root.widget.session,
                    event,
                    |app, event| {
                        if let EditorEvent::Selection(count) = event {
                            app.selected_points = count;
                        }
                    },
                );
            });
        }

        harness.edit_root_widget(|root| {
            assert_eq!(projected_glyph(&root.widget.session), baseline);
            assert_eq!(root.widget.session.selection, selection);
            assert!(root.widget.session.pen_is_active());
            assert!(root.widget.session.pending_canonical.is_none());
        });
        assert!(!workspace.modified);
        assert_eq!(workspace.font.project.document_revision(), revision);
        assert_eq!(workspace.metadata_undo.len(), undo);
        assert_eq!(workspace.metadata_redo.len(), redo);
        assert!(workspace.font.project.can_replay_document_layer_history(
            &address,
            runebender::font::history::HistoryDirection::Redo,
        ));

        let next =
            harness.edit_root_widget(|root| root.widget.glyph_affine() * Point::new(320.0, 180.0));
        harness.mouse_move(next);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_button_release(Some(PointerButton::Primary));
        let mut edited = false;
        while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
            harness.edit_root_widget(|root| {
                dispatch_editor_event(
                    &mut workspace,
                    &mut root.widget.session,
                    event,
                    |app, event| match event {
                        EditorEvent::Edited => {
                            edited = true;
                            app.finish_open_glyph_refresh();
                        }
                        EditorEvent::Selection(count) => app.selected_points = count,
                        _ => {}
                    },
                );
            });
        }
        assert!(edited, "the retained pen contour accepts the next point");
        assert_eq!(
            projected_glyph(&workspace.session)
                .contours
                .last()
                .unwrap()
                .points
                .len(),
            2
        );

        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn pointer_cancel_never_applies_shape_or_knife_gestures() {
        for tool in [Tool::Rect, Tool::Ellipse, Tool::Knife] {
            let mut editor = widget();
            editor.tool = tool;
            let selected = editor.session.point_id_at(0, 0).unwrap();
            editor.session.selection.insert(selected);
            let baseline = projected_glyph(&editor.session);
            let mut harness =
                TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
            harness.mouse_move(Point::new(250.0, 150.0));
            harness.mouse_button_press(Some(PointerButton::Primary));
            harness.mouse_move(Point::new(350.0, 250.0));
            harness.process_pointer_event(PointerEvent::Cancel(PRIMARY_MOUSE));

            harness.edit_root_widget(|root| {
                assert_eq!(projected_glyph(&root.widget.session), baseline);
                assert_eq!(root.widget.session.selection, HashSet::from([selected]));
                assert!(root.widget.session.pending_canonical.is_none());
            });
            while let Some((event, _)) = harness.pop_action::<EditorEvent>() {
                assert!(
                    matches!(event, EditorEvent::Selection(1)),
                    "a cancelled {tool:?} gesture never emits Edited"
                );
            }
        }
    }

    #[test]
    fn hand_tool_primary_drag_pans_without_editing() {
        let mut editor = widget();
        editor.tool = Tool::Hand;
        let selection = editor.session.selection.clone();
        let mut harness =
            TestHarness::create_with_size(default_property_set(), editor.prepare(), (600, 400));
        let before = harness.edit_root_widget(|root| root.widget.session.viewport.offset);

        harness.mouse_move(Point::new(240.0, 180.0));
        assert_eq!(harness.cursor_icon(), CursorIcon::Grab);
        harness.mouse_button_press(Some(PointerButton::Primary));
        harness.mouse_move(Point::new(275.0, 205.0));
        assert_eq!(harness.cursor_icon(), CursorIcon::Grabbing);
        harness.mouse_button_release(Some(PointerButton::Primary));

        let (after, after_selection) = harness.edit_root_widget(|root| {
            (
                root.widget.session.viewport.offset,
                root.widget.session.selection.clone(),
            )
        });
        assert_eq!(after - before, kurbo::Vec2::new(35.0, 25.0));
        assert_eq!(
            after_selection, selection,
            "panning does not select or move points"
        );
    }

    /// Time every step of a point drag on a real glyph and print the budget.
    ///
    /// Run by hand with `cargo test --profile fast frame_budget -- --ignored --nocapture`.
    /// `RUNEBENDER_BENCH_GLYPH` picks the glyph, and `RUNEBENDER_SCREENSHOT=1` with
    /// `RUNEBENDER_EDITOR_ZOOM=<zoom>` frames the canvas at a chosen zoom, as a screenshot does.
    #[test]
    #[ignore = "prints frame timings for a point drag on the Virtua Grotesk sources"]
    fn frame_budget_report() {
        use crate::application::platform::screenshot::Headless;
        use crate::application::view::render::app_logic;
        use masonry::core::{PointerButton, PointerButtonEvent, PointerEvent, PointerUpdate};
        use std::time::{Duration, Instant};
        use xilem::view::sized_box;

        fn stats(label: &str, samples: &mut [Duration]) {
            samples.sort();
            let micros = |d: Duration| d.as_secs_f64() * 1e6;
            let median = samples[samples.len() / 2];
            let worst = samples[samples.len() - 1];
            let count = u32::try_from(samples.len()).expect("a few hundred samples");
            let mean = samples.iter().sum::<Duration>() / count;
            println!(
                "{label:<28} median {:>9.1} µs   mean {:>9.1} µs   max {:>9.1} µs   (n={})",
                micros(median),
                micros(mean),
                micros(worst),
                samples.len()
            );
        }

        /// Print what each inspector section costs to build, so a slow one stands out.
        fn section_costs(app: &Workspace, editing: bool) {
            use crate::application::view::panels::{editor_info, sections};
            fn cost<V>(label: &str, build: impl FnOnce() -> V) {
                let start = Instant::now();
                let view = build();
                let elapsed = start.elapsed();
                drop(view);
                println!(
                    "  section {label:<18} {:>8.1} µs",
                    elapsed.as_secs_f64() * 1e6
                );
            }
            println!("inspector sections:");
            cost("layers", || sections::layers_section(app));
            cost("masters", || sections::masters_section(app));
            cost("mark", || sections::mark_section(app));
            if editing {
                cost("coordinates", || sections::coordinates_section(app));
                cost("transformations", || sections::transformations_section(app));
                cost("curves", || sections::curves_section(app));
                cost("path operations", || sections::path_operations_section(app));
                cost("background", || sections::background_section(app));
                cost("shaping", || sections::shaping_section(app));
                cost("related", || editor_info::related_section(app));
                cost("axes", || sections::axes_section(app));
                cost("measure", || sections::measure_section(app));
                cost("interpolation status", || app.interpolation_status());
                cost("preview request", || app.font.preview_font());
            } else {
                cost("font info", || sections::font_info_section(app));
                cost("dimensions", || editor_info::dimensions_section(app));
                cost("font advanced", || sections::font_advanced_section(app));
                cost("kerning", || editor_info::kerning_section(app));
                cost("groups", || editor_info::groups_section(app));
                cost("compare", || editor_info::compare_section(app));
                cost("features", || editor_info::features_section(app));
                cost("filtered cells", || app.filtered_cells());
            }
        }

        let glyph = std::env::var("RUNEBENDER_BENCH_GLYPH").unwrap_or_else(|_| "a".into());
        let sources = std::env::var_os("RUNEBENDER_TEST_FONTS")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../virtua-grotesk/sources")
            });
        let mut app = Workspace::open(&sources.join("VirtuaGrotesk.designspace"))
            .expect("the designspace opens");
        let index = app.font.index_of(&glyph).expect("the glyph exists");
        let logic = |app: &mut Workspace| sized_box(app_logic(app));
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        if std::env::var_os("RUNEBENDER_BENCH_OVERVIEW").is_some() {
            // The overview: the whole glyph grid, repainted when the pointer enters or
            // leaves it.
            let mut headless = Headless::new(app, logic, (1400, 900), 1.0);
            headless.rebuild(logic);
            headless.anim_frame(Duration::from_millis(500));
            let start = Instant::now();
            headless.redraw();
            let first_paint = start.elapsed();
            let grid_center = headless
                .find_widget::<crate::application::view::canvas::grid::GridWidget>()
                .map(|grid| grid.ctx().window_transform() * grid.ctx().border_box().center())
                .expect("the overview has a grid");
            let hover = |at: Point| {
                PointerEvent::Move(PointerUpdate {
                    pointer: PRIMARY_MOUSE,
                    current: PointerState {
                        position: PhysicalPosition::new(at.x, at.y),
                        ..PointerState::default()
                    },
                    coalesced: vec![],
                    predicted: vec![],
                })
            };
            let mut paints = Vec::new();
            let mut clicks = Vec::new();
            for step in 0..20 {
                let at = if step % 2 == 0 {
                    grid_center
                } else {
                    Point::new(2.0, 2.0)
                };
                headless.pointer(hover(at));
                let start = Instant::now();
                headless.redraw();
                paints.push(start.elapsed());
            }
            for step in 0..10 {
                // Selecting a cell repaints the grid.
                let at = grid_center + (f64::from(step) * 40.0 - 200.0, 0.0);
                let state = PointerState {
                    position: PhysicalPosition::new(at.x, at.y),
                    ..PointerState::default()
                };
                headless.pointer(PointerEvent::Down(PointerButtonEvent {
                    pointer: PRIMARY_MOUSE,
                    button: Some(PointerButton::Primary),
                    state: state.clone(),
                }));
                headless.pointer(PointerEvent::Up(PointerButtonEvent {
                    pointer: PRIMARY_MOUSE,
                    button: Some(PointerButton::Primary),
                    state,
                }));
                let start = Instant::now();
                headless.redraw();
                clicks.push(start.elapsed());
            }
            let start = Instant::now();
            headless.rebuild(logic);
            let rebuild = start.elapsed();
            println!("overview: {} glyphs", headless.app.font.glyphs.len());
            let mut opens = Vec::new();
            for name in ["a", "B", "ampersand", "g", "at"] {
                let Some(index) = headless.app.font.index_of(name) else {
                    continue;
                };
                let start = Instant::now();
                headless.app.open_glyph(index);
                let opened = start.elapsed();
                let start = Instant::now();
                headless.rebuild(logic);
                let rebuilt = start.elapsed();
                headless.redraw();
                println!(
                    "  open {name:<10} session {:>7.2} ms   rebuild {:>7.2} ms",
                    ms(opened),
                    ms(rebuilt)
                );
                opens.push(opened + rebuilt);
                headless.app.back_to_overview();
                headless.rebuild(logic);
            }
            stats("open glyph + rebuild + paint", &mut opens);
            let masters = headless.app.font.master_count();
            if masters > 1 {
                let start = Instant::now();
                headless.app.set_master(1);
                headless.rebuild(logic);
                headless.redraw();
                println!("switch master + rebuild      {:.2} ms", ms(start.elapsed()));
                headless.app.set_master(0);
                headless.rebuild(logic);
            }
            section_costs(&headless.app, false);
            println!("overview first paint         {:.2} ms", ms(first_paint));
            stats("overview hover paint pass", &mut paints);
            stats("overview click paint pass", &mut clicks);
            println!("overview rebuild             {:.2} ms", ms(rebuild));
            return;
        }
        app.open_glyph(index);
        let mut headless = Headless::new(app, logic, (1400, 900), 1.0);
        headless.rebuild(logic);
        headless.anim_frame(Duration::from_millis(500));
        headless.redraw();

        let (origin, transform) = {
            let canvas = headless
                .find_widget::<EditorWidget>()
                .expect("the editor canvas is in the tree");
            let transform = canvas.ctx().window_transform();
            let visible = canvas.ctx().border_box().inset(-20.0);
            let widget = canvas.inner();
            let point = widget
                .screen_points()
                .into_iter()
                .find(|(_, at, on_curve, ..)| *on_curve && visible.contains(*at))
                .expect("an on-curve point is on screen");
            println!(
                "glyph {glyph}: {} points, zoom {:.2}",
                widget.session.points().len(),
                widget.session.viewport.zoom
            );
            (transform * point.1, transform)
        };
        let _ = transform;
        let state_at = |at: Point, down: bool| {
            let mut buttons = masonry::ui_events::pointer::PointerButtons::default();
            if down {
                buttons.insert(PointerButton::Primary);
            }
            PointerState {
                position: PhysicalPosition::new(at.x, at.y),
                buttons,
                ..PointerState::default()
            }
        };

        // No change: how long a rebuild and a paint take when nothing happened.
        let mut idle_rebuild = Vec::new();
        let mut idle_paint = Vec::new();
        for _ in 0..20 {
            let start = Instant::now();
            headless.rebuild(logic);
            idle_rebuild.push(start.elapsed());
            let start = Instant::now();
            headless.redraw();
            idle_paint.push(start.elapsed());
        }

        let start = Instant::now();
        headless.pointer(PointerEvent::Move(PointerUpdate {
            pointer: PRIMARY_MOUSE,
            current: state_at(origin, false),
            coalesced: vec![],
            predicted: vec![],
        }));
        headless.pointer(PointerEvent::Down(PointerButtonEvent {
            pointer: PRIMARY_MOUSE,
            button: Some(PointerButton::Primary),
            state: state_at(origin, true),
        }));
        let press = start.elapsed();

        let moves = 240;
        let mut move_handling = Vec::with_capacity(moves);
        let mut move_paint = Vec::with_capacity(moves);
        for step in 1..=moves {
            let at = origin + (step as f64 * 0.5, (step as f64 * 0.25).sin() * 3.0);
            let start = Instant::now();
            headless.pointer(PointerEvent::Move(PointerUpdate {
                pointer: PRIMARY_MOUSE,
                current: state_at(at, true),
                coalesced: vec![],
                predicted: vec![],
            }));
            move_handling.push(start.elapsed());
            let start = Instant::now();
            headless.redraw();
            move_paint.push(start.elapsed());
        }

        let end = origin + (moves as f64 * 0.5, 0.0);
        let start = Instant::now();
        headless.pointer(PointerEvent::Up(PointerButtonEvent {
            pointer: PRIMARY_MOUSE,
            button: Some(PointerButton::Primary),
            state: state_at(end, false),
        }));
        let release = start.elapsed();

        // The window delivers the widget's actions to the app; do that by hand.
        let mut session = headless
            .find_widget::<EditorWidget>()
            .expect("the editor canvas is in the tree")
            .inner()
            .session
            .clone();
        let start = Instant::now();
        let outcome = headless.app.sync_session_from(&mut session);
        let sync = start.elapsed();
        assert_eq!(
            outcome,
            SessionSyncOutcome::Changed,
            "the drag moved the point"
        );
        let start = Instant::now();
        headless.app.finish_open_glyph_refresh();
        let refresh = start.elapsed();
        let start = Instant::now();
        headless.rebuild(logic);
        let rebuild = start.elapsed();
        let start = Instant::now();
        headless.redraw();
        let paint = start.elapsed();
        let start = Instant::now();
        headless.rebuild(logic);
        let settled = start.elapsed();
        let start = Instant::now();
        headless.app.undo_open_glyph(false);
        let undo = start.elapsed();
        let start = Instant::now();
        headless.rebuild(logic);
        let undo_rebuild = start.elapsed();
        let start = Instant::now();
        let snapshot = headless.app.font.project.babelfont_snapshot();
        let snapshot_time = start.elapsed();
        assert!(snapshot.is_ok());
        // Tests compile the preview synchronously inside the rebuild; the window does it on
        // a thread. Its cost is reported so the rebuild figure above can be read without it.
        let start = Instant::now();
        assert!(headless.app.font.project.compile().is_ok());
        let compile = start.elapsed();

        section_costs(&headless.app, true);
        if !headless.app.preview_text.is_empty() {
            use crate::application::editor::tools::text::{TextInputs, TextState};
            let app = &headless.app;
            let start = Instant::now();
            let inputs = TextInputs::new(&app.font);
            let new = start.elapsed();
            let start = Instant::now();
            let inputs = inputs.with_location(&app.font, &app.axis_values);
            let location = start.elapsed();
            let start = Instant::now();
            let placed = TextState::new(&inputs.with_text(&app.preview_text)).placed();
            let shaped = start.elapsed();
            println!();
            println!("proof text inputs            {:.2} ms", ms(new));
            println!("proof text location          {:.2} ms", ms(location));
            println!(
                "proof text shape and place   {:.2} ms ({} sorts)",
                ms(shaped),
                placed.len()
            );
            if let Ok(Some(compiled)) = app.font.preview_font() {
                let start = Instant::now();
                let font =
                    runebender::text::shape::ShapingFont::from_bytes((*compiled.bytes).clone());
                let parsed = start.elapsed();
                assert!(font.is_ok());
                println!("proof shaping font parse     {:.2} ms", ms(parsed));
            }
        }

        println!();
        stats("idle rebuild", &mut idle_rebuild);
        stats("idle paint pass", &mut idle_paint);
        stats("drag move handling", &mut move_handling);
        stats("drag move paint pass", &mut move_paint);
        println!("press                        {:.2} ms", ms(press));
        println!("release (widget)             {:.2} ms", ms(release));
        println!("release sync_session_from    {:.2} ms", ms(sync));
        println!("release glyph refresh        {:.2} ms", ms(refresh));
        println!("release rebuild              {:.2} ms", ms(rebuild));
        println!("release paint pass           {:.2} ms", ms(paint));
        println!("next rebuild                 {:.2} ms", ms(settled));
        println!("undo                         {:.2} ms", ms(undo));
        println!("undo rebuild                 {:.2} ms", ms(undo_rebuild));
        println!("font snapshot for preview    {:.2} ms", ms(snapshot_time));
        println!("preview compile (threaded)   {:.2} ms", ms(compile));
    }
}

#[cfg(test)]
mod wedge_check {
    #[test]
    fn a_wedge_fills_half_a_circle() {
        use kurbo::Shape as _;
        let center = kurbo::Point::new(0.0, 0.0);
        let radius = 10.0;
        let step = std::f64::consts::PI;
        let start = -std::f64::consts::FRAC_PI_2;
        let arc = kurbo::Arc::new(center, (radius, radius), start, step, 0.0);
        let mut wedge = kurbo::BezPath::new();
        wedge.move_to(center);
        wedge.line_to(center + kurbo::Vec2::new(start.cos(), start.sin()) * radius);
        wedge.extend(arc.append_iter(0.1));
        wedge.close_path();
        let half = std::f64::consts::PI * radius * radius / 2.0;
        assert!(
            (wedge.area().abs() - half).abs() < half * 0.02,
            "{}",
            wedge.area()
        );
    }
}

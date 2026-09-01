// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The editor's state: the `Workspace` struct and the types it is made of.

use crate::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sort {
    Name,
    Unicode,
}

/// The active sidebar selection: a category chip, a language group, or a
/// builtin/GF-coverage filter (mirrors runebender-gpui's `SidebarFilter`).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Sel {
    Category(GlyphCategory),
    Language(usize),
    Filter(usize),
}

/// The active editor tool.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Tool {
    Select,
    Pen,
    Rect,
    Ellipse,
    HyperPen,
    Knife,
    Measure,
    /// Type glyphs into a line and edit them in context: the web
    /// editor's text tool, on runebender-core's text engine.
    Text,
}

pub(crate) struct Workspace {
    pub(crate) font: FontModel,
    pub(crate) palette: Arc<Palette>,
    pub(crate) cells: Arc<Vec<Cell>>,
    pub(crate) mode: Mode,
    pub(crate) selected: Option<usize>,
    pub(crate) multi_selected: Arc<std::collections::HashSet<usize>>,
    pub(crate) filter: String,
    /// The grid's Detail view: cells carry their category and advance.
    pub(crate) detail: bool,
    /// Which tab the editor's left rail is showing.
    pub(crate) rail: Rail,
    /// Writing direction for the text tool, or `None` for automatic.
    /// The chips that set it are in the title bar, which is why this is
    /// application state and not the buffer's.
    pub(crate) text_dir: Option<runebender_core::text::buffer::TextDirection>,
    /// Whether the left column is folded away, as the GPUI build's
    /// grid-icon button in the title bar does it.
    pub(crate) left_collapsed: bool,
    /// Sidebar groups that are folded shut, by title. The GPUI build's
    /// sidebar folds, and a font with four filter groups needs it.
    pub(crate) collapsed: std::collections::HashSet<&'static str>,
    pub(crate) sel: Sel,
    pub(crate) sort: Sort,
    // Editor session, when a glyph is open. This is the live one: the
    // active tab's copy is only written back when tabs change.
    pub(crate) session: Arc<Session>,
    /// One parked session per open tab, in strip order. Each carries its
    /// own selection, viewport and undo stack, and it tracks its glyph by
    /// name, so a rename or a master switch does not lose it.
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active_tab: usize,
    pub(crate) selected_points: usize,
    pub(crate) tool: Tool,
    pub(crate) modified: bool,
    pub(crate) note: String,
    /// Which analysis overlays the editor draws.
    pub(crate) view: canvas::editor::ViewOptions,
    /// What the text tool starts with, from `RUNEBENDER_TEXT`.
    pub(crate) initial_text: String,
    /// Grid cell size, driven by the bottom bar's zoom.
    pub(crate) cell_size: f64,
    pub(crate) advance_buf: String,
    pub(crate) lsb_buf: String,
    pub(crate) rsb_buf: String,
    pub(crate) name_buf: String,
    pub(crate) unicode_buf: String,
    /// Kerning group names for the open glyph, left side then right.
    pub(crate) kern1_buf: String,
    pub(crate) kern2_buf: String,
    /// Copied contours. An in-app clipboard, as in the GPUI build: the
    /// system clipboard carries text, not outlines.
    pub(crate) clipboard: Vec<norad::Contour>,
    /// Draw the UFO background layer under the outline.
    pub(crate) show_background: bool,
    /// A glyph name to show behind the drawing, empty for none.
    pub(crate) reference_buf: String,
    /// Current axis location in user units, one per designspace axis.
    pub(crate) axis_values: Vec<f64>,
    /// Active OKLCH theme id (dark | midnight | gray | light).
    pub(crate) theme_id: &'static str,
    /// Reference corner for the Coordinates fields (the 9-point picker).
    pub(crate) coord_quadrant: runebender_core::outline::path::Quadrant,
    pub(crate) coord_x_buf: String,
    pub(crate) coord_y_buf: String,
    /// Search scope: 0 name and unicode, 1 name only, 2 unicode only.
    pub(crate) search_mode: u8,
    /// Case-sensitive search.
    pub(crate) search_case: bool,
    /// Masters drawn as ghost outlines under the active one. The Layers
    /// section toggles these, one per thumbnail click (gpui's eye).
    pub(crate) reference_layers: std::collections::HashSet<usize>,
}

/// Which surface is showing.
pub(crate) enum Mode {
    /// The glyph grid.
    Overview,
    /// The editor, on the glyph at this index.
    Editor(usize),
}

/// One editing tab: a parked session and the tool it was left on.
pub(crate) struct Tab {
    pub(crate) session: Arc<Session>,
    pub(crate) tool: Tool,
}

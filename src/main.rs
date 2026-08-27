// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender on xix. A font editor: glyph grid, glyph editor, sidebar.
//! See PORT.md for what each slice forced into the framework.

mod design;
mod menu;
mod screenshot;
mod editor;
mod grid;
mod icon_button;
mod model;
mod session;
mod shortcuts;
mod text_label;
mod theme;
mod ui;

use std::path::Path as FsPath;
use std::sync::Arc;

use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use masonry::properties::types::CrossAxisAlignment;
use masonry::theme::default_property_set;
use winit::dpi::LogicalSize;
use winit::error::EventLoopError;
use xilem::style::Style;
use crate::design::{column as xcolumn, row as xrow};
use xilem::view::{
    FlexExt as _, FlexSpacer, button, canvas, flex_col, flex_row, label, portal, sized_box,
    slider, text_button, text_input,
};
use xilem::{EventLoop, EventLoopBuilder, WidgetView, WindowOptions, Xilem};

use editor::editor;
use grid::{Cell, CellMetrics, GridEvent, cells_of, grid};
use icon_button::icon_button;
use model::FontModel;
use runebender_core::category::GlyphCategory;
use session::Session;
use theme::Palette;
use design::{ControlSize, Radius, Region, Space, Stroke, TextSize};
use ui::section_header;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Name,
    Unicode,
}

/// The active sidebar selection: a category chip, a language group, or a
/// builtin/GF-coverage filter (mirrors runebender-gpui's SidebarFilter).
#[derive(Clone, Copy, PartialEq)]
enum Sel {
    Category(GlyphCategory),
    Language(usize),
    Filter(usize),
}

/// The active editor tool.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    Pen,
    Rect,
    Ellipse,
    HyperPen,
    Knife,
    Measure,
}

/// Which surface is showing.
enum Mode {
    /// The glyph grid.
    Overview,
    /// The editor, on the glyph at this index.
    Editor(usize),
}

/// One editing tab: a parked session and the tool it was left on.
pub struct Tab {
    session: Arc<Session>,
    tool: Tool,
}

pub struct App {
    font: FontModel,
    palette: Arc<Palette>,
    cells: Arc<Vec<Cell>>,
    mode: Mode,
    selected: Option<usize>,
    multi_selected: std::sync::Arc<std::collections::HashSet<usize>>,
    filter: String,
    sel: Sel,
    sort: Sort,
    // Editor session, when a glyph is open. This is the live one: the
    // active tab's copy is only written back when tabs change.
    session: Arc<Session>,
    /// One parked session per open tab, in strip order. Each carries its
    /// own selection, viewport and undo stack, and it tracks its glyph by
    /// name, so a rename or a master switch does not lose it.
    tabs: Vec<Tab>,
    active_tab: usize,
    selected_points: usize,
    tool: Tool,
    modified: bool,
    note: String,
    /// Which analysis overlays the editor draws.
    view: editor::ViewOptions,
    advance_buf: String,
    lsb_buf: String,
    rsb_buf: String,
    name_buf: String,
    unicode_buf: String,
    /// Kerning group names for the open glyph, left side then right.
    kern1_buf: String,
    kern2_buf: String,
    /// Copied contours. An in-app clipboard, as in the GPUI build: the
    /// system clipboard carries text, not outlines.
    clipboard: Vec<norad::Contour>,
    /// Draw the UFO background layer under the outline.
    show_background: bool,
    /// A glyph name to show behind the drawing, empty for none.
    reference_buf: String,
    /// Current axis location in user units, one per designspace axis.
    axis_values: Vec<f64>,
    /// Active OKLCH theme id (dark | midnight | gray | light).
    theme_id: &'static str,
    /// Reference corner for the Coordinates fields (the 9-point picker).
    coord_quadrant: runebender_core::path::Quadrant,
    coord_x_buf: String,
    coord_y_buf: String,
    /// Search scope: 0 name and unicode, 1 name only, 2 unicode only.
    search_mode: u8,
    /// Case-sensitive search.
    search_case: bool,
    /// Masters drawn as ghost outlines under the active one. The Layers
    /// section toggles these, one per thumbnail click (gpui's eye).
    reference_layers: std::collections::HashSet<usize>,
}

impl App {
    fn open(path: &FsPath) -> Result<Self, String> {
        let font = FontModel::open(path)?;
        let theme_id: &'static str = match std::env::var("RUNEBENDER_THEME").ok().as_deref() {
            Some("midnight") => "midnight",
            Some("gray") => "gray",
            Some("light") => "light",
            _ => "dark",
        };
        let palette = Arc::new(Palette::load(theme_id));
        let cells = Arc::new(cells_of(&font, &palette));
        let first = font
            .index_of("A")
            .or_else(|| font.index_of("a"))
            .or(if font.glyphs.is_empty() { None } else { Some(0) })
            .ok_or_else(|| "font has no glyphs".to_string())?;
        let session = Arc::new(
            Session::new(&font.font, &font.glyphs[first].name).ok_or("glyph missing")?,
        );
        // For headless screenshots: optionally select all points.
        // (set later, after session is final)
        
        let start_cat = std::env::var("RUNEBENDER_CAT").ok();
        let (mode, open) = match std::env::var("RUNEBENDER_OPEN").ok().and_then(|n| font.index_of(&n)) {
            Some(i) => (Mode::Editor(i), Some(i)),
            None => (Mode::Overview, None),
        };
        let session = match open {
            Some(i) => Arc::new(
                Session::new(&font.font, &font.glyphs[i].name).unwrap_or_else(|| (*session).clone()),
            ),
            None => session,
        };
        // Snap sliders to the active master's location (Glyphs behavior), so
        // opening a master shows no interpolation overlay until you move one.
        let mut axis_values: Vec<f64> = if font.axes.is_empty() {
            Vec::new()
        } else {
            font.master_axis_values(font.active)
        };
        // Headless overrides, so a render can show a state that normally
        // takes clicks to reach. The GPUI build has the same idea.
        let reference_buf = std::env::var("RUNEBENDER_REFERENCE").unwrap_or_default();
        let show_background = std::env::var("RUNEBENDER_BACKGROUND").is_ok();
        // RUNEBENDER_VIEW=comb,continuity,colorize,handles,segments,bearings
        let mut view = editor::ViewOptions::default();
        if let Ok(spec) = std::env::var("RUNEBENDER_VIEW") {
            for name in spec.split(',').map(str::trim) {
                match name {
                    "comb" => view.comb = true,
                    "continuity" => view.continuity = true,
                    "colorize" => view.colorize = true,
                    "handles" => view.handles = true,
                    "segments" => view.segments = true,
                    "bearings" => view.bearings = true,
                    "popcount" => view.popcount = true,
                    _ => {}
                }
            }
        }
        // Headless override: RUNEBENDER_AXIS="wght=500,wdth=80".
        if let Ok(spec) = std::env::var("RUNEBENDER_AXIS") {
            for pair in spec.split(',') {
                if let Some((tag, val)) = pair.split_once('=') {
                    if let Ok(v) = val.trim().parse::<f64>() {
                        if let Some(i) = font.axes.iter().position(|a| a.tag == tag.trim() || a.name == tag.trim()) {
                            axis_values[i] = v.clamp(font.axes[i].min, font.axes[i].max);
                        }
                    }
                }
            }
        }
        // Seed the Name/Unicode fields from the glyph actually shown
        // (the opened one in editor mode, else the first).
        let shown = open.unwrap_or(first);
        let first_name = font.glyphs[shown].name.clone();
        let (kern1, kern2) = (
            font.kern_group(&first_name, true),
            font.kern_group(&first_name, false),
        );
        let first_uni = font.glyphs[shown]
            .codepoint
            .map(|c| format!("{:04X}", c as u32))
            .unwrap_or_default();
        Ok(Self {
            font,
            palette,
            cells,
            mode,
            selected: Some(open.unwrap_or(first)),
            multi_selected: std::sync::Arc::new(std::collections::HashSet::new()),
            filter: String::new(),
            sel: Sel::Category(match start_cat.as_deref() {
                Some("Number") => GlyphCategory::Number,
                Some("Symbol") => GlyphCategory::Symbol,
                Some("Mark") => GlyphCategory::Mark,
                _ => GlyphCategory::All,
            }),
            sort: Sort::Name,
            advance_buf: format!("{}", session.advance() as i64),
            lsb_buf: metric_bufs(&session).0,
            rsb_buf: metric_bufs(&session).1,
            kern1_buf: kern1,
            kern2_buf: kern2,
            clipboard: Vec::new(),
            show_background,
            reference_buf,
            name_buf: first_name,
            unicode_buf: first_uni,
            tabs: vec![Tab {
                session: session.clone(),
                tool: Tool::Select,
            }],
            active_tab: 0,
            session,
            selected_points: 0,
            tool: match std::env::var("RUNEBENDER_TOOL").as_deref() {
                Ok("measure") => Tool::Measure,
                _ => Tool::Select,
            },
            modified: false,
            note: String::new(),
            view,
            axis_values,
            theme_id,
            coord_quadrant: runebender_core::path::Quadrant::Center,
            coord_x_buf: String::new(),
            coord_y_buf: String::new(),
            search_mode: 0,
            search_case: false,
            reference_layers: std::collections::HashSet::new(),
        })
    }

    /// The cells that pass the current search + category filter. The two
    /// toggles beside the search box set the scope (name, unicode, both)
    /// and whether the match is case-sensitive.
    fn filtered_cells(&self) -> Arc<Vec<Cell>> {
        let q = if self.search_case {
            self.filter.clone()
        } else {
            self.filter.to_lowercase()
        };
        let by_name = self.search_mode != 2;
        let by_unicode = self.search_mode != 1;
        let out: Vec<Cell> = self
            .cells
            .iter()
            .filter(|c| {
                let cat_ok = self.cell_matches_sel(c.index);
                let name_hit = by_name
                    && if self.search_case {
                        c.name.contains(&q)
                    } else {
                        c.name.to_lowercase().contains(&q)
                    };
                let uni_hit = by_unicode
                    && c
                        .codepoint
                        .map(|cp| format!("{:04x}", cp as u32).contains(q.to_lowercase().trim_start_matches("u+").trim_start_matches("0x")))
                        .unwrap_or(false);
                let q_ok = q.is_empty() || name_hit || uni_hit;
                cat_ok && q_ok
            })
            .cloned()
            .collect();
        let mut out = out;
        match self.sort {
            Sort::Name => {}
            Sort::Unicode => out.sort_by_key(|c| c.codepoint.map(|cp| cp as u32).unwrap_or(u32::MAX)),
        }
        Arc::new(out)
    }

    /// Codepoints of a glyph entry (the cache keeps only the first).
    fn entry_codepoints(entry: &model::GlyphEntry) -> Vec<u32> {
        entry.codepoint.map(|c| vec![c as u32]).unwrap_or_default()
    }

    /// Does the glyph at `index` pass the active sidebar selection?
    fn cell_matches_sel(&self, index: usize) -> bool {
        use runebender_core::sidebar as sb;
        let entry = &self.font.glyphs[index];
        match self.sel {
            Sel::Category(GlyphCategory::All) => true,
            Sel::Category(cat) => entry.category == cat,
            Sel::Language(i) => sb::language_groups()
                .get(i)
                .map(|g| sb::glyph_matches_language_group(&entry.name, &Self::entry_codepoints(entry), g))
                .unwrap_or(false),
            Sel::Filter(i) => sb::builtin_filters()
                .get(i)
                .and_then(|b| b.glyphset.as_ref())
                .map(|f| sb::glyph_matches_character_filter(&entry.name, &Self::entry_codepoints(entry), f))
                .unwrap_or(false),
        }
    }

    /// How many glyphs in the font match language group `i`.
    fn language_count(&self, i: usize) -> usize {
        use runebender_core::sidebar as sb;
        let Some(g) = sb::language_groups().get(i) else { return 0 };
        self.font
            .glyphs
            .iter()
            .filter(|e| sb::glyph_matches_language_group(&e.name, &Self::entry_codepoints(e), g))
            .count()
    }

    /// Present-count for GF-coverage filter `i` (glyphs the font has).
    fn filter_present(&self, i: usize) -> usize {
        use runebender_core::sidebar as sb;
        let Some(f) = sb::builtin_filters().get(i).and_then(|b| b.glyphset.as_ref()) else { return 0 };
        self.font
            .glyphs
            .iter()
            .filter(|e| sb::glyph_matches_character_filter(&e.name, &Self::entry_codepoints(e), f))
            .count()
    }

    fn category_count(&self, cat: GlyphCategory) -> usize {
        if cat == GlyphCategory::All {
            self.font.glyphs.len()
        } else {
            self.font.glyphs.iter().filter(|g| g.category == cat).count()
        }
    }

    fn cell_metrics(&self, cell: f64) -> CellMetrics {
        CellMetrics {
            cell,
            ascender: self.font.ascender,
            descender: self.font.descender,
            upm: self.font.units_per_em,
        }
    }

    fn new_glyph(&mut self) {
        let name = self.filter.trim().to_string();
        let upm = self.font.units_per_em;
        if self.font.add_glyph(&name, (upm * 0.5).round()) {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.filter.clear();
            if let Some(i) = self.font.index_of(&name) {
                self.open_glyph(i);
            }
            self.modified = true;
        }
    }

    fn grid_select(&mut self, index: usize, cmd: bool, shift: bool) {
        use std::collections::HashSet;
        if cmd {
            let mut m: HashSet<usize> = (*self.multi_selected).clone();
            if !m.remove(&index) {
                m.insert(index);
            }
            self.multi_selected = std::sync::Arc::new(m);
        } else if shift {
            // Range from the current single selection to this index, in cell order.
            let cells = self.filtered_cells();
            let order: Vec<usize> = cells.iter().map(|c| c.index).collect();
            let a = self.selected.and_then(|s| order.iter().position(|&i| i == s));
            let b = order.iter().position(|&i| i == index);
            if let (Some(a), Some(b)) = (a, b) {
                let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                let m: HashSet<usize> = order[lo..=hi].iter().copied().collect();
                self.multi_selected = std::sync::Arc::new(m);
            }
        } else {
            self.multi_selected = std::sync::Arc::new(HashSet::new());
        }
        self.selected = Some(index);
    }

    /// Write the live session back into its tab, so switching away from
    /// it does not lose the edit, the selection, or the undo stack.
    fn park(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.session = self.session.clone();
            tab.tool = self.tool;
        }
    }

    /// Make a tab the live one.
    fn activate_tab(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(index) else { return };
        let (session, tool) = (tab.session.clone(), tab.tool);
        self.park();
        self.active_tab = index;
        self.session = session;
        self.tool = tool;
        let name = self.session.glyph_name.clone();
        if let Some(glyph) = self.font.index_of(&name) {
            self.selected = Some(glyph);
            self.mode = Mode::Editor(glyph);
        }
        self.selected_points = 0;
        self.refresh_metric_bufs();
        self.refresh_coord_bufs();
        self.name_buf = name;
        self.unicode_buf = self
            .selected
            .and_then(|i| self.font.glyphs.get(i))
            .and_then(|g| g.codepoint)
            .map(|c| format!("{:04X}", c as u32))
            .unwrap_or_default();
    }

    /// A second tab on the glyph that is open, with its own session.
    fn new_tab(&mut self) {
        let Some(session) = Session::new(&self.font.font, &self.session.glyph_name) else {
            return;
        };
        self.park();
        self.tabs.push(Tab {
            session: Arc::new(session),
            tool: self.tool,
        });
        self.activate_tab(self.tabs.len() - 1);
    }

    /// Close a tab. Closing the last one leaves the editor.
    fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            self.back_to_overview();
            return;
        }
        self.park();
        self.tabs.remove(index);
        let next = if self.active_tab > index {
            self.active_tab - 1
        } else {
            self.active_tab.min(self.tabs.len() - 1)
        };
        self.active_tab = usize::MAX; // parking again would write to the wrong tab
        self.activate_tab(next);
    }

    fn open_glyph(&mut self, index: usize) {
        // A glyph that already has a tab gets that tab, rather than a
        // second one on the same glyph.
        if let Some(entry) = self.font.glyphs.get(index) {
            let name = entry.name.clone();
            if let Some(existing) = self
                .tabs
                .iter()
                .position(|tab| tab.session.glyph_name == name)
            {
                self.activate_tab(existing);
                return;
            }
        }
        if let Some(entry) = self.font.glyphs.get(index) {
            if let Some(session) = Session::new(&self.font.font, &entry.name) {
                self.advance_buf = format!("{}", session.advance() as i64);
                let (l, r) = metric_bufs(&session);
                self.lsb_buf = l;
                self.rsb_buf = r;
                self.name_buf = entry.name.clone();
                self.unicode_buf = entry.codepoint.map(|c| format!("{:04X}", c as u32)).unwrap_or_default();
                self.session = Arc::new(session);
                if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                    tab.session = self.session.clone();
                } else {
                    self.tabs.push(Tab {
                        session: self.session.clone(),
                        tool: self.tool,
                    });
                    self.active_tab = self.tabs.len() - 1;
                }
                self.selected = Some(index);
                self.selected_points = 0;
                self.mode = Mode::Editor(index);
            }
        }
    }

    /// After an edit, pull the glyph back out of the session and refresh
    /// the model + grid cache so the overview preview matches.
    /// Replace the app's session with the island's live one (called on every
    /// editor event so save/preview see interactive edits).
    pub fn sync_session_from(&mut self, session: &Session) {
        self.session = Arc::new(session.clone());
        // Keep the panel's advance field in step after canvas edits
        // (sidebearing/advance drags). This path is never hit by typing in
        // the field, so it does not clobber input.
        self.refresh_metric_bufs();
        self.selected_points = self.session.selection.len();
    }

    /// The OKLCH themes in menu order (matches runebender-gpui).
    const THEMES: [&'static str; 4] = ["dark", "midnight", "gray", "light"];

    /// Advance to the next theme, reloading the palette and the baked cell
    /// colors. Exercises the design-token kernel: one id swaps every role.
    fn cycle_theme(&mut self) {
        let i = Self::THEMES.iter().position(|t| *t == self.theme_id).unwrap_or(0);
        self.theme_id = Self::THEMES[(i + 1) % Self::THEMES.len()];
        self.palette = Arc::new(Palette::load(self.theme_id));
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
    }

    fn set_master(&mut self, index: usize) {
        if index == self.font.active {
            return;
        }
        self.font.set_active(index);
        self.axis_values = self.font.master_axis_values(index);
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        // Reopen the current glyph in the new master, keeping the viewport.
        if let Mode::Editor(i) = self.mode {
            if let Some(entry) = self.font.glyphs.get(i) {
                if let Some(sess) = Session::new(&self.font.font, &entry.name) {
                    self.session = Arc::new(sess);
                }
            } else if let Some(idx) = self
                .selected
                .and_then(|_| self.font.index_of(&self.session.glyph_name))
            {
                self.mode = Mode::Editor(idx);
                if let Some(sess) = Session::new(&self.font.font, &self.session.glyph_name.clone()) {
                    self.session = Arc::new(sess);
                }
            }
        }
    }

    /// The current axis location as a name->value map (user units).
    fn axis_location(&self) -> std::collections::HashMap<String, f64> {
        self.font
            .axes
            .iter()
            .zip(&self.axis_values)
            .map(|(a, v)| (a.name.clone(), *v))
            .collect()
    }

    /// True when the sliders sit exactly on the active master's location.
    fn on_active_master(&self) -> bool {
        match self.font.master_locations.get(self.font.active) {
            Some(m) => self.font.axes.iter().enumerate().all(|(i, a)| {
                // axis_values are user coords; master locations are design coords.
                let cur = a.user_to_design(self.axis_values.get(i).copied().unwrap_or(a.default));
                let mst = m.get(&a.name).copied().unwrap_or_else(|| a.user_to_design(a.default));
                (cur - mst).abs() < 1e-6
            }),
            None => true,
        }
    }

    /// The interpolated instance outline at the current axis location, shown as
    /// a read-only overlay. `None` on a master (the editable outline is enough)
    /// or when the glyph is not interpolatable.
    fn interp_preview(&self) -> Option<Arc<masonry::kurbo::BezPath>> {
        if self.on_active_master() {
            return None;
        }
        self.font
            .interpolate_outline(&self.session.glyph_name, &self.axis_location())
            .map(Arc::new)
    }

    fn set_axis(&mut self, index: usize, value: f64) {
        if let Some(v) = self.axis_values.get_mut(index) {
            *v = value;
        }
    }

    fn refresh_open_glyph(&mut self) {
        if let Mode::Editor(index) = self.mode {
            let glyph = self.session.glyph.clone();
            self.font.replace_glyph(index, glyph);
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.modified = true;
            self.note.clear();
        }
    }

    fn save(&mut self) {
        self.refresh_open_glyph();
        match self.font.save() {
            Ok(()) => {
                self.modified = false;
                self.note = format!("Saved {}", self.font.source.display());
            }
            Err(e) => self.note = format!("Save failed: {e}"),
        }
    }

    pub fn dispatch(&mut self, action: shortcuts::AppAction) {
        use shortcuts::AppAction as A;
        match action {
            A::Save => self.save(),
            A::Overview => {
                if matches!(self.mode, Mode::Editor(_)) {
                    self.back_to_overview();
                }
            }
            A::Tool(t) => {
                self.tool = t;
                // Picking Measure turns on what the tool is for, keeping
                // whatever curve analyses were already showing.
                if t == Tool::Measure && !self.view.measures() {
                    let measuring = editor::ViewOptions::measuring();
                    self.view = editor::ViewOptions {
                        comb: self.view.comb,
                        continuity: self.view.continuity,
                        ..measuring
                    };
                }
            }
            A::FlipHorizontal => self.apply_op(|s| s.flip_horizontal()),
            A::FlipVertical => self.apply_op(|s| s.flip_vertical()),
            A::Rotate90 => self.apply_op(|s| s.rotate_90()),
            A::RemoveOverlap => self.apply_op(|s| s.remove_overlap()),
            A::Decompose => self.apply_op(|s| s.decompose()),
            A::Duplicate => self.apply_op(|s| s.duplicate()),
            A::Copy => self.copy_contours(),
            A::Paste => self.paste_contours(),
        }
    }

    /// Copy the selected contours, or all of them when nothing is
    /// selected.
    fn copy_contours(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        self.clipboard = self.session.contours_for_copy();
        self.note = match self.clipboard.len() {
            0 => "nothing to copy".into(),
            1 => "copied 1 contour".into(),
            n => format!("copied {n} contours"),
        };
    }

    /// Paste the copied contours into the open glyph, with undo.
    fn paste_contours(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) || self.clipboard.is_empty() {
            return;
        }
        let contours = self.clipboard.clone();
        self.apply_op(move |session| session.paste_contours(&contours));
        self.note = format!("pasted {} contours", self.clipboard.len());
    }

    /// Copy the open glyph's outline into the UFO background layer.
    fn send_to_background(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let name = self.session.glyph_name.clone();
        let contours = self.session.glyph.contours.clone();
        let width = self.session.advance();
        self.font.send_to_background(&name, contours, width);
        self.show_background = true;
        self.modified = true;
        self.note = "sent to background".into();
    }

    /// Exchange the outline with the background layer's copy.
    fn swap_background(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let name = self.session.glyph_name.clone();
        let Some(background) = self.font.background_contours(&name) else {
            self.note = "no background to swap".into();
            return;
        };
        let foreground = self.session.glyph.contours.clone();
        let width = self.session.advance();
        self.apply_op(move |session| session.set_contours(background));
        self.font.send_to_background(&name, foreground, width);
        self.modified = true;
        self.note = "swapped with background".into();
    }

    /// Empty the open glyph's background layer.
    fn clear_background(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let name = self.session.glyph_name.clone();
        self.font.clear_background(&name);
        self.modified = true;
        self.note = "cleared background".into();
    }

    /// What sits under the drawing: the background layer if it is turned
    /// on, and the reference glyph if one is named.
    fn underlay(&self) -> editor::Underlay {
        if !matches!(self.mode, Mode::Editor(_)) {
            return editor::Underlay::default();
        }
        let background = self
            .show_background
            .then(|| self.font.background_outline(&self.session.glyph_name))
            .flatten()
            .map(Arc::new);
        let reference = {
            let name = self.reference_buf.trim();
            (!name.is_empty() && name != self.session.glyph_name)
                .then(|| self.font.glyph_outline(name))
                .flatten()
        };
        editor::Underlay {
            background,
            reference,
        }
    }

    /// Recompute the LSB/RSB/advance text buffers from the current session.
    /// The selection's reference point, at the picked corner.
    fn coord_point(&self) -> Option<masonry::kurbo::Point> {
        let bounds = self.session.selection_bounds()?;
        Some(self.coord_quadrant.point_in_dspace_rect(bounds))
    }

    /// Refill the Coordinates fields from the selection.
    fn refresh_coord_bufs(&mut self) {
        match self.coord_point() {
            Some(p) => {
                self.coord_x_buf = format!("{}", p.x as i64);
                self.coord_y_buf = format!("{}", p.y as i64);
            }
            None => {
                self.coord_x_buf.clear();
                self.coord_y_buf.clear();
            }
        }
    }

    /// Move the selection so its reference point lands on the typed value.
    fn set_coord(&mut self, axis: usize, v: String) {
        if axis == 0 {
            self.coord_x_buf = v.clone();
        } else {
            self.coord_y_buf = v.clone();
        }
        let (Ok(target), Some(now)) = (v.trim().parse::<f64>(), self.coord_point()) else {
            return;
        };
        let (dx, dy) = if axis == 0 {
            (target - now.x, 0.0)
        } else {
            (0.0, target - now.y)
        };
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        self.apply_op(|s| s.nudge(dx, dy));
        self.refresh_coord_bufs();
    }

    fn refresh_metric_bufs(&mut self) {
        self.advance_buf = format!("{}", self.session.advance() as i64);
        let (l, r) = metric_bufs(&self.session);
        self.lsb_buf = l;
        self.rsb_buf = r;
        let name = self.session.glyph_name.clone();
        self.kern1_buf = self.font.kern_group(&name, true);
        self.kern2_buf = self.font.kern_group(&name, false);
    }

    /// Put the open glyph in a kerning group on one side. An empty name
    /// takes it out of the group.
    fn set_kern_group(&mut self, first_side: bool, value: String) {
        if first_side {
            self.kern1_buf = value.clone();
        } else {
            self.kern2_buf = value.clone();
        }
        let name = self.session.glyph_name.clone();
        if self.font.set_kern_group(&name, first_side, value.trim()) {
            self.modified = true;
        }
    }

    /// Set the left sidebearing: shift the glyph so its ink left edge sits at
    /// `v` (advance unchanged, so the right sidebearing moves).
    fn set_lsb_from_buf(&mut self, v: String) {
        self.lsb_buf = v;
        if let Ok(t) = self.lsb_buf.trim().parse::<f64>() {
            let mut sess = (*self.session).clone();
            if let Some(sb) = sess.side_bearings() {
                sess.shift_glyph(t - sb.min_x);
                self.session = Arc::new(sess);
                self.refresh_open_glyph();
                self.advance_buf = format!("{}", self.session.advance() as i64);
                if let Some(sb2) = self.session.side_bearings() {
                    self.rsb_buf = format!("{}", sb2.rsb);
                }
            }
        }
    }

    /// Set the right sidebearing: change the advance so the gap past the ink
    /// right edge equals `v` (left sidebearing unchanged).
    fn set_rsb_from_buf(&mut self, v: String) {
        self.rsb_buf = v;
        if let Ok(t) = self.rsb_buf.trim().parse::<f64>() {
            let mut sess = (*self.session).clone();
            if let Some(sb) = sess.side_bearings() {
                sess.set_advance(sb.max_x + t);
                self.session = Arc::new(sess);
                self.refresh_open_glyph();
                self.advance_buf = format!("{}", self.session.advance() as i64);
            }
        }
    }

    fn apply_op(&mut self, f: impl FnOnce(&mut Session) -> bool) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let mut sess = (*self.session).clone();
        if f(&mut sess) {
            self.session = Arc::new(sess);
            self.refresh_open_glyph();
        }
    }

    fn set_unicode_from_buf(&mut self, v: String) {
        self.unicode_buf = v;
        let mut sess = (*self.session).clone();
        if sess.set_unicode(self.unicode_buf.trim()) {
            self.session = Arc::new(sess);
            self.refresh_open_glyph();
        }
    }

    fn commit_rename(&mut self) {
        let new = self.name_buf.trim().to_string();
        if new.is_empty() || new == self.session.glyph_name {
            return;
        }
        let old = self.session.glyph_name.clone();
        if self.font.rename_glyph(&old, &new) {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            if let Some(i) = self.font.index_of(&new) {
                self.mode = Mode::Editor(i);
                self.selected = Some(i);
                if let Some(sess) = Session::new(&self.font.font, &new) {
                    self.session = Arc::new(sess);
                }
            }
            self.modified = true;
        }
    }

    fn set_mark(&mut self, label: Option<String>) {
        if !self.multi_selected.is_empty() {
            self.apply_mark_to_selection(label);
            return;
        }
        let mut sess = (*self.session).clone();
        sess.set_mark(label.as_deref());
        self.session = Arc::new(sess);
        self.refresh_open_glyph();
    }

    fn apply_mark_to_selection(&mut self, label: Option<String>) {
        let indices: Vec<usize> = self.multi_selected.iter().copied().collect();
        for i in indices {
            if let Some(entry) = self.font.glyphs.get(i) {
                if let Some(mut g) = self.font.font.get_glyph(&entry.name).cloned() {
                    runebender_core::theme_oklch::set_glyph_mark(&mut g, label.as_deref());
                    self.font.replace_glyph(i, g);
                }
            }
        }
        self.modified = true;
    }

    fn set_advance_from_buf(&mut self, v: String) {
        self.advance_buf = v;
        if let Ok(w) = self.advance_buf.trim().parse::<f64>() {
            let mut sess = (*self.session).clone();
            sess.set_advance(w);
            self.session = Arc::new(sess);
            self.refresh_open_glyph();
        }
    }

    fn back_to_overview(&mut self) {
        self.refresh_open_glyph();
        self.mode = Mode::Overview;
    }
}

/// Layers: one row per master, with a thumbnail of the current glyph in
/// that master. Clicking a row switches the active master. This is the
/// gpui inspector's Layers section, and it replaces the old tab strip
/// that sat across the top of the canvas.
fn layers_section(app: &App) -> Option<impl WidgetView<App> + use<>> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Size};
    if app.font.master_names.len() < 2 {
        return None;
    }
    let pal = &app.palette;
    let glyph_name = match app.mode {
        Mode::Editor(_) => Some(app.session.glyph_name.clone()),
        Mode::Overview => app
            .selected
            .and_then(|i| app.font.glyphs.get(i))
            .map(|g| g.name.clone()),
    };
    let (asc, desc) = (app.font.ascender, app.font.descender);
    let rows: Vec<_> = app
        .font
        .short_master_names()
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let active = i == app.font.active;
            let shown = app.reference_layers.contains(&i);
            let (bg, fg) = if active {
                (pal.role("gridSelected").with_alpha(0.22), pal.role("accent"))
            } else {
                (pal.panel, pal.text)
            };
            // A lit thumbnail means the master is drawn as a ghost under
            // the active outline; clicking the thumbnail toggles that.
            let ink = if active || shown { pal.text } else { pal.text_muted };
            let thumb_bg = if shown {
                pal.role("reference").with_alpha(0.28)
            } else {
                pal.control
            };
            let path_and_advance = glyph_name
                .as_ref()
                .and_then(|n| app.font.master_glyph(i, n));
            let thumb = path_and_advance.map(|(path, advance)| {
                sized_box(
                    button(
                        sized_box(canvas(move |_app: &mut App, _ctx, scene, size: Size| {
                            let mut p = Painter::new(scene);
                            let em = (asc - desc).max(1.0);
                            let scale =
                                (size.height / em).min(size.width / advance.max(1.0));
                            let ox = (size.width - advance * scale) / 2.0;
                            let baseline = size.height + desc * scale;
                            let t = Affine::new([scale, 0.0, 0.0, -scale, ox, baseline]);
                            p.fill(&(t * path.clone()), ink).draw();
                        }))
                        .dims(Dimensions::new(
                            Dim::from(ControlSize::Icon),
                            Dim::from(ControlSize::Icon),
                        )),
                        move |app: &mut App| {
                            if !app.reference_layers.remove(&i) {
                                app.reference_layers.insert(i);
                            }
                        },
                    )
                    .background_color(thumb_bg),
                )
                .dims(Dimensions::new(
                    Dim::from(ControlSize::Control),
                    Dim::from(ControlSize::Control),
                ))
            });
            xrow(
                Region::Inline,
                (
                    thumb,
                    sized_box(
                        button(
                            label(name).text_size(TextSize::Body.px()).color(fg),
                            move |app: &mut App| app.set_master(i),
                        )
                        .background_color(bg),
                    )
                    .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Control)))
                    .flex(1.0),
                ),
            )
        })
        .collect();
    Some(
        xcolumn(
            Region::Section,
            (section_header(pal, "Layers"), xcolumn(Region::List, rows)),
        ),
    )
}

/// Axes: one labeled slider per designspace axis, in the inspector.
fn axes_section(app: &App) -> Option<impl WidgetView<App> + use<>> {
    if app.font.axes.is_empty() {
        return None;
    }
    let pal = &app.palette;
    let (muted, text) = (pal.text_muted, pal.text);
    let rows: Vec<_> = app
        .font
        .axes
        .iter()
        .enumerate()
        .map(|(i, ax)| {
            let value = app.axis_values.get(i).copied().unwrap_or(ax.default);
            xcolumn(
                Region::List,
                (
                    xrow(
                        Region::Inline,
                        (
                            label(ax.tag.clone()).text_size(TextSize::Body.px()).color(muted),
                            FlexSpacer::Flex(1.0),
                            label(format!("{value:.0}")).text_size(TextSize::Body.px()).color(text),
                        ),
                    ),
                    slider(ax.min, ax.max, value, move |app: &mut App, v| {
                        app.set_axis(i, v)
                    })
                    .width(Length::px(214.0)),
                ),
            )
        })
        .collect();
    // A short hint when the location sits off any master.
    let hint = (!app.on_active_master())
        .then(|| label("interpolated").text_size(TextSize::Caption.px()).color(pal.role("warning")));
    Some(
        xcolumn(
            Region::Section,
            (
                section_header(pal, "Axes"),
                xcolumn(Region::Form, rows),
                hint,
            ),
        ),
    )
}

/// Display name for a theme id, e.g. "dark" -> "Dark".
fn theme_label(id: &str) -> String {
    let mut c = id.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn titlebar(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let editing = matches!(app.mode, Mode::Editor(_));
    let filename = app
        .font
        .source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let title = match app.mode {
        Mode::Overview => filename.clone(),
        Mode::Editor(i) => app.font.glyphs.get(i).map(|g| g.name.clone()).unwrap_or_default(),
    };
    // Save status, gpui-style: yellow when unsaved, muted otherwise.
    let (save_text, save_color) = if app.modified {
        ("Not saved", pal.role("warning"))
    } else {
        ("Saved", pal.text_muted)
    };
    xrow(
        Region::Toolbar,
        (
            editing.then(|| {
                text_button("‹ Overview", |app: &mut App| app.back_to_overview())
                    .background_color(pal.button)
            }),
            (!editing).then(|| label(title).text_size(TextSize::Title.px()).color(pal.text)),
            FlexSpacer::Flex(1.0),
            editing.then(|| header_tools(app)),
            text_button(theme_label(app.theme_id), |app: &mut App| app.cycle_theme())
                .background_color(pal.button),
            label(save_text.to_string()).color(save_color),
            text_button("Save", |app: &mut App| app.save()).background_color(pal.button),
        ),
    )
    .background_color(pal.panel)
}

/// The tools as a horizontal row for the header (gpui puts them there,
/// not in a left column).
fn header_tools(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let fg = pal.text_muted;
    let fg_active = pal.role("accent");
    let active_bg = pal.role("gridSelected").with_alpha(0.25);
    let hover_bg = pal.control;
    let tile = move |icon: &'static str, tool: Tool| {
        icon_button(icon, app.tool == tool, fg, fg_active, active_bg, hover_bg, move |app: &mut App| {
            app.tool = tool;
        })
    };
    xrow(
        Region::List,
        (
            tile("select", Tool::Select),
            tile("pen", Tool::Pen),
            tile("hyperpen", Tool::HyperPen),
            tile("shape-rectangle", Tool::Rect),
            tile("shape-ellipse", Tool::Ellipse),
            tile("knife", Tool::Knife),
            tile("measure", Tool::Measure),
        ),
    )
}

fn status(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let text = match app.mode {
        Mode::Overview => format!(
            "{} glyphs   {}",
            app.font.glyphs.len(),
            app.font.source.display()
        ),
        Mode::Editor(_) => format!(
            "{}   advance {}   {} points   {} selected",
            app.session.glyph_name.as_str(),
            app.session.advance(),
            app.session.point_count(),
            app.selected_points,
        ),
    };
    let text = if app.note.is_empty() {
        text
    } else {
        format!("{}   {}", text, app.note)
    };
    // Bottom bar: mark swatches on the left (set the current/selected glyphs'
    // mark), then the status text (gpui's bottom bar).
    let swatch = |mark: Option<String>, color: xilem::Color| {
        sized_box(
            text_button("", move |app: &mut App| app.set_mark(mark.clone()))
                .background_color(color),
        )
        .dims(Dimensions::fixed(ControlSize::Swatch.length(), ControlSize::Swatch.length()))
    };
    let marks: Vec<_> = app
        .palette
        .mark_list()
        .into_iter()
        .map(|(name, color)| swatch(Some(name), color))
        .collect();
    xrow(
        Region::Toolbar,
        (
            swatch(None, pal.control),
            xrow(Region::List, marks),
            label(text).color(pal.text_muted),
        ),
    )
    .background_color(pal.panel)
}

fn sidebar(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;

    let cats = [
        GlyphCategory::All,
        GlyphCategory::Letter,
        GlyphCategory::Number,
        GlyphCategory::Punctuation,
        GlyphCategory::Symbol,
        GlyphCategory::Mark,
        GlyphCategory::Other,
    ];
    let cat_rows: Vec<_> = cats
        .into_iter()
        .filter(|c| app.category_count(*c) > 0)
        .map(|c| {
            ui::list_row(
                pal,
                c.display_name().to_string(),
                format!("{}", app.category_count(c)),
                app.sel == Sel::Category(c),
                move |app: &mut App| app.sel = Sel::Category(c),
            )
        })
        .collect();

    let lang_rows: Vec<_> = runebender_core::sidebar::language_groups()
        .iter()
        .enumerate()
        .filter(|(i, _)| app.language_count(*i) > 0)
        .map(|(i, g)| {
            ui::list_row(
                pal,
                g.label.clone(),
                format!("{}", app.language_count(i)),
                app.sel == Sel::Language(i),
                move |app: &mut App| app.sel = Sel::Language(i),
            )
        })
        .collect();

    let filter_rows: Vec<_> = runebender_core::sidebar::builtin_filters()
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let gs = b.glyphset.as_ref()?;
            let expected = gs.expected_count.unwrap_or(gs.glyph_names.len().max(gs.targets.len()));
            Some(ui::list_row(
                pal,
                b.label.clone(),
                format!("{}/{}", app.filter_present(i), expected),
                app.sel == Sel::Filter(i),
                move |app: &mut App| app.sel = Sel::Filter(i),
            ))
        })
        .collect();

    // Search row: the field, then gpui's small scope and case toggles.
    let toggle = |text: String, active: bool, f: fn(&mut App)| {
        ui::toggle(pal, text, active, move |app: &mut App| f(app))
    };
    xcolumn(
        Region::Panel,
        (
        xrow(
            Region::Inline,
            (
                text_input(app.filter.clone(), |app: &mut App, v| app.filter = v)
                    .placeholder("Search")
                    .flex(1.0),
                toggle(
                    match app.search_mode { 1 => "N", 2 => "U", _ => "A" }.to_string(),
                    app.search_mode != 0,
                    |app: &mut App| app.search_mode = (app.search_mode + 1) % 3,
                ),
                toggle("Aa".into(), app.search_case, |app: &mut App| {
                    app.search_case = !app.search_case
                }),
            ),
        ),
        text_button(
            match app.sort { Sort::Name => "Sort: name", Sort::Unicode => "Sort: unicode" },
            |app: &mut App| {
                app.sort = match app.sort { Sort::Name => Sort::Unicode, Sort::Unicode => Sort::Name };
            },
        )
        .background_color(pal.button),
        {
            let fresh = !app.filter.trim().is_empty() && app.font.index_of(app.filter.trim()).is_none();
            fresh.then(|| {
                text_button(format!("+ New {}", app.filter.trim()), |app: &mut App| app.new_glyph())
                    .background_color(pal.role("accent"))
            })
        },
        portal(xcolumn(
            Region::Panel,
            (
                xcolumn(
                    Region::Section,
                    (section_header(pal, "Categories"), xcolumn(Region::List, cat_rows)),
                ),
                (!lang_rows.is_empty()).then(|| {
                    xcolumn(
                        Region::Section,
                        (section_header(pal, "Languages"), xcolumn(Region::List, lang_rows)),
                    )
                }),
                (!filter_rows.is_empty()).then(|| {
                    xcolumn(
                        Region::Section,
                        (section_header(pal, "Filters"), xcolumn(Region::List, filter_rows)),
                    )
                }),
            ),
        ))
        .flex(1.0),
        ),
    )
    .background_color(pal.panel)
}

/// A flat, full-width sidebar row: label left, count right, subtle highlight
/// when selected (gpui's list rows, not pill buttons).

fn editor_nav(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let current = match app.mode { Mode::Editor(i) => Some(i), _ => None };
    xcolumn(
        Region::Panel,
        (
        text_input(app.filter.clone(), |app: &mut App, v| app.filter = v)
            .placeholder("Search"),
        // The grid scrolls itself, so no portal here: nesting the two
        // gave the rail a dead area below the third row.
        grid(
            app.filtered_cells(),
            app.cell_metrics(84.0),
            app.palette.clone(),
            current,
            app.multi_selected.clone(),
            |app: &mut App, ev| match ev {
                GridEvent::Selected { index, .. } => app.open_glyph(index),
                GridEvent::Open(i) => app.open_glyph(i),
            },
        )
        .flex(1.0),
        ),
    )
    .background_color(pal.panel)
}

fn overview(app: &App) -> impl WidgetView<App> + use<> {
    let metrics = app.cell_metrics(104.0);
    grid(
        app.filtered_cells(),
        metrics,
        app.palette.clone(),
        app.selected,
        app.multi_selected.clone(),
        |app: &mut App, ev| match ev {
            GridEvent::Selected { index, cmd, shift } => app.grid_select(index, cmd, shift),
            GridEvent::Open(i) => app.open_glyph(i),
        },
    )
}

fn preview_strip(app: &App) -> impl WidgetView<App> + use<> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Point, Size};
    // When the axis sliders are off a master, preview the interpolated
    // instance (in warm amber) so the strip reflects the current location.
    let interp = app.interp_preview();
    let outline = match &interp {
        Some(o) => o.clone(),
        None => app.session.outline_arc(),
    };
    let components = app.session.components_arc();
    let has_components = !interp.is_some() && !components.elements().is_empty();
    let m = app.session.metrics;
    let advance = app.session.advance();
    let fill = if interp.is_some() {
        app.palette.role("warning")
    } else {
        app.palette.text
    };
    canvas(move |_app: &mut App, _ctx, scene, size: Size| {
        let mut p = Painter::new(scene);
        // Fit the em box (advance wide, ascender..descender tall) into the strip.
        let margin = 16.0;
        let em_w = advance.max(m.upm * 0.5);
        let em_h = m.ascender - m.descender;
        let scale = ((size.width - margin * 2.0) / em_w).min((size.height - margin * 2.0) / em_h);
        let baseline_y = margin + (m.ascender / em_h) * (size.height - margin * 2.0);
        let x0 = (size.width - em_w * scale) / 2.0;
        let t = Affine::new([scale, 0.0, 0.0, -scale, x0, baseline_y]);
        let _ = Point::ORIGIN;
        p.fill(&(t * (*outline).clone()), fill).draw();
        if has_components {
            p.fill(&(t * (*components).clone()), fill).draw();
        }
    })
}

/// A large preview of the selected glyph, at the foot of the inspector in
/// overview mode (gpui's glyph preview panel). The grid cell is small; this
/// is where you look at the shape.
fn glyph_preview(app: &App) -> Option<impl WidgetView<App> + use<>> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Rect, Shape, Size, Stroke};
    let entry = app.selected.and_then(|i| app.font.glyphs.get(i))?;
    let outline = entry.outline.clone();
    let advance = entry.advance;
    let (asc, desc) = (app.font.ascender, app.font.descender);
    let fill = app.palette.text;
    let line = app.palette.role("gridBorder").with_alpha(0.5);
    Some(
        sized_box(canvas(move |_app: &mut App, _ctx, scene, size: Size| {
            let mut p = Painter::new(scene);
            let margin = 18.0;
            let em_w = advance.max(1.0);
            let em_h = (asc - desc).max(1.0);
            let scale = ((size.width - margin * 2.0) / em_w)
                .min((size.height - margin * 2.0) / em_h);
            let ox = (size.width - em_w * scale) / 2.0;
            let baseline = (size.height + em_h * scale) / 2.0 + desc * scale;
            let t = Affine::new([scale, 0.0, 0.0, -scale, ox, baseline]);
            // The advance box, so the preview reads as metrics, not art.
            let box_path = t * Rect::new(0.0, desc, em_w, asc).to_path(0.1);
            p.stroke(&box_path, &Stroke::new(1.0), line).draw();
            p.fill(&(t * (*outline).clone()), fill).draw();
        }))
        .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(170.0)))),
    )
}

fn editor_pane(app: &App) -> impl WidgetView<App> + use<> {
    let ghosts = Arc::new(
        app.font
            .reference_outlines(&app.session.glyph_name, &app.reference_layers),
    );
    let interp = app.interp_preview();
    editor(app.session.clone(), app.palette.clone(), app.tool, app.view, ghosts, interp, app.underlay(), |app: &mut App, ev| match ev {
        editor::EditorEvent::Selection(n) => {
            app.selected_points = n;
            app.refresh_coord_bufs();
        }
        editor::EditorEvent::Edited => app.refresh_open_glyph(),
        editor::EditorEvent::Save => app.save(),
        editor::EditorEvent::Exit => app.back_to_overview(),
    })
}

fn path_section(app: &App) -> impl WidgetView<App> + use<> {
    use icon_button::icon_button;
    use session::BoolOp;
    let pal = &app.palette;
    let fg = pal.text_muted;
    let fga = pal.role("accent");
    let abg = pal.role("gridSelected").with_alpha(0.25);
    let hbg = pal.control;
    let op = move |icon: &'static str, f: fn(&mut Session) -> bool| {
        icon_button(icon, false, fg, fga, abg, hbg, move |app: &mut App| app.apply_op(f))
    };
    xcolumn(
        Region::Section,
        (
            section_header(pal, "Transformations"),
            // Two even rows of four, the way gpui lays its icon grid out. A
            // ragged 3 / 4 / 1 grid was the panel's most visible defect.
            xrow(
                Region::List,
                (
                    op("flip-h", |s| s.flip_horizontal()),
                    op("flip-v", |s| s.flip_vertical()),
                    op("rot-cw", |s| s.rotate_90()),
                    op("close", |s| s.decompose()),
                ),
            ),
            xrow(
                Region::List,
                (
                    op("union", |s| s.remove_overlap()),
                    op("subtract", |s| s.boolean(BoolOp::Subtract)),
                    op("intersect", |s| s.boolean(BoolOp::Intersect)),
                    op("exclude", |s| s.boolean(BoolOp::Exclude)),
                ),
            ),
            // Labeled transform buttons, matching gpui's Transformations block.
            xrow(
                Region::Inline,
                (
                    tbtn(pal, "Harmonize", |s| s.harmonize()),
                    tbtn(pal, "Balance", |s| s.balance()),
                ),
            ),
            xrow(
                Region::Inline,
                (
                    tbtn(pal, "Optimize", |s| s.optimize()),
                    tbtn(pal, "Round", |s| s.round_corners()),
                ),
            ),
            xrow(Region::Inline, (tbtn(pal, "Reverse", |s| s.reverse()),)),
        ),
    )
}

/// The LSB/RSB text-buffer strings for a session.
fn metric_bufs(session: &Session) -> (String, String) {
    match session.side_bearings() {
        Some(sb) => (format!("{}", sb.lsb), format!("{}", sb.rsb)),
        None => (String::new(), String::new()),
    }
}

/// A right-panel section header with a disclosure caret (gpui style).

/// A labeled path-operation button.
fn tbtn(pal: &Palette, text: &'static str, f: fn(&mut Session) -> bool) -> impl WidgetView<App> + use<> {
    text_button(text, move |app: &mut App| app.apply_op(f)).background_color(pal.button)
}

/// Coordinates: the 9-point reference picker beside the X/Y fields, with
/// the selection's size on the right. gpui keeps this panel up whether or
/// not anything is selected, so the inspector does not jump.
fn coordinates_section(app: &App) -> impl WidgetView<App> + use<> {
    use runebender_core::path::Quadrant;
    const QUADRANTS: [Quadrant; 9] = [
        Quadrant::TopLeft,
        Quadrant::Top,
        Quadrant::TopRight,
        Quadrant::Left,
        Quadrant::Center,
        Quadrant::Right,
        Quadrant::BottomLeft,
        Quadrant::Bottom,
        Quadrant::BottomRight,
    ];
    let pal = &app.palette;
    let bounds = app.session.selection_bounds();
    // The picker: three rows of three dots, the active one filled accent.
    let dot = |q: Quadrant| {
        let active = app.coord_quadrant == q;
        let (bg, border) = if active {
            (pal.role("accent"), pal.role("accent"))
        } else {
            (pal.panel, pal.role("gridBorder").with_alpha(0.7))
        };
        sized_box(
            button(label(""), move |app: &mut App| {
                app.coord_quadrant = q;
                app.refresh_coord_bufs();
            })
            // Without this the button's own padding sets a minimum width
            // and the dot stretches into a pill.
            .padding(Space::None)
            .background_color(bg)
            .border_color(border)
            .border_width(Stroke::Hairline.length())
            .corner_radius(Radius::Sm.length()),
        )
        .dims(Dimensions::fixed(ControlSize::Dot.length(), ControlSize::Dot.length()))
    };
    let row = |a: usize| {
        xrow(
            Region::Card,
            (dot(QUADRANTS[a]), dot(QUADRANTS[a + 1]), dot(QUADRANTS[a + 2])),
        )
    };
    let picker = xcolumn(Region::Card, (row(0), row(3), row(6)));
    let field = |name: &'static str, value: String, axis: usize| {
        xrow(
            Region::Inline,
            (
                sized_box(label(name).text_size(TextSize::Body.px()).color(pal.text_muted))
                    .dims(Dimensions::fixed(ControlSize::Swatch.length(), ControlSize::Icon.length())),
                text_input(value, move |app: &mut App, v| app.set_coord(axis, v))
                    .background_color(pal.field())
                    .flex(1.0),
            ),
        )
    };
    let size_row = bounds.map(|b| {
        xrow(
            Region::Inline,
            (
                label("Size").text_size(TextSize::Body.px()).color(pal.text_muted),
                FlexSpacer::Flex(1.0),
                label(format!("{:.0} x {:.0}", b.width(), b.height()))
                    .text_size(TextSize::Body.px())
                    .color(pal.text),
            ),
        )
    });
    xcolumn(
        Region::Section,
        (
            section_header(pal, "Coordinates"),
            xrow(
                Region::Inline,
                (
                    picker,
                    xcolumn(
                        Region::List,
                        (
                            field("X", app.coord_x_buf.clone(), 0),
                            field("Y", app.coord_y_buf.clone(), 1),
                        ),
                    )
                    .flex(1.0),
                ),
            ),
            size_row,
        ),
    )
}

/// Curves: the two analyses that are about shape quality rather than
/// measurement. Both read from runebender-core, so they say the same
/// thing here as in the other two editors.
/// The tab strip: one tab per open glyph, with a close box, and a plus
/// that opens a second view on the glyph in hand.
fn tab_strip(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let active = app.active_tab;
    let closable = app.tabs.len() > 1;
    let tabs: Vec<_> = app
        .tabs
        .iter()
        .enumerate()
        .map(|(index, tab)| {
            let is_active = index == active;
            let (bg, fg) = if is_active {
                (pal.role("gridSelected").with_alpha(0.22), pal.role("accent"))
            } else {
                (pal.panel, pal.text)
            };
            xrow(
                Region::List,
                (
                    sized_box(
                        button(
                            label(tab.session.glyph_name.clone())
                                .text_size(TextSize::Body.px())
                                .color(fg),
                            move |app: &mut App| app.activate_tab(index),
                        )
                        .background_color(bg),
                    )
                    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Row))),
                    closable.then(|| {
                        ui::toggle(pal, "x".into(), false, move |app: &mut App| {
                            app.close_tab(index)
                        })
                    }),
                ),
            )
        })
        .collect();
    xrow(
        Region::Toolbar,
        (
            xrow(Region::Inline, tabs),
            ui::toggle(pal, "+".into(), false, |app: &mut App| app.new_tab()),
        ),
    )
    .background_color(pal.panel)
}

fn curves_section(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let view = app.view;
    xcolumn(
        Region::Section,
        (
            section_header(pal, "Curves"),
            xrow(
                Region::Inline,
                (
                    ui::toggle(pal, "Comb".into(), view.comb, |app: &mut App| {
                        app.view.comb = !app.view.comb;
                    }),
                    ui::toggle(pal, "G0-G3".into(), view.continuity, |app: &mut App| {
                        app.view.continuity = !app.view.continuity;
                    }),
                ),
            ),
        ),
    )
}

/// Measure: the option toggles the Measure tool works through. Picking
/// the tool turns the usual three on; the toggles are what make it
/// answer a specific question instead of drawing everything at once.
fn measure_section(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let view = app.view;
    xcolumn(
        Region::Section,
        (
            section_header(pal, "Measure"),
            xrow(
                Region::Inline,
                (
                    ui::toggle(pal, "Color".into(), view.colorize, |app: &mut App| {
                        app.view.colorize = !app.view.colorize;
                    }),
                    ui::toggle(pal, "Handles".into(), view.handles, |app: &mut App| {
                        app.view.handles = !app.view.handles;
                    }),
                ),
            ),
            xrow(
                Region::Inline,
                (
                    ui::toggle(pal, "Segments".into(), view.segments, |app: &mut App| {
                        app.view.segments = !app.view.segments;
                    }),
                    ui::toggle(pal, "Bearings".into(), view.bearings, |app: &mut App| {
                        app.view.bearings = !app.view.bearings;
                    }),
                ),
            ),
            // Lengths as sums of powers of two: 96 reads as 64+32. The
            // web editor's habit, and the reason for the tier colors.
            ui::toggle(pal, "Popcount sums".into(), view.popcount, |app: &mut App| {
                app.view.popcount = !app.view.popcount;
            }),
        ),
    )
}

/// Background: the UFO's background layer, and a reference glyph. Both
/// are things to trace against, so both draw quietly and neither can be
/// selected.
fn background_section(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let has_background = app.font.background_contours(&app.session.glyph_name).is_some();
    let show = app.show_background;
    xcolumn(
        Region::Section,
        (
            section_header(pal, "Background"),
            xrow(
                Region::Inline,
                (
                    ui::toggle(pal, "Show".into(), show && has_background, |app: &mut App| {
                        app.show_background = !app.show_background;
                    }),
                    ui::action(pal, "Send".into(), |app: &mut App| app.send_to_background()),
                    ui::action(pal, "Swap".into(), |app: &mut App| app.swap_background()),
                    ui::action(pal, "Clear".into(), |app: &mut App| app.clear_background()),
                ),
            ),
            ui::field(pal, "Reference glyph", app.reference_buf.clone(), |app: &mut App, v| {
                app.reference_buf = v;
            }),
        ),
    )
}

fn mark_section(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let swatch = |label: Option<String>, color: xilem::Color| {
        sized_box(
            text_button("", move |app: &mut App| app.set_mark(label.clone()))
                .background_color(color),
        )
        .dims(Dimensions::fixed(ControlSize::Row.length(), ControlSize::Row.length()))
    };
    let marks: Vec<_> = app.palette.mark_list().into_iter().map(|(name, color)| swatch(Some(name), color)).collect();
    xcolumn(
        Region::Section,
        (
            section_header(pal, "Mark"),
            xrow(Region::Inline, (swatch(None, pal.control),)),
            xrow(Region::List, marks),
        ),
    )
}

fn info_panel(app: &App) -> impl WidgetView<App> + use<> {
    let pal = &app.palette;
    let row = move |k: String, v: String| ui::kv(pal, k, v);
    let (name, adv, pts, cp) = match app.mode {
        Mode::Editor(_) => (
            app.session.glyph_name.clone(),
            format!("{}", app.session.advance() as i64),
            format!("{}", app.session.point_count()),
            String::new(),
        ),
        Mode::Overview => {
            let g = app.selected.and_then(|i| app.font.glyphs.get(i));
            (
                g.map(|g| g.name.clone()).unwrap_or_default(),
                g.map(|g| format!("{}", g.advance as i64)).unwrap_or_default(),
                String::new(),
                g.and_then(|g| g.codepoint).map(|c| format!("U+{:04X}", c as u32)).unwrap_or_default(),
            )
        }
    };
    let editing = matches!(app.mode, Mode::Editor(_));
    // Width / LSB / RSB in one row (gpui's metrics row). Each field commits
    // live; LSB shifts the glyph, RSB changes the advance.
    let field_bg = pal.field();
    let _ = field_bg;
    let advance_field = editing.then(|| {
        xrow(
            Region::Form,
            (
                ui::field(pal, "Width", app.advance_buf.clone(), |app: &mut App, v| {
                    app.set_advance_from_buf(v)
                })
                .flex(1.0),
                ui::field(pal, "LSB", app.lsb_buf.clone(), |app: &mut App, v| {
                    app.set_lsb_from_buf(v)
                })
                .flex(1.0),
                ui::field(pal, "RSB", app.rsb_buf.clone(), |app: &mut App, v| {
                    app.set_rsb_from_buf(v)
                })
                .flex(1.0),
            ),
        )
    });
    let name_field = editing.then(|| {
        xcolumn(
            Region::Form,
            (
                ui::field(pal, "Name", app.name_buf.clone(), |app: &mut App, v| {
                    app.name_buf = v
                }),
                ui::field(pal, "Unicode", app.unicode_buf.clone(), |app: &mut App, v| {
                    app.set_unicode_from_buf(v)
                }),
                // Kerning groups, left side then right, as gpui's Glyph
                // panel has them. Empty takes the glyph out of the group,
                // and the write lands in every master, because a
                // designspace's masters have to agree about groups.
                label("Kerning Groups (L \u{00b7} R)")
                    .text_size(TextSize::Caption.px())
                    .color(pal.text_muted),
                // Fixed widths: a group name is long enough that letting
                // the inputs size to their content pushes the whole
                // inspector past its column.
                xrow(
                    Region::Form,
                    (
                        sized_box(ui::field(pal, "", app.kern1_buf.clone(), |app: &mut App, v| {
                            app.set_kern_group(true, v)
                        }))
                        .dims(Dimensions::new(Dim::Fixed(Length::px(105.0)), Dim::Auto)),
                        sized_box(ui::field(pal, "", app.kern2_buf.clone(), |app: &mut App, v| {
                            app.set_kern_group(false, v)
                        }))
                        .dims(Dimensions::new(Dim::Fixed(Length::px(105.0)), Dim::Auto)),
                    ),
                ),
            ),
        )
    });
    let show_multi_mark = !editing && !app.multi_selected.is_empty();
    xcolumn(
        Region::Panel,
        (
            xcolumn(
                Region::Section,
                (
                    section_header(pal, "Glyph"),
                    xcolumn(
                        Region::List,
                        (
                            show_multi_mark.then(|| {
                                row("Selected".into(), format!("{}", app.multi_selected.len()))
                            }),
                            (!editing).then(|| row("Name".into(), name)),
                            (!editing).then(|| row("Unicode".into(), cp.clone())),
                            (!editing).then(|| row("Advance".into(), adv)),
                            (!pts.is_empty()).then(|| row("Points".into(), pts)),
                            editing.then(|| {
                                row("Selected".into(), format!("{}", app.selected_points))
                            }),
                        ),
                    ),
                    show_multi_mark.then(|| mark_section(app)),
                    name_field,
                    advance_field,
                ),
            ),
            editing.then(|| coordinates_section(app)),
            editing.then(|| path_section(app)),
            editing.then(|| curves_section(app)),
            editing.then(|| measure_section(app)),
            editing.then(|| background_section(app)),
            editing.then(|| mark_section(app)),
            layers_section(app),
            editing.then(|| axes_section(app)).flatten(),
            (!editing).then(|| glyph_preview(app)).flatten(),
        ),
    )
    .background_color(pal.panel)
}

fn app_logic(app: &mut App) -> impl WidgetView<App> + use<> {
    use xilem::core::one_of::Either;
    let pal = &app.palette;

    // Left column: category sidebar in overview only. In the editor the
    // tools live in the header (gpui-style), so the left column collapses.
    let editing_mode = matches!(app.mode, Mode::Editor(_));
    let _ = &app.multi_selected;

    // Center: title bar + body + status bar.
    let body = match app.mode {
        Mode::Overview => Either::A(overview(app)),
        Mode::Editor(_) => Either::B(editor_pane(app)),
    };
    let preview = matches!(app.mode, Mode::Editor(_))
        .then(|| sized_box(preview_strip(app)).dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(120.0)))).background_color(pal.panel));
    let center = flex_col((
        titlebar(app),
        editing_mode.then(|| tab_strip(app)),
        body.flex(1.0),
        preview,
        status(app),
    ))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .gap(Space::None)
        .background_color(pal.app);

    let left = match app.mode {
        Mode::Overview => Either::A(sidebar(app)),
        Mode::Editor(_) => Either::B(editor_nav(app)),
    };
    let left_width = if editing_mode { 232.0 } else { 224.0 };
    // The menu bar is built on the main thread, which is here, and only
    // once. Xilem owns the event loop and offers no startup hook.
    menu::install();
    menu::with_menu_events(shortcuts::shortcut_host(flex_row((
        sized_box(left)
            .dims(Dimensions::new(Dim::Fixed(Length::px(left_width)), Dim::Stretch))
            .background_color(pal.panel),
        sized_box(center)
            .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
            .background_color(pal.app)
            .flex(1.0),
        sized_box(portal(info_panel(app)))
            .dims(Dimensions::new(Dim::Fixed(Length::px(250.0)), Dim::Stretch))
            .background_color(pal.panel),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(Space::None)
    .background_color(pal.app)))
}

fn run(event_loop: EventLoopBuilder) -> Result<(), EventLoopError> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: runebender-xilem <Font.ufo|Font.designspace>");
    let mut app = App::open(FsPath::new(&path)).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    if std::env::var("RUNEBENDER_SELECTALL").is_ok() {
        let mut sess = (*app.session).clone();
        sess.select_all();
        app.selected_points = sess.selection_bounds().map(|_| 999).unwrap_or(0);
        let n = { let mut c = 0; for co in &sess.glyph.contours { c += co.points.len(); } c };
        app.selected_points = n;
        app.session = std::sync::Arc::new(sess);
        app.refresh_coord_bufs();
    }
    // Headless: render one frame and exit. No window, no event loop.
    if let Ok(path) = std::env::var("RUNEBENDER_SCREENSHOT") {
        // The harness needs a root widget with a concrete type, so wrap
        // the app's root view in a sized box.
        screenshot::render_to(app, |app: &mut App| sized_box(app_logic(app)), (1100, 720), &path);
        return Ok(());
    }
    let background = app.palette.app;
    let window_options =
        WindowOptions::new("Runebender").with_initial_inner_size(LogicalSize::new(1100., 720.));
    Xilem::new_simple(app, app_logic, window_options)
        .with_default_properties(default_property_set())
        .with_default_base_color(background)
        .run_in(event_loop)
}


fn main() -> Result<(), EventLoopError> {
    run(EventLoop::with_user_event())
}

#[cfg(test)]
mod tab_tests {
    use super::*;

    /// A two-glyph UFO on disk, because `App::open` takes a path. Each
    /// test gets its own directory so they can run in parallel.
    fn app() -> App {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        let mut font = norad::Font::new();
        for name in ["A", "B"] {
            let mut glyph = norad::Glyph::new(name);
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
        }
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("runebender-tabs-{n}.ufo"));
        let _ = std::fs::remove_dir_all(&path);
        font.save(&path).expect("save the test font");
        App::open(&path).expect("open the test font")
    }

    #[test]
    fn opening_a_glyph_twice_reuses_its_tab() {
        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        let b = app.font.index_of("B").expect("B");
        app.open_glyph(a);
        app.new_tab();
        app.open_glyph(b);
        let tabs = app.tabs.len();
        app.open_glyph(a);
        assert_eq!(app.tabs.len(), tabs, "no tab was added");
        assert_eq!(app.session.glyph_name, "A");
    }

    #[test]
    fn a_tab_keeps_its_own_selection() {
        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        app.open_glyph(a);
        let mut session = (*app.session).clone();
        session.select_all();
        app.session = Arc::new(session);
        let selected = app.session.selection.len();
        assert!(selected > 0, "the test glyph has points");

        app.new_tab();
        assert_eq!(app.session.selection.len(), 0, "the new tab starts clean");

        app.activate_tab(0);
        assert_eq!(app.session.selection.len(), selected, "the first tab kept it");
    }

    #[test]
    fn closing_the_last_tab_leaves_the_editor() {
        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        app.open_glyph(a);
        app.close_tab(0);
        assert!(matches!(app.mode, Mode::Overview));
    }

    #[test]
    fn closing_a_tab_before_the_active_one_keeps_the_right_glyph() {
        let mut app = app();
        let a = app.font.index_of("A").expect("A");
        let b = app.font.index_of("B").expect("B");
        app.open_glyph(a);
        app.new_tab();
        app.open_glyph(b);
        assert_eq!(app.tabs.len(), 2);
        app.activate_tab(1);
        let active = app.session.glyph_name.clone();
        app.close_tab(0);
        assert_eq!(app.session.glyph_name, active);
    }
}

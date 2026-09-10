// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! What the menus and shortcuts call. One method is the whole of one user-facing command.

use crate::*;

const SAMPLE_STRINGS: &[&str] = &[
    "HHOHOHOO",
    "nnonoonoo",
    "hamburgefonstiv",
    "HAMBURGEFONSTIV",
    "0123456789",
    "AVATAR Wave Toy Vy",
    "((\"quoted\")) [j] {f}!?",
];

impl Workspace {
    pub(crate) fn new_glyph(&mut self) {
        let name = self.filter.trim().to_string();
        let upm = self.font.units_per_em();
        if self.font.add_glyph(&name, (upm * 0.5).round(), None) {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.filter.clear();
            if let Some(i) = self.font.index_of(&name) {
                self.open_glyph(i);
            }
            self.modified = true;
        }
    }

    /// Add every glyph a coverage filter is missing, to every master.
    ///
    /// The GF sets carry a name and a codepoint per glyph, so what lands
    /// is named and encoded, which is what makes the row's count move.
    pub(crate) fn generate_missing(&mut self, index: usize) {
        let filters = runebender_core::ui::sidebar::builtin_filters();
        let Some(set) = filters.get(index).and_then(|f| f.glyphset.as_ref()) else {
            return;
        };
        let mut wanted: Vec<(String, Option<u32>)> = set
            .targets
            .iter()
            .map(|target| (target.name.clone(), Some(target.unicode)))
            .collect();
        for name in &set.glyph_names {
            if !wanted.iter().any(|(existing, _)| existing == name) {
                wanted.push((name.clone(), None));
            }
        }
        let added = self.font.add_missing(&wanted);
        if added > 0 {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.modified = true;
        }
        self.note = match added {
            0 => "nothing missing".into(),
            1 => "added 1 glyph".into(),
            n => format!("added {n} glyphs"),
        };
    }

    /// Advance to the next theme, reloading the palette and the baked cell
    /// colors. Exercises the design-token kernel: one id swaps every role.
    pub(crate) fn cycle_theme(&mut self) {
        let i = Self::THEMES
            .iter()
            .position(|t| *t == self.theme_id)
            .unwrap_or(0);
        self.theme_id = Self::THEMES[(i + 1) % Self::THEMES.len()];
        self.palette = Arc::new(Palette::load(self.theme_id));
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
    }

    pub(crate) fn dispatch(&mut self, action: shortcuts::AppAction) {
        use shortcuts::AppAction as A;
        match action {
            A::Quit => unreachable!("Quit is handled at the application shell"),
            A::Save => {
                // On the nodes canvas, Save writes the graph file too.
                if matches!(self.mode, Mode::Nodes) {
                    self.save_nodes_file();
                }
                self.save();
            }
            A::Undo => self.undo_open_glyph(false),
            A::Redo => self.undo_open_glyph(true),
            A::Overview => {
                if matches!(self.mode, Mode::Editor(_) | Mode::Nodes) {
                    self.back_to_overview();
                }
            }
            A::Tool(t) => {
                self.tool = t;
                // Picking Measure turns on what the tool is for, keeping
                // whatever curve analyses were already showing.
                if t == Tool::Measure && !self.view.measures() {
                    let measuring = canvas::editor::ViewOptions::measuring();
                    self.view = canvas::editor::ViewOptions {
                        comb: self.view.comb,
                        continuity: self.view.continuity,
                        ..measuring
                    };
                }
            }
            A::FlipHorizontal => self.apply_op(|s| s.flip_horizontal()),
            A::FlipVertical => self.apply_op(|s| s.flip_vertical()),
            A::Rotate90 => self.apply_op(|s| s.rotate_90()),
            A::RotateRight => {
                self.apply_op(|s| s.transform(kurbo::Affine::new([0.0, -1.0, 1.0, 0.0, 0.0, 0.0])));
            }
            A::Rotate180 => self.apply_op(|s| s.transform(kurbo::Affine::scale(-1.0))),
            A::RemoveOverlap => self.apply_op(|s| s.remove_overlap()),
            A::BooleanUnion => self.apply_op(|s| s.boolean(session::BoolOp::Union)),
            A::BooleanSubtract => self.apply_op(|s| s.boolean(session::BoolOp::Subtract)),
            A::BooleanIntersect => self.apply_op(|s| s.boolean(session::BoolOp::Intersect)),
            A::BooleanExclude => self.apply_op(|s| s.boolean(session::BoolOp::Exclude)),
            A::Decompose => self.apply_op(|s| s.decompose()),
            A::Duplicate => self.apply_op(|s| s.duplicate()),
            A::DuplicateRepeat => self.apply_op(|s| s.duplicate_repeat()),
            A::ReverseContours => self.apply_op(|s| s.reverse()),
            A::SetStartPoint => self.apply_op(|s| s.set_start()),
            A::TidyPaths => self.apply_op(|s| s.tidy_paths()),
            A::AddExtremes => self.apply_op(|s| s.add_extremes()),
            A::RoundCoordinates => self.apply_op(|s| s.round_coordinates()),
            A::CorrectPathDirection => self.apply_op(|s| s.correct_path_direction()),
            A::HyperToCubic => self.apply_op(|s| s.hyper_to_cubic()),
            A::QuadsToCubics => self.apply_op(|s| s.quads_to_cubics()),
            A::CubicsToQuads => self.apply_op(|s| s.cubics_to_quads()),
            A::RoundCorners => self.apply_op(|s| s.round_corners()),
            A::Harmonize => self.apply_op(|s| s.harmonize()),
            A::Balance => self.apply_op(|s| s.balance()),
            A::Optimize => self.apply_op(|s| s.optimize()),
            A::NewFont => self.new_font(),
            A::CycleTheme => self.cycle_theme(),
            A::Theme(id) => {
                self.theme_id = id;
                self.palette = Arc::new(Palette::load(id));
                self.cells = Arc::new(cells_of(&self.font, &self.palette));
            }
            A::ZoomToFit => {
                let mut session = (*self.session).clone();
                session.fitted = false;
                self.session = Arc::new(session);
            }
            A::ShowAllMasters => {
                self.show_all_masters = !self.show_all_masters;
                self.reference_layers.clear();
                if self.show_all_masters {
                    self.reference_layers.extend(
                        (0..self.font.master_count()).filter(|index| *index != self.font.active()),
                    );
                }
                self.note = if self.show_all_masters {
                    "showing all masters".into()
                } else {
                    "showing selected reference masters".into()
                };
            }
            A::NextMaster | A::PreviousMaster => {
                let count = self.font.master_count();
                if count > 1 {
                    let current = self.font.active();
                    let next = if matches!(action, A::NextMaster) {
                        (current + 1) % count
                    } else {
                        (current + count - 1) % count
                    };
                    self.set_master(next);
                }
            }
            A::NextSampleString | A::PreviousSampleString => {
                let count = SAMPLE_STRINGS.len();
                self.sample_index = if matches!(action, A::NextSampleString) {
                    (self.sample_index + 1) % count
                } else {
                    (self.sample_index + count - 1) % count
                };
                self.preview_text = SAMPLE_STRINGS[self.sample_index].into();
                self.note = format!("Sample: {}", self.preview_text);
            }
            A::MeasureColorize => self.view.colorize = !self.view.colorize,
            A::MeasureHandles => self.view.handles = !self.view.handles,
            A::MeasureSegments => self.view.segments = !self.view.segments,
            A::MeasureSizes => self.view.sizes = !self.view.sizes,
            A::MeasureSpans => self.view.spans = !self.view.spans,
            A::GridDots => self.view.grid_lines = false,
            A::GridLines => self.view.grid_lines = true,
            A::MeasureSideBearings => self.view.bearings = !self.view.bearings,
            A::MeasurePopcount => self.view.popcount = !self.view.popcount,
            A::MeasureAllOn => {
                self.view.colorize = true;
                self.view.handles = true;
                self.view.segments = true;
                self.view.sizes = true;
                self.view.spans = true;
                self.view.bearings = true;
            }
            A::MeasureAllOff => {
                self.view.colorize = false;
                self.view.handles = false;
                self.view.segments = false;
                self.view.sizes = false;
                self.view.spans = false;
                self.view.bearings = false;
            }
            A::GenerateMissing => match self.sel {
                Sel::Filter(i) => {
                    let missing = self.filter_missing(i);
                    if missing == 0 {
                        self.note = "nothing missing in this filter".into();
                    } else {
                        self.generate_missing(i);
                    }
                }
                _ => self.note = "select a coverage filter in the sidebar first".into(),
            },
            A::SortByName => self.sort = Sort::Name,
            A::SortByUnicode => self.sort = Sort::Unicode,
            A::NodesTab => self.enter_nodes_mode(),
            A::NodesNew => self.new_nodes_file(),
            A::NodesSave => self.save_nodes_file(),
            A::NodesRun => {
                if self.nodes.graph.is_none() {
                    self.enter_nodes_mode();
                }
                self.run_nodes();
            }
            A::Copy => self.copy_contours(),
            A::Paste => self.paste_contours(),
            A::SelectAll => {
                if matches!(self.mode, Mode::Editor(_)) {
                    let mut session = (*self.session).clone();
                    session.select_all();
                    self.selected_points = session.selection.len();
                    self.session = Arc::new(session);
                }
            }
            A::DeselectAll => {
                if matches!(self.mode, Mode::Editor(_)) {
                    let mut session = (*self.session).clone();
                    session.selection.clear();
                    self.selected_points = 0;
                    self.session = Arc::new(session);
                }
            }
            A::InvertSelection => {
                if matches!(self.mode, Mode::Editor(_)) {
                    let mut session = (*self.session).clone();
                    let all: std::collections::HashSet<_> =
                        session.points().into_iter().map(|point| point.id).collect();
                    session.selection = all.difference(&session.selection).copied().collect();
                    self.selected_points = session.selection.len();
                    self.session = Arc::new(session);
                }
            }
        }
    }

    /// Set the editor's zoom outright, for the slider in the bar.
    pub(crate) fn zoom_to(&mut self, zoom: f64) {
        let mut session = (*self.session).clone();
        session.viewport.zoom = zoom.clamp(0.02, 64.0);
        self.session = Arc::new(session);
    }

    /// Copy the selected contours, or all of them when nothing is
    /// selected.
    pub(crate) fn copy_contours(&mut self) {
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
    pub(crate) fn paste_contours(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) || self.clipboard.is_empty() {
            return;
        }
        let contours = self.clipboard.clone();
        self.apply_op(move |session| session.paste_contours(&contours));
        self.note = format!("pasted {} contours", self.clipboard.len());
    }

    /// Copy the open glyph's outline into the UFO background layer.
    pub(crate) fn send_to_background(&mut self) {
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
    pub(crate) fn swap_background(&mut self) {
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
    pub(crate) fn clear_background(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let name = self.session.glyph_name.clone();
        self.font.clear_background(&name);
        self.modified = true;
        self.note = "cleared background".into();
    }

    pub(crate) fn apply_op(&mut self, f: impl FnOnce(&mut Session) -> bool) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let mut sess = (*self.session).clone();
        if f(&mut sess) {
            self.sync_session_from(&mut sess);
            self.refresh_open_glyph();
        }
    }
}

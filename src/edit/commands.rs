// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! What the menus and shortcuts call. One method is the whole of one user-facing command.

use crate::*;

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
            A::Save => {
                // On the nodes canvas, Save writes the graph file too.
                if matches!(self.mode, Mode::Nodes) {
                    self.save_nodes_file();
                }
                self.save();
            }
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
            A::RemoveOverlap => self.apply_op(|s| s.remove_overlap()),
            A::Decompose => self.apply_op(|s| s.decompose()),
            A::Duplicate => self.apply_op(|s| s.duplicate()),
            A::NewFont => self.new_font(),
            A::CycleTheme => self.cycle_theme(),
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
            self.session = Arc::new(sess);
            self.refresh_open_glyph();
        }
    }
}

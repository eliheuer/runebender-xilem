// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The info panel's fields, and what typing in them does to the font.

use crate::*;

impl Workspace {
    /// What sits under the drawing: the background layer if it is turned
    /// on, and the reference glyph if one is named.
    pub(crate) fn underlay(&self) -> crate::view::canvas::editor::Underlay {
        if !matches!(self.mode, Mode::Editor(_)) {
            return crate::view::canvas::editor::Underlay::default();
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
        crate::view::canvas::editor::Underlay {
            background,
            reference,
        }
    }

    /// Recompute the LSB/RSB/advance text buffers from the current session.
    /// The selection's reference point, at the picked corner.
    pub(crate) fn coord_point(&self) -> Option<masonry::kurbo::Point> {
        let bounds = self.session.selection_bounds()?;
        Some(self.coord_quadrant.point_in_dspace_rect(bounds))
    }

    /// Refill the Coordinates fields from the selection.
    pub(crate) fn refresh_coord_bufs(&mut self) {
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
    pub(crate) fn set_coord(&mut self, axis: usize, v: String) {
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

    pub(crate) fn refresh_metric_bufs(&mut self) {
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
    pub(crate) fn set_kern_group(&mut self, first_side: bool, value: String) {
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
    pub(crate) fn set_lsb_from_buf(&mut self, v: String) {
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
    pub(crate) fn set_rsb_from_buf(&mut self, v: String) {
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

    pub(crate) fn set_unicode_from_buf(&mut self, v: String) {
        self.unicode_buf = v;
        let mut sess = (*self.session).clone();
        if sess.set_unicode(self.unicode_buf.trim()) {
            self.session = Arc::new(sess);
            self.refresh_open_glyph();
        }
    }

    pub(crate) fn commit_rename(&mut self) {
        let new = self.name_buf.trim().to_string();
        self.rename_to(&self.session.glyph_name.clone(), &new);
    }

    /// Rename `old` to `new` everywhere, and keep the interface pointing
    /// at the glyph rather than at the name it used to have.
    pub(crate) fn rename_to(&mut self, old: &str, new: &str) {
        if new.is_empty() || new == old {
            return;
        }
        if !self.font.rename_glyph(old, new) {
            return;
        }
        // Tabs address their glyph by name, so every tab showing the
        // old one has to learn the new one or it points at nothing.
        for tab in &mut self.tabs {
            if tab.session.glyph_name == old
                && let Some(session) = Session::new(&self.font.font, new)
            {
                tab.session = Arc::new(session);
            }
        }
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        if let Some(i) = self.font.index_of(new) {
            self.selected = Some(i);
            if matches!(self.mode, Mode::Editor(_)) {
                self.mode = Mode::Editor(i);
                if let Some(sess) = Session::new(&self.font.font, new) {
                    self.session = Arc::new(sess);
                }
            }
        }
        self.modified = true;
    }

    /// The overview panel writes to the highlighted cell, not to a
    /// session: in that mode no glyph is open. Each of these is the
    /// overview twin of an editor field.
    pub(crate) fn overview_rename(&mut self, v: String) {
        self.name_buf = v;
        let Some(old) = self
            .selected
            .and_then(|i| self.font.glyphs.get(i))
            .map(|g| g.name.clone())
        else {
            return;
        };
        let new = self.name_buf.trim().to_string();
        self.rename_to(&old, &new);
    }

    pub(crate) fn overview_set_unicode(&mut self, v: String) {
        self.unicode_buf = v;
        if let Some(i) = self.selected
            && self.font.set_glyph_unicode(i, &self.unicode_buf)
        {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.modified = true;
        }
    }

    pub(crate) fn overview_set_advance(&mut self, v: String) {
        self.advance_buf = v;
        let Ok(width) = self.advance_buf.trim().parse::<f64>() else {
            return;
        };
        if let Some(i) = self.selected
            && self.font.set_glyph_advance(i, width)
        {
            self.modified = true;
        }
    }

    pub(crate) fn set_mark(&mut self, label: Option<String>) {
        if !self.multi_selected.is_empty() {
            self.apply_mark_to_selection(label);
            return;
        }
        let mut sess = (*self.session).clone();
        sess.set_mark(label.as_deref());
        self.session = Arc::new(sess);
        self.refresh_open_glyph();
    }

    pub(crate) fn apply_mark_to_selection(&mut self, label: Option<String>) {
        let indices: Vec<usize> = self.multi_selected.iter().copied().collect();
        for i in indices {
            if let Some(entry) = self.font.glyphs.get(i)
                && let Some(mut g) = self.font.font.get_glyph(&entry.name).cloned()
            {
                runebender_core::ui::theme_oklch::set_glyph_mark(&mut g, label.as_deref());
                self.font.replace_glyph(i, g);
            }
        }
        self.modified = true;
    }

    pub(crate) fn set_advance_from_buf(&mut self, v: String) {
        self.advance_buf = v;
        if let Ok(w) = self.advance_buf.trim().parse::<f64>() {
            let mut sess = (*self.session).clone();
            sess.set_advance(w);
            self.session = Arc::new(sess);
            self.refresh_open_glyph();
        }
    }
}

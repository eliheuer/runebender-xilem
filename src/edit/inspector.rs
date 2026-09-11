// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The info panel's fields, and what typing in them does to the font.

use crate::*;
use runebender_core::outline::glyph_paths::round_units;

impl Workspace {
    /// What sits under the drawing: the background layer if it is turned
    /// on, and the reference glyph if one is named.
    pub(crate) fn underlay(&self) -> canvas::editor::Underlay {
        if !matches!(self.mode, Mode::Editor(_)) {
            return canvas::editor::Underlay::default();
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
        let proposal = self
            .ai
            .preview_task
            .as_deref()
            .and_then(|task| self.font.proposal_outline(task, &self.session.glyph_name))
            .map(Arc::new);
        canvas::editor::Underlay {
            background,
            reference,
            proposal,
        }
    }

    /// Recompute the LSB/RSB/advance text buffers from the current session.
    /// The selection's reference point, at the picked corner.
    pub(crate) fn coord_point(&self) -> Option<kurbo::Point> {
        let bounds = self.session.selection_bounds()?;
        Some(self.coord_quadrant.point_in_dspace_rect(bounds))
    }

    /// Refill the Coordinates fields from the selection.
    pub(crate) fn refresh_coord_bufs(&mut self) {
        if let Some(bounds) = self.session.selection_bounds() {
            self.coord_w_buf = format!("{}", round_units(bounds.width()));
            self.coord_h_buf = format!("{}", round_units(bounds.height()));
        } else {
            self.coord_w_buf.clear();
            self.coord_h_buf.clear();
        }
        match self.coord_point() {
            Some(p) => {
                self.coord_x_buf = format!("{}", round_units(p.x));
                self.coord_y_buf = format!("{}", round_units(p.y));
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

    /// Resize selected points about the chosen reference, through the undoable transform path.
    pub(crate) fn set_coord_size(&mut self, width: bool, value: String) {
        if width {
            self.coord_w_buf = value.clone();
        } else {
            self.coord_h_buf = value.clone();
        }
        let (Ok(target), Some(bounds)) =
            (value.trim().parse::<f64>(), self.session.selection_bounds())
        else {
            return;
        };
        let current = if width {
            bounds.width()
        } else {
            bounds.height()
        };
        if !target.is_finite() || target <= 0.0 || current.abs() < 1e-9 {
            return;
        }
        let scale = target / current;
        if !scale.is_finite() || (scale - 1.0).abs() < 1e-9 {
            return;
        }
        // Core's transform is in a frame centered on the selection bounds.
        let reference = self.coord_quadrant.point_in_dspace_rect(bounds) - bounds.center();
        let (sx, sy) = if width { (scale, 1.0) } else { (1.0, scale) };
        let transform = kurbo::Affine::translate(-reference)
            .then_scale_non_uniform(sx, sy)
            .then_translate(reference);
        self.apply_op(|session| session.transform(transform));
        self.refresh_coord_bufs();
    }

    pub(crate) fn refresh_metric_bufs(&mut self) {
        self.advance_buf = format!("{}", round_units(self.session.advance()));
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
        if let Ok(t) = self.lsb_buf.trim().parse::<f64>()
            && t.is_finite()
        {
            let mut sess = (*self.session).clone();
            if let Some(sb) = sess.side_bearings() {
                let delta = t - sb.min_x;
                if delta == 0.0 {
                    return;
                }
                sess.shift_glyph(delta);
                self.session = Arc::new(sess);
                self.refresh_open_glyph();
                self.advance_buf = format!("{}", round_units(self.session.advance()));
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
        if let Ok(t) = self.rsb_buf.trim().parse::<f64>()
            && t.is_finite()
        {
            let mut sess = (*self.session).clone();
            if let Some(sb) = sess.side_bearings() {
                let advance = (sb.max_x + t).max(0.0);
                if advance == sess.advance() {
                    return;
                }
                sess.set_advance(advance);
                self.session = Arc::new(sess);
                self.refresh_open_glyph();
                self.advance_buf = format!("{}", round_units(self.session.advance()));
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
            self.name_buf = old.to_string();
            self.note = format!("Cannot rename {old} to {new}");
            return;
        }
        // Tabs address their glyph by name, so every tab showing the
        // old one has to learn the new one or it points at nothing.
        for tab in &mut self.tabs {
            if tab.session.glyph_name == old
                && let Some(session) = Session::new(self.font.font(), new)
            {
                tab.session = Arc::new(session);
            }
        }
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        if let Some(i) = self.font.index_of(new) {
            self.selected = Some(i);
            if matches!(self.mode, Mode::Editor(_)) {
                self.mode = Mode::Editor(i);
                if let Some(sess) = Session::new(self.font.font(), new) {
                    self.session = Arc::new(sess);
                }
            }
        }
        self.modified = true;
        self.note = format!("Renamed {old} to {new}");
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
        if width.is_finite()
            && let Some(i) = self.selected
            && self.font.set_glyph_advance(i, width)
        {
            self.modified = true;
        }
    }

    pub(crate) fn set_mark(&mut self, label: Option<String>) {
        if matches!(self.mode, Mode::Overview) {
            let mut indices: Vec<usize> = if self.multi_selected.is_empty() {
                self.selected.into_iter().collect()
            } else {
                self.multi_selected.iter().copied().collect()
            };
            indices.sort_unstable();
            indices.retain(|index| {
                self.font
                    .glyphs
                    .get(*index)
                    .is_some_and(|glyph| glyph.mark.as_deref() != label.as_deref())
            });
            if indices.is_empty() {
                return;
            }
            for &index in &indices {
                self.font.master_mut().record_undo(index);
                self.font.master_mut().edit_glyph(index, |glyph| {
                    runebender_core::ui::theme::set_glyph_mark(glyph, label.as_deref());
                });
                self.font.refresh_entry(index);
            }
            self.overview_undo.push(OverviewEditBatch {
                master: self.font.active(),
                glyphs: indices
                    .iter()
                    .filter_map(|index| self.font.glyphs.get(*index))
                    .map(|glyph| glyph.name.clone())
                    .collect(),
            });
            self.overview_redo.clear();
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.modified = true;
            return;
        }
        let mut sess = (*self.session).clone();
        sess.set_mark(label.as_deref());
        self.session = Arc::new(sess);
        self.refresh_open_glyph();
    }

    pub(crate) fn set_advance_from_buf(&mut self, v: String) {
        self.advance_buf = v;
        if let Ok(w) = self.advance_buf.trim().parse::<f64>()
            && w.is_finite()
            && w.max(0.0) != self.session.advance()
        {
            let mut sess = (*self.session).clone();
            sess.set_advance(w);
            self.session = Arc::new(sess);
            self.refresh_open_glyph();
        }
    }
}

impl Workspace {
    /// The grid selection as names, the primary included.
    fn selection_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .multi_selected
            .iter()
            .filter_map(|&i| self.font.glyphs.get(i).map(|g| g.name.clone()))
            .collect();
        if let Some(name) = self
            .selected
            .and_then(|i| self.font.glyphs.get(i))
            .map(|g| g.name.clone())
            && !names.contains(&name)
        {
            names.push(name);
        }
        names.sort();
        names
    }

    /// Drop one kerning pair, on every master.
    pub(crate) fn delete_kern_pair(&mut self, first: &str, second: &str) {
        self.font.for_each_master(|font| {
            let mut emptied = false;
            if let Some(seconds) = font.kerning.get_mut(first) {
                seconds.retain(|name, _| name.as_str() != second);
                emptied = seconds.is_empty();
            }
            if emptied {
                font.kerning.retain(|name, _| name.as_str() != first);
            }
        });
        self.modified = true;
    }

    /// Set the pair in the Kerning section's editor row, on the
    /// active master. Enter in any of its three fields.
    pub(crate) fn set_kern_pair_from_bufs(&mut self) {
        let first = self.kern_first_buf.trim().to_string();
        let second = self.kern_second_buf.trim().to_string();
        let Ok(value) = self.kern_value_buf.trim().parse::<f64>() else {
            self.note = "kerning value is not a number".into();
            return;
        };
        let (Ok(f), Ok(s)) = (norad::Name::new(&first), norad::Name::new(&second)) else {
            self.note = "a kerning pair needs two names".into();
            return;
        };
        let font = self.font.font_mut();
        font.kerning.entry(f).or_default().insert(s, value);
        self.modified = true;
        self.note = format!("{first} \u{00b7} {second} = {value}");
    }

    /// Add the grid selection to a kerning group, on every master.
    pub(crate) fn add_selection_to_group(&mut self, first_side: bool, group: &str) {
        let names = self.selection_names();
        if names.is_empty() {
            self.note = "Select glyphs in the grid first".into();
            return;
        }
        let prefix = if first_side {
            "public.kern1."
        } else {
            "public.kern2."
        };
        let Ok(group_name) = norad::Name::new(&format!("{prefix}{group}")) else {
            return;
        };
        let mut added = 0_usize;
        self.font.for_each_master(|font| {
            let members = font.groups.entry(group_name.clone()).or_default();
            for name in &names {
                if let Ok(member) = norad::Name::new(name)
                    && !members.contains(&member)
                {
                    members.push(member);
                    added += 1;
                }
            }
        });
        self.modified = true;
        self.note = format!("@{group}: {added} membership(s) added");
    }

    /// Drop one glyph from a kerning group, on every master. An
    /// emptied group is removed.
    pub(crate) fn remove_from_group(&mut self, full_group: &str, member: &str) {
        self.font.for_each_master(|font| {
            let mut emptied = false;
            if let Some(members) = font.groups.get_mut(full_group) {
                members.retain(|m| m.as_str() != member);
                emptied = members.is_empty();
            }
            if emptied {
                font.groups.retain(|k, _| k.as_str() != full_group);
            }
        });
        self.modified = true;
    }

    /// A new left-side group from the Groups field, holding the grid
    /// selection.
    pub(crate) fn new_group_from_buf(&mut self) {
        let group = self
            .group_name_buf
            .trim()
            .trim_start_matches('@')
            .to_string();
        if group.is_empty() {
            return;
        }
        self.add_selection_to_group(true, &group);
        self.group_name_buf.clear();
    }

    /// Replace the generated mark and mkmk lookups in the feature
    /// file with what core derives from the anchors now.
    pub(crate) fn generate_features(&mut self) {
        let fea = runebender_core::text::features::with_generated(self.font.font());
        if fea == self.font.font().features {
            self.features_status = Some("Nothing to generate from anchors".into());
            return;
        }
        self.font.font_mut().features = fea;
        self.modified = true;
        self.features_status = Some("Generated mark and mkmk from anchors".into());
    }

    /// The feature file as it stands is what shaping reads; here that
    /// means noting it, since the text is not edited in place yet.
    pub(crate) fn apply_features(&mut self) {
        self.features_status = Some("Applied".into());
        self.modified = true;
    }
}

#[cfg(test)]
mod size_tests {
    use super::*;

    #[test]
    fn overview_mark_batch_updates_cells_and_undoes_once() {
        let path =
            std::env::temp_dir().join(format!("runebender-mark-test-{}.ufo", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        let mut font = norad::Font::new();
        for name in ["mark_a", "mark_b"] {
            font.default_layer_mut()
                .insert_glyph(norad::Glyph::new(name));
        }
        font.save(&path).expect("save disposable test font");
        let mut app = Workspace::open(&path).expect("open test font");
        let a = app.font.index_of("mark_a").expect("mark_a");
        let b = app.font.index_of("mark_b").expect("mark_b");
        app.selected = Some(a);
        app.multi_selected = Arc::new([a, b].into_iter().collect());

        app.set_mark(Some("blue".into()));
        assert_eq!(app.font.glyphs[a].mark.as_deref(), Some("blue"));
        assert_eq!(app.font.glyphs[b].mark.as_deref(), Some("blue"));
        assert!(app.cells[a].mark.is_some() && app.cells[b].mark.is_some());
        assert_eq!(app.overview_undo[0].master, app.font.active());
        assert_eq!(app.overview_undo[0].glyphs, vec!["mark_a", "mark_b"]);

        app.undo_active_edit(false);
        assert!(app.font.glyphs[a].mark.is_none());
        assert!(app.font.glyphs[b].mark.is_none());
        assert!(app.cells[a].mark.is_none() && app.cells[b].mark.is_none());
        assert_eq!(app.overview_redo[0].glyphs, vec!["mark_a", "mark_b"]);

        app.undo_active_edit(true);
        assert_eq!(app.font.glyphs[a].mark.as_deref(), Some("blue"));
        assert_eq!(app.font.glyphs[b].mark.as_deref(), Some("blue"));
        assert_eq!(app.overview_undo[0].glyphs, vec!["mark_a", "mark_b"]);

        app.set_mark(Some("blue".into()));
        assert_eq!(app.overview_undo.len(), 1, "a no-op adds no undo step");
        std::fs::remove_dir_all(path).expect("remove disposable font");
    }

    #[test]
    fn dimensions_keep_reference_and_support_undo() {
        let path =
            std::env::temp_dir().join(format!("runebender-size-test-{}.ufo", std::process::id()));
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("size_test");
        let mut contour = norad::Contour::default();
        for (x, y) in [(20.0, 30.0), (120.0, 30.0), (120.0, 230.0), (20.0, 230.0)] {
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
        font.save(&path).expect("save disposable test font");
        let mut app = Workspace::open(&path).expect("open test font");
        app.open_glyph(app.font.index_of("size_test").expect("test glyph"));
        let mut session = (*app.session).clone();
        session.select_all();
        app.session = Arc::new(session);
        app.coord_quadrant = runebender_core::outline::path::Quadrant::TopRight;
        let reference = app.coord_point().expect("selected points");
        app.set_coord_size(true, "250".into());
        assert_eq!(app.session.selection_bounds().unwrap().width(), 250.0);
        assert_eq!(app.coord_point(), Some(reference));
        app.set_coord_size(false, "400".into());
        assert_eq!(app.session.selection_bounds().unwrap().height(), 400.0);
        assert_eq!(app.coord_point(), Some(reference));
        let before = app.session.glyph.clone();
        for invalid in ["NaN", "inf", "0", "-10", "-"] {
            app.set_coord_size(true, invalid.into());
            assert_eq!(app.session.glyph, before);
        }
        app.undo_open_glyph(false);
        let points = &app.session.glyph.contours[0].points;
        assert_eq!(points[2].y - points[0].y, 200.0);
        std::fs::remove_dir_all(path).expect("remove disposable font");
    }
}

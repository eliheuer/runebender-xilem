// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The info panel's fields, and what typing in them does to the font.

use crate::application::editor::session::Session;
use crate::application::font_model::FontModel;
use crate::application::view::canvas;
use crate::application::view::canvas::grid::cells_of;
use crate::application::view::panels::sections::metric_bufs;
use crate::application::workspace::{MetadataEdit, Mode, OverviewEditBatch, Workspace};
use runebender::document::CanonicalSourceMetadataSnapshot;
use runebender::document::canonical_metadata::{KerningParticipant, KerningSide};
use runebender::document::history::HistoryDirection;
use runebender::document::project::DocumentHistoryReplayOutcome;
use runebender::outline::glyph_paths::round_units;
use std::sync::Arc;

fn kerning_participant(raw: &str, side: KerningSide) -> Option<KerningParticipant> {
    if raw.starts_with(side.prefix()) {
        KerningParticipant::group(side, raw).ok()
    } else {
        KerningParticipant::glyph(raw).ok()
    }
}

fn mark_cloud(font: &FontModel, base: &norad::Glyph) -> Vec<Arc<kurbo::BezPath>> {
    let base_anchors: Vec<_> = base
        .anchors
        .iter()
        .filter_map(|anchor| {
            Some((
                anchor.name.as_ref()?.to_string(),
                kurbo::Point::new(anchor.x, anchor.y),
            ))
        })
        .collect();
    let mut placed = Vec::new();
    'candidates: for entry in &font.glyphs {
        let Some(candidate) = font.font().get_glyph(&entry.name) else {
            continue;
        };
        for anchor in &candidate.anchors {
            let Some(mark_name) = anchor.name.as_ref().and_then(|name| name.strip_prefix('_'))
            else {
                continue;
            };
            let Some((_, target)) = base_anchors.iter().find(|(name, _)| name == mark_name) else {
                continue;
            };
            if entry.outline.elements().is_empty() {
                continue;
            }
            placed.push(Arc::new(
                kurbo::Affine::translate((target.x - anchor.x, target.y - anchor.y))
                    * (*entry.outline).clone(),
            ));
            if placed.len() >= 60 {
                break 'candidates;
            }
            continue 'candidates;
        }
    }
    placed
}

impl Workspace {
    fn font_data_history_context(
        &self,
    ) -> Option<(String, usize, CanonicalSourceMetadataSnapshot)> {
        let glyph = match self.mode {
            Mode::Editor(_) => self.session.glyph_name.clone(),
            Mode::Overview => self
                .selected
                .and_then(|index| self.font.glyphs.get(index))?
                .name
                .clone(),
            Mode::Nodes => return None,
        };
        let undo_depth = self
            .font
            .index_of(&glyph)
            .map_or(0, |index| self.font.master().undo_depth(index));
        Some((
            glyph,
            undo_depth,
            self.font.project.begin_document_source_metadata_history(),
        ))
    }

    fn finish_font_data_history(
        &mut self,
        context: Option<(String, usize, CanonicalSourceMetadataSnapshot)>,
        label: &str,
    ) {
        let Some((glyph, undo_depth, before)) = context else {
            return;
        };
        if !self
            .font
            .project
            .record_document_source_metadata_history(before)
        {
            return;
        }
        self.metadata_undo.push(MetadataEdit::SourceMetadata {
            glyph,
            label: label.into(),
            undo_depth,
        });
        self.metadata_redo.clear();
    }

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
        let mark_cloud = if self.show_mark_cloud {
            mark_cloud(&self.font, &self.session.glyph)
        } else {
            Vec::new()
        };
        canvas::editor::Underlay {
            background,
            reference,
            mark_cloud,
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
        // The engine transform is in a frame centered on the selection bounds.
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
        let history = self.font_data_history_context();
        if self.font.set_kern_group(&name, first_side, value.trim()) {
            self.modified = true;
            self.finish_font_data_history(history, "kerning group");
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
        let name = self.session.glyph_name.clone();
        self.set_unicode_across_masters(&name);
    }

    fn set_unicode_across_masters(&mut self, name: &str) {
        let Some(before) = self.font.glyph_codepoints(name) else {
            return;
        };
        let Some(glyph) = self.font.font().get_glyph(name) else {
            return;
        };
        let mut parsed = glyph.clone();
        if !runebender::document::font_ops::set_glyph_unicode(&mut parsed, self.unicode_buf.trim())
        {
            return;
        }
        let codepoints: Vec<char> = parsed.codepoints.iter().collect();
        let after = vec![codepoints; before.len()];
        if before == after {
            return;
        }
        let undo_depth = self
            .font
            .index_of(name)
            .map_or(0, |index| self.font.master().undo_depth(index));
        if self.apply_unicode_snapshot(name, &after) {
            self.metadata_undo.push(MetadataEdit::Unicode {
                source_ids: (0..self.font.master_count())
                    .map(|index| self.font.project.source_id(index).expect("source identity"))
                    .collect(),
                glyph: name.into(),
                before,
                after,
                undo_depth,
            });
            self.metadata_redo.clear();
            self.note = format!("Updated Unicode for {name}");
        }
    }

    fn apply_unicode_snapshot(&mut self, name: &str, values: &[Vec<char>]) -> bool {
        if !self.font.set_glyph_codepoints(name, values) {
            return false;
        }
        let active = values.get(self.font.active()).cloned().unwrap_or_default();
        if self.session.glyph_name == name {
            Arc::make_mut(&mut self.session).glyph.codepoints =
                norad::Codepoints::new(active.iter().copied());
        }
        for tab in &mut self.tabs {
            if tab.session.glyph_name == name {
                Arc::make_mut(&mut tab.session).glyph.codepoints =
                    norad::Codepoints::new(active.iter().copied());
            }
        }
        self.selected = self.font.index_of(name);
        if matches!(self.mode, Mode::Editor(_))
            && let Some(index) = self.selected
        {
            self.mode = Mode::Editor(index);
        }
        self.unicode_buf = active
            .first()
            .map(|codepoint| format!("{:04X}", *codepoint as u32))
            .unwrap_or_default();
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.modified = true;
        true
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
        let undo_depth = self
            .font
            .index_of(old)
            .map_or(0, |index| self.font.master().undo_depth(index));
        if self.rename_without_history(old, new) {
            self.metadata_undo.push(MetadataEdit::Rename {
                before: old.into(),
                after: new.into(),
                undo_depth,
            });
            self.metadata_redo.clear();
        }
    }

    /// Apply an already validated rename without creating another rename step.
    fn rename_without_history(&mut self, old: &str, new: &str) -> bool {
        if !self.font.rename_glyph(old, new) {
            self.name_buf = old.to_string();
            self.note = format!("Cannot rename {old} to {new}");
            return false;
        }
        // Tabs address their glyph by name, so every tab showing the
        // old one has to learn the new one or it points at nothing.
        for tab in &mut self.tabs {
            if tab.session.glyph_name == old
                && let Some(session) = Session::new_from_model(&self.font, new)
            {
                tab.session = Arc::new(session);
            }
        }
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        if let Some(i) = self.font.index_of(new) {
            self.selected = Some(i);
            if matches!(self.mode, Mode::Editor(_)) {
                self.mode = Mode::Editor(i);
                if let Some(sess) = Session::new_from_model(&self.font, new) {
                    self.session = Arc::new(sess);
                }
            }
        }
        self.modified = true;
        self.note = format!("Renamed {old} to {new}");
        true
    }

    /// Undo or redo metadata when it is next in the active glyph's history.
    pub(crate) fn metadata_history_step(&mut self, redo: bool) -> bool {
        let candidate = if redo {
            self.metadata_redo.last()
        } else {
            self.metadata_undo.last()
        }
        .cloned();
        let Some(edit) = candidate else {
            return false;
        };
        let (expected, undo_depth) = match &edit {
            MetadataEdit::Rename {
                before,
                after,
                undo_depth,
            } => (if redo { before } else { after }, *undo_depth),
            MetadataEdit::Unicode {
                glyph, undo_depth, ..
            }
            | MetadataEdit::SourceMetadata {
                glyph, undo_depth, ..
            }
            | MetadataEdit::DocumentLayer {
                glyph, undo_depth, ..
            } => (glyph, *undo_depth),
        };
        let current_name = match self.mode {
            Mode::Editor(_) => Some(self.session.glyph_name.as_str()),
            Mode::Overview => self
                .selected
                .and_then(|index| self.font.glyphs.get(index))
                .map(|glyph| glyph.name.as_str()),
            Mode::Nodes => None,
        };
        if current_name != Some(expected.as_str()) {
            return false;
        }
        let Some(index) = self.font.index_of(expected) else {
            return false;
        };
        let depth = self.font.master().undo_depth(index);
        if depth != undo_depth {
            // A lower depth means an older glyph edit must redo first; a
            // higher depth means a later edit must undo first.
            return false;
        }
        let note = match &edit {
            MetadataEdit::Rename { before, after, .. } => {
                let (old, new) = if redo {
                    (before, after)
                } else {
                    (after, before)
                };
                if !self.rename_without_history(old, new) {
                    return false;
                }
                format!("{} rename to {new}", if redo { "Redid" } else { "Undid" })
            }
            MetadataEdit::Unicode {
                glyph,
                source_ids,
                before,
                after,
                ..
            } => {
                let values = if redo { after } else { before };
                let Some(values) = self.reorder_source_snapshot(source_ids, values) else {
                    return false;
                };
                if !self.apply_unicode_snapshot(glyph, &values) {
                    return false;
                }
                format!(
                    "{} Unicode for {glyph}",
                    if redo { "Redid" } else { "Undid" }
                )
            }
            MetadataEdit::SourceMetadata { label, .. } => {
                let direction = if redo {
                    HistoryDirection::Redo
                } else {
                    HistoryDirection::Undo
                };
                let Ok(DocumentHistoryReplayOutcome::Changed { .. }) = self
                    .font
                    .project
                    .replay_document_source_metadata_history(direction)
                else {
                    return false;
                };
                self.features_buf = self.font.feature_text().to_owned();
                self.features_edited = false;
                self.refresh_metric_bufs();
                self.modified = true;
                format!("{} {label}", if redo { "Redid" } else { "Undid" })
            }
            MetadataEdit::DocumentLayer { address, label, .. } => {
                let direction = if redo {
                    HistoryDirection::Redo
                } else {
                    HistoryDirection::Undo
                };
                let Ok(DocumentHistoryReplayOutcome::Changed { .. }) = self
                    .font
                    .project
                    .replay_document_layer_history(address, direction)
                else {
                    return false;
                };
                if !self.reload_canonical_layer(address) {
                    return false;
                }
                format!("{} {label}", if redo { "Redid" } else { "Undid" })
            }
        };
        if redo {
            self.metadata_redo.pop();
            self.metadata_undo.push(edit);
        } else {
            self.metadata_undo.pop();
            self.metadata_redo.push(edit);
        }
        self.note = note;
        true
    }

    pub(crate) fn can_metadata_history_step(&self, redo: bool) -> bool {
        let candidate = if redo {
            self.metadata_redo.last()
        } else {
            self.metadata_undo.last()
        };
        let Some(edit) = candidate else {
            return false;
        };
        let (expected, undo_depth) = match edit {
            MetadataEdit::Rename {
                before,
                after,
                undo_depth,
            } => (if redo { before } else { after }, *undo_depth),
            MetadataEdit::Unicode {
                glyph, undo_depth, ..
            }
            | MetadataEdit::SourceMetadata {
                glyph, undo_depth, ..
            }
            | MetadataEdit::DocumentLayer {
                glyph, undo_depth, ..
            } => (glyph, *undo_depth),
        };
        let current_name = match self.mode {
            Mode::Editor(_) => Some(self.session.glyph_name.as_str()),
            Mode::Overview => self
                .selected
                .and_then(|index| self.font.glyphs.get(index))
                .map(|glyph| glyph.name.as_str()),
            Mode::Nodes => None,
        };
        let source_metadata_available = !matches!(edit, MetadataEdit::SourceMetadata { .. })
            || self
                .font
                .project
                .can_replay_document_source_metadata_history(if redo {
                    HistoryDirection::Redo
                } else {
                    HistoryDirection::Undo
                });
        let document_layer_available = match edit {
            MetadataEdit::DocumentLayer { address, .. } => {
                self.font.project.can_replay_document_layer_history(
                    address,
                    if redo {
                        HistoryDirection::Redo
                    } else {
                        HistoryDirection::Undo
                    },
                )
            }
            _ => true,
        };
        source_metadata_available
            && document_layer_available
            && current_name == Some(expected.as_str())
            && self
                .font
                .index_of(expected)
                .is_some_and(|index| self.font.master().undo_depth(index) == undo_depth)
    }

    fn reorder_source_snapshot<T: Clone>(
        &self,
        ids: &[runebender::document::variable::SourceId],
        values: &[T],
    ) -> Option<Vec<T>> {
        if ids.len() != self.font.master_count() {
            return None;
        }
        (0..self.font.master_count())
            .map(|index| {
                let id = self.font.project.source_id(index)?;
                let original = ids.iter().position(|candidate| *candidate == id)?;
                values.get(original).cloned()
            })
            .collect()
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
        let name = self
            .selected
            .and_then(|index| self.font.glyphs.get(index))
            .map(|glyph| glyph.name.clone());
        if let Some(name) = name {
            self.set_unicode_across_masters(&name);
        }
    }

    pub(crate) fn overview_set_advance(&mut self, v: String) {
        self.advance_buf = v;
        let Ok(width) = self.advance_buf.trim().parse::<f64>() else {
            return;
        };
        if width.is_finite()
            && let Some(i) = self.selected
        {
            let glyph = self.font.glyphs[i].name.clone();
            self.font.master_mut().record_undo(i);
            if self.font.set_glyph_advance(i, width) {
                self.overview_undo.push(OverviewEditBatch {
                    source: self
                        .font
                        .project
                        .source_id(self.font.active())
                        .expect("active source identity"),
                    glyphs: vec![glyph],
                });
                self.overview_redo.clear();
                self.modified = true;
            } else {
                self.font.master_mut().discard_last_undo(i);
            }
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
                    runebender::ui::theme::set_glyph_mark(glyph, label.as_deref());
                });
                self.font.refresh_entry(index);
            }
            self.overview_undo.push(OverviewEditBatch {
                source: self
                    .font
                    .project
                    .source_id(self.font.active())
                    .expect("active source identity"),
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

    /// Drop one kerning pair from the active master.
    pub(crate) fn delete_kern_pair(&mut self, first: &str, second: &str) {
        let history = self.font_data_history_context();
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            return;
        };
        let (Some(first), Some(second)) = (
            kerning_participant(first, KerningSide::First),
            kerning_participant(second, KerningSide::Second),
        ) else {
            return;
        };
        let Some(mut metadata) = self.font.project.document_font_metadata(source).cloned() else {
            return;
        };
        if metadata.set_kerning_pair(first.clone(), second.clone(), None) != Ok(true) {
            return;
        }
        if !matches!(
            self.font
                .project
                .edit_document_source_metadata(source, |draft| {
                    draft.set_font_metadata(metadata);
                    Ok(())
                }),
            Ok(runebender::document::project::DocumentEditOutcome::Changed { .. })
        ) {
            return;
        }
        self.modified = true;
        self.note = format!("Removed {} · {}", first.as_raw_name(), second.as_raw_name());
        self.finish_font_data_history(history, "kerning pair deletion");
    }

    /// Set the pair in the Kerning section's editor row, on the
    /// active master. Enter in any of its three fields.
    pub(crate) fn set_kern_pair_from_bufs(&mut self) {
        let history = self.font_data_history_context();
        let first = self.kern_first_buf.trim().to_string();
        let second = self.kern_second_buf.trim().to_string();
        let Ok(value) = self.kern_value_buf.trim().parse::<f64>() else {
            self.note = "kerning value is not a number".into();
            return;
        };
        if !value.is_finite() {
            self.note = "kerning value must be finite".into();
            return;
        }
        let (Some(first_participant), Some(second_participant)) = (
            kerning_participant(&first, KerningSide::First),
            kerning_participant(&second, KerningSide::Second),
        ) else {
            self.note = "a kerning pair needs two names".into();
            return;
        };
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            return;
        };
        let Some(mut metadata) = self.font.project.document_font_metadata(source).cloned() else {
            return;
        };
        match metadata.set_kerning_pair(first_participant, second_participant, Some(value)) {
            Ok(false) => {
                self.note = format!("{first} · {second} already equals {value}");
                return;
            }
            Ok(true) => {}
            Err(error) => {
                self.note = error.to_string();
                return;
            }
        }
        if !matches!(
            self.font
                .project
                .edit_document_source_metadata(source, |draft| {
                    draft.set_font_metadata(metadata);
                    Ok(())
                }),
            Ok(runebender::document::project::DocumentEditOutcome::Changed { .. })
        ) {
            self.note = format!("{first} · {second} already equals {value}");
            return;
        }
        self.modified = true;
        self.note = format!("{first} \u{00b7} {second} = {value}");
        self.finish_font_data_history(history, "kerning pair");
    }

    /// Add the grid selection to a kerning group, on every master.
    pub(crate) fn add_selection_to_group(&mut self, first_side: bool, group: &str) {
        let history = self.font_data_history_context();
        let names = self.selection_names();
        if names.is_empty() {
            self.note = "Select glyphs in the grid first".into();
            return;
        }
        let side = if first_side {
            KerningSide::First
        } else {
            KerningSide::Second
        };
        let mut added = 0_usize;
        let sources: Vec<_> = self
            .font
            .project
            .document_sources()
            .map(|source| source.id())
            .collect();
        for source in sources {
            let Some(mut metadata) = self.font.project.document_font_metadata(source).cloned()
            else {
                continue;
            };
            let mut source_added = 0_usize;
            for name in &names {
                if metadata.set_kerning_group(name, side, Some(group)) == Ok(true) {
                    source_added += 1;
                }
            }
            if source_added > 0
                && matches!(
                    self.font
                        .project
                        .edit_document_source_metadata(source, |draft| {
                            draft.set_font_metadata(metadata);
                            Ok(())
                        }),
                    Ok(runebender::document::project::DocumentEditOutcome::Changed { .. })
                )
            {
                added += source_added;
            }
        }
        if added == 0 {
            self.note = format!("@{group}: selection already present");
            return;
        }
        self.modified = true;
        self.note = format!("@{group}: {added} membership(s) added");
        self.finish_font_data_history(history, "kerning group membership");
    }

    /// Drop one glyph from a kerning group, on every master. An
    /// emptied group is removed.
    pub(crate) fn remove_from_group(&mut self, full_group: &str, member: &str) {
        let history = self.font_data_history_context();
        let mut removed = 0_usize;
        let sources: Vec<_> = self
            .font
            .project
            .document_sources()
            .map(|source| source.id())
            .collect();
        for source in sources {
            let Some(mut metadata) = self.font.project.document_font_metadata(source).cloned()
            else {
                continue;
            };
            let Some(current) = metadata.groups().get(full_group) else {
                continue;
            };
            let members: Vec<_> = current
                .iter()
                .filter(|candidate| candidate.as_str() != member)
                .cloned()
                .collect();
            if members.len() == current.len() {
                continue;
            }
            let changed = if members.is_empty() {
                metadata.remove_group(full_group)
            } else {
                metadata.set_group(full_group, members)
            };
            if changed == Ok(true)
                && matches!(
                    self.font
                        .project
                        .edit_document_source_metadata(source, |draft| {
                            draft.set_font_metadata(metadata);
                            Ok(())
                        }),
                    Ok(runebender::document::project::DocumentEditOutcome::Changed { .. })
                )
            {
                removed += 1;
            }
        }
        if removed == 0 {
            return;
        }
        self.modified = true;
        self.note = format!("Removed {member} from {removed} group membership(s)");
        self.finish_font_data_history(history, "kerning group membership");
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

    /// Update the font-wide feature draft without applying it.
    pub(crate) fn edit_features(&mut self, value: String) {
        self.features_buf = value;
        self.features_edited = self.features_buf != self.font.feature_text();
        self.modified |= self.features_edited;
        self.features_status = None;
    }

    /// Discard the feature draft and restore the font's applied text.
    pub(crate) fn revert_features(&mut self) {
        self.features_buf = self.font.feature_text().to_owned();
        self.features_edited = false;
        self.modified = self
            .font
            .project
            .sources()
            .iter()
            .any(|master| master.dirty);
        self.features_status = Some("Reverted feature draft".into());
    }

    /// Put generated mark and mkmk lookups in the feature draft for review.
    pub(crate) fn generate_features(&mut self) {
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            self.features_status = Some("The active source is unavailable".into());
            return;
        };
        let Some(layer) = self
            .font
            .project
            .document_source(source)
            .map(|source| source.default_layer())
        else {
            self.features_status = Some("The active source layer is unavailable".into());
            return;
        };
        let fea = runebender::text::features::with_generated_document(
            &self.features_buf,
            self.font
                .project
                .glyph_names()
                .filter_map(|name| self.font.project.document_layer(name, &layer)),
            |name| self.font.project.document_layer(name, &layer),
        );
        if fea == self.features_buf {
            self.features_status = Some("Nothing to generate from anchors".into());
            return;
        }
        self.edit_features(fea);
        self.features_status = Some("Generated mark and mkmk · review and Apply".into());
    }

    fn feature_compile_verdict(&self, features: &str) -> Result<(), String> {
        self.font.project.check_features(features)
    }

    fn feature_verdict_status(prefix: &str, verdict: Result<(), String>) -> String {
        match verdict {
            Ok(()) => format!("{prefix} · compiled clean · shaping updated"),
            Err(error) => {
                let first = error
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .unwrap_or("feature compile error");
                format!("{prefix}, but does not compile: {first}")
            }
        }
    }

    /// Compile-check the current draft without applying or dirtying it further.
    pub(crate) fn check_features(&mut self) {
        let verdict = self.feature_compile_verdict(&self.features_buf);
        self.features_status = Some(Self::feature_verdict_status("Checked", verdict));
    }

    /// Apply the shared feature draft to its default source and refresh shaping data.
    pub(crate) fn apply_features(&mut self) {
        let verdict = self.feature_compile_verdict(&self.features_buf);
        if !self.features_edited {
            self.features_status = Some(Self::feature_verdict_status("Already applied", verdict));
            return;
        }
        let history = self.font_data_history_context();
        self.font
            .project
            .set_feature_text(self.features_buf.clone());
        self.features_edited = false;
        self.modified = true;
        self.finish_font_data_history(history, "feature text");
        self.features_status = Some(Self::feature_verdict_status("Applied", verdict));
    }
}

#[cfg(test)]
mod size_tests {
    use super::*;

    fn disposable_font(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "runebender-{name}-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ))
    }

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
        assert_eq!(
            Some(app.overview_undo[0].source),
            app.font.project.source_id(app.font.active())
        );
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
    fn mark_cloud_places_only_marks_with_matching_anchors() {
        use kurbo::Shape as _;

        let path = disposable_font("mark-cloud");
        let mut font = norad::Font::new();
        let mut base = norad::Glyph::new("base");
        base.anchors.push(norad::Anchor::new(
            300.0,
            500.0,
            norad::Name::new("top").ok(),
            None,
            None,
        ));
        font.default_layer_mut().insert_glyph(base);
        for (name, anchor_name) in [("acute", "_top"), ("cedilla", "_bottom")] {
            let mut mark = norad::Glyph::new(name);
            mark.anchors.push(norad::Anchor::new(
                20.0,
                30.0,
                norad::Name::new(anchor_name).ok(),
                None,
                None,
            ));
            mark.contours.push(norad::Contour::new(
                vec![
                    norad::ContourPoint::new(10.0, 20.0, norad::PointType::Line, false, None, None),
                    norad::ContourPoint::new(30.0, 20.0, norad::PointType::Line, false, None, None),
                    norad::ContourPoint::new(20.0, 40.0, norad::PointType::Line, false, None, None),
                ],
                None,
            ));
            font.default_layer_mut().insert_glyph(mark);
        }
        font.save(&path).expect("save mark-cloud fixture");
        let mut app = Workspace::open(&path).expect("open mark-cloud fixture");
        let base = app.font.index_of("base").unwrap();
        app.open_glyph(base);
        app.show_mark_cloud = true;

        let cloud = app.underlay().mark_cloud;
        assert_eq!(cloud.len(), 1);
        let bounds = cloud[0].bounding_box();
        assert_eq!(bounds, kurbo::Rect::new(290.0, 490.0, 310.0, 510.0));
        std::fs::remove_dir_all(path).expect("remove mark-cloud fixture");
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
        app.coord_quadrant = runebender::outline::path::Quadrant::TopRight;
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

    #[test]
    fn kerning_and_groups_validate_refresh_shaping_and_roundtrip() {
        use crate::application::editor::tools::text::{TextInputs, TextState};

        let path = disposable_font("kerning-roundtrip");
        let mut font = norad::Font::new();
        for (name, codepoint) in [("A", 'A'), ("V", 'V')] {
            let mut glyph = norad::Glyph::new(name);
            glyph.width = 500.0;
            glyph.codepoints.insert(codepoint);
            font.default_layer_mut().insert_glyph(glyph);
        }
        font.save(&path).expect("save disposable test font");
        let mut app = Workspace::open(&path).expect("open test font");
        let a = app.font.index_of("A").expect("A");
        app.selected = Some(a);

        app.open_glyph(a);
        app.set_kern_group(true, "A".into());
        assert_eq!(app.font.kern_group("A", true), "public.kern1.A");
        assert_eq!(
            app.font
                .project
                .document_source_metadata_history_depth(HistoryDirection::Undo),
            1
        );
        assert!(app.font.master().dirty);
        assert!(app.font.master().kerning_dirty);
        app.undo_active_edit(false);
        assert_eq!(app.font.kern_group("A", true), "");
        assert_eq!(
            app.font
                .project
                .document_source_metadata_history_depth(HistoryDirection::Redo),
            1
        );
        app.undo_active_edit(true);
        assert_eq!(app.font.kern_group("A", true), "public.kern1.A");

        app.kern_first_buf = "public.kern1.A".into();
        app.kern_second_buf = "V".into();
        app.kern_value_buf = "-80".into();
        app.set_kern_pair_from_bufs();
        let state = TextState::new(&TextInputs::new(&app.font).with_text("AV"));
        assert_eq!(state.buffer.layout(state.line_height).items[1].x, 420.0);
        app.undo_active_edit(false);
        let state = TextState::new(&TextInputs::new(&app.font).with_text("AV"));
        assert_eq!(state.buffer.layout(state.line_height).items[1].x, 500.0);
        app.undo_active_edit(true);
        let state = TextState::new(&TextInputs::new(&app.font).with_text("AV"));
        assert_eq!(state.buffer.layout(state.line_height).items[1].x, 420.0);

        let history_len = app.metadata_undo.len();
        app.kern_value_buf = "NaN".into();
        app.set_kern_pair_from_bufs();
        assert_eq!(app.note, "kerning value must be finite");
        assert_eq!(app.font.font().kerning["public.kern1.A"]["V"], -80.0);
        assert_eq!(app.metadata_undo.len(), history_len);

        assert!(app.save());
        assert!(!app.modified);
        app.kern_value_buf = "-80".into();
        app.set_kern_pair_from_bufs();
        assert!(!app.modified, "setting the existing value is a no-op");
        app.remove_from_group("public.kern1.A", "missing");
        assert!(!app.modified, "removing a missing member is a no-op");

        let mut reopened = Workspace::open(&path).expect("reopen saved test font");
        assert_eq!(reopened.font.kern_group("A", true), "public.kern1.A");
        assert_eq!(reopened.font.font().kerning["public.kern1.A"]["V"], -80.0);
        let state = TextState::new(&TextInputs::new(&reopened.font).with_text("AV"));
        assert_eq!(state.buffer.layout(state.line_height).items[1].x, 420.0);

        reopened.delete_kern_pair("public.kern1.A", "V");
        assert!(reopened.modified);
        assert!(reopened.font.master().kerning_dirty);
        reopened.undo_active_edit(false);
        assert_eq!(reopened.font.font().kerning["public.kern1.A"]["V"], -80.0);
        reopened.undo_active_edit(true);
        assert!(reopened.font.font().kerning.is_empty());
        assert!(reopened.save());
        let reopened = Workspace::open(&path).expect("reopen after pair deletion");
        assert!(reopened.font.font().kerning.is_empty());
        std::fs::remove_dir_all(path).expect("remove disposable font");
    }

    #[test]
    fn feature_check_reports_errors_without_dirtying_the_font() {
        let path = disposable_font("feature-check");
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("A"));
        font.features = "feature liga { nonsense ; } liga;".into();
        font.save(&path).expect("save disposable test font");
        let mut app = Workspace::open(&path).expect("open test font");

        app.check_features();

        assert!(
            app.features_status
                .as_deref()
                .is_some_and(|status| status.starts_with("Checked, but does not compile:"))
        );
        assert!(!app.modified);
        assert!(!app.font.master().dirty);
        std::fs::remove_dir_all(path).expect("remove disposable font");
    }

    #[test]
    fn generated_features_are_undoable() {
        let path = disposable_font("feature-history");
        let mut font = norad::Font::new();
        let mut base = norad::Glyph::new("A");
        base.anchors.push(norad::Anchor::new(
            250.0,
            700.0,
            Some(norad::Name::new("top").expect("anchor name")),
            None,
            None,
        ));
        let mut mark = norad::Glyph::new("acutecomb");
        mark.anchors.push(norad::Anchor::new(
            0.0,
            0.0,
            Some(norad::Name::new("_top").expect("anchor name")),
            None,
            None,
        ));
        font.default_layer_mut().insert_glyph(base);
        font.default_layer_mut().insert_glyph(mark);
        font.save(&path).expect("save disposable test font");
        let mut app = Workspace::open(&path).expect("open test font");
        app.open_glyph(app.font.index_of("A").expect("A"));

        app.generate_features();
        let generated = app.features_buf.clone();
        assert!(generated.contains("feature mark"));
        assert!(app.font.font().features.is_empty());
        assert!(app.features_edited);
        assert!(!app.save(), "an unapplied draft cannot be silently skipped");
        app.apply_features();
        assert_eq!(app.font.font().features, generated);
        assert!(!app.features_edited);
        app.undo_active_edit(false);
        assert!(app.font.font().features.is_empty());
        assert!(app.features_buf.is_empty());
        app.undo_active_edit(true);
        assert_eq!(app.font.font().features, generated);
        assert_eq!(app.features_buf, generated);
        assert!(app.save());
        let reopened = Workspace::open(&path).expect("reopen generated features");
        assert_eq!(reopened.font.font().features, generated);
        assert_eq!(reopened.features_buf, generated);
        std::fs::remove_dir_all(path).expect("remove disposable font");
    }

    #[test]
    fn feature_draft_reverts_without_dirtying_the_font() {
        let path = disposable_font("feature-revert");
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("A"));
        font.features = "languagesystem DFLT dflt;\n".into();
        font.save(&path).expect("save disposable test font");
        let mut app = Workspace::open(&path).expect("open test font");

        app.edit_features("feature liga { sub A A by A; } liga;\n".into());
        assert!(app.features_edited);
        assert!(app.modified);
        assert!(!app.font.master().dirty);
        app.revert_features();
        assert_eq!(app.features_buf, "languagesystem DFLT dflt;\n");
        assert!(!app.features_edited);
        assert!(!app.modified);
        assert!(!app.font.master().dirty);
        std::fs::remove_dir_all(path).expect("remove disposable font");
    }

    #[test]
    fn anchor_drag_delete_undo_and_reopen() {
        let path = disposable_font("anchor-history");
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("beh-ar");
        glyph.anchors.push(norad::Anchor::new(
            300.0,
            500.0,
            Some(norad::Name::new("top").expect("anchor name")),
            None,
            None,
        ));
        font.default_layer_mut().insert_glyph(glyph);
        font.save(&path).expect("save disposable test font");
        let mut app = Workspace::open(&path).expect("open test font");
        app.open_glyph(app.font.index_of("beh-ar").expect("beh-ar"));

        let mut session = (*app.session).clone();
        session.selected_anchor = Some(0);
        session.move_anchor(0, 340.0, 560.0);
        session.move_anchor(0, 360.0, 580.0);
        session.end_metric_drag();
        app.session = Arc::new(session);
        app.refresh_open_glyph();
        assert_eq!(app.font.master().undo_depth(0), 1);
        app.undo_open_glyph(false);
        assert_eq!(
            (
                app.session.glyph.anchors[0].x,
                app.session.glyph.anchors[0].y
            ),
            (300.0, 500.0)
        );
        app.undo_open_glyph(true);
        assert_eq!(
            (
                app.session.glyph.anchors[0].x,
                app.session.glyph.anchors[0].y
            ),
            (360.0, 580.0)
        );

        let mut session = (*app.session).clone();
        session.selected_anchor = Some(0);
        assert!(session.delete_selected_anchor());
        app.session = Arc::new(session);
        app.refresh_open_glyph();
        assert!(app.session.glyph.anchors.is_empty());
        app.undo_open_glyph(false);
        assert_eq!(app.session.glyph.anchors.len(), 1);
        assert!(app.save());
        let reopened = Workspace::open(&path).expect("reopen saved anchor");
        let anchor = &reopened.font.font().get_glyph("beh-ar").unwrap().anchors[0];
        assert_eq!((anchor.x, anchor.y), (360.0, 580.0));
        std::fs::remove_dir_all(path).expect("remove disposable font");
    }
}

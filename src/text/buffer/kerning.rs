// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Kerning between sorts: the model with group fallback, and manual kerning by dragging a sort.

use super::*;

impl TextBuffer {
    /// Index of the sort being dragged in a manual kerning session, or `None` when no drag is in progress.
    pub fn manual_kerning_sort(&self) -> Option<usize> {
        self.manual_kerning.map(|session| session.sort_index)
    }

    /// Replace the kerning model. Any manual kerning edits in the old model are lost.
    pub fn set_kerning_model(&mut self, kerning: TextKerningModel) {
        self.kerning = kerning;
    }

    /// The current kerning model, including pairs written by manual kerning.
    pub fn kerning_model(&self) -> &TextKerningModel {
        &self.kerning
    }

    /// Start dragging the kerning of the pair that ends at `sort_index`, with the pointer at `start_x`.
    /// Records the pair's current value as the baseline and activates the sort. Returns false for index `0`, a line break, or an index out of range.
    pub fn begin_manual_kerning(&mut self, sort_index: usize, start_x: f64) -> bool {
        if sort_index == 0
            || !matches!(
                self.sorts.get(sort_index).map(|sort| &sort.kind),
                Some(TextSortKind::Glyph { .. })
            )
        {
            return false;
        }
        let original_value = self
            .glyph_pair_names(sort_index)
            .map(|(left, right)| self.lookup_kerning(&left, &right))
            .unwrap_or(0.0)
            .round();
        self.manual_kerning = Some(ManualKerningSession {
            sort_index,
            start_x,
            original_value,
            current_offset: 0.0,
        });
        self.activate_sort(sort_index);
        true
    }

    /// Update the dragged pair from the pointer's new x, rounding the offset to whole units.
    /// The offset is negated on an RTL line so a rightward drag still closes the gap. Writes the new value into the kerning model and returns it, or `None` when no session is open or the rounded offset did not change.
    pub fn drag_manual_kerning(&mut self, current_x: f64) -> Option<f64> {
        let session = self.manual_kerning?;
        let mut current_offset = (current_x - session.start_x).round();
        // In a right-to-left line the pair's visual gap sits on the
        // other side of the dragged sort — the logical-previous glyph
        // draws to the right — so a rightward drag closes the gap and
        // the offset flips sign.
        let line = self.line_number_for_sort(session.sort_index);
        if self.resolved_line_direction(line) == TextDirection::RightToLeft {
            current_offset = -current_offset;
        }
        if current_offset == session.current_offset {
            return None;
        }
        self.manual_kerning = Some(ManualKerningSession {
            current_offset,
            ..session
        });
        let (left, right) = self.glyph_pair_names(session.sort_index)?;
        let value = (session.original_value + current_offset).round();
        self.set_direct_kerning(&left, &right, value);
        Some(value)
    }

    /// Close the manual kerning session, keeping the value written so far. Returns false when no session was open.
    pub fn end_manual_kerning(&mut self) -> bool {
        self.manual_kerning.take().is_some()
    }

    pub(super) fn glyph_pair_names(&self, sort_index: usize) -> Option<(String, String)> {
        let left = self.sort_glyph_name(sort_index.checked_sub(1)?)?;
        let right = self.sort_glyph_name(sort_index)?;
        Some((left.to_string(), right.to_string()))
    }

    pub(super) fn lookup_kerning(&self, left: &str, right: &str) -> f64 {
        lookup_xilem_kerning(
            &self.kerning.kerning,
            &self.kerning.groups,
            left,
            self.kerning.right_groups.get(left).map(String::as_str),
            right,
            self.kerning.left_groups.get(right).map(String::as_str),
        )
    }

    pub(super) fn set_direct_kerning(&mut self, left: &str, right: &str, value: f64) {
        if value == 0.0 {
            if let Some(pairs) = self.kerning.kerning.get_mut(left) {
                pairs.remove(right);
                if pairs.is_empty() {
                    self.kerning.kerning.remove(left);
                }
            }
            return;
        }
        self.kerning
            .kerning
            .entry(left.to_string())
            .or_default()
            .insert(right.to_string(), value);
    }
}

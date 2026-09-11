// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Direction: per line, pinned or detected, and the visual runs the Unicode Bidirectional Algorithm splits a line into.

use super::*;

impl TextBuffer {
    /// The pinned base direction. In auto mode this is only the fallback for lines with no strong character.
    pub fn direction(&self) -> TextDirection {
        self.direction
    }

    /// Direction of the line the cursor is on: what the toolbar shows.
    pub fn cursor_direction(&self) -> TextDirection {
        self.resolved_line_direction(self.line_number_for_sort(self.cursor))
    }

    /// True while each line picks its own direction from its content.
    pub fn direction_is_auto(&self) -> bool {
        self.auto_direction
    }

    /// Pin every line to one direction (the toolbar's LTR / RTL).
    pub fn set_direction(&mut self, direction: TextDirection) {
        self.direction = direction;
        self.auto_direction = false;
    }

    /// Go back to per-line detection.
    pub fn set_auto_direction(&mut self) {
        self.auto_direction = true;
    }

    /// Whether the bidi run containing `index` reads right to left.
    ///
    /// This is the sort's own rendering direction, not the line's: a
    /// Latin glyph inside an Arabic sentence is in an LTR run and
    /// reports false. The metrics panel uses it to keep "left" and
    /// "right" meaning what they look like on screen.
    pub fn sort_rtl(&self, index: usize) -> bool {
        let line = self.line_number_for_sort(index);
        let (start, end) = self.line_range_for_number(line);
        self.visual_runs_for_line(start, end)
            .iter()
            .find(|run| run.sorts.contains(&index))
            .map(|run| run.rtl)
            .unwrap_or(false)
    }

    /// Sorts of one line in visual order, left to right, grouped into
    /// runs of one direction each.
    ///
    /// Returns one run per stretch of equal embedding level, in the
    /// order they appear on screen, with `rtl` set for odd levels.
    pub(super) fn visual_runs_for_line(
        &self,
        line_start: usize,
        line_end: usize,
    ) -> Rc<Vec<VisualRun>> {
        let line_number = self.line_number_for_index(line_start);
        let base_rtl = self.resolved_line_direction(line_number) == TextDirection::RightToLeft;
        self.visual_runs_in(line_start, line_end, base_rtl)
    }

    /// The same, over any stretch of sorts with the base direction given.
    /// Line breaks inside the range are skipped, so the preview strip can
    /// treat a run of same-direction lines as one paragraph.
    pub(super) fn visual_runs_in(
        &self,
        start: usize,
        end: usize,
        base_rtl: bool,
    ) -> Rc<Vec<VisualRun>> {
        let key = (start, end, base_rtl, self.bidi_fingerprint(start, end));
        if let Some(runs) = self.bidi_runs.0.borrow().get(&key) {
            return Rc::clone(runs);
        }
        let runs = Rc::new(self.compute_visual_runs(start, end, base_rtl));
        let mut cache = self.bidi_runs.0.borrow_mut();
        if cache.len() >= BIDI_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, Rc::clone(&runs));
        runs
    }

    /// What the runs for a stretch of sorts depend on: the characters
    /// themselves and where the line breaks fall.
    pub(super) fn bidi_fingerprint(&self, start: usize, end: usize) -> u64 {
        let mut hasher = DefaultHasher::new();
        for index in start..end.min(self.sorts.len()) {
            match &self.sorts[index].kind {
                TextSortKind::Glyph { codepoint, .. } => codepoint.hash(&mut hasher),
                TextSortKind::LineBreak => u32::MAX.hash(&mut hasher),
            }
        }
        hasher.finish()
    }

    pub(super) fn compute_visual_runs(
        &self,
        start: usize,
        end: usize,
        base_rtl: bool,
    ) -> Vec<VisualRun> {
        // Sorts with no codepoint (an unencoded glyph typed by name)
        // have no bidi class of their own. They take the paragraph's
        // direction, which is what an object replacement character does.
        const NO_CODEPOINT: char = '\u{fffc}';

        let mut text = String::new();
        let mut sort_for_offset: Vec<usize> = Vec::new();
        for index in start..end.min(self.sorts.len()) {
            if matches!(self.sorts[index].kind, TextSortKind::LineBreak) {
                continue;
            }
            let char = self.sort_codepoint(index).unwrap_or(NO_CODEPOINT);
            for _ in 0..char.len_utf8() {
                sort_for_offset.push(index);
            }
            text.push(char);
        }
        if text.is_empty() {
            return Vec::new();
        }

        let base_level = if base_rtl { Level::rtl() } else { Level::ltr() };

        let info = BidiInfo::new(&text, Some(base_level));
        let Some(paragraph) = info.paragraphs.first() else {
            return Vec::new();
        };
        let (levels, ranges) = info.visual_runs(paragraph, paragraph.range.clone());

        let mut runs: Vec<VisualRun> = Vec::new();
        for range in ranges {
            let rtl = levels[range.start].is_rtl();
            let mut sorts: Vec<usize> = Vec::new();
            for offset in range {
                let Some(&index) = sort_for_offset.get(offset) else {
                    continue;
                };
                if sorts.last() != Some(&index) {
                    sorts.push(index);
                }
            }
            // Within an RTL run the sorts are still in logical order;
            // the run as a whole reads right to left.
            if !sorts.is_empty() {
                runs.push(VisualRun::new(rtl, sorts));
            }
        }
        runs
    }
}

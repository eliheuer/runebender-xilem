// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Placing sorts on lines, and mapping a point back to a sort or a cursor position.

use super::*;

impl TextBuffer {
    /// In auto mode a line reads right-to-left when its first strong
    /// character does; otherwise every line follows the pinned
    /// direction.
    pub fn resolved_line_direction(&self, line: usize) -> TextDirection {
        if !self.auto_direction {
            return self.direction;
        }
        let (start, end) = self.line_range_for_number(line);
        for index in start..end.min(self.sorts.len()) {
            if let TextSortKind::Glyph {
                codepoint: Some(char),
                ..
            } = &self.sorts[index].kind
                && let Some(direction) = strong_direction(*char)
            {
                return direction;
            }
        }
        self.direction
    }

    pub(super) fn line_number_for_sort(&self, sort_index: usize) -> usize {
        self.sorts[..sort_index.min(self.sorts.len())]
            .iter()
            .filter(|sort| matches!(sort.kind, TextSortKind::LineBreak))
            .count()
    }

    /// Number of lines, which is one more than the number of line breaks.
    pub fn line_count(&self) -> usize {
        1 + self
            .sorts
            .iter()
            .filter(|sort| matches!(sort.kind, TextSortKind::LineBreak))
            .count()
    }

    // TODO(perf): every frame walks the whole buffer, and every sort is
    // drawn whether or not it is on screen. At a page of text this is the
    // bulk of a frame. Cache the layout against a buffer revision, and
    // cull sorts outside the viewport before handing them to the scene.
    /// Place every drawn sort and the caret in font units.
    /// Lines stack downward by `line_height`; LTR lines start at x `0` and RTL lines share one right edge at the widest line. Kerning and bidi run order are applied. Absorbed sorts and line breaks get no item.
    pub fn layout(&self, line_height: f64) -> TextLayout {
        let mut items = Vec::with_capacity(self.sorts.len());
        let mut cursor_x = 0.0;
        let mut cursor_y = 0.0;
        let mut line_start = 0;
        let mut line_number = 0;
        // RTL lines share one right edge so a paragraph stays aligned,
        // the way xilem lines them up; LTR lines start at the origin.
        let rtl_line_start_x = self.rtl_line_start_x();

        while line_start <= self.sorts.len() {
            let line_end = self.next_line_end(line_start);
            let direction = self.resolved_line_direction(line_number);
            let y = -line_height * line_number as f64;

            // Everything is placed left to right, run by run. A line that
            // reads right to left is right-aligned rather than reversed
            // wholesale: the bidi runs carry the order, so Latin inside
            // an Arabic sentence still reads left to right.
            let runs = self.visual_runs_for_line(line_start, line_end);

            // Lay the line out from zero, then shift the finished line
            // into place: a right-to-left line is right-aligned on the
            // widest line. Measuring first would mean looking up every
            // kerning pair twice, which at a page of text is the most
            // expensive thing in the frame.
            let line_items_start = items.len();
            let mut x = 0.0;
            // Caret position within the line, filled in as the run
            // holding the cursor is placed.
            let mut caret_at: Option<f64> = if self.cursor == line_start {
                Some(0.0)
            } else {
                None
            };
            let mut caret_at_line_start = self.cursor == line_start;

            for run in runs.iter() {
                let mut previous: Option<&str> = None;
                for &index in run.visual_order() {
                    // A character folded into a ligature has no glyph of
                    // its own: no item, no width, no kerning pair.
                    if self.sorts[index].absorbed {
                        if self.cursor == index + 1 {
                            caret_at = Some(x);
                            caret_at_line_start = false;
                        }
                        continue;
                    }
                    let advance_width = self.sort_advance(index);
                    let glyph_name = self.sort_glyph_name(index);
                    // Kerning pairs are in logical order. Inside an RTL
                    // run the glyph placed just before this one is the
                    // logically *following* one, so the pair is reversed.
                    x += kern_between(self, previous, glyph_name, run.rtl);
                    items.push(TextLayoutItem {
                        index,
                        x,
                        y,
                        advance_width,
                    });
                    x += advance_width;
                    previous = glyph_name;

                    // The caret follows the text, not the screen: after a
                    // sort in an RTL run it sits at that sort's left edge.
                    if self.cursor == index + 1 {
                        caret_at = Some(if run.rtl { x - advance_width } else { x });
                        caret_at_line_start = false;
                    }
                }
            }

            let line_width = x;
            let line_left = match direction {
                TextDirection::LeftToRight => 0.0,
                TextDirection::RightToLeft => rtl_line_start_x - line_width,
            };
            if line_left != 0.0 {
                for item in &mut items[line_items_start..] {
                    item.x += line_left;
                }
            }
            // An empty caret slot, or one at the start of a line, sits at
            // the edge the line reads from.
            if caret_at_line_start && direction == TextDirection::RightToLeft {
                caret_at = Some(line_width);
            }
            let caret_at = caret_at.map(|caret| caret + line_left);

            let caret_at = match caret_at {
                Some(caret) => Some(caret),
                None if self.cursor >= line_start && self.cursor <= line_end => {
                    Some(match direction {
                        TextDirection::LeftToRight => line_left,
                        TextDirection::RightToLeft => line_left + line_width,
                    })
                }
                None => None,
            };
            if let Some(caret) = caret_at {
                cursor_x = caret;
                cursor_y = y;
            }

            if line_end >= self.sorts.len() {
                break;
            }

            // Skip the line-break sort.
            if self.cursor == line_end + 1 {
                cursor_x = match self.resolved_line_direction(line_number + 1) {
                    TextDirection::LeftToRight => 0.0,
                    TextDirection::RightToLeft => rtl_line_start_x,
                };
                cursor_y = -line_height * (line_number + 1) as f64;
            }
            line_start = line_end + 1;
            line_number += 1;
        }

        TextLayout {
            items,
            cursor_x,
            cursor_y,
        }
    }

    /// Place every drawn sort on one visual line at y `0`, for the preview strip.
    /// Consecutive lines that read the same way run on as one paragraph. Kerning is applied within a run but never across a line break.
    pub fn preview_layout(&self) -> Vec<TextLayoutItem> {
        // The strip is one visual line. Lines that read the same way run
        // on into each other as a single paragraph — an Arabic paragraph
        // keeps reading right to left across its line breaks — while the
        // bidi runs inside still order themselves, so a Latin word in an
        // Arabic sentence reads left to right here too.
        let mut items = Vec::with_capacity(self.sorts.len());
        let mut x = 0.0;

        for (start, end, rtl) in self.preview_groups() {
            for run in self.visual_runs_in(start, end, rtl).iter() {
                let mut previous: Option<(usize, &str)> = None;
                for &index in run.visual_order() {
                    if self.sorts[index].absorbed {
                        continue;
                    }
                    let advance_width = self.sort_advance(index);
                    let glyph_name = self.sort_glyph_name(index);
                    // Kerning stops at a run edge and at a line break:
                    // two lines are not a kerning context, however they
                    // are drawn.
                    let kern = match (previous, glyph_name) {
                        (Some((previous_index, previous_name)), Some(name))
                            if !self.line_break_between(previous_index, index) =>
                        {
                            if run.rtl {
                                self.lookup_kerning(name, previous_name)
                            } else {
                                self.lookup_kerning(previous_name, name)
                            }
                        }
                        _ => 0.0,
                    };
                    x += kern;
                    items.push(TextLayoutItem {
                        index,
                        x,
                        y: 0.0,
                        advance_width,
                    });
                    x += advance_width;
                    previous = glyph_name.map(|name| (index, name));
                }
            }
        }

        items
    }

    /// Stretches of consecutive lines that read the same way, as
    /// (start, end, rtl). The preview strip lays out one at a time.
    pub(super) fn preview_groups(&self) -> Vec<(usize, usize, bool)> {
        let mut groups: Vec<(usize, usize, bool)> = Vec::new();
        let mut line_start = 0;
        let mut line_number = 0;
        while line_start <= self.sorts.len() {
            let line_end = self.next_line_end(line_start);
            let rtl = self.resolved_line_direction(line_number) == TextDirection::RightToLeft;
            match groups.last_mut() {
                Some(group) if group.2 == rtl => group.1 = line_end,
                _ => groups.push((line_start, line_end, rtl)),
            }
            if line_end >= self.sorts.len() {
                break;
            }
            line_start = line_end + 1;
            line_number += 1;
        }
        groups
    }

    /// Report whether a line break separates sorts `a` and `b`.
    pub(super) fn line_break_between(&self, a: usize, b: usize) -> bool {
        let (low, high) = if a <= b { (a, b) } else { (b, a) };
        self.sorts[low.min(self.sorts.len())..high.min(self.sorts.len())]
            .iter()
            .any(|sort| matches!(sort.kind, TextSortKind::LineBreak))
    }

    /// Find what a point in font units lands on.
    /// Returns the sort whose box, from `descender` to `ascender` above its baseline, contains the point, with the cursor placed after it. Otherwise returns the nearest cursor boundary on the line under `y` and no sort.
    pub fn hit_test(
        &self,
        x: f64,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
    ) -> TextHit {
        let layout = self.layout(line_height);
        self.hit_test_with_layout(x, y, line_height, ascender, descender, &layout)
    }

    pub(super) fn hit_test_with_layout(
        &self,
        x: f64,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
        layout: &TextLayout,
    ) -> TextHit {
        if self.sorts.is_empty() {
            return TextHit {
                cursor: 0,
                active_sort: None,
            };
        }

        let line_height = line_height.max(1.0);
        let target_line = self.line_number_for_y(y, line_height, ascender, descender);
        let (line_start, line_end) = self.line_range_for_number(target_line);
        let nearest_cursor = self.nearest_cursor_for_line(x, line_start, line_end, layout);

        for item in layout
            .items
            .iter()
            .filter(|item| (line_start..line_end).contains(&item.index))
        {
            // Match xilem's `kurbo::Rect::contains` sort hit-test:
            // min edges inclusive, max edges exclusive.
            let within_x = x >= item.x && x < item.x + item.advance_width;
            let within_y = y >= item.y + descender && y < item.y + ascender;
            if within_x && within_y {
                return TextHit {
                    cursor: item.index + 1,
                    active_sort: Some(item.index),
                };
            }
        }

        TextHit {
            cursor: nearest_cursor,
            active_sort: None,
        }
    }

    pub(super) fn line_number_for_index(&self, index: usize) -> usize {
        self.sorts[..index.min(self.sorts.len())]
            .iter()
            .filter(|sort| matches!(sort.kind, TextSortKind::LineBreak))
            .count()
    }

    pub(super) fn next_line_end(&self, start: usize) -> usize {
        self.sorts[start..]
            .iter()
            .position(|sort| matches!(sort.kind, TextSortKind::LineBreak))
            .map(|offset| start + offset)
            .unwrap_or(self.sorts.len())
    }

    pub(super) fn line_range_for_number(&self, line_number: usize) -> (usize, usize) {
        let mut start = 0;
        let mut current_line = 0;
        while start <= self.sorts.len() {
            let end = self.next_line_end(start);
            if current_line == line_number || end >= self.sorts.len() {
                return (start, end);
            }
            start = end + 1;
            current_line += 1;
        }
        (self.sorts.len(), self.sorts.len())
    }

    pub(super) fn line_number_for_y(
        &self,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
    ) -> usize {
        let mut start = 0;
        let mut line_number = 0;
        let mut nearest_line = 0;
        let mut nearest_distance = f64::INFINITY;
        while start <= self.sorts.len() {
            let baseline = -line_height * line_number as f64;
            let top = baseline + ascender;
            let bottom = baseline + descender;
            if y >= bottom && y <= top {
                return line_number;
            }
            let distance = if y > top { y - top } else { bottom - y };
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest_line = line_number;
            }

            let end = self.next_line_end(start);
            if end >= self.sorts.len() {
                break;
            }
            start = end + 1;
            line_number += 1;
        }
        nearest_line
    }

    pub(super) fn hit_sort_item_at(
        &self,
        x: f64,
        y: f64,
        line_height: f64,
        ascender: f64,
        descender: f64,
        layout: &TextLayout,
    ) -> Option<TextLayoutItem> {
        if self.sorts.is_empty() {
            return None;
        }

        let line_height = line_height.max(1.0);
        let target_line = self.line_number_for_y(y, line_height, ascender, descender);
        let (line_start, line_end) = self.line_range_for_number(target_line);
        for item in layout
            .items
            .iter()
            .filter(|item| (line_start..line_end).contains(&item.index))
        {
            let within_x = x >= item.x && x < item.x + item.advance_width;
            let within_y = y >= item.y + descender && y < item.y + ascender;
            if within_x && within_y {
                return Some(*item);
            }
        }
        None
    }

    pub(super) fn line_width(&self, start: usize, end: usize) -> f64 {
        let mut width = 0.0;
        let mut previous_glyph_name: Option<&str> = None;
        for index in start..end {
            let glyph_name = self.sort_glyph_name(index);
            if let Some((left, right)) = previous_glyph_name.zip(glyph_name) {
                width += self.lookup_kerning(left, right);
            }
            width += self.sort_advance(index);
            previous_glyph_name = glyph_name;
        }
        width
    }

    pub(super) fn nearest_cursor_for_line(
        &self,
        x: f64,
        line_start: usize,
        line_end: usize,
        layout: &TextLayout,
    ) -> usize {
        let mut nearest_cursor = line_start;
        let mut nearest_distance = f64::INFINITY;
        let line_start_x = match self.direction {
            TextDirection::LeftToRight => self.line_width(line_start, line_end),
            TextDirection::RightToLeft => self.rtl_line_start_x(),
        };

        for candidate in line_start..=line_end {
            let cursor_x = if candidate == line_start {
                match self.direction {
                    TextDirection::LeftToRight => 0.0,
                    TextDirection::RightToLeft => line_start_x,
                }
            } else {
                layout
                    .items
                    .iter()
                    .find(|item| item.index + 1 == candidate)
                    .map(|item| match self.direction {
                        TextDirection::LeftToRight => item.x + item.advance_width,
                        TextDirection::RightToLeft => item.x,
                    })
                    .unwrap_or(0.0)
            };
            let distance = (x - cursor_x).abs();
            if distance < nearest_distance {
                nearest_distance = distance;
                nearest_cursor = candidate;
            }
        }

        nearest_cursor
    }

    /// Where an RTL line begins: the widest line in the buffer, so
    /// every RTL line ends up sharing the same right edge. Xilem
    /// summed the whole buffer instead, which is too wide once lines
    /// stack.
    pub(super) fn rtl_line_start_x(&self) -> f64 {
        let mut widest: f64 = 0.0;
        let mut line_start = 0;
        while line_start <= self.sorts.len() {
            let line_end = self.next_line_end(line_start);
            let width: f64 = (line_start..line_end).map(|i| self.sort_advance(i)).sum();
            widest = widest.max(width);
            if line_end >= self.sorts.len() {
                break;
            }
            line_start = line_end + 1;
        }
        widest
    }

    pub(super) fn sort_advance(&self, index: usize) -> f64 {
        match &self.sorts[index].kind {
            TextSortKind::Glyph { advance_width, .. } => *advance_width,
            TextSortKind::LineBreak => 0.0,
        }
    }

    pub(super) fn sort_glyph_name(&self, index: usize) -> Option<&str> {
        match &self.sorts[index].kind {
            TextSortKind::Glyph { name, .. } => Some(name),
            TextSortKind::LineBreak => None,
        }
    }

    pub(super) fn sort_codepoint(&self, index: usize) -> Option<char> {
        match &self.sorts[index].kind {
            TextSortKind::Glyph { codepoint, .. } => *codepoint,
            TextSortKind::LineBreak => None,
        }
    }
}

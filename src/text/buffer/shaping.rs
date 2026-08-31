// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Shaping the buffer: through the font's own features when they compile, and through the Arabic joining rules otherwise.

use super::*;

impl TextBuffer {
    /// The compiled shaping font for the current inventory, or `None`
    /// when there is no features.fea or it does not compile. Built once
    /// and cached until the inventory changes.
    pub(super) fn shaping_font(&self) -> Option<Rc<ShapingFont>> {
        if self.glyph_inventory.features.trim().is_empty() {
            return None;
        }
        if let Some(cached) = self.shaping_font.get() {
            return cached;
        }

        // Glyph order: every glyph the inventory knows, .notdef first so
        // it takes glyph id 0 the way a real font does.
        let mut names: Vec<&String> = self
            .glyph_inventory
            .widths
            .keys()
            .chain(self.glyph_inventory.outlines.keys())
            .collect();
        names.sort();
        names.dedup();

        let mut unicodes: HashMap<&str, Vec<u32>> = HashMap::new();
        for (codepoint, name) in &self.glyph_inventory.unicode {
            unicodes.entry(name.as_str()).or_default().push(*codepoint);
        }

        let glyphs: Vec<ShapingGlyph> = std::iter::once(".notdef")
            .chain(
                names
                    .iter()
                    .map(|name| name.as_str())
                    .filter(|name| *name != ".notdef"),
            )
            .map(|name| ShapingGlyph {
                name: name.to_string(),
                advance: self
                    .glyph_inventory
                    .widths
                    .get(name)
                    .copied()
                    .unwrap_or(0.0),
                unicodes: unicodes.get(name).cloned().unwrap_or_default(),
            })
            .collect();

        let built = ShapingFont::build(&ShapingSource {
            units_per_em: self.glyph_inventory.units_per_em,
            glyphs,
            features: self.glyph_inventory.features.clone(),
        })
        .map(Rc::new)
        .map_err(|e| {
            // Expected while the feature file is being edited; the old
            // joining rules carry on.
            log_shaping_failure(&e);
        })
        .ok();

        self.shaping_font.set(built.clone());
        built
    }

    /// Shape every line through the font's own rules. Returns false when
    /// there is no usable font, so the caller can fall back.
    ///
    /// Lines are split into runs first. A line mixing Latin and Arabic
    /// has to be shaped a run at a time: handed the whole line, the
    /// shaper takes its script from the first character, and the Arabic
    /// features, including the lam-alef ligature, never run.
    pub(super) fn shape_with_font(&mut self) -> bool {
        let Some(font) = self.shaping_font() else {
            return false;
        };

        let mut updates: Vec<(usize, String, f64)> = Vec::new();
        let mut absorbed: Vec<bool> = vec![false; self.sorts.len()];

        for line in 0..self.line_count() {
            let (line_start, line_end) = self.line_range_for_number(line);

            // Shape run by run, the way the text is laid out: a run's
            // direction comes from its bidi level, not from the line
            // around it, and a run is one script so the shaper can pick
            // the right rules for it.
            for bidi_run in self.visual_runs_for_line(line_start, line_end).iter() {
                let chars: Vec<(char, usize)> = bidi_run
                    .sorts
                    .iter()
                    .filter_map(|&index| Some((self.sort_codepoint(index)?, index)))
                    .collect();
                let run_rtl = bidi_run.rtl;

                let mut run_start = 0;
                while run_start < chars.len() {
                    let arabic = joining::is_arabic(chars[run_start].0);
                    let mut run_end = run_start;
                    while run_end < chars.len() && joining::is_arabic(chars[run_end].0) == arabic {
                        run_end += 1;
                    }

                    let mut text = String::new();
                    let mut sort_for_offset: Vec<usize> = Vec::new();
                    for &(char, index) in &chars[run_start..run_end] {
                        for _ in 0..char.len_utf8() {
                            sort_for_offset.push(index);
                        }
                        text.push(char);
                    }

                    let Ok(shaped) = font.shape_with_options(
                        &text,
                        run_rtl,
                        &self.feature_overrides,
                        self.script_override.as_deref(),
                        self.language_override.as_deref(),
                    ) else {
                        return false;
                    };

                    // Clusters are byte offsets into the run. A ligature
                    // reports the offset of its first character and stands
                    // for every character up to the next cluster.
                    let mut covered = vec![false; sort_for_offset.len()];
                    for glyph in &shaped {
                        let Some(&sort_index) = sort_for_offset.get(glyph.cluster as usize) else {
                            continue;
                        };
                        let Some(name) = font.glyph_name(glyph.glyph_id) else {
                            continue;
                        };
                        updates.push((sort_index, name.to_string(), glyph.x_advance));
                        for (offset, covered) in covered.iter_mut().enumerate() {
                            if sort_for_offset[offset] == sort_index {
                                *covered = true;
                            }
                        }
                    }

                    let mut seen_sort: Option<usize> = None;
                    for (offset, &sort_index) in sort_for_offset.iter().enumerate() {
                        if seen_sort == Some(sort_index) {
                            continue;
                        }
                        seen_sort = Some(sort_index);
                        if !covered[offset] {
                            absorbed[sort_index] = true;
                            updates.push((sort_index, String::new(), 0.0));
                        }
                    }

                    run_start = run_end;
                }
            }
        }

        let changed = self.apply_shape_updates(updates);
        let mut absorbed_changed = false;
        for (index, sort) in self.sorts.iter_mut().enumerate() {
            let want = absorbed.get(index).copied().unwrap_or(false);
            if sort.absorbed != want {
                sort.absorbed = want;
                absorbed_changed = true;
            }
        }
        changed || absorbed_changed
    }

    /// Shape the whole buffer: through the font's `features.fea` when it compiles, otherwise with the built-in Arabic joining rules on RTL lines.
    /// Updates glyph names and advance widths in place. Returns true when any sort changed.
    pub fn shape_arabic(&mut self) -> bool {
        // The font's own GSUB first: it gives ligatures and contextual
        // rules the joining table below cannot express. Falls through
        // when there is no features.fea or it does not compile.
        if self.shape_with_font() {
            return true;
        }
        let chars = self.glyph_chars();
        let mut updates = Vec::new();

        for index in 0..self.sorts.len() {
            let Some(char) = self.sort_codepoint(index) else {
                continue;
            };
            let char_index = self.char_index_for_sort_index(index);
            let name = self.shaped_glyph_name_for_character(char, &chars, char_index, index);
            let advance_width = self
                .glyph_inventory
                .widths
                .get(&name)
                .copied()
                .unwrap_or_else(|| self.sort_advance(index));
            updates.push((index, name, advance_width));
        }

        self.apply_shape_updates(updates)
    }

    /// Shape when any line in the buffer reads RTL: a Latin line next
    /// to an Arabic one must not stop the Arabic from joining.
    ///
    /// With a shaping font the direction gate does not apply: the font's
    /// rules cover every script it supports, not just Arabic.
    pub fn shape_arabic_if_rtl(&mut self) -> bool {
        if self.shape_with_font() {
            return true;
        }
        let has_rtl_line = (0..self.line_count())
            .any(|line| self.resolved_line_direction(line) == TextDirection::RightToLeft);
        if !has_rtl_line {
            return false;
        }
        self.shape_arabic()
    }

    /// Shape after an edit at `position`.
    /// With a shaping font the whole buffer is reshaped, since a ligature can form several sorts away. Without one only the Arabic neighbors are rejoined, and only when the line reads RTL. Returns true when any sort changed.
    pub fn shape_arabic_around_if_rtl(&mut self, position: usize) -> bool {
        // Reshaping the whole buffer through the font is cheap at editor
        // sizes, and a ligature can appear or break several sorts away
        // from the one that changed.
        if self.shape_with_font() {
            return true;
        }
        let line = self.line_number_for_sort(position);
        if self.resolved_line_direction(line) != TextDirection::RightToLeft {
            return false;
        }
        self.shape_arabic_around(position)
    }

    pub(super) fn shape_arabic_around(&mut self, position: usize) -> bool {
        if self.sorts.is_empty() {
            return false;
        }

        let indices = self.arabic_shape_indices_around(position);
        if indices.is_empty() {
            return false;
        }

        let chars = self.glyph_chars();
        let mut updates = Vec::new();

        for index in indices {
            let Some(char) = self.sort_codepoint(index) else {
                continue;
            };
            if !joining::is_arabic(char) {
                continue;
            }
            let char_index = self.char_index_for_sort_index(index);
            let name = self.shaped_glyph_name_for_character(char, &chars, char_index, index);
            let advance_width = self
                .glyph_inventory
                .widths
                .get(&name)
                .copied()
                .unwrap_or_else(|| self.sort_advance(index));
            updates.push((index, name, advance_width));
        }

        self.apply_shape_updates(updates)
    }

    pub(super) fn arabic_shape_indices_around(&self, position: usize) -> Vec<usize> {
        let mut indices = Vec::new();

        if let Some(index) = self.previous_nontransparent_arabic_sort(position) {
            indices.push(index);
        }

        if let Some(index) = self.next_nontransparent_arabic_sort(position) {
            indices.push(index);
            if let Some(next_index) = self.next_nontransparent_arabic_sort(index + 1) {
                indices.push(next_index);
            }
        }

        indices.dedup();
        indices
    }

    pub(super) fn previous_nontransparent_arabic_sort(&self, position: usize) -> Option<usize> {
        let end = position.min(self.sorts.len());
        (0..end)
            .rev()
            .find(|index| self.is_nontransparent_arabic_sort(*index))
    }

    pub(super) fn next_nontransparent_arabic_sort(&self, position: usize) -> Option<usize> {
        (position..self.sorts.len()).find(|index| self.is_nontransparent_arabic_sort(*index))
    }

    pub(super) fn is_nontransparent_arabic_sort(&self, index: usize) -> bool {
        self.sort_codepoint(index).is_some_and(|char| {
            joining::is_arabic(char) && !joining::arabic_joining_type(char).is_transparent()
        })
    }

    pub(super) fn glyph_chars(&self) -> Vec<char> {
        self.sorts
            .iter()
            .filter_map(|sort| match sort.kind {
                TextSortKind::Glyph {
                    codepoint: Some(char),
                    ..
                } => Some(char),
                _ => None,
            })
            .collect()
    }

    pub(super) fn char_index_for_sort_index(&self, sort_index: usize) -> usize {
        self.sorts[..sort_index]
            .iter()
            .filter(|sort| {
                matches!(
                    sort.kind,
                    TextSortKind::Glyph {
                        codepoint: Some(_),
                        ..
                    }
                )
            })
            .count()
    }

    pub(super) fn apply_shape_updates(&mut self, updates: Vec<(usize, String, f64)>) -> bool {
        let mut changed = false;
        for (index, name, advance_width) in updates {
            let Some(sort) = self.sorts.get_mut(index) else {
                continue;
            };
            let TextSortKind::Glyph {
                name: glyph_name,
                advance_width: glyph_advance_width,
                ..
            } = &mut sort.kind
            else {
                continue;
            };
            if *glyph_name != name || *glyph_advance_width != advance_width {
                *glyph_name = name;
                *glyph_advance_width = advance_width;
                changed = true;
            }
        }

        changed
    }

    pub(super) fn shaped_glyph_name_for_character(
        &self,
        char: char,
        line_chars: &[char],
        char_index: usize,
        sort_index: usize,
    ) -> String {
        let base_name = self
            .glyph_inventory
            .unicode
            .get(&(char as u32))
            .cloned()
            .or_else(|| self.sort_glyph_name(sort_index).map(ToOwned::to_owned))
            .unwrap_or_else(|| ".notdef".to_string());
        // Shape by the *line's* direction, not the buffer's: in Auto
        // mode an Arabic line joins even when the buffer default (or
        // another line) is left-to-right.
        let line = self.line_number_for_sort(sort_index);
        if self.resolved_line_direction(line) != TextDirection::RightToLeft
            || !joining::is_arabic(char)
        {
            return base_name;
        }

        let suffix = joining::arabic_positional_form(line_chars, char_index).suffix();
        let shaped_name = format!("{base_name}{suffix}");
        if !suffix.is_empty() && self.glyph_inventory.has_glyph(&shaped_name) {
            shaped_name
        } else {
            base_name
        }
    }
}

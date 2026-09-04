// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The sidebar's filters and the grid selection.

use crate::*;
use runebender_core::outline::glyph_paths::round_units;

impl Workspace {
    /// The cells that pass the current search + category filter. The two
    /// toggles beside the search box set the scope (name, unicode, both)
    /// and whether the match is case-sensitive.
    pub(crate) fn filtered_cells(&self) -> Arc<Vec<Cell>> {
        let q = if self.search_case {
            self.filter.clone()
        } else {
            self.filter.to_lowercase()
        };
        let by_name = self.search_mode != 2;
        let by_unicode = self.search_mode != 1;
        let re = self
            .search_regex
            .then_some(self.search_re.as_ref())
            .flatten();
        let out: Vec<Cell> = self
            .cells
            .iter()
            .filter(|c| {
                let cat_ok = self.cell_matches_sel(c.index);
                let name_hit = by_name
                    && match re {
                        Some(re) => re.is_match(&c.name),
                        None if self.search_case => c.name.contains(&q),
                        None => c.name.to_lowercase().contains(&q),
                    };
                let uni_hit = by_unicode
                    && c.codepoint
                        .map(|cp| {
                            format!("{:04x}", cp as u32).contains(
                                q.to_lowercase()
                                    .trim_start_matches("u+")
                                    .trim_start_matches("0x"),
                            )
                        })
                        .unwrap_or(false);
                let q_ok = q.is_empty() || name_hit || uni_hit;
                cat_ok && q_ok
            })
            .cloned()
            .collect();
        let mut out = out;
        match self.sort {
            Sort::Name => {}
            Sort::Unicode => {
                out.sort_by_key(|c| c.codepoint.map(|cp| cp as u32).unwrap_or(u32::MAX));
            }
        }
        Arc::new(out)
    }

    /// Codepoints of a glyph entry (the cache keeps only the first).
    pub(crate) fn entry_codepoints(entry: &model::GlyphEntry) -> Vec<u32> {
        entry.codepoint.map(|c| vec![c as u32]).unwrap_or_default()
    }

    /// Does the glyph at `index` pass the active sidebar selection?
    pub(crate) fn cell_matches_sel(&self, index: usize) -> bool {
        use runebender_core::ui::sidebar as sb;
        let entry = &self.font.glyphs[index];
        match self.sel {
            Sel::Category(GlyphCategory::All) => true,
            Sel::Category(cat) => entry.category == cat,
            Sel::Subfilter(cat, sub) => {
                entry.category == cat
                    && sb::glyph_matches_subfilter(&entry.name, &Self::entry_codepoints(entry), sub)
            }
            Sel::LanguageFilter(gi, fi) => sb::language_groups()
                .get(gi)
                .and_then(|g| g.filters.get(fi))
                .map(|f| {
                    sb::glyph_matches_character_filter(
                        &entry.name,
                        &Self::entry_codepoints(entry),
                        f,
                    )
                })
                .unwrap_or(false),
            Sel::Language(i) => sb::language_groups()
                .get(i)
                .map(|g| {
                    sb::glyph_matches_language_group(&entry.name, &Self::entry_codepoints(entry), g)
                })
                .unwrap_or(false),
            Sel::Filter(i) => sb::builtin_filters()
                .get(i)
                .and_then(|b| b.glyphset.as_ref())
                .map(|f| {
                    sb::glyph_matches_character_filter(
                        &entry.name,
                        &Self::entry_codepoints(entry),
                        f,
                    )
                })
                .unwrap_or(false),
        }
    }

    /// How many glyphs in the font match language group `i`.
    pub(crate) fn language_count(&self, i: usize) -> usize {
        use runebender_core::ui::sidebar as sb;
        let Some(g) = sb::language_groups().get(i) else {
            return 0;
        };
        self.font
            .glyphs
            .iter()
            .filter(|e| sb::glyph_matches_language_group(&e.name, &Self::entry_codepoints(e), g))
            .count()
    }

    /// Present-count for GF-coverage filter `i` (glyphs the font has).
    pub(crate) fn filter_present(&self, i: usize) -> usize {
        use runebender_core::ui::sidebar as sb;
        let Some(f) = sb::builtin_filters()
            .get(i)
            .and_then(|b| b.glyphset.as_ref())
        else {
            return 0;
        };
        self.font
            .glyphs
            .iter()
            .filter(|e| sb::glyph_matches_character_filter(&e.name, &Self::entry_codepoints(e), f))
            .count()
    }

    /// How many glyphs a category's subfilter holds.
    pub(crate) fn subfilter_count(&self, cat: GlyphCategory, sub: &str) -> usize {
        use runebender_core::ui::sidebar as sb;
        self.font
            .glyphs
            .iter()
            .filter(|e| {
                e.category == cat
                    && sb::glyph_matches_subfilter(&e.name, &Self::entry_codepoints(e), sub)
            })
            .count()
    }

    /// How many glyphs a language group's filter holds, and how many
    /// it expects.
    pub(crate) fn language_filter_count(&self, gi: usize, fi: usize) -> (usize, Option<usize>) {
        use runebender_core::ui::sidebar as sb;
        let Some(f) = sb::language_groups()
            .get(gi)
            .and_then(|g| g.filters.get(fi))
        else {
            return (0, None);
        };
        let present = self
            .font
            .glyphs
            .iter()
            .filter(|e| sb::glyph_matches_character_filter(&e.name, &Self::entry_codepoints(e), f))
            .count();
        (present, f.expected_count)
    }

    /// Compile the search as a regular expression, when that is on.
    /// A pattern that does not parse leaves no expression, and the
    /// search matches nothing until it does.
    pub(crate) fn rebuild_search_regex(&mut self) {
        self.search_re = if self.search_regex && !self.filter.is_empty() {
            let pattern = if self.search_case {
                self.filter.clone()
            } else {
                format!("(?i){}", self.filter)
            };
            regex::Regex::new(&pattern).ok()
        } else {
            None
        };
    }

    pub(crate) fn category_count(&self, cat: GlyphCategory) -> usize {
        if cat == GlyphCategory::All {
            self.font.glyphs.len()
        } else {
            self.font
                .glyphs
                .iter()
                .filter(|g| g.category == cat)
                .count()
        }
    }

    pub(crate) fn cell_metrics(&self, cell: f64) -> CellMetrics {
        CellMetrics {
            cell,
            ascender: self.font.ascender,
            descender: self.font.descender,
            upm: self.font.units_per_em,
            detail: self.detail,
        }
    }

    /// How many glyphs a coverage filter is still missing.
    pub(crate) fn filter_missing(&self, index: usize) -> usize {
        let filters = runebender_core::ui::sidebar::builtin_filters();
        let Some(set) = filters.get(index).and_then(|f| f.glyphset.as_ref()) else {
            return 0;
        };
        let expected = set
            .expected_count
            .unwrap_or_else(|| set.glyph_names.len().max(set.targets.len()));
        expected.saturating_sub(self.filter_present(index))
    }

    pub(crate) fn grid_select(&mut self, index: usize, cmd: bool, shift: bool) {
        use std::collections::HashSet;
        if cmd {
            let mut m: HashSet<usize> = (*self.multi_selected).clone();
            if !m.remove(&index) {
                m.insert(index);
            }
            self.multi_selected = Arc::new(m);
        } else if shift {
            // Range from the current single selection to this index, in cell order.
            let cells = self.filtered_cells();
            let order: Vec<usize> = cells.iter().map(|c| c.index).collect();
            let a = self
                .selected
                .and_then(|s| order.iter().position(|&i| i == s));
            let b = order.iter().position(|&i| i == index);
            if let (Some(a), Some(b)) = (a, b) {
                let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                let m: HashSet<usize> = order[lo..=hi].iter().copied().collect();
                self.multi_selected = Arc::new(m);
            }
        } else {
            self.multi_selected = Arc::new(HashSet::new());
        }
        self.selected = Some(index);
        // The overview panel edits the highlighted cell, so its boxes
        // have to follow the highlight.
        if matches!(self.mode, Mode::Overview)
            && let Some(entry) = self.font.glyphs.get(index)
        {
            self.name_buf = entry.name.clone();
            self.unicode_buf = entry
                .codepoint
                .map(|c| format!("{:04X}", c as u32))
                .unwrap_or_default();
            self.advance_buf = format!("{}", round_units(entry.advance));
        }
    }
}

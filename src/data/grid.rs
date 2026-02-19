// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph grid operations for AppState

use super::AppState;
use crate::components::{GlyphCategory, NavDirection};
use crate::model::{read_workspace, write_workspace};
use crate::theme;

#[allow(dead_code)]
impl AppState {
    /// Get the number of glyphs in the current font
    pub fn glyph_count(&self) -> Option<usize> {
        self.active_workspace()
            .map(|w| read_workspace(&w).glyph_count())
    }

    /// Select a glyph by name (clears multi-selection)
    pub fn select_glyph(&mut self, name: String) {
        self.selected_glyphs.clear();
        self.selected_glyphs.insert(name.clone());
        self.selected_glyph = Some(name);
    }

    /// Toggle a glyph in/out of multi-selection (shift-click)
    pub fn toggle_glyph_selection(&mut self, name: String) {
        if self.selected_glyphs.contains(&name) {
            self.selected_glyphs.remove(&name);
            self.selected_glyph = self.selected_glyphs.iter().next().cloned();
        } else {
            self.selected_glyphs.insert(name.clone());
            self.selected_glyph = Some(name);
        }
    }

    /// Get all glyph names
    pub fn glyph_names(&self) -> Vec<String> {
        self.active_workspace()
            .map(|w| read_workspace(&w).glyph_names())
            .unwrap_or_default()
    }

    /// Calculate number of grid columns based on window width
    pub fn grid_columns(&self) -> usize {
        const CELL_WIDTH: f64 = 128.0;
        const GAP: f64 = 6.0;
        const MARGIN: f64 = 3.0 * 2.0; // Internal portal padding

        // Available width for cells (minus margins)
        let available = self.window_width - MARGIN;

        // Each cell needs CELL_WIDTH + GAP (except the last)
        // n <= (available + GAP) / (CELL_WIDTH + GAP)
        let max_cols = ((available + GAP) / (CELL_WIDTH + GAP)).floor() as usize;

        // Clamp between 1 and 8 columns
        max_cols.clamp(1, 8)
    }

    /// How many grid rows fit in the visible area
    pub fn visible_grid_rows(&self) -> usize {
        // Row height = cell (192) + gap (6) = 198
        const ROW_HEIGHT: f64 = 198.0;
        // Toolbar row ~70, outer gaps top/bottom ~12, grid padding ~6
        const CHROME_HEIGHT: f64 = 88.0;
        let available = (self.window_height - CHROME_HEIGHT).max(0.0);
        (available / ROW_HEIGHT).floor().max(1.0) as usize
    }

    /// Total number of rows in the grid for the given glyph count
    pub fn total_grid_rows(&self, glyph_count: usize) -> usize {
        let cols = self.grid_columns();
        glyph_count.div_ceil(cols)
    }

    /// Scroll the grid by `delta` rows (positive = down, negative = up)
    pub fn scroll_grid(&mut self, delta: i32, glyph_count: usize) {
        let total = self.total_grid_rows(glyph_count);
        let visible = self.visible_grid_rows();
        let max_row = total.saturating_sub(visible);

        if delta < 0 {
            self.grid_scroll_row = self
                .grid_scroll_row
                .saturating_sub(delta.unsigned_abs() as usize);
        } else {
            self.grid_scroll_row = (self.grid_scroll_row + delta as usize).min(max_row);
        }
    }

    /// Ordered list of glyph names matching the current
    /// category filter. Cheap — no bezpath computation.
    pub fn filtered_glyph_names(&self) -> Vec<String> {
        let Some(workspace_arc) = self.active_workspace() else {
            return Vec::new();
        };
        let workspace = read_workspace(&workspace_arc);
        let names = workspace.glyph_names();
        if self.glyph_category_filter == GlyphCategory::All {
            return names;
        }
        names
            .into_iter()
            .filter(|name| {
                if let Some(glyph) = workspace.get_glyph(name) {
                    if glyph.codepoints.is_empty() {
                        self.glyph_category_filter == GlyphCategory::Other
                    } else {
                        GlyphCategory::from_codepoint(glyph.codepoints[0])
                            == self.glyph_category_filter
                    }
                } else {
                    false
                }
            })
            .collect()
    }

    /// Move the grid selection in the given direction.
    ///
    /// Left/Right move by one cell; Up/Down jump by one row
    /// (column-count cells). The viewport auto-scrolls to keep
    /// the selection visible.
    pub fn navigate_grid_selection(&mut self, direction: NavDirection) {
        let names = self.filtered_glyph_names();
        if names.is_empty() {
            return;
        }
        self.cached_filtered_count = names.len();

        let columns = self.grid_columns() as isize;

        // Find current selection index
        let current_index = self
            .selected_glyph
            .as_ref()
            .and_then(|sel| names.iter().position(|n| n == sel));

        let target = match current_index {
            None => 0, // No selection → first glyph
            Some(idx) => {
                let idx = idx as isize;
                let delta = match direction {
                    NavDirection::Left => -1,
                    NavDirection::Right => 1,
                    NavDirection::Up => -columns,
                    NavDirection::Down => columns,
                };
                let t = idx + delta;
                t.clamp(0, names.len() as isize - 1) as usize
            }
        };

        self.select_glyph(names[target].clone());
        self.scroll_to_glyph_index(target);
    }

    /// Adjust `grid_scroll_row` so that the glyph at `index`
    /// (within the filtered list) is visible.
    fn scroll_to_glyph_index(&mut self, index: usize) {
        let columns = self.grid_columns();
        let row = index / columns;
        let visible = self.visible_grid_rows();
        let total = self.total_grid_rows(self.cached_filtered_count);
        let max_row = total.saturating_sub(visible);

        if row < self.grid_scroll_row {
            self.grid_scroll_row = row;
        } else if row >= self.grid_scroll_row + visible {
            self.grid_scroll_row = (row + 1).saturating_sub(visible);
        }
        self.grid_scroll_row = self.grid_scroll_row.min(max_row);
    }

    /// Count of glyphs matching the current category filter
    pub fn filtered_glyph_count(&self) -> usize {
        let names = self.glyph_names();
        if self.glyph_category_filter == GlyphCategory::All {
            return names.len();
        }
        let Some(workspace_arc) = self.active_workspace() else {
            return names.len();
        };
        let workspace = read_workspace(&workspace_arc);
        names
            .iter()
            .filter(|name| {
                if let Some(glyph) = workspace.get_glyph(name) {
                    if glyph.codepoints.is_empty() {
                        self.glyph_category_filter == GlyphCategory::Other
                    } else {
                        GlyphCategory::from_codepoint(glyph.codepoints[0])
                            == self.glyph_category_filter
                    }
                } else {
                    false
                }
            })
            .count()
    }

    /// Get the selected glyph's advance width
    pub fn selected_glyph_advance(&self) -> Option<f64> {
        let workspace = self.active_workspace()?;
        let glyph_name = self.selected_glyph.as_ref()?;
        read_workspace(&workspace)
            .get_glyph(glyph_name)
            .map(|g| g.width)
    }

    /// Get the selected glyph's unicode value
    pub fn selected_glyph_unicode(&self) -> Option<String> {
        let workspace_arc = self.active_workspace()?;
        let glyph_name = self.selected_glyph.as_ref()?;
        let workspace = read_workspace(&workspace_arc);
        let glyph = workspace.get_glyph(glyph_name)?;

        if glyph.codepoints.is_empty() {
            return None;
        }

        glyph
            .codepoints
            .first()
            .map(|c| format!("U+{:04X}", *c as u32))
    }

    /// Set mark color for all selected glyphs by palette index
    ///
    /// Pass `None` to clear the mark color, or `Some(index)` where
    /// index is 0–11 corresponding to `theme::mark::COLORS`.
    pub fn set_glyph_mark_color(&mut self, color_index: Option<usize>) {
        if self.selected_glyphs.is_empty() {
            return;
        }
        let Some(workspace_arc) = self.active_workspace() else {
            return;
        };
        let names: Vec<String> = self.selected_glyphs.iter().cloned().collect();
        let mut workspace = write_workspace(&workspace_arc);
        for name in &names {
            if let Some(glyph) = workspace.get_glyph_mut(name) {
                glyph.mark_color = color_index.map(|i| theme::mark::RGBA_STRINGS[i].to_string());
            }
        }
    }
}

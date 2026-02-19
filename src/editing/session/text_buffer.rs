// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Text buffer and sort management methods for EditSession

use super::{EditSession, WorkspaceGlyphProvider};
use crate::model::read_workspace;
use crate::path::Path;
use crate::shaping::{ArabicShaper, TextDirection};
use kurbo::Point;
use std::sync::Arc;

impl EditSession {
    /// Create a Sort from a character using the workspace character map (Phase 5)
    ///
    /// Looks up the glyph for the given character and creates a Sort.
    /// If the character matches the currently active sort, uses the live edited
    /// glyph width instead of the workspace version.
    ///
    /// Returns None if:
    /// - No workspace is available
    /// - Character has no mapped glyph
    /// - Glyph data cannot be found
    pub fn create_sort_from_char(&self, c: char) -> Option<crate::sort::Sort> {
        tracing::debug!(
            "[create_sort_from_char] Trying to create sort for character: '{}' (U+{:04X})",
            c,
            c as u32
        );

        let workspace_lock = self.workspace.as_ref()?;
        let workspace = read_workspace(workspace_lock);

        tracing::debug!(
            "[create_sort_from_char] Workspace has {} glyphs",
            workspace.glyphs.len()
        );

        // Find a glyph with this codepoint
        let (glyph_name, glyph) = workspace
            .glyphs
            .iter()
            .find(|(_, g)| g.codepoints.contains(&c))?;

        tracing::debug!(
            "[create_sort_from_char] Found glyph: '{}' for character '{}'",
            glyph_name,
            c
        );

        // Check if this character matches the currently active sort's character
        // If so, use the current glyph (with live edits) instead of workspace version
        let advance_width = if self
            .active_sort_unicode
            .as_ref()
            .and_then(|u| {
                u.strip_prefix("U+")
                    .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            })
            .and_then(char::from_u32)
            .map(|active_char| active_char == c)
            .unwrap_or(false)
        {
            // Active sort matches this character - use current glyph width
            self.glyph.width
        } else {
            // Different character - use workspace version
            glyph.width
        };

        Some(crate::sort::Sort::new_glyph(
            glyph_name.clone(),
            Some(c),
            advance_width,
            false, // New sorts are inactive by default
        ))
    }

    /// Create a Sort from a character with Arabic shaping support.
    ///
    /// When text direction is RTL and the character is Arabic, this method:
    /// 1. Determines the correct positional form based on buffer contents
    /// 2. Uses the shaped glyph name (with suffix like .init, .medi, .fina)
    /// 3. Returns a sort with the shaped glyph name
    ///
    /// For LTR or non-Arabic characters, falls back to `create_sort_from_char`.
    pub fn create_shaped_sort_from_char(&self, c: char) -> Option<crate::sort::Sort> {
        // Only use Arabic shaping for RTL mode and Arabic characters
        if self.text_direction != TextDirection::RightToLeft || !crate::shaping::is_arabic(c) {
            return self.create_sort_from_char(c);
        }

        let workspace_lock = self.workspace.as_ref()?;
        let workspace = read_workspace(workspace_lock);
        let font = WorkspaceGlyphProvider::new(&workspace);

        // Get the current text from buffer to determine context
        let buffer = self.text_buffer.as_ref()?;
        let cursor_pos = buffer.cursor();

        // Build character array from buffer for context-aware shaping
        let mut chars: Vec<char> = buffer
            .iter()
            .filter_map(|sort| match &sort.kind {
                crate::sort::SortKind::Glyph { codepoint, .. } => *codepoint,
                _ => None,
            })
            .collect();

        // Insert the new character at cursor position for shaping
        chars.insert(cursor_pos, c);

        // Shape the new character
        let shaper = ArabicShaper::new();
        let shaped = shaper.shape_char_at(&chars, cursor_pos, &font)?;

        tracing::debug!(
            "[create_shaped_sort_from_char] Character '{}' shaped to '{}' ({:?})",
            c,
            shaped.glyph_name,
            shaped.form
        );

        Some(crate::sort::Sort::new_glyph(
            shaped.glyph_name,
            Some(c),
            shaped.advance_width,
            false,
        ))
    }

    /// Reshape sorts in the buffer around a position after an edit.
    ///
    /// This updates the glyph names of affected sorts to reflect their new
    /// positional forms. Call this after inserting or deleting a character.
    ///
    /// Returns the range of indices that were updated.
    pub fn reshape_buffer_around(&mut self, position: usize) -> Option<(usize, usize)> {
        // Only reshape in RTL mode
        if self.text_direction != TextDirection::RightToLeft {
            return None;
        }

        let workspace_lock = self.workspace.as_ref()?.clone();
        let workspace = read_workspace(&workspace_lock);
        let font = WorkspaceGlyphProvider::new(&workspace);

        let buffer = self.text_buffer.as_mut()?;

        // Build character array from buffer
        let chars: Vec<char> = buffer
            .iter()
            .filter_map(|sort| match &sort.kind {
                crate::sort::SortKind::Glyph { codepoint, .. } => *codepoint,
                _ => None,
            })
            .collect();

        if chars.is_empty() {
            return None;
        }

        // Determine range to reshape (position ± 1, clamped)
        let start = position.saturating_sub(1);
        let end = (position + 1).min(chars.len());

        let shaper = ArabicShaper::new();

        // Reshape each character in the affected range
        for i in start..end {
            if i >= chars.len() {
                continue;
            }

            let c = chars[i];

            // Only reshape Arabic characters
            if !crate::shaping::is_arabic(c) {
                continue;
            }

            if let Some(shaped) = shaper.shape_char_at(&chars, i, &font) {
                // Update the sort at this position
                if let Some(sort) = buffer.get_mut(i)
                    && let crate::sort::SortKind::Glyph {
                        name,
                        advance_width,
                        ..
                    } = &mut sort.kind
                    && *name != shaped.glyph_name
                {
                    tracing::debug!(
                        "[reshape_buffer_around] Updated sort {}: '{}' -> '{}'",
                        i,
                        name,
                        shaped.glyph_name
                    );
                    *name = shaped.glyph_name.clone();
                    *advance_width = shaped.advance_width;
                }
            }
        }

        Some((start, end))
    }

    /// Find and activate the sort at a given position (Phase 7)
    ///
    /// Hit tests the position against each sort's bounding box and activates
    /// the clicked sort. This loads the glyph's paths from the workspace into
    /// session.paths for editing. Returns true if a sort was found and activated.
    pub fn activate_sort_at_position(&mut self, pos: Point) -> bool {
        // First, find which sort was clicked (if any) and its x-offset
        let sort_to_activate: Option<(usize, String, Option<char>, f64)> = {
            let buffer = match &self.text_buffer {
                Some(buf) => buf,
                None => return false,
            };

            // Calculate sort positions and check for hit
            let mut x_offset = 0.0;

            let mut found_sort = None;
            for (index, sort) in buffer.iter().enumerate() {
                match &sort.kind {
                    crate::sort::SortKind::Glyph {
                        name,
                        advance_width,
                        codepoint,
                    } => {
                        // Check if click is within this sort's bounds
                        let sort_left = x_offset;
                        let sort_right = x_offset + advance_width;
                        let sort_top = self.ascender;
                        let sort_bottom = self.descender;

                        if pos.x >= sort_left
                            && pos.x <= sort_right
                            && pos.y >= sort_bottom
                            && pos.y <= sort_top
                        {
                            // Found the clicked sort - capture x_offset too
                            found_sort = Some((index, name.clone(), *codepoint, x_offset));
                            break;
                        }

                        x_offset += advance_width;
                    }
                    crate::sort::SortKind::LineBreak => {
                        x_offset = 0.0;
                    }
                }
            }
            found_sort
        };

        // If we found a sort to activate, load its paths
        if let Some((index, glyph_name, codepoint, x_offset)) = sort_to_activate {
            let workspace_lock = match &self.workspace {
                Some(ws) => ws,
                None => {
                    tracing::warn!("No workspace available to load glyph paths");
                    return false;
                }
            };
            let workspace = read_workspace(workspace_lock);

            let glyph = match workspace.glyphs.get(&glyph_name) {
                Some(g) => g,
                None => {
                    tracing::warn!("Glyph '{}' not found in workspace", glyph_name);
                    return false;
                }
            };

            // Convert contours to paths
            let paths: Vec<Path> = glyph.contours.iter().map(Path::from_contour).collect();

            // Update session state with loaded paths AND glyph
            self.paths = Arc::new(paths);
            self.glyph = Arc::new(glyph.clone()); // Update glyph so to_glyph() has correct metadata
            self.active_sort_index = Some(index);
            self.active_sort_name = Some(glyph_name.clone());
            self.active_sort_unicode = codepoint.map(|c| format!("U+{:04X}", c as u32));
            self.active_sort_x_offset = x_offset;

            // Update buffer to mark this sort as active
            if let Some(buffer) = &mut self.text_buffer {
                buffer.set_active_sort(index);
            }

            tracing::info!(
                "Activated sort {} (glyph: {}, {} paths loaded, x_offset: {})",
                index,
                glyph_name,
                self.paths.len(),
                x_offset
            );

            return true;
        }

        false
    }

    /// Add a glyph to the text buffer by name (for component base glyph editing)
    ///
    /// This looks up the glyph in the workspace and inserts it into the text buffer
    /// at the current cursor position. Used when double-clicking a component to
    /// add its base glyph to the buffer for editing.
    ///
    /// Returns true if the glyph was added successfully.
    pub fn add_glyph_to_buffer(&mut self, glyph_name: &str) -> bool {
        // Get the glyph from workspace
        let workspace = match &self.workspace {
            Some(ws) => ws.clone(),
            None => return false,
        };

        let workspace_guard = read_workspace(&workspace);
        let glyph = match workspace_guard.glyphs.get(glyph_name) {
            Some(g) => g.clone(),
            None => {
                tracing::warn!("Glyph '{}' not found in workspace", glyph_name);
                return false;
            }
        };

        // Get the advance width
        let advance_width = glyph.width as f64;

        // Get the first codepoint if any
        let codepoint = glyph.codepoints.first().copied();

        // Drop the lock before we modify the buffer
        drop(workspace_guard);

        // Create a new sort for this glyph
        let sort = crate::sort::Sort::new_glyph(
            glyph_name.to_string(),
            codepoint,
            advance_width,
            false, // Not active initially
        );

        // Insert into buffer
        if let Some(buffer) = &mut self.text_buffer {
            buffer.insert(sort);
            tracing::info!("Added glyph '{}' to buffer for editing", glyph_name);
            true
        } else {
            false
        }
    }
}

// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Kerning and glyph property operations for AppState

use super::AppState;
use crate::model::workspace::Workspace;
use crate::model::{read_workspace, write_workspace};
use std::sync::{Arc, RwLock};

#[allow(dead_code)]
impl AppState {
    /// Update the glyph's advance width
    pub fn update_glyph_width(&mut self, new_width: String) {
        // Parse the width value
        let Ok(width) = new_width.parse::<f64>() else {
            return;
        };

        // Get workspace arc first (before borrowing session mutably)
        let workspace_arc = self.active_workspace();

        let Some(session) = &mut self.editor_session else {
            return;
        };

        // Update the glyph in the session
        let glyph = Arc::make_mut(&mut session.glyph);
        glyph.width = width;

        // Sync to workspace (inline to avoid borrow issues)
        if let Some(workspace_arc) = workspace_arc
            && let Some(active_name) = &session.active_sort_name
        {
            let updated_glyph = session.to_glyph();
            write_workspace(&workspace_arc).update_glyph(active_name, updated_glyph);
        }
    }

    /// Update the glyph's left kerning group
    pub fn update_left_group(&mut self, new_group: String) {
        // Get workspace arc first (before borrowing session mutably)
        let workspace_arc = self.active_workspace();

        let Some(session) = &mut self.editor_session else {
            return;
        };

        // Update the glyph in the session
        let glyph = Arc::make_mut(&mut session.glyph);
        glyph.left_group = if new_group.is_empty() || new_group == "-" {
            None
        } else {
            Some(new_group)
        };

        // Sync to workspace (inline to avoid borrow issues)
        if let Some(workspace_arc) = workspace_arc
            && let Some(active_name) = &session.active_sort_name
        {
            let updated_glyph = session.to_glyph();
            write_workspace(&workspace_arc).update_glyph(active_name, updated_glyph);
        }
    }

    /// Update the glyph's right kerning group
    pub fn update_right_group(&mut self, new_group: String) {
        // Get workspace arc first (before borrowing session mutably)
        let workspace_arc = self.active_workspace();

        let Some(session) = &mut self.editor_session else {
            return;
        };

        // Update the glyph in the session
        let glyph = Arc::make_mut(&mut session.glyph);
        glyph.right_group = if new_group.is_empty() || new_group == "-" {
            None
        } else {
            Some(new_group)
        };

        // Sync to workspace (inline to avoid borrow issues)
        if let Some(workspace_arc) = workspace_arc
            && let Some(active_name) = &session.active_sort_name
        {
            let updated_glyph = session.to_glyph();
            write_workspace(&workspace_arc).update_glyph(active_name, updated_glyph);
        }
    }

    /// Get the left kern value (kerning from previous glyph to current glyph)
    /// Returns None if there's no previous glyph or no kerning defined
    pub fn get_left_kern(&self) -> Option<f64> {
        let session = self.editor_session.as_ref()?;
        let workspace_arc = self.active_workspace()?;
        let buffer = session.text_buffer.as_ref()?;
        let active_index = session.active_sort_index?;

        // Can't have left kerning if we're the first glyph
        if active_index == 0 {
            return None;
        }

        // Get previous glyph
        let prev_sort = buffer.get(active_index - 1)?;
        let prev_name = match &prev_sort.kind {
            crate::sort::SortKind::Glyph { name, .. } => name,
            crate::sort::SortKind::LineBreak => return None,
        };

        // Get current glyph name
        let curr_name = session.active_sort_name.as_ref()?;

        // Look up kerning
        let workspace = read_workspace(&workspace_arc);
        let prev_glyph = workspace.get_glyph(prev_name)?;
        let curr_glyph = workspace.get_glyph(curr_name)?;

        let kern_value = crate::model::kerning::lookup_kerning(
            &workspace.kerning,
            &workspace.groups,
            prev_name,
            prev_glyph.right_group.as_deref(),
            curr_name,
            curr_glyph.left_group.as_deref(),
        );

        if kern_value == 0.0 {
            None
        } else {
            Some(kern_value)
        }
    }

    /// Get the right kern value (kerning from current glyph to next glyph)
    /// Returns None if there's no next glyph or no kerning defined
    pub fn get_right_kern(&self) -> Option<f64> {
        let session = self.editor_session.as_ref()?;
        let workspace_arc = self.active_workspace()?;
        let buffer = session.text_buffer.as_ref()?;
        let active_index = session.active_sort_index?;

        // Can't have right kerning if we're the last glyph
        if active_index + 1 >= buffer.len() {
            return None;
        }

        // Get next glyph
        let next_sort = buffer.get(active_index + 1)?;
        let next_name = match &next_sort.kind {
            crate::sort::SortKind::Glyph { name, .. } => name,
            crate::sort::SortKind::LineBreak => return None,
        };

        // Get current glyph name
        let curr_name = session.active_sort_name.as_ref()?;

        // Look up kerning
        let workspace = read_workspace(&workspace_arc);
        let curr_glyph = workspace.get_glyph(curr_name)?;
        let next_glyph = workspace.get_glyph(next_name)?;

        let kern_value = crate::model::kerning::lookup_kerning(
            &workspace.kerning,
            &workspace.groups,
            curr_name,
            curr_glyph.right_group.as_deref(),
            next_name,
            next_glyph.left_group.as_deref(),
        );

        if kern_value == 0.0 {
            None
        } else {
            Some(kern_value)
        }
    }

    /// Update the left kern value (kerning from previous glyph to current glyph)
    pub fn update_left_kern(&mut self, new_value: String) {
        let Some(session) = &self.editor_session else {
            return;
        };
        let Some(workspace_arc) = self.active_workspace() else {
            return;
        };
        let Some(buffer) = &session.text_buffer else {
            return;
        };
        let Some(active_index) = session.active_sort_index else {
            return;
        };

        // Can't set left kerning if we're the first glyph
        if active_index == 0 {
            return;
        }

        // Get previous glyph name
        let Some(prev_sort) = buffer.get(active_index - 1) else {
            return;
        };

        let prev_name = match &prev_sort.kind {
            crate::sort::SortKind::Glyph { name, .. } => name.clone(),
            crate::sort::SortKind::LineBreak => return,
        };

        // Get current glyph name
        let Some(curr_name) = session.active_sort_name.clone() else {
            return;
        };

        update_kern_pair(&workspace_arc, prev_name, curr_name, new_value);
    }

    /// Update the right kern value (kerning from current glyph to next glyph)
    pub fn update_right_kern(&mut self, new_value: String) {
        let Some(session) = &self.editor_session else {
            return;
        };
        let Some(workspace_arc) = self.active_workspace() else {
            return;
        };
        let Some(buffer) = &session.text_buffer else {
            return;
        };
        let Some(active_index) = session.active_sort_index else {
            return;
        };

        // Can't set right kerning if we're the last glyph
        if active_index + 1 >= buffer.len() {
            return;
        }

        // Get next glyph name
        let Some(next_sort) = buffer.get(active_index + 1) else {
            return;
        };

        let next_name = match &next_sort.kind {
            crate::sort::SortKind::Glyph { name, .. } => name.clone(),
            crate::sort::SortKind::LineBreak => return,
        };

        // Get current glyph name
        let Some(curr_name) = session.active_sort_name.clone() else {
            return;
        };

        update_kern_pair(&workspace_arc, curr_name, next_name, new_value);
    }
}

/// Parse a kern value string and update (or remove) the kerning pair.
///
/// If `new_value` is empty or "-", removes the pair. Otherwise parses
/// as `f64` and inserts into the workspace kerning table.
fn update_kern_pair(
    workspace_arc: &Arc<RwLock<Workspace>>,
    first_name: String,
    second_name: String,
    new_value: String,
) {
    if new_value.is_empty() || new_value == "-" {
        let mut workspace = write_workspace(workspace_arc);
        if let Some(first_pairs) = workspace.kerning.get_mut(&first_name) {
            first_pairs.remove(&second_name);
        }
        return;
    }

    let Ok(kern_value) = new_value.parse::<f64>() else {
        return;
    };

    let mut workspace = write_workspace(workspace_arc);
    workspace
        .kerning
        .entry(first_name)
        .or_default()
        .insert(second_name, kern_value);
}

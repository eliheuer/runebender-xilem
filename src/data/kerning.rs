// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Kerning and glyph property operations for AppState

use super::AppState;
use std::sync::Arc;

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

        let session = match &mut self.editor_session {
            Some(s) => s,
            None => return,
        };

        // Update the glyph in the session
        let glyph = Arc::make_mut(&mut session.glyph);
        glyph.width = width;

        // Sync to workspace (inline to avoid borrow issues)
        if let Some(workspace_arc) = workspace_arc
            && let Some(active_name) = &session.active_sort_name
        {
            let updated_glyph = session.to_glyph();
            workspace_arc
                .write()
                .unwrap()
                .update_glyph(active_name, updated_glyph);
        }
    }

    /// Update the glyph's left kerning group
    pub fn update_left_group(&mut self, new_group: String) {
        // Get workspace arc first (before borrowing session mutably)
        let workspace_arc = self.active_workspace();

        let session = match &mut self.editor_session {
            Some(s) => s,
            None => return,
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
            workspace_arc
                .write()
                .unwrap()
                .update_glyph(active_name, updated_glyph);
        }
    }

    /// Update the glyph's right kerning group
    pub fn update_right_group(&mut self, new_group: String) {
        // Get workspace arc first (before borrowing session mutably)
        let workspace_arc = self.active_workspace();

        let session = match &mut self.editor_session {
            Some(s) => s,
            None => return,
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
            workspace_arc
                .write()
                .unwrap()
                .update_glyph(active_name, updated_glyph);
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
        let workspace = workspace_arc.read().unwrap();
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
        let workspace = workspace_arc.read().unwrap();
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
        let session = match &self.editor_session {
            Some(s) => s,
            None => return,
        };

        let workspace_arc = match self.active_workspace() {
            Some(w) => w,
            None => return,
        };

        let buffer = match &session.text_buffer {
            Some(b) => b,
            None => return,
        };

        let active_index = match session.active_sort_index {
            Some(i) => i,
            None => return,
        };

        // Can't set left kerning if we're the first glyph
        if active_index == 0 {
            return;
        }

        // Get previous glyph name
        let prev_sort = match buffer.get(active_index - 1) {
            Some(s) => s,
            None => return,
        };

        let prev_name = match &prev_sort.kind {
            crate::sort::SortKind::Glyph { name, .. } => name.clone(),
            crate::sort::SortKind::LineBreak => return,
        };

        // Get current glyph name
        let curr_name = match &session.active_sort_name {
            Some(n) => n.clone(),
            None => return,
        };

        // Parse the new value (empty string means remove kerning)
        let kern_value = if new_value.is_empty() || new_value == "-" {
            // Remove kerning pair
            let mut workspace = workspace_arc.write().unwrap();
            if let Some(first_pairs) = workspace.kerning.get_mut(&prev_name) {
                first_pairs.remove(&curr_name);
            }
            return;
        } else {
            match new_value.parse::<f64>() {
                Ok(v) => v,
                Err(_) => return, // Invalid number, ignore
            }
        };

        // Update kerning in workspace
        let mut workspace = workspace_arc.write().unwrap();
        workspace
            .kerning
            .entry(prev_name.clone())
            .or_insert_with(std::collections::HashMap::new)
            .insert(curr_name, kern_value);
    }

    /// Update the right kern value (kerning from current glyph to next glyph)
    pub fn update_right_kern(&mut self, new_value: String) {
        let session = match &self.editor_session {
            Some(s) => s,
            None => return,
        };

        let workspace_arc = match self.active_workspace() {
            Some(w) => w,
            None => return,
        };

        let buffer = match &session.text_buffer {
            Some(b) => b,
            None => return,
        };

        let active_index = match session.active_sort_index {
            Some(i) => i,
            None => return,
        };

        // Can't set right kerning if we're the last glyph
        if active_index + 1 >= buffer.len() {
            return;
        }

        // Get next glyph name
        let next_sort = match buffer.get(active_index + 1) {
            Some(s) => s,
            None => return,
        };

        let next_name = match &next_sort.kind {
            crate::sort::SortKind::Glyph { name, .. } => name.clone(),
            crate::sort::SortKind::LineBreak => return,
        };

        // Get current glyph name
        let curr_name = match &session.active_sort_name {
            Some(n) => n.clone(),
            None => return,
        };

        // Parse the new value (empty string means remove kerning)
        let kern_value = if new_value.is_empty() || new_value == "-" {
            // Remove kerning pair
            let mut workspace = workspace_arc.write().unwrap();
            if let Some(first_pairs) = workspace.kerning.get_mut(&curr_name) {
                first_pairs.remove(&next_name);
            }
            return;
        } else {
            match new_value.parse::<f64>() {
                Ok(v) => v,
                Err(_) => return, // Invalid number, ignore
            }
        };

        // Update kerning in workspace
        let mut workspace = workspace_arc.write().unwrap();
        workspace
            .kerning
            .entry(curr_name.clone())
            .or_insert_with(std::collections::HashMap::new)
            .insert(next_name, kern_value);
    }
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! One application history entry and cache refresh for each canonical agent group.

use std::collections::BTreeSet;
use std::sync::Arc;

use runebender::document::history::HistoryDirection;
use runebender::document::project::{
    DocumentChange, DocumentEditHistoryReplayOutcome, EditHistoryGroupId,
};

use crate::application::view::canvas::grid::cells_of;
use crate::application::workspace::{MetadataEdit, Mode, Workspace};

impl Workspace {
    pub(crate) fn record_agent_group(
        &mut self,
        group: EditHistoryGroupId,
        change: &DocumentChange,
    ) {
        self.metadata_undo.retain(|edit| match edit {
            MetadataEdit::AgentGroup { group, .. } => self
                .font
                .project
                .document_edit_history_group_state(*group)
                .is_some(),
            _ => true,
        });
        self.metadata_undo.push(MetadataEdit::AgentGroup {
            group,
            addresses: change.affected_layers().to_vec(),
            overview_undo_depth: self.overview_undo.len(),
        });
        self.metadata_redo.clear();
        self.overview_redo.clear();
        self.refresh_agent_change(change);
    }

    /// Refresh only active-source views; switching sources rebuilds other source views canonically.
    fn refresh_agent_change(&mut self, change: &DocumentChange) {
        let active_source = self.font.project.source_id(self.font.active());
        let addresses = change
            .affected_layers()
            .iter()
            .chain(change.dependent_layers());
        let names = addresses
            .filter(|address| Some(address.layer.source) == active_source)
            .filter(|address| {
                self.font.active_layer_address(&address.glyph).as_ref() == Some(*address)
            })
            .map(|address| address.glyph.clone())
            .collect::<BTreeSet<_>>();
        for name in &names {
            if let Some(index) = self.font.index_of(name) {
                self.font.refresh_entry(index);
            }
        }
        for tab in &mut self.tabs {
            if names.contains(&tab.session.glyph_name)
                && let Some(address) = self.font.active_layer_address(&tab.session.glyph_name)
            {
                Arc::make_mut(&mut tab.session).reload_from_project(&self.font.project, &address);
            }
        }
        if names.contains(&self.session.glyph_name)
            && let Some(address) = self.font.active_layer_address(&self.session.glyph_name)
        {
            Arc::make_mut(&mut self.session).reload_from_project(&self.font.project, &address);
            if let Some(tab) = self.tabs.get_mut(self.active_tab)
                && tab.session.glyph_name == self.session.glyph_name
            {
                tab.session = self.session.clone();
            }
            self.selected_points = self.session.selection.len();
            self.refresh_metric_bufs();
            self.refresh_coord_bufs();
        }
        if !names.is_empty() {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
        }
        self.modified |= self.font.project.is_modified();
    }

    pub(crate) fn can_agent_metadata_step(&self, edit: &MetadataEdit, redo: bool) -> bool {
        let MetadataEdit::AgentGroup {
            group,
            addresses,
            overview_undo_depth,
        } = edit
        else {
            return false;
        };
        if self.session.gesture_in_progress()
            || (matches!(self.mode, Mode::Overview)
                && self.overview_undo.len() != *overview_undo_depth)
        {
            return false;
        }
        let glyph = match self.mode {
            Mode::Editor(_) => Some(self.session.glyph_name.as_str()),
            Mode::Overview => self
                .selected
                .and_then(|index| self.font.glyphs.get(index))
                .map(|glyph| glyph.name.as_str()),
            Mode::Nodes => None,
        };
        let Some(address) = glyph.and_then(|glyph| self.font.active_layer_address(glyph)) else {
            return false;
        };
        // Auxiliary edits belong to this glyph's history even though its foreground is displayed.
        addresses.iter().any(|target| {
            target.glyph == address.glyph && target.layer.source == address.layer.source
        }) && self
            .font
            .project
            .check_document_edit_history_group(*group, direction(redo))
            .is_ok()
    }

    pub(crate) fn agent_metadata_step(&mut self, edit: &MetadataEdit, redo: bool) -> bool {
        if !self.can_agent_metadata_step(edit, redo) {
            return false;
        }
        let MetadataEdit::AgentGroup { group, .. } = edit else {
            return false;
        };
        self.replay_agent_group(*group, direction(redo)).is_ok()
    }

    /// Targeted and ordinary history both move this same entry after one guarded engine replay.
    pub(crate) fn replay_agent_group(
        &mut self,
        group: EditHistoryGroupId,
        direction: HistoryDirection,
    ) -> Result<DocumentEditHistoryReplayOutcome, String> {
        if self.session.gesture_in_progress() {
            return Err("finish the canvas gesture before history replay".into());
        }
        let redo = direction == HistoryDirection::Redo;
        let entries = if redo {
            &self.metadata_redo
        } else {
            &self.metadata_undo
        };
        let position = entries.iter().position(|edit| matches!(edit, MetadataEdit::AgentGroup { group: candidate, .. } if *candidate == group))
            .ok_or("group has no application history entry in this direction; inspect its current state")?;
        let result = self
            .font
            .project
            .replay_document_edit_history_group(group, direction)
            .map_err(|error| error.to_string())?;
        let mut edit = if redo {
            self.metadata_redo.remove(position)
        } else {
            self.metadata_undo.remove(position)
        };
        if let MetadataEdit::AgentGroup {
            overview_undo_depth,
            ..
        } = &mut edit
        {
            *overview_undo_depth = self.overview_undo.len();
        }
        if redo {
            self.metadata_undo.push(edit);
        } else {
            self.metadata_redo.push(edit);
        }
        self.refresh_agent_change(&result.change);
        let label = self
            .font
            .project
            .document_edit_history_group_name(group)
            .unwrap_or("agent edit");
        self.note = format!("{} {label}", if redo { "Redid" } else { "Undid" });
        Ok(result)
    }
}

fn direction(redo: bool) -> HistoryDirection {
    if redo {
        HistoryDirection::Redo
    } else {
        HistoryDirection::Undo
    }
}

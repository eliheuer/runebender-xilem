// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Source/layer authoring commands and refresh of source-dependent editor state.

use std::sync::Arc;

use runebender::font::variable::LayerId;

use crate::application::{
    view::canvas::grid::cells_of,
    workspace::{Mode, Workspace},
};

impl Workspace {
    pub(crate) fn change_sources(&mut self, action: &str) {
        if self.features_edited {
            self.note = "Apply or Revert feature edits before changing sources".into();
            return;
        }
        self.park();
        let index = self.font.active();
        let id = self
            .font
            .project
            .source_id(index)
            .expect("active source identity");
        let location = self
            .font
            .axes
            .iter()
            .zip(&self.axis_values)
            .map(|(axis, value)| (axis.name.clone(), axis.user_to_normalized(*value)))
            .collect();
        let name = if self.source_name_buf.trim().is_empty() {
            format!("Source {}", self.font.master_count() + 1)
        } else {
            self.source_name_buf.trim().to_owned()
        };
        let previous_ids: Vec<_> = (0..self.font.master_count())
            .map(|index| self.font.project.source_id(index).expect("source identity"))
            .collect();
        let result = match action {
            "add" => {
                let filename: String = name
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || c == '-' || c == '_' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                self.font
                    .project
                    .add_interpolated_source(&name, &format!("{filename}.ufo"), &location)
                    .map(|_| ())
            }
            "update" => self.font.project.update_source(id, &name, &location),
            "remove" => self.font.project.remove_source(id),
            "up" => self
                .font
                .project
                .move_source(id, index.saturating_sub(1))
                .map(|_| ()),
            "down" => self
                .font
                .project
                .move_source(id, (index + 1).min(self.font.master_count() - 1))
                .map(|_| ()),
            "undo" => self.font.project.undo_sources(false).map(|_| ()),
            "redo" => self.font.project.undo_sources(true).map(|_| ()),
            "layer-add" => {
                let from = LayerId {
                    source: id,
                    name: self
                        .font
                        .project
                        .document_source(id)
                        .expect("active source remains in the document")
                        .default_layer()
                        .name,
                };
                self.font
                    .project
                    .add_glyph_layer(&self.session.glyph_name, &from, self.layer_name_buf.trim())
                    .map(|_| ())
            }
            "layer-remove" => self.font.project.remove_glyph_layer(
                &self.session.glyph_name,
                &LayerId {
                    source: id,
                    name: self.layer_name_buf.trim().into(),
                },
            ),
            _ => return,
        };
        if let Err(error) = result {
            self.note = error;
            return;
        }
        self.reference_layers = self
            .reference_layers
            .iter()
            .filter_map(|index| {
                previous_ids
                    .get(*index)
                    .and_then(|id| self.font.project.source_index(*id))
            })
            .collect();
        self.font.rebuild_cache();
        self.refresh_source_views();
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.modified = true;
        self.note = match action {
            "add" => "Added an interpolated source; Save writes its new UFO",
            "remove" => "Removed source from the project; its UFO remains on disk",
            "undo" => "Undid source/layer change",
            "redo" => "Redid source/layer change",
            "layer-add" => "Added glyph layer",
            "layer-remove" => "Removed glyph layer",
            _ => "Updated sources",
        }
        .into();
    }

    pub(crate) fn can_manage_glyph_layers(&self) -> bool {
        matches!(self.mode, Mode::Editor(_))
    }
}

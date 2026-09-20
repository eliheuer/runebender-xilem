// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Live metaball selection, gestures, and inspector commands.

use crate::application::editor::session::Session;
use crate::application::view::canvas::grid::cells_of;
use crate::application::workspace::{Mode, OverviewEditBatch, Workspace};
use runebender::formats::metaballs::{Metaball, MetaballGroup};
use runebender::outline::metaballs::OutlineOptions;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Default)]
pub(crate) struct MetaballSelection {
    pub selected: HashSet<(u32, u32)>,
    pub active_group: Option<u32>,
    pub drafts: HashMap<&'static str, String>,
    pub error: Option<String>,
}

impl Session {
    pub(crate) fn refresh_metaball_preview(&mut self) {
        self.metaballs.drafts.clear();
        let source = self.metaball_data().map_err(|error| error.to_string());
        match source.and_then(|source| {
            let mut path = kurbo::BezPath::new();
            for group in &source.groups {
                for contour in
                    runebender::outline::metaballs::preview(group, OutlineOptions::default())?
                {
                    path.extend(contour);
                }
            }
            Ok((source, path))
        }) {
            Ok((source, path)) => {
                self.metaball_preview = path;
                self.metaballs.error = None;
                self.metaballs.selected.retain(|(g, b)| {
                    source
                        .groups
                        .iter()
                        .any(|group| group.id == *g && group.balls.iter().any(|ball| ball.id == *b))
                });
            }
            Err(error) => {
                self.metaball_preview = kurbo::BezPath::new();
                self.metaballs.error = Some(error);
            }
        }
    }

    pub(crate) fn metaball_click(&mut self, at: kurbo::Point, radius: f64, shift: bool) -> bool {
        let result = (|| {
            let mut source = self.metaball_data().map_err(|error| error.to_string())?;
            let hit = source
                .groups
                .iter()
                .flat_map(|g| g.balls.iter().map(move |b| (g.id, b)))
                .filter(|(_, b)| kurbo::Point::new(b.x, b.y).distance(at) <= radius)
                .min_by(|(_, a), (_, b)| {
                    kurbo::Point::new(a.x, a.y)
                        .distance(at)
                        .total_cmp(&kurbo::Point::new(b.x, b.y).distance(at))
                })
                .map(|(g, b)| (g, b.id));
            self.selection.clear();
            self.selected_anchor = None;
            self.selected_component = None;
            self.metaballs.drafts.clear();
            if let Some(id) = hit {
                if shift {
                    if !self.metaballs.selected.insert(id) {
                        self.metaballs.selected.remove(&id);
                    }
                } else if !self.metaballs.selected.contains(&id) {
                    self.metaballs.selected.clear();
                    self.metaballs.selected.insert(id);
                }
                self.metaballs.active_group = Some(id.0);
                return Ok(false);
            }
            let group_index = self
                .metaballs
                .active_group
                .and_then(|id| source.groups.iter().position(|g| g.id == id));
            let index = if let Some(index) = group_index {
                index
            } else {
                let id = source
                    .groups
                    .iter()
                    .map(|g| g.id)
                    .max()
                    .unwrap_or(0)
                    .checked_add(1)
                    .ok_or("group identifiers exhausted")?;
                source.groups.push(MetaballGroup {
                    id,
                    threshold: 0.5,
                    balls: vec![],
                });
                source.groups.len() - 1
            };
            let group = &mut source.groups[index];
            let id = group
                .balls
                .iter()
                .map(|b| b.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("center identifiers exhausted")?;
            group.balls.push(Metaball {
                id,
                x: at.x,
                y: at.y,
                radius: self.metrics.upm * 0.18,
                stiffness: 2.0,
            });
            let selected = (group.id, id);
            let changed = self.store_metaballs(source, true)?;
            self.refresh_metaball_preview();
            self.metaballs.selected = HashSet::from([selected]);
            self.metaballs.active_group = Some(selected.0);
            Ok::<_, String>(changed)
        })();
        self.metaball_result(result)
    }

    fn metaball_result(&mut self, result: Result<bool, String>) -> bool {
        match result {
            Ok(changed) => changed,
            Err(error) => {
                self.metaballs.error = Some(error);
                false
            }
        }
    }

    pub(crate) fn move_metaballs(&mut self, delta: kurbo::Vec2, drag: bool) -> bool {
        let result = (|| {
            let mut source = self.metaball_data().map_err(|error| error.to_string())?;
            for group in &mut source.groups {
                for ball in &mut group.balls {
                    if self.metaballs.selected.contains(&(group.id, ball.id)) {
                        ball.x += delta.x;
                        ball.y += delta.y;
                    }
                }
            }
            let changed = self.store_metaballs(source, drag)?;
            self.refresh_metaball_preview();
            Ok(changed)
        })();
        self.metaball_result(result)
    }

    pub(crate) fn select_all_metaballs(&mut self) {
        if let Ok(source) = self.metaball_data() {
            self.metaballs.selected = source
                .groups
                .iter()
                .flat_map(|g| g.balls.iter().map(move |b| (g.id, b.id)))
                .collect();
            self.metaballs.drafts.clear();
        }
    }

    pub(crate) fn delete_metaballs(&mut self) -> bool {
        let result = (|| {
            let mut source = self.metaball_data().map_err(|error| error.to_string())?;
            for g in &mut source.groups {
                g.balls
                    .retain(|b| !self.metaballs.selected.contains(&(g.id, b.id)));
            }
            source.groups.retain(|g| !g.balls.is_empty());
            self.store_metaballs(source, false)
        })();
        let changed = self.metaball_result(result);
        if changed {
            self.metaballs.selected.clear();
        }
        changed
    }

    pub(crate) fn metaball_value(&self, field: &'static str) -> String {
        if let Some(value) = self.metaballs.drafts.get(field) {
            return value.clone();
        }
        let Ok(source) = self.metaball_data() else {
            return String::new();
        };
        let mut values = source
            .groups
            .iter()
            .flat_map(|g| g.balls.iter().map(move |b| (g, b)))
            .filter(|(g, b)| self.metaballs.selected.contains(&(g.id, b.id)))
            .map(|(g, b)| match field {
                "X" => b.x,
                "Y" => b.y,
                "Radius" => b.radius,
                "Strength" => b.stiffness,
                _ => g.threshold,
            });
        let Some(value) = values.next() else {
            return String::new();
        };
        if values.any(|v| v != value) {
            String::new()
        } else {
            format!("{value:.2}")
        }
    }

    pub(crate) fn set_metaball_value(&mut self, field: &'static str, value: String) -> bool {
        let result = (|| {
            let value = value
                .parse::<f64>()
                .map_err(|_| "Enter a number".to_string())?;
            let mut source = self.metaball_data().map_err(|error| error.to_string())?;
            for g in &mut source.groups {
                for b in &mut g.balls {
                    if self.metaballs.selected.contains(&(g.id, b.id)) {
                        match field {
                            "X" => b.x = value,
                            "Y" => b.y = value,
                            "Radius" => b.radius = value,
                            "Strength" => b.stiffness = value,
                            _ => g.threshold = value,
                        }
                    }
                }
            }
            self.store_metaballs(source, false)
        })();
        self.metaball_result(result)
    }

    pub(crate) fn collapse_metaballs(&mut self, selected: bool) -> bool {
        let result = (|| {
            let ids: Vec<_> = self.metaballs.selected.iter().map(|id| id.0).collect();
            if selected && ids.is_empty() {
                return Ok(false);
            }
            let changed = self.stage_canonical_string_edit("collapse metaballs", |draft| {
                draft
                    .collapse_metaballs(
                        selected.then_some(ids.as_slice()),
                        OutlineOptions::default(),
                    )
                    .map(|count| count > 0)
            })?;
            if !changed {
                return Ok(false);
            }
            self.metaballs = MetaballSelection::default();
            self.refresh_metaball_preview();
            Ok(true)
        })();
        self.metaball_result(result)
    }
}

impl Workspace {
    pub(crate) fn edit_metaballs(&mut self, edit: impl FnOnce(&mut Session) -> bool) {
        let changed = edit(Arc::make_mut(&mut self.session));
        if changed {
            self.refresh_open_glyph();
        }
    }
}

impl Workspace {
    pub(crate) fn collapse_font_metaballs(&mut self) {
        if !matches!(self.mode, Mode::Overview) {
            return;
        }
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            self.note = "The active source is unavailable".into();
            return;
        };
        let Some(layer) = self
            .font
            .project
            .document_source(source)
            .map(|source| source.default_layer())
        else {
            self.note = "The active source layer is unavailable".into();
            return;
        };
        let candidates = self
            .font
            .glyphs
            .iter()
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();
        let mut names = Vec::new();
        for name in candidates {
            let address = runebender::document::variable::GlyphLayerAddress {
                glyph: name.clone(),
                layer: layer.clone(),
            };
            let Ok(mut transaction) = self.font.project.begin_document_layer_transaction(&address)
            else {
                continue;
            };
            let collapsed = match transaction
                .draft_mut()
                .collapse_metaballs(None, OutlineOptions::default())
            {
                Ok(collapsed) => collapsed,
                Err(error) => {
                    self.note = error;
                    return;
                }
            };
            if collapsed == 0 {
                continue;
            }
            if matches!(
                self.font
                    .project
                    .commit_document_layer_transaction(transaction),
                Ok(runebender::document::project::DocumentEditOutcome::Changed { .. })
            ) {
                names.push(name);
            }
        }
        if names.is_empty() {
            self.note = "No live metaballs in this master".into();
            return;
        }
        self.overview_undo.push(OverviewEditBatch {
            source,
            glyphs: names,
        });
        self.overview_redo.clear();
        self.font.rebuild_cache();
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.modified = true;
        self.note = "Converted this master's live metaballs to cubic contours".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::workspace::Tool;

    fn projected_glyph(session: &Session) -> norad::Glyph {
        session
            .projected_glyph()
            .expect("an editor session has a canonical layer")
    }

    #[test]
    fn live_sources_save_reopen_convert_and_undo_in_xilem() {
        let path = std::env::temp_dir().join(format!("xilem-metaballs-{}.ufo", std::process::id()));
        let mut font = norad::Font::new();
        for name in ["i", "j"] {
            let mut g = norad::Glyph::new(name);
            g.width = 500.0;
            font.default_layer_mut().insert_glyph(g);
        }
        font.save(&path).unwrap();
        let mut app = Workspace::open(&path).unwrap();
        for name in ["i", "j"] {
            app.open_glyph(app.font.index_of(name).unwrap());
            app.select_tool(Tool::Metaball);
            app.edit_metaballs(|s| {
                let changed = s.metaball_click(kurbo::Point::new(250.0, 200.0), 10.0, false);
                s.end_metaball_drag();
                changed
            });
            app.edit_metaballs(|s| {
                let changed = s.metaball_click(kurbo::Point::new(250.0, 400.0), 10.0, false);
                s.end_metaball_drag();
                changed
            });
            Arc::make_mut(&mut app.session).select_all_metaballs();
            app.edit_metaballs(|s| s.set_metaball_value("Radius", "200".into()));
            assert!(app.session.outline_is_empty());
            assert_eq!(
                app.session.metaball_data().unwrap().groups[0].balls.len(),
                2
            );
            let before = projected_glyph(&app.session);
            app.edit_metaballs(|s| s.set_metaball_value("Radius", "NaN".into()));
            assert_eq!(
                projected_glyph(&app.session),
                before,
                "invalid input preserves live source"
            );
            app.edit_metaballs(|s| s.collapse_metaballs(false));
            assert!(!app.session.outline_is_empty());
            app.undo_open_glyph(false);
            assert_eq!(
                projected_glyph(&app.session),
                before,
                "conversion undo restores exact source"
            );
            app.undo_open_glyph(true);
            assert!(!app.session.outline_is_empty());
            app.undo_open_glyph(false);
        }
        app.font.font_snapshot().save(&path).unwrap();
        let reopened = norad::Font::load(&path).unwrap();
        assert_eq!(
            runebender::formats::metaballs::read_metaballs(reopened.get_glyph("i").unwrap())
                .unwrap()
                .groups[0]
                .balls
                .len(),
            2
        );
        app.back_to_overview();
        app.collapse_font_metaballs();
        for name in ["i", "j"] {
            assert!(
                !app.font
                    .font_snapshot()
                    .get_glyph(name)
                    .unwrap()
                    .contours
                    .is_empty()
            );
        }
        app.undo_active_edit(false);
        for name in ["i", "j"] {
            assert!(
                app.font
                    .font_snapshot()
                    .get_glyph(name)
                    .unwrap()
                    .contours
                    .is_empty()
            );
        }
        app.undo_active_edit(true);
        app.open_glyph(app.font.index_of("i").unwrap());
        assert!(
            !app.session.outline_is_empty(),
            "parked tabs load the converted glyph"
        );
        std::fs::remove_dir_all(path).unwrap();
    }
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Live metaball selection, gestures, and inspector commands.

use crate::application::editor::session::Session;
use crate::application::view::canvas::grid::cells_of;
use crate::application::workspace::{Mode, OverviewEditBatch, Workspace};
use runebender::formats::metaballs::{Metaball, MetaballGroup, MetaballLink};
use runebender::outline::metaballs::OutlineOptions;
use runebender::outline::metaballs::parameters::{set_size_and_reach, visible_size};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Default)]
pub(crate) struct MetaballSelection {
    pub selected: HashSet<(u32, u32)>,
    pub selected_link: Option<(u32, u32)>,
    size_controls_during_drag: Option<bool>,
    slider_range: Option<(String, f64, f64)>,
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
                self.metaballs.selected_link = self.metaballs.selected_link.filter(|(g, l)| {
                    source
                        .groups
                        .iter()
                        .any(|group| group.id == *g && group.links.iter().any(|link| link.id == *l))
                });
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
                self.metaballs.selected_link = None;
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
            let link_hit = source.groups.iter().find_map(|g| {
                g.links.iter().find_map(|link| {
                    let a = g.balls.iter().find(|b| b.id == link.start)?;
                    let b = g.balls.iter().find(|b| b.id == link.end)?;
                    let midpoint = kurbo::Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                    (midpoint.distance(at) <= radius).then_some((g.id, link.id))
                })
            });
            if let Some(id) = link_hit {
                self.metaballs.selected.clear();
                self.metaballs.selected_link = Some(id);
                self.metaballs.active_group = Some(id.0);
                return Ok(false);
            }
            self.metaballs.selected_link = None;
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
                    links: vec![],
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
                let endpoints = self.metaballs.selected_link.and_then(|(g, id)| {
                    (g == group.id)
                        .then(|| group.links.iter().find(|l| l.id == id))
                        .flatten()
                        .map(|l| (l.start, l.end))
                });
                for ball in &mut group.balls {
                    if self.metaballs.selected.contains(&(group.id, ball.id))
                        || endpoints.is_some_and(|(a, b)| ball.id == a || ball.id == b)
                    {
                        ball.x += delta.x;
                        ball.y += delta.y;
                    }
                }
            }
            source.validate()?;
            let changed = self.store_metaballs(source, drag)?;
            self.refresh_metaball_preview();
            Ok(changed)
        })();
        self.metaball_result(result)
    }

    pub(crate) fn select_all_metaballs(&mut self) {
        self.metaballs.selected_link = None;
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
                g.links.retain(|link| {
                    self.metaballs.selected_link != Some((g.id, link.id))
                        && g.balls.iter().any(|b| b.id == link.start)
                        && g.balls.iter().any(|b| b.id == link.end)
                });
            }
            source.groups.retain(|g| !g.balls.is_empty());
            self.store_metaballs(source, false)
        })();
        let changed = self.metaball_result(result);
        if changed {
            self.metaballs.selected.clear();
            self.metaballs.selected_link = None;
            self.refresh_metaball_preview();
        }
        changed
    }

    /// Only a new, same-group pair can become a connection.
    pub(crate) fn metaball_connect_pair(&self) -> Option<(u32, u32, u32)> {
        if self.metaballs.selected.len() != 2 || self.metaballs.selected_link.is_some() {
            return None;
        }
        let mut selected: Vec<_> = self.metaballs.selected.iter().copied().collect();
        selected.sort_unstable();
        let [(g, a), (other, b)] = selected.as_slice() else {
            return None;
        };
        if g != other {
            return None;
        }
        let source = self.metaball_data().ok()?;
        let group = source.groups.iter().find(|group| group.id == *g)?;
        if !group.balls.iter().any(|ball| ball.id == *a)
            || !group.balls.iter().any(|ball| ball.id == *b)
            || group
                .links
                .iter()
                .any(|l| (l.start == *a && l.end == *b) || (l.start == *b && l.end == *a))
        {
            return None;
        }
        Some((*g, *a, *b))
    }

    pub(crate) fn connect_metaballs(&mut self) -> bool {
        let result = (|| {
            let Some((g, start, end)) = self.metaball_connect_pair() else {
                return Ok(false);
            };
            let mut source = self.metaball_data().map_err(|e| e.to_string())?;
            let group = source
                .groups
                .iter_mut()
                .find(|group| group.id == g)
                .ok_or("Missing metaball group")?;
            let id = group
                .links
                .iter()
                .map(|l| l.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("Connection identifiers exhausted")?;
            group.links.push(MetaballLink {
                id,
                start,
                end,
                width: (self.metrics.upm * 0.03).clamp(2.0, 100_000.0),
            });
            source.version = 2;
            source.validate()?;
            let changed = self.store_metaballs(source, false)?;
            if changed {
                self.metaballs.selected.clear();
                self.metaballs.selected_link = Some((g, id));
                self.refresh_metaball_preview();
            }
            Ok(changed)
        })();
        self.metaball_result(result)
    }

    pub(crate) fn metaball_selection_has_negative(&self) -> bool {
        self.metaball_values("Strength")
            .iter()
            .any(|value| *value < 0.0)
    }

    pub(crate) fn metaball_slider_range(&self, field: &str) -> (f64, f64, f64) {
        let upm = self.metrics.upm;
        let (min, max, step): (f64, f64, f64) = match field {
            "X" | "Y" => (-2.0 * upm, 2.0 * upm, 1.0),
            "Radius" | "Size" | "Blend reach" => (1.0, upm, 1.0),
            "Width" => (2.0, upm, 1.0),
            "Strength" => (-5.0, 5.0, 0.01),
            _ => (0.01, 2.0, 0.01),
        };
        if let Some((active, min, max)) = &self.metaballs.slider_range
            && active == field
        {
            return (*min, *max, step);
        }
        let values = self.metaball_values(field);
        (
            values.iter().copied().fold(min, f64::min),
            values.iter().copied().fold(max, f64::max),
            step,
        )
    }

    pub(crate) fn set_metaball_sign(&mut self, positive: bool) -> bool {
        let result = (|| {
            let mut source = self.metaball_data().map_err(|e| e.to_string())?;
            for g in &mut source.groups {
                for b in &mut g.balls {
                    if self.metaballs.selected.contains(&(g.id, b.id)) {
                        b.stiffness = b.stiffness.abs() * if positive { 1.0 } else { -1.0 };
                    }
                }
            }
            let changed = self.store_metaballs(source, false)?;
            if changed {
                self.refresh_metaball_preview();
            }
            Ok(changed)
        })();
        self.metaball_result(result)
    }

    pub(crate) fn metaball_uses_size_controls(&self) -> bool {
        if let Some(size_controls) = self.metaballs.size_controls_during_drag {
            return size_controls;
        }
        let Ok(source) = self.metaball_data() else {
            return false;
        };
        source.groups.iter().all(|g| {
            g.balls.iter().all(|b| {
                !self.metaballs.selected.contains(&(g.id, b.id))
                    || visible_size(b, g.threshold).is_some()
            })
        })
    }

    fn metaball_values(&self, field: &str) -> Vec<f64> {
        let Ok(source) = self.metaball_data() else {
            return Vec::new();
        };
        if field == "Width" {
            return source
                .groups
                .iter()
                .flat_map(|g| {
                    g.links.iter().filter_map(move |l| {
                        (self.metaballs.selected_link == Some((g.id, l.id))).then_some(l.width)
                    })
                })
                .collect();
        }
        source
            .groups
            .iter()
            .flat_map(|g| {
                g.balls.iter().filter_map(move |b| {
                    if !self.metaballs.selected.contains(&(g.id, b.id)) {
                        return None;
                    }
                    Some(match field {
                        "X" => b.x,
                        "Y" => b.y,
                        "Radius" => b.radius,
                        "Strength" => b.stiffness,
                        "Size" => visible_size(b, g.threshold)?.0,
                        "Blend reach" => visible_size(b, g.threshold)?.1,
                        "Threshold" => g.threshold,
                        _ => return None,
                    })
                })
            })
            .collect()
    }

    pub(crate) fn metaball_value(&self, field: &'static str) -> String {
        if let Some(value) = self.metaballs.drafts.get(field) {
            return value.clone();
        }
        let mut values = self.metaball_values(field).into_iter();
        let Some(value) = values.next() else {
            return String::new();
        };
        if values.any(|v| v != value) {
            String::new()
        } else {
            format!("{value:.2}")
        }
    }

    /// The selection mean positions a mixed-value slider; X/Y move the selection together.
    pub(crate) fn metaball_slider_value(&self, field: &str) -> f64 {
        let values = self.metaball_values(field);
        let (sum, count) = values
            .iter()
            .fold((0.0, 0.0), |(sum, count), value| (sum + value, count + 1.0));
        if count == 0.0 { 0.0 } else { sum / count }
    }

    pub(crate) fn set_metaball_value(&mut self, field: &str, value: f64, drag: bool) -> bool {
        // Keep the captured slider in place if a raw strength crosses the visible threshold.
        if drag && self.metaballs.size_controls_during_drag.is_none() {
            self.metaballs.size_controls_during_drag = Some(self.metaball_uses_size_controls());
            let (min, max, _) = self.metaball_slider_range(field);
            self.metaballs.slider_range = Some((field.into(), min, max));
        }
        let offset = value - self.metaball_slider_value(field);
        let result = (|| {
            let mut source = self.metaball_data().map_err(|error| error.to_string())?;
            for g in &mut source.groups {
                if field == "Width" {
                    for link in &mut g.links {
                        if self.metaballs.selected_link == Some((g.id, link.id)) {
                            link.width = value;
                        }
                    }
                }
                for b in &mut g.balls {
                    if self.metaballs.selected.contains(&(g.id, b.id)) {
                        match field {
                            "X" => b.x += offset,
                            "Y" => b.y += offset,
                            "Radius" => b.radius = value,
                            "Strength" => b.stiffness = value,
                            "Size" | "Blend reach" => {
                                let (size, reach) = visible_size(b, g.threshold)
                                    .ok_or("This selection requires raw field controls")?;
                                set_size_and_reach(
                                    b,
                                    g.threshold,
                                    if field == "Size" { value } else { size },
                                    if field == "Blend reach" { value } else { reach },
                                )?;
                            }
                            "Threshold" => g.threshold = value,
                            _ => {}
                        }
                    }
                }
            }
            source.validate()?;
            let changed = self.store_metaballs(source, drag)?;
            if changed {
                self.refresh_metaball_preview();
            }
            Ok(changed)
        })();
        self.metaball_result(result)
    }

    pub(crate) fn collapse_metaballs(&mut self, selected: bool) -> bool {
        let result = (|| {
            let ids: Vec<_> = self
                .metaballs
                .selected
                .iter()
                .map(|id| id.0)
                .chain(self.metaballs.selected_link.map(|id| id.0))
                .collect();
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
    pub(crate) fn slide_metaball_value(&mut self, field: &str, value: f64, drag: bool) {
        let changed = Arc::make_mut(&mut self.session).set_metaball_value(field, value, drag);
        if changed && !drag {
            self.refresh_open_glyph();
        }
    }

    pub(crate) fn finish_metaball_slider(&mut self, cancelled: bool) {
        let session = Arc::make_mut(&mut self.session);
        session.metaballs.size_controls_during_drag = None;
        session.metaballs.slider_range = None;
        if cancelled {
            session.cancel_metaball_drag();
        } else {
            session.end_metaball_drag();
            if session.pending_canonical.is_some() {
                self.refresh_open_glyph();
            }
        }
    }

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
    fn metaball_links_follow_centers_preview_save_convert_and_undo() {
        let path =
            std::env::temp_dir().join(format!("xilem-metaball-links-{}.ufo", std::process::id()));
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("i");
        glyph.width = 1000.0;
        glyph.codepoints.insert('i');
        font.default_layer_mut().insert_glyph(glyph);
        font.save(&path).unwrap();
        let mut app = Workspace::open(&path).unwrap();
        app.open_glyph(0);
        app.select_tool(Tool::Metaball);
        for x in [200.0, 800.0] {
            app.edit_metaballs(|s| {
                let changed = s.metaball_click(kurbo::Point::new(x, 300.0), 10.0, false);
                s.end_metaball_drag();
                changed
            });
        }
        Arc::make_mut(&mut app.session).select_all_metaballs();
        let v1 = app.session.metaball_data().unwrap();
        assert_eq!(v1.version, 1);
        assert!(app.session.metaball_connect_pair().is_some());
        let history = app.metadata_undo.len();
        app.edit_metaballs(|s| s.connect_metaballs());
        let connected = app.session.metaball_data().unwrap();
        assert_eq!(connected.version, 2);
        assert_eq!(connected.groups[0].links.len(), 1);
        assert_eq!(app.metadata_undo.len(), history + 1);
        assert!(!app.session.metaball_preview.elements().is_empty());
        app.undo_open_glyph(false);
        assert_eq!(app.session.metaball_data().unwrap(), v1);
        app.undo_open_glyph(true);
        assert_eq!(app.session.metaball_data().unwrap(), connected);

        // Midpoint selection edits the connection; slider cancellation is non-destructive.
        let session = Arc::make_mut(&mut app.session);
        assert!(!session.metaball_click(kurbo::Point::new(500.0, 300.0), 10.0, false));
        assert_eq!(session.metaballs.selected_link, Some((1, 1)));
        let before = session.outline_arc();
        if let Ok(dir) = std::env::var("RUNEBENDER_METABALL_PROOF_DIR") {
            std::fs::create_dir_all(&dir).unwrap();
            app.palette = Arc::new(crate::application::view::theme::Palette::load("gray"));
            let background = app.palette.panel;
            app = crate::application::platform::screenshot::render_to(
                app,
                background,
                |app| xilem::view::sized_box(crate::application::view::render::app_logic(app)),
                (1100, 720),
                1.0,
                &format!("{dir}/metaball-connection-gray.png"),
            );
        }
        for width in [36.0, 40.0, 48.0] {
            app.slide_metaball_value("Width", width, true);
            assert_ne!(app.session.outline_arc(), before);
            assert_eq!(app.metadata_undo.len(), history + 1);
        }
        app.finish_metaball_slider(true);
        assert_eq!(app.session.metaball_data().unwrap(), connected);
        app.slide_metaball_value("Width", 40.0, true);
        app.finish_metaball_slider(false);
        assert_eq!(app.metadata_undo.len(), history + 2);
        app.undo_open_glyph(false);
        assert_eq!(app.session.metaball_data().unwrap(), connected);
        app.undo_open_glyph(true);

        // Moving one center keeps the endpoint reference and moves the actual preview.
        Arc::make_mut(&mut app.session).metaball_click(
            kurbo::Point::new(800.0, 300.0),
            10.0,
            false,
        );
        let before_move = app.session.metaball_data().unwrap();
        app.edit_metaballs(|s| s.move_metaballs(kurbo::Vec2::new(0.0, 100.0), false));
        let moved = app.session.metaball_data().unwrap();
        assert_eq!(moved.groups[0].links, before_move.groups[0].links);
        assert_eq!(moved.groups[0].balls[1].y, 400.0);
        assert_eq!(moved.groups[0].balls[0].y, 300.0);
        assert!(
            runebender::outline::metaballs::field(
                &moved.groups[0],
                kurbo::Point::new(500.0, 350.0)
            ) > moved.groups[0].threshold
        );
        app.font.project.save().unwrap();
        let mut reopened = Workspace::open(&path).unwrap();
        reopened.open_glyph(0);
        assert_eq!(reopened.session.metaball_data().unwrap(), moved);
        assert_eq!(
            reopened.session.metaball_preview,
            app.session.metaball_preview
        );

        // Deleting an endpoint removes dangling links; undo restores exact source metadata.
        app.edit_metaballs(|s| s.delete_metaballs());
        assert!(
            app.session.metaball_data().unwrap().groups[0]
                .links
                .is_empty()
        );
        app.undo_open_glyph(false);
        assert_eq!(app.session.metaball_data().unwrap(), moved);
        // Deleting just the connection retains both centers.
        Arc::make_mut(&mut app.session).metaball_click(
            kurbo::Point::new(500.0, 350.0),
            10.0,
            false,
        );
        app.edit_metaballs(|s| s.delete_metaballs());
        let unlinked = app.session.metaball_data().unwrap();
        assert_eq!(unlinked.groups[0].balls.len(), 2);
        assert!(unlinked.groups[0].links.is_empty());
        app.undo_open_glyph(false);
        assert_eq!(app.session.metaball_data().unwrap(), moved);
        // Selecting a link converts the complete blended group through img2bez.
        Arc::make_mut(&mut app.session).metaball_click(
            kurbo::Point::new(500.0, 350.0),
            10.0,
            false,
        );
        app.edit_metaballs(|s| s.collapse_metaballs(true));
        assert!(
            app.session.metaballs.error.is_none(),
            "{:?}",
            app.session.metaballs.error
        );
        assert!(app.session.metaball_data().unwrap().groups.is_empty());
        assert!(!projected_glyph(&app.session).contours.is_empty());
        app.undo_open_glyph(false);
        assert_eq!(app.session.metaball_data().unwrap(), moved);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn metaball_sliders_preview_group_changes_and_commit_once() {
        use crate::application::editor::tools::text::{TextInputs, TextState};
        let path =
            std::env::temp_dir().join(format!("xilem-metaball-sliders-{}.ufo", std::process::id()));
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("i");
        glyph.width = 500.0;
        glyph.codepoints.insert('i');
        font.default_layer_mut().insert_glyph(glyph);
        font.save(&path).unwrap();
        let mut app = Workspace::open(&path).unwrap();
        app.open_glyph(0);
        app.select_tool(Tool::Metaball);
        for x in [200.0, 400.0] {
            app.edit_metaballs(|s| {
                let changed = s.metaball_click(kurbo::Point::new(x, 300.0), 10.0, false);
                s.end_metaball_drag();
                changed
            });
        }
        Arc::make_mut(&mut app.session).select_all_metaballs();
        let original = app.session.metaball_data().unwrap();
        let before = app.session.outline_arc();
        let revision = app.font.project.document_revision();
        let history = app.metadata_undo.len();
        for value in [310.0, 325.0, 350.0] {
            app.slide_metaball_value("X", value, true);
            let data = app.session.metaball_data().unwrap();
            assert_eq!(data.groups[0].balls[1].x - data.groups[0].balls[0].x, 200.0);
            assert_eq!(app.session.metaball_slider_value("X"), value);
            assert_ne!(app.session.outline_arc(), before);
            assert_eq!(app.font.project.document_revision(), revision);
            assert_eq!(app.metadata_undo.len(), history);
        }
        let live = app.session.outline_arc();
        let inputs = TextInputs::new(&app.font)
            .with_text("ii")
            .with_live_outline("i", live.clone());
        let placed = TextState::new(&inputs).placed();
        assert_eq!(placed.len(), 2);
        for sort in placed {
            assert_eq!(
                sort.path,
                kurbo::Affine::translate(sort.origin.to_vec2()) * (*live).clone()
            );
        }
        app.finish_metaball_slider(false);
        assert_eq!(app.font.project.document_revision(), revision + 1);
        assert_eq!(app.metadata_undo.len(), history + 1);
        app.undo_open_glyph(false);
        assert_eq!(app.session.metaball_data().unwrap(), original);
        Arc::make_mut(&mut app.session).select_all_metaballs();
        let size = app.session.metaball_slider_value("Size");
        app.slide_metaball_value("Blend reach", 250.0, true);
        assert!((app.session.metaball_slider_value("Size") - size).abs() < 1e-9);
        assert_eq!(app.session.metaball_data().unwrap().version, 1);
        app.finish_metaball_slider(true);
        assert_eq!(app.session.metaball_data().unwrap(), original);
        app.slide_metaball_value("Blend reach", 0.0, false);
        assert_eq!(app.session.metaball_data().unwrap(), original);
        assert!(app.session.metaballs.error.is_some());
        app.slide_metaball_value("Radius", 250.0, true);
        assert_ne!(app.session.outline_arc(), before);
        app.finish_metaball_slider(true);
        assert_eq!(app.session.metaball_data().unwrap(), original);
        assert_eq!(app.session.outline_arc(), before);
        assert_eq!(app.metadata_undo.len(), history);
        app.slide_metaball_value("Strength", 2.5, false);
        assert_eq!(
            app.metadata_undo.len(),
            history + 1,
            "keyboard changes commit directly"
        );

        // Optional deterministic visual evidence using the same selected source and controls.
        if let Ok(dir) = std::env::var("RUNEBENDER_METABALL_PROOF_DIR") {
            std::fs::create_dir_all(&dir).unwrap();
            {
                let theme = "gray";
                app.palette = Arc::new(crate::application::view::theme::Palette::load(theme));
                let background = app.palette.panel;
                let _app = crate::application::platform::screenshot::render_to(
                    app,
                    background,
                    |app| {
                        xilem::view::sized_box(crate::application::view::panels::metaballs::panel(
                            app,
                        ))
                    },
                    (246, 620),
                    1.0,
                    &format!("{dir}/metaball-sliders-{theme}.png"),
                );
            }
        }
        std::fs::remove_dir_all(path).unwrap();
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
            app.edit_metaballs(|s| s.set_metaball_value("Radius", 200.0, false));
            assert!(app.session.outline_is_empty());
            assert_eq!(
                app.session.metaball_data().unwrap().groups[0].balls.len(),
                2
            );
            let before = projected_glyph(&app.session);
            app.edit_metaballs(|s| s.set_metaball_value("Radius", f64::NAN, false));
            assert_eq!(
                projected_glyph(&app.session),
                before,
                "invalid input preserves live source"
            );
            app.edit_metaballs(|s| s.collapse_metaballs(false));
            assert!(!app.session.outline_is_empty());
            for contour in &projected_glyph(&app.session).contours {
                let start = contour
                    .points
                    .first()
                    .expect("converted contour has points");
                assert_ne!(start.typ, norad::PointType::OffCurve);
                assert!(
                    contour
                        .points
                        .iter()
                        .filter(|p| p.typ != norad::PointType::OffCurve)
                        .all(|p| start.y <= p.y + 1e-9),
                    "editor contour starts at the bottom"
                );
            }
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

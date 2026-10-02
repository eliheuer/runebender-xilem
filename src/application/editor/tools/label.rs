// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The label tool: say which ink of a neural item belongs to each letter.
//!
//! The item's text gives the letters. One letter is active at a time, and a new region goes to
//! it. A region is a polygon (placed corner by corner, or drawn as a freehand loop) or one whole
//! contour, such as a dot. Regions of joined letters may overlap; the letters share that ink.
//!
//! `runebender::font::model::neural_item` owns the data. This module owns the gestures.

use crate::application::editor::session::Session;
use crate::application::workspace::Workspace;
use kurbo::{Point, Shape as _};
use runebender::font::model::neural_item::{NeuralItem, NeuralRegion};
use std::sync::Arc;

/// What the label tool is doing in one editor session.
#[derive(Clone, Default)]
pub(crate) struct LabelState {
    /// Position of the active letter in the item's list of letters.
    pub active: usize,
    /// Corners of the polygon being placed, in font units.
    pub draft: Vec<Point>,
    /// Why the last gesture did nothing, shown in the panel.
    pub error: Option<String>,
}

/// A freehand loop keeps a corner only when it is this far from the last one, in font units.
const LASSO_SPACING: f64 = 6.0;

impl Session {
    /// The neural item of the open layer; empty when it has none or cannot be read.
    pub(crate) fn neural_item(&self) -> NeuralItem {
        self.neural_item_data().unwrap_or_default()
    }

    /// The text index and character of the active letter.
    pub(crate) fn active_letter(&self) -> Option<(u32, char)> {
        let letters = self.neural_item().letters();
        letters
            .get(self.label.active.min(letters.len().saturating_sub(1)))
            .copied()
    }

    /// Make the letter `step` places away active, stopping at either end.
    pub(crate) fn step_label_letter(&mut self, step: isize) {
        let count = self.neural_item().letters().len();
        if count == 0 {
            return;
        }
        let at = self.label.active.min(count - 1) as isize + step;
        self.label.active = at.clamp(0, count as isize - 1) as usize;
        self.label.draft.clear();
    }

    /// Replace the item's text. Regions of letters past the end of a shorter text are dropped.
    pub(crate) fn set_label_text(&mut self, text: String) -> bool {
        let mut item = self.neural_item();
        if item.text == text {
            return false;
        }
        item.text = text;
        item.retain_valid_owners();
        let count = item.letters().len();
        self.label.active = self.label.active.min(count.saturating_sub(1));
        self.store_label(item)
    }

    /// Add one corner to the polygon being placed.
    pub(crate) fn add_label_corner(&mut self, at: Point) {
        self.label.error = None;
        self.label.draft.push(at);
    }

    /// Give the placed polygon to the active letter. Needs three corners.
    pub(crate) fn close_label_polygon(&mut self) -> bool {
        let corners = std::mem::take(&mut self.label.draft);
        self.add_label_polygon(corners)
    }

    /// Give a freehand loop to the active letter, thinned to a workable number of corners.
    pub(crate) fn add_label_lasso(&mut self, points: &[Point]) -> bool {
        let mut corners: Vec<Point> = Vec::new();
        for point in points {
            if corners
                .last()
                .is_none_or(|last| last.distance(*point) >= LASSO_SPACING)
            {
                corners.push(*point);
            }
        }
        self.add_label_polygon(corners)
    }

    fn add_label_polygon(&mut self, corners: Vec<Point>) -> bool {
        if corners.len() < 3 {
            self.label.error = Some("A region needs three corners".into());
            return false;
        }
        self.add_label_region(NeuralRegion {
            owners: Vec::new(),
            polygon: corners.iter().map(|p| [p.x.round(), p.y.round()]).collect(),
            contour_at: None,
        })
    }

    /// Give the whole contour under `at` to the active letter.
    pub(crate) fn assign_label_contour(&mut self, at: Point) -> bool {
        if !self
            .label_contours()
            .iter()
            .any(|contour| contour.contains(at))
        {
            self.label.error = Some("No contour there".into());
            return false;
        }
        self.add_label_region(NeuralRegion {
            owners: Vec::new(),
            polygon: Vec::new(),
            contour_at: Some([at.x.round(), at.y.round()]),
        })
    }

    fn add_label_region(&mut self, mut region: NeuralRegion) -> bool {
        let Some((index, _)) = self.active_letter() else {
            self.label.error = Some("Type the text of this item first".into());
            return false;
        };
        region.owners = vec![index];
        let mut item = self.neural_item();
        item.regions.push(region);
        self.store_label(item)
    }

    /// Remove the newest region of the active letter, or the last placed corner.
    pub(crate) fn delete_label_region(&mut self) -> bool {
        if self.label.draft.pop().is_some() {
            return false;
        }
        let Some((index, _)) = self.active_letter() else {
            return false;
        };
        let mut item = self.neural_item();
        let Some(position) = item.regions_of(index).last() else {
            return false;
        };
        item.regions.remove(position);
        self.store_label(item)
    }

    fn store_label(&mut self, item: NeuralItem) -> bool {
        match self.store_neural_item(item) {
            Ok(changed) => {
                self.label.error = None;
                changed
            }
            Err(error) => {
                self.label.error = Some(error);
                false
            }
        }
    }

    /// Each contour of the open layer as its own path, for contour regions.
    pub(crate) fn label_contours(&self) -> Vec<kurbo::BezPath> {
        self.current_layer()
            .into_iter()
            .flat_map(|layer| layer.contours())
            .map(|contour| {
                runebender::outline::path::Path::from_document_contour(contour).to_bezpath()
            })
            .collect()
    }

    /// The area each letter owns, in font units: its polygons and its whole contours.
    ///
    /// Entries follow the item's letters. Painting clips the outline to each area.
    pub(crate) fn label_areas(&self) -> Vec<kurbo::BezPath> {
        let item = self.neural_item();
        let contours = self.label_contours();
        item.letters()
            .into_iter()
            .map(|(index, _)| {
                let mut area = kurbo::BezPath::new();
                for position in item.regions_of(index) {
                    let region = &item.regions[position];
                    if let Some(at) = region.contour_at {
                        let at = Point::new(at[0], at[1]);
                        // A little larger than the contour, so the clip keeps its whole edge.
                        for contour in contours.iter().filter(|contour| contour.contains(at)) {
                            area.extend(contour.bounding_box().inflate(2.0, 2.0).to_path(0.1));
                        }
                    } else {
                        area.extend(region.polygon_path());
                    }
                }
                area
            })
            .collect()
    }
}

impl Workspace {
    /// Run one label edit on the open session and commit it to the document.
    pub(crate) fn edit_label(&mut self, edit: impl FnOnce(&mut Session) -> bool) {
        let changed = edit(Arc::make_mut(&mut self.session));
        if changed {
            self.refresh_open_glyph();
        }
    }

    /// The text field of the label panel: what is being typed, or the item's text.
    pub(crate) fn label_text(&self) -> String {
        match &self.label_buf {
            Some((glyph, text)) if *glyph == self.session.glyph_name => text.clone(),
            _ => self.session.neural_item().text,
        }
    }

    /// Write a phrase file for every labeled item of the active master.
    ///
    /// The files go in a `phrases` directory beside the document, one per glyph. Items that are
    /// not ready (a letter without a region, a contour region that points at nothing) are named
    /// in the note and skipped.
    pub(crate) fn command_export_phrases(&mut self) {
        let project = &self.font.project;
        let Some(source) = project
            .document_sources()
            .find(|source| Some(source.id()) == project.source_id(self.font.active()))
        else {
            return;
        };
        let layer_id = source.default_layer();
        let upm = self.session.metrics.upm;
        let directory = self
            .font
            .document_source()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("phrases");
        let mut written = 0;
        let mut skipped: Vec<String> = Vec::new();
        for glyph in &self.font.glyphs {
            let Some(layer) = project.document_layer(&glyph.name, &layer_id) else {
                continue;
            };
            if layer.neural_item().is_ok_and(|item| item.is_empty()) {
                continue;
            }
            let result =
                runebender::formats::neural_phrase::layer_phrase(layer, upm).and_then(|phrase| {
                    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
                    let text = serde_json::to_string(&phrase).map_err(|error| error.to_string())?;
                    std::fs::write(directory.join(format!("{}.json", glyph.name)), text)
                        .map_err(|error| error.to_string())
                });
            match result {
                Ok(()) => written += 1,
                Err(error) => skipped.push(format!("{}: {error}", glyph.name)),
            }
        }
        self.note = if skipped.is_empty() {
            format!("Wrote {written} phrase file(s) to {}", directory.display())
        } else {
            format!(
                "Wrote {written} phrase file(s); skipped {}",
                skipped.join("; ")
            )
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::workspace::Tool;

    fn rect_contour(x0: f64, y0: f64, x1: f64, y1: f64) -> norad::Contour {
        let point =
            |x, y| norad::ContourPoint::new(x, y, norad::PointType::Line, false, None, None);
        norad::Contour::new(
            vec![point(x0, y0), point(x1, y0), point(x1, y1), point(x0, y1)],
            None,
        )
    }

    /// One item: a wide bar that two letters share, and a dot above it.
    fn item_font(tag: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("xilem-label-{tag}-{}.ufo", std::process::id()));
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("item");
        glyph.width = 700.0;
        glyph.contours.push(rect_contour(100.0, 0.0, 600.0, 100.0));
        glyph
            .contours
            .push(rect_contour(450.0, 200.0, 500.0, 250.0));
        font.default_layer_mut().insert_glyph(glyph);
        font.save(&path).unwrap();
        path
    }

    fn square(x0: f64, x1: f64) -> Vec<Point> {
        [(x0, -50.0), (x1, -50.0), (x1, 150.0), (x0, 150.0)]
            .into_iter()
            .map(Point::from)
            .collect()
    }

    #[test]
    fn regions_go_to_the_active_letter_and_undo_one_at_a_time() {
        let path = item_font("regions");
        let mut app = Workspace::open(&path).unwrap();
        app.open_glyph(app.font.index_of("item").unwrap());
        app.select_tool(Tool::Label);

        // no text yet: a region has no letter to go to
        app.edit_label(|s| s.add_label_lasso(&square(50.0, 400.0)));
        assert!(app.session.neural_item().regions.is_empty());
        assert!(app.session.label.error.is_some());

        app.edit_label(|s| s.set_label_text("ب س".into()));
        assert_eq!(app.session.active_letter(), Some((0, 'ب')));

        // a freehand loop for the first letter, a placed polygon for the second
        app.edit_label(|s| s.add_label_lasso(&square(50.0, 400.0)));
        Arc::make_mut(&mut app.session).step_label_letter(1);
        assert_eq!(
            app.session.active_letter(),
            Some((2, 'س')),
            "the space is not a letter"
        );
        for corner in square(300.0, 650.0) {
            Arc::make_mut(&mut app.session).add_label_corner(corner);
        }
        assert!(
            app.session.neural_item().regions.len() == 1,
            "corners are not a region yet"
        );
        app.edit_label(|s| s.close_label_polygon());
        // the dot is a whole contour
        app.edit_label(|s| s.assign_label_contour(Point::new(475.0, 225.0)));
        app.edit_label(|s| s.assign_label_contour(Point::new(0.0, 900.0)));

        let item = app.session.neural_item();
        assert_eq!(item.regions.len(), 3);
        assert_eq!(item.regions[0].owners, vec![0]);
        assert_eq!(item.regions[1].owners, vec![2]);
        assert_eq!(item.regions[2].contour_at, Some([475.0, 225.0]));
        assert!(item.unlabeled().is_empty());
        assert_eq!(app.session.label.error.as_deref(), Some("No contour there"));

        // both letters own the middle of the bar; only the second owns the dot
        let areas = app.session.label_areas();
        let middle = Point::new(350.0, 50.0);
        assert!(areas[0].contains(middle) && areas[1].contains(middle));
        assert!(!areas[0].contains(Point::new(475.0, 225.0)));
        assert!(areas[1].contains(Point::new(475.0, 225.0)));

        // Delete removes the active letter's newest region; undo brings it back
        app.edit_label(|s| s.delete_label_region());
        assert_eq!(app.session.neural_item().regions.len(), 2);
        app.undo_open_glyph(false);
        assert_eq!(app.session.neural_item().regions.len(), 3);

        // a shorter text drops the regions of the letters it lost
        app.edit_label(|s| s.set_label_text("ب".into()));
        assert_eq!(app.session.neural_item().regions.len(), 1);
        app.undo_open_glyph(false);
        assert_eq!(app.session.neural_item().regions.len(), 3);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn labels_survive_a_save_and_export_as_phrase_files() {
        let path = item_font("export");
        let mut app = Workspace::open(&path).unwrap();
        app.open_glyph(app.font.index_of("item").unwrap());
        app.select_tool(Tool::Label);
        app.edit_label(|s| s.set_label_text("بس".into()));
        app.edit_label(|s| s.add_label_lasso(&square(50.0, 400.0)));

        // one letter still has no ink: the export names it and writes nothing
        app.command_export_phrases();
        assert!(
            app.note.contains("skipped") && app.note.contains('س'),
            "{}",
            app.note
        );

        Arc::make_mut(&mut app.session).step_label_letter(1);
        app.edit_label(|s| s.add_label_lasso(&square(300.0, 650.0)));
        app.font.project.save().unwrap();

        let reopened = Workspace::open(&path).unwrap();
        let glyph = norad::Font::load(&path).unwrap();
        let glyph = glyph.default_layer().get_glyph("item").unwrap();
        assert!(
            glyph
                .lib
                .contains_key(runebender::font::model::neural_item::NEURAL_ITEM_KEY)
        );
        let mut reopened = reopened;
        reopened.open_glyph(reopened.font.index_of("item").unwrap());
        assert_eq!(reopened.session.neural_item(), app.session.neural_item());

        reopened.command_export_phrases();
        let file = path.parent().unwrap().join("phrases").join("item.json");
        let phrase: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(phrase["text"], "بس");
        assert_eq!(phrase["clusters"].as_array().unwrap().len(), 2);
        std::fs::remove_file(&file).ok();
        std::fs::remove_dir_all(&path).ok();
    }
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The label tool: mark the samples on a neural canvas and say which ink is which letter.
//!
//! A canvas holds samples, each one connected piece of writing. With no sample selected, a
//! loop drawn around some ink makes a new sample. With a sample selected, its text gives the
//! letters; one letter is active at a time, and a new region goes to it. A region is a polygon
//! (placed corner by corner, or drawn as a freehand loop) or one whole contour, such as a dot.
//! Regions of joined letters may overlap; the letters share that ink. Escape leaves a sample.
//!
//! `runebender::font::model::neural_item` owns the data. This module owns the gestures.

use crate::application::editor::session::Session;
use crate::application::workspace::Workspace;
use kurbo::{Point, Shape as _};
use runebender::font::model::neural_item::{NeuralItem, NeuralRegion, NeuralSample};
use std::sync::Arc;

/// What the label tool is doing in one editor session.
#[derive(Clone, Default)]
pub(crate) struct LabelState {
    /// The selected sample, by its position on the canvas.
    pub sample: Option<usize>,
    /// Position of the active letter in the selected sample's list of letters.
    pub active: usize,
    /// Corners of the polygon being placed, in font units.
    pub draft: Vec<Point>,
    /// Why the last gesture did nothing, shown in the panel.
    pub error: Option<String>,
}

/// A freehand loop keeps a corner only when it is this far from the last one, in font units.
const LASSO_SPACING: f64 = 6.0;

/// One labeled area, for painting.
pub(crate) struct LabelArea {
    /// The sample's position on the canvas.
    pub sample: usize,
    /// The letter's position in its sample's letters.
    pub letter: usize,
    /// The letter's polygons and whole contours, in font units.
    pub area: kurbo::BezPath,
}

impl Session {
    /// The neural item of the open layer; empty when it has none or cannot be read.
    pub(crate) fn neural_item(&self) -> NeuralItem {
        self.neural_item_data().unwrap_or_default()
    }

    /// The selected sample and its position, while it exists.
    pub(crate) fn selected_sample(&self) -> Option<(usize, NeuralSample)> {
        let position = self.label.sample?;
        Some((position, self.neural_item().samples.get(position)?.clone()))
    }

    /// Select a sample, or none, and start at its first letter.
    pub(crate) fn select_sample(&mut self, position: Option<usize>) {
        self.label.sample = position;
        self.label.active = 0;
        self.label.draft.clear();
        self.label.error = None;
    }

    /// The text index and character of the active letter of the selected sample.
    pub(crate) fn active_letter(&self) -> Option<(u32, char)> {
        let letters = self.selected_sample()?.1.letters();
        letters
            .get(self.label.active.min(letters.len().saturating_sub(1)))
            .copied()
    }

    /// Make the letter `step` places away active, stopping at either end.
    pub(crate) fn step_label_letter(&mut self, step: isize) {
        let Some((_, sample)) = self.selected_sample() else {
            return;
        };
        let count = sample.letters().len();
        if count == 0 {
            return;
        }
        let at = self.label.active.min(count - 1);
        self.label.active = if step < 0 {
            at.saturating_sub(step.unsigned_abs())
        } else {
            at.saturating_add(step.unsigned_abs()).min(count - 1)
        };
        self.label.draft.clear();
    }

    /// Replace the selected sample's text. Regions of letters past the end of a shorter text
    /// are dropped.
    pub(crate) fn set_sample_text(&mut self, text: String) -> bool {
        let Some(position) = self.label.sample else {
            return false;
        };
        let mut item = self.neural_item();
        let Some(sample) = item.samples.get_mut(position) else {
            return false;
        };
        if sample.text == text {
            return false;
        }
        sample.text = text;
        sample.retain_valid_owners();
        let count = sample.letters().len();
        self.label.active = self.label.active.min(count.saturating_sub(1));
        self.store_label(item)
    }

    /// Remove the selected sample and its labels. The ink stays.
    pub(crate) fn delete_sample(&mut self) -> bool {
        let Some(position) = self.label.sample else {
            return false;
        };
        let mut item = self.neural_item();
        if position >= item.samples.len() {
            return false;
        }
        item.samples.remove(position);
        self.select_sample(None);
        self.store_label(item)
    }

    /// Add one corner to the polygon being placed.
    pub(crate) fn add_label_corner(&mut self, at: Point) {
        self.label.error = None;
        self.label.draft.push(at);
    }

    /// Close the placed polygon: a region of the active letter, or a new sample.
    pub(crate) fn close_label_polygon(&mut self) -> bool {
        let corners = std::mem::take(&mut self.label.draft);
        self.add_label_polygon(corners)
    }

    /// Use a freehand loop, thinned to a workable number of corners: a region of the active
    /// letter, or with no sample selected, a new sample.
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
            self.label.error = Some("A loop needs three corners".into());
            return false;
        }
        let polygon: Vec<[f64; 2]> = corners.iter().map(|p| [p.x.round(), p.y.round()]).collect();
        if self.label.sample.is_none() {
            return self.add_sample(polygon);
        }
        self.add_label_region(NeuralRegion {
            owners: Vec::new(),
            polygon,
            contour_at: None,
        })
    }

    fn add_sample(&mut self, boundary: Vec<[f64; 2]>) -> bool {
        let mut item = self.neural_item();
        item.samples.push(NeuralSample {
            boundary,
            text: String::new(),
            regions: Vec::new(),
        });
        let position = item.samples.len() - 1;
        let changed = self.store_label(item);
        if changed {
            self.select_sample(Some(position));
        }
        changed
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
        let Some(position) = self.label.sample else {
            self.label.error = Some("Draw a loop around some ink to make a sample first".into());
            return false;
        };
        let Some((index, _)) = self.active_letter() else {
            self.label.error = Some("Type the text of this sample first".into());
            return false;
        };
        region.owners = vec![index];
        let mut item = self.neural_item();
        let Some(sample) = item.samples.get_mut(position) else {
            return false;
        };
        sample.regions.push(region);
        self.store_label(item)
    }

    /// Remove the last placed corner, or else the newest region of the active letter.
    pub(crate) fn delete_label_region(&mut self) -> bool {
        if self.label.draft.pop().is_some() {
            return false;
        }
        let (Some(position), Some((index, _))) = (self.label.sample, self.active_letter()) else {
            return false;
        };
        let mut item = self.neural_item();
        let Some(sample) = item.samples.get_mut(position) else {
            return false;
        };
        let Some(region) = sample.regions_of(index).last() else {
            return false;
        };
        sample.regions.remove(region);
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

    /// The area every letter of every sample owns, in font units: its polygons and its whole
    /// contours. Painting clips the outline to each area.
    pub(crate) fn label_areas(&self) -> Vec<LabelArea> {
        let item = self.neural_item();
        let contours = self.label_contours();
        let mut areas = Vec::new();
        for (sample_position, sample) in item.samples.iter().enumerate() {
            for (letter, (index, _)) in sample.letters().into_iter().enumerate() {
                let mut area = kurbo::BezPath::new();
                for position in sample.regions_of(index) {
                    let region = &sample.regions[position];
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
                if !area.elements().is_empty() {
                    areas.push(LabelArea {
                        sample: sample_position,
                        letter,
                        area,
                    });
                }
            }
        }
        areas
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

    /// The text field of the label panel: what is being typed, or the selected sample's text.
    pub(crate) fn label_text(&self) -> String {
        let sample = self.session.label.sample;
        match &self.label_buf {
            Some((glyph, at, text)) if *glyph == self.session.glyph_name && Some(*at) == sample => {
                text.clone()
            }
            _ => self
                .session
                .selected_sample()
                .map(|(_, sample)| sample.text)
                .unwrap_or_default(),
        }
    }

    /// Write a phrase file for every labeled sample of the active master.
    ///
    /// The files go in a `phrases` directory beside the document, one per sample. Samples that
    /// are not ready (no text, a letter without a region) are named in the note and skipped.
    pub(crate) fn command_export_phrases(&mut self) {
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            return;
        };
        let directory = self
            .font
            .document_source()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("phrases");
        self.note = match runebender::formats::neural_phrase::export_source(
            &self.font.project,
            source,
            &directory,
        ) {
            Ok(export) if export.skipped.is_empty() => format!(
                "Wrote {} phrase file(s) to {}",
                export.written.len(),
                directory.display()
            ),
            Ok(export) => format!(
                "Wrote {} phrase file(s); skipped {}",
                export.written.len(),
                export
                    .skipped
                    .iter()
                    .map(|(name, reason)| format!("{name}: {reason}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            Err(error) => format!("Phrase export failed: {error}"),
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

    fn loop_around(x0: f64, x1: f64, y0: f64, y1: f64) -> Vec<Point> {
        [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
            .into_iter()
            .map(Point::from)
            .collect()
    }

    #[test]
    fn a_loop_makes_a_sample_and_then_labels_its_letters() {
        let path = item_font("regions");
        let mut app = Workspace::open(&path).unwrap();
        app.open_glyph(app.font.index_of("item").unwrap());
        app.select_tool(Tool::Label);

        // no sample selected: the loop around the ink is a new sample
        app.edit_label(|s| s.add_label_lasso(&loop_around(50.0, 650.0, -50.0, 300.0)));
        assert_eq!(app.session.neural_item().samples.len(), 1);
        assert_eq!(app.session.label.sample, Some(0));

        // no text yet: a loop has no letter to go to
        app.edit_label(|s| s.add_label_lasso(&square(50.0, 400.0)));
        assert!(app.session.neural_item().samples[0].regions.is_empty());
        assert!(app.session.label.error.is_some());

        app.edit_label(|s| s.set_sample_text("ب س".into()));
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
        assert_eq!(
            app.session.neural_item().samples[0].regions.len(),
            1,
            "corners are not a region yet"
        );
        app.edit_label(|s| s.close_label_polygon());
        // the dot is a whole contour
        app.edit_label(|s| s.assign_label_contour(Point::new(475.0, 225.0)));
        app.edit_label(|s| s.assign_label_contour(Point::new(0.0, 900.0)));
        assert_eq!(app.session.label.error.as_deref(), Some("No contour there"));

        let sample = app.session.neural_item().samples[0].clone();
        assert_eq!(sample.regions.len(), 3);
        assert_eq!(sample.regions[0].owners, vec![0]);
        assert_eq!(sample.regions[1].owners, vec![2]);
        assert_eq!(sample.regions[2].contour_at, Some([475.0, 225.0]));
        assert!(sample.unlabeled().is_empty());

        // both letters own the middle of the bar; only the second owns the dot
        let areas = app.session.label_areas();
        let middle = Point::new(350.0, 50.0);
        assert!(areas[0].area.contains(middle) && areas[1].area.contains(middle));
        assert!(!areas[0].area.contains(Point::new(475.0, 225.0)));
        assert!(areas[1].area.contains(Point::new(475.0, 225.0)));

        // Delete removes the active letter's newest region; undo brings it back
        app.edit_label(|s| s.delete_label_region());
        assert_eq!(app.session.neural_item().samples[0].regions.len(), 2);
        app.undo_open_glyph(false);
        assert_eq!(app.session.neural_item().samples[0].regions.len(), 3);

        // Escape leaves the sample; the next loop is a second sample
        Arc::make_mut(&mut app.session).select_sample(None);
        app.edit_label(|s| s.add_label_lasso(&loop_around(700.0, 900.0, 0.0, 100.0)));
        assert_eq!(app.session.neural_item().samples.len(), 2);
        assert_eq!(app.session.label.sample, Some(1));
        app.edit_label(|s| s.delete_sample());
        assert_eq!(app.session.neural_item().samples.len(), 1);
        assert_eq!(app.session.label.sample, None);
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn labels_survive_a_save_and_export_one_phrase_file_per_sample() {
        let path = item_font("export");
        let mut app = Workspace::open(&path).unwrap();
        app.open_glyph(app.font.index_of("item").unwrap());
        app.select_tool(Tool::Label);
        app.edit_label(|s| s.add_label_lasso(&loop_around(50.0, 650.0, -50.0, 300.0)));
        app.edit_label(|s| s.set_sample_text("بس".into()));
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

        let glyph = norad::Font::load(&path).unwrap();
        let glyph = glyph.default_layer().get_glyph("item").unwrap();
        assert!(
            glyph
                .lib
                .contains_key(runebender::font::model::neural_item::NEURAL_ITEM_KEY)
        );
        let mut reopened = Workspace::open(&path).unwrap();
        reopened.open_glyph(reopened.font.index_of("item").unwrap());
        assert_eq!(reopened.session.neural_item(), app.session.neural_item());

        reopened.command_export_phrases();
        let file = path.parent().unwrap().join("phrases").join("item-1.json");
        let phrase: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(phrase["text"], "بس");
        assert_eq!(phrase["clusters"].as_array().unwrap().len(), 2);
        std::fs::remove_file(&file).ok();
        std::fs::remove_dir_all(&path).ok();
    }
}

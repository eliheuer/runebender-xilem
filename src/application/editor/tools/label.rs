// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The label tool: mark the samples on a neural canvas and say which ink is which letter.
//!
//! A canvas holds samples, each one connected piece of writing. With no sample selected, a
//! loop drawn around some ink makes a new sample. With a sample selected, its text gives the
//! letters; one letter is active at a time. Labeling is painting: a click gives the piece of
//! ink under the pointer to the active letter, a drag across a stroke cuts it into two
//! pieces, and a loop lassos a polygon for the odd cases. Pieces are the sample's contours
//! cut by its cuts; a seed names one. Corners, cuts and regions can be selected, moved and
//! deleted, so a mistake is a small fix. Escape leaves a sample.
//!
//! `runebender::font::model::neural_item` owns the data. This module owns the gestures.

use crate::application::editor::session::Session;
use crate::application::workspace::Workspace;
use kurbo::{BezPath, Line, Point, Shape as _};
use runebender::font::model::neural_item::{NeuralItem, NeuralRegion, NeuralSample};
use runebender::outline::label_pieces::{
    OVERLAP, is_cut_gesture, nearest_on_segment, piece_at, pieces, seed_polygon, simplify_loop,
};
use runebender::outline::path::Path;
use std::sync::Arc;

/// Something of the selected sample the designer picked, to move or delete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LabelSelection {
    /// A corner of a lasso polygon: the region and the corner.
    Corner(usize, usize),
    /// A cut, by its position in the sample's cuts.
    Cut(usize),
    /// A whole region.
    Region(usize),
}

/// Something being dragged, shown at once and stored on release.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LabelMoving {
    /// A corner of a lasso polygon, now at this point.
    Corner {
        region: usize,
        corner: usize,
        to: Point,
    },
    /// One end of a cut, now at this point.
    CutEnd { cut: usize, end: usize, to: Point },
    /// A whole cut, moved by this much.
    Cut { cut: usize, by: kurbo::Vec2 },
}

/// What is under the pointer, in order of what a press would take first.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LabelHit {
    Corner {
        region: usize,
        corner: usize,
    },
    CutEnd {
        cut: usize,
        end: usize,
    },
    /// A lasso polygon's edge: a click adds a corner after `after`, at `at`.
    Edge {
        region: usize,
        after: usize,
        at: Point,
    },
    Cut {
        cut: usize,
    },
    /// A piece of ink, with its outline for the glow.
    Piece(BezPath),
    Nothing,
}

/// What the label tool is doing in one editor session.
#[derive(Clone, Default)]
pub(crate) struct LabelState {
    /// The selected sample, by its position on the canvas.
    pub sample: Option<usize>,
    /// Position of the active letter in the selected sample's list of letters.
    pub active: usize,
    /// Why the last gesture did nothing, shown in the panel.
    pub error: Option<String>,
    /// What is being dragged, if anything.
    pub moving: Option<LabelMoving>,
    /// What was last picked, for Delete.
    pub selected: Option<LabelSelection>,
    /// The letter whose chip the pointer is over, lit on the canvas.
    pub hover_letter: Option<usize>,
}

/// A freehand loop keeps a corner only when it is this far from the last one, in font units.
const LASSO_SPACING: f64 = 4.0;
/// A lasso polygon keeps at most this many corners.
const LASSO_MAX_CORNERS: usize = 16;

/// One labeled area, for painting.
pub(crate) struct LabelArea {
    /// The sample's position on the canvas.
    pub sample: usize,
    /// The letter's position in its sample's letters.
    pub letter: usize,
    /// The letter's polygons and whole contours, in font units.
    pub area: BezPath,
}

/// One lasso polygon of the selected sample, for its outline and corner handles.
pub(crate) struct LabelPolygon {
    /// The region's position in the sample.
    pub region: usize,
    /// The owner's position in the sample's letters.
    pub letter: usize,
    /// The corners, in font units.
    pub corners: Vec<Point>,
}

impl Session {
    /// The neural item of the open layer; empty when it has none or cannot be read.
    pub(crate) fn neural_item(&self) -> NeuralItem {
        self.neural_item_data().unwrap_or_default()
    }

    /// The neural item as shown: with whatever is being dragged at the pointer.
    fn shown_neural_item(&self) -> NeuralItem {
        let mut item = self.neural_item();
        if let (Some(position), Some(moving)) = (self.label.sample, self.label.moving)
            && let Some(sample) = item.samples.get_mut(position)
        {
            match moving {
                LabelMoving::Corner { region, corner, to } => {
                    if let Some(at) = sample
                        .regions
                        .get_mut(region)
                        .and_then(|region| region.polygon.get_mut(corner))
                    {
                        *at = [to.x, to.y];
                    }
                }
                LabelMoving::CutEnd { cut, end, to } => {
                    if let Some(cut) = sample.cuts.get_mut(cut) {
                        cut[end.min(1)] = [to.x, to.y];
                    }
                }
                LabelMoving::Cut { cut, by } => {
                    if let Some(cut) = sample.cuts.get_mut(cut) {
                        for end in cut {
                            end[0] += by.x;
                            end[1] += by.y;
                        }
                    }
                }
            }
        }
        item
    }

    /// The sample whose boundary holds `at`, the smallest when they nest.
    pub(crate) fn sample_at(&self, at: Point) -> Option<usize> {
        self.neural_item()
            .samples
            .iter()
            .enumerate()
            .filter(|(_, sample)| sample.boundary_path().contains(at))
            .min_by(|a, b| {
                let area = |sample: &NeuralSample| sample.boundary_path().area().abs();
                area(a.1).total_cmp(&area(b.1))
            })
            .map(|(position, _)| position)
    }

    /// The selected sample and its position, while it exists.
    pub(crate) fn selected_sample(&self) -> Option<(usize, NeuralSample)> {
        let position = self.label.sample?;
        Some((position, self.neural_item().samples.get(position)?.clone()))
    }

    /// Select a sample, or none, and start at its first unlabeled letter.
    pub(crate) fn select_sample(&mut self, position: Option<usize>) {
        self.label.sample = position;
        self.label.active = 0;
        self.label.error = None;
        self.label.moving = None;
        self.label.selected = None;
        self.step_to_unlabeled_letter();
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
        self.label.selected = None;
    }

    /// Make the next letter without ink active, from the active one on; the last letter when
    /// every one after it has ink.
    pub(crate) fn step_to_unlabeled_letter(&mut self) {
        let Some((_, sample)) = self.selected_sample() else {
            return;
        };
        let letters = sample.letters();
        if letters.is_empty() {
            return;
        }
        let from = self.label.active.min(letters.len() - 1);
        let next = letters
            .iter()
            .enumerate()
            .skip(from)
            .find(|(_, (index, _))| sample.regions_of(*index).next().is_none())
            .map(|(position, _)| position);
        self.label.active = next.unwrap_or(from);
        self.label.selected = None;
    }

    /// Enter: the next letter without ink after the active one.
    pub(crate) fn next_label_letter(&mut self) {
        let Some((_, sample)) = self.selected_sample() else {
            return;
        };
        let count = sample.letters().len();
        if self.label.active + 1 < count {
            self.label.active += 1;
            self.step_to_unlabeled_letter();
        }
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

    /// Each contour of the open layer as a path, for cutting.
    fn label_paths(&self) -> Vec<Path> {
        self.current_layer()
            .into_iter()
            .flat_map(|layer| layer.contours())
            .map(Path::from_document_contour)
            .collect()
    }

    /// Each contour of the open layer as its own path.
    pub(crate) fn label_contours(&self) -> Vec<BezPath> {
        self.label_paths().iter().map(Path::to_bezpath).collect()
    }

    /// The selected sample's cuts as lines, as shown.
    pub(crate) fn label_cuts(&self) -> Vec<Line> {
        let Some(position) = self.label.sample else {
            return Vec::new();
        };
        let item = self.shown_neural_item();
        item.samples
            .get(position)
            .map(|sample| sample.cuts.iter().map(|c| cut_line(*c)).collect())
            .unwrap_or_default()
    }

    /// The pieces of the layer's ink under the selected sample's cuts.
    pub(crate) fn label_pieces(&self) -> Vec<BezPath> {
        pieces(&self.label_paths(), &self.label_cuts())
    }

    /// The piece of ink under `at`, if any.
    pub(crate) fn label_piece_at(&self, at: Point) -> Option<BezPath> {
        let pieces = self.label_pieces();
        piece_at(&pieces, at).map(|index| pieces[index].clone())
    }

    /// What is under `at` within `reach` font units, in the order a press takes it.
    pub(crate) fn label_hit(&self, at: Point, reach: f64) -> LabelHit {
        let Some((_, sample)) = self.selected_sample() else {
            return match self.label_piece_at(at) {
                Some(piece) => LabelHit::Piece(piece),
                None => LabelHit::Nothing,
            };
        };
        let active = self.active_letter().map(|(index, _)| index);
        let editable = |position: usize, region: &NeuralRegion| {
            region.seed.is_none()
                && (active.is_some_and(|index| region.owners.contains(&index))
                    || self.label.selected == Some(LabelSelection::Region(position)))
        };
        // The nearest of one kind within reach, each kind tried in turn.
        struct Nearest(Option<(f64, LabelHit)>);
        impl Nearest {
            fn consider(&mut self, reach: f64, distance: f64, hit: LabelHit) {
                if distance <= reach && self.0.as_ref().is_none_or(|(d, _)| distance < *d) {
                    self.0 = Some((distance, hit));
                }
            }
        }
        let mut best = Nearest(None);
        let consider = |best: &mut Nearest, distance: f64, hit: LabelHit| {
            best.consider(reach, distance, hit);
        };
        for (position, region) in sample.regions.iter().enumerate() {
            if !editable(position, region) {
                continue;
            }
            for (corner, p) in region.polygon.iter().enumerate() {
                consider(
                    &mut best,
                    Point::new(p[0], p[1]).distance(at),
                    LabelHit::Corner {
                        region: position,
                        corner,
                    },
                );
            }
        }
        if let Some((_, hit)) = best.0.take() {
            return hit;
        }
        for (cut, line) in sample.cuts.iter().enumerate() {
            for (end, p) in line.iter().enumerate() {
                consider(
                    &mut best,
                    Point::new(p[0], p[1]).distance(at),
                    LabelHit::CutEnd { cut, end },
                );
            }
        }
        if let Some((_, hit)) = best.0.take() {
            return hit;
        }
        for (position, region) in sample.regions.iter().enumerate() {
            if !editable(position, region) || region.polygon.len() < 3 {
                continue;
            }
            let corners = &region.polygon;
            for after in 0..corners.len() {
                let a = corners[after];
                let b = corners[(after + 1) % corners.len()];
                let (on, distance) = nearest_on_segment(at, Line::new((a[0], a[1]), (b[0], b[1])));
                consider(
                    &mut best,
                    distance,
                    LabelHit::Edge {
                        region: position,
                        after,
                        at: on,
                    },
                );
            }
        }
        if let Some((_, hit)) = best.0.take() {
            return hit;
        }
        for (cut, line) in sample.cuts.iter().enumerate() {
            let (_, distance) = nearest_on_segment(at, cut_line(*line));
            consider(&mut best, distance, LabelHit::Cut { cut });
        }
        if let Some((_, hit)) = best.0.take() {
            return hit;
        }
        match self.label_piece_at(at) {
            Some(piece) => LabelHit::Piece(piece),
            None => LabelHit::Nothing,
        }
    }

    /// Give the piece of ink under `at` to the active letter, or with `erase`, take it away.
    ///
    /// A piece another letter owns changes hands. A new seed is stored with its training
    /// polygon.
    pub(crate) fn paint_piece(&mut self, at: Point, erase: bool) -> bool {
        let Some(position) = self.label.sample else {
            self.label.error = Some("Draw a loop around some ink to make a sample first".into());
            return false;
        };
        let Some((index, _)) = self.active_letter() else {
            self.label.error = Some("Type the text of this sample first".into());
            return false;
        };
        let pieces = self.label_pieces();
        let Some(piece) = piece_at(&pieces, at) else {
            self.label.error = Some("No ink there".into());
            return false;
        };
        let mut item = self.neural_item();
        let Some(sample) = item.samples.get_mut(position) else {
            return false;
        };
        let same_piece: Vec<usize> = sample
            .regions
            .iter()
            .enumerate()
            .filter(|(_, region)| {
                region.seed.is_some_and(|seed| {
                    piece_at(&pieces, Point::new(seed[0], seed[1])) == Some(piece)
                })
            })
            .map(|(position, _)| position)
            .collect();
        if erase {
            let before = sample.regions.len();
            let mut position = 0;
            sample.regions.retain(|region| {
                let keep = !(same_piece.contains(&position) && region.owners.contains(&index));
                position += 1;
                keep
            });
            if sample.regions.len() == before {
                return false;
            }
        } else if same_piece
            .iter()
            .any(|p| sample.regions[*p].owners == vec![index])
        {
            return false;
        } else if let Some(p) = same_piece.first().copied() {
            sample.regions[p].owners = vec![index];
        } else {
            sample.regions.push(NeuralRegion {
                owners: vec![index],
                polygon: Vec::new(),
                seed: Some([at.x.round(), at.y.round()]),
            });
        }
        self.derive_and_store(item)
    }

    /// Cut the ink along the line from `a` to `b`.
    pub(crate) fn add_cut(&mut self, a: Point, b: Point) -> bool {
        let Some(position) = self.label.sample else {
            self.label.error = Some("Draw a loop around some ink to make a sample first".into());
            return false;
        };
        let mut item = self.neural_item();
        let Some(sample) = item.samples.get_mut(position) else {
            return false;
        };
        sample
            .cuts
            .push([[a.x.round(), a.y.round()], [b.x.round(), b.y.round()]]);
        self.label.selected = Some(LabelSelection::Cut(sample.cuts.len() - 1));
        self.derive_and_store(item)
    }

    /// Use a freehand loop as a lasso: a polygon with few corners for the active letter, or
    /// with no sample selected, a new sample.
    pub(crate) fn add_label_lasso(&mut self, points: &[Point]) -> bool {
        let mut spaced: Vec<Point> = Vec::new();
        for point in points {
            if spaced
                .last()
                .is_none_or(|last| last.distance(*point) >= LASSO_SPACING)
            {
                spaced.push(*point);
            }
        }
        if spaced.len() < 3 {
            self.label.error = Some("A loop needs three corners".into());
            return false;
        }
        let bounds = spaced
            .iter()
            .fold(kurbo::Rect::from_points(spaced[0], spaced[0]), |r, p| {
                r.union_pt(*p)
            });
        let tolerance = (bounds.width().hypot(bounds.height()) * 0.015).max(2.0);
        let corners = simplify_loop(&spaced, tolerance, LASSO_MAX_CORNERS);
        let polygon: Vec<[f64; 2]> = corners.iter().map(|p| [p.x.round(), p.y.round()]).collect();
        if self.label.sample.is_none() {
            return self.add_sample(polygon);
        }
        let Some((index, _)) = self.active_letter() else {
            self.label.error = Some("Type the text of this sample first".into());
            return false;
        };
        let mut item = self.neural_item();
        let Some(sample) = item.samples.get_mut(self.label.sample.unwrap_or(0)) else {
            return false;
        };
        sample.regions.push(NeuralRegion {
            owners: vec![index],
            polygon,
            seed: None,
        });
        self.label.selected = Some(LabelSelection::Region(sample.regions.len() - 1));
        self.store_label(item)
    }

    fn add_sample(&mut self, boundary: Vec<[f64; 2]>) -> bool {
        let mut item = self.neural_item();
        item.samples.push(NeuralSample {
            boundary,
            text: String::new(),
            cuts: Vec::new(),
            regions: Vec::new(),
        });
        let position = item.samples.len() - 1;
        let changed = self.store_label(item);
        if changed {
            self.select_sample(Some(position));
        }
        changed
    }

    /// Add a corner to a lasso polygon after corner `after`, at `at`. Returns the new
    /// corner's position.
    pub(crate) fn add_label_corner(
        &mut self,
        region: usize,
        after: usize,
        at: Point,
    ) -> Option<usize> {
        let position = self.label.sample?;
        let mut item = self.neural_item();
        let polygon = &mut item
            .samples
            .get_mut(position)?
            .regions
            .get_mut(region)?
            .polygon;
        if after >= polygon.len() {
            return None;
        }
        polygon.insert(after + 1, [at.x.round(), at.y.round()]);
        let corner = after + 1;
        self.store_label(item).then_some(corner)
    }

    /// `at`, or the corner of another lasso polygon of the selected sample within `reach` of
    /// it, so neighbors can share a corner exactly. `skip` is the region the corner belongs to.
    pub(crate) fn snapped_label_corner(&self, at: Point, reach: f64, skip: Option<usize>) -> Point {
        let Some((_, sample)) = self.selected_sample() else {
            return at;
        };
        sample
            .regions
            .iter()
            .enumerate()
            .filter(|(position, region)| Some(*position) != skip && region.seed.is_none())
            .flat_map(|(_, region)| region.polygon.iter())
            .map(|p| Point::new(p[0], p[1]))
            .filter(|corner| corner.distance(at) <= reach)
            .min_by(|a, b| a.distance(at).total_cmp(&b.distance(at)))
            .unwrap_or(at)
    }

    /// Make the owner of a region of the selected sample the active letter.
    pub(crate) fn activate_label_region(&mut self, region: usize) {
        let Some((_, sample)) = self.selected_sample() else {
            return;
        };
        let owner = sample.regions.get(region).and_then(|r| r.owners.first());
        if let Some(letter) = owner.and_then(|owner| {
            sample
                .letters()
                .iter()
                .position(|(index, _)| index == owner)
        }) {
            self.label.active = letter;
        }
    }

    /// Store whatever was dragged where it was dropped.
    pub(crate) fn finish_label_move(&mut self, keep: bool) -> bool {
        let Some(moving) = self.label.moving.take() else {
            return false;
        };
        let Some(position) = self.label.sample.filter(|_| keep) else {
            return false;
        };
        let mut item = self.neural_item();
        let Some(sample) = item.samples.get_mut(position) else {
            return false;
        };
        let round = |p: Point| [p.x.round(), p.y.round()];
        match moving {
            LabelMoving::Corner { region, corner, to } => {
                let Some(at) = sample
                    .regions
                    .get_mut(region)
                    .and_then(|region| region.polygon.get_mut(corner))
                else {
                    return false;
                };
                if *at == round(to) {
                    return false;
                }
                *at = round(to);
                self.store_label(item)
            }
            LabelMoving::CutEnd { cut, end, to } => {
                let Some(line) = sample.cuts.get_mut(cut) else {
                    return false;
                };
                if line[end.min(1)] == round(to) {
                    return false;
                }
                line[end.min(1)] = round(to);
                self.derive_and_store(item)
            }
            LabelMoving::Cut { cut, by } => {
                let Some(line) = sample.cuts.get_mut(cut) else {
                    return false;
                };
                if by.hypot() < 0.5 {
                    return false;
                }
                for end in line.iter_mut() {
                    *end = [(end[0] + by.x).round(), (end[1] + by.y).round()];
                }
                self.derive_and_store(item)
            }
        }
    }

    /// Delete what is selected: a corner (a polygon keeps three), a cut (its pieces merge), or
    /// a region. With nothing selected, the active letter's newest region.
    pub(crate) fn delete_label_selection(&mut self) -> bool {
        let Some(position) = self.label.sample else {
            return false;
        };
        let mut item = self.neural_item();
        let Some(sample) = item.samples.get_mut(position) else {
            return false;
        };
        let selected = self.label.selected.take();
        match selected {
            Some(LabelSelection::Corner(region, corner)) => {
                let Some(polygon) = sample.regions.get_mut(region).map(|r| &mut r.polygon) else {
                    return false;
                };
                if polygon.len() <= 3 {
                    sample.regions.remove(region);
                } else if corner < polygon.len() {
                    polygon.remove(corner);
                } else {
                    return false;
                }
                self.store_label(item)
            }
            Some(LabelSelection::Cut(cut)) => {
                if cut >= sample.cuts.len() {
                    return false;
                }
                sample.cuts.remove(cut);
                self.derive_and_store(item)
            }
            Some(LabelSelection::Region(region)) => {
                if region >= sample.regions.len() {
                    return false;
                }
                sample.regions.remove(region);
                self.store_label(item)
            }
            None => {
                let Some((index, _)) = self.active_letter() else {
                    return false;
                };
                let Some(region) = sample.regions_of(index).last() else {
                    return false;
                };
                sample.regions.remove(region);
                self.store_label(item)
            }
        }
    }

    /// Write every seed region's training polygon from the pieces, then store.
    fn derive_and_store(&mut self, mut item: NeuralItem) -> bool {
        if let Some(sample) = self.label.sample.and_then(|p| item.samples.get_mut(p)) {
            let paths = self.label_paths();
            let cuts: Vec<Line> = sample.cuts.iter().map(|c| cut_line(*c)).collect();
            let mut lost = 0;
            for region in &mut sample.regions {
                let Some(seed) = region.seed else {
                    continue;
                };
                match seed_polygon(&paths, &cuts, Point::new(seed[0], seed[1]), OVERLAP) {
                    Some(polygon) if polygon.len() >= 3 => region.polygon = polygon,
                    _ => lost += 1,
                }
            }
            if lost > 0 {
                self.label.error = Some(format!("{lost} seeds sit on no ink; move or delete them"));
            }
        }
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

    /// The lasso polygons of the selected sample, as shown.
    pub(crate) fn label_polygons(&self) -> Vec<LabelPolygon> {
        let Some(position) = self.label.sample else {
            return Vec::new();
        };
        let item = self.shown_neural_item();
        let Some(sample) = item.samples.get(position) else {
            return Vec::new();
        };
        let letters = sample.letters();
        sample
            .regions
            .iter()
            .enumerate()
            .filter(|(_, region)| region.seed.is_none() && !region.polygon.is_empty())
            .filter_map(|(position, region)| {
                let owner = region.owners.first()?;
                Some(LabelPolygon {
                    region: position,
                    letter: letters.iter().position(|(index, _)| index == owner)?,
                    corners: region
                        .polygon
                        .iter()
                        .map(|p| Point::new(p[0], p[1]))
                        .collect(),
                })
            })
            .collect()
    }

    /// The area every letter of every sample owns, in font units: its polygons and its whole
    /// contours. Painting clips the outline to each area.
    pub(crate) fn label_areas(&self) -> Vec<LabelArea> {
        let item = self.shown_neural_item();
        let contours = self.label_contours();
        let mut areas = Vec::new();
        for (sample_position, sample) in item.samples.iter().enumerate() {
            // While something moves, the shown item's seed polygons are stale: pieces are
            // cut again from the shown cuts.
            let moving_here =
                self.label.moving.is_some() && self.label.sample == Some(sample_position);
            let pieces = moving_here.then(|| {
                let cuts: Vec<Line> = sample.cuts.iter().map(|c| cut_line(*c)).collect();
                pieces(&self.label_paths(), &cuts)
            });
            for (letter, (index, _)) in sample.letters().into_iter().enumerate() {
                let mut area = BezPath::new();
                for position in sample.regions_of(index) {
                    let region = &sample.regions[position];
                    if let (Some(pieces), Some(seed)) = (&pieces, region.seed) {
                        if let Some(at) = piece_at(pieces, Point::new(seed[0], seed[1])) {
                            area.extend(pieces[at].elements().iter().copied());
                        }
                    } else if region.polygon.is_empty()
                        && let Some(at) = region.seed
                    {
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

fn cut_line(cut: [[f64; 2]; 2]) -> Line {
    Line::new((cut[0][0], cut[0][1]), (cut[1][0], cut[1][1]))
}

/// Whether a freehand drag over this layer's ink is a cut.
pub(crate) fn drag_is_cut(points: &[Point], contours: &[BezPath]) -> bool {
    is_cut_gesture(points, contours)
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

    fn owns(app: &Workspace, letter: usize, at: (f64, f64)) -> bool {
        app.session
            .label_areas()
            .iter()
            .any(|area| area.letter == letter && area.area.contains(Point::from(at)))
    }

    #[test]
    fn painting_cutting_and_erasing_label_the_ink() {
        let path = item_font("paint");
        let mut app = Workspace::open(&path).unwrap();
        app.open_glyph(app.font.index_of("item").unwrap());
        app.select_tool(Tool::Label);

        // no sample selected: the loop around the ink is a new sample
        app.edit_label(|s| s.add_label_lasso(&loop_around(50.0, 650.0, -50.0, 300.0)));
        assert_eq!(app.session.neural_item().samples.len(), 1);
        assert_eq!(app.session.label.sample, Some(0));

        // no text yet: nothing to paint with
        app.edit_label(|s| s.paint_piece(Point::new(150.0, 50.0), false));
        assert!(app.session.neural_item().samples[0].regions.is_empty());
        assert!(app.session.label.error.is_some());

        app.edit_label(|s| s.set_sample_text("ب س".into()));
        assert_eq!(app.session.active_letter(), Some((0, 'ب')));

        // one click: the whole bar is ب
        app.edit_label(|s| s.paint_piece(Point::new(150.0, 50.0), false));
        assert!(owns(&app, 0, (550.0, 50.0)));
        assert_eq!(
            app.session.label.active, 0,
            "painting does not move on by itself"
        );

        // a cut across the bar, then the right side is س
        app.edit_label(|s| s.add_cut(Point::new(350.0, -50.0), Point::new(350.0, 150.0)));
        assert_eq!(app.session.label.selected, Some(LabelSelection::Cut(0)));
        Arc::make_mut(&mut app.session).next_label_letter();
        assert_eq!(
            app.session.active_letter(),
            Some((2, 'س')),
            "the space is not a letter"
        );
        app.edit_label(|s| s.paint_piece(Point::new(500.0, 50.0), false));
        app.edit_label(|s| s.paint_piece(Point::new(475.0, 225.0), false));
        assert!(owns(&app, 0, (200.0, 50.0)) && !owns(&app, 0, (500.0, 50.0)));
        assert!(owns(&app, 1, (500.0, 50.0)) && owns(&app, 1, (475.0, 225.0)));
        assert!(
            owns(&app, 0, (360.0, 50.0)) && owns(&app, 1, (340.0, 50.0)),
            "both letters own a band across the cut"
        );
        let sample = app.session.neural_item().samples[0].clone();
        assert_eq!(sample.regions.len(), 3);
        assert!(
            sample
                .regions
                .iter()
                .all(|r| r.seed.is_some() && r.polygon.len() >= 3)
        );

        // painting a piece again with another letter changes hands; Option-click erases
        Arc::make_mut(&mut app.session).label.active = 0;
        app.edit_label(|s| s.paint_piece(Point::new(475.0, 225.0), false));
        assert!(owns(&app, 0, (475.0, 225.0)) && !owns(&app, 1, (475.0, 225.0)));
        app.edit_label(|s| s.paint_piece(Point::new(475.0, 225.0), true));
        assert!(!owns(&app, 0, (475.0, 225.0)));
        assert_eq!(app.session.neural_item().samples[0].regions.len(), 2);

        // what is under the pointer
        assert_eq!(
            app.session.label_hit(Point::new(352.0, -48.0), 8.0),
            LabelHit::CutEnd { cut: 0, end: 0 }
        );
        assert_eq!(
            app.session.label_hit(Point::new(352.0, 50.0), 8.0),
            LabelHit::Cut { cut: 0 }
        );
        assert!(matches!(
            app.session.label_hit(Point::new(200.0, 50.0), 8.0),
            LabelHit::Piece(_)
        ));
        assert_eq!(
            app.session.label_hit(Point::new(200.0, 500.0), 8.0),
            LabelHit::Nothing
        );

        // deleting the cut merges the pieces: both seeds now share the bar
        Arc::make_mut(&mut app.session).label.selected = Some(LabelSelection::Cut(0));
        app.edit_label(|s| s.delete_label_selection());
        assert!(app.session.neural_item().samples[0].cuts.is_empty());
        assert!(owns(&app, 0, (500.0, 50.0)) && owns(&app, 1, (200.0, 50.0)));
        app.undo_open_glyph(false);
        assert_eq!(app.session.neural_item().samples[0].cuts.len(), 1);
        assert!(!owns(&app, 0, (500.0, 50.0)));

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
    fn a_lasso_is_a_polygon_with_few_corners_that_can_be_edited() {
        let path = item_font("lasso");
        let mut app = Workspace::open(&path).unwrap();
        app.open_glyph(app.font.index_of("item").unwrap());
        app.select_tool(Tool::Label);
        app.edit_label(|s| s.add_label_lasso(&loop_around(50.0, 650.0, -50.0, 300.0)));
        app.edit_label(|s| s.set_sample_text("بس".into()));

        // a wobbly freehand loop becomes a handful of corners
        let wobbly: Vec<Point> = (0..240)
            .map(|i| {
                let t = f64::from(i) / 240.0 * std::f64::consts::TAU;
                let r = 120.0 + 3.0 * (t * 17.0).sin();
                Point::new(225.0 + r * t.cos(), 50.0 + r * t.sin())
            })
            .collect();
        app.edit_label(|s| s.add_label_lasso(&wobbly));
        let region = app.session.neural_item().samples[0].regions[0].clone();
        assert!(region.seed.is_none());
        assert!(
            region.polygon.len() >= 3 && region.polygon.len() <= LASSO_MAX_CORNERS,
            "{}",
            region.polygon.len()
        );
        assert_eq!(app.session.label.selected, Some(LabelSelection::Region(0)));
        assert!(owns(&app, 0, (200.0, 50.0)));

        // a second letter's square; its corner and edge are what a press finds
        Arc::make_mut(&mut app.session).step_label_letter(1);
        app.edit_label(|s| s.add_label_lasso(&square(300.0, 650.0)));
        assert_eq!(
            app.session.label_hit(Point::new(302.0, -48.0), 8.0),
            LabelHit::Corner {
                region: 1,
                corner: 0
            }
        );
        let edge = app.session.label_hit(Point::new(475.0, -46.0), 8.0);
        assert!(
            matches!(
                edge,
                LabelHit::Edge {
                    region: 1,
                    after: 0,
                    ..
                }
            ),
            "{edge:?}"
        );
        assert!(
            matches!(
                app.session.label_hit(Point::new(200.0, 50.0), 8.0),
                LabelHit::Piece(_)
            ),
            "the other letter's polygon is not editable"
        );

        // add a corner on the edge and drag it; a drag near a neighbor snaps
        let mut corner = None;
        app.edit_label(|s| {
            corner = s.add_label_corner(1, 0, Point::new(475.0, -50.0));
            corner.is_some()
        });
        assert_eq!(corner, Some(1));
        assert_eq!(
            app.session.neural_item().samples[0].regions[1]
                .polygon
                .len(),
            5
        );
        assert_eq!(
            app.session
                .snapped_label_corner(Point::new(403.0, 148.0), 8.0, Some(1)),
            Point::new(403.0, 148.0),
            "the wobbly loop has no corner there"
        );
        let session = Arc::make_mut(&mut app.session);
        session.label.moving = Some(LabelMoving::Corner {
            region: 1,
            corner: 1,
            to: Point::new(475.0, -120.0),
        });
        assert_eq!(
            session.label_polygons()[1].corners[1],
            Point::new(475.0, -120.0),
            "shown at once"
        );
        app.edit_label(|s| s.finish_label_move(true));
        assert_eq!(
            app.session.neural_item().samples[0].regions[1].polygon[1],
            [475.0, -120.0]
        );

        // Delete removes the selected corner; a triangle loses the whole region instead
        Arc::make_mut(&mut app.session).label.selected = Some(LabelSelection::Corner(1, 1));
        app.edit_label(|s| s.delete_label_selection());
        assert_eq!(
            app.session.neural_item().samples[0].regions[1]
                .polygon
                .len(),
            4
        );
        std::fs::remove_dir_all(&path).ok();
    }

    #[test]
    fn labels_survive_a_save_and_read_back_as_training_input() {
        let path = item_font("training");
        let mut app = Workspace::open(&path).unwrap();
        app.open_glyph(app.font.index_of("item").unwrap());
        app.select_tool(Tool::Label);
        app.edit_label(|s| s.add_label_lasso(&loop_around(50.0, 650.0, -50.0, 300.0)));
        app.edit_label(|s| s.set_sample_text("بس".into()));
        app.edit_label(|s| s.add_cut(Point::new(350.0, -50.0), Point::new(350.0, 150.0)));
        app.edit_label(|s| s.paint_piece(Point::new(150.0, 50.0), false));
        app.font.project.save().unwrap();

        // what a trainer reads: one letter still has no ink, and the error names it
        let source = nufo::Source::load(&path).unwrap();
        let canvas = &source.canvases[0];
        let error = nufo::training::prepare(&canvas.item.samples[0], &canvas.contours).unwrap_err();
        assert!(error.contains('س'), "{error}");

        Arc::make_mut(&mut app.session).next_label_letter();
        app.edit_label(|s| s.paint_piece(Point::new(500.0, 50.0), false));
        app.edit_label(|s| s.paint_piece(Point::new(475.0, 225.0), false));
        app.font.project.save().unwrap();

        let mut reopened = Workspace::open(&path).unwrap();
        reopened.open_glyph(reopened.font.index_of("item").unwrap());
        assert_eq!(reopened.session.neural_item(), app.session.neural_item());
        let source = nufo::Source::load(&path).unwrap();
        let canvas = &source.canvases[0];
        let prepared = nufo::training::prepare(&canvas.item.samples[0], &canvas.contours).unwrap();
        assert_eq!(prepared.text, "بس");
        assert_eq!(prepared.letters.len(), 2);
        // the trainer gets polygons it can clip with, grown across the cut
        let right = &prepared.letters[1].regions[0];
        let min_x = right.iter().map(|p| p.x).fold(f64::MAX, f64::min);
        assert!((min_x - (350.0 - OVERLAP)).abs() < 1.5, "{min_x}");
        std::fs::remove_dir_all(&path).ok();
    }
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A labeled example for a neural font: the text of one item and which ink belongs to each letter.
//!
//! In a neural source a glyph is not one code point.
//! It is a word, a phrase or a line of calligraphy, drawn as one outline.
//! The label says which parts of that outline each letter of the text owns.
//! A training run turns the outline and the label into examples for a model.
//!
//! A label is a set of regions with owners, not a partition of the outline.
//! Two joined letters share the ink where their regions overlap.
//! One letter can own several regions: its body, its dots and its marks.
//! Regions are stored as coordinates, so they survive a new trace of the same image.

use serde::{Deserialize, Serialize};

/// UFO glyph-lib key for the neural item of one glyph layer.
pub const NEURAL_ITEM_KEY: &str = "com.runebender.neuralItem";

/// One part of an item's ink and the letters that own it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeuralRegion {
    /// Indices of the owning letters, counted in characters of the item's text.
    pub owners: Vec<u32>,
    /// Corners of a polygon in font units; the owners own the ink inside it.
    /// Empty for a region that names a whole contour.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub polygon: Vec<[f64; 2]>,
    /// A point inside one whole contour that the owners own, such as a dot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contour_at: Option<[f64; 2]>,
}

impl NeuralRegion {
    /// Whether the region owns ink at `point`, given the item's contours.
    pub fn contains(&self, point: kurbo::Point, contours: &[kurbo::BezPath]) -> bool {
        use kurbo::Shape as _;
        if let Some(at) = self.contour_at {
            let at = kurbo::Point::new(at[0], at[1]);
            return contours
                .iter()
                .any(|contour| contour.contains(at) && contour.contains(point));
        }
        self.polygon_path().contains(point)
    }

    /// The polygon as a closed path; empty for a contour region.
    pub fn polygon_path(&self) -> kurbo::BezPath {
        let mut path = kurbo::BezPath::new();
        for (index, corner) in self.polygon.iter().enumerate() {
            let point = kurbo::Point::new(corner[0], corner[1]);
            if index == 0 {
                path.move_to(point);
            } else {
                path.line_to(point);
            }
        }
        if !self.polygon.is_empty() {
            path.close_path();
        }
        path
    }
}

/// The text of one neural item and the regions that label its ink.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeuralItem {
    /// Schema version; currently only version one is supported.
    pub version: u32,
    /// The Unicode text the ink spells, in logical order.
    pub text: String,
    /// Labeled regions; they can overlap and need not cover the outline.
    pub regions: Vec<NeuralRegion>,
}

impl Default for NeuralItem {
    fn default() -> Self {
        Self {
            version: 1,
            text: String::new(),
            regions: Vec::new(),
        }
    }
}

impl NeuralItem {
    /// Whether the item carries no text and no regions, so its key can be removed.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.regions.is_empty()
    }

    /// The letters that can own ink: every character of the text that is not white space.
    ///
    /// Each entry is the character's index in the text and the character.
    pub fn letters(&self) -> Vec<(u32, char)> {
        self.text
            .chars()
            .enumerate()
            .filter(|(_, c)| !c.is_whitespace())
            .map(|(index, c)| (index as u32, c))
            .collect()
    }

    /// Indices of the regions owned by the letter at `index`.
    pub fn regions_of(&self, index: u32) -> impl Iterator<Item = usize> + '_ {
        self.regions
            .iter()
            .enumerate()
            .filter(move |(_, region)| region.owners.contains(&index))
            .map(|(position, _)| position)
    }

    /// Letters that own no region yet.
    pub fn unlabeled(&self) -> Vec<(u32, char)> {
        self.letters()
            .into_iter()
            .filter(|(index, _)| self.regions_of(*index).next().is_none())
            .collect()
    }

    /// Validate the schema, the owners, and that every coordinate is finite and bounded.
    ///
    /// At most 4096 characters, 4096 regions and 4096 corners in one region are accepted.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err(format!("unsupported neural item version {}", self.version));
        }
        let length = self.text.chars().count();
        if length > 4096 || self.regions.len() > 4096 {
            return Err("too many characters or regions in a neural item".into());
        }
        let finite = |point: &[f64; 2]| {
            point
                .iter()
                .all(|value| value.is_finite() && value.abs() <= 1_000_000.0)
        };
        for region in &self.regions {
            if region.owners.is_empty()
                || region.owners.iter().any(|owner| *owner as usize >= length)
            {
                return Err("a neural region names a letter outside the text".into());
            }
            let polygon = !region.polygon.is_empty();
            if polygon == region.contour_at.is_some() {
                return Err("a neural region needs a polygon or a contour point, not both".into());
            }
            if polygon && (region.polygon.len() < 3 || region.polygon.len() > 4096) {
                return Err("a neural region polygon needs 3 to 4096 corners".into());
            }
            if !region.polygon.iter().all(finite) || !region.contour_at.iter().all(finite) {
                return Err("a neural region has a nonfinite or unbounded coordinate".into());
            }
        }
        Ok(())
    }

    /// Drop owners that fall outside a changed text, and regions left with none.
    ///
    /// Returns how many regions were removed.
    pub fn retain_valid_owners(&mut self) -> usize {
        let length = self.text.chars().count();
        let before = self.regions.len();
        for region in &mut self.regions {
            region.owners.retain(|owner| (*owner as usize) < length);
        }
        self.regions.retain(|region| !region.owners.is_empty());
        before - self.regions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, owner: u32) -> NeuralRegion {
        NeuralRegion {
            owners: vec![owner],
            polygon: vec![[x, 0.0], [x + 100.0, 0.0], [x + 100.0, 100.0], [x, 100.0]],
            contour_at: None,
        }
    }

    #[test]
    fn letters_skip_spaces_and_keep_text_indices() {
        let item = NeuralItem {
            text: "ab c".into(),
            ..NeuralItem::default()
        };
        assert_eq!(item.letters(), vec![(0, 'a'), (1, 'b'), (3, 'c')]);
        assert_eq!(item.unlabeled().len(), 3);
    }

    #[test]
    fn regions_validate_owners_shape_and_coordinates() {
        let mut item = NeuralItem {
            text: "ab".into(),
            regions: vec![square(0.0, 0), square(50.0, 1)],
            ..NeuralItem::default()
        };
        assert_eq!(item.validate(), Ok(()));
        assert!(item.unlabeled().is_empty());

        item.regions[1].owners = vec![2];
        assert!(item.validate().is_err());
        item.regions[1].owners = vec![1];
        item.regions[1].contour_at = Some([1.0, 1.0]);
        assert!(
            item.validate().is_err(),
            "polygon and contour point together"
        );
        item.regions[1].polygon.clear();
        assert_eq!(item.validate(), Ok(()));
        item.regions[1].contour_at = Some([f64::NAN, 1.0]);
        assert!(item.validate().is_err());
    }

    #[test]
    fn a_shorter_text_drops_regions_of_removed_letters() {
        let mut item = NeuralItem {
            text: "ab".into(),
            regions: vec![square(0.0, 0), square(50.0, 1)],
            ..NeuralItem::default()
        };
        item.text = "a".into();
        assert_eq!(item.retain_valid_owners(), 1);
        assert_eq!(item.validate(), Ok(()));
    }

    #[test]
    fn the_plist_encoding_round_trips() {
        let item = NeuralItem {
            text: "بسم".into(),
            regions: vec![
                square(0.0, 0),
                NeuralRegion {
                    owners: vec![0, 1],
                    polygon: Vec::new(),
                    contour_at: Some([12.5, -40.0]),
                },
            ],
            ..NeuralItem::default()
        };
        let value = plist::to_value(&item).unwrap();
        let back: NeuralItem = plist::from_value(&value).unwrap();
        assert_eq!(back, item);
    }

    #[test]
    fn a_contour_region_owns_the_whole_contour() {
        let dot = kurbo::Rect::new(0.0, 0.0, 10.0, 10.0);
        let contours = vec![kurbo::Shape::to_path(&dot, 0.1)];
        let region = NeuralRegion {
            owners: vec![0],
            polygon: Vec::new(),
            contour_at: Some([5.0, 5.0]),
        };
        assert!(region.contains(kurbo::Point::new(1.0, 9.0), &contours));
        assert!(!region.contains(kurbo::Point::new(20.0, 5.0), &contours));
    }
}

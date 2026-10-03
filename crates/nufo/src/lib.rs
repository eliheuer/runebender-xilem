// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! NUFO: the source format of a neural font.
//!
//! A `.nufo` is a UFO directory whose glyphs are canvases, not glyphs.
//! A canvas is an open drawing for one family of related shapes, such as the basic forms of ba.
//! It holds samples: each sample is one connected piece of writing, anywhere on the canvas,
//! with its own text and labels that say which ink belongs to each letter.
//! A training run reads the samples straight from the source.
//!
//! A sample's label is a set of regions with owners, not a partition of its ink.
//! Two joined letters share the ink where their regions overlap.
//! One letter can own several regions: its body, its dots and its marks.
//! Regions and boundaries are stored as coordinates, so they survive a new trace.
//!
//! [`NeuralItem`] is the label of one canvas, stored in its glyph lib under [`NEURAL_ITEM_KEY`].
//! [`Source::load`] reads a whole `.nufo`, and [`training`] turns a sample into the ink each of
//! its letters owns.

pub mod training;

pub use training::{LetterInk, TrainingSample};

/// File extension of a neural source directory. The directory is a UFO in every other way.
pub const EXTENSION: &str = "nufo";

use serde::{Deserialize, Serialize};

/// UFO glyph-lib key for the neural item of one glyph layer.
pub const NEURAL_ITEM_KEY: &str = "com.runebender.neuralItem";

/// One part of a sample's ink and the letters that own it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeuralRegion {
    /// Indices of the owning letters, counted in characters of the sample's text.
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
    /// The polygon as a closed path; empty for a contour region.
    pub fn polygon_path(&self) -> kurbo::BezPath {
        polygon_path(&self.polygon)
    }
}

/// A closed path through polygon corners; empty for no corners.
pub fn polygon_path(corners: &[[f64; 2]]) -> kurbo::BezPath {
    let mut path = kurbo::BezPath::new();
    for (index, corner) in corners.iter().enumerate() {
        let point = kurbo::Point::new(corner[0], corner[1]);
        if index == 0 {
            path.move_to(point);
        } else {
            path.line_to(point);
        }
    }
    if !corners.is_empty() {
        path.close_path();
    }
    path
}

/// One connected piece of writing on a canvas, with its text and its letter labels.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeuralSample {
    /// A loose loop around the sample's ink, in font units.
    pub boundary: Vec<[f64; 2]>,
    /// The Unicode text the ink spells, in logical order.
    #[serde(default)]
    pub text: String,
    /// Labeled regions; they can overlap and need not cover the ink.
    #[serde(default)]
    pub regions: Vec<NeuralRegion>,
}

impl NeuralSample {
    /// The boundary as a closed path.
    pub fn boundary_path(&self) -> kurbo::BezPath {
        polygon_path(&self.boundary)
    }

    /// The letters that can own ink: every character of the text that is not white space.
    ///
    /// Each entry is the character's index in the text and the character.
    pub fn letters(&self) -> Vec<(u32, char)> {
        self.text
            .chars()
            .enumerate()
            .filter(|(_, c)| !c.is_whitespace())
            .filter_map(|(index, c)| Some((u32::try_from(index).ok()?, c)))
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

    /// The contours of a canvas that belong to this sample: those mostly inside its boundary.
    pub fn contours_in<'a>(
        &self,
        contours: &'a [kurbo::BezPath],
    ) -> impl Iterator<Item = (usize, &'a kurbo::BezPath)> + 'a {
        use kurbo::Shape as _;
        let boundary = self.boundary_path();
        contours.iter().enumerate().filter(move |(_, contour)| {
            let mut inside = 0;
            let mut total = 0;
            for element in contour.elements() {
                if let Some(point) = element.end_point() {
                    total += 1;
                    inside += usize::from(boundary.contains(point));
                }
            }
            total > 0 && inside * 2 > total
        })
    }

    fn validate(&self) -> Result<(), String> {
        let length = self.text.chars().count();
        if length > 4096 || self.regions.len() > 4096 {
            return Err("too many characters or regions in a neural sample".into());
        }
        if self.boundary.len() < 3 || self.boundary.len() > 4096 {
            return Err("a neural sample boundary needs 3 to 4096 corners".into());
        }
        if !self.boundary.iter().all(finite) {
            return Err("a neural sample boundary has a nonfinite or unbounded corner".into());
        }
        for region in &self.regions {
            if region.owners.is_empty()
                || region.owners.iter().any(|owner| *owner as usize >= length)
            {
                return Err("a neural region names a letter outside its sample's text".into());
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
}

fn finite(point: &[f64; 2]) -> bool {
    point
        .iter()
        .all(|value| value.is_finite() && value.abs() <= 1_000_000.0)
}

/// The samples of one canvas.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeuralItem {
    /// Schema version; currently only version two is supported.
    pub version: u32,
    /// The canvas's samples, in the order they were made.
    pub samples: Vec<NeuralSample>,
}

impl Default for NeuralItem {
    fn default() -> Self {
        Self {
            version: 2,
            samples: Vec::new(),
        }
    }
}

/// Version one held one text for the whole canvas; it reads as a canvas with no samples, since
/// it has no boundary to place a sample by.
#[derive(Deserialize)]
struct LegacyItem {
    version: u32,
}

impl NeuralItem {
    /// Decode a stored item, reading a version one item as an empty canvas.
    pub fn from_value(value: &plist::Value) -> Result<Self, String> {
        if let Ok(item) = plist::from_value::<Self>(value) {
            item.validate()?;
            return Ok(item);
        }
        match plist::from_value::<LegacyItem>(value) {
            Ok(LegacyItem { version: 1 }) => Ok(Self::default()),
            _ => Err("unreadable neural item".into()),
        }
    }

    /// Whether the canvas has no samples, so its key can be removed.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Validate the schema and every sample.
    ///
    /// At most 4096 samples are accepted; each sample has the limits of its own validation.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 2 {
            return Err(format!("unsupported neural item version {}", self.version));
        }
        if self.samples.len() > 4096 {
            return Err("too many samples in a neural item".into());
        }
        self.samples.iter().try_for_each(NeuralSample::validate)
    }
}

/// One canvas of a source: its name, its contours, and its samples.
#[derive(Clone, Debug)]
pub struct Canvas {
    /// The glyph name, such as `ba-basic`.
    pub name: String,
    /// Every contour of the canvas, in font units.
    pub contours: Vec<kurbo::BezPath>,
    /// The canvas's samples and their labels.
    pub item: NeuralItem,
}

/// A neural source read from disk.
#[derive(Clone, Debug)]
pub struct Source {
    /// Units per em of the source.
    pub units_per_em: f64,
    /// The canvases of the default layer, in glyph order.
    pub canvases: Vec<Canvas>,
}

impl Source {
    /// Read a `.nufo` directory.
    ///
    /// A canvas with an unreadable label is an error, not an empty canvas: training on a
    /// source must not quietly skip work.
    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let font =
            norad::Font::load(path).map_err(|error| format!("{}: {error}", path.display()))?;
        let units_per_em = font
            .font_info
            .units_per_em
            .map_or(1000.0, |value| value.as_f64());
        let mut canvases = Vec::new();
        for glyph in font.default_layer().iter() {
            let item = match glyph.lib.get(NEURAL_ITEM_KEY) {
                Some(value) => NeuralItem::from_value(value)
                    .map_err(|error| format!("{}: {error}", glyph.name()))?,
                None => NeuralItem::default(),
            };
            canvases.push(Canvas {
                name: glyph.name().to_string(),
                contours: glyph.contours.iter().map(contour_path).collect(),
                item,
            });
        }
        canvases.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Self {
            units_per_em,
            canvases,
        })
    }
}

/// A UFO contour as a closed cubic path.
pub fn contour_path(contour: &norad::Contour) -> kurbo::BezPath {
    let points = &contour.points;
    let mut path = kurbo::BezPath::new();
    let Some(start) = points
        .iter()
        .position(|point| point.typ != norad::PointType::OffCurve)
    else {
        return path;
    };
    let point = |index: usize| {
        let p = &points[index % points.len()];
        kurbo::Point::new(p.x, p.y)
    };
    path.move_to(point(start));
    let mut pending: Vec<kurbo::Point> = Vec::new();
    for step in 1..=points.len() {
        let index = (start + step) % points.len();
        let p = &points[index];
        match p.typ {
            norad::PointType::OffCurve => pending.push(point(index)),
            norad::PointType::Curve => {
                match pending.as_slice() {
                    [a, b] => path.curve_to(*a, *b, point(index)),
                    [a] => path.quad_to(*a, point(index)),
                    _ => path.line_to(point(index)),
                }
                pending.clear();
            }
            norad::PointType::QCurve => {
                for pair in pending.windows(2) {
                    let mid = pair[0].midpoint(pair[1]);
                    path.quad_to(pair[0], mid);
                }
                match pending.last() {
                    Some(last) => path.quad_to(*last, point(index)),
                    None => path.line_to(point(index)),
                }
                pending.clear();
            }
            _ => {
                path.line_to(point(index));
                pending.clear();
            }
        }
    }
    path.close_path();
    path
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

    fn sample(text: &str) -> NeuralSample {
        NeuralSample {
            boundary: vec![
                [-50.0, -50.0],
                [400.0, -50.0],
                [400.0, 200.0],
                [-50.0, 200.0],
            ],
            text: text.into(),
            regions: Vec::new(),
        }
    }

    #[test]
    fn letters_skip_spaces_and_keep_text_indices() {
        let sample = sample("ab c");
        assert_eq!(sample.letters(), vec![(0, 'a'), (1, 'b'), (3, 'c')]);
        assert_eq!(sample.unlabeled().len(), 3);
    }

    #[test]
    fn regions_validate_owners_shape_and_coordinates() {
        let mut item = NeuralItem {
            samples: vec![sample("ab")],
            ..NeuralItem::default()
        };
        item.samples[0].regions = vec![square(0.0, 0), square(50.0, 1)];
        assert_eq!(item.validate(), Ok(()));
        assert!(item.samples[0].unlabeled().is_empty());

        let region = &mut item.samples[0].regions[1];
        region.owners = vec![2];
        assert!(item.validate().is_err());
        let region = &mut item.samples[0].regions[1];
        region.owners = vec![1];
        region.contour_at = Some([1.0, 1.0]);
        assert!(
            item.validate().is_err(),
            "polygon and contour point together"
        );
        item.samples[0].regions[1].polygon.clear();
        assert_eq!(item.validate(), Ok(()));
        item.samples[0].boundary.truncate(2);
        assert!(item.validate().is_err(), "a sample needs a boundary");
    }

    #[test]
    fn a_shorter_text_drops_regions_of_removed_letters() {
        let mut sample = sample("ab");
        sample.regions = vec![square(0.0, 0), square(50.0, 1)];
        sample.text = "a".into();
        assert_eq!(sample.retain_valid_owners(), 1);
    }

    #[test]
    fn the_plist_encoding_round_trips_and_reads_version_one_as_empty() {
        let mut first = sample("بسم");
        first.regions = vec![
            square(0.0, 0),
            NeuralRegion {
                owners: vec![0, 1],
                polygon: Vec::new(),
                contour_at: Some([12.5, -40.0]),
            },
        ];
        let item = NeuralItem {
            samples: vec![first, sample("بب")],
            ..NeuralItem::default()
        };
        let value = plist::to_value(&item).unwrap();
        assert_eq!(NeuralItem::from_value(&value), Ok(item));

        let mut legacy = plist::Dictionary::new();
        legacy.insert("version".into(), 1.into());
        legacy.insert("text".into(), "بسم".into());
        legacy.insert("regions".into(), plist::Value::Array(Vec::new()));
        assert_eq!(
            NeuralItem::from_value(&plist::Value::Dictionary(legacy)),
            Ok(NeuralItem::default())
        );
    }

    #[test]
    fn a_sample_takes_the_contours_mostly_inside_its_boundary() {
        use kurbo::Shape as _;
        let inside = kurbo::Rect::new(0.0, 0.0, 100.0, 100.0).to_path(0.1);
        let across = kurbo::Rect::new(300.0, 0.0, 600.0, 100.0).to_path(0.1);
        let outside = kurbo::Rect::new(700.0, 0.0, 800.0, 100.0).to_path(0.1);
        let contours = vec![inside, across, outside];
        let taken: Vec<usize> = sample("a").contours_in(&contours).map(|(i, _)| i).collect();
        assert_eq!(
            taken,
            vec![0],
            "a contour half outside its boundary is not taken"
        );
    }

    #[test]
    fn a_source_loads_its_canvases_with_their_samples_and_contours() {
        let path = std::env::temp_dir().join(format!("nufo-load-{}.nufo", std::process::id()));
        let mut font = norad::Font::new();
        font.font_info.units_per_em =
            Some(norad::fontinfo::NonNegativeIntegerOrFloat::new(2048.0).unwrap());
        let point =
            |x, y| norad::ContourPoint::new(x, y, norad::PointType::Line, false, None, None);
        let mut glyph = norad::Glyph::new("ba-basic");
        glyph.contours.push(norad::Contour::new(
            vec![
                point(0.0, 0.0),
                point(300.0, 0.0),
                point(300.0, 100.0),
                point(0.0, 100.0),
            ],
            None,
        ));
        let item = NeuralItem {
            samples: vec![sample("بسم")],
            ..NeuralItem::default()
        };
        glyph
            .lib
            .insert(NEURAL_ITEM_KEY.into(), plist::to_value(&item).unwrap());
        font.default_layer_mut().insert_glyph(glyph);
        font.default_layer_mut()
            .insert_glyph(norad::Glyph::new("empty"));
        font.save(&path).unwrap();

        let source = Source::load(&path).unwrap();
        assert_eq!(source.units_per_em, 2048.0);
        assert_eq!(source.canvases.len(), 2);
        let canvas = &source.canvases[0];
        assert_eq!(canvas.name, "ba-basic");
        assert_eq!(canvas.item, item);
        assert_eq!(canvas.contours.len(), 1);
        use kurbo::Shape as _;
        assert_eq!(
            canvas.contours[0].bounding_box(),
            kurbo::Rect::new(0.0, 0.0, 300.0, 100.0)
        );
        assert!(source.canvases[1].item.is_empty());
        std::fs::remove_dir_all(&path).ok();
    }
}

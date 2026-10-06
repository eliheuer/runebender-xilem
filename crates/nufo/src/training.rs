// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! From a labeled sample to training input: the ink each letter owns.
//!
//! A sample's ink is the contours mostly inside its boundary. Each letter owns the ink inside
//! its polygons, plus the whole contours its contour regions point into. A trainer rasterizes
//! each letter's share and learns it in the context of the sample's text.

use kurbo::{BezPath, Shape as _};

use crate::NeuralSample;

/// The ink one letter of a sample owns.
#[derive(Clone, Debug, PartialEq)]
pub struct LetterInk {
    /// The letter.
    pub letter: char,
    /// Polygons in font units: the letter owns the sample's ink inside them.
    pub regions: Vec<Vec<kurbo::Point>>,
    /// Whole contours the letter owns, such as its dots.
    pub contours: Vec<BezPath>,
}

/// One sample, ready for a trainer.
#[derive(Clone, Debug, PartialEq)]
pub struct TrainingSample {
    /// The sample's text, with each run of white space as one space.
    pub text: String,
    /// The sample's ink.
    pub outline: BezPath,
    /// One entry per letter of the text, in logical order, without spaces.
    pub letters: Vec<LetterInk>,
}

/// The training input of one sample, given every contour of its canvas.
///
/// Every letter must own at least one region, and every contour region must point into one of
/// the sample's contours. The error says which part is missing.
pub fn prepare(sample: &NeuralSample, contours: &[BezPath]) -> Result<TrainingSample, String> {
    if sample.text.trim().is_empty() {
        return Err("the sample has no text".into());
    }
    let own: Vec<&BezPath> = sample
        .contours_in(contours)
        .map(|(_, contour)| contour)
        .collect();
    if own.is_empty() {
        return Err("the sample's boundary holds no ink".into());
    }
    let unlabeled = sample.unlabeled();
    if !unlabeled.is_empty() {
        let letters: String = unlabeled.iter().map(|(_, c)| *c).collect();
        return Err(format!(
            "{} letters have no region yet: {letters}",
            unlabeled.len()
        ));
    }
    let mut outline = BezPath::new();
    for contour in &own {
        outline.extend(contour.elements().iter().copied());
    }
    let mut letters = Vec::new();
    for (index, letter) in sample.letters() {
        let mut ink = LetterInk {
            letter,
            regions: Vec::new(),
            contours: Vec::new(),
        };
        for position in sample.regions_of(index) {
            let region = &sample.regions[position];
            if region.polygon.is_empty()
                && let Some(at) = region.seed
            {
                // An older seed region without its derived polygon: the whole contour.
                let at = kurbo::Point::new(at[0], at[1]);
                let owned = own
                    .iter()
                    .find(|contour| contour.contains(at))
                    .ok_or_else(|| {
                        format!("a contour region of {letter} points at no contour of its sample")
                    })?;
                ink.contours.push((*owned).clone());
            } else {
                ink.regions.push(
                    region
                        .polygon
                        .iter()
                        .map(|p| kurbo::Point::new(p[0], p[1]))
                        .collect(),
                );
            }
        }
        letters.push(ink);
    }
    Ok(TrainingSample {
        text: sample.text.split_whitespace().collect::<Vec<_>>().join(" "),
        outline,
        letters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NeuralRegion;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        kurbo::Rect::new(x0, y0, x1, y1).to_path(0.1)
    }

    fn polygon(x0: f64, x1: f64) -> Vec<[f64; 2]> {
        vec![[x0, -10.0], [x1, -10.0], [x1, 110.0], [x0, 110.0]]
    }

    #[test]
    fn a_labeled_sample_lists_one_cluster_per_letter_and_only_its_own_ink() {
        // the sample's body and dot, and a second piece of writing it must not take
        let contours = vec![
            rect(0.0, 0.0, 300.0, 100.0),
            rect(40.0, 150.0, 60.0, 170.0),
            rect(1000.0, 0.0, 1200.0, 100.0),
        ];
        let sample = NeuralSample {
            cuts: Vec::new(),
            boundary: vec![
                [-50.0, -50.0],
                [350.0, -50.0],
                [350.0, 250.0],
                [-50.0, 250.0],
            ],
            text: "ab  c".into(),
            regions: vec![
                NeuralRegion {
                    owners: vec![0],
                    polygon: polygon(-10.0, 120.0),
                    seed: None,
                },
                NeuralRegion {
                    owners: vec![0],
                    polygon: Vec::new(),
                    seed: Some([50.0, 160.0]),
                },
                NeuralRegion {
                    owners: vec![1, 4],
                    polygon: polygon(90.0, 310.0),
                    seed: None,
                },
            ],
        };
        let prepared = prepare(&sample, &contours).unwrap();
        assert_eq!(prepared.text, "ab c", "runs of spaces collapse to one");
        assert_eq!(prepared.letters.len(), 3);
        assert_eq!(prepared.letters[0].letter, 'a');
        assert_eq!(prepared.letters[0].contours.len(), 1, "the dot");
        assert_eq!(
            prepared.letters[2].regions, prepared.letters[1].regions,
            "shared ink"
        );
        assert!(
            prepared.outline.bounding_box().x1 < 400.0,
            "the other piece of writing stays out"
        );
    }

    #[test]
    fn an_unlabeled_letter_or_an_empty_boundary_is_named_in_the_error() {
        let mut sample = NeuralSample {
            cuts: Vec::new(),
            boundary: vec![[-5.0, -5.0], [15.0, -5.0], [15.0, 15.0], [-5.0, 15.0]],
            text: "ab".into(),
            regions: vec![NeuralRegion {
                owners: vec![0],
                polygon: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
                seed: None,
            }],
        };
        let error = prepare(&sample, &[rect(0.0, 0.0, 10.0, 10.0)]).unwrap_err();
        assert!(error.contains('b'), "{error}");
        sample.boundary = vec![[500.0, 500.0], [600.0, 500.0], [600.0, 600.0]];
        let error = prepare(&sample, &[rect(0.0, 0.0, 10.0, 10.0)]).unwrap_err();
        assert!(error.contains("no ink"), "{error}");
    }
}

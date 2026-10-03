// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Phrase files: each labeled sample of a neural canvas as training input.
//!
//! A phrase file is JSON.
//! It carries the text of one sample, the outline of its ink, and for each letter the regions
//! and whole contours that letter owns.
//! The `NeuralType` tools read it and turn it into training rows (`distill hand`).
//!
//! Coordinates are font units with y up.
//! The `clusters` array lists every letter of every word in logical order, with spaces left out.

use kurbo::BezPath;

use crate::font::LayerView;
use crate::font::model::neural_item::NeuralSample;
use crate::font::project::Project;
use crate::font::variable::SourceId;

/// What an export of phrase files did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhraseExport {
    /// Names of the files written, without the directory.
    pub written: Vec<String>,
    /// Samples that are not ready, each named `glyph #n`, with the reason.
    pub skipped: Vec<(String, String)>,
}

/// Write one phrase file for every labeled sample of a source into `directory`.
///
/// A file is named after its canvas and the sample's position on it, as in `ba-basic-1.json`.
/// A sample that is not ready, such as one with a letter that owns no ink, is reported in
/// `skipped` and writes nothing.
pub fn export_source(
    project: &Project,
    source: SourceId,
    directory: &std::path::Path,
) -> Result<PhraseExport, String> {
    let default_layer = project
        .document_source(source)
        .ok_or("the source does not exist")?
        .default_layer();
    let upm = project
        .document_font_info(source)
        .ok_or("the source has no font information")?
        .metrics
        .resolved()
        .units_per_em;
    let mut export = PhraseExport::default();
    let names: Vec<String> = project.glyph_names().map(str::to_owned).collect();
    for name in names {
        let Some(layer) = project.document_layer(&name, &default_layer) else {
            continue;
        };
        for (position, phrase) in layer_phrases(layer, upm).into_iter().enumerate() {
            let number = position + 1;
            match phrase {
                Ok(phrase) => {
                    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
                    let file = format!("{name}-{number}.json");
                    let text = serde_json::to_string(&phrase).map_err(|error| error.to_string())?;
                    std::fs::write(directory.join(&file), text)
                        .map_err(|error| error.to_string())?;
                    export.written.push(file);
                }
                Err(reason) => export.skipped.push((format!("{name} #{number}"), reason)),
            }
        }
    }
    Ok(export)
}

/// The phrase file of every sample on one layer, or the reason a sample is not ready.
///
/// An unreadable label reads as no samples.
pub fn layer_phrases(layer: LayerView<'_>, upm: f64) -> Vec<Result<serde_json::Value, String>> {
    let item = layer.neural_item().unwrap_or_default();
    let contours: Vec<BezPath> = layer
        .contours()
        .map(|contour| crate::outline::path::Path::from_document_contour(contour).to_bezpath())
        .collect();
    item.samples
        .iter()
        .map(|sample| phrase(sample, &contours, upm))
        .collect()
}

/// The phrase file of one sample, given every contour of its canvas.
///
/// The sample's ink is the contours mostly inside its boundary. Every letter must own at least
/// one region, and every contour region must point into one of the sample's contours.
pub fn phrase(
    sample: &NeuralSample,
    contours: &[BezPath],
    upm: f64,
) -> Result<serde_json::Value, String> {
    use kurbo::Shape as _;
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
    let mut clusters = Vec::new();
    for (index, letter) in sample.letters() {
        let mut regions = Vec::new();
        let mut paths = Vec::new();
        for position in sample.regions_of(index) {
            let region = &sample.regions[position];
            if let Some(at) = region.contour_at {
                let at = kurbo::Point::new(at[0], at[1]);
                let owned = own
                    .iter()
                    .find(|contour| contour.contains(at))
                    .ok_or_else(|| {
                        format!("a contour region of {letter} points at no contour of its sample")
                    })?;
                paths.push(owned.to_svg());
            } else {
                regions.push(region.polygon.clone());
            }
        }
        clusters.push(serde_json::json!({
            "letters": letter.to_string(),
            "regions": regions,
            "paths": paths,
        }));
    }
    Ok(serde_json::json!({
        "text": sample.text.split_whitespace().collect::<Vec<_>>().join(" "),
        "upm": upm,
        "outline": outline.to_svg(),
        "baseline_y": 0.0,
        "clusters": clusters,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font::model::neural_item::NeuralRegion;
    use kurbo::Shape as _;

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
                    contour_at: None,
                },
                NeuralRegion {
                    owners: vec![0],
                    polygon: Vec::new(),
                    contour_at: Some([50.0, 160.0]),
                },
                NeuralRegion {
                    owners: vec![1, 4],
                    polygon: polygon(90.0, 310.0),
                    contour_at: None,
                },
            ],
        };
        let file = phrase(&sample, &contours, 1000.0).unwrap();
        assert_eq!(file["text"], "ab c", "runs of spaces collapse to one");
        let clusters = file["clusters"].as_array().unwrap();
        assert_eq!(clusters.len(), 3);
        assert_eq!(clusters[0]["letters"], "a");
        assert_eq!(clusters[0]["paths"].as_array().unwrap().len(), 1, "the dot");
        assert_eq!(clusters[2]["regions"], clusters[1]["regions"], "shared ink");
        let outline = BezPath::from_svg(file["outline"].as_str().unwrap()).unwrap();
        assert!(
            outline.bounding_box().x1 < 400.0,
            "the other piece of writing stays out"
        );
    }

    #[test]
    fn an_unlabeled_letter_or_an_empty_boundary_is_named_in_the_error() {
        let mut sample = NeuralSample {
            boundary: vec![[-5.0, -5.0], [15.0, -5.0], [15.0, 15.0], [-5.0, 15.0]],
            text: "ab".into(),
            regions: vec![NeuralRegion {
                owners: vec![0],
                polygon: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
                contour_at: None,
            }],
        };
        let error = phrase(&sample, &[rect(0.0, 0.0, 10.0, 10.0)], 1000.0).unwrap_err();
        assert!(error.contains('b'), "{error}");
        sample.boundary = vec![[500.0, 500.0], [600.0, 500.0], [600.0, 600.0]];
        let error = phrase(&sample, &[rect(0.0, 0.0, 10.0, 10.0)], 1000.0).unwrap_err();
        assert!(error.contains("no ink"), "{error}");
    }
}

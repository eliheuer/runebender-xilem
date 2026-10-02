// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The phrase file: one labeled neural item as training input.
//!
//! A phrase file is JSON.
//! It carries the text of an item, the outline of its ink, and for each letter the regions
//! and whole contours that letter owns.
//! The NeuralType tools read it and turn it into training rows (`distill hand`).
//!
//! Coordinates are font units with y up.
//! The `clusters` array lists every letter of every word in logical order, with spaces left out.

use kurbo::BezPath;

use crate::font::LayerView;
use crate::font::model::neural_item::NeuralItem;
use crate::font::project::Project;
use crate::font::variable::SourceId;

/// What an export of phrase files did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PhraseExport {
    /// Names of the glyphs whose phrase files were written.
    pub written: Vec<String>,
    /// Glyphs with a neural item that is not ready, each with the reason.
    pub skipped: Vec<(String, String)>,
}

/// Write one phrase file for every labeled item of a source into `directory`.
///
/// A glyph without a neural item is not an item and is passed over. An item that is not ready,
/// such as one with a letter that owns no ink, is reported in `skipped` and writes nothing.
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
        if layer.neural_item().is_ok_and(|item| item.is_empty()) {
            continue;
        }
        match layer_phrase(layer, upm) {
            Ok(phrase) => {
                std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
                let text = serde_json::to_string(&phrase).map_err(|error| error.to_string())?;
                std::fs::write(directory.join(format!("{name}.json")), text)
                    .map_err(|error| error.to_string())?;
                export.written.push(name);
            }
            Err(reason) => export.skipped.push((name, reason)),
        }
    }
    Ok(export)
}

/// The phrase file for one layer, or the reason it cannot be written yet.
pub fn layer_phrase(layer: LayerView<'_>, upm: f64) -> Result<serde_json::Value, String> {
    let item = layer.neural_item().map_err(|error| error.to_string())?;
    let contours: Vec<BezPath> = layer
        .contours()
        .map(|contour| crate::outline::path::Path::from_document_contour(contour).to_bezpath())
        .collect();
    phrase(&item, &contours, upm)
}

/// The phrase file for an item and the contours of its ink.
///
/// Every letter must own at least one region, and every contour region must point into a contour.
pub fn phrase(
    item: &NeuralItem,
    contours: &[BezPath],
    upm: f64,
) -> Result<serde_json::Value, String> {
    use kurbo::Shape as _;
    item.validate()?;
    if item.text.trim().is_empty() {
        return Err("the item has no text".into());
    }
    if contours.is_empty() {
        return Err("the item has no outline".into());
    }
    let unlabeled = item.unlabeled();
    if !unlabeled.is_empty() {
        let letters: String = unlabeled.iter().map(|(_, c)| *c).collect();
        return Err(format!(
            "{} letters have no region yet: {letters}",
            unlabeled.len()
        ));
    }
    let mut outline = BezPath::new();
    for contour in contours {
        outline.extend(contour.elements().iter().copied());
    }
    let mut clusters = Vec::new();
    for (index, letter) in item.letters() {
        let mut regions = Vec::new();
        let mut paths = Vec::new();
        for position in item.regions_of(index) {
            let region = &item.regions[position];
            if let Some(at) = region.contour_at {
                let at = kurbo::Point::new(at[0], at[1]);
                let owned = contours
                    .iter()
                    .find(|contour| contour.contains(at))
                    .ok_or_else(|| format!("a contour region of {letter} points at no contour"))?;
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
        "text": item.text.split_whitespace().collect::<Vec<_>>().join(" "),
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

    #[test]
    fn a_labeled_item_lists_one_cluster_per_letter() {
        let contours = vec![rect(0.0, 0.0, 300.0, 100.0), rect(40.0, 150.0, 60.0, 170.0)];
        let item = NeuralItem {
            text: "ab  c".into(),
            regions: vec![
                NeuralRegion {
                    owners: vec![0],
                    polygon: vec![
                        [-10.0, -10.0],
                        [120.0, -10.0],
                        [120.0, 110.0],
                        [-10.0, 110.0],
                    ],
                    contour_at: None,
                },
                NeuralRegion {
                    owners: vec![0],
                    polygon: Vec::new(),
                    contour_at: Some([50.0, 160.0]),
                },
                NeuralRegion {
                    owners: vec![1, 4],
                    polygon: vec![[90.0, -10.0], [310.0, -10.0], [310.0, 110.0], [90.0, 110.0]],
                    contour_at: None,
                },
            ],
            ..NeuralItem::default()
        };
        let file = phrase(&item, &contours, 1000.0).unwrap();
        assert_eq!(file["text"], "ab c", "runs of spaces collapse to one");
        let clusters = file["clusters"].as_array().unwrap();
        assert_eq!(clusters.len(), 3);
        assert_eq!(clusters[0]["letters"], "a");
        assert_eq!(clusters[0]["regions"].as_array().unwrap().len(), 1);
        assert_eq!(clusters[0]["paths"].as_array().unwrap().len(), 1, "the dot");
        assert_eq!(clusters[2]["letters"], "c");
        assert_eq!(clusters[2]["regions"], clusters[1]["regions"], "shared ink");
        assert!(BezPath::from_svg(file["outline"].as_str().unwrap()).is_ok());
    }

    #[test]
    fn an_unlabeled_letter_is_named_in_the_error() {
        let item = NeuralItem {
            text: "ab".into(),
            regions: vec![NeuralRegion {
                owners: vec![0],
                polygon: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]],
                contour_at: None,
            }],
            ..NeuralItem::default()
        };
        let error = phrase(&item, &[rect(0.0, 0.0, 10.0, 10.0)], 1000.0).unwrap_err();
        assert!(error.contains('b'), "{error}");
    }
}

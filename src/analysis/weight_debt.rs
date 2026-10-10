// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! What a heavier master still owes.
//!
//! While a family grows a second weight, the custom is to copy every
//! lighter glyph into the heavier master so the build interpolates,
//! then redraw each copy. A copy that still matches its lighter glyph
//! point for point is debt. This module lists that debt by script, and
//! scores the no-model offset of [`crate::outline::embolden`] against
//! the glyphs already drawn in both masters, so a draft arrives with
//! the error it is likely to carry.
//!
//! Nothing here changes a font.

use std::collections::BTreeMap;

use kurbo::Point;

use crate::analysis::category::GlyphCategory;
use crate::font::LayerView;
use crate::font::project::Project;
use crate::font::variable::{LayerId, SourceId};
use crate::outline::embolden::{self, Offset};

/// A group of glyphs that gain weight the same way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Script {
    /// Latin letters, and anything with no better home.
    Latin,
    /// Arabic letters and their forms.
    Arabic,
    /// Hebrew letters and points.
    Hebrew,
    /// Combining and spacing marks outside Arabic and Hebrew.
    Marks,
    /// Figures, punctuation and symbols.
    Figures,
    /// Glyphs with no codepoint and no script in their name.
    Other,
}

impl Script {
    /// The label a panel shows.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Latin => "Latin",
            Self::Arabic => "Arabic",
            Self::Hebrew => "Hebrew",
            Self::Marks => "Marks",
            Self::Figures => "Figures and symbols",
            Self::Other => "Other",
        }
    }

    /// Where a glyph belongs, from its codepoint when it has one and
    /// its name otherwise.
    pub fn of(name: &str, codepoint: Option<char>) -> Self {
        if let Some(c) = codepoint {
            let code = c as u32;
            if matches!(
                code,
                0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF
            ) {
                return Self::Arabic;
            }
            if matches!(code, 0x0590..=0x05FF | 0xFB1D..=0xFB4F) {
                return Self::Hebrew;
            }
            return match GlyphCategory::from_codepoint(c) {
                GlyphCategory::Letter => Self::Latin,
                GlyphCategory::Mark => Self::Marks,
                GlyphCategory::Number | GlyphCategory::Punctuation | GlyphCategory::Symbol => {
                    Self::Figures
                }
                _ => Self::Other,
            };
        }
        let base = name.split('.').next().unwrap_or(name);
        if base.ends_with("-ar") {
            Self::Arabic
        } else if base.ends_with("-hb") {
            Self::Hebrew
        } else if base.ends_with("comb") {
            Self::Marks
        } else {
            Self::Other
        }
    }
}

/// One script's debt and its evidence.
#[derive(Clone, Debug, PartialEq)]
pub struct DebtGroup {
    /// Which script.
    pub script: Script,
    /// Glyphs whose heavier copy still matches the lighter glyph.
    pub pending: Vec<String>,
    /// Glyphs drawn in both masters with matching structure, which
    /// score a method.
    pub drawn: Vec<String>,
}

/// The debt of one heavier master against one lighter master.
#[derive(Clone, Debug, PartialEq)]
pub struct DebtReport {
    /// The lighter master.
    pub light: SourceId,
    /// The heavier master.
    pub heavy: SourceId,
    /// One group per script that has debt or evidence, in [`Script`] order.
    pub groups: Vec<DebtGroup>,
}

impl DebtReport {
    /// How many glyphs are owed in all.
    pub fn pending_total(&self) -> usize {
        self.groups.iter().map(|group| group.pending.len()).sum()
    }
}

/// A lighter master to draw the active one from, by the weight axis.
///
/// `None` when the active source is the lightest, or when the sources
/// have no axis that reads as weight.
pub fn lighter_master(project: &Project, active: SourceId) -> Option<SourceId> {
    let sources: Vec<_> = project.document_sources().collect();
    let location = sources
        .iter()
        .find(|source| source.id() == active)?
        .location();
    let key = location
        .keys()
        .find(|key| {
            let key = key.to_ascii_lowercase();
            key.contains("weight") || key == "wght"
        })
        .or_else(|| {
            (location.len() == 1)
                .then(|| location.keys().next())
                .flatten()
        })?
        .clone();
    let active_weight = *location.get(&key)?;
    sources
        .iter()
        .filter(|source| source.id() != active)
        .filter_map(|source| {
            let weight = *source.location().get(&key)?;
            (weight < active_weight).then_some((weight, source.id()))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

/// Every drawn glyph that is identical in both masters, with the drawn
/// pairs that score a method, by script.
pub fn report(project: &Project, light: SourceId, heavy: SourceId) -> Result<DebtReport, String> {
    let light_layer = project
        .document_source(light)
        .ok_or("the lighter source does not exist")?
        .default_layer();
    let heavy_layer = project
        .document_source(heavy)
        .ok_or("the heavier source does not exist")?
        .default_layer();
    let mut groups: BTreeMap<Script, DebtGroup> = BTreeMap::new();
    for entry in project.document_source_glyph_entries(heavy)? {
        let name = entry.name();
        let (Some(light_glyph), Some(heavy_glyph)) = (
            project.document_layer(name, &light_layer),
            project.document_layer(name, &heavy_layer),
        ) else {
            continue;
        };
        if !is_drawn(light_glyph) || !is_drawn(heavy_glyph) {
            continue;
        }
        let script = Script::of(name, entry.codepoint());
        let group = groups.entry(script).or_insert_with(|| DebtGroup {
            script,
            pending: Vec::new(),
            drawn: Vec::new(),
        });
        if canonical_outline(light_glyph) == canonical_outline(heavy_glyph) {
            // A frozen copy is deliberate, not owed.
            if !matches!(entry.mark_label(), Some("green" | "purple")) {
                group.pending.push(name.to_owned());
            }
        } else if compatible(light_glyph, heavy_glyph) {
            group.drawn.push(name.to_owned());
        }
    }
    Ok(DebtReport {
        light,
        heavy,
        groups: groups.into_values().collect(),
    })
}

/// A learned offset and how it fares where the answer is known.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OffsetFit {
    /// How far the drawn glyphs moved along their normals.
    pub offset: Offset,
    /// How much the advance grew, on average.
    pub advance_delta: f64,
    /// How many drawn glyphs were scored.
    pub scored: usize,
    /// Mean absolute coordinate error of the offset, in font units.
    pub mean_error: f64,
    /// The same error for shifting every point by the mean move.
    pub baseline_error: f64,
    /// Glyphs where the offset beats that shift.
    pub wins: usize,
}

/// Learn an offset from `drawn` glyphs and score it on the same glyphs.
///
/// The offset has two parameters, so scoring it on what it learned
/// from flatters it little. The baseline shifts every point by the
/// mean move, which is what a method must beat to be worth running.
pub fn fit_offset(
    project: &Project,
    light: SourceId,
    heavy: SourceId,
    drawn: &[String],
) -> Option<OffsetFit> {
    let light_layer = project.document_source(light)?.default_layer();
    let heavy_layer = project.document_source(heavy)?.default_layer();
    let pairs: Vec<_> = drawn
        .iter()
        .filter_map(|name| {
            Some((
                project.document_layer(name, &light_layer)?,
                project.document_layer(name, &heavy_layer)?,
            ))
        })
        .filter(|(light, heavy)| compatible(*light, *heavy))
        .collect();
    let offset = embolden::learn_layer_offset(&pairs)?;
    let advance_delta = embolden::learn_layer_advance_delta(&pairs).unwrap_or(0.0);
    let (mut sum_dx, mut sum_dy, mut count) = (0.0, 0.0, 0_usize);
    for (light, heavy) in &pairs {
        for (a, b) in flat_points(*light).into_iter().zip(flat_points(*heavy)) {
            sum_dx += b.x - a.x;
            sum_dy += b.y - a.y;
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let mean = Point::new(sum_dx / count as f64, sum_dy / count as f64);
    let (mut offset_error, mut baseline_error, mut wins) = (0.0, 0.0, 0_usize);
    for (light, heavy) in &pairs {
        let truth = flat_points(*heavy);
        let predicted = embolden::layer_point_moves(*light, offset, 0.0);
        let shifted: Vec<_> = flat_points(*light)
            .into_iter()
            .map(|p| p + mean.to_vec2())
            .collect();
        let error_of = |guess: &[Point]| {
            guess
                .iter()
                .zip(&truth)
                .map(|(g, t)| ((g.x - t.x).abs() + (g.y - t.y).abs()) / 2.0)
                .sum::<f64>()
                / truth.len().max(1) as f64
        };
        let (o, b) = (error_of(&predicted), error_of(&shifted));
        offset_error += o;
        baseline_error += b;
        wins += usize::from(o < b);
    }
    let scored = pairs.len();
    Some(OffsetFit {
        offset,
        advance_delta,
        scored,
        mean_error: offset_error / scored as f64,
        baseline_error: baseline_error / scored as f64,
        wins,
    })
}

/// Contours and no components: an outline a method can move.
fn is_drawn(layer: LayerView<'_>) -> bool {
    layer.contours().next().is_some() && layer.components().next().is_none()
}

fn canonical_outline(layer: LayerView<'_>) -> Vec<Vec<(Point, crate::font::LayerPointType, bool)>> {
    layer
        .contours()
        .map(|contour| {
            contour
                .points()
                .map(|point| (point.position(), point.point_type(), point.is_smooth()))
                .collect()
        })
        .collect()
}

/// Same contours, same point counts, same point types.
pub fn compatible(first: LayerView<'_>, second: LayerView<'_>) -> bool {
    let (first, second) = (canonical_outline(first), canonical_outline(second));
    first.len() == second.len()
        && first
            .iter()
            .zip(&second)
            .all(|(a, b)| a.len() == b.len() && a.iter().zip(b).all(|(p, q)| p.1 == q.1))
}

fn flat_points(layer: LayerView<'_>) -> Vec<Point> {
    layer
        .contours()
        .flat_map(|contour| contour.points().map(|point| point.position()))
        .collect()
}

/// The default layer of a source, for callers that hold only its id.
pub fn default_layer(project: &Project, source: SourceId) -> Option<LayerId> {
    Some(project.document_source(source)?.default_layer())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fonts;

    fn virtua() -> (Project, SourceId, SourceId) {
        let project = Project::load(&fonts::designspace()).expect("the designspace loads");
        let ids: Vec<_> = project.document_sources().map(|s| s.id()).collect();
        assert_eq!(ids.len(), 2, "Virtua has two masters");
        let heavy = ids
            .iter()
            .copied()
            .find(|id| lighter_master(&project, *id).is_some())
            .expect("one master is heavier");
        let light = lighter_master(&project, heavy).unwrap();
        (project, light, heavy)
    }

    #[test]
    fn scripts_come_from_codepoints_then_names() {
        assert_eq!(Script::of("alef-ar", Some('\u{0627}')), Script::Arabic);
        assert_eq!(Script::of("alef-hb", Some('\u{05D0}')), Script::Hebrew);
        assert_eq!(Script::of("a", Some('a')), Script::Latin);
        assert_eq!(Script::of("acutecomb", Some('\u{0301}')), Script::Marks);
        assert_eq!(Script::of("five", Some('5')), Script::Figures);
        assert_eq!(Script::of("dal-ar.fina", None), Script::Arabic);
        assert_eq!(Script::of("finalkaf-hb", None), Script::Hebrew);
        assert_eq!(Script::of("E_004", None), Script::Other);
    }

    #[test]
    fn the_lightest_master_has_nothing_lighter() {
        let (project, light, heavy) = virtua();
        assert!(lighter_master(&project, light).is_none());
        assert_eq!(lighter_master(&project, heavy), Some(light));
    }

    #[test]
    fn debt_is_what_still_matches_the_lighter_master() {
        let (project, light, heavy) = virtua();
        let report = report(&project, light, heavy).unwrap();
        let light_layer = default_layer(&project, light).unwrap();
        let heavy_layer = default_layer(&project, heavy).unwrap();
        for group in &report.groups {
            for name in &group.pending {
                let a = project.document_layer(name, &light_layer).unwrap();
                let b = project.document_layer(name, &heavy_layer).unwrap();
                assert_eq!(canonical_outline(a), canonical_outline(b), "{name}");
            }
            for name in &group.drawn {
                let a = project.document_layer(name, &light_layer).unwrap();
                let b = project.document_layer(name, &heavy_layer).unwrap();
                assert!(compatible(a, b), "{name}");
                assert_ne!(canonical_outline(a), canonical_outline(b), "{name}");
            }
        }
        // A family mid-way through its second weight owes something and
        // has drawn something.
        assert!(report.pending_total() > 0);
        assert!(report.groups.iter().any(|g| !g.drawn.is_empty()));
    }

    #[test]
    fn a_fitted_offset_reports_its_error_beside_the_baseline() {
        let (project, light, heavy) = virtua();
        let report = report(&project, light, heavy).unwrap();
        let latin = report
            .groups
            .iter()
            .find(|g| g.script == Script::Latin)
            .expect("Latin is drawn");
        let fit = fit_offset(&project, light, heavy, &latin.drawn).expect("Latin pairs fit");
        assert_eq!(fit.scored, latin.drawn.len());
        assert!(fit.offset.x > 0.0, "Bold stems grow: {:?}", fit.offset);
        assert!(fit.mean_error.is_finite() && fit.baseline_error.is_finite());
        assert!(fit.wins <= fit.scored);
    }
}

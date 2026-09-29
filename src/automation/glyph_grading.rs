// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical grading and explicit reference captures for bounded glyph proposals.
//!
//! This policy applies to automated complete-outline replacement, not ordinary manual editing.
//! Grades come from the layer's existing mark label and exact saved UFO palette color.

use std::collections::BTreeSet;

use kurbo::{PathEl, Shape};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::agent_edit::AgentLayerGuard;
use crate::font::LayerView;
use crate::font::edit_batch::canonical_glyph_revision;
use crate::font::model::glyph_metadata::MarkColor;
use crate::font::project::Project;
use crate::font::variable::{GlyphLayerAddress, LayerId, SourceId};
use crate::ui::theme::ufo_rgba_for_label;

// Existing approved Arabic drawings use this saved value, predating the current UFO palette.
const LEGACY_GREEN_UFO_RGBA: &str = "0.09,0.72,0.44,1";

/// Human grade interpreted as an automation permission, never assigned by this helper.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GlyphGrade {
    /// Existing drawing may be wholly replaced.
    Red,
    /// Preserve the useful drawing and use local edits only.
    Orange,
    /// Preserve the drawing and use local edits only.
    Yellow,
    /// Frozen approved reference.
    Green,
    /// Composite requiring diagnosis and repair, not blanket replacement.
    Blue,
    /// Frozen approved composite reference.
    Purple,
    /// Composite requiring smaller edits, not blanket replacement.
    Pink,
    /// Uncolored, malformed or unfamiliar metadata grants no automatic permission.
    Unknown,
}

impl GlyphGrade {
    fn from_label(label: &str) -> Self {
        match label {
            "red" => Self::Red,
            "orange" => Self::Orange,
            "yellow" => Self::Yellow,
            "green" => Self::Green,
            "blue" => Self::Blue,
            "purple" => Self::Purple,
            "pink" => Self::Pink,
            _ => Self::Unknown,
        }
    }

    /// Whether this glyph may serve as an approved frozen reference.
    pub fn is_reference(self) -> bool {
        matches!(self, Self::Green | Self::Purple)
    }

    /// Whether automated complete-outline replacement is permitted.
    pub fn permits_replacement(self) -> bool {
        self == Self::Red
    }
}

/// An explicitly chosen reference and a bounded statement of its design relevance.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GradingReferenceRequest {
    /// Exact canonical layer identity and revision observed when choosing the reference.
    pub guard: AgentLayerGuard,
    /// Concrete Arabic construction or optical feature for which this reference was chosen.
    pub rationale: String,
}

/// Current canonical geometry and grade, including resolved component ink identity.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct GradedLayerSnapshot {
    /// Glyph name in this source.
    pub glyph: String,
    /// Stable glyph identity.
    pub glyph_id: String,
    /// Stable source identity.
    pub source: usize,
    /// Source display name.
    pub source_name: String,
    /// Selected layer name.
    pub layer: String,
    /// Exact canonical layer revision, including metadata.
    pub revision: String,
    /// Current human grade.
    pub grade: GlyphGrade,
    /// Unicode scalars attached to this layer, when any.
    pub codepoints: Vec<u32>,
    /// Exact horizontal advance in font units.
    pub advance: f64,
    /// Resolved ink bounds in font units, absent for empty outlines.
    pub ink_bounds: Option<[f64; 4]>,
    /// Ordered canonical contour count.
    pub contours: usize,
    /// Ordered canonical component count.
    pub components: usize,
    /// Canonical anchor count.
    pub anchors: usize,
    /// Identity of the actual resolved contour and component geometry.
    pub resolved_outline_sha256: String,
}

/// One target and its deliberately selected, current approved references.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct GradingContext {
    /// Canonical document revision from which every layer was measured.
    pub document_revision: u64,
    /// Red replacement target.
    pub target: GradedLayerSnapshot,
    /// Explicit approved references and their caller-provided relevance.
    pub references: Vec<GradedReference>,
}

/// One measured reference with its stated design purpose.
#[derive(Clone, Debug, PartialEq, Serialize, schemars::JsonSchema)]
pub struct GradedReference {
    /// Current source data, not a copied fixture or label supplied by the caller.
    pub layer: GradedLayerSnapshot,
    /// Why this glyph is relevant to the selected Arabic form.
    pub rationale: String,
}

/// Resolve a replacement target and explicit references from one current Regular source.
///
/// The caller chooses reference names and reasons; this function never ranks or selects glyphs.
/// Unknown or conflicting metadata refuses permission rather than using display hue snapping.
pub fn capture_replacement_context(
    project: &Project,
    source: SourceId,
    target: &AgentLayerGuard,
    references: &[GradingReferenceRequest],
) -> Result<GradingContext, String> {
    let source_view = project
        .document_source(source)
        .ok_or("selected source is absent")?;
    if source_view.name() != "Regular" {
        return Err("automatic outline replacement requires the Regular source".into());
    }
    if references.is_empty() || references.len() > 8 {
        return Err("choose 1..=8 explicit approved Arabic references".into());
    }
    let target = snapshot(project, source, target)?;
    if !target.grade.permits_replacement() {
        return Err(format!(
            "target grade {:?} does not permit complete-outline replacement",
            target.grade
        ));
    }
    let mut names = BTreeSet::from([target.glyph.clone()]);
    let references = references
        .iter()
        .map(|reference| {
            let rationale = reference.rationale.trim();
            if rationale.len() < 8 || rationale.len() > 512 {
                return Err("reference rationale must contain 8..=512 UTF-8 bytes".into());
            }
            if !names.insert(reference.guard.glyph.clone()) {
                return Err(
                    "reference glyphs must be distinct from the target and each other".into(),
                );
            }
            let layer = snapshot(project, source, &reference.guard)?;
            if !layer.grade.is_reference() {
                return Err(format!(
                    "reference {} grade {:?} is not approved frozen reference material",
                    layer.glyph, layer.grade
                ));
            }
            if layer.ink_bounds.is_none() {
                return Err(format!(
                    "reference {} has no measurable outline",
                    layer.glyph
                ));
            }
            Ok(GradedReference {
                layer,
                rationale: rationale.to_owned(),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(GradingContext {
        document_revision: project.document_revision(),
        target,
        references,
    })
}

fn grade(layer: LayerView<'_>) -> GlyphGrade {
    let Ok(label) = layer.mark_label() else {
        return GlyphGrade::Unknown;
    };
    let Ok(color) = layer.mark_color() else {
        return GlyphGrade::Unknown;
    };
    match (label, color) {
        (Some(label), Some(color)) => {
            let grade = GlyphGrade::from_label(label);
            if grade != GlyphGrade::Unknown
                && (canonical_color(label) == Some(color)
                    || (grade == GlyphGrade::Green && color == legacy_green_color()))
            {
                grade
            } else {
                GlyphGrade::Unknown
            }
        }
        (None, Some(color)) if color == legacy_green_color() => GlyphGrade::Green,
        (None, Some(color)) => ["red", "orange", "yellow", "green", "blue", "purple", "pink"]
            .into_iter()
            .find(|label| canonical_color(label) == Some(color))
            .map(GlyphGrade::from_label)
            .unwrap_or(GlyphGrade::Unknown),
        _ => GlyphGrade::Unknown,
    }
}

/// Read a layer's existing human grade without changing it or inferring approval from hue alone.
pub fn document_layer_grade(project: &Project, source: SourceId, glyph: &str) -> GlyphGrade {
    let Some(layer) = project
        .document_source(source)
        .and_then(|source| project.document_layer(glyph, &source.default_layer()))
    else {
        return GlyphGrade::Unknown;
    };
    grade(layer)
}

fn legacy_green_color() -> MarkColor {
    MarkColor::parse(LEGACY_GREEN_UFO_RGBA).expect("checked legacy green palette value")
}

fn canonical_color(label: &str) -> Option<MarkColor> {
    ufo_rgba_for_label(label)
        .as_deref()
        .and_then(MarkColor::parse)
}

fn snapshot(
    project: &Project,
    source: SourceId,
    guard: &AgentLayerGuard,
) -> Result<GradedLayerSnapshot, String> {
    let glyph = project
        .document_glyph(&guard.glyph)
        .ok_or("graded glyph is absent")?;
    if glyph.id().to_wire() != guard.glyph_id {
        return Err(format!("graded glyph {} identity changed", guard.glyph));
    }
    let address = GlyphLayerAddress {
        glyph: guard.glyph.clone(),
        layer: LayerId {
            source,
            name: guard.layer.clone(),
        },
    };
    let layer = project
        .document_layer(&address.glyph, &address.layer)
        .ok_or("graded layer is absent")?;
    let revision = canonical_glyph_revision(layer)?;
    if revision != guard.expected_revision {
        return Err(format!("graded layer {} revision changed", guard.glyph));
    }
    let path = project
        .document_layer_path(&address)
        .map_err(|error| format!("graded glyph {} outline: {error}", guard.glyph))?;
    if path.elements().len() > 100_000 {
        return Err("reference outline exceeds measured geometry limit".into());
    }
    let bounds = (!path.is_empty()).then(|| {
        let bounds = path.bounding_box();
        [bounds.x0, bounds.y0, bounds.x1, bounds.y1]
    });
    let mut digest = Sha256::new();
    for element in path.elements() {
        match element {
            PathEl::MoveTo(point) => hash_points(&mut digest, 0, &[*point]),
            PathEl::LineTo(point) => hash_points(&mut digest, 1, &[*point]),
            PathEl::QuadTo(first, last) => hash_points(&mut digest, 2, &[*first, *last]),
            PathEl::CurveTo(first, second, last) => {
                hash_points(&mut digest, 3, &[*first, *second, *last]);
            }
            PathEl::ClosePath => digest.update([4]),
        }
    }
    let source_name = project
        .document_source(source)
        .ok_or("selected source is absent")?
        .name()
        .to_owned();
    Ok(GradedLayerSnapshot {
        glyph: guard.glyph.clone(),
        glyph_id: guard.glyph_id.clone(),
        source: source.0,
        source_name,
        layer: guard.layer.clone(),
        revision,
        grade: grade(layer),
        codepoints: layer.codepoints().map(u32::from).collect(),
        advance: layer.width(),
        ink_bounds: bounds,
        contours: layer.contours().count(),
        components: layer.components().count(),
        anchors: layer.anchors().count(),
        resolved_outline_sha256: format!("sha256:{:x}", digest.finalize()),
    })
}

fn hash_points(digest: &mut Sha256, kind: u8, points: &[kurbo::Point]) {
    digest.update([kind]);
    for point in points {
        digest.update(point.x.to_bits().to_le_bytes());
        digest.update(point.y.to_bits().to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn mark(project: &mut Project, glyph: &str, label: &str) {
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let color = canonical_color(label).unwrap();
        project
            .edit_document_layer(glyph, &layer, |draft| {
                draft.set_mark(Some(label), Some(color))?;
                Ok(())
            })
            .unwrap();
    }

    fn guard(project: &Project, glyph: &str) -> AgentLayerGuard {
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        AgentLayerGuard {
            glyph: glyph.into(),
            glyph_id: project.document_glyph(glyph).unwrap().id().to_wire(),
            layer: layer.name.clone(),
            expected_revision: canonical_glyph_revision(
                project.document_layer(glyph, &layer).unwrap(),
            )
            .unwrap(),
        }
    }

    #[test]
    fn semantic_permissions_use_exact_canonical_grade_metadata() {
        let mut project = Project::new_font(std::env::temp_dir().join("grade-policy.ufo"));
        for label in ["red", "orange", "yellow", "green", "blue", "purple", "pink"] {
            let glyph = format!("test_{label}");
            project.add_document_glyph(&glyph, 400.0, None).unwrap();
            mark(&mut project, &glyph, label);
            if label == "green" {
                let layer = project
                    .document_source(SourceId(0))
                    .unwrap()
                    .default_layer();
                project
                    .edit_document_layer(&glyph, &layer, |draft| {
                        draft.add_shape_contour(kurbo::Rect::new(10.0, 0.0, 80.0, 110.0), false)?;
                        Ok(())
                    })
                    .unwrap();
            }
            let layer = project
                .document_source(SourceId(0))
                .unwrap()
                .default_layer();
            let grade = grade(project.document_layer(&glyph, &layer).unwrap());
            assert_eq!(grade, GlyphGrade::from_label(label));
            assert_eq!(grade.permits_replacement(), label == "red");
            assert_eq!(grade.is_reference(), matches!(label, "green" | "purple"));
        }
        let references = vec![GradingReferenceRequest {
            guard: guard(&project, "test_green"),
            rationale: "Approved Arabic stroke and baseline construction".into(),
        }];
        for label in ["red", "orange", "yellow", "green", "blue", "purple", "pink"] {
            let target = guard(&project, &format!("test_{label}"));
            assert_eq!(
                capture_replacement_context(&project, SourceId(0), &target, &references).is_ok(),
                label == "red"
            );
        }
        project.add_document_glyph("unmarked", 400.0, None).unwrap();
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        assert_eq!(
            grade(project.document_layer("unmarked", &layer).unwrap()),
            GlyphGrade::Unknown
        );
        let green = canonical_color("green").unwrap();
        project
            .edit_document_layer("unmarked", &layer, |draft| {
                draft.set_mark(Some("red"), Some(green))?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            grade(project.document_layer("unmarked", &layer).unwrap()),
            GlyphGrade::Unknown
        );
        assert!(
            capture_replacement_context(
                &project,
                SourceId(0),
                &guard(&project, "unmarked"),
                &references
            )
            .is_err()
        );
        let red = canonical_color("red").unwrap();
        project
            .edit_document_layer("unmarked", &layer, |draft| {
                draft.set_mark(Some("unknown"), Some(red))?;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            grade(project.document_layer("unmarked", &layer).unwrap()),
            GlyphGrade::Unknown
        );
    }

    #[test]
    fn exact_legacy_green_is_frozen_reference_even_without_label() {
        let mut font = norad::Font::new();
        for (name, label, color) in [
            ("red", Some("red"), "1,0,0,1"),
            ("legacy_label", Some("green"), LEGACY_GREEN_UFO_RGBA),
            ("legacy_color_only", None, LEGACY_GREEN_UFO_RGBA),
            ("conflict", Some("red"), LEGACY_GREEN_UFO_RGBA),
        ] {
            let mut glyph = norad::Glyph::new(name);
            glyph.width = 400.0;
            glyph.lib.insert(
                crate::font::model::glyph_metadata::MARK_COLOR_KEY.into(),
                plist::Value::String(color.into()),
            );
            if let Some(label) = label {
                glyph.lib.insert(
                    crate::font::model::glyph_metadata::MARK_LABEL_KEY.into(),
                    plist::Value::String(label.into()),
                );
            }
            if name != "red" {
                glyph.contours.push(norad::Contour::new(
                    [(10.0, 0.0), (80.0, 0.0), (80.0, 110.0), (10.0, 110.0)]
                        .into_iter()
                        .map(|(x, y)| {
                            norad::ContourPoint::new(
                                x,
                                y,
                                norad::PointType::Line,
                                false,
                                None,
                                None,
                            )
                        })
                        .collect(),
                    None,
                ));
            }
            font.default_layer_mut().insert_glyph(glyph);
        }
        let project = Project::from_source(crate::font::project::SourceInput::from_font(
            font,
            std::env::temp_dir().join("legacy-green-policy.ufo"),
        ));
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        let legacy = legacy_green_color();
        for glyph in ["legacy_label", "legacy_color_only"] {
            let layer_view = project.document_layer(glyph, &layer).unwrap();
            assert_eq!(layer_view.mark_color().unwrap(), Some(legacy));
            let actual = grade(layer_view);
            assert_eq!(actual, GlyphGrade::Green);
            assert!(actual.is_reference());
            assert!(!actual.permits_replacement());
            let reference = GradingReferenceRequest {
                guard: guard(&project, glyph),
                rationale: "Approved Arabic joining and stroke reference".into(),
            };
            assert!(
                capture_replacement_context(
                    &project,
                    SourceId(0),
                    &guard(&project, "red"),
                    &[reference]
                )
                .is_ok()
            );
            assert!(
                capture_replacement_context(
                    &project,
                    SourceId(0),
                    &guard(&project, glyph),
                    &[GradingReferenceRequest {
                        guard: guard(
                            &project,
                            if glyph == "legacy_label" {
                                "legacy_color_only"
                            } else {
                                "legacy_label"
                            },
                        ),
                        rationale: "Other approved Arabic stroke reference".into(),
                    }]
                )
                .is_err()
            );
        }
        assert_eq!(
            grade(project.document_layer("conflict", &layer).unwrap()),
            GlyphGrade::Unknown
        );
        let reference = GradingReferenceRequest {
            guard: guard(&project, "conflict"),
            rationale: "Conflicting red label must not become an approved reference".into(),
        };
        assert!(
            capture_replacement_context(
                &project,
                SourceId(0),
                &guard(&project, "red"),
                &[reference]
            )
            .is_err()
        );
    }

    #[test]
    fn illustrative_arabic_fixture_requires_explicit_current_approved_references() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/glyph_workflow/missing_arabic_regular_form.json"
        ))
        .unwrap();
        assert_eq!(fixture["illustrative_only"], true);
        let target_name = fixture["document"]["target"]["glyph"].as_str().unwrap();
        let mut project = Project::new_font(std::env::temp_dir().join("grade-fixture.ufo"));
        project
            .add_document_glyph(target_name, 400.0, None)
            .unwrap();
        mark(&mut project, target_name, "red");
        let mut references = Vec::new();
        for entry in fixture["references"]["green"].as_array().unwrap() {
            let glyph = entry["glyph"].as_str().unwrap();
            project.add_document_glyph(glyph, 400.0, None).unwrap();
            mark(&mut project, glyph, "green");
            let layer = project
                .document_source(SourceId(0))
                .unwrap()
                .default_layer();
            let offset = if references.is_empty() { 20.0 } else { 25.0 };
            project
                .edit_document_layer(glyph, &layer, |draft| {
                    draft.add_shape_contour(
                        kurbo::Rect::new(offset, 0.0, offset + 70.0, 110.0),
                        false,
                    )?;
                    Ok(())
                })
                .unwrap();
            let rationale = entry["roles"]
                .as_array()
                .unwrap()
                .iter()
                .map(|role| role.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(", ");
            references.push(GradingReferenceRequest {
                guard: guard(&project, glyph),
                rationale,
            });
        }
        let protected = &fixture["references"]["protected"][0];
        let purple_glyph = protected["glyph"].as_str().unwrap();
        project
            .add_document_glyph(purple_glyph, 400.0, None)
            .unwrap();
        mark(&mut project, purple_glyph, "purple");
        let layer = project
            .document_source(SourceId(0))
            .unwrap()
            .default_layer();
        project
            .edit_document_layer(purple_glyph, &layer, |draft| {
                draft.add_shape_contour(kurbo::Rect::new(30.0, 0.0, 100.0, 110.0), false)?;
                Ok(())
            })
            .unwrap();
        references.push(GradingReferenceRequest {
            guard: guard(&project, purple_glyph),
            rationale: protected["reason"].as_str().unwrap().into(),
        });
        let target = guard(&project, target_name);
        let context =
            capture_replacement_context(&project, SourceId(0), &target, &references).unwrap();
        assert_eq!(context.target.grade, GlyphGrade::Red);
        assert_eq!(context.references.len(), 3);
        assert_eq!(context.references[0].layer.grade, GlyphGrade::Green);
        assert_eq!(context.references[2].layer.grade, GlyphGrade::Purple);
        assert!(
            context.references[0]
                .layer
                .resolved_outline_sha256
                .starts_with("sha256:")
        );
        assert_eq!(
            context.references[0].layer.ink_bounds,
            Some([20.0, 0.0, 90.0, 110.0])
        );
        assert_ne!(
            context.references[0].layer.resolved_outline_sha256,
            context.references[1].layer.resolved_outline_sha256
        );
        assert!(capture_replacement_context(&project, SourceId(0), &target, &[]).is_err());
        references[0].guard.expected_revision = "stale".into();
        assert!(capture_replacement_context(&project, SourceId(0), &target, &references).is_err());
        let first_reference = references[0].guard.glyph.clone();
        mark(&mut project, &first_reference, "orange");
        references[0].guard = guard(&project, &first_reference);
        assert!(capture_replacement_context(&project, SourceId(0), &target, &references).is_err());
    }
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Validated, revision-checked glyph edits offered as one proposal batch.

use std::collections::HashSet;

use norad::{Font, Glyph};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::document::proposal::{self, ProposalSummary};
use crate::formats::lib_keys::write_proposal_base;

/// Opaque SHA-256 revision of a glyph's canonical GLIF, including its metadata.
/// Returns an error if the glyph cannot be serialized. Re-read after a core upgrade.
pub fn glyph_revision(glyph: &Glyph) -> Result<String, String> {
    let bytes = glyph.encode_xml().map_err(|e| e.to_string())?;
    Ok(format!("glif-sha256:{:x}", Sha256::digest(bytes)))
}

/// A batch starts from the foreground and writes a new, uniquely named proposal.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditBatch {
    /// Unique proposal task name; existing tasks are never overwritten.
    pub task: String,
    /// Human-readable design intent, persisted with every proposed glyph.
    pub reason: String,
    /// Glyphs to edit, each exactly once. Empty batches are rejected.
    pub edits: Vec<GlyphEdit>,
}

/// Ordered edits to one glyph, based on a revision returned by `read_glyph`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GlyphEdit {
    /// Foreground glyph name.
    pub glyph: String,
    /// Revision read before deciding the edit.
    pub expected_revision: String,
    /// Operations applied in order to a private copy.
    pub operations: Vec<Operation>,
}

/// An exact edit in font units. Only `SetOutline` changes contour and point order.
/// Coordinates must be finite. Point indices are zero-based and revision-scoped.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    /// Replace every contour with an explicit drawing in font units. Components are
    /// removed when `clear_components` is true; anchors, encoding, marks and width stay.
    SetOutline {
        /// Complete replacement contours in UFO point order.
        contours: Vec<crate::outline::drawing::DrawingContour>,
        /// Explicitly remove components when replacing a composite with drawn outlines.
        #[serde(default)]
        clear_components: bool,
    },
    /// Set the intended smooth flag on an existing on-curve point.
    SetSmooth {
        /// Contour index.
        contour: usize,
        /// Point index within the contour.
        point: usize,
        /// Intended smoothness; use curve analysis to measure actual continuity.
        smooth: bool,
    },
    /// Change the advance, leaving the outline in place.
    SetWidth {
        /// Nonnegative advance in font units.
        width: f64,
    },
    /// Move one point, including an off-curve control point, to an exact location.
    SetPoint {
        /// Contour index from `read_glyph`.
        contour: usize,
        /// Point index within the contour.
        point: usize,
        /// New x coordinate.
        x: f64,
        /// New y coordinate.
        y: f64,
    },
    /// Translate all outline points, component offsets, and anchors; keep the advance.
    Translate {
        /// Horizontal displacement.
        dx: f64,
        /// Vertical displacement.
        dy: f64,
    },
    /// Move an existing uniquely named anchor, or add it if absent.
    SetAnchor {
        /// Nonempty anchor name, such as `top` or `_top`.
        name: String,
        /// New x coordinate.
        x: f64,
        /// New y coordinate.
        y: f64,
    },
}

fn finite(values: &[f64]) -> Result<(), String> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err("coordinates and widths must be finite".into())
    }
}

fn apply(glyph: &mut Glyph, operation: &Operation) -> Result<(), String> {
    match operation {
        Operation::SetOutline {
            contours,
            clear_components,
        } => {
            glyph.contours = crate::outline::drawing::contours(contours)?;
            if *clear_components {
                glyph.components.clear();
            }
        }
        Operation::SetSmooth {
            contour,
            point,
            smooth,
        } => {
            let target = glyph
                .contours
                .get_mut(*contour)
                .and_then(|c| c.points.get_mut(*point))
                .ok_or("unknown point")?;
            if target.typ == norad::PointType::OffCurve {
                return Err("off-curve point cannot be smooth".into());
            }
            target.smooth = *smooth;
        }
        Operation::SetWidth { width } => {
            finite(&[*width])?;
            if *width < 0.0 {
                return Err("advance must be nonnegative".into());
            }
            glyph.width = *width;
        }
        Operation::SetPoint {
            contour,
            point,
            x,
            y,
        } => {
            finite(&[*x, *y])?;
            let target = glyph
                .contours
                .get_mut(*contour)
                .and_then(|c| c.points.get_mut(*point))
                .ok_or_else(|| format!("no point {contour}:{point}"))?;
            target.x = *x;
            target.y = *y;
        }
        Operation::Translate { dx, dy } => {
            finite(&[*dx, *dy])?;
            for point in glyph.contours.iter_mut().flat_map(|c| &mut c.points) {
                point.x += dx;
                point.y += dy;
                finite(&[point.x, point.y])?;
            }
            for component in &mut glyph.components {
                component.transform.x_offset += dx;
                component.transform.y_offset += dy;
                finite(&[component.transform.x_offset, component.transform.y_offset])?;
            }
            for anchor in &mut glyph.anchors {
                anchor.x += dx;
                anchor.y += dy;
                finite(&[anchor.x, anchor.y])?;
            }
        }
        Operation::SetAnchor { name, x, y } => {
            finite(&[*x, *y])?;
            if name.is_empty() {
                return Err("anchor name must not be empty".into());
            }
            let matches = glyph
                .anchors
                .iter()
                .filter(|a| a.name.as_deref() == Some(name))
                .count();
            if matches > 1 {
                return Err(format!("anchor {name} is ambiguous"));
            }
            if let Some(anchor) = glyph
                .anchors
                .iter_mut()
                .find(|a| a.name.as_deref() == Some(name))
            {
                anchor.x = *x;
                anchor.y = *y;
            } else {
                let name = norad::Name::new(name).map_err(|e| e.to_string())?;
                glyph
                    .anchors
                    .push(norad::Anchor::new(*x, *y, Some(name), None, None));
            }
        }
    }
    Ok(())
}

/// Validate every edit on private glyph copies, then create a proposal layer.
/// Errors leave `font` unchanged. Never edits the foreground or saves files.
/// Existing proposal tasks, duplicate glyphs, stale revisions, and empty edits fail.
pub fn propose(font: &mut Font, batch: &EditBatch) -> Result<ProposalSummary, String> {
    if batch.task.is_empty()
        || !batch
            .task
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
    {
        return Err("task must contain only ASCII letters, digits, hyphens, or underscores".into());
    }
    if batch.reason.trim().is_empty() || batch.edits.is_empty() {
        return Err("reason and edits must not be empty".into());
    }
    if font
        .layers
        .get(&proposal::layer_name(&batch.task))
        .is_some()
    {
        return Err("proposal task already exists; use a new task name".into());
    }
    let mut seen = HashSet::new();
    let mut proposed = Vec::new();
    for edit in &batch.edits {
        if !seen.insert(&edit.glyph) || edit.operations.is_empty() {
            return Err(format!(
                "{}: duplicate glyph or empty operations",
                edit.glyph
            ));
        }
        let original = font
            .get_glyph(&edit.glyph)
            .ok_or_else(|| format!("no glyph named {}", edit.glyph))?;
        if glyph_revision(original)? != edit.expected_revision {
            return Err(format!(
                "{}: stale revision; read the glyph again",
                edit.glyph
            ));
        }
        let mut glyph = original.clone();
        for operation in &edit.operations {
            apply(&mut glyph, operation).map_err(|e| format!("{}: {e}", edit.glyph))?;
        }
        if glyph == *original {
            return Err(format!("{}: operations make no change", edit.glyph));
        }
        write_proposal_base(&mut glyph, &edit.expected_revision, &batch.reason);
        proposed.push(glyph);
    }
    proposal::write(font, &batch.task, proposed).map_err(|e| e.to_string())
}

/// Create a proposal on disk without rewriting foreground GLIFs or font metadata.
/// Validates the complete batch, writes its new layer, then atomically replaces
/// `layercontents.plist`. Rechecks glyph revisions and the layer index before publication.
/// Other applications do not participate in this writer's lock: callers must coordinate
/// external saves. The revision checks do not provide a cross-process filesystem transaction.
pub fn save_proposal(
    source: &std::path::Path,
    batch: &EditBatch,
) -> Result<ProposalSummary, String> {
    use std::fs;
    let lock_path = source.join(".runebender-proposal.lock");
    let lock = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|e| format!("cannot acquire proposal writer lock: {e}"))?;
    let result = (|| -> Result<ProposalSummary, String> {
        let index_path = source.join("layercontents.plist");
        let index_before = fs::read(&index_path).map_err(|e| e.to_string())?;
        let mut font = Font::load(source).map_err(|e| e.to_string())?;
        let summary = propose(&mut font, batch)?;
        let layer = font
            .layers
            .get(&summary.layer)
            .ok_or("proposal layer missing")?;
        let directory = source.join(layer.path());
        fs::create_dir(&directory).map_err(|e| e.to_string())?;
        let index_temp = source.join(".runebender-layercontents.plist");
        let publish = (|| -> Result<(), String> {
            let mut contents = plist::Dictionary::new();
            for glyph in layer.iter() {
                let path = layer.get_path(glyph.name()).ok_or("glyph path missing")?;
                fs::write(
                    directory.join(path),
                    glyph.encode_xml().map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                contents.insert(
                    glyph.name().to_string(),
                    path.to_string_lossy().to_string().into(),
                );
            }
            plist::Value::Dictionary(contents)
                .to_file_xml(directory.join("contents.plist"))
                .map_err(|e| e.to_string())?;
            let layers = font
                .iter_layers()
                .map(|l| {
                    plist::Value::Array(vec![
                        l.name().to_string().into(),
                        l.path().to_string_lossy().to_string().into(),
                    ])
                })
                .collect();
            plist::Value::Array(layers)
                .to_file_xml(&index_temp)
                .map_err(|e| e.to_string())?;
            let latest = Font::load(source).map_err(|e| e.to_string())?;
            for edit in &batch.edits {
                let glyph = latest
                    .get_glyph(&edit.glyph)
                    .ok_or("foreground glyph removed")?;
                if glyph_revision(glyph)? != edit.expected_revision {
                    return Err(format!(
                        "{} changed while preparing the proposal",
                        edit.glyph
                    ));
                }
            }
            if fs::read(&index_path).map_err(|e| e.to_string())? != index_before {
                return Err("layer index changed while preparing the proposal".into());
            }
            fs::rename(&index_temp, &index_path).map_err(|e| e.to_string())
        })();
        if publish.is_err() {
            let _ = fs::remove_dir_all(&directory);
            let _ = fs::remove_file(&index_temp);
        }
        publish?;
        Ok(summary)
    })();
    drop(lock);
    let _ = fs::remove_file(lock_path);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (Font, EditBatch) {
        let mut font = Font::new();
        font.default_layer_mut().insert_glyph(Glyph::new("n"));
        let batch = EditBatch {
            task: "spacing-1".into(),
            reason: "Test spacing".into(),
            edits: vec![GlyphEdit {
                glyph: "n".into(),
                expected_revision: glyph_revision(font.get_glyph("n").unwrap()).unwrap(),
                operations: vec![Operation::SetWidth { width: 600.0 }],
            }],
        };
        (font, batch)
    }

    #[test]
    fn translation_moves_components_and_anchors_and_point_edits_are_exact() {
        let (mut font, mut batch) = fixture();
        let glyph = font.get_glyph_mut("n").unwrap();
        glyph.contours.push(norad::Contour::new(
            vec![norad::ContourPoint::new(
                10.0,
                20.0,
                norad::PointType::Move,
                false,
                None,
                None,
            )],
            None,
        ));
        glyph.components.push(norad::Component::new(
            norad::Name::new("base").unwrap(),
            norad::AffineTransform::default(),
            None,
        ));
        glyph.anchors.push(norad::Anchor::new(
            5.0,
            6.0,
            Some(norad::Name::new("top").unwrap()),
            None,
            None,
        ));
        batch.edits[0].expected_revision = glyph_revision(glyph).unwrap();
        batch.edits[0].operations = vec![
            Operation::Translate { dx: 12.0, dy: 3.0 },
            Operation::SetPoint {
                contour: 0,
                point: 0,
                x: 23.0,
                y: 24.0,
            },
            Operation::SetAnchor {
                name: "bottom".into(),
                x: 0.0,
                y: -10.0,
            },
        ];
        propose(&mut font, &batch).unwrap();
        let proposed = font
            .layers
            .get(&proposal::layer_name(&batch.task))
            .unwrap()
            .get_glyph("n")
            .unwrap();
        assert_eq!(
            (
                proposed.contours[0].points[0].x,
                proposed.contours[0].points[0].y
            ),
            (23.0, 24.0)
        );
        assert_eq!(
            (
                proposed.components[0].transform.x_offset,
                proposed.components[0].transform.y_offset
            ),
            (12.0, 3.0)
        );
        assert_eq!((proposed.anchors[0].x, proposed.anchors[0].y), (17.0, 9.0));
        assert_eq!(proposed.anchors.len(), 2);
        assert_eq!(proposed.width, 0.0);
        assert_eq!(font.get_glyph("n").unwrap().contours[0].points[0].x, 10.0);
    }

    #[test]
    fn batch_is_atomic_and_requires_fresh_reads() {
        let (mut font, mut batch) = fixture();
        batch.edits[0].operations.push(Operation::SetPoint {
            contour: 99,
            point: 0,
            x: 1.0,
            y: 2.0,
        });
        assert!(propose(&mut font, &batch).is_err());
        assert!(proposal::list(&font).is_empty());
        assert_eq!(font.get_glyph("n").unwrap().width, 0.0);
        batch.edits[0].operations.pop();
        font.get_glyph_mut("n").unwrap().width = 5.0;
        assert!(propose(&mut font, &batch).unwrap_err().contains("stale"));
    }

    #[test]
    fn proposal_is_reviewable_and_stale_install_is_skipped() {
        let (mut font, batch) = fixture();
        propose(&mut font, &batch).unwrap();
        assert_eq!(font.get_glyph("n").unwrap().width, 0.0);
        assert!(propose(&mut font, &batch).is_err());
        font.get_glyph_mut("n").unwrap().width = 20.0;
        let installed =
            proposal::install(&mut font, &batch.task, None, true, &mut |_, _| {}).unwrap();
        assert!(installed.installed.is_empty());
        assert!(installed.skipped[0].1.contains("stale"));
        assert_eq!(font.get_glyph("n").unwrap().width, 20.0);
    }

    #[test]
    fn fresh_install_preserves_metadata_and_invalid_numbers_fail() {
        let (mut font, mut batch) = fixture();
        batch.edits[0].operations[0] = Operation::SetWidth { width: f64::NAN };
        assert!(propose(&mut font, &batch).is_err());
        batch.edits[0].operations[0] = Operation::SetWidth { width: 600.0 };
        propose(&mut font, &batch).unwrap();
        let installed =
            proposal::install(&mut font, &batch.task, None, true, &mut |_, _| {}).unwrap();
        assert_eq!(installed.installed, ["n"]);
        assert_eq!(font.get_glyph("n").unwrap().width, 600.0);
        assert!(font.get_glyph("n").unwrap().lib.is_empty());
    }
}

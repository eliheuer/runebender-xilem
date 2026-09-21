// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Standalone UFO proposal-layer serialization.
//!
//! Proposal operations execute on canonical Project layers. This module only decodes a source,
//! encodes the resulting auxiliary glyphs, and publishes the external layer contract without
//! rewriting foreground GLIFs.

use std::fs;
use std::path::Path;

use norad::{Font, Glyph};

use crate::font::edit_batch::{self, EditBatch};
use crate::font::project::{Project, SourceInput};
use crate::font::proposal::{ProposalError, ProposalSummary, layer_name};
use crate::font::variable::{LayerId, SourceId};

fn compatible(foreground: &Glyph, proposed: &Glyph) -> bool {
    foreground
        .contours
        .iter()
        .map(|contour| {
            contour
                .points
                .iter()
                .map(|point| point.typ)
                .collect::<Vec<_>>()
        })
        .eq(proposed.contours.iter().map(|contour| {
            contour
                .points
                .iter()
                .map(|point| point.typ)
                .collect::<Vec<_>>()
        }))
}

fn describe(glyph: &Glyph) -> String {
    let points = glyph
        .contours
        .iter()
        .map(|contour| contour.points.len())
        .sum::<usize>();
    format!("{}c · {points}pt", glyph.contours.len())
}

/// Write detached glyphs into one standalone UFO proposal layer.
///
/// This is a format-boundary fixture and external-tool adapter. Live documents use canonical
/// proposal transactions instead.
pub fn write_proposal_layer(
    font: &mut Font,
    task: &str,
    glyphs: impl IntoIterator<Item = Glyph>,
) -> Result<ProposalSummary, ProposalError> {
    let proposal_name = layer_name(task);
    let layer = font
        .layers
        .get_or_create_layer(&proposal_name)
        .map_err(|error| ProposalError::BadLayerName {
            name: proposal_name.clone(),
            reason: error.to_string(),
        })?;
    for glyph in glyphs {
        layer.insert_glyph(glyph);
    }
    let mut summary = ProposalSummary {
        task: task.to_owned(),
        layer: proposal_name,
        glyphs: Vec::new(),
        compatible: Vec::new(),
        incompatible: Vec::new(),
        missing: Vec::new(),
    };
    let layer = font
        .layers
        .get(&summary.layer)
        .expect("the proposal layer was just created");
    for proposed in layer.iter() {
        let name = proposed.name().to_string();
        summary.glyphs.push(name.clone());
        match font.default_layer().get_glyph(&name) {
            None => summary.missing.push(name),
            Some(foreground) if compatible(foreground, proposed) => {
                summary.compatible.push(name);
            }
            Some(foreground) => summary.incompatible.push((
                name,
                format!(
                    "foreground {} · proposed {}",
                    describe(foreground),
                    describe(proposed)
                ),
            )),
        }
    }
    Ok(summary)
}

/// Create a proposal on disk without rewriting foreground GLIFs or font metadata.
///
/// The complete batch runs through the canonical proposal transaction before this serializer
/// writes its auxiliary GLIFs and atomically publishes the new layer index. Other applications do
/// not participate in this writer's lock, so callers must still coordinate external saves.
pub fn save_proposal(source: &Path, batch: &EditBatch) -> Result<ProposalSummary, String> {
    let lock_path = source.join(".runebender-proposal.lock");
    let lock = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
        .map_err(|error| format!("cannot acquire proposal writer lock: {error}"))?;
    let result = (|| -> Result<ProposalSummary, String> {
        let index_path = source.join("layercontents.plist");
        let index_before = fs::read(&index_path).map_err(|error| error.to_string())?;

        let input = SourceInput::load(source)?;
        let mut project = Project::from_source(input);
        let source_id = SourceId(0);
        let summary = edit_batch::propose_project(&mut project, source_id, batch)?;
        let proposal_layer = LayerId {
            source: source_id,
            name: summary.layer.clone(),
        };

        let mut publication = Font::load(source).map_err(|error| error.to_string())?;
        {
            let layer = publication
                .layers
                .get_or_create_layer(&summary.layer)
                .map_err(|error| error.to_string())?;
            for glyph in &summary.glyphs {
                let encoded = project
                    .encode_ufo_layer(glyph, &proposal_layer)
                    .ok_or_else(|| {
                        format!("proposal glyph {glyph} disappeared before publication")
                    })?;
                layer.insert_glyph(encoded);
            }
        }
        let layer = publication
            .layers
            .get(&summary.layer)
            .expect("the publication layer was just created");

        let directory = source.join(layer.path());
        fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let index_temp = source.join(".runebender-layercontents.plist");
        let publish = (|| -> Result<(), String> {
            let mut contents = plist::Dictionary::new();
            for glyph in layer.iter() {
                let path = layer.get_path(glyph.name()).ok_or("glyph path missing")?;
                fs::write(
                    directory.join(path),
                    glyph.encode_xml().map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
                contents.insert(
                    glyph.name().to_string(),
                    path.to_string_lossy().to_string().into(),
                );
            }
            plist::Value::Dictionary(contents)
                .to_file_xml(directory.join("contents.plist"))
                .map_err(|error| error.to_string())?;
            let layers = publication
                .iter_layers()
                .map(|layer| {
                    plist::Value::Array(vec![
                        layer.name().to_string().into(),
                        layer.path().to_string_lossy().to_string().into(),
                    ])
                })
                .collect();
            plist::Value::Array(layers)
                .to_file_xml(&index_temp)
                .map_err(|error| error.to_string())?;
            let latest = Font::load(source).map_err(|error| error.to_string())?;
            for edit in &batch.edits {
                let glyph = latest
                    .get_glyph(&edit.glyph)
                    .ok_or("foreground glyph removed")?;
                if super::ufo::glyph_revision(glyph)? != edit.expected_revision {
                    return Err(format!(
                        "{} changed while preparing the proposal",
                        edit.glyph
                    ));
                }
            }
            if fs::read(&index_path).map_err(|error| error.to_string())? != index_before {
                return Err("layer index changed while preparing the proposal".into());
            }
            fs::rename(&index_temp, &index_path).map_err(|error| error.to_string())
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

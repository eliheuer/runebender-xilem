// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The piece preview: the proof strip's text set from the labeled pieces of every sample in
//! a neural source, with no model.

use std::hash::{Hash as _, Hasher as _};
use std::sync::Arc;

use runebender::outline::piece_assembly::{Piece, pieces_of_sample};

use crate::application::workspace::Workspace;

/// What the proof strip shows for a neural source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PreviewView {
    /// The open canvas's outline, as for any font.
    #[default]
    Outline,
    /// The typed text assembled from labeled pieces.
    Pieces,
    /// The typed text drawn by the latest trained font.
    Model,
}

/// The pieces of every sample, kept until the labels or the ink change.
#[derive(Default)]
pub(crate) struct PieceCache {
    /// What the pieces were cut from: the document revision and the open canvas's state.
    key: u64,
    pub(crate) pieces: Arc<Vec<Piece>>,
}

impl Workspace {
    /// What the proof strip sets in the piece view: the preview text, else the open sample's
    /// text, else the first sample's.
    pub(crate) fn piece_preview_text(&self) -> String {
        if !self.preview_text.is_empty() {
            return self.preview_text.clone();
        }
        let item = self.session.neural_item();
        self.session
            .selected_sample()
            .map(|(_, sample)| sample.text)
            .or_else(|| item.samples.first().map(|sample| sample.text.clone()))
            .unwrap_or_default()
    }

    /// Cut the pieces again when the labels or the ink have changed since the last time.
    pub(crate) fn refresh_pieces(&mut self) {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.font.project.document_revision().hash(&mut hasher);
        self.session.glyph_name.hash(&mut hasher);
        let item = self.session.neural_item();
        format!("{item:?}").hash(&mut hasher);
        let contours = self.session.label_contours();
        for contour in &contours {
            for element in contour.elements() {
                format!("{element:?}").hash(&mut hasher);
            }
        }
        let key = hasher.finish();
        if self.pieces.key == key && !self.pieces.pieces.is_empty() {
            return;
        }
        let mut pieces = Vec::new();
        let mut next_sample = 0;
        for entry in &self.font.glyphs {
            let live = entry.name == self.session.glyph_name;
            let (outline, item) = if live {
                let mut outline = kurbo::BezPath::new();
                for contour in &contours {
                    outline.extend(contour.elements().iter().copied());
                }
                (outline, item.clone())
            } else {
                let Some(address) = self.font.active_layer_address(&entry.name) else {
                    continue;
                };
                let Some(layer) = self
                    .font
                    .project
                    .document_layer(&entry.name, &address.layer)
                else {
                    continue;
                };
                let Ok(item) = layer.neural_item() else {
                    continue;
                };
                ((*entry.outline).clone(), item)
            };
            for sample in &item.samples {
                pieces.extend(pieces_of_sample(&outline, sample, next_sample));
                next_sample += 1;
            }
        }
        self.pieces = PieceCache {
            key,
            pieces: Arc::new(pieces),
        };
    }
}

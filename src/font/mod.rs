// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The canonical open font and its persistent representation.
//!
//! `project` owns the variable font; `variable` owns glyph-local layers;
//! `source` supplies guarded UFO projections for existing tools.
//! `axis` and `var_model` wrap the private Babelfont/fontdrasil backend;
//! `interpolation` checks and combines each glyph's participating sources.
//! `composites` places components; `filesystem` stages UFO and Designspace I/O;
//! `font_memory` and `new_font` build fonts without a filesystem. `model` keeps the
//! kerning lookup, glyph metadata, and entity ids. `history` is the
//! one undo pile, and `proposal` is how a model or a tool offers an
//! edit the designer can install or discard.

pub mod axis;
mod babelfont;
pub use babelfont::{
    AnchorId, AnchorView, CanonicalLayerSnapshot, ComponentId, ComponentView, ContourId,
    ContourView, CopiedContour, DocumentEditError, DocumentSegmentEndpoint, ImportedContours,
    LayerEditDraft, LayerImage, LayerPointType, LayerShapeView, LayerView, PastedContours, PointId,
    PointView, QuadraticSegmentInsertion,
};
pub use variable::{CanonicalSourceMetadataSnapshot, CanonicalSourceStructureSnapshot};
pub mod canonical_metadata;
pub mod compile;
mod compile_metadata;
pub mod compiled_proof;
pub mod compose;
pub mod composites;
pub mod edit_batch;
pub mod font_memory;
pub mod font_ops;
pub mod history;
mod interpolation;
pub mod model;
pub mod new_font;
pub mod project;
#[cfg(not(target_arch = "wasm32"))]
pub mod proof_jobs;
pub mod proposal;
pub mod source;
mod source_format;
pub(crate) mod ufo_codec;
pub mod var_model;
pub mod variable;

pub mod experiments;
mod filesystem;

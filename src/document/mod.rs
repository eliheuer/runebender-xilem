// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The open font and its family.
//!
//! `project` owns the variable font; `variable` owns glyph-local layers;
//! `source` supplies guarded UFO projections for existing tools.
//! `axis` and `var_model` wrap the private Babelfont/fontdrasil backend;
//! `interpolation` checks and combines each glyph's participating sources.
//! `composites` places components; `filesystem` stages UFO and Designspace I/O;
//! `font_memory` and `new_font` build fonts without a filesystem. `model` keeps the
//! kerning lookup, glyph metadata, and entity ids. `history` is the
//! one undo pile, and `proposal` is how a model or a tool offers an
//! edit the designer can install or discard. `nodes` is a workflow
//! of those tools as boxes and wires.

pub mod agent;
pub mod axis;
mod babelfont;
pub use babelfont::{
    AnchorId, AnchorView, CanonicalLayerSnapshot, ComponentId, ComponentView, ContourId,
    ContourView, CopiedContour, DocumentEditError, DocumentSegmentEndpoint, LayerEditDraft,
    LayerPointType, LayerShapeView, LayerView, PastedContours, PointId, PointView,
    QuadraticSegmentInsertion,
};
pub use variable::{CanonicalSourceMetadataSnapshot, CanonicalSourceStructureSnapshot};
pub mod canonical_metadata;
pub mod compile;
mod compile_metadata;
pub mod compose;
pub mod composites;
pub mod edit_batch;
pub mod font_memory;
pub mod font_ops;
pub mod history;
mod interpolation;
pub mod live;
#[cfg(unix)]
pub mod live_socket;
pub mod model;
pub mod new_font;
pub mod nodes;
pub mod nodes_run;
pub mod project;
pub mod proposal;
pub mod source;
mod source_format;
pub mod var_model;
pub mod variable;

pub mod experiments;
mod filesystem;

/// Connected workflows over the editor's live font versions.
pub mod nodes_live;

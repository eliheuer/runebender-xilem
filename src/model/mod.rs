// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Font data model.
//!
//! `entity_id` and `kerning` live in the shared `runebender-core`
//! crate; they're re-exported here so existing `crate::model::*`
//! paths keep working unchanged. The kurbo-touching modules
//! (`workspace`, `designspace`, `glyph_renderer`) stay local until
//! the xilem-side ecosystem catches up to kurbo 0.13.

pub mod designspace;
pub mod glyph_renderer;
pub mod workspace;

pub use runebender_core::model::{EntityId, entity_id, kerning};
pub use workspace::{read_workspace, write_workspace};

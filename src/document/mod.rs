// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The open font and its family.
//!
//! `project` holds a `Master` and a `Project`; `var_model` interpolates
//! across a designspace; `composites` places components; `font_memory`
//! and `new_font` build fonts without a filesystem. `model` keeps the
//! kerning lookup, glyph metadata, and entity ids. `history` is the
//! one undo pile, and `proposal` is how a model or a tool offers an
//! edit the designer can install or discard. `nodes` is a workflow
//! of those tools as boxes and wires.

pub mod agent;
pub mod compose;
pub mod composites;
pub mod edit_batch;
pub mod font_memory;
pub mod font_ops;
pub mod history;
pub mod live;
#[cfg(unix)]
pub mod live_socket;
pub mod model;
pub mod new_font;
pub mod nodes;
pub mod nodes_run;
pub mod project;
pub mod proposal;
pub mod var_model;

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! User-selectable editing tools and tool-like workflows.
//!
//! This is the first place to look for interaction behavior tied to a named
//! tool. Geometry and font mutations stay in the library: for example,
//! [`metaballs`] handles the editor interaction while
//! `runebender::outline::metaballs` owns the shape operation.

pub(crate) mod chat;
pub(crate) mod local_ai;
pub(crate) mod metaballs;
pub(crate) mod nodes;
#[allow(
    dead_code,
    reason = "central Workspace and proof-queue integration consumes this focused adapter"
)]
pub(crate) mod nodes_execution;
pub(crate) mod scripts;
pub(crate) mod text;

#[cfg(unix)]
pub(crate) mod nodes_workspace;

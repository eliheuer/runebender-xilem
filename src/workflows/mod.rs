// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Saved and live node-graph workflows.
//!
//! Workflows describe and run typed operations over a font.
//! They do not own the canonical font or application session.

pub mod nodes;
/// Connected workflows over the editor's isolated font versions.
pub mod nodes_live;
pub mod nodes_run;
pub mod nodes_session;

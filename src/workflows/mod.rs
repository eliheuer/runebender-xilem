// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Saved and live node-graph workflows.
//!
//! Workflows describe and run typed operations over a font.
//! They do not own the canonical font or application session.

/// Bounded requests to an externally managed local chat server.
pub mod local_chat;
/// Detached invocation of the installed sketch-to-outline model.
pub mod local_sketch;
pub mod nodes;
/// Connected workflows over the editor's isolated font versions.
pub mod nodes_live;
pub mod nodes_run;
pub mod nodes_session;
/// Bounded native subprocess execution shared by worker adapters.
pub mod process;

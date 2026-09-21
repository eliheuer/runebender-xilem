// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Guarded operations for language models, scripts, and live editor connections.
//!
//! This module defines automation contracts and bounded operation state.
//! It does not own font data, persistence, or application state.

pub mod agent;
pub mod agent_cancellation;
pub mod agent_edit;
pub mod agent_nodes;
pub mod agent_proof;
pub mod agent_session;
pub mod live;
#[cfg(unix)]
pub mod live_socket;
pub mod script_recipe;

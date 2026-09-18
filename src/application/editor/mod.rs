// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Editing interaction: commands, sessions, inspectors, and tools.
//!
//! Put a user-selectable editing tool in [`tools`]. Put behavior shared by
//! several tools in this module beside the session or command machinery it
//! extends.

pub(crate) mod commands;
pub(crate) mod inspector;
pub(crate) mod session;
pub(crate) mod sidebar;
pub(crate) mod tools;

// These aliases preserve concise call sites (`edit::text_tool`, for example)
// while the filesystem groups every user-selectable tool in one place.
pub(crate) use tools::text as text_tool;
pub(crate) use tools::{chat, local_ai, metaballs, nodes};

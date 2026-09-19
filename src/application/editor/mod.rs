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
pub(crate) mod sources;
pub(crate) mod tools;

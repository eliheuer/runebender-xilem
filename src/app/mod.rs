// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The Xilem application around Runebender's font engine.
//!
//! The package's reusable font behavior lives in the library modules rooted at
//! `src/lib.rs`. This module owns runtime concerns: application state, editing
//! interactions, platform services, views, and the small widgets those views
//! need. See `ARCHITECTURE.md` for the dependency map and change routing guide.

pub(crate) mod actions;
#[cfg(target_arch = "wasm32")]
pub(crate) mod browser;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod cli;
pub(crate) mod editor;
pub(crate) mod font_model;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) mod launch;
pub(crate) mod platform;
pub(crate) mod view;
pub(crate) mod widgets;
pub(crate) mod workspace;

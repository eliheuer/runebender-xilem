// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Editing model and interaction.
//!
//! `edit_types`, `selection`, and `undo` live in the shared
//! `runebender-core` crate; they're re-exported here so existing
//! `crate::editing::*` paths keep working unchanged. The
//! kurbo-touching modules (mouse, hit_test, viewport,
//! background_image, etc.) stay local until the xilem-side
//! ecosystem catches up to kurbo 0.13.

pub mod background_image;
pub mod compat;
pub mod hit_test;
pub mod mouse;
pub mod session;
pub mod quiver;
pub mod tracing;
pub mod viewport;

// `selection` re-exported as a module because the editing/session
// code uses `crate::editing::selection::Selection` (deep path).
// `edit_types` and `undo` aren't deep-pathed anywhere, so just the
// types are re-exported.
pub use runebender_core::editing::selection;
pub use runebender_core::editing::{EditType, Selection, UndoState};

pub use background_image::BackgroundImage;
pub use mouse::{Drag, Modifiers, Mouse, MouseButton, MouseDelegate, MouseEvent};
pub use session::{EditSession, FontMetrics};

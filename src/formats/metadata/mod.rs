// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Codecs for metadata stored at a file-format boundary.
//!
//! These modules interpret UFO lib keys and other serialized values.
//! They do not own the canonical font model or the editor's behavior.

pub mod lib_keys;
pub mod mark_color;
pub mod metaballs;
pub mod metrics_keys;

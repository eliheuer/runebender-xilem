// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Everything that changes a shape.
//!
//! Point and segment edits, the knife, primitives, cleanup, effects,
//! curve conversion, emboldening, and the norad-to-kurbo path builder.
//! The segment maths (cubic, quadratic, hyperbezier) is in `path`.

pub mod cleanup;
pub mod component_ops;
pub mod convert;
pub mod drawing;
pub mod effects;
pub mod embolden;
pub mod glyph_ops;
pub mod glyph_paths;
pub mod knife;
pub mod path;
pub mod point_ops;
pub mod segment_ops;

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Everything that reads a font to answer a question.
//!
//! Measurement, optical weight, spacing against the family's grid,
//! curvature, Unicode categories, and the glyph search language.
//! Nothing here changes a font.

pub mod category;
pub mod curve;
pub mod measure;
pub mod optical;
pub mod search;
pub mod spacing;

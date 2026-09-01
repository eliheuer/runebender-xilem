// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Everything that reads a font to answer a question.
//!
//! Measurement, curvature, Unicode categories, and the glyph search
//! language.
//! Nothing here changes a font.

pub mod category;
pub mod curve;
pub mod measure;
pub mod search;

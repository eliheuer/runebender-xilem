// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Browser embedding exports for the same upstream Xilem/Masonry widgets.

pub use xilem_masonry::*;
pub use masonry::{kurbo, peniko, dpi, palette};
pub use masonry::parley::Alignment as TextAlign;
pub use masonry::parley::style::FontWeight;
pub use masonry::peniko::{Blob, Color, ImageBrush, ImageFormat};
pub use masonry::widgets::InsertNewline;

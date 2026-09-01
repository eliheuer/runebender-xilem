// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A small helper to draw a shaped text label into a scene, reusing
//! Masonry's `render_text` and Parley. Used for measure labels and
//! grid cell labels.
//!
//! xix note: this is exactly the "draw text into a canvas scene" the
//! framework should provide directly. Every canvas app needs it and
//! there is no one-liner today, so each app carries this file.

use std::cell::RefCell;

use masonry::core::render_text;
use masonry::kurbo::{Affine, Point};
use masonry::parley::{FontContext, Layout, LayoutContext, StyleProperty};
use masonry::peniko::Brush;
use xilem::Color;

use masonry::core::BrushIndex;

thread_local! {
    static FONT_CX: RefCell<FontContext> = RefCell::new(FontContext::new());
    static LAYOUT_CX: RefCell<LayoutContext<BrushIndex>> = RefCell::new(LayoutContext::new());
}

/// Horizontal anchor for a drawn label.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Anchor {
    Start,
    Middle,
    End,
}

/// Draw `text` at screen point `at`, `size` px, in `color`.
/// `anchor` positions the text horizontally; it is vertically centered.
pub(crate) fn draw(
    painter: &mut masonry::imaging::Painter<'_>,
    at: Point,
    text: &str,
    size: f32,
    color: Color,
    anchor: Anchor,
) {
    FONT_CX.with(|font_cx| {
        LAYOUT_CX.with(|layout_cx| {
            let mut font_cx = font_cx.borrow_mut();
            let mut layout_cx = layout_cx.borrow_mut();
            let mut builder = layout_cx.ranged_builder(&mut font_cx, text, 1.0, true);
            builder.push_default(StyleProperty::FontSize(size));
            builder.push_default(StyleProperty::Brush(BrushIndex(0)));
            let mut layout: Layout<BrushIndex> = builder.build(text);
            layout.break_all_lines(None);
            let w = layout.width() as f64;
            let h = layout.height() as f64;
            let x = match anchor {
                Anchor::Start => at.x,
                Anchor::Middle => at.x - w / 2.0,
                Anchor::End => at.x - w,
            };
            let y = at.y - h / 2.0;
            let brush: Brush = color.into();
            render_text(painter, Affine::translate((x, y)), &layout, &[brush], false);
        });
    });
}

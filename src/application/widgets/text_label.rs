// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! A small helper to draw a shaped text label into a scene, reusing
//! Masonry's `render_text` and Parley. Used for measure labels and
//! grid cell labels.
//!
//! This stays local while Masonry has no one-call equivalent for shaped
//! canvas labels.

use std::cell::RefCell;

use masonry::core::render_text;
use masonry::kurbo::{Affine, Point};
use masonry::parley::{FontContext, Layout, LayoutContext, StyleProperty};
use masonry::peniko::Brush;
use xilem::Color;

use masonry::core::BrushIndex;

thread_local! {
    static FONT_CX: RefCell<FontContext> = RefCell::new({
        let mut cx = FontContext::new();
        cx.collection
            .register_fonts(xilem::Blob::new(std::sync::Arc::new(crate::UI_FONT)), None);
        cx
    });
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
            builder.push_default(masonry::parley::FontFamily::named(crate::UI_FONT_FAMILY));
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

/// Measure a shaped UI label in logical pixels.
pub(crate) fn width(text: &str, size: f32) -> f64 {
    FONT_CX.with(|font_cx| {
        LAYOUT_CX.with(|layout_cx| {
            let mut fonts = font_cx.borrow_mut();
            let mut layouts = layout_cx.borrow_mut();
            let mut builder = layouts.ranged_builder(&mut fonts, text, 1.0, true);
            builder.push_default(StyleProperty::FontSize(size));
            builder.push_default(masonry::parley::FontFamily::named(crate::UI_FONT_FAMILY));
            let mut layout: Layout<BrushIndex> = builder.build(text);
            layout.break_all_lines(None);
            f64::from(layout.width())
        })
    })
}

/// Draw a single-line label, with an ellipsis if its measured width exceeds `available`.
pub(crate) fn draw_elided(
    painter: &mut masonry::imaging::Painter<'_>,
    at: Point,
    text: &str,
    size: f32,
    color: Color,
    anchor: Anchor,
    available: f64,
) {
    if width(text, size) <= available {
        return draw(painter, at, text, size, color, anchor);
    }
    if width("…", size) > available {
        return;
    }
    let mut end = text.len();
    loop {
        let candidate = format!("{}…", &text[..end]);
        if width(&candidate, size) <= available {
            return draw(painter, at, &candidate, size, color, anchor);
        }
        end = text[..end]
            .char_indices()
            .next_back()
            .map_or(0, |(index, _)| index);
    }
}

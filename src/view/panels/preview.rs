// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The preview strip and the glyph preview.

use crate::*;

pub(crate) fn preview_strip(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Point, Size};
    // When the axis sliders are off a master, preview the interpolated
    // instance (in warm amber) so the strip reflects the current location.
    let interp = app.interp_preview();
    let outline = match &interp {
        Some(o) => o.clone(),
        None => app.session.outline_arc(),
    };
    let components = app.session.components_arc();
    let has_components = interp.is_none() && !components.elements().is_empty();
    let m = app.session.metrics;
    let advance = app.session.advance();
    // The preview is drawn in the status yellow, as the GPUI build draws
    // it: the strip is a reading of the letter, not another copy of the
    // canvas, and the colour is what tells you so at a glance.
    let fill = app.palette.role("warning");
    canvas(move |_app: &mut Workspace, _ctx, scene, size: Size| {
        let mut p = Painter::new(scene);
        // Fit the em box (advance wide, ascender..descender tall) into the strip.
        let margin = 16.0;
        let em_w = advance.max(m.upm * 0.5);
        let em_h = m.ascender - m.descender;
        let scale = ((size.width - margin * 2.0) / em_w).min((size.height - margin * 2.0) / em_h);
        let baseline_y = margin + (m.ascender / em_h) * (size.height - margin * 2.0);
        let x0 = (size.width - em_w * scale) / 2.0;
        let t = Affine::new([scale, 0.0, 0.0, -scale, x0, baseline_y]);
        let _ = Point::ORIGIN;
        p.fill(&(t * (*outline).clone()), fill).draw();
        if has_components {
            p.fill(&(t * (*components).clone()), fill).draw();
        }
    })
}

/// A large preview of the selected glyph, at the foot of the inspector in
/// overview mode (gpui's glyph preview panel). The grid cell is small; this
/// is where you look at the shape.
pub(crate) fn glyph_preview(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Rect, Shape, Size, Stroke};
    let entry = app.selected.and_then(|i| app.font.glyphs.get(i))?;
    let outline = entry.outline.clone();
    let advance = entry.advance;
    let (asc, desc) = (app.font.ascender(), app.font.descender());
    let fill = app.palette.text;
    let line = app.palette.role("gridBorder").with_alpha(0.5);
    Some(
        sized_box(canvas(
            move |_app: &mut Workspace, _ctx, scene, size: Size| {
                let mut p = Painter::new(scene);
                let margin = 18.0;
                let em_w = advance.max(1.0);
                let em_h = (asc - desc).max(1.0);
                let scale =
                    ((size.width - margin * 2.0) / em_w).min((size.height - margin * 2.0) / em_h);
                let ox = (size.width - em_w * scale) / 2.0;
                let baseline = (size.height + em_h * scale) / 2.0 + desc * scale;
                let t = Affine::new([scale, 0.0, 0.0, -scale, ox, baseline]);
                // The advance box, so the preview reads as metrics, not art.
                let box_path = t * Rect::new(0.0, desc, em_w, asc).to_path(0.1);
                p.stroke(&box_path, &Stroke::new(1.0), line).draw();
                p.fill(&(t * (*outline).clone()), fill).draw();
            },
        ))
        .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(170.0)))),
    )
}

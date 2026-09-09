// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The preview strip and the glyph preview.

use crate::*;

pub(crate) fn preview_strip(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Shape, Size};
    // Off-master preview remains the interpolated current glyph.
    let interp = app.interp_preview();
    let outline = match &interp {
        Some(o) => o.clone(),
        None => app.session.outline_arc(),
    };
    let components = app.session.components_arc();
    let has_components = interp.is_none() && !components.elements().is_empty();
    // The preview is type, so it takes the text colour, as the GPUI
    // build draws it: ink on the panel, no hue.
    let fill = if app.preview_invert {
        app.palette.selected_ink()
    } else {
        app.palette.text
    };
    let background = if app.preview_invert {
        app.palette.selected_bg()
    } else {
        app.palette.panel
    };
    let blur = app.preview_blur;
    let instance_preview = interp.is_some();
    let has_preview_text = !app.preview_text.is_empty() && !instance_preview;
    let preview_paths = if !has_preview_text {
        let mut paths = vec![(*outline).clone()];
        if has_components {
            paths.push((*components).clone());
        }
        paths
    } else {
        let inputs = text_tool::TextInputs::new(&app.font)
            .with_direction(app.text_dir)
            .with_text(&app.preview_text);
        text_tool::TextState::new(&inputs)
            .placed()
            .into_iter()
            .map(|sort| sort.path)
            .collect::<Vec<_>>()
    };
    let preview_bounds = preview_paths
        .iter()
        .map(|path| path.bounding_box())
        .reduce(|bounds, next| bounds.union(next));
    let drawing = canvas(move |_app: &mut Workspace, _ctx, scene, size: Size| {
        let mut p = Painter::new(scene);
        if let Some(bounds) = preview_bounds {
            let padding = Space::Md.px();
            let scale = ((size.width - padding * 2.0) / bounds.width().max(1.0))
                .min((size.height - padding * 2.0) / bounds.height().max(1.0))
                .max(0.0);
            let t = Affine::translate(-bounds.center().to_vec2())
                .then_scale_non_uniform(scale, -scale)
                .then_translate((size.width / 2.0, size.height / 2.0).into());
            if blur > 0.0 {
                let raster_scale = (4096.0 / size.width.max(1.0))
                    .min(4096.0 / size.height.max(1.0))
                    .min(1.0);
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "bounded positive preview raster dimensions"
                )]
                let pixels = (
                    (size.width * raster_scale).ceil().clamp(1.0, 4096.0) as u32,
                    (size.height * raster_scale).ceil().clamp(1.0, 4096.0) as u32,
                );
                if let Some(image) = preview_blur::render(
                    &preview_paths,
                    t.then_scale(raster_scale),
                    pixels,
                    blur * raster_scale,
                    fill,
                ) {
                    p.draw_image(&image, Affine::scale(1.0 / raster_scale));
                    return;
                }
            }
            for path in &preview_paths {
                p.fill(&(t * path), fill).draw();
            }
        }
    });
    flex_col((
        drawing.background_color(background).flex(1.0),
        xrow(
            Region::Inline,
            (
                instance_preview.then(|| {
                    label("Instance glyph preview")
                        .text_size(TextSize::Body.px())
                        .color(app.palette.text_muted)
                }),
                (!instance_preview).then(|| {
                    recipes::field_bare(
                        &app.palette,
                        "Preview text",
                        app.preview_text.clone(),
                        |app: &mut Workspace, value| app.preview_text = value,
                        |app: &mut Workspace, value| app.preview_text = value,
                    )
                    .flex(1.0)
                }),
                recipes::toggle(
                    &app.palette,
                    "Invert".into(),
                    app.preview_invert,
                    |app: &mut Workspace| app.preview_invert = !app.preview_invert,
                ),
                direction_chips(app),
                label("Blur")
                    .text_size(TextSize::Body.px())
                    .color(app.palette.text_muted),
                slider(0.0, 8.0, app.preview_blur, |app: &mut Workspace, value| {
                    app.preview_blur = value;
                })
                .width(Length::px(96.0)),
            ),
        )
        .padding(Space::Sm),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(Space::None)
}

/// A large preview of the selected glyph, at the foot of the inspector in
/// overview mode (gpui's glyph preview panel). The grid cell is small; this
/// is where you look at the shape.
pub(crate) fn glyph_preview(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Circle, Line, Point, Rect, Shape, Size, Stroke};
    let entry = app.selected.and_then(|i| app.font.glyphs.get(i))?;
    let outline = entry.outline.clone();
    let contours = app.font.font().get_glyph(&entry.name)?.contours.clone();
    let pal = app.palette.clone();
    let bounds = outline.bounding_box();
    Some(
        sized_box(canvas(
            move |_app: &mut Workspace, _ctx, scene, size: Size| {
                let mut p = Painter::new(scene);
                let margin = Space::Xl.px();
                let scale = ((size.width - margin * 2.0) / bounds.width().max(1.0))
                    .min((size.height - margin * 2.0) / bounds.height().max(1.0))
                    .max(0.0);
                let t = Affine::translate(-bounds.center().to_vec2())
                    .then_scale_non_uniform(scale, -scale)
                    .then_translate((size.width / 2.0, size.height / 2.0).into());
                let stroke = Stroke::new(1.0);
                p.stroke(&(t * (*outline).clone()), &stroke, pal.role("pathStroke"))
                    .draw();
                for contour in &contours {
                    let n = contour.points.len();
                    for (i, point) in contour.points.iter().enumerate() {
                        if point.typ != norad::PointType::OffCurve {
                            continue;
                        }
                        let off = t * Point::new(point.x, point.y);
                        for j in [(i + n - 1) % n, (i + 1) % n] {
                            let on = &contour.points[j];
                            if on.typ != norad::PointType::OffCurve {
                                p.stroke(
                                    Line::new(off, t * Point::new(on.x, on.y)),
                                    &stroke,
                                    pal.role("pointOffcurve").with_alpha(0.7),
                                )
                                .draw();
                            }
                        }
                    }
                    for point in &contour.points {
                        let at = t * Point::new(point.x, point.y);
                        let off = point.typ == norad::PointType::OffCurve;
                        let hue = pal.role(if off {
                            "pointOffcurve"
                        } else if point.smooth {
                            "pointSmooth"
                        } else {
                            "pointCorner"
                        });
                        let (fill, border) = if pal.points_filled {
                            (hue, pal.point_outline.unwrap_or(pal.text))
                        } else {
                            (pal.panel, hue)
                        };
                        let radius = ControlSize::Dot.px() * 0.4;
                        if off || point.smooth {
                            let shape = Circle::new(at, radius);
                            p.fill(shape, fill).draw();
                            p.stroke(shape, &stroke, border).draw();
                        } else {
                            let shape = Rect::new(
                                at.x - radius,
                                at.y - radius,
                                at.x + radius,
                                at.y + radius,
                            );
                            p.fill(shape, fill).draw();
                            p.stroke(shape, &stroke, border).draw();
                        }
                    }
                }
            },
        ))
        .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(360.0)))),
    )
}

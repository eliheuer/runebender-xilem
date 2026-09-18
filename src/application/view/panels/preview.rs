// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The preview strip and the glyph preview.

use crate::application::editor::tools::text as text_tool;
use crate::application::view::design;
use crate::application::view::design::{ControlSize, Space};
use crate::application::widgets::preview_blur;
use crate::application::workspace::Workspace;
use masonry::layout::Dim;
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{canvas, sized_box};

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
    // The proof is type, so it takes the editor's quiet neutral ink on the
    // panel rather than the structural keyline or a semantic hue.
    let fill = if app.preview_invert {
        app.palette.selected_ink()
    } else {
        app.palette.editor_ink()
    };
    let background = if app.preview_invert {
        app.palette.selected_bg()
    } else {
        app.palette.panel
    };
    let blur = app.preview_blur;
    let instance_preview = interp.is_some();
    // The proof strip follows the open text composition even while an outline
    // tool edits one sort in context. Tool choice and composition lifetime are
    // separate state in both GPUI and Web.
    let proof_text = if app.has_text_session {
        &app.initial_text
    } else {
        &app.preview_text
    };
    let has_preview_text = !proof_text.is_empty() && !instance_preview;
    let (preview_paths, advance) = if !has_preview_text {
        let mut paths = vec![(*outline).clone()];
        if has_components {
            paths.push((*components).clone());
        }
        (paths, app.session.advance())
    } else {
        let inputs = text_tool::TextInputs::new(&app.font)
            .with_direction(app.text_dir)
            .with_text(proof_text)
            .with_shaping_options(
                &app.text_features_disabled,
                app.text_script.as_deref(),
                app.text_language.as_deref(),
            );
        let placed = text_tool::TextState::new(&inputs).placed();
        let advance = placed
            .iter()
            .map(|sort| sort.origin.x + sort.advance)
            .fold(0.0, f64::max);
        (
            placed.into_iter().map(|sort| sort.path).collect::<Vec<_>>(),
            advance,
        )
    };
    let preview_bounds = preview_paths
        .iter()
        .filter(|path| !path.elements().is_empty())
        .map(|path| path.bounding_box())
        .reduce(|bounds, next| bounds.union(next));
    let drawing = canvas(move |_app: &mut Workspace, _ctx, scene, size: Size| {
        let mut p = Painter::new(scene);
        if let Some(bounds) = preview_bounds {
            let t = proof_transform(bounds, advance, size);
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
    drawing.background_color(background)
}

/// Fit the advance horizontally and the actual ink vertically, as the GPUI
/// proof does. Sidebearings stay meaningful; empty descender space does not
/// displace the visible ink from the middle of the pane.
fn proof_transform(bounds: kurbo::Rect, advance: f64, size: kurbo::Size) -> kurbo::Affine {
    use masonry::kurbo::Affine;
    let padding = Space::Xl.px();
    let by_height = (size.height - padding * 2.0).max(0.0) / bounds.height().max(1.0);
    let by_width = if advance > 0.0 {
        (size.width - padding * 2.0).max(0.0) / advance
    } else {
        by_height
    };
    let scale = by_height.min(by_width);
    Affine::scale_non_uniform(scale, -scale).then_translate(
        (
            (size.width - advance * scale) / 2.0,
            size.height / 2.0 + bounds.center().y * scale,
        )
            .into(),
    )
}

/// A large preview of the selected glyph, at the foot of the inspector in
/// overview mode. The grid cell is small; this
/// is where you look at the shape.
pub(crate) fn glyph_preview(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Circle, Line, Point, Rect, Shape, Size, Stroke};
    let data = app.selected.and_then(|i| {
        let entry = app.font.glyphs.get(i)?;
        let contours = app.font.font().get_glyph(&entry.name)?.contours.clone();
        Some((entry.outline.clone(), contours))
    });
    let pal = app.palette.clone();
    let background = pal.canvas;
    sized_box(canvas(
        move |_app: &mut Workspace, _ctx, scene, size: Size| {
            let mut p = Painter::new(scene);
            // Own every pixel of the preview allocation. Relying only on the
            // wrapper background left the splitter's top edge on the panel
            // surface, which read as a second-colour strip above the glyph.
            p.fill_rect(size.to_rect(), background);
            let Some((outline, contours)) = &data else {
                return;
            };
            let bounds = outline.bounding_box();
            let scale = (size.width * design::OVERVIEW_GLYPH_PREVIEW_FILL
                / bounds.width().max(1.0))
            .min(size.height * design::OVERVIEW_GLYPH_PREVIEW_FILL / bounds.height().max(1.0))
            .max(0.0);
            let t = Affine::translate(-bounds.center().to_vec2())
                .then_scale_non_uniform(scale, -scale)
                .then_translate((size.width / 2.0, size.height / 2.0).into());
            let stroke = Stroke::new(1.0);
            p.stroke(&(t * (**outline).clone()), &stroke, pal.role("pathStroke"))
                .draw();
            for contour in contours {
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
                        (pal.canvas, hue)
                    };
                    let radius = ControlSize::Dot.px() * 0.4;
                    if off || point.smooth {
                        let shape = Circle::new(at, radius);
                        p.fill(shape, fill).draw();
                        p.stroke(shape, &stroke, border).draw();
                    } else {
                        let shape =
                            Rect::new(at.x - radius, at.y - radius, at.x + radius, at.y + radius);
                        p.fill(shape, fill).draw();
                        p.stroke(shape, &stroke, border).draw();
                    }
                }
            }
        },
    ))
    .background_color(background)
    // Its enclosing inspector split supplies the height. The canvas itself
    // must fill that allocation so no panel-colored tail can appear below.
    .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
}

#[cfg(test)]
mod proof_tests {
    use super::*;
    use masonry::kurbo::{Point, Rect, Size};

    #[test]
    fn proof_centers_ink_vertically_and_preserves_sidebearings() {
        let t = proof_transform(
            Rect::new(80.0, 0.0, 608.0, 760.0),
            668.0,
            Size::new(786.0, 140.0),
        );
        let top = t * Point::new(80.0, 760.0);
        let bottom = t * Point::new(608.0, 0.0);
        assert!((top.y - 16.0).abs() < 1e-9);
        assert!((bottom.y - 124.0).abs() < 1e-9);
        let advance_center = t * Point::new(334.0, 380.0);
        assert_eq!(advance_center, Point::new(393.0, 70.0));
        assert!((top.x + bottom.x) / 2.0 > advance_center.x);
    }

    #[test]
    fn long_proof_fits_the_width_and_small_panes_do_not_invert_it() {
        let t = proof_transform(
            Rect::new(0.0, -200.0, 4000.0, 800.0),
            4000.0,
            Size::new(200.0, 140.0),
        );
        let left = t * Point::new(0.0, 300.0);
        let right = t * Point::new(4000.0, 300.0);
        assert_eq!(left, Point::new(16.0, 70.0));
        assert_eq!(right, Point::new(184.0, 70.0));
        let tiny = proof_transform(Rect::ZERO, 0.0, Size::new(20.0, 20.0));
        assert_eq!(tiny * Point::new(10.0, 10.0), Point::new(10.0, 10.0));
    }
}

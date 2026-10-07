// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The preview strip and the glyph preview.

use crate::application::editor::tools::text as text_tool;
use crate::application::view::design;
use crate::application::view::design::Space;
use crate::application::widgets::preview_blur;
use crate::application::workspace::Workspace;
use masonry::layout::Dim;
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{canvas, sized_box};

pub(crate) fn preview_strip(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use crate::application::pieces::PreviewView;
    use xilem::core::one_of::OneOf3;
    if app.font.project.is_neural() {
        match app.preview_view {
            PreviewView::Pieces => return OneOf3::A(piece_strip(app)),
            PreviewView::Model => return OneOf3::B(model_strip_view(app)),
            PreviewView::Outline => {}
        }
    }
    OneOf3::C(outline_strip(app))
}

/// The text drawn by the trained font, with a draggable node at every caret.
fn model_strip_view(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use crate::application::widgets::model_strip::{StripInks, model_strip};
    let pal = &app.palette;
    let (ink, background) = if app.preview_invert {
        (pal.selected_ink(), pal.selected_bg())
    } else {
        (pal.proof_ink(), pal.proof_strip)
    };
    // The names and roles of docs/VIEWER.md section 3.
    model_strip(
        app.model.render.clone(),
        StripInks {
            ground: background,
            ink,
            strand: pal.role("danger"),
            active: pal.role("selection"),
            ring: pal.role("pointSelected"),
            cloud: pal.role("previewFill").with_alpha(0.22),
        },
        |app: &mut Workspace, event| app.model_strip_event(event),
    )
}

/// The typed text set from the labeled pieces of every sample; a box stands for a letter no
/// sample has.
fn piece_strip(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Shape, Size, Stroke};
    use runebender::outline::piece_assembly::assemble;
    let fill = if app.preview_invert {
        app.palette.selected_ink()
    } else {
        app.palette.proof_ink()
    };
    let background = if app.preview_invert {
        app.palette.selected_bg()
    } else {
        app.palette.proof_strip
    };
    let missing = app.palette.text_muted;
    let placed = assemble(&app.piece_preview_text(), &app.pieces.pieces);
    let bounds = placed
        .iter()
        .map(|letter| letter.frame)
        .reduce(|bounds, next| bounds.union(next));
    let drawing = canvas(move |_app: &mut Workspace, _ctx, scene, size: Size| {
        let mut p = Painter::new(scene);
        let Some(bounds) = bounds else {
            return;
        };
        let t = proof_transform(bounds, 0.0, size);
        for letter in &placed {
            match &letter.ink {
                Some(ink) => p.fill(&(t * ink.clone()), fill).draw(),
                None => p
                    .stroke(
                        t.transform_rect_bbox(letter.frame).to_path(0.1),
                        &Stroke::new(1.0),
                        missing,
                    )
                    .draw(),
            }
        }
    });
    drawing.background_color(background)
}

fn outline_strip(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
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
    // Proof ink is independent of the drawing: Dark uses yellow proof type
    // while keeping the editable outlines neutral.
    let fill = if app.preview_invert {
        app.palette.selected_ink()
    } else {
        app.palette.proof_ink()
    };
    let background = if app.preview_invert {
        app.palette.selected_bg()
    } else {
        app.palette.proof_strip
    };
    let blur = app.preview_blur;
    // The proof strip follows the open text composition even while an outline
    // tool edits one sort in context. Tool choice and composition lifetime are
    // separate state in both GPUI and Web.
    let proof_text = if app.has_text_session {
        &app.initial_text
    } else {
        &app.preview_text
    };
    let has_preview_text = !proof_text.is_empty();
    let (preview_paths, advance) = if !has_preview_text {
        let mut paths = vec![(*outline).clone()];
        if has_components {
            paths.push((*components).clone());
        }
        (paths, app.session.advance())
    } else {
        let inputs = text_tool::TextInputs::new(&app.font)
            .with_location(&app.font, &app.axis_values)
            .with_direction(app.text_dir)
            .with_text(proof_text)
            .with_shaping_options(
                &app.text_features_disabled,
                app.text_script.as_deref(),
                app.text_language.as_deref(),
            );
        let inputs = if app.on_active_master() {
            let mut live = (*outline).clone();
            if has_components {
                live.extend((*components).clone());
            }
            inputs.with_live_outline(&app.session.glyph_name, std::sync::Arc::new(live))
        } else {
            inputs
        };
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
/// displace the visible ink from the middle of the pane. With no advance, as
/// on a neural canvas, the ink is centered on both axes.
pub(crate) fn proof_transform(
    bounds: kurbo::Rect,
    advance: f64,
    size: kurbo::Size,
) -> kurbo::Affine {
    use masonry::kurbo::Affine;
    let padding = Space::Xl.px();
    let by_height = (size.height - padding * 2.0).max(0.0) / bounds.height().max(1.0);
    let width = if advance > 0.0 {
        advance
    } else {
        bounds.width()
    };
    let by_width = (size.width - padding * 2.0).max(0.0) / width.max(1.0);
    let scale = by_height.min(by_width);
    let x = if advance > 0.0 {
        (size.width - advance * scale) / 2.0
    } else {
        size.width / 2.0 - bounds.center().x * scale
    };
    Affine::scale_non_uniform(scale, -scale)
        .then_translate((x, size.height / 2.0 + bounds.center().y * scale).into())
}

type PreviewContour = Vec<(f64, f64, runebender::font::LayerPointType, bool)>;

fn preview_contours(layer: runebender::font::LayerView<'_>) -> Vec<PreviewContour> {
    layer
        .contours()
        .map(|contour| {
            contour
                .points()
                .map(|point| {
                    (
                        point.position().x,
                        point.position().y,
                        point.point_type(),
                        point.is_smooth(),
                    )
                })
                .collect()
        })
        .collect()
}

/// A large preview of the selected glyph at the foot of the inspector.
/// In the editor, use the live session so the preview follows in-progress edits.
pub(crate) fn glyph_preview(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Circle, Line, Point, Rect, Shape, Size, Stroke};
    let data = if matches!(app.mode, crate::application::workspace::Mode::Editor(_)) {
        app.session.current_layer().map(|layer| {
            let mut outline = app.session.outline();
            outline.extend(app.session.components.clone());
            let contours = preview_contours(layer);
            (std::sync::Arc::new(outline), contours)
        })
    } else {
        app.selected.and_then(|i| {
            let entry = app.font.glyphs.get(i)?;
            let address = app.font.active_layer_address(&entry.name)?;
            let contours = preview_contours(
                app.font
                    .project
                    .document_layer(&entry.name, &address.layer)?,
            );
            Some((entry.outline.clone(), contours))
        })
    };
    let pal = app.palette.clone();
    let background = pal.glyph_preview;
    let neural = app.font.project.is_neural();
    sized_box(canvas(
        move |_app: &mut Workspace, _ctx, scene, size: Size| {
            let mut p = Painter::new(scene);
            // Own every pixel of the preview allocation. Relying only on the
            // wrapper background left the splitter's top edge on the panel
            // surface, which read as a second-colour strip above the glyph.
            p.fill_rect(size.to_rect(), background);
            // Canvas measures at least 100px tall even when the inspector has
            // less space left. In that case a fitted outline would be clipped
            // by the window, so keep the small remainder empty.
            if size.height < design::INSPECTOR_PREVIEW_MIN_READABLE_HEIGHT {
                return;
            }
            let Some((outline, contours)) = &data else {
                return;
            };
            let bounds = outline.bounding_box();
            // A neural canvas is a whole piece of writing: points would only
            // crowd it. Fill the space with solid copies of the ink instead.
            if neural {
                let ink = pal.proof_ink();
                for t in stacked_transforms(bounds, size) {
                    p.fill(&(t * (**outline).clone()), ink).draw();
                }
                return;
            }
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
                let n = contour.len();
                for (i, (x, y, point_type, _)) in contour.iter().enumerate() {
                    if *point_type != runebender::font::LayerPointType::OffCurve {
                        continue;
                    }
                    let off = t * Point::new(*x, *y);
                    for j in [(i + n - 1) % n, (i + 1) % n] {
                        let (on_x, on_y, on_type, _) = contour[j];
                        if on_type != runebender::font::LayerPointType::OffCurve {
                            p.stroke(
                                Line::new(off, t * Point::new(on_x, on_y)),
                                &stroke,
                                pal.handle_line,
                            )
                            .draw();
                        }
                    }
                }
                for (x, y, point_type, smooth) in contour {
                    let at = t * Point::new(*x, *y);
                    let off = *point_type == runebender::font::LayerPointType::OffCurve;
                    let hue = pal.role(if off {
                        "pointOffcurve"
                    } else if *smooth {
                        "pointSmooth"
                    } else {
                        "pointCorner"
                    });
                    let (fill, border) = if pal.points_filled {
                        (hue, pal.point_outline.unwrap_or(pal.text))
                    } else {
                        (background, hue)
                    };
                    let radius = if off || *smooth {
                        design::POINT_CURVE_RADIUS
                    } else {
                        design::POINT_CORNER_RADIUS
                    } * design::INSPECTOR_PREVIEW_POINT_SCALE;
                    if off || *smooth {
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
    // The inspector stack supplies the remaining height. Fill that allocation
    // so no panel-colored tail can appear below the glyph.
    .dims(Dimensions::new(Dim::Stretch, Dim::Stretch))
}

/// Copies of `ink`, fitted to the width of `size` and stacked down it as many
/// times as fit, with the stack centered. Ink too tall for one copy at full
/// width is fitted to the height instead.
fn stacked_transforms(ink: kurbo::Rect, size: kurbo::Size) -> Vec<kurbo::Affine> {
    use masonry::kurbo::Affine;
    const MAX_COPIES: u32 = 8;
    let fill = design::OVERVIEW_GLYPH_PREVIEW_FILL;
    let (width, height) = (ink.width().max(1.0), ink.height().max(1.0));
    let room = size.height * fill;
    let stack = |copies: u32, scale: f64| {
        let row = height * scale;
        row * f64::from(copies) + row * 0.3 * f64::from(copies.saturating_sub(1))
    };
    let mut scale = size.width * fill / width;
    let mut copies = (1..=MAX_COPIES)
        .take_while(|&copies| stack(copies, scale) <= room)
        .last()
        .unwrap_or(0);
    if copies == 0 {
        copies = 1;
        scale = room / height;
    }
    let row = height * scale;
    let top = (size.height - stack(copies, scale)) / 2.0;
    (0..copies)
        .map(|copy| {
            let center_y = top + row * 1.3 * f64::from(copy) + row / 2.0;
            Affine::translate(-ink.center().to_vec2())
                .then_scale_non_uniform(scale, -scale)
                .then_translate((size.width / 2.0, center_y).into())
        })
        .collect()
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

    #[test]
    fn wide_ink_repeats_down_the_inspector_preview() {
        let ink = Rect::new(0.0, 0.0, 1000.0, 250.0);
        let size = Size::new(300.0, 400.0);
        let copies = stacked_transforms(ink, size);
        assert!(copies.len() >= 2, "{}", copies.len());
        for t in &copies {
            let placed = t.transform_rect_bbox(ink);
            assert!(placed.x0 >= 0.0 && placed.x1 <= size.width);
            assert!(placed.y0 >= 0.0 && placed.y1 <= size.height);
            assert!((placed.center().x - size.width / 2.0).abs() < 1e-9);
        }
        // Tall ink does not fit at full width: one copy, fitted to the height.
        let tall = stacked_transforms(Rect::new(0.0, 0.0, 100.0, 1000.0), size);
        assert_eq!(tall.len(), 1);
        assert!(
            tall[0]
                .transform_rect_bbox(Rect::new(0.0, 0.0, 100.0, 1000.0))
                .height()
                <= 400.0
        );
    }

    #[test]
    fn ink_without_an_advance_sits_in_the_middle_of_the_proof() {
        // A neural canvas has no advance, and its ink can lie anywhere.
        let ink = Rect::new(-3000.0, -500.0, -1000.0, 500.0);
        let t = proof_transform(ink, 0.0, Size::new(400.0, 140.0));
        assert_eq!(t * ink.center(), Point::new(200.0, 70.0));
    }
}

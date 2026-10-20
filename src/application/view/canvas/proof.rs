// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The proof view: the glyph being edited and the proposal waiting for it,
//! as drawings side by side and in running text, shown in the edit area in
//! place of the canvas.
//!
//! The loop it serves: the model proposes a change in the chat, the designer
//! reads it here in context, installs it, or goes back to the canvas, draws
//! it their own way and tells the model to look again.

use std::sync::Arc;

use crate::application::design::recipes;
use crate::application::design::theme::Palette;
use crate::application::design::{
    ControlKind, ControlSize, ControlStyle, Region, Space, TextSize, column as xcolumn, row as xrow,
};
use crate::application::editor::tools::text as text_tool;
use crate::application::view::label;
use crate::application::view::panels::preview::proof_transform;
use crate::application::widgets::scroll_viewport::portal;
use crate::application::workspace::Workspace;
use masonry::imaging::Painter;
use masonry::kurbo::{Affine, BezPath, Line, Point, Rect, Shape, Size, Stroke};
use masonry::layout::{Dim, Length};
use masonry::peniko::Color;
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{FlexExt as _, FlexSpacer, canvas, sized_box};

/// Reading sizes for the text lines, in pixels per em: display, text, small.
const TEXT_SIZES: [f64; 3] = [96.0, 48.0, 24.0];
/// Line height as a share of the em, with the ascender and descender inside.
const LINE_HEIGHT: f64 = 1.3;
/// Room for the word that names a line.
const CAPTION_WIDTH: f64 = 72.0;
/// The height of the drawings row: tall enough to judge a curve, short
/// enough that the first text line is in view with it.
const DRAWING_HEIGHT: f64 = 280.0;

/// What one side of the comparison draws: an outline and its advance.
#[derive(Clone)]
struct Side {
    caption: &'static str,
    outline: Arc<BezPath>,
    advance: f64,
}

/// Running text set with the current drawing, or with the proposal in its place.
struct Line_ {
    caption: &'static str,
    paths: Arc<Vec<BezPath>>,
}

pub(crate) fn proof_view(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use xilem::core::one_of::Either;
    let pal = &app.palette;
    let glyph = app.session.glyph_name.clone();
    let (ink, paper) = if app.preview_invert {
        (pal.selected_ink(), pal.selected_bg())
    } else {
        (pal.proof_ink(), pal.proof_strip)
    };
    let rule = pal.outline;
    let stroke = pal.stroke;

    // The current drawing, live from the session, with its components.
    let mut current = (*app.session.outline_arc()).clone();
    let components = app.session.components_arc();
    if !components.elements().is_empty() {
        current.extend((*components).clone());
    }
    let current = Side {
        caption: "Current",
        outline: Arc::new(current),
        advance: app.session.advance(),
    };

    // The proposal waiting for this glyph: the latest one that names it.
    let task = app
        .ai
        .proposals
        .iter()
        .rev()
        .find(|proposal| proposal.glyphs.contains(&glyph))
        .map(|proposal| proposal.task.clone());
    let proposed = task.as_deref().and_then(|task| {
        let outline = app.model_proposal_outline(task, &glyph)?;
        Some(Side {
            caption: "Proposal",
            outline: Arc::new(outline),
            advance: app
                .font
                .proposal_advance(task, &glyph)
                .unwrap_or(current.advance),
        })
    });

    // The heading: the glyph, the proposal if there is one, and its two answers.
    let title = match &task {
        Some(task) => format!("{glyph} \u{00b7} proposal {task}"),
        None => format!("{glyph} \u{00b7} no proposal waiting"),
    };
    let actions = task.clone().map(|task| {
        let install = task.clone();
        xrow(
            Region::Inline,
            (
                word(pal, ControlKind::Secondary, "Install", move |app| {
                    app.install_proposal(&install, None);
                }),
                word(pal, ControlKind::Quiet, "Discard", move |app| {
                    app.discard_proposal(&task);
                }),
            ),
        )
        .gap(Space::Sm)
    });
    let header = xrow(
        Region::Inline,
        (
            label(title).text_size(TextSize::Body.px()).color(pal.text),
            FlexSpacer::Flex(1.0),
            actions,
        ),
    );

    // The drawings, at one scale so a moved point reads as a move.
    let bounds = [Some(&current), proposed.as_ref()]
        .into_iter()
        .flatten()
        .map(|side| side.outline.bounding_box())
        .filter(|bounds| bounds.area() > 0.0)
        .reduce(|a, b| a.union(b))
        .unwrap_or(Rect::new(0.0, 0.0, current.advance.max(1.0), 1.0));
    let widest = current
        .advance
        .max(proposed.as_ref().map_or(0.0, |side| side.advance));
    let drawings = match &proposed {
        Some(proposed) => Either::A(
            xrow(
                Region::Inline,
                (
                    drawing(pal, &current, bounds, widest, ink, paper, rule, stroke).flex(1.0),
                    drawing(pal, proposed, bounds, widest, ink, paper, rule, stroke).flex(1.0),
                ),
            )
            .gap(Space::Md),
        ),
        None => Either::B(drawing(
            pal, &current, bounds, widest, ink, paper, rule, stroke,
        )),
    };

    // Running text: the open composition, or a sample built around the glyph.
    let text = if app.has_text_session {
        app.initial_text.clone()
    } else {
        app.preview_text.clone()
    };
    let codepoint = app
        .font
        .glyphs
        .iter()
        .find(|entry| entry.name == glyph)
        .and_then(|entry| entry.codepoint);
    let text = if !text.trim().is_empty() {
        text
    } else {
        codepoint.map(sample_text).unwrap_or_default()
    };
    let lines: Vec<Line_> = if text.is_empty() {
        Vec::new()
    } else {
        let inputs = text_tool::TextInputs::new(&app.font)
            .with_location(&app.font, &app.axis_values)
            .with_direction(app.text_dir)
            .with_text(&text)
            .with_shaping_options(
                &app.text_features_disabled,
                app.text_script.as_deref(),
                app.text_language.as_deref(),
            );
        let live = app.on_active_master();
        let mut lines = Vec::new();
        let before = if live {
            inputs
                .clone()
                .with_live_outline(&glyph, current.outline.clone())
        } else {
            inputs.clone()
        };
        lines.push(Line_ {
            caption: current.caption,
            paths: Arc::new(placed(&before)),
        });
        if let Some(proposed) = &proposed {
            let after = inputs
                .with_live_outline(&glyph, proposed.outline.clone())
                .with_live_advance(&glyph, proposed.advance);
            lines.push(Line_ {
                caption: proposed.caption,
                paths: Arc::new(placed(&after)),
            });
        }
        lines
    };
    let upm = app.font.units_per_em().max(1.0);
    let ascender = app.font.ascender();
    let text_rows: Vec<_> = TEXT_SIZES
        .iter()
        .map(|&size| {
            let rows: Vec<_> = lines
                .iter()
                .map(|line| {
                    text_line(
                        pal,
                        line.caption,
                        line.paths.clone(),
                        size / upm,
                        ascender,
                        size * LINE_HEIGHT,
                        ink,
                        paper,
                    )
                })
                .collect();
            xcolumn(Region::List, rows).gap(Space::Sm)
        })
        .collect();

    // A proof is read top to bottom and scrolls, as a printed one would be
    // turned: the drawings at a fixed height, then the text at each size.
    portal(
        sized_box(
            xcolumn(
                Region::Panel,
                (
                    header,
                    sized_box(drawings).dims(Dimensions::new(
                        Dim::Stretch,
                        Dim::Fixed(Length::px(DRAWING_HEIGHT)),
                    )),
                    xcolumn(Region::List, text_rows).gap(Space::Md),
                ),
            )
            .gap(Space::Md),
        )
        .padding(Space::Md.length())
        .dims(Dimensions::new(Dim::Stretch, Dim::Auto)),
    )
    .constrain_horizontal(true)
}

/// A sample that shows a glyph among flat, round and capital neighbours.
fn sample_text(c: char) -> String {
    format!("{c}{c}{c} n{c}n o{c}o H{c}H O{c}O {c}")
}

/// The sorts of a text line as positioned paths.
fn placed(inputs: &text_tool::TextInputs) -> Vec<BezPath> {
    text_tool::TextState::new(inputs)
        .placed()
        .into_iter()
        .map(|sort| sort.path)
        .collect()
}

/// One drawing in its advance box, with its caption above.
#[expect(
    clippy::too_many_arguments,
    reason = "a drawing is one picture with its colors; a struct would name the same things"
)]
fn drawing(
    pal: &Palette,
    side: &Side,
    bounds: Rect,
    widest: f64,
    ink: Color,
    paper: Color,
    rule: Color,
    stroke: f64,
) -> impl WidgetView<Workspace> + use<> {
    let outline = side.outline.clone();
    let advance = side.advance;
    let picture = canvas(move |_: &mut Workspace, _, scene, size: Size| {
        let mut p = Painter::new(scene);
        p.fill(size.to_rect(), paper).draw();
        let t = proof_transform(bounds, widest, size);
        // The advance box: baseline, left and right sidebearing lines.
        let pen = Stroke::new(stroke);
        let baseline = Line::new(t * Point::new(0.0, 0.0), t * Point::new(advance, 0.0));
        p.stroke(baseline, &pen, rule).draw();
        for x in [0.0, advance] {
            let edge = Line::new(
                t * Point::new(x, bounds.min_y()),
                t * Point::new(x, bounds.max_y()),
            );
            p.stroke(edge, &pen, rule).draw();
        }
        p.fill(&(t * &*outline), ink).draw();
    });
    xcolumn(
        Region::List,
        (
            label(side.caption)
                .text_size(TextSize::Caption.px())
                .color(pal.text_muted),
            sized_box(picture).dims(Dimensions::new(Dim::Stretch, Dim::Stretch)),
        ),
    )
    .gap(Space::Sm)
}

/// One line of running text at a reading size, with the word that names it.
#[expect(
    clippy::too_many_arguments,
    reason = "a line is one picture with its colors; a struct would name the same things"
)]
fn text_line(
    pal: &Palette,
    caption: &'static str,
    paths: Arc<Vec<BezPath>>,
    scale: f64,
    ascender: f64,
    height: f64,
    ink: Color,
    paper: Color,
) -> impl WidgetView<Workspace> + use<> {
    let picture = canvas(move |_: &mut Workspace, _, scene, size: Size| {
        let mut p = Painter::new(scene);
        p.fill(size.to_rect(), paper).draw();
        let inset = Space::Md.px();
        // A line longer than the pane is set smaller, whole, rather than cut.
        let length = paths
            .iter()
            .filter(|path| !path.elements().is_empty())
            .map(|path| path.bounding_box().max_x())
            .fold(0.0, f64::max);
        let room = (size.width - 2.0 * inset).max(1.0);
        let scale = if length * scale > room {
            room / length
        } else {
            scale
        };
        let t = Affine::scale_non_uniform(scale, -scale)
            .then_translate((inset, inset + ascender * scale).into());
        for path in paths.iter() {
            p.fill(&(t * path), ink).draw();
        }
    });
    xrow(
        Region::Inline,
        (
            sized_box(
                label(caption)
                    .text_size(TextSize::Caption.px())
                    .color(pal.text_muted),
            )
            .dims(Dimensions::new(
                Dim::Fixed(Length::px(CAPTION_WIDTH)),
                Dim::Auto,
            )),
            sized_box(picture)
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::Fixed(Length::px(height)),
                ))
                .flex(1.0),
        ),
    )
    .gap(Space::Sm)
}

/// A word on a control at control height.
fn word<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    kind: ControlKind,
    text: &'static str,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let style = ControlStyle::new(pal, kind);
    sized_box(
        recipes::button(
            style,
            label(text)
                .text_size(TextSize::Body.px())
                .color(style.ink()),
            on_click,
        )
        .padding(masonry::properties::Padding::horizontal(Space::Md.length())),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Swatch)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_puts_the_glyph_among_flat_round_and_capital_neighbours() {
        let text = sample_text('0');
        assert!(text.starts_with("000 "), "{text}");
        for context in ["n0n", "o0o", "H0H", "O0O"] {
            assert!(text.contains(context), "{text} lacks {context}");
        }
    }
}

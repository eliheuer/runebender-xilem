// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Paint a glyph from the editable application icon UFO in any control.

use masonry::imaging::Painter;
use masonry::kurbo::{Cap, Join, Rect, Stroke};
use masonry::layout::Length;
use masonry::properties::Dimensions;
use runebender::ui::icons::icons;
use xilem::style::Style as _;
use xilem::view::{canvas, sized_box};
use xilem::{Color, WidgetView};

use crate::application::workspace::Workspace;

/// Fit and paint a named icon in a control's frame.
pub(crate) fn paint(painter: &mut Painter<'_>, name: &str, frame: Rect, color: Color) {
    let Some(icon) = icons().get(name) else {
        return;
    };
    let path = icon.fitted_path(frame);
    if icon.stroke {
        painter
            .stroke(
                &path,
                &Stroke::new(1.5)
                    .with_caps(Cap::Round)
                    .with_join(Join::Round),
                color,
            )
            .draw();
    } else {
        painter.fill(&path, color).draw();
    }
}

/// A square icon view for embedding in an existing button.
pub(crate) fn view(
    name: &'static str,
    label: &'static str,
    color: Color,
    size: f64,
) -> impl WidgetView<Workspace> {
    sized_box(
        canvas(move |_: &mut Workspace, _, scene, frame| {
            paint(
                &mut Painter::new(scene).as_dyn(),
                name,
                frame.to_rect(),
                color,
            );
        })
        .alt_text(label),
    )
    .dims(Dimensions::fixed(Length::px(size), Length::px(size)))
}

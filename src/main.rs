// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Runebender on xix. A font editor: glyph grid, glyph editor, sidebar.
//! See `docs/XILEM-GAPS.md` for what this build costs against the
//! same editor on GPUI.

mod actions;
mod edit;
mod launch;
mod model;
mod platform;
mod view;
mod widgets;
mod workspace;

use std::path::Path as FsPath;
use std::sync::Arc;

use crate::view::design::{column as xcolumn, row as xrow};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use masonry::properties::types::CrossAxisAlignment;
/// Keep native scroll containers, but give their overlay bars no visible area.
/// Wheel and trackpad input is handled by Portal independently of these bars.
fn default_property_set() -> masonry::core::DefaultProperties {
    let mut properties = masonry::theme::default_property_set();
    properties.insert::<masonry::widgets::ScrollBar, _>(Dimensions::new(
        Dim::Fixed(Length::ZERO),
        Dim::Fixed(Length::ZERO),
    ));
    properties
}
use winit::dpi::LogicalSize;
use winit::error::EventLoopError;
use xilem::style::Style;
use xilem::view::{
    FlexExt as _, FlexSpacer, button, canvas, flex_col, flex_row, portal, sized_box, text_button,
};
use xilem::{EventLoop, EventLoopBuilder, WidgetView, WindowOptions, Xilem};

use edit::session::Session;
use edit::*;
use launch::*;
use model::FontModel;
use platform::*;
use runebender_core::analysis::category::GlyphCategory;
use view::canvas::editor::editor;
use view::canvas::grid::{Cell, CellMetrics, GridEvent, cells_of, grid};
use view::chrome::*;
use view::design::{ControlSize, Radius, Region, Space, Stroke, TextSize};
use view::panels::{
    editor::*, editor_info::*, info::*, local_ai::*, nodes::*, preview::*, sections::*, tabs::*,
};
use view::render::*;
use view::theme::Palette;
use view::*;
use widgets::drag_region::drag_region;
use widgets::icon_button::icon_button;
use widgets::*;
use workspace::*;

/// The interface font that ships with the editor: Virtua Grotesk, the
/// same family as the demo font. Registered at launch.
pub(crate) const UI_FONT: &[u8] = include_bytes!("../assets/fonts/VirtuaGrotesk-Regular.ttf");
/// Its family name, as the font's name table spells it.
pub(crate) const UI_FONT_FAMILY: &str = "Virtua Grotesk";

/// A label in the interface font. Every label goes through here so
/// the family is set in one place.
pub(crate) fn label(text: impl Into<masonry::core::ArcStr>) -> xilem::view::Label {
    xilem::view::label(text)
        .font(UI_FONT_FAMILY)
        .text_size(TextSize::Body.px())
}

/// Editable interface text uses the same family and size as labels.
fn text_input<F, State, Action: 'static>(
    contents: String,
    on_changed: F,
) -> xilem::view::TextInput<State, Action>
where
    F: Fn(&mut State, String) -> Action + Send + Sync + 'static,
    State: 'static,
{
    xilem::view::text_input(contents, on_changed)
        .font(UI_FONT_FAMILY)
        .text_size(TextSize::Body.px())
}

fn main() -> Result<(), EventLoopError> {
    run(EventLoop::with_user_event())
}

#[cfg(test)]
mod scrollbar_tests {
    use super::default_property_set;
    use masonry::core::Widget;
    use masonry::kurbo::Vec2;
    use masonry::layout::AsUnit;
    use masonry::widgets::{Portal, SizedBox};
    use masonry_testing::TestHarness;

    #[test]
    fn hidden_bars_preserve_wheel_scrolling() {
        let content = SizedBox::empty().size(300.px(), 1000.px()).prepare();
        let mut harness = TestHarness::create_with_size(
            default_property_set(),
            Portal::new(content).prepare(),
            (200, 200),
        );
        harness.mouse_move((100., 100.));
        harness.mouse_wheel(Vec2::new(0., -100.));
        let y = harness.edit_root_widget(|portal| portal.widget.get_viewport_pos().y);
        assert!(y > 0., "wheel input must still move the viewport");
        let _ = harness.render();
    }
}

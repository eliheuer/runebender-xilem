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
use masonry::theme::default_property_set;
use winit::dpi::LogicalSize;
use winit::error::EventLoopError;
use xilem::style::Style;
use xilem::view::{
    FlexExt as _, FlexSpacer, button, canvas, flex_col, flex_row, label, portal, sized_box, slider,
    text_button, text_input,
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
use view::panels::{editor::*, info::*, local_ai::*, nodes::*, preview::*, sections::*, tabs::*};
use view::render::*;
use view::theme::Palette;
use view::*;
use widgets::icon_button::icon_button;
use widgets::*;
use workspace::*;

fn main() -> Result<(), EventLoopError> {
    run(EventLoop::with_user_event())
}

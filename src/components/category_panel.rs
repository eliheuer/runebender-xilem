// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Category panel for filtering glyphs in the glyph grid view
//!
//! Based on Glyphs.app's sidebar with categories like Letter,
//! Figures, Punctuation, etc.

use masonry::properties::types::AsUnit;
use masonry::properties::Padding;
use xilem::style::Style;
use xilem::view::{button, flex_col, label, sized_box, CrossAxisAlignment};
use xilem::WidgetView;

use crate::data::AppState;
use crate::theme;

/// Width of the category panel
pub const CATEGORY_PANEL_WIDTH: f64 = 200.0;

/// Glyph categories for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlyphCategory {
    /// Show all glyphs
    #[default]
    All,
    /// Letters (uppercase and lowercase)
    Letter,
    /// Numbers/figures
    Number,
    /// Punctuation marks
    Punctuation,
    /// Symbols
    Symbol,
    /// Marks (combining diacritics)
    Mark,
    /// Separators (spaces, etc.)
    Separator,
    /// Other/uncategorized
    Other,
}

impl GlyphCategory {
    /// Get the display name for the category
    pub fn display_name(&self) -> &'static str {
        match self {
            GlyphCategory::All => "All",
            GlyphCategory::Letter => "Letter",
            GlyphCategory::Number => "Number",
            GlyphCategory::Punctuation => "Punctuation",
            GlyphCategory::Symbol => "Symbol",
            GlyphCategory::Mark => "Mark",
            GlyphCategory::Separator => "Separator",
            GlyphCategory::Other => "Other",
        }
    }

    /// Get all categories
    pub fn all_categories() -> &'static [GlyphCategory] {
        &[
            GlyphCategory::All,
            GlyphCategory::Letter,
            GlyphCategory::Number,
            GlyphCategory::Punctuation,
            GlyphCategory::Symbol,
            GlyphCategory::Mark,
            GlyphCategory::Separator,
            GlyphCategory::Other,
        ]
    }

    /// Determine category from a Unicode codepoint
    pub fn from_codepoint(c: char) -> GlyphCategory {
        use unicode_general_category::{
            get_general_category, GeneralCategory,
        };

        match get_general_category(c) {
            // Letters
            GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter => GlyphCategory::Letter,

            // Numbers
            GeneralCategory::DecimalNumber
            | GeneralCategory::LetterNumber
            | GeneralCategory::OtherNumber => GlyphCategory::Number,

            // Punctuation
            GeneralCategory::ConnectorPunctuation
            | GeneralCategory::DashPunctuation
            | GeneralCategory::OpenPunctuation
            | GeneralCategory::ClosePunctuation
            | GeneralCategory::InitialPunctuation
            | GeneralCategory::FinalPunctuation
            | GeneralCategory::OtherPunctuation => {
                GlyphCategory::Punctuation
            }

            // Symbols
            GeneralCategory::MathSymbol
            | GeneralCategory::CurrencySymbol
            | GeneralCategory::ModifierSymbol
            | GeneralCategory::OtherSymbol => GlyphCategory::Symbol,

            // Marks
            GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark => GlyphCategory::Mark,

            // Separators
            GeneralCategory::SpaceSeparator
            | GeneralCategory::LineSeparator
            | GeneralCategory::ParagraphSeparator => {
                GlyphCategory::Separator
            }

            // Other
            _ => GlyphCategory::Other,
        }
    }
}

/// Category panel view for the left sidebar
pub fn category_panel(
    state: &AppState,
) -> impl WidgetView<AppState> + use<> {
    let selected_category = state.glyph_category_filter;

    let category_buttons: Vec<_> =
        GlyphCategory::all_categories()
            .iter()
            .map(|&cat| {
                let is_selected = cat == selected_category;
                category_button(cat, is_selected)
            })
            .collect();

    sized_box(
        flex_col((
            // Header
            sized_box(
                label("CATEGORIES")
                    .text_size(12.0)
                    .color(theme::text::SECONDARY),
            )
            .padding(Padding::from_vh(8.0, 8.0)),
            // Category list
            flex_col(category_buttons),
        ))
        .cross_axis_alignment(CrossAxisAlignment::Fill),
    )
    .width(CATEGORY_PANEL_WIDTH.px())
    .background_color(theme::panel::BACKGROUND)
    .border_color(theme::panel::OUTLINE)
    .border_width(1.5)
    .corner_radius(theme::size::PANEL_RADIUS)
}

/// Single category button
fn category_button(
    category: GlyphCategory,
    is_selected: bool,
) -> impl WidgetView<AppState> + use<> {
    let name = category.display_name();
    let bg_color = if is_selected {
        theme::grid::CELL_SELECTED_BACKGROUND
    } else {
        theme::panel::BACKGROUND
    };

    sized_box(
        button(
            label(name)
                .text_size(14.0)
                .color(theme::text::PRIMARY),
            move |state: &mut AppState| {
                state.glyph_category_filter = category;
            },
        )
        .background_color(bg_color)
        .border_color(masonry::vello::peniko::Color::TRANSPARENT),
    )
    .expand_width()
    .padding(Padding::from_vh(1.0, 6.0))
}

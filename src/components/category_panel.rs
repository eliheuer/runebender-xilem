// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Category panel for filtering glyphs in the glyph grid view
//!
//! A sidebar list of category filters modelled after the Glyphs.app
//! sidebar: plain left-aligned text labels, selected item shown
//! with a tinted highlight row. No button chrome.

use kurbo::{Affine, Rect, RoundedRect, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, BrushIndex, ChildrenIds, EventCtx,
    LayoutCtx, PaintCtx, PointerButton, PointerButtonEvent,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    StyleProperty, TextEvent, Update, UpdateCtx, Widget, render_text,
};
use masonry::vello::Scene;
use masonry::vello::peniko::{Brush, Color, Fill};
use parley::{FontContext, FontStack, LayoutContext};
use std::marker::PhantomData;
use xilem::core::{
    MessageContext, MessageResult, Mut, View, ViewMarker,
};
use xilem::{Pod, ViewCtx};

use crate::theme;

/// Width of the category panel
pub const CATEGORY_PANEL_WIDTH: f64 = 220.0;

// ============================================================
// Layout constants
// ============================================================

/// Left/right text inset within the panel
const TEXT_INSET: f64 = 12.0;
/// Vertical padding above the header label
const HEADER_TOP: f64 = 10.0;
/// Height of a single category row
const ROW_HEIGHT: f64 = 24.0;
/// Gap between header and first item
const HEADER_GAP: f64 = 6.0;
/// Font size for the header label
const HEADER_FONT_SIZE: f64 = 12.0;
/// Font size for category labels
const ITEM_FONT_SIZE: f64 = 14.0;
/// Corner radius on the selected-row highlight
const HIGHLIGHT_RADIUS: f64 = 4.0;
/// Horizontal inset for the highlight rect
const HIGHLIGHT_INSET: f64 = 4.0;

// ============================================================
// GlyphCategory enum (unchanged)
// ============================================================

/// Glyph categories for filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GlyphCategory {
    #[default]
    All,
    Letter,
    Number,
    Punctuation,
    Symbol,
    Mark,
    Separator,
    Other,
}

impl GlyphCategory {
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

    pub fn from_codepoint(c: char) -> GlyphCategory {
        use unicode_general_category::{
            GeneralCategory, get_general_category,
        };

        match get_general_category(c) {
            GeneralCategory::UppercaseLetter
            | GeneralCategory::LowercaseLetter
            | GeneralCategory::TitlecaseLetter
            | GeneralCategory::ModifierLetter
            | GeneralCategory::OtherLetter => GlyphCategory::Letter,

            GeneralCategory::DecimalNumber
            | GeneralCategory::LetterNumber
            | GeneralCategory::OtherNumber => GlyphCategory::Number,

            GeneralCategory::ConnectorPunctuation
            | GeneralCategory::DashPunctuation
            | GeneralCategory::OpenPunctuation
            | GeneralCategory::ClosePunctuation
            | GeneralCategory::InitialPunctuation
            | GeneralCategory::FinalPunctuation
            | GeneralCategory::OtherPunctuation => {
                GlyphCategory::Punctuation
            }

            GeneralCategory::MathSymbol
            | GeneralCategory::CurrencySymbol
            | GeneralCategory::ModifierSymbol
            | GeneralCategory::OtherSymbol => GlyphCategory::Symbol,

            GeneralCategory::NonspacingMark
            | GeneralCategory::SpacingMark
            | GeneralCategory::EnclosingMark => GlyphCategory::Mark,

            GeneralCategory::SpaceSeparator
            | GeneralCategory::LineSeparator
            | GeneralCategory::ParagraphSeparator => {
                GlyphCategory::Separator
            }

            _ => GlyphCategory::Other,
        }
    }
}

// ============================================================
// Custom Masonry Widget
// ============================================================

/// Action emitted when a category is clicked
#[derive(Debug, Clone, Copy)]
pub struct CategorySelected(pub GlyphCategory);

/// A custom widget that renders the category list as plain
/// left-aligned text — no button chrome. The selected row gets
/// a subtle rounded-rect highlight.
pub struct CategoryListWidget {
    selected: GlyphCategory,
    hover_index: Option<usize>,
}

impl CategoryListWidget {
    pub fn new(selected: GlyphCategory) -> Self {
        Self {
            selected,
            hover_index: None,
        }
    }

    /// Y offset where the item rows begin (after header)
    fn items_top(&self) -> f64 {
        HEADER_TOP + HEADER_FONT_SIZE + HEADER_GAP
    }

    /// Which row index is at a given y coordinate
    fn row_at_y(&self, y: f64) -> Option<usize> {
        let top = self.items_top();
        if y < top {
            return None;
        }
        let index = ((y - top) / ROW_HEIGHT) as usize;
        let cats = GlyphCategory::all_categories();
        if index < cats.len() {
            Some(index)
        } else {
            None
        }
    }

}

impl Widget for CategoryListWidget {
    type Action = CategorySelected;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {}

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        let width = CATEGORY_PANEL_WIDTH;
        let height = bc.max().height;
        bc.constrain(Size::new(width, height))
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        scene: &mut Scene,
    ) {
        let size = ctx.size();
        let width = size.width;
        let cats = GlyphCategory::all_categories();

        // --- Panel background and border ---
        let panel_rect = RoundedRect::from_rect(
            Rect::from_origin_size(
                kurbo::Point::ZERO,
                size,
            ),
            theme::size::PANEL_RADIUS,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(theme::panel::BACKGROUND),
            None,
            &panel_rect,
        );
        scene.stroke(
            &kurbo::Stroke::new(
                theme::size::TOOLBAR_BORDER_WIDTH,
            ),
            Affine::IDENTITY,
            &Brush::Solid(theme::panel::OUTLINE),
            None,
            &panel_rect,
        );

        // --- Header ---
        let mut font_cx = FontContext::default();
        let mut layout_cx = LayoutContext::new();

        let header_text = "Categories";
        let mut builder = layout_cx.ranged_builder(
            &mut font_cx,
            header_text,
            1.0,
            false,
        );
        builder.push_default(StyleProperty::FontSize(
            HEADER_FONT_SIZE as f32,
        ));
        builder.push_default(StyleProperty::FontStack(
            FontStack::Single(parley::FontFamily::Generic(
                parley::GenericFamily::SansSerif,
            )),
        ));
        builder
            .push_default(StyleProperty::Brush(BrushIndex(0)));
        let mut layout = builder.build(header_text);
        layout.break_all_lines(None);

        let header_color: Color = theme::text::SECONDARY;
        let header_brushes = vec![Brush::Solid(header_color)];
        render_text(
            scene,
            Affine::translate((TEXT_INSET, HEADER_TOP)),
            &layout,
            &header_brushes,
            false,
        );

        // --- Category rows ---
        let top = self.items_top();

        for (i, &cat) in cats.iter().enumerate() {
            let row_y = top + (i as f64) * ROW_HEIGHT;
            let is_selected = cat == self.selected;
            let is_hovered = self.hover_index == Some(i);

            // Selected highlight
            if is_selected {
                let highlight = RoundedRect::from_rect(
                    Rect::new(
                        HIGHLIGHT_INSET,
                        row_y,
                        width - HIGHLIGHT_INSET,
                        row_y + ROW_HEIGHT,
                    ),
                    HIGHLIGHT_RADIUS,
                );
                scene.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    &Brush::Solid(
                        theme::grid::CELL_SELECTED_BACKGROUND,
                    ),
                    None,
                    &highlight,
                );
            }

            // Text color
            let text_color = if is_selected {
                theme::grid::CELL_SELECTED_OUTLINE
            } else if is_hovered {
                theme::text::PRIMARY
            } else {
                theme::text::SECONDARY
            };

            // Render label
            let name = cat.display_name();
            let mut builder = layout_cx.ranged_builder(
                &mut font_cx,
                name,
                1.0,
                false,
            );
            builder.push_default(StyleProperty::FontSize(
                ITEM_FONT_SIZE as f32,
            ));
            builder.push_default(StyleProperty::FontStack(
                FontStack::Single(parley::FontFamily::Generic(
                    parley::GenericFamily::SansSerif,
                )),
            ));
            builder.push_default(StyleProperty::Brush(
                BrushIndex(0),
            ));
            let mut item_layout = builder.build(name);
            item_layout.break_all_lines(None);

            let text_y = row_y
                + (ROW_HEIGHT - item_layout.height() as f64) / 2.0;
            let brushes = vec![Brush::Solid(text_color)];
            render_text(
                scene,
                Affine::translate((TEXT_INSET, text_y)),
                &item_layout,
                &brushes,
                false,
            );
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::List
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                let pos = ctx.local_position(state.position);
                if let Some(index) = self.row_at_y(pos.y) {
                    let cat =
                        GlyphCategory::all_categories()[index];
                    ctx.submit_action::<CategorySelected>(
                        CategorySelected(cat),
                    );
                }
                ctx.set_handled();
            }
            PointerEvent::Move(pointer_move) => {
                let pos = ctx.local_position(
                    pointer_move.current.position,
                );
                let new_hover = self.row_at_y(pos.y);
                if new_hover != self.hover_index {
                    self.hover_index = new_hover;
                    ctx.request_render();
                }
            }
            PointerEvent::Leave(_) => {
                if self.hover_index.is_some() {
                    self.hover_index = None;
                    ctx.request_render();
                }
            }
            _ => {}
        }
    }

    fn on_text_event(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &TextEvent,
    ) {
    }
}

// ============================================================
// Xilem View wrapper
// ============================================================

type CategoryCallback<State> =
    Box<dyn Fn(&mut State, GlyphCategory) + Send + Sync>;

pub fn category_panel<State, Action>(
    selected: GlyphCategory,
    callback: impl Fn(&mut State, GlyphCategory)
        + Send
        + Sync
        + 'static,
) -> CategoryPanelView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    CategoryPanelView {
        selected,
        callback: Box::new(callback),
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
pub struct CategoryPanelView<State, Action = ()> {
    selected: GlyphCategory,
    callback: CategoryCallback<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker
    for CategoryPanelView<State, Action>
{
}

impl<State: 'static, Action: 'static + Default>
    View<State, Action, ViewCtx>
    for CategoryPanelView<State, Action>
{
    type Element = Pod<CategoryListWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget = CategoryListWidget::new(self.selected);
        let pod = ctx.create_pod(widget);
        ctx.record_action(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        _prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        if element.widget.selected != self.selected {
            element.widget.selected = self.selected;
        }
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<Action> {
        match message.take_message::<CategorySelected>() {
            Some(action) => {
                (self.callback)(app_state, action.0);
                MessageResult::Action(Action::default())
            }
            None => MessageResult::Stale,
        }
    }
}

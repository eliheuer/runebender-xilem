// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph grid view - displays all glyphs in a scrollable grid

use std::collections::HashSet;
use std::marker::PhantomData;

use kurbo::{Affine, BezPath, Rect, RoundedRect, Shape, Size};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, BrushIndex, ChildrenIds, EventCtx,
    LayoutCtx, PaintCtx, PointerButton, PointerButtonEvent,
    PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
    StyleProperty, TextEvent, Update, UpdateCtx, Widget,
    render_text,
};
use masonry::properties::types::AsUnit;
use masonry::vello::Scene;
use masonry::vello::peniko::{Brush, Color, Fill};
use parley::{FontContext, FontStack, LayoutContext};
use xilem::core::one_of::Either;
use xilem::core::{
    MessageContext, MessageResult, Mut, View, ViewMarker,
};
use xilem::style::Style;
use xilem::view::{
    flex_col, flex_row, label, sized_box, zstack,
    CrossAxisAlignment, FlexExt,
};
use xilem::{Pod, ViewCtx, WidgetView};

use crate::components::{
    category_panel, create_master_infos, glyph_info_panel,
    grid_scroll_handler, mark_color_panel, master_toolbar_view,
    size_tracker, system_toolbar_view, GlyphCategory,
    SystemToolbarButton, CATEGORY_PANEL_WIDTH,
    GLYPH_INFO_PANEL_WIDTH,
};
use crate::data::AppState;
use crate::glyph_renderer;
use crate::theme;
use crate::workspace;

// ============================================================
// Bento Layout Constants
// ============================================================

/// Uniform gap between all tiles — panels, grid cells, outer padding
const BENTO_GAP: f64 = 6.0;

// ============================================================
// Glyph Grid Tab View
// ============================================================

/// Tab 0: Glyph grid view with bento tile layout
pub fn glyph_grid_tab(
    state: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
    zstack((
        // Invisible: size tracker (measures window dimensions)
        size_tracker(|state: &mut AppState, width, height| {
            // Grid width = window - panels - outer padding - inner gaps
            state.window_width = width
                - CATEGORY_PANEL_WIDTH
                - GLYPH_INFO_PANEL_WIDTH
                - BENTO_GAP * 4.0;
            state.window_height = height;
        }),
        // Bento tile layout
        flex_col((
            // Row 1: File info stretches, toolbars fixed on right
            flex_row((
                file_info_panel(state).flex(1.0),
                master_toolbar_panel(state),
                system_toolbar_view(
                    |state: &mut AppState, button| match button {
                        SystemToolbarButton::Save => {
                            state.save_workspace();
                        }
                    },
                ),
            ))
            .gap(BENTO_GAP.px()),
            // Row 2: Three-column content (fills remaining height)
            flex_row((
                flex_col((
                    category_panel(
                        state.glyph_category_filter,
                        |state: &mut AppState, cat| {
                            state.glyph_category_filter = cat;
                            state.grid_scroll_row = 0;
                        },
                    )
                    .flex(1.0),
                    mark_color_panel(
                        current_mark_color_index(state),
                        |state: &mut AppState, color_index| {
                            state.set_glyph_mark_color(color_index);
                        },
                    ),
                ))
                .gap(BENTO_GAP.px()),
                // Grid wrapped in scroll handler container
                // (captures scroll wheel, arrow keys, Cmd+S)
                grid_scroll_handler(
                    glyph_grid_view(state),
                    |state: &mut AppState, delta| {
                        let count =
                            state.cached_filtered_count;
                        state.scroll_grid(delta, count);
                    },
                    |state: &mut AppState| {
                        state.save_workspace();
                    },
                )
                .flex(1.0),
                glyph_info_panel(state),
            ))
            .gap(BENTO_GAP.px())
            .cross_axis_alignment(CrossAxisAlignment::Fill)
            .flex(1.0),
        ))
        .gap(BENTO_GAP.px())
        .padding(BENTO_GAP * 2.0)
        .background_color(theme::app::BACKGROUND),
    ))
}

// ============================================================
// Toolbar Panels
// ============================================================

/// File info panel showing the loaded file path and last save time
fn file_info_panel(
    state: &AppState,
) -> impl WidgetView<AppState> + use<> {
    let path_display = state
        .loaded_file_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "No file loaded".to_string());

    let save_display = state
        .last_saved_display()
        .map(|s| format!("Saved {}", s))
        .unwrap_or_else(|| "Not saved".to_string());

    sized_box(
        flex_col((
            label(path_display)
                .text_size(16.0)
                .color(theme::grid::CELL_TEXT),
            label(save_display)
                .text_size(16.0)
                .color(theme::grid::CELL_TEXT),
        ))
        .gap(2.px())
        .cross_axis_alignment(CrossAxisAlignment::Start),
    )
    .expand_width()
    .padding(12.0)
    .background_color(theme::panel::BACKGROUND)
    .border_color(theme::panel::OUTLINE)
    .border_width(1.5)
    .corner_radius(theme::size::PANEL_RADIUS)
}

/// Master toolbar panel — only shown when designspace has multiple masters
fn master_toolbar_panel(
    state: &AppState,
) -> impl WidgetView<AppState> + use<> {
    if let Some(ref designspace) = state.designspace
        && designspace.masters.len() > 1
    {
        let master_infos =
            create_master_infos(&designspace.masters);
        let active_master = designspace.active_master;

        return Either::A(master_toolbar_view(
            master_infos,
            active_master,
            |state: &mut AppState, index| {
                if let Some(ref mut ds) = state.designspace {
                    ds.switch_master(index);
                }
            },
        ));
    }

    Either::B(sized_box(label("")).width(0.px()).height(0.px()))
}

// ============================================================
// Mark Color Helpers
// ============================================================

/// Look up the selected glyph's mark color palette index
fn current_mark_color_index(state: &AppState) -> Option<usize> {
    let glyph_name = state.selected_glyph.as_ref()?;
    let workspace_arc = state.active_workspace()?;
    let workspace = workspace_arc.read().unwrap();
    let glyph = workspace.get_glyph(glyph_name)?;
    glyph
        .mark_color
        .as_ref()
        .and_then(|s| rgba_string_to_palette_index(s))
}

// ============================================================
// Glyph Grid View
// ============================================================

/// Glyph grid showing only rows that fit in the visible area.
/// Scrolling is handled by `grid_scroll_handler` which adjusts
/// `state.grid_scroll_row`.
fn glyph_grid_view(
    state: &mut AppState,
) -> impl WidgetView<AppState> + use<> {
    let columns = state.grid_columns();
    let visible = state.visible_grid_rows();
    let upm = get_upm_from_state(state);
    let selected_glyphs = state.selected_glyphs.clone();

    // Build only the visible slice of glyph data —
    // filter first, then slice, then build bezpaths.
    let (visible_data, filtered_count) =
        build_visible_glyph_data(
            state,
            columns,
            visible,
            state.grid_scroll_row,
        );
    // Cache the filtered count so scroll callbacks don't
    // have to re-iterate all glyphs.
    state.cached_filtered_count = filtered_count;

    let rows_of_cells = build_glyph_rows(
        &visible_data,
        columns,
        &selected_glyphs,
        upm,
    );

    // Each row flexes to fill available height evenly
    let flexy_rows: Vec<_> = rows_of_cells
        .into_iter()
        .map(|row| row.flex(1.0))
        .collect();

    flex_col(flexy_rows)
        .gap(BENTO_GAP.px())
        .cross_axis_alignment(CrossAxisAlignment::Fill)
}

// ============================================================
// Grid Building Helpers
// ============================================================

/// Get UPM (units per em) from workspace state
fn get_upm_from_state(state: &AppState) -> f64 {
    state
        .active_workspace()
        .and_then(|w| w.read().unwrap().units_per_em)
        .unwrap_or(1000.0)
}

/// Type alias for glyph data tuple
/// (name, path with components, codepoints, contour count,
///  mark color palette index)
type GlyphData = (
    String,
    Option<BezPath>,
    Vec<char>,
    usize,
    Option<usize>,
);

/// Build glyph data for only the visible rows.
///
/// Filters by category (cheap — only checks codepoints), slices
/// to the visible window, THEN builds bezpaths (expensive) for
/// only those glyphs.
fn build_visible_glyph_data(
    state: &AppState,
    columns: usize,
    visible_rows: usize,
    scroll_row: usize,
) -> (Vec<GlyphData>, usize) {
    let workspace_arc = match state.active_workspace() {
        Some(w) => w,
        None => return (Vec::new(), 0),
    };
    let workspace = workspace_arc.read().unwrap();
    let category_filter = state.glyph_category_filter;

    // Step 1: Collect filtered glyph names (cheap — no bezpath)
    let all_names = workspace.glyph_names();
    let filtered_names: Vec<&str> = all_names
        .iter()
        .filter(|name| {
            if let Some(glyph) = workspace.get_glyph(name) {
                matches_category(
                    &glyph.codepoints,
                    category_filter,
                )
            } else {
                false
            }
        })
        .map(|s| s.as_str())
        .collect();

    // Step 2: Slice to only the visible window
    let start = scroll_row * columns;
    let end =
        ((scroll_row + visible_rows) * columns)
            .min(filtered_names.len());
    let total_filtered = filtered_names.len();
    if start > total_filtered {
        return (Vec::new(), total_filtered);
    }
    let visible_names = &filtered_names[start..end];

    // Step 3: Build full glyph data (with bezpaths) for
    // only the visible glyphs
    let data = visible_names
        .iter()
        .map(|name| {
            build_single_glyph_data(&workspace, name)
        })
        .collect();
    (data, total_filtered)
}

/// Check if a glyph matches the category filter
fn matches_category(
    codepoints: &[char],
    category: GlyphCategory,
) -> bool {
    if category == GlyphCategory::All {
        return true;
    }
    if codepoints.is_empty() {
        return category == GlyphCategory::Other;
    }
    let glyph_category =
        GlyphCategory::from_codepoint(codepoints[0]);
    glyph_category == category
}

/// Build data for a single glyph
fn build_single_glyph_data(
    workspace: &workspace::Workspace,
    name: &str,
) -> GlyphData {
    if let Some(glyph) = workspace.get_glyph(name) {
        let count = glyph.contours.len();
        let codepoints = glyph.codepoints.clone();
        let path =
            glyph_renderer::glyph_to_bezpath_with_components(
                glyph, workspace,
            );
        let mark_index = glyph
            .mark_color
            .as_ref()
            .and_then(|s| rgba_string_to_palette_index(s));
        (
            name.to_string(),
            Some(path),
            codepoints,
            count,
            mark_index,
        )
    } else {
        (name.to_string(), None, Vec::new(), 0, None)
    }
}

/// Convert an RGBA string to a palette index by matching
/// against the known palette strings
fn rgba_string_to_palette_index(rgba: &str) -> Option<usize> {
    theme::mark::RGBA_STRINGS
        .iter()
        .position(|&s| s == rgba)
}

/// Build rows of glyph cells from glyph data
fn build_glyph_rows(
    glyph_data: &[GlyphData],
    columns: usize,
    selected_glyphs: &HashSet<String>,
    upm: f64,
) -> Vec<impl WidgetView<AppState> + use<>> {
    glyph_data
        .chunks(columns)
        .map(|chunk| {
            let row_items: Vec<_> = chunk
                .iter()
                .map(
                    |(name, path_opt, codepoints, _, mark_color)| {
                        let is_selected =
                            selected_glyphs.contains(name);
                        glyph_cell(
                            name.clone(),
                            path_opt.clone(),
                            codepoints.clone(),
                            is_selected,
                            upm,
                            *mark_color,
                        )
                        .flex(1.0)
                    },
                )
                .collect();
            flex_row(row_items)
                .gap(BENTO_GAP.px())
                .cross_axis_alignment(CrossAxisAlignment::Fill)
        })
        .collect()
}

// ============================================================
// Glyph Cell (custom widget approach)
// ============================================================

/// Individual glyph cell in the grid — uses custom widget for
/// single-click (select), double-click (open editor),
/// and shift-click (toggle multi-select).
fn glyph_cell(
    glyph_name: String,
    path_opt: Option<BezPath>,
    codepoints: Vec<char>,
    is_selected: bool,
    upm: f64,
    mark_color: Option<usize>,
) -> impl WidgetView<AppState> + use<> {
    glyph_cell_view(
        glyph_name,
        path_opt,
        codepoints,
        is_selected,
        upm,
        mark_color,
        |state: &mut AppState, action| match action {
            GlyphCellAction::Select(name) => {
                state.select_glyph(name);
            }
            GlyphCellAction::ShiftSelect(name) => {
                state.toggle_glyph_selection(name);
            }
            GlyphCellAction::Open(name) => {
                state.select_glyph(name.clone());
                state.open_editor(name);
            }
        },
    )
}

// ============================================================
// GlyphCellAction
// ============================================================

/// Actions emitted by the glyph cell widget
#[derive(Debug, Clone)]
enum GlyphCellAction {
    /// Single-click without shift — select this glyph only
    Select(String),
    /// Single-click with shift — toggle in/out of multi-select
    ShiftSelect(String),
    /// Double-click — open glyph in editor
    Open(String),
}

// ============================================================
// GlyphCellWidget (custom Masonry widget)
// ============================================================

/// Font size for cell labels
const CELL_LABEL_SIZE: f64 = 16.0;

thread_local! {
    static FONT_CX: std::cell::RefCell<FontContext> =
        std::cell::RefCell::new(FontContext::default());
    static LAYOUT_CX: std::cell::RefCell<
        LayoutContext<BrushIndex>,
    > = std::cell::RefCell::new(LayoutContext::new());
}
/// Height reserved for the label area at the bottom of the cell
const CELL_LABEL_HEIGHT: f64 = 44.0;
/// Padding around the glyph preview and labels
const CELL_PAD: f64 = 8.0;

/// Custom widget that renders a glyph cell and handles
/// click, double-click, and shift-click events.
struct GlyphCellWidget {
    glyph_name: String,
    path: Option<BezPath>,
    codepoints: Vec<char>,
    upm: f64,
    is_selected: bool,
    mark_color: Option<usize>,
}

impl GlyphCellWidget {
    fn new(
        glyph_name: String,
        path: Option<BezPath>,
        codepoints: Vec<char>,
        upm: f64,
        is_selected: bool,
        mark_color: Option<usize>,
    ) -> Self {
        Self {
            glyph_name,
            path,
            codepoints,
            upm,
            is_selected,
            mark_color,
        }
    }

    /// Resolve the mark color to a Color value
    fn mark(&self) -> Option<Color> {
        self.mark_color.map(|i| theme::mark::COLORS[i])
    }

    /// Get (background, border) colors for this cell
    fn cell_colors(&self, is_hovered: bool) -> (Color, Color) {
        if self.is_selected {
            (
                theme::grid::CELL_BACKGROUND,
                theme::grid::CELL_SELECTED_OUTLINE,
            )
        } else if is_hovered {
            (
                theme::grid::CELL_BACKGROUND,
                theme::grid::CELL_SELECTED_OUTLINE,
            )
        } else if let Some(color) = self.mark() {
            (theme::grid::CELL_BACKGROUND, color)
        } else {
            (
                theme::grid::CELL_BACKGROUND,
                theme::grid::CELL_OUTLINE,
            )
        }
    }

    /// Paint the glyph bezpath into the preview area
    fn paint_glyph(
        &self,
        scene: &mut Scene,
        preview_rect: Rect,
    ) {
        let path = match &self.path {
            Some(p) if !p.is_empty() => p,
            _ => return,
        };

        let bounds = path.bounding_box();
        let scale = preview_rect.height() / self.upm;
        let scale = scale * 0.8;

        // Center horizontally based on bounding box
        let scaled_width = bounds.width() * scale;
        let left_pad =
            (preview_rect.width() - scaled_width) / 2.0;
        let x_translation =
            preview_rect.x0 + left_pad - bounds.x0 * scale;

        // Baseline at ~6% from bottom of preview area
        let baseline_offset = 0.06;
        let baseline = preview_rect.height() * baseline_offset;

        let transform = Affine::new([
            scale,
            0.0,
            0.0,
            -scale,
            x_translation,
            preview_rect.y1 - baseline,
        ]);

        let transformed_path = transform * path;
        let color = if self.is_selected {
            theme::grid::CELL_SELECTED_OUTLINE
        } else {
            self.mark().unwrap_or(theme::grid::CELL_OUTLINE)
        };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(color),
            None,
            &transformed_path,
        );
    }

    /// Paint the name and unicode labels
    fn paint_labels(
        &self,
        scene: &mut Scene,
        label_rect: Rect,
        is_hovered: bool,
    ) {
        let text_color = if self.is_selected || is_hovered {
            theme::grid::CELL_SELECTED_OUTLINE
        } else {
            self.mark().unwrap_or(theme::grid::CELL_TEXT)
        };

        let display_name = format_display_name(&self.glyph_name);
        let unicode_display =
            format_unicode_display(&self.codepoints);

        FONT_CX.with(|font_cell| {
            LAYOUT_CX.with(|layout_cell| {
                let mut font_cx = font_cell.borrow_mut();
                let mut layout_cx = layout_cell.borrow_mut();

                // Name label
                let mut builder = layout_cx.ranged_builder(
                    &mut font_cx,
                    &display_name,
                    1.0,
                    false,
                );
                builder.push_default(StyleProperty::FontSize(
                    CELL_LABEL_SIZE as f32,
                ));
                builder.push_default(StyleProperty::FontStack(
                    FontStack::Single(
                        parley::FontFamily::Generic(
                            parley::GenericFamily::SansSerif,
                        ),
                    ),
                ));
                builder.push_default(StyleProperty::Brush(
                    BrushIndex(0),
                ));
                let mut name_layout =
                    builder.build(&display_name);
                name_layout.break_all_lines(None);

                let brushes = vec![Brush::Solid(text_color)];
                let name_y = label_rect.y0 + 2.0;
                render_text(
                    scene,
                    Affine::translate((
                        label_rect.x0,
                        name_y,
                    )),
                    &name_layout,
                    &brushes,
                    false,
                );

                // Unicode label
                if !unicode_display.is_empty() {
                    let mut builder = layout_cx.ranged_builder(
                        &mut font_cx,
                        &unicode_display,
                        1.0,
                        false,
                    );
                    builder.push_default(
                        StyleProperty::FontSize(
                            CELL_LABEL_SIZE as f32,
                        ),
                    );
                    builder.push_default(
                        StyleProperty::FontStack(
                            FontStack::Single(
                                parley::FontFamily::Generic(
                                    parley::GenericFamily::SansSerif,
                                ),
                            ),
                        ),
                    );
                    builder.push_default(
                        StyleProperty::Brush(BrushIndex(0)),
                    );
                    let mut uni_layout =
                        builder.build(&unicode_display);
                    uni_layout.break_all_lines(None);

                    let uni_y =
                        name_y + CELL_LABEL_SIZE + 2.0;
                    render_text(
                        scene,
                        Affine::translate((
                            label_rect.x0,
                            uni_y,
                        )),
                        &uni_layout,
                        &brushes,
                        false,
                    );
                }
            });
        });
    }
}

impl Widget for GlyphCellWidget {
    type Action = GlyphCellAction;

    fn register_children(
        &mut self,
        _ctx: &mut RegisterCtx<'_>,
    ) {
    }

    fn update(
        &mut self,
        ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &Update,
    ) {
        if matches!(event, Update::HoveredChanged(_)) {
            ctx.request_render();
        }
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        // Fill available space from flex layout
        bc.max()
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        scene: &mut Scene,
    ) {
        let size = ctx.size();
        let (bg_color, border_color) =
            self.cell_colors(ctx.is_hovered());

        // Panel background and border
        let panel_rect = RoundedRect::from_rect(
            Rect::from_origin_size(kurbo::Point::ZERO, size),
            theme::size::PANEL_RADIUS,
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(bg_color),
            None,
            &panel_rect,
        );
        scene.stroke(
            &kurbo::Stroke::new(
                theme::size::TOOLBAR_BORDER_WIDTH,
            ),
            Affine::IDENTITY,
            &Brush::Solid(border_color),
            None,
            &panel_rect,
        );

        // Glyph preview area (above labels, inset by padding)
        let preview_height =
            (size.height - CELL_LABEL_HEIGHT).max(0.0);
        let preview_rect = Rect::new(
            CELL_PAD,
            CELL_PAD,
            size.width - CELL_PAD,
            preview_height,
        );
        self.paint_glyph(scene, preview_rect);

        // Label area (bottom of cell, inset by padding)
        let label_rect = Rect::new(
            CELL_PAD,
            preview_height,
            size.width - CELL_PAD,
            size.height - CELL_PAD,
        );
        self.paint_labels(scene, label_rect, ctx.is_hovered());
    }

    fn accessibility_role(&self) -> Role {
        Role::Button
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
                let name = self.glyph_name.clone();
                if state.count >= 2 {
                    ctx.submit_action::<GlyphCellAction>(
                        GlyphCellAction::Open(name),
                    );
                } else if state.modifiers.shift() {
                    ctx.submit_action::<GlyphCellAction>(
                        GlyphCellAction::ShiftSelect(name),
                    );
                } else {
                    ctx.submit_action::<GlyphCellAction>(
                        GlyphCellAction::Select(name),
                    );
                }
                // Don't set_handled — let Down bubble to the
                // GridScrollWidget container so it grabs focus
                // for arrow key scrolling.
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
// GlyphCellView (Xilem View wrapper)
// ============================================================

type GlyphCellCallback<State> =
    Box<dyn Fn(&mut State, GlyphCellAction) + Send + Sync>;

fn glyph_cell_view<State, Action>(
    glyph_name: String,
    path: Option<BezPath>,
    codepoints: Vec<char>,
    is_selected: bool,
    upm: f64,
    mark_color: Option<usize>,
    callback: impl Fn(&mut State, GlyphCellAction)
        + Send
        + Sync
        + 'static,
) -> GlyphCellView<State, Action>
where
    State: 'static,
    Action: 'static,
{
    GlyphCellView {
        glyph_name,
        path,
        codepoints,
        is_selected,
        upm,
        mark_color,
        callback: Box::new(callback),
        phantom: PhantomData,
    }
}

#[must_use = "View values do nothing unless provided to Xilem."]
struct GlyphCellView<State, Action = ()> {
    glyph_name: String,
    path: Option<BezPath>,
    codepoints: Vec<char>,
    is_selected: bool,
    upm: f64,
    mark_color: Option<usize>,
    callback: GlyphCellCallback<State>,
    phantom: PhantomData<fn() -> (State, Action)>,
}

impl<State, Action> ViewMarker
    for GlyphCellView<State, Action>
{
}

impl<State: 'static, Action: 'static + Default>
    View<State, Action, ViewCtx>
    for GlyphCellView<State, Action>
{
    type Element = Pod<GlyphCellWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget = GlyphCellWidget::new(
            self.glyph_name.clone(),
            self.path.clone(),
            self.codepoints.clone(),
            self.upm,
            self.is_selected,
            self.mark_color,
        );
        let pod = ctx.create_pod(widget);
        ctx.record_action(pod.new_widget.id());
        (pod, ())
    }

    fn rebuild(
        &self,
        prev: &Self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _app_state: &mut State,
    ) {
        let mut changed = false;
        let w = &mut element.widget;
        if w.is_selected != self.is_selected {
            w.is_selected = self.is_selected;
            changed = true;
        }
        if w.glyph_name != self.glyph_name {
            w.glyph_name = self.glyph_name.clone();
            changed = true;
        }
        if self.path != prev.path {
            w.path = self.path.clone();
            changed = true;
        }
        if self.codepoints != prev.codepoints {
            w.codepoints = self.codepoints.clone();
            changed = true;
        }
        if self.upm != prev.upm {
            w.upm = self.upm;
            changed = true;
        }
        if self.mark_color != prev.mark_color {
            w.mark_color = self.mark_color;
            changed = true;
        }
        if changed {
            element.ctx.request_render();
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
        match message.take_message::<GlyphCellAction>() {
            Some(action) => {
                (self.callback)(app_state, *action);
                MessageResult::Action(Action::default())
            }
            None => MessageResult::Stale,
        }
    }
}

// ============================================================
// Cell Formatting Helpers
// ============================================================

/// Format display name with truncation if too long
fn format_display_name(glyph_name: &str) -> String {
    if glyph_name.len() > 12 {
        format!("{}...", &glyph_name[..9])
    } else {
        glyph_name.to_string()
    }
}

/// Format Unicode codepoint display string
fn format_unicode_display(codepoints: &[char]) -> String {
    if let Some(first_char) = codepoints.first() {
        format!("U+{:04X}", *first_char as u32)
    } else {
        String::new()
    }
}

// ============================================================
// Path Helpers
// ============================================================


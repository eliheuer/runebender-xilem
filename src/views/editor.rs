// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Editor view — the main glyph editing tab.
//!
//! Builds the full editor layout: canvas (via `EditorWidget`), toolbars,
//! coordinate panel, and a text-buffer preview strip showing neighboring
//! glyphs with live kerning. This is the view shown when `Tab::Editor` is
//! active. The `build_text_buffer_preview` function renders the multi-glyph
//! text strip with real-time path rendering and kerning lookups.

use std::sync::Arc;

use kurbo::{BezPath, Shape};
use masonry::properties::Padding;
use masonry::layout::{AsUnit, UnitPoint};
use xilem::WidgetView;
use xilem::core::one_of::Either;
use xilem::style::Style;
use xilem::view::{
    ChildAlignment, ZStackExt, flex_col, flex_row, label, sized_box,
    split, text_input, transformed, zstack,
};

use crate::components::workspace_toolbar::WorkspaceToolbarButton;
use crate::components::{
    TransformAction, coordinate_panel, create_master_infos, edit_mode_toolbar_view, editor_view,
    glyph_view, master_toolbar_view, multi_glyph_view, shapes_toolbar_view,
    text_direction_toolbar_view, transform_panel, workspace_toolbar_view,
};
use crate::data::AppState;
use crate::model::read_workspace;
use crate::theme;
use crate::theme::size::{UI_PANEL_GAP, UI_PANEL_MARGIN};
use crate::tools::shapes::ShapeType;
use crate::tools::{ToolBox, ToolId};

// ===== Editor Tab View =====

/// Tab 1: Editor view with toolbar floating over canvas
pub fn editor_tab(state: &mut AppState) -> impl WidgetView<AppState> + use<> {
    let Some(session) = &state.editor_session else {
        // No session - show empty view (shouldn't happen)
        return Either::B(flex_col((label("No editor session"),)));
    };

    let current_tool = session.current_tool.id();
    let glyph_name = session
        .active_sort_name
        .clone()
        .unwrap_or_else(|| "".to_string());
    let session_arc = Arc::new(session.clone());

    // Get current shape type if shapes tool is selected
    let current_shape = if let ToolBox::Shapes(shapes_tool) = &session.current_tool {
        shapes_tool.shape_type()
    } else {
        ShapeType::Rectangle // Default
    };

    // Get current text direction
    let current_text_direction = session.text_direction;

    // Determine which sub-toolbar to show
    let show_shapes_toolbar = current_tool == ToolId::Shapes;
    let show_text_direction_toolbar = current_tool == ToolId::Text;

    // Editor canvas with floating overlays on top,
    // text buffer preview in a separate bottom panel
    let has_text_buffer = session.text_buffer.is_some();

    let canvas_with_overlays = zstack((
        // Background: the editor canvas (full screen)
        editor_view(
            session_arc.clone(),
            |state: &mut AppState,
             updated_session,
             save_requested,
             close_requested| {
                state.update_editor_session(updated_session);
                if save_requested {
                    state.save_workspace();
                }
                if close_requested {
                    state.close_editor();
                }
            },
        ),
        // Foreground: floating toolbars
        transformed(
            flex_col((
                edit_mode_toolbar_view(current_tool, |state: &mut AppState, tool_id| {
                    state.set_editor_tool(tool_id);
                }),
                if show_shapes_toolbar {
                    Either::A(shapes_toolbar_view(
                        current_shape,
                        |state: &mut AppState, shape_type| {
                            state.set_shape_type(shape_type);
                        },
                    ))
                } else if show_text_direction_toolbar {
                    Either::B(Either::A(text_direction_toolbar_view(
                        current_text_direction,
                        |state: &mut AppState, direction| {
                            state.set_text_direction(direction);
                        },
                    )))
                } else {
                    Either::B(Either::B(label("")))
                },
            ))
            .cross_axis_alignment(xilem::view::CrossAxisAlignment::Start),
        )
        .translate((UI_PANEL_MARGIN, UI_PANEL_MARGIN))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::TOP_LEFT)),
        // Bottom-left: glyph preview panel
        transformed(if session.panels_visible {
            Either::A(glyph_preview_pane(session_arc.clone(), glyph_name.clone()))
        } else {
            Either::B(sized_box(label("")).width(0.px()).height(0.px()))
        })
        .translate((UI_PANEL_MARGIN, -UI_PANEL_MARGIN))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::BOTTOM_LEFT)),
        // Bottom-center: active glyph panel
        transformed(if session.panels_visible {
            Either::A(active_glyph_panel_centered(state))
        } else {
            Either::B(sized_box(label("")).width(0.px()).height(0.px()))
        })
        .translate((0.0, -UI_PANEL_MARGIN))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::BOTTOM)),
        // Bottom-right: coordinate panel
        transformed(if session.panels_visible {
            Either::A(coordinate_panel_from_session(&session_arc))
        } else {
            Either::B(sized_box(label("")).width(0.px()).height(0.px()))
        })
        .translate((-UI_PANEL_MARGIN, -UI_PANEL_MARGIN))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::BOTTOM_RIGHT)),
        // Right side: transform panel
        transformed(if session.panels_visible {
            let has_selection = !session.selection.is_empty();
            let contour_count = session.paths.len();
            Either::A(transform_panel(
                has_selection,
                contour_count,
                apply_transform,
            ))
        } else {
            Either::B(sized_box(label("")).width(0.px()).height(0.px()))
        })
        .translate((-UI_PANEL_MARGIN, 0.0))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::new(1.0, 0.5))),
        // Top-right: Master toolbar + Workspace toolbar
        transformed(
            flex_row((
                master_toolbar_panel(state),
                workspace_toolbar_view(|state: &mut AppState, button| match button {
                    WorkspaceToolbarButton::GlyphGrid => {
                        state.close_editor();
                    }
                }),
            ))
            .gap(UI_PANEL_GAP.px()),
        )
        .translate((-UI_PANEL_MARGIN, UI_PANEL_MARGIN))
        .alignment(ChildAlignment::SelfAligned(UnitPoint::TOP_RIGHT)),
    ));

    // Vertical split: canvas on top, text preview on bottom.
    // When no text buffer, use a minimal 0px bottom panel.
    Either::A(
        sized_box(
            split(
                canvas_with_overlays,
                text_buffer_preview_bottom(
                    if has_text_buffer {
                        Some(session_arc.clone())
                    } else {
                        None
                    },
                ),
            )
            .split_axis(masonry::kurbo::Axis::Vertical)
            .split_point(if has_text_buffer { 0.85 } else { 1.0 })
            .min_lengths(200.px(), 0.px())
            .bar_thickness(0.px())
            .min_bar_area(6.px())
            .draggable(has_text_buffer),
        )
        .background_color(theme::canvas::BACKGROUND),
    )
}

// ===== Helper Views =====

/// Master toolbar panel - only shown when a designspace is loaded
fn master_toolbar_panel(state: &AppState) -> impl WidgetView<AppState> + use<> {
    // Only show master toolbar when we have a designspace with multiple masters
    if let Some(ref designspace) = state.designspace
        && designspace.masters.len() > 1
    {
        let master_infos = create_master_infos(&designspace.masters);
        let active_master = designspace.active_master;

        return Either::A(master_toolbar_view(
            master_infos,
            active_master,
            |state: &mut AppState, index| {
                // Switch to the selected master while preserving text buffer
                state.switch_editor_master(index);
            },
        ));
    }

    // No designspace or single master - return empty view
    Either::B(sized_box(label("")).width(0.px()).height(0.px()))
}

/// Helper to create coordinate panel from session data
fn coordinate_panel_from_session(
    session: &Arc<crate::editing::EditSession>,
) -> impl WidgetView<AppState> + use<> {
    tracing::debug!(
        "[coordinate_panel_from_session] Building view with \
         quadrant={:?}",
        session.coord_selection.quadrant
    );
    coordinate_panel(
        Arc::clone(session),
        |state: &mut AppState, updated_session| {
            tracing::debug!(
                "[coordinate_panel callback] Session updated, \
                 new quadrant={:?}",
                updated_session.coord_selection.quadrant
            );
            state.editor_session = Some(updated_session);
        },
        |state: &mut AppState, field, value| {
            state.update_selection_coordinate(field, value);
        },
    )
}

/// Glyph preview pane — compact panel sized to fit the glyph
fn glyph_preview_pane(
    session: Arc<crate::editing::EditSession>,
    _glyph_name: String,
) -> impl WidgetView<AppState> + use<> {
    const PANEL_HEIGHT: f64 = 140.0;
    const MARGIN: f64 = 0.1; // 10% margin used in fit_to_bounds

    let glyph_path = build_glyph_path(&session);
    let upm = session.ascender - session.descender;

    // Size panel width from the glyph's visual bounding box
    // so the glyph fills the space with equal margins
    let bounds = glyph_path.bounding_box();
    let usable_h = PANEL_HEIGHT * (1.0 - 2.0 * MARGIN);
    let aspect = if bounds.height() > 0.0 {
        bounds.width() / bounds.height()
    } else {
        1.0
    };
    let fitted_w = usable_h * aspect;
    let panel_width = (fitted_w / (1.0 - 2.0 * MARGIN))
        .max(60.0)
        .min(200.0);

    let glyph_preview = if !glyph_path.is_empty() {
        Either::A(
            glyph_view(
                glyph_path,
                panel_width,
                PANEL_HEIGHT,
                upm,
            )
            .color(theme::panel::GLYPH_PREVIEW)
            .fit_to_bounds(),
        )
    } else {
        Either::B(label(""))
    };

    sized_box(glyph_preview)
    .width(panel_width.px())
    .height(PANEL_HEIGHT.px())
    .background_color(theme::panel::BACKGROUND)
    .border_color(theme::panel::OUTLINE)
    .border_width(1.5.px())
    .corner_radius(8.0.px())
}

/// Active glyph panel showing editable metrics (Glyphs app style)
/// Only shown when a glyph is active
fn active_glyph_panel_centered(state: &AppState) -> impl WidgetView<AppState> + use<> {
    const PANEL_HEIGHT: f64 = 140.0;
    const PANEL_WIDTH: f64 = 488.0; // Match text buffer preview width

    // Get session and glyph name
    let session = match &state.editor_session {
        Some(s) => s,
        None => return Either::B(sized_box(label("")).width(0.px()).height(0.px())),
    };

    let glyph_name = match &session.active_sort_name {
        Some(n) => n.clone(),
        None => return Either::B(sized_box(label("")).width(0.px()).height(0.px())),
    };

    // Only show if we have an active glyph
    if glyph_name.is_empty() {
        return Either::B(sized_box(label("")).width(0.px()).height(0.px()));
    }

    // Compute LSB/RSB from live paths, not the stale
    // session.glyph.contours, so they update during drags
    let width = session.glyph.width;
    let (lsb, rsb) = live_sidebearings(session);
    let left_group = session.glyph.left_group.as_deref().unwrap_or("");
    let right_group = session.glyph.right_group.as_deref().unwrap_or("");

    // Get kerning values
    let left_kern = state.get_left_kern();
    let right_kern = state.get_right_kern();

    // Format Unicode
    let unicode_display = if let Some(first_char) = session.glyph.codepoints.first() {
        format!("{:04X}", *first_char as u32)
    } else {
        String::from("")
    };

    // Row 1 (Top): Name and Unicode (both editable)
    // Widths: 346 (3 quarters) + 8 gap + 110 (1 quarter) = 464px (aligns with row 2)
    let top_row = flex_row((
        sized_box(
            text_input(glyph_name.clone(), |_state: &mut AppState, _new_value| {
                // TODO: implement glyph name editing
            })
            .text_alignment(parley::Alignment::Center),
        )
        .width(346.px()),
        sized_box(
            text_input(unicode_display, |_state: &mut AppState, _new_value| {
                // TODO: implement unicode editing
            })
            .text_alignment(parley::Alignment::Center),
        )
        .width(110.px()),
    ))
    .main_axis_alignment(xilem::view::MainAxisAlignment::Start)
    .gap(8.px());

    // Row 2 (Middle): Left kern, LSB, RSB, Right kern (all editable)
    // Widths: 4 × 110 + 3 × 8 gaps = 464px
    // NOTE: text_alignment is set before placeholder, but due to an upstream issue in Xilem 0.4.0,
    // placeholder text does not respect text_alignment and remains left-aligned.
    // See context/text-input-placeholder-alignment.md for details and upstream PR tracking.
    let middle_row = flex_row((
        sized_box(
            text_input(
                left_kern.map(|v| format!("{:.0}", v)).unwrap_or_default(),
                |state: &mut AppState, new_value| {
                    state.update_left_kern(new_value);
                },
            )
            .text_alignment(parley::Alignment::Center) // Placeholder won't center until upstream fix
            .placeholder("Kern"),
        )
        .width(110.px()),
        sized_box(
            text_input(
                format!("{:.0}", lsb),
                |_state: &mut AppState, _new_value| {
                    // TODO: implement LSB editing
                },
            )
            .text_alignment(parley::Alignment::Center),
        )
        .width(110.px()),
        sized_box(
            text_input(
                format!("{:.0}", rsb),
                |_state: &mut AppState, _new_value| {
                    // TODO: implement RSB editing
                },
            )
            .text_alignment(parley::Alignment::Center),
        )
        .width(110.px()),
        sized_box(
            text_input(
                right_kern.map(|v| format!("{:.0}", v)).unwrap_or_default(),
                |state: &mut AppState, new_value| {
                    state.update_right_kern(new_value);
                },
            )
            .text_alignment(parley::Alignment::Center) // Placeholder won't center until upstream fix
            .placeholder("Kern"),
        )
        .width(110.px()),
    ))
    .main_axis_alignment(xilem::view::MainAxisAlignment::Start)
    .gap(8.px());

    // Row 3 (Bottom): Left kern group, Width, Right kern group (all editable)
    // Widths: 149 + 8 + 150 + 8 + 149 = 464px
    // NOTE: Placeholder alignment issue same as row 2 - see context/text-input-placeholder-alignment.md
    let bottom_row = flex_row((
        sized_box(
            text_input(left_group.to_string(), |state: &mut AppState, new_value| {
                state.update_left_group(new_value);
            })
            .text_alignment(parley::Alignment::Center) // Placeholder won't center until upstream fix
            .placeholder("Group"),
        )
        .width(149.px()),
        sized_box(
            text_input(
                format!("{:.0}", width),
                |state: &mut AppState, new_value| {
                    state.update_glyph_width(new_value);
                },
            )
            .text_alignment(parley::Alignment::Center),
        )
        .width(150.px()),
        sized_box(
            text_input(
                right_group.to_string(),
                |state: &mut AppState, new_value| {
                    state.update_right_group(new_value);
                },
            )
            .text_alignment(parley::Alignment::Center) // Placeholder won't center until upstream fix
            .placeholder("Group"),
        )
        .width(149.px()),
    ))
    .main_axis_alignment(xilem::view::MainAxisAlignment::Start)
    .gap(8.px());

    // Combine all three rows with consistent 8px vertical gap
    let content = flex_col((top_row, middle_row, bottom_row))
        .main_axis_alignment(xilem::view::MainAxisAlignment::Center)
        .gap(8.px());

    Either::A(
        sized_box(content)
            .width(PANEL_WIDTH.px())
            .height(PANEL_HEIGHT.px())
            .background_color(theme::panel::BACKGROUND)
            .border_color(theme::panel::OUTLINE)
            .border_width(1.5.px())
            .corner_radius(8.0.px())
            .padding(Padding {
                left: 12.0.px(),
                right: 12.0.px(),
                top: 0.0.px(),
                bottom: 0.0.px(),
            }),
    )
}

// ===== Preview Pane Helpers =====

/// Build the glyph path from session paths
fn build_glyph_path(session: &crate::editing::EditSession) -> BezPath {
    let mut glyph_path = BezPath::new();
    for path in session.paths.iter() {
        glyph_path.extend(path.to_bezpath());
    }
    glyph_path
}

/// Compute LSB and RSB from the live editing paths
///
/// During drags, `session.glyph.contours` is stale — the live data
/// is in `session.paths`. This iterates all points to find the
/// x-extent, then derives sidebearings from the advance width.
fn live_sidebearings(session: &crate::editing::EditSession) -> (f64, f64) {
    use crate::path::Path;

    let width = session.glyph.width;

    let mut min_x: Option<f64> = None;
    let mut max_x: Option<f64> = None;

    for path in session.paths.iter() {
        let points = match path {
            Path::Cubic(c) => c.points(),
            Path::Quadratic(q) => q.points(),
            Path::Hyper(h) => h.points(),
        };
        for point in points.iter() {
            let x = point.point.x;
            min_x = Some(min_x.map_or(x, |m: f64| m.min(x)));
            max_x = Some(max_x.map_or(x, |m: f64| m.max(x)));
        }
    }

    let lsb = min_x.unwrap_or(0.0);
    let rsb = match max_x {
        Some(mx) => width - mx,
        None => width,
    };
    (lsb, rsb)
}

/// Bottom panel text buffer preview (Glyphs-style).
///
/// Full-width panel with dark background, rendered outside the
/// canvas zstack so it never blocks pointer events.
fn text_buffer_preview_bottom(
    session: Option<Arc<crate::editing::EditSession>>,
) -> impl WidgetView<AppState> + use<> {
    let session = match session {
        Some(s) => s,
        None => {
            return Either::B(
                sized_box(label("")).width(0.px()).height(0.px()),
            );
        }
    };

    let workspace = match &session.workspace {
        Some(ws) => ws,
        None => {
            return Either::B(
                sized_box(label("")).width(0.px()).height(0.px()),
            );
        }
    };

    let buffer = match &session.text_buffer {
        Some(b) => b,
        None => {
            return Either::B(
                sized_box(label("")).width(0.px()).height(0.px()),
            );
        }
    };

    let is_rtl = session.text_direction.is_rtl();

    let total_width: f64 = if is_rtl {
        buffer
            .iter()
            .filter_map(|sort| {
                if let crate::sort::SortKind::Glyph {
                    advance_width, ..
                } = &sort.kind
                {
                    Some(*advance_width)
                } else {
                    None
                }
            })
            .sum()
    } else {
        0.0
    };

    let mut glyph_paths: Vec<BezPath> = Vec::new();
    let mut x_offset = if is_rtl { total_width } else { 0.0 };
    let mut prev_glyph_name: Option<String> = None;
    let mut prev_glyph_group: Option<String> = None;

    for sort in buffer.iter() {
        match &sort.kind {
            crate::sort::SortKind::Glyph {
                name,
                advance_width,
                ..
            } => {
                if is_rtl {
                    x_offset -= advance_width;
                }

                if let Some(prev_name) = &prev_glyph_name {
                    let ws = read_workspace(workspace);
                    let curr_group = ws
                        .get_glyph(name)
                        .and_then(|g| g.left_group.as_deref());
                    let kern = crate::model::kerning::lookup_kerning(
                        &ws.kerning,
                        &ws.groups,
                        prev_name,
                        prev_glyph_group.as_deref(),
                        name,
                        curr_group,
                    );
                    if is_rtl {
                        x_offset -= kern;
                    } else {
                        x_offset += kern;
                    }
                }

                let mut glyph_path = BezPath::new();
                if sort.is_active {
                    for path in session.paths.iter() {
                        glyph_path.extend(path.to_bezpath());
                    }
                    let ws = read_workspace(workspace);
                    for component in &session.glyph.components {
                        append_component_path(
                            &mut glyph_path,
                            component,
                            &ws,
                            kurbo::Affine::IDENTITY,
                        );
                    }
                } else {
                    let ws = read_workspace(workspace);
                    if let Some(glyph) = ws.glyphs.get(name) {
                        glyph_path =
                            crate::model::glyph_renderer::glyph_to_bezpath_with_components(
                                glyph, &ws,
                            );
                    }
                }

                let translated =
                    kurbo::Affine::translate((x_offset, 0.0)) * glyph_path;
                glyph_paths.push(translated);

                if !is_rtl {
                    x_offset += advance_width;
                }

                prev_glyph_name = Some(name.clone());
                prev_glyph_group = read_workspace(workspace)
                    .get_glyph(name)
                    .and_then(|g| g.right_group.clone());
            }
            crate::sort::SortKind::LineBreak => {
                prev_glyph_name = None;
                prev_glyph_group = None;
            }
        }
    }

    let upm = session.ascender - session.descender;

    Either::A(
        sized_box(
            multi_glyph_view(
                glyph_paths, 10000.0, 10000.0, upm,
            )
            .color(theme::panel::GLYPH_PREVIEW)
            .fit_to_bounds(),
        )
        .background_color(theme::panel::BACKGROUND),
    )
}

/// Helper function to append component paths to a BezPath
///
/// Recursively resolves component references and applies transforms.
fn append_component_path(
    path: &mut BezPath,
    component: &crate::model::workspace::Component,
    workspace: &crate::model::workspace::Workspace,
    parent_transform: kurbo::Affine,
) {
    // Look up the base glyph
    let base_glyph = match workspace.glyphs.get(&component.base) {
        Some(g) => g,
        None => return,
    };

    // Combine transforms
    let combined_transform = parent_transform * component.transform;

    // Add contours from base glyph
    for contour in &base_glyph.contours {
        let contour_path = crate::path::Path::from_contour(contour);
        let transformed = combined_transform * contour_path.to_bezpath();
        path.extend(transformed);
    }

    // Recursively add nested components
    for nested_component in &base_glyph.components {
        append_component_path(path, nested_component, workspace, combined_transform);
    }
}

// ===== Transform Panel Dispatch =====

/// Apply a transform action from the transform panel
fn apply_transform(
    state: &mut AppState,
    action: TransformAction,
) {
    let Some(session) = &mut state.editor_session else {
        return;
    };

    match action {
        TransformAction::FlipH => {
            session.flip_selection_horizontal();
        }
        TransformAction::FlipV => {
            session.flip_selection_vertical();
        }
        TransformAction::RotateCW => {
            session.rotate_selection(-90.0);
        }
        TransformAction::RotateCCW => {
            session.rotate_selection(90.0);
        }
        TransformAction::Duplicate => {
            session.duplicate_selection();
        }
        TransformAction::DuplicateRepeat => {
            session.duplicate_selection();
            if let Some(affine) = session.last_transform {
                session.transform_selection(affine);
            }
        }
        TransformAction::Union => {
            session.boolean_op(linesweeper::BinaryOp::Union);
        }
        TransformAction::Subtract => {
            session.boolean_op(
                linesweeper::BinaryOp::Difference,
            );
        }
        TransformAction::Intersect => {
            session.boolean_op(
                linesweeper::BinaryOp::Intersection,
            );
        }
        TransformAction::Exclude => {
            session.boolean_op(linesweeper::BinaryOp::Xor);
        }
    }

    session.sync_to_workspace();
}

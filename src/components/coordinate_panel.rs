// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Coordinate panel widget - displays and allows editing of point coordinates
//!
//! This widget shows the x, y, width, and height of the current selection,
//! and includes a quadrant picker to choose which corner/edge to use as the
//! reference point for multi-point selections.

use crate::quadrant::Quadrant;
use kurbo::{Circle, Point, Rect};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx,
    Widget,
};
use masonry::kurbo::Size;
use masonry::properties::types::{AsUnit, MainAxisAlignment};
use masonry::vello::Scene;
use tracing;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, sized_box};

// Import from theme (includes all sizing and color constants)
use crate::theme::coordinate_panel::*;

// ============================================================================
// LAYOUT CONSTANTS
// ============================================================================
//
// All layout dimensions are defined here for easy editing.
// Change these values to adjust spacing, sizes, and positioning.

mod layout {
    /// Overall panel dimensions
    pub const PANEL_WIDTH: f64 = 240.0;
    pub const PANEL_HEIGHT: f64 = 140.0;

    /// Quadrant selector (3x3 grid picker) size
    /// Coordinate inputs total ~100px (48 + 4 + 48), so reducing quadrant
    /// size helps balance the visual weight. Try: 88.0, 86.0, 84.0
    pub const QUADRANT_SIZE: f64 = 80.0;

    /// Text input field dimensions
    pub const INPUT_WIDTH: f64 = 48.0;

    /// Spacing between elements
    pub const GAP_BETWEEN_INPUTS: f64 = 4.0; // Horizontal gap in input rows
    pub const GAP_BETWEEN_ROWS: f64 = 8.0; // Vertical gap between X/Y and W/H rows
    pub const GAP_BETWEEN_SECTIONS: f64 = 6.0; // Gap between quadrant and inputs

    /// Padding around the content inside the panel
    pub const CONTENT_PADDING: f64 = 8.0;

    /// Panel styling
    pub const BORDER_WIDTH: f64 = 1.5;
    pub const CORNER_RADIUS: f64 = 8.0;
}

// ============================================================================
// LAYOUT BUILDING FUNCTIONS
// ============================================================================

/// Coordinate data extracted from the session
#[derive(Clone)]
struct CoordinateData {
    x: String,
    y: String,
    width: String,
    height: String,
}

/// Extract and format coordinate values from the selection
fn prepare_coordinate_data(coordinate_selection: &CoordinateSelection) -> CoordinateData {
    if coordinate_selection.count == 0 {
        return CoordinateData {
            x: String::new(),
            y: String::new(),
            width: String::new(),
            height: String::new(),
        };
    }

    let reference_point = coordinate_selection.reference_point();
    let x = format!("{:.0}", reference_point.x);
    let y = format!("{:.0}", reference_point.y);

    // Width and height only shown when multiple points are selected
    let width = if coordinate_selection.count > 1 {
        format!("{:.0}", coordinate_selection.width())
    } else {
        String::new()
    };

    let height = if coordinate_selection.count > 1 {
        format!("{:.0}", coordinate_selection.height())
    } else {
        String::new()
    };

    CoordinateData {
        x,
        y,
        width,
        height,
    }
}

/// Build the quadrant selector widget (3x3 grid picker)
fn build_quadrant_selector<State: 'static, F>(
    session: Arc<crate::edit_session::EditSession>,
    on_session_update: F,
) -> impl WidgetView<State>
where
    F: Fn(&mut State, crate::edit_session::EditSession) + Send + Sync + 'static,
{
    sized_box(coordinate_panel_view(session, on_session_update))
        .width(layout::QUADRANT_SIZE.px())
        .height(layout::QUADRANT_SIZE.px())
}

/// Build a single coordinate input field
fn build_coord_input<State: 'static>(value: String, placeholder: &str) -> impl WidgetView<State> {
    sized_box(
        xilem::view::text_input(value, |_state: &mut State, _new_value| {
            // TODO: Handle coordinate updates
        })
        .text_alignment(parley::Alignment::Center)
        .placeholder(placeholder),
    )
    .width(layout::INPUT_WIDTH.px())
}

/// Build the coordinate input section (X, Y, W, H fields)
///
/// Creates two rows:
/// - Row 1: X and Y inputs
/// - Row 2: W and H inputs
fn build_coordinate_inputs<State: 'static>(data: CoordinateData) -> impl WidgetView<State> {
    // Row 1: X and Y position inputs
    let row1 = flex_row((
        build_coord_input(data.x, "X"),
        build_coord_input(data.y, "Y"),
    ))
    .gap(layout::GAP_BETWEEN_INPUTS.px())
    .cross_axis_alignment(CrossAxisAlignment::Center);

    // Row 2: Width and Height inputs
    let row2 = flex_row((
        build_coord_input(data.width, "W"),
        build_coord_input(data.height, "H"),
    ))
    .gap(layout::GAP_BETWEEN_INPUTS.px())
    .cross_axis_alignment(CrossAxisAlignment::Center);

    // Stack the two rows vertically
    flex_col((row1, row2))
        .gap(layout::GAP_BETWEEN_ROWS.px())
        .main_axis_alignment(MainAxisAlignment::Center)
        .cross_axis_alignment(CrossAxisAlignment::End)
}

/// Build the final panel container with background, border, and layout
///
/// Arranges: [Quadrant Selector] [Coordinate Inputs]
/// Content is centered both horizontally and vertically within the panel.
fn build_panel_container<State: 'static>(
    quadrant_selector: impl WidgetView<State>,
    coordinate_inputs: impl WidgetView<State>,
) -> impl WidgetView<State> {
    // Main horizontal layout: quadrant | inputs
    let row = flex_row((quadrant_selector, coordinate_inputs))
        .main_axis_alignment(MainAxisAlignment::Center)
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .gap(layout::GAP_BETWEEN_SECTIONS.px());

    // Center the row both horizontally and vertically within the panel
    let centered_content = flex_col((row,))
        .main_axis_alignment(MainAxisAlignment::Center)
        .cross_axis_alignment(CrossAxisAlignment::Center);

    // Apply panel styling, dimensions, and padding
    sized_box(centered_content)
        .width(layout::PANEL_WIDTH.px())
        .height(layout::PANEL_HEIGHT.px())
        .padding(layout::CONTENT_PADDING)
        .background_color(crate::theme::panel::BACKGROUND)
        .border_color(crate::theme::panel::OUTLINE)
        .border_width(layout::BORDER_WIDTH)
        .corner_radius(layout::CORNER_RADIUS)
}

/// Complete coordinate info panel with quadrant picker and editable inputs
///
/// Layout structure:
/// ```
/// ┌─────────────────────────┐
/// │  ┌──────┐  ┌───┐ ┌───┐  │
/// │  │ 3x3  │  │ X │ │ Y │  │
/// │  │ grid │  └───┘ └───┘  │
/// │  │      │  ┌───┐ ┌───┐  │
/// │  │      │  │ W │ │ H │  │
/// │  └──────┘  └───┘ └───┘  │
/// └─────────────────────────┘
/// ```
pub fn coordinate_panel<State: 'static, F>(
    session: Arc<crate::edit_session::EditSession>,
    on_session_update: F,
) -> impl WidgetView<State>
where
    F: Fn(&mut State, crate::edit_session::EditSession) + Send + Sync + 'static,
{
    // Step 1: Prepare coordinate data (clone strings for use in closures)
    let coord_data = prepare_coordinate_data(&session.coord_selection);

    // Step 2: Build the quadrant selector
    let quadrant_selector = build_quadrant_selector(session, on_session_update);

    // Step 3: Build the coordinate input fields
    // Clone the data so it can be moved into the view closures
    let coordinate_inputs = build_coordinate_inputs::<State>(coord_data.clone());

    // Step 4: Assemble the final panel
    build_panel_container(quadrant_selector, coordinate_inputs)
}

// ============================================================================
// DATA MODEL
// ============================================================================

/// Coordinate selection information for displaying/editing point coordinates
///
/// This stores the bounding box of the current selection and which quadrant
/// to use as the reference point for coordinate display.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoordinateSelection {
    /// Number of points selected
    pub count: usize,
    /// Bounding box of the selection (in design space)
    pub frame: Rect,
    /// Which quadrant/anchor point to use for coordinate display
    pub quadrant: Quadrant,
}

impl CoordinateSelection {
    /// Create a new coordinate selection
    pub fn new(count: usize, frame: Rect, quadrant: Quadrant) -> Self {
        Self {
            count,
            frame,
            quadrant,
        }
    }

    /// Get the reference point based on the selected quadrant
    pub fn reference_point(&self) -> Point {
        self.quadrant.point_in_dspace_rect(self.frame)
    }

    /// Get the width of the selection
    pub fn width(&self) -> f64 {
        self.frame.width()
    }

    /// Get the height of the selection
    pub fn height(&self) -> f64 {
        self.frame.height()
    }
}

impl Default for CoordinateSelection {
    fn default() -> Self {
        Self {
            count: 0,
            frame: Rect::ZERO,
            quadrant: Quadrant::default(),
        }
    }
}

// ===== Widget =====

/// Coordinate panel widget
pub struct CoordinatePanelWidget {
    session: crate::edit_session::EditSession,
    /// Current widget size (updated during layout)
    widget_size: Size,
}

impl CoordinatePanelWidget {
    pub fn new(session: crate::edit_session::EditSession) -> Self {
        Self {
            session,
            widget_size: Size::ZERO,
        }
    }

    /// Get the bounds of the quadrant picker within the widget
    ///
    /// This calculates the selector size dynamically based on available space
    /// to ensure it fits with proper margins on all sides.
    fn quadrant_picker_bounds(&self) -> Rect {
        if self.widget_size.width == 0.0 || self.widget_size.height == 0.0 {
            // Widget hasn't been laid out yet, use default size
            return Rect::new(
                PADDING,
                PADDING,
                PADDING + SELECTOR_SIZE,
                PADDING + SELECTOR_SIZE,
            );
        }

        // Calculate available space after accounting for padding
        let available_width = self.widget_size.width - (PADDING * 2.0);
        let available_height = self.widget_size.height - (PADDING * 2.0);

        // Selector should be square, so use the smaller dimension
        let selector_size = available_width.min(available_height);

        // Center the selector vertically if there's extra vertical space
        let top = PADDING + ((available_height - selector_size) / 2.0).max(0.0);

        Rect::new(PADDING, top, PADDING + selector_size, top + selector_size)
    }

    /// Get the center point for a specific quadrant dot within the picker bounds
    fn quadrant_dot_center(&self, quadrant: Quadrant, bounds: Rect) -> Point {
        let center_x = bounds.center().x;
        let center_y = bounds.center().y;

        match quadrant {
            Quadrant::TopLeft => Point::new(bounds.min_x(), bounds.min_y()),
            Quadrant::Top => Point::new(center_x, bounds.min_y()),
            Quadrant::TopRight => Point::new(bounds.max_x(), bounds.min_y()),
            Quadrant::Left => Point::new(bounds.min_x(), center_y),
            Quadrant::Center => Point::new(center_x, center_y),
            Quadrant::Right => Point::new(bounds.max_x(), center_y),
            Quadrant::BottomLeft => Point::new(bounds.min_x(), bounds.max_y()),
            Quadrant::Bottom => Point::new(center_x, bounds.max_y()),
            Quadrant::BottomRight => Point::new(bounds.max_x(), bounds.max_y()),
        }
    }

    /// Calculate the dot radius based on the selector size
    ///
    /// This scales the dot radius proportionally to the selector size
    /// to maintain consistent appearance at different sizes.
    fn dot_radius(&self, bounds: Rect) -> f64 {
        let selector_size = bounds.width();
        // Scale dot radius based on selector size
        // Default is 8.0 for 64.0 selector (8/64 = 0.125 ratio)
        selector_size * (DOT_RADIUS / SELECTOR_SIZE)
    }

    /// Determine which quadrant (if any) a point is hovering over
    ///
    /// Uses grid-based hit detection (matching Runebender's approach):
    /// Divides the widget into a 3x3 grid and returns which zone was clicked.
    /// This eliminates overlapping hit areas and ensures every part of the
    /// widget is clickable.
    fn quadrant_at_point(&self, point: Point) -> Option<Quadrant> {
        // Use the FULL widget bounds for hit detection, not just the visual
        // picker bounds. The padding is just for visual spacing, not for
        // limiting clickability.
        let hit_bounds = Rect::from_origin_size(kurbo::Point::ZERO, self.widget_size);

        if !hit_bounds.contains(point) {
            return None;
        }

        // Use grid-based hit detection instead of circle-based.
        // This matches Runebender's approach and eliminates overlapping hit
        // areas.
        Some(Quadrant::for_point_in_bounds(point, hit_bounds))
    }
}

/// Action emitted by the coord panel widget when the quadrant is changed
#[derive(Debug, Clone)]
pub struct SessionUpdate {
    pub session: crate::edit_session::EditSession,
}

impl Widget for CoordinatePanelWidget {
    type Action = SessionUpdate;

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {
        // Leaf widget - no children
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
        // State updates handled externally
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        // Store the widget size so we can use it in paint
        self.widget_size = bc.constrain(Size::new(layout::PANEL_WIDTH, layout::PANEL_HEIGHT));
        self.widget_size
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        if let PointerEvent::Down(PointerButtonEvent {
            button: Some(PointerButton::Primary),
            state,
            ..
        }) = event
        {
            let local_pos = ctx.local_position(state.position);
            tracing::debug!("Pointer down at local_pos: {:?}", local_pos);
            if let Some(quadrant) = self.quadrant_at_point(local_pos) {
                tracing::debug!(
                    "Clicked on quadrant: {:?}, old: {:?}",
                    quadrant,
                    self.session.coord_selection.quadrant
                );

                // Update the session's quadrant selection
                self.session.coord_selection.quadrant = quadrant;

                // Emit SessionUpdate action
                ctx.submit_action::<SessionUpdate>(SessionUpdate {
                    session: self.session.clone(),
                });

                // Request a repaint to show the new selected quadrant
                ctx.request_render();
            } else {
                tracing::debug!("Click was not on any quadrant dot");
                ctx.request_render();
            }
        }
    }

    fn paint(&mut self, _ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        // Background and border are now handled by the sized_box wrapper in
        // lib.rs. This widget only paints the quadrant picker. Coordinate text
        // values are handled by Xilem views in lib.rs.

        // Always show quadrant picker (user can select quadrant even without
        // points selected)
        self.paint_quadrant_picker(scene);
    }

    fn accessibility_role(&self) -> Role {
        Role::Group
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        _node: &mut Node,
    ) {
        // Could add accessibility info for coordinate display
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

impl CoordinatePanelWidget {
    /// Paint the quadrant picker (3x3 grid of dots)
    fn paint_quadrant_picker(&self, scene: &mut Scene) {
        let bounds = self.quadrant_picker_bounds();
        let dot_radius = self.dot_radius(bounds);

        // Draw frame around picker using theme stroke width
        masonry::util::stroke(scene, &bounds, GRID_LINE, STROKE_WIDTH);

        // Draw grid lines (horizontal and vertical lines forming 3x3 grid)
        let center_x = bounds.center().x;
        let center_y = bounds.center().y;

        // Horizontal lines
        let h_line_top = kurbo::Line::new(
            kurbo::Point::new(bounds.min_x(), bounds.min_y()),
            kurbo::Point::new(bounds.max_x(), bounds.min_y()),
        );
        let h_line_middle = kurbo::Line::new(
            kurbo::Point::new(bounds.min_x(), center_y),
            kurbo::Point::new(bounds.max_x(), center_y),
        );
        let h_line_bottom = kurbo::Line::new(
            kurbo::Point::new(bounds.min_x(), bounds.max_y()),
            kurbo::Point::new(bounds.max_x(), bounds.max_y()),
        );

        // Vertical lines
        let v_line_left = kurbo::Line::new(
            kurbo::Point::new(bounds.min_x(), bounds.min_y()),
            kurbo::Point::new(bounds.min_x(), bounds.max_y()),
        );
        let v_line_middle = kurbo::Line::new(
            kurbo::Point::new(center_x, bounds.min_y()),
            kurbo::Point::new(center_x, bounds.max_y()),
        );
        let v_line_right = kurbo::Line::new(
            kurbo::Point::new(bounds.max_x(), bounds.min_y()),
            kurbo::Point::new(bounds.max_x(), bounds.max_y()),
        );

        // Draw all grid lines using theme stroke width
        let grid_lines = [
            &h_line_top,
            &h_line_middle,
            &h_line_bottom,
            &v_line_left,
            &v_line_middle,
            &v_line_right,
        ];
        for line in grid_lines {
            masonry::util::stroke(scene, line, GRID_LINE, STROKE_WIDTH);
        }

        // Draw all 9 quadrant dots with two-tone style like editor points
        for quadrant in &[
            Quadrant::TopLeft,
            Quadrant::Top,
            Quadrant::TopRight,
            Quadrant::Left,
            Quadrant::Center,
            Quadrant::Right,
            Quadrant::BottomLeft,
            Quadrant::Bottom,
            Quadrant::BottomRight,
        ] {
            let center = self.quadrant_dot_center(*quadrant, bounds);
            let is_selected = *quadrant == self.session.coord_selection.quadrant;

            let (inner_color, outer_color) = if is_selected {
                (DOT_SELECTED_INNER, DOT_SELECTED_OUTER)
            } else {
                (DOT_UNSELECTED_INNER, DOT_UNSELECTED_OUTER)
            };

            // Draw two-tone filled circles to simulate outlined circles
            // Outer circle - use calculated dot radius
            let outer_circle = Circle::new(center, dot_radius);
            masonry::util::fill_color(scene, &outer_circle, outer_color);

            // Inner circle - make the "outline" match the container border
            // width (1.5px) by subtracting 1.5 from the radius
            let inner_radius = (dot_radius - 1.5).max(0.0);
            let inner_circle = Circle::new(center, inner_radius);
            masonry::util::fill_color(scene, &inner_circle, inner_color);
        }
    }
}

// ===== Xilem View Wrapper =====

use std::marker::PhantomData;
use std::sync::Arc;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

/// Create a coordinate panel view from an EditSession
pub fn coordinate_panel_view<State, F>(
    session: Arc<crate::edit_session::EditSession>,
    on_session_update: F,
) -> CoordinatePanelView<State, F>
where
    F: Fn(&mut State, crate::edit_session::EditSession) + Send + Sync + 'static,
{
    CoordinatePanelView {
        session,
        on_session_update,
        phantom: PhantomData,
    }
}

/// The Xilem View for CoordinatePanelWidget
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct CoordinatePanelView<State, F> {
    session: Arc<crate::edit_session::EditSession>,
    on_session_update: F,
    phantom: PhantomData<fn() -> State>,
}

impl<State, F> ViewMarker for CoordinatePanelView<State, F> {}

// Xilem View trait implementation
impl<State: 'static, F: Fn(&mut State, crate::edit_session::EditSession) + Send + Sync + 'static>
    View<State, (), ViewCtx> for CoordinatePanelView<State, F>
{
    type Element = Pod<CoordinatePanelWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _app_state: &mut State) -> (Self::Element, Self::ViewState) {
        let widget = CoordinatePanelWidget::new((*self.session).clone());
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
        // Update the widget's session if it changed.
        // We compare Arc pointers - if they're different, the session was
        // updated.
        if !Arc::ptr_eq(&self.session, &prev.session) {
            tracing::debug!(
                "Session Arc changed, old quadrant: {:?}, new: {:?}",
                prev.session.coord_selection.quadrant,
                self.session.coord_selection.quadrant
            );

            // Get mutable access to the widget and update the session
            let mut widget = element.downcast::<CoordinatePanelWidget>();
            widget.widget.session = (*self.session).clone();
            widget.ctx.request_render();
        }
    }

    fn teardown(
        &self,
        _view_state: &mut Self::ViewState,
        _ctx: &mut ViewCtx,
        _element: Mut<'_, Self::Element>,
    ) {
        // No cleanup needed
    }

    fn message(
        &self,
        _view_state: &mut Self::ViewState,
        message: &mut MessageContext,
        _element: Mut<'_, Self::Element>,
        app_state: &mut State,
    ) -> MessageResult<()> {
        // Handle SessionUpdate messages from the widget
        match message.take_message::<SessionUpdate>() {
            Some(update) => {
                tracing::debug!(
                    "Handling SessionUpdate, quadrant={:?}",
                    update.session.coord_selection.quadrant
                );
                (self.on_session_update)(app_state, update.session);
                // Use RequestRebuild instead of Action to avoid destroying the
                // window
                MessageResult::RequestRebuild
            }
            None => MessageResult::Stale,
        }
    }
}

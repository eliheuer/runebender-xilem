// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Coordinate panel widget - displays and allows editing of point coordinates
//!
//! This widget shows the x, y, width, and height of the current selection,
//! and includes a quadrant picker to choose which corner/edge to use as the
//! reference point for multi-point selections.

use crate::quadrant::Quadrant;
use crate::theme;
use kurbo::{Circle, Point, Rect};
use tracing;
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx, PaintCtx,
    PointerButton, PointerButtonEvent, PointerEvent, PropertiesMut, PropertiesRef,
    RegisterCtx, Update, UpdateCtx, Widget,
};
use masonry::kurbo::Size;
use masonry::vello::Scene;

/// Local constants for the coordinate panel, maybe move to settings?
const PANEL_WIDTH: f64 = 240.0;
const PANEL_HEIGHT: f64 = 100.0;

// Import from theme (includes all sizing and color constants)
use crate::theme::coordinate_panel::*;

// ===== Data Model =====

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
        let hit_bounds =
            Rect::from_origin_size(kurbo::Point::ZERO, self.widget_size);

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
        self.widget_size = bc.constrain(Size::new(PANEL_WIDTH, PANEL_HEIGHT));
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
            tracing::debug!(
                "Pointer down at local_pos: {:?}",
                local_pos
            );
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

    fn paint(
        &mut self,
        _ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        scene: &mut Scene,
    ) {
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
            &h_line_top, &h_line_middle, &h_line_bottom,
            &v_line_left, &v_line_middle, &v_line_right,
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
    F: Fn(&mut State, crate::edit_session::EditSession)
        + Send
        + Sync
        + 'static,
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
impl<
        State: 'static,
        F: Fn(&mut State, crate::edit_session::EditSession)
            + Send
            + Sync
            + 'static,
    > View<State, (), ViewCtx> for CoordinatePanelView<State, F>
{
    type Element = Pod<CoordinatePanelWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
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

// ===== Complete Coordinate Panel View =====

use masonry::properties::types::{AsUnit, MainAxisAlignment};
use xilem::style::Style;
use xilem::view::{CrossAxisAlignment, flex_col, flex_row, label, sized_box};
use xilem::WidgetView;

/// Complete coordinate info panel with quadrant picker and coordinate labels
///
/// This is the main entry point for displaying the coordinate panel in the
/// editor window. It combines the quadrant picker widget with coordinate text
/// labels.
pub fn coordinate_panel<State: 'static, F>(
    session: Arc<crate::edit_session::EditSession>,
    on_session_update: F,
) -> impl WidgetView<State>
where
    F: Fn(&mut State, crate::edit_session::EditSession)
        + Send
        + Sync
        + 'static,
{
    let coord_sel = session.coord_selection;

    // Calculate coordinate values based on the selection
    let (x_text, y_text, w_text, h_text) = if coord_sel.count == 0 {
        (
            "—".to_string(),
            "—".to_string(),
            "—".to_string(),
            "—".to_string(),
        )
    } else {
        let pt = coord_sel.reference_point();
        let x = format!("{:.0}", pt.x);
        let y = format!("{:.0}", pt.y);

        // Width and height only shown when multiple points are selected
        let w = if coord_sel.count > 1 {
            format!("{:.0}", coord_sel.width())
        } else {
            "—".to_string()
        };
        let h = if coord_sel.count > 1 {
            format!("{:.0}", coord_sel.height())
        } else {
            "—".to_string()
        };
        (x, y, w, h)
    };

    // Helper function to create styled coordinate labels
    let coord_label = |text: String| {
        label(text)
            .text_size(18.0)
            .text_alignment(parley::Alignment::Start)
            .color(theme::text::PRIMARY)
    };

    let quadrant_selector = sized_box(
        coordinate_panel_view(session, on_session_update)
    ).width(104.px());

    let coord_values = flex_col((
        coord_label(format!("x: {:<6}", x_text)),
        coord_label(format!("y: {:<6}", y_text)),
        coord_label(format!("w: {:<6}", w_text)),
        coord_label(format!("h: {:<6}", h_text)),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .gap(0.px());

    sized_box(
        flex_row((quadrant_selector, coord_values))
            .main_axis_alignment(MainAxisAlignment::Start)
            .gap(0.px()),
    )
    .width(166.px())
    .height(116.px())
    .padding(8.0)
    .background_color(crate::theme::panel::BACKGROUND)
    .border_color(crate::theme::panel::OUTLINE)
    .border_width(1.5)
    .corner_radius(8.0)
}
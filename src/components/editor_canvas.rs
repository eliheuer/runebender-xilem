// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph editor canvas widget - the main canvas for editing glyphs

use crate::edit_session::EditSession;
use crate::edit_types::EditType;
use crate::mouse::Mouse;
use crate::point::PointType;
use crate::settings;
use crate::sort::TextCursor;
use crate::theme;
use crate::undo::UndoState;
use kurbo::{Affine, Circle, Point, Rect as KurboRect, Stroke};
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent,
    PointerUpdate, PropertiesMut, PropertiesRef, RegisterCtx,
    TextEvent, Update, UpdateCtx, Widget,
};
use masonry::kurbo::Size;
use masonry::util::fill_color;
use masonry::vello::Scene;
use masonry::vello::peniko::Brush;
use std::sync::Arc;
use tracing;

/// The main glyph editor canvas widget
pub struct EditorWidget {
    /// The editing session (mutable copy for editing)
    pub session: EditSession,

    /// Mouse state machine
    mouse: Mouse,

    /// Canvas size
    size: Size,

    /// Undo/redo state
    undo: UndoState<EditSession>,

    /// The last edit type (for grouping consecutive edits)
    last_edit_type: Option<EditType>,

    /// Tool to return to when spacebar is released
    /// (for temporary preview mode)
    previous_tool: Option<crate::tools::ToolId>,

    /// Frame counter for throttling preview updates during drag
    ///
    /// PERFORMANCE OPTIMIZATION: Emitting SessionUpdate on every
    /// mouse move during drag causes significant lag because each
    /// update triggers a full Xilem view rebuild, which includes
    /// cloning the entire EditSession, running app_logic(), and
    /// rebuilding the preview pane's BezPath. By throttling to
    /// every Nth frame (currently every 3rd), we achieve a 67%
    /// reduction in rebuilds while maintaining smooth visual
    /// feedback. The main canvas still redraws every frame - only
    /// the expensive Xilem rebuild is throttled.
    drag_update_counter: u32,

    /// Text cursor for text editing mode
    text_cursor: TextCursor,
}

impl EditorWidget {
    /// Create a new editor widget
    pub fn new(session: Arc<EditSession>) -> Self {
        // Clone the session to get a mutable copy
        // This is cheap due to Arc-based fields
        Self {
            session: (*session).clone(),
            mouse: Mouse::new(),
            size: Size::new(800.0, 600.0),
            undo: UndoState::new(),
            last_edit_type: None,
            previous_tool: None,
            drag_update_counter: 0,
            text_cursor: TextCursor::new(),
        }
    }

    /// Set the canvas size
    #[allow(dead_code)]
    pub fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// Record an edit operation for undo
    ///
    /// This manages undo grouping:
    /// - If the edit type matches the last edit, update the
    ///   current undo group
    /// - If the edit type is different, create a new undo group
    fn record_edit(&mut self, edit_type: EditType) {
        match self.last_edit_type {
            Some(last) if last == edit_type => {
                // Same edit type - update current undo group
                self.undo.update_current_undo(self.session.clone());
            }
            _ => {
                // Different edit type or first edit - create new
                // undo group
                self.undo.add_undo_group(self.session.clone());
                self.last_edit_type = Some(edit_type);
            }
        }
    }

    /// Undo the last edit
    fn undo(&mut self) {
        if let Some(previous) = self.undo.undo(self.session.clone()) {
            self.session = previous;
            tracing::debug!("Undo: restored previous state");
        }
    }

    /// Redo the last undone edit
    fn redo(&mut self) {
        if let Some(next) = self.undo.redo(self.session.clone()) {
            self.session = next;
            tracing::debug!("Redo: restored next state");
        }
    }

    /// Convert hyperbezier paths to cubic bezier paths
    ///
    /// If points are selected, converts only paths containing selected points.
    /// If no points are selected, converts all hyperbezier paths in the glyph.
    ///
    /// Returns true if any paths were converted.
    fn convert_selected_hyper_to_cubic(&mut self) -> bool {
        use crate::path::Path;
        use std::sync::Arc;

        let mut converted = false;
        let has_selection = !self.session.selection.is_empty();

        // Clone paths, convert hyperbezier paths to cubic
        let mut new_paths = (*self.session.paths).clone();

        for path in &mut new_paths {
            let should_convert = if has_selection {
                // If points are selected, only convert paths with selected points
                match path {
                    Path::Hyper(hyper) => hyper.points().iter().any(|pt| {
                        self.session.selection.contains(&pt.id)
                    }),
                    _ => false,
                }
            } else {
                // If nothing selected, convert all hyperbezier paths
                matches!(path, Path::Hyper(_))
            };

            // Convert if needed
            if should_convert {
                if let Path::Hyper(hyper) = path {
                    *path = Path::Cubic(hyper.to_cubic());
                    converted = true;
                    tracing::info!("Converted hyperbezier path to cubic");
                }
            }
        }

        if converted {
            // Update paths with converted versions
            self.session.paths = Arc::new(new_paths);

            // Clear selection since point IDs will have changed
            self.session.selection = crate::selection::Selection::new();
        }

        converted
    }
}

/// Action emitted by the editor widget when the session is updated
#[derive(Debug, Clone)]
pub struct SessionUpdate {
    pub session: EditSession,
    /// If true, save the current state to disk
    pub save_requested: bool,
}

impl Widget for EditorWidget {
    type Action = SessionUpdate;

    fn accepts_focus(&self) -> bool {
        // Allow this widget to receive keyboard events
        true
    }

    fn register_children(&mut self, _ctx: &mut RegisterCtx<'_>) {
        // Leaf widget - no children
    }

    fn update(
        &mut self,
        _ctx: &mut UpdateCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        _event: &Update,
    ) {
        // TODO: Handle updates to the session
    }

    fn layout(
        &mut self,
        _ctx: &mut LayoutCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        bc: &BoxConstraints,
    ) -> Size {
        // Use all available space (expand to fill the window)
        let size = bc.max();
        self.size = size;
        size
    }

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        _props: &PropertiesRef<'_>,
        scene: &mut Scene,
    ) {
        let canvas_size = ctx.size();

        // Fill background
        let bg_rect = canvas_size.to_rect();
        fill_color(scene, &bg_rect, crate::theme::canvas::BACKGROUND);

        // Get the glyph outline from the editable paths
        let mut glyph_path = kurbo::BezPath::new();
        for path in self.session.paths.iter() {
            glyph_path.extend(path.to_bezpath());
        }

        // Initialize viewport on first paint
        if !self.session.viewport_initialized {
            self.initialize_viewport(canvas_size);
        }

        // Build transform from viewport (always uses current zoom/offset)
        let transform = self.session.viewport.affine();

        // Check if we're in preview mode (Preview tool is active)
        let is_preview_mode =
            self.session.current_tool.id() == crate::tools::ToolId::Preview;

        // Phase 3: Check if we have a text buffer (always show it if it exists)
        if self.session.text_buffer.is_some() {
            // Text buffer rendering: render multiple sorts
            // This is shown regardless of which tool is active
            self.render_text_buffer(scene, &transform, is_preview_mode);
            return;
        }

        // Traditional single-glyph rendering (only when no text buffer exists)
        if !is_preview_mode {
            // Edit mode: Draw font metrics guides
            draw_metrics_guides(
                scene,
                &transform,
                &self.session,
                canvas_size,
            );
        }

        if glyph_path.is_empty() {
            return;
        }

        // Apply transform to path
        let transformed_path = transform * &glyph_path;

        if is_preview_mode {
            // Preview mode: Fill the glyph with light gray
            // (visible on dark theme)
            let fill_brush = Brush::Solid(theme::path::PREVIEW_FILL);
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                &fill_brush,
                None,
                &transformed_path,
            );
        } else {
            // Edit mode: Draw the glyph outline with stroke
            let stroke = Stroke::new(theme::size::PATH_STROKE_WIDTH);
            let brush = Brush::Solid(theme::path::STROKE);
            scene.stroke(
                &stroke,
                Affine::IDENTITY,
                &brush,
                None,
                &transformed_path,
            );

            // Draw control point lines and points
            draw_paths_with_points(scene, &self.session, &transform);

            // Draw tool overlays (e.g., selection rectangle for
            // marquee). Temporarily take ownership of the tool to
            // call paint (requires &mut)
            let mut tool = std::mem::replace(
                &mut self.session.current_tool,
                crate::tools::ToolBox::for_id(
                    crate::tools::ToolId::Select,
                ),
            );
            tool.paint(scene, &self.session, &transform);
            self.session.current_tool = tool;
        }
    }

    fn on_pointer_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &PointerEvent,
    ) {
        // Always request focus on any pointer event so keyboard shortcuts work
        ctx.request_focus();

        match event {
            PointerEvent::Down(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                self.handle_pointer_down(ctx, state);
            }

            PointerEvent::Move(PointerUpdate { current, .. }) => {
                self.handle_pointer_move(ctx, current);
            }

            PointerEvent::Up(PointerButtonEvent {
                button: Some(PointerButton::Primary),
                state,
                ..
            }) => {
                self.handle_pointer_up(ctx, state);
            }

            PointerEvent::Cancel(_) => {
                self.handle_pointer_cancel(ctx);
            }

            _ => {
                // TODO: Implement wheel event handling once Masonry
                // exposes it. For now, zooming can be done via
                // keyboard shortcuts or commands
            }
        }
    }

    fn on_text_event(
        &mut self,
        ctx: &mut EventCtx<'_>,
        _props: &mut PropertiesMut<'_>,
        event: &TextEvent,
    ) {
        use masonry::core::keyboard::{Key, KeyState};

        if let TextEvent::Keyboard(key_event) = event {
            tracing::debug!(
                "[EditorWidget::on_text_event] key: {:?}, state: {:?}",
                key_event.key,
                key_event.state
            );

            // Handle shift key for shape constraining
            if let Key::Named(masonry::core::keyboard::NamedKey::Shift) = key_event.key {
                let shift_pressed = key_event.state == KeyState::Down;
                if let crate::tools::ToolBox::Shapes(shapes_tool) = &mut self.session.current_tool {
                    shapes_tool.set_shift_locked(shift_pressed);
                    ctx.request_render(); // Repaint to update preview
                }
            }

            // Handle spacebar for temporary preview mode
            if self.handle_spacebar(ctx, key_event) {
                return;
            }

            // Only handle key down events for other keys
            if key_event.state != KeyState::Down {
                return;
            }

            // Check for keyboard shortcuts
            let cmd = key_event.modifiers.meta()
                || key_event.modifiers.ctrl();
            let shift = key_event.modifiers.shift();

            // Debug logging for key events
            if cmd {
                tracing::info!(
                    "[EditorWidget] Cmd+Key: {:?}, cmd={}, shift={}",
                    key_event.key,
                    cmd,
                    shift
                );
            }

            // Handle keyboard shortcuts first (before text input)
            // This allows Cmd+Z, Cmd+-, Cmd+= etc. to work in text mode
            if self.handle_keyboard_shortcuts(
                ctx,
                &key_event.key,
                cmd,
                shift,
            ) {
                return;
            }

            // Phase 5: Handle text mode input (character typing, cursor movement)
            // Only handle after shortcuts, and only if no modifiers (except shift for caps)
            if self.session.text_mode_active && self.session.text_buffer.is_some() {
                if self.handle_text_mode_input(ctx, &key_event.key, cmd) {
                    return;
                }
            }

            // Handle arrow keys for nudging
            self.handle_arrow_keys(ctx, &key_event.key, shift, cmd);
        }
    }

    fn accessibility_role(&self) -> Role {
        Role::Canvas
    }

    fn accessibility(
        &mut self,
        _ctx: &mut AccessCtx<'_>,
        _props: &PropertiesRef<'_>,
        node: &mut Node,
    ) {
        node.set_label(format!(
            "Editing glyph: {}",
            self.session.glyph_name
        ));
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

impl EditorWidget {
    /// Initialize viewport positioning to center the glyph
    fn initialize_viewport(&mut self, canvas_size: Size) {
        let ascender = self.session.ascender;
        let descender = self.session.descender;

        // Calculate the visible height in design space
        let design_height = ascender - descender;

        // Center the viewport on the canvas
        let center_x = canvas_size.width / 2.0;
        let center_y = canvas_size.height / 2.0;

        // Create a transform that:
        // 1. Scales to fit the canvas (with some padding)
        // 2. Centers the glyph
        let padding = 0.8; // Leave 20% padding
        let scale = (canvas_size.height * padding) / design_height;

        // Center point in design space (middle of advance width,
        // middle of height)
        let design_center_x = self.session.glyph.width / 2.0;
        let design_center_y = (ascender + descender) / 2.0;

        // Update the viewport to match our rendering transform
        // The viewport uses: zoom (scale) and offset (translation
        // after scale)
        self.session.viewport.zoom = scale;
        // Offset calculation based on to_screen formula:
        // screen.x = design.x * zoom + offset.x
        // screen.y = -design.y * zoom + offset.y
        // For design_center to map to canvas_center:
        self.session.viewport.offset = kurbo::Vec2::new(
            center_x - design_center_x * scale,
            center_y + design_center_y * scale, // Y is flipped
        );

        self.session.viewport_initialized = true;
    }

    /// Render the text buffer with multiple sorts (Phase 3)
    ///
    /// This renders all sorts in the text buffer, laying them out horizontally
    /// with correct spacing based on advance widths.
    fn render_text_buffer(
        &self,
        scene: &mut Scene,
        transform: &Affine,
        is_preview_mode: bool,
    ) {
        let buffer = match &self.session.text_buffer {
            Some(buf) => buf,
            None => return,
        };

        let mut x_offset = 0.0;
        let baseline_y = 0.0;
        let cursor_position = buffer.cursor();

        // Phase 6: Calculate cursor x position while rendering sorts
        let mut cursor_x = 0.0;

        for (index, sort) in buffer.iter().enumerate() {
            // Track cursor position
            if index == cursor_position {
                cursor_x = x_offset;
            }

            match &sort.kind {
                crate::sort::SortKind::Glyph { name, advance_width, .. } => {
                    let sort_position = Point::new(x_offset, baseline_y);

                    // Draw metrics box for this sort
                    if !is_preview_mode {
                        self.render_sort_metrics(scene, x_offset, *advance_width, transform);
                    }

                    if sort.is_active && !is_preview_mode {
                        // Render active sort with control points (editable)
                        self.render_active_sort(scene, name, sort_position, transform);
                    } else {
                        // Render inactive sort as filled preview
                        self.render_inactive_sort(scene, name, sort_position, transform);
                    }

                    x_offset += advance_width;
                }
                crate::sort::SortKind::LineBreak => {
                    // Line break: reset x, move y down
                    x_offset = 0.0;
                    // baseline_y -= self.session.line_height(); // TODO: multi-line support
                }
            }
        }

        // Cursor might be at the end of the buffer
        if cursor_position >= buffer.len() {
            cursor_x = x_offset;
        }

        // Phase 6: Render cursor in text mode (not in preview mode)
        if !is_preview_mode {
            self.render_text_cursor(scene, cursor_x, baseline_y, transform);
        }
    }

    /// Render an active sort with control points and handles
    fn render_active_sort(
        &self,
        scene: &mut Scene,
        glyph_name: &str,
        position: Point,
        transform: &Affine,
    ) {
        // Load glyph paths from workspace
        // For now, we'll use the current session paths if the glyph name matches
        // TODO Phase 8: Load paths for any glyph by name
        if glyph_name == self.session.glyph_name {
            // Apply position offset
            let sort_transform = *transform * Affine::translate(position.to_vec2());

            // Render path stroke
            let mut glyph_path = kurbo::BezPath::new();
            for path in self.session.paths.iter() {
                glyph_path.extend(path.to_bezpath());
            }

            if !glyph_path.is_empty() {
                let transformed_path = sort_transform * &glyph_path;
                let stroke = Stroke::new(theme::size::PATH_STROKE_WIDTH);
                let brush = Brush::Solid(theme::path::STROKE);
                scene.stroke(
                    &stroke,
                    Affine::IDENTITY,
                    &brush,
                    None,
                    &transformed_path,
                );

                // Draw control points and handles
                // Note: This uses session paths which already have the correct structure
                draw_paths_with_points(scene, &self.session, &sort_transform);
            }
        }
    }

    /// Render an inactive sort as a filled preview
    fn render_inactive_sort(
        &self,
        scene: &mut Scene,
        glyph_name: &str,
        position: Point,
        transform: &Affine,
    ) {
        // Load glyph from workspace and render as filled
        let workspace = match &self.session.workspace {
            Some(ws) => ws,
            None => return,
        };

        let glyph = match workspace.glyphs.get(glyph_name) {
            Some(g) => g,
            None => {
                tracing::warn!("Glyph '{}' not found in workspace", glyph_name);
                return;
            }
        };

        // Apply position offset
        let sort_transform = *transform * Affine::translate(position.to_vec2());

        // Build BezPath from glyph contours
        let mut glyph_path = kurbo::BezPath::new();
        for contour in &glyph.contours {
            let path = crate::path::Path::from_contour(contour);
            glyph_path.extend(path.to_bezpath());
        }

        if !glyph_path.is_empty() {
            let transformed_path = sort_transform * &glyph_path;
            let fill_brush = Brush::Solid(theme::path::PREVIEW_FILL);
            scene.fill(
                peniko::Fill::NonZero,
                Affine::IDENTITY,
                &fill_brush,
                None,
                &transformed_path,
            );
        }
    }

    /// Render the text cursor (Phase 6)
    ///
    /// Draws a vertical line at the cursor position in design space, aligned with sort metrics
    /// Only visible in text edit mode. Includes triangular indicators at top and bottom.
    fn render_text_cursor(
        &self,
        scene: &mut Scene,
        cursor_x: f64,
        _baseline_y: f64,
        transform: &Affine,
    ) {
        // Only render cursor in text edit mode
        if !self.session.text_mode_active {
            return;
        }

        // Draw cursor as a vertical line from ascender to descender (matching sort metrics)
        let cursor_top = Point::new(cursor_x, self.session.ascender);
        let cursor_bottom = Point::new(cursor_x, self.session.descender);

        // Transform to screen coordinates
        let cursor_top_screen = *transform * cursor_top;
        let cursor_bottom_screen = *transform * cursor_bottom;

        let cursor_line = kurbo::Line::new(cursor_top_screen, cursor_bottom_screen);

        // Use orange color (same as selection marquee) with 1.5px stroke
        let stroke = Stroke::new(1.5);
        let brush = Brush::Solid(theme::selection::RECT_STROKE);

        scene.stroke(
            &stroke,
            Affine::IDENTITY,
            &brush,
            None,
            &cursor_line,
        );

        // Draw triangular indicators at top and bottom (like Glyphs app)
        // Triangle size in screen space - slightly smaller than 4x
        let triangle_width = 24.0;
        let triangle_height = 16.0;

        // Top triangle (pointing down/inward, aligned with ascender)
        // Base at ascender, tip extends downward into the metrics box
        let mut top_triangle = kurbo::BezPath::new();
        top_triangle.move_to((cursor_top_screen.x - triangle_width / 2.0, cursor_top_screen.y)); // Left corner at ascender
        top_triangle.line_to((cursor_top_screen.x + triangle_width / 2.0, cursor_top_screen.y)); // Right corner at ascender
        top_triangle.line_to((cursor_top_screen.x, cursor_top_screen.y + triangle_height)); // Tip below, pointing down
        top_triangle.close_path();

        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            &brush,
            None,
            &top_triangle,
        );

        // Bottom triangle (pointing up/inward, aligned with descender)
        // Base at descender, tip extends upward into the metrics box
        let mut bottom_triangle = kurbo::BezPath::new();
        bottom_triangle.move_to((cursor_bottom_screen.x - triangle_width / 2.0, cursor_bottom_screen.y)); // Left corner at descender
        bottom_triangle.line_to((cursor_bottom_screen.x + triangle_width / 2.0, cursor_bottom_screen.y)); // Right corner at descender
        bottom_triangle.line_to((cursor_bottom_screen.x, cursor_bottom_screen.y - triangle_height)); // Tip above, pointing up
        bottom_triangle.close_path();

        scene.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            &brush,
            None,
            &bottom_triangle,
        );
    }

    /// Render metrics box for a single sort (Phase 6)
    ///
    /// This draws the bounding rectangle that defines the sort.
    /// Shows the advance width, baseline, ascender, descender, and font metrics.
    fn render_sort_metrics(
        &self,
        scene: &mut Scene,
        x_offset: f64,
        advance_width: f64,
        transform: &Affine,
    ) {
        let stroke = Stroke::new(theme::size::METRIC_LINE_WIDTH);
        let brush = Brush::Solid(theme::metrics::GUIDE);

        // Draw vertical lines (left and right edges of the sort)
        let left_top = Point::new(x_offset, self.session.ascender);
        let left_bottom = Point::new(x_offset, self.session.descender);
        let left_line = kurbo::Line::new(
            *transform * left_top,
            *transform * left_bottom,
        );
        scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &left_line);

        let right_top = Point::new(x_offset + advance_width, self.session.ascender);
        let right_bottom = Point::new(x_offset + advance_width, self.session.descender);
        let right_line = kurbo::Line::new(
            *transform * right_top,
            *transform * right_bottom,
        );
        scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &right_line);

        // Draw horizontal lines (baseline, ascender, descender, etc.)
        let draw_hline = |scene: &mut Scene, y: f64| {
            let start = Point::new(x_offset, y);
            let end = Point::new(x_offset + advance_width, y);
            let line = kurbo::Line::new(*transform * start, *transform * end);
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &line);
        };

        // Descender (bottom of metrics box)
        draw_hline(scene, self.session.descender);

        // Baseline (y=0)
        draw_hline(scene, 0.0);

        // X-height (if available)
        if let Some(x_height) = self.session.x_height {
            draw_hline(scene, x_height);
        }

        // Cap-height (if available)
        if let Some(cap_height) = self.session.cap_height {
            draw_hline(scene, cap_height);
        }

        // Ascender (top of metrics box)
        draw_hline(scene, self.session.ascender);
    }

    /// Handle pointer down event
    fn handle_pointer_down(
        &mut self,
        ctx: &mut EventCtx<'_>,
        state: &masonry::core::PointerState,
    ) {
        use crate::mouse::{MouseButton, MouseEvent, Modifiers};
        use crate::tools::{ToolBox, ToolId};

        tracing::debug!(
            "[EditorWidget::on_pointer_event] Down at {:?}, \
             current_tool: {:?}",
            state.position,
            self.session.current_tool.id()
        );

        // Request focus to receive keyboard events
        tracing::debug!("[EditorWidget] Requesting focus!");
        ctx.request_focus();

        // Capture pointer to receive drag events
        ctx.capture_pointer();

        let local_pos = ctx.local_position(state.position);

        // Extract modifier keys from pointer state
        // state.modifiers is keyboard_types::Modifiers from
        // ui-events crate
        let mods = Modifiers {
            shift: state.modifiers.shift(),
            ctrl: state.modifiers.ctrl(),
            alt: state.modifiers.alt(),
            meta: state.modifiers.meta(),
        };

        // Create MouseEvent for our mouse state machine
        let mouse_event = MouseEvent::with_modifiers(
            local_pos,
            Some(MouseButton::Left),
            mods,
        );

        // Temporarily take ownership of the tool to avoid borrow
        // conflicts
        let mut tool = std::mem::replace(
            &mut self.session.current_tool,
            ToolBox::for_id(ToolId::Select),
        );
        self.mouse
            .mouse_down(mouse_event, &mut tool, &mut self.session);
        self.session.current_tool = tool;

        ctx.request_render();
    }

    /// Handle pointer move event
    fn handle_pointer_move(
        &mut self,
        ctx: &mut EventCtx<'_>,
        current: &masonry::core::PointerState,
    ) {
        use crate::mouse::MouseEvent;
        use crate::tools::{ToolBox, ToolId};

        // Request focus when mouse moves over canvas so keyboard
        // shortcuts (zoom, etc.) work even after clicking toolbar
        ctx.request_focus();

        let local_pos = ctx.local_position(current.position);

        // Create MouseEvent
        let mouse_event = MouseEvent::new(local_pos, None);

        // Temporarily take ownership of the tool
        let mut tool = std::mem::replace(
            &mut self.session.current_tool,
            ToolBox::for_id(ToolId::Select),
        );
        self.mouse
            .mouse_moved(mouse_event, &mut tool, &mut self.session);
        self.session.current_tool = tool;

        // Request render during drag OR when pen tool needs hover
        // feedback
        let needs_render =
            ctx.is_active() || self.session.current_tool.id() == ToolId::Pen;
        if needs_render {
            ctx.request_render();
        }

        // PERFORMANCE: Emit SessionUpdate during active drag so
        // preview pane updates in real-time BUT throttle to every
        // Nth frame to avoid excessive Xilem view rebuilds. This
        // provides smooth preview updates without killing
        // performance. Adjust
        // settings::performance::DRAG_UPDATE_THROTTLE to tune
        // responsiveness vs performance.
        if ctx.is_active() {
            self.drag_update_counter += 1;
            let throttle = settings::performance::DRAG_UPDATE_THROTTLE;
            if self.drag_update_counter.is_multiple_of(throttle) {
                // Update coordinate selection before emitting update
                self.session.update_coord_selection();

                ctx.submit_action::<SessionUpdate>(SessionUpdate {
                    session: self.session.clone(),
                    save_requested: false,
                });
            }
        }
    }

    /// Handle pointer up event
    fn handle_pointer_up(
        &mut self,
        ctx: &mut EventCtx<'_>,
        state: &masonry::core::PointerState,
    ) {
        use crate::mouse::{MouseButton, MouseEvent, Modifiers};
        use crate::tools::{ToolBox, ToolId};

        let local_pos = ctx.local_position(state.position);

        // Extract modifier keys from pointer state
        let mods = Modifiers {
            shift: state.modifiers.shift(),
            ctrl: state.modifiers.ctrl(),
            alt: state.modifiers.alt(),
            meta: state.modifiers.meta(),
        };

        // Create MouseEvent with modifiers
        let mouse_event = MouseEvent::with_modifiers(
            local_pos,
            Some(MouseButton::Left),
            mods,
        );

        // Temporarily take ownership of the tool
        let mut tool = std::mem::replace(
            &mut self.session.current_tool,
            ToolBox::for_id(ToolId::Select),
        );
        self.mouse
            .mouse_up(mouse_event, &mut tool, &mut self.session);

        // Record undo if an edit occurred
        if let Some(edit_type) = tool.edit_type() {
            self.record_edit(edit_type);
        }

        self.session.current_tool = tool;

        // Update coordinate selection after tool operation
        self.session.update_coord_selection();

        // Reset drag update counter for next drag operation
        self.drag_update_counter = 0;

        // Emit action to notify view of session changes
        ctx.submit_action::<SessionUpdate>(SessionUpdate {
            session: self.session.clone(),
            save_requested: false,
        });

        ctx.release_pointer();
        ctx.request_render();
    }

    /// Handle pointer cancel event
    fn handle_pointer_cancel(&mut self, ctx: &mut EventCtx<'_>) {
        use crate::tools::{ToolBox, ToolId};

        // Temporarily take ownership of the tool
        let mut tool = std::mem::replace(
            &mut self.session.current_tool,
            ToolBox::for_id(ToolId::Select),
        );
        self.mouse.cancel(&mut tool, &mut self.session);
        self.session.current_tool = tool;

        ctx.request_render();
    }

    /// Handle spacebar for temporary preview mode
    fn handle_spacebar(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key_event: &masonry::core::keyboard::KeyboardEvent,
    ) -> bool {
        use masonry::core::keyboard::{Key, KeyState};

        if !matches!(&key_event.key, Key::Character(c) if c == " ") {
            return false;
        }

        tracing::debug!(
            "[EditorWidget] Spacebar detected! state: {:?}, \
             previous_tool: {:?}",
            key_event.state,
            self.previous_tool
        );

        if key_event.state == KeyState::Down
            && self.previous_tool.is_none()
        {
            // Spacebar pressed: save current tool and switch to
            // Preview
            let current_tool = self.session.current_tool.id();
            if current_tool != crate::tools::ToolId::Preview {
                self.previous_tool = Some(current_tool);

                // Cancel the current tool and reset mouse state
                // (like Runebender)
                use crate::tools::ToolBox;
                let mut tool = std::mem::replace(
                    &mut self.session.current_tool,
                    ToolBox::for_id(crate::tools::ToolId::Select),
                );
                self.mouse.cancel(&mut tool, &mut self.session);

                // Reset mouse state by creating new instance
                self.mouse = Mouse::new();

                // Switch to Preview tool
                self.session.current_tool =
                    ToolBox::for_id(crate::tools::ToolId::Preview);

                tracing::debug!(
                    "Spacebar down: switched to Preview, will \
                     return to {:?}",
                    current_tool
                );

                // Emit SessionUpdate so the toolbar reflects the
                // change
                ctx.submit_action::<SessionUpdate>(SessionUpdate {
                    session: self.session.clone(),
                    save_requested: false,
                });

                ctx.request_render();
                ctx.set_handled();
            }
            return true;
        } else if key_event.state == KeyState::Up
            && self.previous_tool.is_some()
        {
            // Spacebar released: return to previous tool
            if let Some(previous) = self.previous_tool.take() {
                // Reset mouse state by creating new instance
                self.mouse = Mouse::new();

                self.session.current_tool =
                    crate::tools::ToolBox::for_id(previous);
                tracing::debug!("Spacebar up: returned to {:?}", previous);

                // Emit SessionUpdate so the toolbar reflects the
                // change
                ctx.submit_action::<SessionUpdate>(SessionUpdate {
                    session: self.session.clone(),
                    save_requested: false,
                });

                ctx.request_render();
                ctx.set_handled();
            }
            return true;
        }

        false
    }

    /// Handle keyboard shortcuts (undo, redo, zoom, save, etc.)
    fn handle_keyboard_shortcuts(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &masonry::core::keyboard::Key,
        cmd: bool,
        shift: bool,
    ) -> bool {
        use masonry::core::keyboard::{Key, NamedKey};

        // Undo/Redo
        if cmd && matches!(key, Key::Character(c) if c == "z") {
            if shift {
                // Cmd+Shift+Z = Redo
                self.redo();
            } else {
                // Cmd+Z = Undo
                self.undo();
            }
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        // Zoom in (Cmd/Ctrl + or =)
        if cmd {
            let is_zoom_in = matches!(key, Key::Character(c) if c == "+" || c == "=");
            if is_zoom_in {
                let new_zoom = (self.session.viewport.zoom * 1.1)
                    .min(settings::editor::MAX_ZOOM);
                self.session.viewport.zoom = new_zoom;
                tracing::info!("Zoom in: new zoom = {:.2}", new_zoom);
                ctx.request_render();
                ctx.set_handled();
                return true;
            }
        }

        // Zoom out (Cmd/Ctrl -)
        if cmd {
            let is_zoom_out = matches!(key, Key::Character(c) if c == "-" || c == "_");
            if is_zoom_out {
                let new_zoom = (self.session.viewport.zoom / 1.1)
                    .max(settings::editor::MIN_ZOOM);
                self.session.viewport.zoom = new_zoom;
                tracing::info!("Zoom out: new zoom = {:.2}", new_zoom);
                ctx.request_render();
                ctx.set_handled();
                return true;
            }
        }

        // Fit to window (Cmd/Ctrl+0)
        if cmd && matches!(key, Key::Character(c) if c == "0") {
            // Reset viewport to fit glyph in window
            self.session.viewport_initialized = false;
            tracing::debug!("Fit to window: resetting viewport");
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        // Convert hyperbezier to cubic (Cmd/Ctrl+Shift+H)
        if cmd && shift && matches!(key, Key::Character(c) if c.eq_ignore_ascii_case("h")) {
            tracing::info!("Cmd+Shift+H pressed - attempting to convert hyperbezier to cubic");
            if self.convert_selected_hyper_to_cubic() {
                tracing::info!("Successfully converted hyperbezier paths to cubic");
                ctx.request_render();
                ctx.set_handled();
                return true;
            } else {
                tracing::warn!("No hyperbezier paths to convert");
            }
        }

        // Save (Cmd/Ctrl+S)
        if cmd && matches!(key, Key::Character(c) if c == "s") {
            // Emit save request action
            ctx.submit_action::<SessionUpdate>(SessionUpdate {
                session: self.session.clone(),
                save_requested: true,
            });
            ctx.set_handled();
            return true;
        }

        // Delete selected points (Backspace or Delete key)
        if matches!(
            key,
            Key::Named(NamedKey::Backspace) | Key::Named(NamedKey::Delete)
        ) {
            self.session.delete_selection();
            self.record_edit(EditType::Normal);
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        // Toggle point type (T key)
        if matches!(key, Key::Character(c) if c == "t") {
            self.session.toggle_point_type();
            self.record_edit(EditType::Normal);
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        // Reverse contours (R key)
        if matches!(key, Key::Character(c) if c == "r") {
            self.session.reverse_contours();
            self.record_edit(EditType::Normal);
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        // Tool switching shortcuts (without modifiers)
        if !cmd && !shift {
            let new_tool = match key {
                Key::Character(c) if c == "v" => {
                    Some(crate::tools::ToolId::Select)
                }
                Key::Character(c) if c == "p" => {
                    Some(crate::tools::ToolId::Pen)
                }
                Key::Character(c) if c == "h" => {
                    Some(crate::tools::ToolId::HyperPen)
                }
                Key::Character(c) if c == "k" => {
                    Some(crate::tools::ToolId::Knife)
                }
                _ => None,
            };

            if let Some(tool_id) = new_tool {
                // Cancel current tool
                let mut tool = std::mem::replace(
                    &mut self.session.current_tool,
                    crate::tools::ToolBox::for_id(
                        crate::tools::ToolId::Select,
                    ),
                );
                self.mouse.cancel(&mut tool, &mut self.session);
                self.mouse = crate::mouse::Mouse::new();

                // Switch to new tool
                self.session.current_tool =
                    crate::tools::ToolBox::for_id(tool_id);

                // Notify toolbar of change
                ctx.submit_action::<SessionUpdate>(SessionUpdate {
                    session: self.session.clone(),
                    save_requested: false,
                });

                ctx.request_render();
                ctx.set_handled();
                return true;
            }
        }

        false
    }

    /// Handle arrow keys for nudging
    fn handle_arrow_keys(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &masonry::core::keyboard::Key,
        shift: bool,
        ctrl: bool,
    ) {
        use masonry::core::keyboard::{Key, NamedKey};

        let (dx, dy) = match key {
            Key::Named(NamedKey::ArrowLeft) => {
                tracing::debug!("Arrow Left pressed");
                (-1.0, 0.0)
            }
            Key::Named(NamedKey::ArrowRight) => {
                tracing::debug!("Arrow Right pressed");
                (1.0, 0.0)
            }
            Key::Named(NamedKey::ArrowUp) => {
                tracing::debug!("Arrow Up pressed");
                (0.0, 1.0) // Design space: Y increases upward
            }
            Key::Named(NamedKey::ArrowDown) => {
                tracing::debug!("Arrow Down pressed");
                (0.0, -1.0) // Design space: Y increases upward
            }
            _ => return,
        };

        tracing::debug!(
            "Nudging selection: dx={} dy={} shift={} ctrl={} \
             selection_len={}",
            dx,
            dy,
            shift,
            ctrl,
            self.session.selection.len()
        );

        self.session.nudge_selection(dx, dy, shift, ctrl);
        ctx.request_render();
        ctx.set_handled();
    }

    /// Handle text mode keyboard input (Phase 5)
    ///
    /// Handles:
    /// - Character typing (insert sorts)
    /// - Arrow keys (cursor movement in buffer)
    /// - Backspace/Delete (remove sorts)
    /// - Enter (line breaks)
    ///
    /// Returns true if the key was handled, false otherwise
    fn handle_text_mode_input(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &masonry::core::keyboard::Key,
        cmd: bool,
    ) -> bool {
        use masonry::core::keyboard::{Key, NamedKey};

        // Handle arrow keys for cursor movement
        match key {
            Key::Named(NamedKey::ArrowLeft) => {
                if let Some(buffer) = &mut self.session.text_buffer {
                    buffer.move_cursor_left();
                    self.text_cursor.reset(); // Reset cursor to visible on movement
                    ctx.request_render();
                    ctx.set_handled();
                    return true;
                }
            }
            Key::Named(NamedKey::ArrowRight) => {
                if let Some(buffer) = &mut self.session.text_buffer {
                    buffer.move_cursor_right();
                    self.text_cursor.reset(); // Reset cursor to visible on movement
                    ctx.request_render();
                    ctx.set_handled();
                    return true;
                }
            }
            Key::Named(NamedKey::Backspace) => {
                if let Some(buffer) = &mut self.session.text_buffer {
                    buffer.delete();
                    self.text_cursor.reset(); // Reset cursor to visible on edit
                    // Emit session update to persist text buffer changes
                    ctx.submit_action::<SessionUpdate>(SessionUpdate {
                        session: self.session.clone(),
                        save_requested: false,
                    });
                    ctx.request_render();
                    ctx.set_handled();
                    return true;
                }
            }
            Key::Named(NamedKey::Delete) => {
                if let Some(buffer) = &mut self.session.text_buffer {
                    buffer.delete_forward();
                    self.text_cursor.reset(); // Reset cursor to visible on edit
                    // Emit session update to persist text buffer changes
                    ctx.submit_action::<SessionUpdate>(SessionUpdate {
                        session: self.session.clone(),
                        save_requested: false,
                    });
                    ctx.request_render();
                    ctx.set_handled();
                    return true;
                }
            }
            Key::Named(NamedKey::Enter) => {
                // Insert line break as a sort
                if let Some(buffer) = &mut self.session.text_buffer {
                    use crate::sort::{LayoutMode, Sort, SortKind};

                    let line_break = Sort {
                        kind: SortKind::LineBreak,
                        is_active: false,
                        layout_mode: LayoutMode::LTR,
                        position: Point::ZERO,
                    };

                    buffer.insert(line_break);
                    self.text_cursor.reset(); // Reset cursor to visible on edit

                    // Emit session update to persist text buffer changes
                    ctx.submit_action::<SessionUpdate>(SessionUpdate {
                        session: self.session.clone(),
                        save_requested: false,
                    });
                    ctx.request_render();
                    ctx.set_handled();
                    return true;
                }
            }
            Key::Character(s) => {
                // Don't insert characters when Cmd/Ctrl is held (let shortcuts through)
                if cmd {
                    return false;
                }

                // Insert character as a sort
                if let Some(c) = s.chars().next() {
                    if let Some(sort) = self.session.create_sort_from_char(c) {
                        if let Some(buffer) = &mut self.session.text_buffer {
                            buffer.insert(sort);
                            self.text_cursor.reset(); // Reset cursor to visible on edit
                            // Emit session update to persist text buffer changes
                            ctx.submit_action::<SessionUpdate>(SessionUpdate {
                                session: self.session.clone(),
                                save_requested: false,
                            });
                            ctx.request_render();
                            ctx.set_handled();
                            return true;
                        }
                    } else {
                        tracing::warn!("No glyph found for character: '{}'", c);
                    }
                }
            }
            _ => {}
        }

        false
    }
}

/// Draw font metric guidelines
fn draw_metrics_guides(
    scene: &mut Scene,
    transform: &Affine,
    session: &EditSession,
    _canvas_size: Size,
) {
    let stroke = Stroke::new(theme::size::METRIC_LINE_WIDTH);
    let brush = Brush::Solid(theme::metrics::GUIDE);

    // Helper to draw a horizontal line at a given Y coordinate in
    // design space. Lines are contained within the metrics box
    // (from x=0 to x=advance_width)
    let draw_hline = |scene: &mut Scene, y: f64| {
        let start = Point::new(0.0, y);
        let end = Point::new(session.glyph.width, y);

        let start_screen = *transform * start;
        let end_screen = *transform * end;

        let line = kurbo::Line::new(start_screen, end_screen);
        scene.stroke(
            &stroke,
            Affine::IDENTITY,
            &brush,
            None,
            &line,
        );
    };

    // Helper to draw a vertical line at a given X coordinate in
    // design space. Lines are contained within the metrics box
    // (from y=descender to y=ascender)
    let draw_vline = |scene: &mut Scene, x: f64| {
        let start = Point::new(x, session.descender);
        let end = Point::new(x, session.ascender);

        let start_screen = *transform * start;
        let end_screen = *transform * end;

        let line = kurbo::Line::new(start_screen, end_screen);
        scene.stroke(
            &stroke,
            Affine::IDENTITY,
            &brush,
            None,
            &line,
        );
    };

    // Draw vertical lines (left and right edges of metrics box)
    draw_vline(scene, 0.0);
    draw_vline(scene, session.glyph.width);

    // Draw horizontal lines
    // Descender (bottom of metrics box)
    draw_hline(scene, session.descender);

    // Baseline (y=0)
    draw_hline(scene, 0.0);

    // X-height (if available)
    if let Some(x_height) = session.x_height {
        draw_hline(scene, x_height);
    }

    // Cap-height (if available)
    if let Some(cap_height) = session.cap_height {
        draw_hline(scene, cap_height);
    }

    // Ascender (top of metrics box)
    draw_hline(scene, session.ascender);
}

/// Draw paths with control point lines and styled points
fn draw_paths_with_points(
    scene: &mut Scene,
    session: &EditSession,
    transform: &Affine,
) {
    use crate::path::Path;

    // First pass: draw control point lines (handles)
    // In cubic bezier curves, handles connect on-curve points to
    // their adjacent off-curve control points
    for path in session.paths.iter() {
        match path {
            Path::Cubic(cubic) => {
                draw_control_handles(scene, cubic, transform);
            }
            Path::Quadratic(quadratic) => {
                draw_control_handles_quadratic(
                    scene,
                    quadratic,
                    transform,
                );
            }
            Path::Hyper(hyper) => {
                // Hyper paths use similar handle drawing to cubic
                draw_control_handles_hyper(scene, hyper, transform);
            }
        }
    }

    // Second pass: draw points
    for path in session.paths.iter() {
        match path {
            Path::Cubic(cubic) => {
                draw_points(scene, cubic, session, transform);
            }
            Path::Quadratic(quadratic) => {
                draw_points_quadratic(
                    scene,
                    quadratic,
                    session,
                    transform,
                );
            }
            Path::Hyper(hyper) => {
                // Hyper paths use similar point drawing to cubic
                draw_points_hyper(scene, hyper, session, transform);
            }
        }
    }
}

/// Draw control handles for a cubic path
fn draw_control_handles(
    scene: &mut Scene,
    cubic: &crate::cubic_path::CubicPath,
    transform: &Affine,
) {
    let points: Vec<_> = cubic.points.iter().collect();
    if points.is_empty() {
        return;
    }

    // For each point, if it's on-curve, draw handles to adjacent
    // off-curve points
    for i in 0..points.len() {
        let pt = points[i];

        if !pt.is_on_curve() {
            continue;
        }

        // Look at the next point (with wrapping for closed paths)
        let next_i = if i + 1 < points.len() {
            i + 1
        } else if cubic.closed {
            0
        } else {
            continue;
        };

        // Look at the previous point (with wrapping for closed
        // paths)
        let prev_i = if i > 0 {
            i - 1
        } else if cubic.closed {
            points.len() - 1
        } else {
            continue;
        };

        // Draw handle to next point if it's off-curve
        if next_i < points.len() && points[next_i].is_off_curve() {
            let start = *transform * pt.point;
            let end = *transform * points[next_i].point;
            let line = kurbo::Line::new(start, end);
            let stroke = Stroke::new(theme::size::HANDLE_LINE_WIDTH);
            let brush = Brush::Solid(theme::handle::LINE);
            scene.stroke(
                &stroke,
                Affine::IDENTITY,
                &brush,
                None,
                &line,
            );
        }

        // Draw handle to previous point if it's off-curve
        if prev_i < points.len() && points[prev_i].is_off_curve() {
            let start = *transform * pt.point;
            let end = *transform * points[prev_i].point;
            let line = kurbo::Line::new(start, end);
            let stroke = Stroke::new(theme::size::HANDLE_LINE_WIDTH);
            let brush = Brush::Solid(theme::handle::LINE);
            scene.stroke(
                &stroke,
                Affine::IDENTITY,
                &brush,
                None,
                &line,
            );
        }
    }
}

/// Draw points for a cubic path
fn draw_points(
    scene: &mut Scene,
    cubic: &crate::cubic_path::CubicPath,
    session: &EditSession,
    transform: &Affine,
) {
    for pt in cubic.points.iter() {
        let screen_pos = *transform * pt.point;
        let is_selected = session.selection.contains(&pt.id);

        match pt.typ {
            PointType::OnCurve { smooth } => {
                if smooth {
                    draw_smooth_point(scene, screen_pos, is_selected);
                } else {
                    draw_corner_point(scene, screen_pos, is_selected);
                }
            }
            PointType::OffCurve { .. } => {
                draw_offcurve_point(scene, screen_pos, is_selected);
            }
        }
    }
}

/// Draw a smooth on-curve point as a circle
fn draw_smooth_point(
    scene: &mut Scene,
    screen_pos: Point,
    is_selected: bool,
) {
    let radius = if is_selected {
        theme::size::SMOOTH_POINT_SELECTED_RADIUS
    } else {
        theme::size::SMOOTH_POINT_RADIUS
    };

    let (inner_color, outer_color) = if is_selected {
        (theme::point::SELECTED_INNER, theme::point::SELECTED_OUTER)
    } else {
        (theme::point::SMOOTH_INNER, theme::point::SMOOTH_OUTER)
    };

    // Outer circle (border)
    let outer_circle = Circle::new(screen_pos, radius + 1.0);
    fill_color(scene, &outer_circle, outer_color);

    // Inner circle
    let inner_circle = Circle::new(screen_pos, radius);
    fill_color(scene, &inner_circle, inner_color);
}

/// Draw a corner on-curve point as a square
fn draw_corner_point(
    scene: &mut Scene,
    screen_pos: Point,
    is_selected: bool,
) {
    let half_size = if is_selected {
        theme::size::CORNER_POINT_SELECTED_HALF_SIZE
    } else {
        theme::size::CORNER_POINT_HALF_SIZE
    };

    let (inner_color, outer_color) = if is_selected {
        (theme::point::SELECTED_INNER, theme::point::SELECTED_OUTER)
    } else {
        (theme::point::CORNER_INNER, theme::point::CORNER_OUTER)
    };

    // Outer square (border)
    let outer_rect = KurboRect::new(
        screen_pos.x - half_size - 1.0,
        screen_pos.y - half_size - 1.0,
        screen_pos.x + half_size + 1.0,
        screen_pos.y + half_size + 1.0,
    );
    fill_color(scene, &outer_rect, outer_color);

    // Inner square
    let inner_rect = KurboRect::new(
        screen_pos.x - half_size,
        screen_pos.y - half_size,
        screen_pos.x + half_size,
        screen_pos.y + half_size,
    );
    fill_color(scene, &inner_rect, inner_color);
}

/// Draw an off-curve point as a small circle
fn draw_offcurve_point(
    scene: &mut Scene,
    screen_pos: Point,
    is_selected: bool,
) {
    let radius = if is_selected {
        theme::size::OFFCURVE_POINT_SELECTED_RADIUS
    } else {
        theme::size::OFFCURVE_POINT_RADIUS
    };

    let (inner_color, outer_color) = if is_selected {
        (theme::point::SELECTED_INNER, theme::point::SELECTED_OUTER)
    } else {
        (theme::point::OFFCURVE_INNER, theme::point::OFFCURVE_OUTER)
    };

    // Outer circle (border)
    let outer_circle = Circle::new(screen_pos, radius + 1.0);
    fill_color(scene, &outer_circle, outer_color);

    // Inner circle
    let inner_circle = Circle::new(screen_pos, radius);
    fill_color(scene, &inner_circle, inner_color);
}

/// Draw a hyperbezier on-curve point as a circle (cyan/teal color)
fn draw_hyper_point(
    scene: &mut Scene,
    screen_pos: Point,
    is_selected: bool,
) {
    let radius = if is_selected {
        theme::size::HYPER_POINT_SELECTED_RADIUS
    } else {
        theme::size::HYPER_POINT_RADIUS
    };

    let (inner_color, outer_color) = if is_selected {
        (theme::point::SELECTED_INNER, theme::point::SELECTED_OUTER)
    } else {
        (theme::point::HYPER_INNER, theme::point::HYPER_OUTER)
    };

    // Outer circle (border)
    let outer_circle = Circle::new(screen_pos, radius + 1.0);
    fill_color(scene, &outer_circle, outer_color);

    // Inner circle
    let inner_circle = Circle::new(screen_pos, radius);
    fill_color(scene, &inner_circle, inner_color);
}

/// Draw control handles for a quadratic path
fn draw_control_handles_quadratic(
    scene: &mut Scene,
    quadratic: &crate::quadratic_path::QuadraticPath,
    transform: &Affine,
) {
    let points: Vec<_> = quadratic.points.iter().collect();
    if points.is_empty() {
        return;
    }

    // For each point, if it's on-curve, draw handles to adjacent
    // off-curve points
    for i in 0..points.len() {
        let pt = points[i];

        if !pt.is_on_curve() {
            continue;
        }

        // Look at the next point (with wrapping for closed paths)
        let next_i = if i + 1 < points.len() {
            i + 1
        } else if quadratic.closed {
            0
        } else {
            continue;
        };

        // Look at the previous point (with wrapping for closed
        // paths)
        let prev_i = if i > 0 {
            i - 1
        } else if quadratic.closed {
            points.len() - 1
        } else {
            continue;
        };

        // Draw handle to next point if it's off-curve
        if next_i < points.len() && points[next_i].is_off_curve() {
            let start = *transform * pt.point;
            let end = *transform * points[next_i].point;
            let line = kurbo::Line::new(start, end);
            let stroke = Stroke::new(theme::size::HANDLE_LINE_WIDTH);
            let brush = Brush::Solid(theme::handle::LINE);
            scene.stroke(
                &stroke,
                Affine::IDENTITY,
                &brush,
                None,
                &line,
            );
        }

        // Draw handle to previous point if it's off-curve
        if prev_i < points.len() && points[prev_i].is_off_curve() {
            let start = *transform * pt.point;
            let end = *transform * points[prev_i].point;
            let line = kurbo::Line::new(start, end);
            let stroke = Stroke::new(theme::size::HANDLE_LINE_WIDTH);
            let brush = Brush::Solid(theme::handle::LINE);
            scene.stroke(
                &stroke,
                Affine::IDENTITY,
                &brush,
                None,
                &line,
            );
        }
    }
}

/// Draw points for a quadratic path
fn draw_points_quadratic(
    scene: &mut Scene,
    quadratic: &crate::quadratic_path::QuadraticPath,
    session: &EditSession,
    transform: &Affine,
) {
    for pt in quadratic.points.iter() {
        let screen_pos = *transform * pt.point;
        let is_selected = session.selection.contains(&pt.id);

        match pt.typ {
            PointType::OnCurve { smooth } => {
                if smooth {
                    draw_smooth_point(scene, screen_pos, is_selected);
                } else {
                    draw_corner_point(scene, screen_pos, is_selected);
                }
            }
            PointType::OffCurve { .. } => {
                draw_offcurve_point(scene, screen_pos, is_selected);
            }
        }
    }
}

/// Draw control handles for a hyper path
fn draw_control_handles_hyper(
    scene: &mut Scene,
    hyper: &crate::hyper_path::HyperPath,
    transform: &Affine,
) {
    let points: Vec<_> = hyper.points.iter().collect();
    if points.is_empty() {
        return;
    }

    // For each point, if it's on-curve, draw handles to adjacent
    // off-curve points
    for i in 0..points.len() {
        let pt = points[i];

        if !pt.is_on_curve() {
            continue;
        }

        // Look at the next point (with wrapping for closed paths)
        let next_i = if i + 1 < points.len() {
            i + 1
        } else if hyper.closed {
            0
        } else {
            continue;
        };

        // Look at the previous point (with wrapping for closed paths)
        let prev_i = if i > 0 {
            i - 1
        } else if hyper.closed {
            points.len() - 1
        } else {
            continue;
        };

        // Draw handle to next point if it's off-curve
        if next_i < points.len() && points[next_i].is_off_curve() {
            let start = *transform * pt.point;
            let end = *transform * points[next_i].point;
            let line = kurbo::Line::new(start, end);
            let stroke = Stroke::new(theme::size::HANDLE_LINE_WIDTH);
            let brush = Brush::Solid(theme::handle::LINE);
            scene.stroke(
                &stroke,
                Affine::IDENTITY,
                &brush,
                None,
                &line,
            );
        }

        // Draw handle to previous point if it's off-curve
        if prev_i < points.len() && points[prev_i].is_off_curve() {
            let start = *transform * pt.point;
            let end = *transform * points[prev_i].point;
            let line = kurbo::Line::new(start, end);
            let stroke = Stroke::new(theme::size::HANDLE_LINE_WIDTH);
            let brush = Brush::Solid(theme::handle::LINE);
            scene.stroke(
                &stroke,
                Affine::IDENTITY,
                &brush,
                None,
                &line,
            );
        }
    }
}

/// Draw points for a hyper path
fn draw_points_hyper(
    scene: &mut Scene,
    hyper: &crate::hyper_path::HyperPath,
    session: &EditSession,
    transform: &Affine,
) {
    for pt in hyper.points.iter() {
        let screen_pos = *transform * pt.point;
        let is_selected = session.selection.contains(&pt.id);

        match pt.typ {
            PointType::OnCurve { .. } => {
                // All hyperbezier on-curve points use the hyper point style
                draw_hyper_point(scene, screen_pos, is_selected);
            }
            PointType::OffCurve { .. } => {
                draw_offcurve_point(scene, screen_pos, is_selected);
            }
        }
    }
}

// ===== XILEM VIEW WRAPPER =====

use std::marker::PhantomData;
use xilem::core::{MessageContext, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx};

/// Create an editor view from an edit session with a callback for
/// session updates
///
/// The callback receives the updated session and a boolean indicating
/// whether save was requested (Cmd+S).
pub fn editor_view<State, F>(
    session: Arc<EditSession>,
    on_session_update: F,
) -> EditorView<State, F>
where
    F: Fn(&mut State, EditSession, bool),
{
    EditorView {
        session,
        on_session_update,
        phantom: PhantomData,
    }
}

/// The Xilem View for EditorWidget
#[must_use = "View values do nothing unless provided to Xilem."]
pub struct EditorView<State, F> {
    session: Arc<EditSession>,
    on_session_update: F,
    phantom: PhantomData<fn() -> State>,
}

impl<State, F> ViewMarker for EditorView<State, F> {}

impl<State: 'static, F: Fn(&mut State, EditSession, bool) + 'static>
    View<State, (), ViewCtx> for EditorView<State, F>
{
    type Element = Pod<EditorWidget>;
    type ViewState = ();

    fn build(
        &self,
        ctx: &mut ViewCtx,
        _app_state: &mut State,
    ) -> (Self::Element, Self::ViewState) {
        let widget = EditorWidget::new(self.session.clone());
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
        // Update the widget's session if it changed (e.g., tool
        // selection changed). We compare Arc pointers - if they're
        // different, the session was updated
        if !Arc::ptr_eq(&self.session, &prev.session) {
            tracing::debug!(
                "[EditorView::rebuild] Session Arc changed, \
                 updating widget"
            );
            tracing::debug!(
                "[EditorView::rebuild] Old tool: {:?}, New tool: \
                 {:?}",
                prev.session.current_tool.id(),
                self.session.current_tool.id()
            );

            // Get mutable access to the widget
            let mut widget = element.downcast::<EditorWidget>();

            // Update the session, but preserve:
            // - Mouse state (to avoid breaking active drag
            //   operations)
            // - Undo state
            // - Canvas size
            // This allows tool changes and other session updates to
            // take effect
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
                    "[EditorView::message] Handling SessionUpdate, \
                     calling callback, selection.len()={}, save_requested={}",
                    update.session.selection.len(),
                    update.save_requested
                );
                (self.on_session_update)(
                    app_state,
                    update.session,
                    update.save_requested,
                );
                tracing::debug!(
                    "[EditorView::message] Callback complete, \
                     returning Action(())"
                );
                // Return Action(()) to propagate to root and trigger
                // full app rebuild. This ensures all UI elements
                // (including coordinate pane) see the updated state
                MessageResult::Action(())
            }
            None => MessageResult::Stale,
        }
    }
}

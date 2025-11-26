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
    AccessCtx, BoxConstraints, BrushIndex, ChildrenIds, EventCtx, LayoutCtx,
    PaintCtx, PointerButton, PointerButtonEvent, PointerEvent,
    PointerScrollEvent, PointerUpdate, PropertiesMut, PropertiesRef,
    RegisterCtx, ScrollDelta, StyleProperty, TextEvent, Update, UpdateCtx, Widget,
    render_text,
};
use masonry::kurbo::Size;
use masonry::util::fill_color;
use masonry::vello::Scene;
use masonry::vello::peniko::Brush;
use parley::{FontContext, FontFamily, FontStack, GenericFamily, LayoutContext};
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

    /// Last click time for double-click detection
    last_click_time: Option<std::time::Instant>,

    /// Last click position for double-click detection
    last_click_position: Option<Point>,

    /// Manual kerning mode state
    kern_mode_active: bool,

    /// Index of the sort being kerned (dragged)
    kern_sort_index: Option<usize>,

    /// Starting X position when kern drag began
    kern_start_x: f64,

    /// Original kern value before drag started
    kern_original_value: f64,

    /// Current horizontal offset from start position during kern drag
    kern_current_offset: f64,
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
            last_click_time: None,
            last_click_position: None,
            kern_mode_active: false,
            kern_sort_index: None,
            kern_start_x: 0.0,
            kern_original_value: 0.0,
            kern_current_offset: 0.0,
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

            // Draw tool overlays (e.g., selection rectangle for marquee, pen preview)
            // Temporarily take ownership of the tool to call paint (requires &mut)
            if !is_preview_mode {
                let mut tool = std::mem::replace(
                    &mut self.session.current_tool,
                    crate::tools::ToolBox::for_id(
                        crate::tools::ToolId::Select,
                    ),
                );
                tool.paint(scene, &self.session, &transform);
                self.session.current_tool = tool;
            }

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

            PointerEvent::Scroll(PointerScrollEvent { delta, .. }) => {
                self.handle_scroll_zoom(ctx, delta);
            }

            _ => {
                // Ignore other pointer events
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
            let ctrl = key_event.modifiers.ctrl();

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
                ctrl,
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
        let glyph_label = self.session.active_sort_name
            .as_deref()
            .unwrap_or("(no active sort)");
        node.set_label(format!(
            "Editing glyph: {}",
            glyph_label
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
        let padding = 0.6; // Leave 40% padding (more zoomed out)
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

        // Check text direction for RTL support
        let is_rtl = self.session.text_direction.is_rtl();

        // For RTL: calculate total width first so we can start from the right
        let total_width = if is_rtl {
            self.calculate_buffer_width()
        } else {
            0.0 // Not needed for LTR
        };

        // Starting position: LTR starts at 0, RTL starts at total_width
        let mut x_offset = if is_rtl { total_width } else { 0.0 };
        let mut baseline_y = 0.0;
        let cursor_position = buffer.cursor();

        // UPM height for line spacing (top of UPM on new line aligns with bottom of descender on previous line)
        let upm_height = self.session.ascender - self.session.descender;

        // Phase 6: Calculate cursor position while rendering sorts
        let mut cursor_x = x_offset;
        let mut cursor_y = 0.0;

        // Track previous glyph for kerning lookup
        let mut prev_glyph_name: Option<String> = None;
        let mut prev_glyph_group: Option<String> = None;

        tracing::debug!("[Cursor] buffer.len()={}, cursor_position={}, is_rtl={}", buffer.len(), cursor_position, is_rtl);

        // Cursor at position 0 (before any sorts)
        if cursor_position == 0 {
            cursor_x = x_offset;
            cursor_y = baseline_y;
            tracing::debug!("[Cursor] Position 0: ({}, {})", cursor_x, cursor_y);
        }

        for (index, sort) in buffer.iter().enumerate() {
            match &sort.kind {
                crate::sort::SortKind::Glyph { name, advance_width, .. } => {
                    // For RTL: move x left BEFORE drawing this glyph
                    if is_rtl {
                        x_offset -= advance_width;
                    }

                    // Apply kerning if we have a previous glyph
                    if let Some(prev_name) = &prev_glyph_name {
                        if let Some(workspace_arc) = &self.session.workspace {
                            let workspace = workspace_arc.read().unwrap();

                            // Get current glyph's left kerning group
                            let curr_group = workspace.get_glyph(name)
                                .and_then(|g| g.left_group.as_ref().map(|s| s.as_str()));

                            // Look up kerning value
                            let kern_value = crate::kerning::lookup_kerning(
                                &workspace.kerning,
                                &workspace.groups,
                                prev_name,
                                prev_glyph_group.as_deref(),
                                name,
                                curr_group,
                            );

                            if is_rtl {
                                x_offset -= kern_value; // RTL: kerning moves left
                            } else {
                                x_offset += kern_value;
                            }
                        }
                    }

                    // Don't apply visual offset - kerning is already applied to x_offset
                    let sort_position = Point::new(x_offset, baseline_y);

                    // Draw metrics based on mode
                    if !is_preview_mode {
                        if self.session.text_mode_active {
                            // Determine metrics color based on kern mode
                            let metrics_color = if self.kern_mode_active {
                                if Some(index) == self.kern_sort_index {
                                    // Active dragged glyph: bright turquoise-green
                                    masonry::vello::peniko::Color::from_rgb8(0x00, 0xff, 0xcc)
                                } else if Some(index + 1) == self.kern_sort_index {
                                    // Previous glyph: orange (selection marquee color)
                                    masonry::vello::peniko::Color::from_rgb8(0xff, 0xaa, 0x33)
                                } else {
                                    // Normal gray
                                    theme::metrics::GUIDE
                                }
                            } else {
                                // Normal gray when not in kern mode
                                theme::metrics::GUIDE
                            };

                            // Text mode: minimal metrics for all sorts
                            // Use x_offset directly - kerning is already applied
                            self.render_sort_minimal_metrics(scene, x_offset, baseline_y, *advance_width, transform, metrics_color);
                        } else if sort.is_active {
                            // Non-text mode: full metrics only for active sort
                            self.render_sort_metrics(scene, x_offset, baseline_y, *advance_width, transform);
                        }
                        // Inactive sorts in non-text mode: no metrics at all
                    }

                    if sort.is_active && !is_preview_mode && !self.session.text_mode_active {
                        // Non-text mode: render active sort with control points (editable)
                        self.render_active_sort(scene, name, sort_position, transform);
                    } else {
                        // All other cases: render as filled preview
                        self.render_inactive_sort(scene, name, sort_position, transform);
                    }

                    // For LTR: advance x forward AFTER drawing
                    if !is_rtl {
                        x_offset += advance_width;
                    }
                    // (For RTL, we already moved x_offset before drawing)

                    // Update previous glyph info for next iteration
                    prev_glyph_name = Some(name.clone());
                    if let Some(workspace_arc) = &self.session.workspace {
                        let workspace = workspace_arc.read().unwrap();
                        prev_glyph_group = workspace.get_glyph(name)
                            .and_then(|g| g.right_group.clone());
                    }
                }
                crate::sort::SortKind::LineBreak => {
                    // Line break: reset x, move y down by UPM height
                    // Top of UPM on new line aligns with bottom of descender on previous line
                    x_offset = if is_rtl { total_width } else { 0.0 };
                    baseline_y -= upm_height;

                    // Reset kerning tracking (no kerning across lines)
                    prev_glyph_name = None;
                    prev_glyph_group = None;
                }
            }

            // Track cursor position AFTER processing this sort
            // The cursor is positioned after the sort at this index
            if index + 1 == cursor_position {
                cursor_x = x_offset;
                cursor_y = baseline_y;
                tracing::debug!("[Cursor] After sort {}: ({}, {})", index, cursor_x, cursor_y);
            }
        }

        // Cursor might be at the end of the buffer (after all sorts)
        if cursor_position >= buffer.len() {
            cursor_x = x_offset;
            cursor_y = baseline_y;
            tracing::debug!("[Cursor] At end: ({}, {})", cursor_x, cursor_y);
        }

        tracing::debug!("[Cursor] Final position: ({}, {})", cursor_x, cursor_y);

        // Phase 6: Render cursor in text mode (not in preview mode)
        if !is_preview_mode {
            self.render_text_cursor(scene, cursor_x, cursor_y, transform);
        }
    }

    /// Calculate the total width of the text buffer (for RTL rendering)
    ///
    /// This sums all glyph advance widths to determine where RTL text should start.
    fn calculate_buffer_width(&self) -> f64 {
        let buffer = match &self.session.text_buffer {
            Some(buf) => buf,
            None => return 0.0,
        };

        let mut total_width = 0.0;
        for sort in buffer.iter() {
            if let crate::sort::SortKind::Glyph { advance_width, .. } = &sort.kind {
                total_width += advance_width;
            }
        }
        total_width
    }

    /// Render an active sort with control points and handles
    fn render_active_sort(
        &self,
        scene: &mut Scene,
        _glyph_name: &str,
        position: Point,
        transform: &Affine,
    ) {
        // Render the active sort using session.paths (the editable version)
        // The caller already verified this is the active sort via sort.is_active

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

        // Render components for the active glyph
        // Components are rendered as filled shapes in a distinct color
        if let Some(workspace) = &self.session.workspace {
            let workspace_guard = workspace.read().unwrap();
            self.render_glyph_components(
                scene,
                &self.session.glyph,
                &sort_transform,
                &workspace_guard,
            );
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

        let workspace_guard = workspace.read().unwrap();
        let glyph = match workspace_guard.glyphs.get(glyph_name) {
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

        // Render components (references to other glyphs)
        self.render_glyph_components(scene, glyph, &sort_transform, &workspace_guard);
    }

    /// Render components of a glyph recursively
    ///
    /// Components are rendered in a distinct color and can be nested
    /// (a component's base glyph may itself contain components).
    /// For the active glyph, selected components get a highlight outline.
    fn render_glyph_components(
        &self,
        scene: &mut Scene,
        glyph: &crate::workspace::Glyph,
        transform: &Affine,
        workspace: &crate::workspace::Workspace,
    ) {
        for component in &glyph.components {
            // Look up the base glyph
            let base_glyph = match workspace.glyphs.get(&component.base) {
                Some(g) => g,
                None => {
                    tracing::warn!(
                        "Component base glyph '{}' not found in workspace",
                        component.base
                    );
                    continue;
                }
            };

            // Combine transform: parent transform * component transform
            let component_transform = *transform * component.transform;

            // Build BezPath from base glyph's contours
            let mut component_path = kurbo::BezPath::new();
            for contour in &base_glyph.contours {
                let path = crate::path::Path::from_contour(contour);
                component_path.extend(path.to_bezpath());
            }

            // Render the component in a distinct color
            if !component_path.is_empty() {
                let transformed_path = component_transform * &component_path;

                // Check if this component is selected (for the active glyph)
                let is_selected = self.session.selected_component == Some(component.id);

                // Use a brighter color if selected
                let fill_color = if is_selected {
                    // Brighter blue for selected component
                    peniko::Color::from_rgb8(0x88, 0xbb, 0xff)
                } else {
                    theme::component::FILL
                };

                let fill_brush = Brush::Solid(fill_color);
                scene.fill(
                    peniko::Fill::NonZero,
                    Affine::IDENTITY,
                    &fill_brush,
                    None,
                    &transformed_path,
                );

                // Draw selection outline if selected
                if is_selected {
                    let stroke = Stroke::new(2.0);
                    let stroke_brush = Brush::Solid(theme::selection::RECT_STROKE);
                    scene.stroke(
                        &stroke,
                        Affine::IDENTITY,
                        &stroke_brush,
                        None,
                        &transformed_path,
                    );
                }
            }

            // Recursively render nested components
            self.render_glyph_components(scene, base_glyph, &component_transform, workspace);
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
        baseline_y: f64,
        transform: &Affine,
    ) {
        // Only render cursor in text edit mode
        if !self.session.text_mode_active {
            return;
        }

        // Draw cursor as a vertical line from ascender to descender (matching sort metrics)
        // Offset by baseline_y to support multi-line text
        let cursor_top = Point::new(cursor_x, baseline_y + self.session.ascender);
        let cursor_bottom = Point::new(cursor_x, baseline_y + self.session.descender);

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
        baseline_y: f64,
        advance_width: f64,
        transform: &Affine,
    ) {
        let stroke = Stroke::new(theme::size::METRIC_LINE_WIDTH);
        let brush = Brush::Solid(theme::metrics::GUIDE);

        // Draw vertical lines (left and right edges of the sort)
        // Offset by baseline_y to support multi-line text
        let left_top = Point::new(x_offset, baseline_y + self.session.ascender);
        let left_bottom = Point::new(x_offset, baseline_y + self.session.descender);
        let left_line = kurbo::Line::new(
            *transform * left_top,
            *transform * left_bottom,
        );
        scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &left_line);

        let right_top = Point::new(x_offset + advance_width, baseline_y + self.session.ascender);
        let right_bottom = Point::new(x_offset + advance_width, baseline_y + self.session.descender);
        let right_line = kurbo::Line::new(
            *transform * right_top,
            *transform * right_bottom,
        );
        scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &right_line);

        // Draw horizontal lines (baseline, ascender, descender, etc.)
        // Offset by baseline_y to support multi-line text
        let draw_hline = |scene: &mut Scene, y: f64| {
            let start = Point::new(x_offset, baseline_y + y);
            let end = Point::new(x_offset + advance_width, baseline_y + y);
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

    /// Render minimal metrics markers for text mode (Glyphs.app style)
    ///
    /// Shows cross markers (+) at each edge point where metrics lines would be
    fn render_sort_minimal_metrics(
        &self,
        scene: &mut Scene,
        x_offset: f64,
        baseline_y: f64,
        advance_width: f64,
        transform: &Affine,
        color: masonry::vello::peniko::Color,
    ) {
        let stroke = Stroke::new(theme::size::METRIC_LINE_WIDTH * 2.0);
        let brush = Brush::Solid(color);
        let cross_size = 24.0; // Length of each arm of the cross from center

        // Helper to draw a cross (+) at a given point
        let draw_cross = |scene: &mut Scene, x: f64, y: f64| {
            // Horizontal line
            let h_line = kurbo::Line::new(
                *transform * Point::new(x - cross_size, y),
                *transform * Point::new(x + cross_size, y),
            );
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &h_line);

            // Vertical line
            let v_line = kurbo::Line::new(
                *transform * Point::new(x, y - cross_size),
                *transform * Point::new(x, y + cross_size),
            );
            scene.stroke(&stroke, Affine::IDENTITY, &brush, None, &v_line);
        };

        // Left edge crosses
        draw_cross(scene, x_offset, baseline_y + self.session.descender); // Bottom
        draw_cross(scene, x_offset, baseline_y); // Baseline
        draw_cross(scene, x_offset, baseline_y + self.session.ascender); // Top

        // Right edge crosses
        draw_cross(scene, x_offset + advance_width, baseline_y + self.session.descender); // Bottom
        draw_cross(scene, x_offset + advance_width, baseline_y); // Baseline
        draw_cross(scene, x_offset + advance_width, baseline_y + self.session.ascender); // Top
    }

    /// Render width and sidebearing labels for text mode (Glyphs.app style)
    ///
    /// Shows LSB, width, and RSB in light gray text at the bottom of the sort
    #[allow(dead_code)]
    fn render_sort_labels(
        &self,
        scene: &mut Scene,
        x_offset: f64,
        baseline_y: f64,
        lsb: f64,
        width: f64,
        rsb: f64,
        transform: &Affine,
    ) {
        // Text styling
        let font_size = 14.0;
        let text_color = masonry::vello::peniko::Color::from_rgb8(0x80, 0x80, 0x80); // Gray
        let brushes = vec![Brush::Solid(text_color)];

        // Position labels below the descender
        let label_y = baseline_y + self.session.descender - 16.0;

        // Helper function to render a single label
        let render_label = |scene: &mut Scene, text: String, x: f64, y: f64| {
            let mut font_cx = FontContext::default();
            let mut layout_cx = LayoutContext::new();

            let mut builder = layout_cx.ranged_builder(&mut font_cx, &text, 1.0, false);
            builder.push_default(StyleProperty::FontSize(font_size));
            builder.push_default(StyleProperty::FontStack(FontStack::Single(
                FontFamily::Generic(GenericFamily::SansSerif)
            )));
            builder.push_default(StyleProperty::Brush(BrushIndex(0)));
            let mut layout = builder.build(&text);
            layout.break_all_lines(None);

            // Center the text horizontally at the given x position
            let text_width = layout.width() as f64;
            let text_height = layout.height() as f64;

            // Transform the position from font space to screen space
            let screen_pos = *transform * Point::new(x, y);

            // Render text in screen space (no flip)
            render_text(
                scene,
                Affine::translate((screen_pos.x - text_width / 2.0, screen_pos.y - text_height / 2.0)),
                &layout,
                &brushes,
                false,
            );
        };

        // Render three labels: LSB, Width, RSB
        render_label(scene, format!("{:.0}", lsb), x_offset, label_y);
        render_label(scene, format!("{:.0}", width), x_offset + width / 2.0, label_y);
        render_label(scene, format!("{:.0}", rsb), x_offset + width, label_y);
    }

    // ===== Phase 7: Active Sort Toggling =====

    /// Check if the current click is a double-click
    ///
    /// Returns true if the click is within 500ms and 10px of the last click
    fn is_double_click(&mut self, position: Point) -> bool {
        const DOUBLE_CLICK_TIME_MS: u128 = 500;
        const DOUBLE_CLICK_DISTANCE_PX: f64 = 10.0;

        let now = std::time::Instant::now();

        let is_double = if let (Some(last_time), Some(last_pos)) =
            (self.last_click_time, self.last_click_position)
        {
            let time_diff = now.duration_since(last_time).as_millis();
            let distance = ((position.x - last_pos.x).powi(2)
                + (position.y - last_pos.y).powi(2))
            .sqrt();

            time_diff < DOUBLE_CLICK_TIME_MS
                && distance < DOUBLE_CLICK_DISTANCE_PX
        } else {
            false
        };

        // Update tracking (even if this is a double-click, it could be triple-click next)
        self.last_click_time = Some(now);
        self.last_click_position = Some(position);

        is_double
    }

    /// Find which sort is at the given design-space position
    ///
    /// Returns the index of the sort, or None if no sort was clicked
    fn find_sort_at_position(&self, position: Point) -> Option<usize> {
        let buffer = self.session.text_buffer.as_ref()?;

        let mut x_offset = 0.0;
        let mut baseline_y = 0.0;
        let upm_height = self.session.ascender - self.session.descender;

        // Track previous glyph for kerning lookup
        let mut prev_glyph_name: Option<String> = None;
        let mut prev_glyph_group: Option<String> = None;

        for (index, sort) in buffer.iter().enumerate() {
            match &sort.kind {
                crate::sort::SortKind::Glyph { name, advance_width, .. } => {
                    // Apply kerning if we have a previous glyph
                    if let Some(prev_name) = &prev_glyph_name {
                        if let Some(workspace_arc) = &self.session.workspace {
                            let workspace = workspace_arc.read().unwrap();

                            // Get current glyph's left kerning group
                            let curr_group = workspace.get_glyph(name)
                                .and_then(|g| g.left_group.as_ref().map(|s| s.as_str()));

                            // Look up kerning value
                            let kern_value = crate::kerning::lookup_kerning(
                                &workspace.kerning,
                                &workspace.groups,
                                prev_name,
                                prev_glyph_group.as_deref(),
                                name,
                                curr_group,
                            );

                            x_offset += kern_value;
                        }
                    }

                    // Create bounding box for this sort
                    let sort_rect = kurbo::Rect::new(
                        x_offset,
                        baseline_y + self.session.descender,
                        x_offset + advance_width,
                        baseline_y + self.session.ascender,
                    );

                    if sort_rect.contains(position) {
                        return Some(index);
                    }

                    x_offset += advance_width;

                    // Update previous glyph info for next iteration
                    prev_glyph_name = Some(name.clone());
                    if let Some(workspace_arc) = &self.session.workspace {
                        let workspace = workspace_arc.read().unwrap();
                        prev_glyph_group = workspace.get_glyph(name)
                            .and_then(|g| g.right_group.clone());
                    }
                }
                crate::sort::SortKind::LineBreak => {
                    x_offset = 0.0;
                    baseline_y -= upm_height;

                    // Reset kerning tracking (no kerning across lines)
                    prev_glyph_name = None;
                    prev_glyph_group = None;
                }
            }
        }

        None
    }

    /// Activate a sort for editing
    ///
    /// This loads the sort's paths into session.paths, updates the active_sort_* fields,
    /// and sets the is_active flag in the buffer.
    fn activate_sort(&mut self, sort_index: usize) {
        let buffer = match &mut self.session.text_buffer {
            Some(buf) => buf,
            None => return,
        };

        // Get the sort at this index
        let sort = match buffer.get(sort_index) {
            Some(s) => s,
            None => return,
        };

        // Get glyph name and unicode
        let (glyph_name, unicode) = match &sort.kind {
            crate::sort::SortKind::Glyph { name, codepoint, .. } => {
                let unicode_str = codepoint.map(|c| format!("U+{:04X}", c as u32));
                (name.clone(), unicode_str)
            }
            crate::sort::SortKind::LineBreak => return, // Can't activate line breaks
        };

        tracing::info!(
            "Activating sort {} (glyph: {}, unicode: {:?})",
            sort_index,
            glyph_name,
            unicode
        );

        // Load the glyph's paths from the workspace
        let workspace = match &self.session.workspace {
            Some(ws) => ws,
            None => {
                tracing::warn!("No workspace available to load glyph paths");
                return;
            }
        };

        let workspace_guard = workspace.read().unwrap();
        let glyph = match workspace_guard.glyphs.get(&glyph_name) {
            Some(g) => g,
            None => {
                tracing::warn!("Glyph '{}' not found in workspace", glyph_name);
                return;
            }
        };

        // Convert contours to paths
        let paths: Vec<crate::path::Path> = glyph
            .contours
            .iter()
            .map(|contour| crate::path::Path::from_contour(contour))
            .collect();

        // Calculate x-offset for this sort by summing advance widths and kerning of all previous sorts
        let mut x_offset = 0.0;
        let mut prev_glyph_name: Option<String> = None;
        let mut prev_glyph_group: Option<String> = None;

        for i in 0..sort_index {
            if let Some(sort) = buffer.get(i) {
                match &sort.kind {
                    crate::sort::SortKind::Glyph { name, advance_width, .. } => {
                        // Apply kerning if we have a previous glyph
                        if let Some(prev_name) = &prev_glyph_name {
                            // Get current glyph's left kerning group
                            let curr_group = workspace_guard.get_glyph(name)
                                .and_then(|g| g.left_group.as_ref().map(|s| s.as_str()));

                            // Look up kerning value
                            let kern_value = crate::kerning::lookup_kerning(
                                &workspace_guard.kerning,
                                &workspace_guard.groups,
                                prev_name,
                                prev_glyph_group.as_deref(),
                                name,
                                curr_group,
                            );

                            x_offset += kern_value;
                        }

                        x_offset += advance_width;

                        // Update previous glyph info for next iteration
                        prev_glyph_name = Some(name.clone());
                        prev_glyph_group = workspace_guard.get_glyph(name)
                            .and_then(|g| g.right_group.clone());
                    }
                    crate::sort::SortKind::LineBreak => {
                        // Reset kerning tracking (no kerning across lines)
                        prev_glyph_name = None;
                        prev_glyph_group = None;
                    }
                }
            }
        }

        // Update session state
        self.session.paths = std::sync::Arc::new(paths);
        self.session.glyph = std::sync::Arc::new(glyph.clone()); // Preserve codepoints for sync_to_workspace
        self.session.active_sort_index = Some(sort_index);
        self.session.active_sort_name = Some(glyph_name);
        self.session.active_sort_unicode = unicode;
        self.session.active_sort_x_offset = x_offset;

        // Update buffer to mark this sort as active
        buffer.set_active_sort(sort_index);

        tracing::info!(
            "Sort {} activated with {} paths loaded, x_offset={}",
            sort_index,
            self.session.paths.len(),
            x_offset
        );
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

        // Phase 7: Check for double-click on a sort to activate it
        // Convert local position to design space
        let design_pos = self.session.viewport.screen_to_design(local_pos);

        // Check if this is a double-click
        if self.is_double_click(design_pos) {
            // Check if we clicked on a sort
            if let Some(sort_index) = self.find_sort_at_position(design_pos) {
                tracing::info!("Double-click detected on sort {}", sort_index);
                self.activate_sort(sort_index);

                // Emit SessionUpdate so AppState gets the updated session with new paths
                ctx.submit_action::<SessionUpdate>(SessionUpdate {
                    session: self.session.clone(),
                    save_requested: false,
                });

                ctx.request_render();
                return; // Don't dispatch to tool
            }
        }

        // Check for shift+click in text mode to enter manual kerning mode
        if self.session.text_mode_active && state.modifiers.shift() {
            if let Some(sort_index) = self.find_sort_at_position(design_pos) {
                // Can only kern if there's a previous glyph
                if sort_index > 0 {
                    // Get current kern value for this pair
                    let current_kern_value = if let Some(buffer) = &self.session.text_buffer {
                        if let (Some(curr_sort), Some(prev_sort)) = (buffer.get(sort_index), buffer.get(sort_index - 1)) {
                            if let (
                                crate::sort::SortKind::Glyph { name: curr_name, .. },
                                crate::sort::SortKind::Glyph { name: prev_name, .. }
                            ) = (&curr_sort.kind, &prev_sort.kind) {
                                if let Some(workspace_arc) = &self.session.workspace {
                                    let workspace = workspace_arc.read().unwrap();
                                    let prev_glyph = workspace.get_glyph(prev_name);
                                    let curr_glyph = workspace.get_glyph(curr_name);

                                    crate::kerning::lookup_kerning(
                                        &workspace.kerning,
                                        &workspace.groups,
                                        prev_name,
                                        prev_glyph.and_then(|g| g.right_group.as_deref()),
                                        curr_name,
                                        curr_glyph.and_then(|g| g.left_group.as_deref()),
                                    )
                                } else {
                                    0.0
                                }
                            } else {
                                0.0
                            }
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    };

                    tracing::info!("Entering kern mode for sort {}, current kern = {}", sort_index, current_kern_value);
                    self.kern_mode_active = true;
                    self.kern_sort_index = Some(sort_index);
                    self.kern_start_x = design_pos.x;
                    self.kern_original_value = current_kern_value;
                    self.kern_current_offset = 0.0;

                    // Activate this sort so the panel updates
                    self.activate_sort(sort_index);

                    // Emit session update for panel
                    ctx.submit_action::<SessionUpdate>(SessionUpdate {
                        session: self.session.clone(),
                        save_requested: false,
                    });

                    ctx.request_render();
                    return; // Don't dispatch to tool
                }
            }
        }

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

        // Handle kern mode dragging (horizontal constraint)
        if self.kern_mode_active {
            let design_pos = self.session.viewport.screen_to_design(local_pos);
            // Only horizontal movement
            self.kern_current_offset = design_pos.x - self.kern_start_x;

            // Apply kern value in real-time
            if let (Some(sort_index), Some(buffer)) = (self.kern_sort_index, &self.session.text_buffer) {
                if let (Some(curr_sort), Some(prev_sort)) = (buffer.get(sort_index), buffer.get(sort_index - 1)) {
                    if let (
                        crate::sort::SortKind::Glyph { name: curr_name, .. },
                        crate::sort::SortKind::Glyph { name: prev_name, .. }
                    ) = (&curr_sort.kind, &prev_sort.kind) {
                        if let Some(workspace_arc) = &self.session.workspace {
                            let new_kern_value = self.kern_original_value + self.kern_current_offset;
                            let mut workspace = workspace_arc.write().unwrap();

                            if new_kern_value == 0.0 {
                                // Remove kerning if value is 0
                                if let Some(first_pairs) = workspace.kerning.get_mut(prev_name) {
                                    first_pairs.remove(curr_name);
                                }
                            } else {
                                // Set or update kerning value
                                workspace.kerning
                                    .entry(prev_name.clone())
                                    .or_insert_with(std::collections::HashMap::new)
                                    .insert(curr_name.clone(), new_kern_value);
                            }
                        }
                    }
                }
            }

            // Emit session update so panel reflects new value
            ctx.submit_action::<SessionUpdate>(SessionUpdate {
                session: self.session.clone(),
                save_requested: false,
            });

            ctx.request_render();
            return; // Don't dispatch to tool while kerning
        }

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

        // Handle kern mode release - kerning was already applied during drag
        if self.kern_mode_active {
            let final_kern_value = self.kern_original_value + self.kern_current_offset;
            tracing::info!("Kern mode released: final value = {}", final_kern_value);

            // Reset kern mode
            self.kern_mode_active = false;
            self.kern_sort_index = None;
            self.kern_original_value = 0.0;
            self.kern_current_offset = 0.0;

            // Emit final session update
            ctx.submit_action::<SessionUpdate>(SessionUpdate {
                session: self.session.clone(),
                save_requested: false,
            });

            ctx.request_render();
            return; // Don't dispatch to tool
        }

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
            // Sync edits to workspace immediately so all instances update
            self.session.sync_to_workspace();
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

    /// Handle scroll wheel zoom
    fn handle_scroll_zoom(&mut self, ctx: &mut EventCtx<'_>, delta: &ScrollDelta) {
        // Extract the Y component of the scroll delta
        // Negative Y = scroll up = zoom in
        // Positive Y = scroll down = zoom out
        let scroll_y = match delta {
            ScrollDelta::LineDelta(_x, y) => *y,
            ScrollDelta::PixelDelta(pos) => (pos.y / 10.0) as f32, // Scale down pixel deltas
            ScrollDelta::PageDelta(_x, y) => *y * 3.0, // Page scrolls are bigger
        };

        if scroll_y.abs() < 0.001 {
            return; // Ignore very small scrolls
        }

        // Calculate zoom factor: negative scroll_y means zoom in
        let zoom_factor = if scroll_y < 0.0 {
            1.1 // Zoom in
        } else {
            1.0 / 1.1 // Zoom out
        };

        // Apply zoom with limits
        let new_zoom = (self.session.viewport.zoom * zoom_factor)
            .max(settings::editor::MIN_ZOOM)
            .min(settings::editor::MAX_ZOOM);

        self.session.viewport.zoom = new_zoom;
        tracing::debug!("Scroll zoom: scroll_y={:.2}, new zoom={:.2}", scroll_y, new_zoom);

        ctx.request_render();
    }

    /// Handle spacebar for temporary preview mode
    /// Note: Disabled in text edit mode to allow typing spaces
    fn handle_spacebar(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key_event: &masonry::core::keyboard::KeyboardEvent,
    ) -> bool {
        use masonry::core::keyboard::{Key, KeyState};

        if !matches!(&key_event.key, Key::Character(c) if c == " ") {
            return false;
        }

        // Don't handle spacebar in text edit mode - let it insert space characters
        if self.session.text_mode_active {
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
        ctrl: bool,
    ) -> bool {
        use masonry::core::keyboard::{Key, NamedKey};

        // Ctrl+Space: Toggle preview mode (works in all modes including text edit)
        if ctrl && matches!(key, Key::Character(c) if c == " ") {
            let current_tool = self.session.current_tool.id();
            if current_tool == crate::tools::ToolId::Preview {
                // Already in preview mode - this shouldn't happen with Ctrl+Space
                // since we don't have a "previous tool" tracked for Ctrl+Space
                // Just do nothing
            } else {
                // Switch to Preview tool
                use crate::tools::ToolBox;
                let mut tool = std::mem::replace(
                    &mut self.session.current_tool,
                    ToolBox::for_id(crate::tools::ToolId::Select),
                );
                self.mouse.cancel(&mut tool, &mut self.session);
                self.mouse = Mouse::new();

                self.session.current_tool = ToolBox::for_id(crate::tools::ToolId::Preview);

                ctx.submit_action::<SessionUpdate>(SessionUpdate {
                    session: self.session.clone(),
                    save_requested: false,
                });
                ctx.request_render();
                ctx.set_handled();
                return true;
            }
        }

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
        // Skip in text mode - let text mode handler deal with backspace
        if !self.session.text_mode_active
            && matches!(
                key,
                Key::Named(NamedKey::Backspace) | Key::Named(NamedKey::Delete)
            )
        {
            self.session.delete_selection();
            self.record_edit(EditType::Normal);
            self.session.sync_to_workspace();
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        // Toggle point type (T key) - disabled in text mode
        if !self.session.text_mode_active && matches!(key, Key::Character(c) if c == "t") {
            self.session.toggle_point_type();
            self.record_edit(EditType::Normal);
            self.session.sync_to_workspace();
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        // Reverse contours (R key) - disabled in text mode
        if !self.session.text_mode_active && matches!(key, Key::Character(c) if c == "r") {
            self.session.reverse_contours();
            self.record_edit(EditType::Normal);
            self.session.sync_to_workspace();
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        // Tool switching shortcuts (without modifiers) - disabled in text mode
        if !self.session.text_mode_active && !cmd && !shift {
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

        // Check if we have a component selected (takes priority over points)
        if self.session.selected_component.is_some() {
            tracing::debug!(
                "Nudging selected component: dx={} dy={} shift={} ctrl={}",
                dx, dy, shift, ctrl
            );

            // Calculate the actual nudge amount
            let multiplier = if ctrl {
                100.0
            } else if shift {
                10.0
            } else {
                1.0
            };
            let delta = kurbo::Vec2::new(dx * multiplier, dy * multiplier);
            self.session.move_selected_component(delta);
        } else {
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
        }

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

        tracing::debug!("[handle_text_mode_input] key={:?}, text_mode_active={}, has_buffer={}",
            key, self.session.text_mode_active, self.session.text_buffer.is_some());

        // Handle arrow keys for cursor movement
        // In RTL mode, visual left/right is inverted from logical left/right
        let is_rtl = self.session.text_direction.is_rtl();

        match key {
            Key::Named(NamedKey::ArrowLeft) => {
                if let Some(buffer) = &mut self.session.text_buffer {
                    // Visual left: in RTL, moves cursor forward (right in buffer)
                    //              in LTR, moves cursor backward (left in buffer)
                    if is_rtl {
                        buffer.move_cursor_right();
                    } else {
                        buffer.move_cursor_left();
                    }
                    self.text_cursor.reset(); // Reset cursor to visible on movement
                    ctx.request_render();
                    ctx.set_handled();
                    return true;
                }
            }
            Key::Named(NamedKey::ArrowRight) => {
                if let Some(buffer) = &mut self.session.text_buffer {
                    // Visual right: in RTL, moves cursor backward (left in buffer)
                    //               in LTR, moves cursor forward (right in buffer)
                    if is_rtl {
                        buffer.move_cursor_left();
                    } else {
                        buffer.move_cursor_right();
                    }
                    self.text_cursor.reset(); // Reset cursor to visible on movement
                    ctx.request_render();
                    ctx.set_handled();
                    return true;
                }
            }
            Key::Named(NamedKey::Backspace) => {
                tracing::info!("[Backspace] Handling backspace in text mode");
                let reshape_pos = if let Some(buffer) = &mut self.session.text_buffer {
                    let cursor_before = buffer.cursor();
                    let len_before = buffer.len();
                    let deleted = buffer.delete();
                    tracing::info!(
                        "[Backspace] cursor: {} -> {}, len: {} -> {}, deleted: {:?}",
                        cursor_before,
                        buffer.cursor(),
                        len_before,
                        buffer.len(),
                        deleted.is_some()
                    );
                    Some(buffer.cursor())
                } else {
                    None
                };

                // Reshape neighbors after deletion (their forms may have changed)
                if let Some(pos) = reshape_pos {
                    self.session.reshape_buffer_around(pos);
                }

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
            Key::Named(NamedKey::Delete) => {
                let reshape_pos = if let Some(buffer) = &mut self.session.text_buffer {
                    let cursor_pos = buffer.cursor();
                    buffer.delete_forward();
                    Some(cursor_pos)
                } else {
                    None
                };

                // Reshape neighbors after deletion
                if let Some(pos) = reshape_pos {
                    self.session.reshape_buffer_around(pos);
                }

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

                // Insert character as a sort (with Arabic shaping if RTL)
                if let Some(c) = s.chars().next() {
                    // Use shaped sort for Arabic text in RTL mode
                    if let Some(sort) = self.session.create_shaped_sort_from_char(c) {
                        // Get cursor position before insertion for reshaping
                        let cursor_pos = self.session.text_buffer.as_ref()
                            .map(|b| b.cursor())
                            .unwrap_or(0);

                        if let Some(buffer) = &mut self.session.text_buffer {
                            buffer.insert(sort);
                        }

                        // Reshape neighbors (their forms may have changed)
                        // The cursor has moved, so reshape around cursor-1 (the inserted char)
                        self.session.reshape_buffer_around(cursor_pos);

                        self.text_cursor.reset(); // Reset cursor to visible on edit
                        // Emit session update to persist text buffer changes
                        ctx.submit_action::<SessionUpdate>(SessionUpdate {
                            session: self.session.clone(),
                            save_requested: false,
                        });
                        ctx.request_render();
                        ctx.set_handled();
                        return true;
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

            // Preserve viewport state before updating session
            let old_viewport = widget.widget.session.viewport.clone();
            let old_viewport_initialized = widget.widget.session.viewport_initialized;

            // Update the session, but preserve:
            // - Mouse state (to avoid breaking active drag
            //   operations)
            // - Undo state
            // - Canvas size
            // - Viewport state (to avoid re-initialization and flickering)
            // This allows tool changes and other session updates to
            // take effect
            widget.widget.session = (*self.session).clone();

            // Restore viewport state
            widget.widget.session.viewport = old_viewport;
            widget.widget.session.viewport_initialized = old_viewport_initialized;

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

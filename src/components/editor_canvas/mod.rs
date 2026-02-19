// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Glyph editor canvas widget - the main canvas for editing glyphs

mod drawing;
mod keyboard;
mod paint;
mod pointer;
mod text_buffer;
mod view;

pub use view::editor_view;

use crate::editing::EditSession;
use crate::editing::EditType;
use crate::editing::Mouse;
use crate::editing::UndoState;
use crate::sort::TextCursor;
use kurbo::Point;
use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, BoxConstraints, ChildrenIds, EventCtx, LayoutCtx, PaintCtx, PointerButton,
    PointerButtonEvent, PointerEvent, PointerScrollEvent, PointerUpdate, PropertiesMut,
    PropertiesRef, RegisterCtx, TextEvent, Update, UpdateCtx, Widget,
};
use masonry::kurbo::Size;
use masonry::vello::Scene;
use std::sync::Arc;

/// The main glyph editor canvas widget
pub struct EditorWidget {
    /// The editing session (mutable copy for editing)
    pub session: EditSession,

    /// Mouse state machine
    pub(super) mouse: Mouse,

    /// Canvas size
    pub(super) size: Size,

    /// Undo/redo state
    pub(super) undo: UndoState<EditSession>,

    /// The last edit type (for grouping consecutive edits)
    pub(super) last_edit_type: Option<EditType>,

    /// Tool to return to when spacebar is released
    /// (for temporary preview mode)
    pub(super) previous_tool: Option<crate::tools::ToolId>,

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
    pub(super) drag_update_counter: u32,

    /// Text cursor for text editing mode
    pub(super) text_cursor: TextCursor,

    /// Last click time for double-click detection
    pub(super) last_click_time: Option<std::time::Instant>,

    /// Last click position for double-click detection
    pub(super) last_click_position: Option<Point>,

    /// Manual kerning mode state
    pub(super) kern_mode_active: bool,

    /// Index of the sort being kerned (dragged)
    pub(super) kern_sort_index: Option<usize>,

    /// Starting X position when kern drag began
    pub(super) kern_start_x: f64,

    /// Original kern value before drag started
    pub(super) kern_original_value: f64,

    /// Current horizontal offset from start position during kern drag
    pub(super) kern_current_offset: f64,
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

    /// Record an edit operation for undo
    ///
    /// This manages undo grouping:
    /// - If the edit type matches the last edit, update the
    ///   current undo group
    /// - If the edit type is different, create a new undo group
    pub(super) fn record_edit(&mut self, edit_type: EditType) {
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
    pub(super) fn undo(&mut self) {
        if let Some(previous) = self.undo.undo(self.session.clone()) {
            self.session = previous;
            tracing::debug!("Undo: restored previous state");
        }
    }

    /// Redo the last undone edit
    pub(super) fn redo(&mut self) {
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
    pub(super) fn convert_selected_hyper_to_cubic(&mut self) -> bool {
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
                    Path::Hyper(hyper) => hyper
                        .points()
                        .iter()
                        .any(|pt| self.session.selection.contains(&pt.id)),
                    _ => false,
                }
            } else {
                // If nothing selected, convert all hyperbezier paths
                matches!(path, Path::Hyper(_))
            };

            // Convert if needed
            if should_convert && let Path::Hyper(hyper) = path {
                *path = Path::Cubic(hyper.to_cubic());
                converted = true;
                tracing::info!("Converted hyperbezier path to cubic");
            }
        }

        if converted {
            // Update paths with converted versions
            self.session.paths = Arc::new(new_paths);

            // Clear selection since point IDs will have changed
            self.session.selection = crate::editing::Selection::new();
        }

        converted
    }

    /// Emit a session update action
    pub(super) fn emit_session_update(&self, ctx: &mut EventCtx<'_>, save_requested: bool) {
        ctx.submit_action::<SessionUpdate>(SessionUpdate {
            session: self.session.clone(),
            save_requested,
        });
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

    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _props: &PropertiesRef<'_>, scene: &mut Scene) {
        let canvas_size = ctx.size();
        self.paint_background(scene, canvas_size);

        if !self.session.viewport_initialized {
            self.initialize_viewport(canvas_size);
        }

        let transform = self.session.viewport.affine();
        let is_preview_mode = self.is_preview_mode();

        if self.session.text_buffer.is_some() {
            self.paint_text_buffer_mode(scene, &transform, is_preview_mode);
            return;
        }

        self.paint_single_glyph_mode(scene, &transform, is_preview_mode);
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
            let cmd = key_event.modifiers.meta() || key_event.modifiers.ctrl();
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
            if self.handle_keyboard_shortcuts(ctx, &key_event.key, cmd, shift, ctrl) {
                return;
            }

            // Phase 5: Handle text mode input (character typing, cursor movement)
            // Only handle after shortcuts, and only if no modifiers (except shift for caps)
            if self.session.text_mode_active
                && self.session.text_buffer.is_some()
                && self.handle_text_mode_input(ctx, &key_event.key, cmd)
            {
                return;
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
        let glyph_label = self
            .session
            .active_sort_name
            .as_deref()
            .unwrap_or("(no active sort)");
        node.set_label(format!("Editing glyph: {}", glyph_label));
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}

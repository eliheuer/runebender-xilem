// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Keyboard event handlers for EditorWidget

use super::{EditorWidget, SessionUpdate};
use crate::editing::{EditType, Mouse};
use crate::settings;
use kurbo::Point;
use masonry::core::EventCtx;

impl EditorWidget {
    /// Handle spacebar for temporary preview mode
    /// Note: Disabled in text edit mode to allow typing spaces
    pub(super) fn handle_spacebar(
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

        if key_event.state == KeyState::Down && self.previous_tool.is_none() {
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
                self.session.current_tool = ToolBox::for_id(crate::tools::ToolId::Preview);

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
        } else if key_event.state == KeyState::Up && self.previous_tool.is_some() {
            // Spacebar released: return to previous tool
            if let Some(previous) = self.previous_tool.take() {
                // Reset mouse state by creating new instance
                self.mouse = Mouse::new();

                self.session.current_tool = crate::tools::ToolBox::for_id(previous);
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
    pub(super) fn handle_keyboard_shortcuts(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &masonry::core::keyboard::Key,
        cmd: bool,
        shift: bool,
        ctrl: bool,
    ) -> bool {
        if self.handle_toggle_panels(ctx, key) {
            return true;
        }

        if self.handle_ctrl_space_toggle(ctx, ctrl, key) {
            return true;
        }

        if self.handle_undo_redo(ctx, cmd, shift, key) {
            return true;
        }

        if self.handle_zoom_shortcuts(ctx, cmd, key) {
            return true;
        }

        if self.handle_fit_to_window(ctx, cmd, key) {
            return true;
        }

        if self.handle_convert_hyperbezier(ctx, cmd, shift, key) {
            return true;
        }

        if self.handle_copy_points(ctx, cmd, key) {
            return true;
        }

        if self.handle_paste_points(ctx, cmd, key) {
            return true;
        }

        if self.handle_save(ctx, cmd, key) {
            return true;
        }

        if self.handle_delete_points(ctx, key) {
            return true;
        }

        if self.handle_toggle_point_type(ctx, key) {
            return true;
        }

        if self.handle_reverse_contours(ctx, key) {
            return true;
        }

        if self.handle_import_image(ctx, cmd, shift, key) {
            return true;
        }

        if self.handle_toggle_image_lock(ctx, cmd, shift, key) {
            return true;
        }

        if self.handle_tool_switching(ctx, cmd, shift, key) {
            return true;
        }

        false
    }

    // ============================================================================
    // KEYBOARD SHORTCUT HANDLERS
    // ============================================================================

    fn handle_toggle_panels(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::{Key, NamedKey};

        if !matches!(key, Key::Named(NamedKey::Tab)) {
            return false;
        }

        // Don't toggle panels in text edit mode (Tab may be used for other purposes)
        if self.session.text_mode_active {
            return false;
        }

        self.session.panels_visible = !self.session.panels_visible;
        self.emit_session_update(ctx, false);
        ctx.request_render();
        ctx.set_handled();
        true
    }

    fn handle_ctrl_space_toggle(
        &mut self,
        ctx: &mut EventCtx<'_>,
        ctrl: bool,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use crate::tools::ToolBox;
        use masonry::core::keyboard::Key;

        if !ctrl || !matches!(key, Key::Character(c) if c == " ") {
            return false;
        }

        let current_tool = self.session.current_tool.id();
        if current_tool == crate::tools::ToolId::Preview {
            return false;
        }

        let mut tool = std::mem::replace(
            &mut self.session.current_tool,
            ToolBox::for_id(crate::tools::ToolId::Select),
        );
        self.mouse.cancel(&mut tool, &mut self.session);
        self.mouse = Mouse::new();

        self.session.current_tool = ToolBox::for_id(crate::tools::ToolId::Preview);

        self.emit_session_update(ctx, false);
        ctx.request_render();
        ctx.set_handled();
        true
    }

    fn handle_undo_redo(
        &mut self,
        ctx: &mut EventCtx<'_>,
        cmd: bool,
        shift: bool,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::Key;

        if !cmd || !matches!(key, Key::Character(c) if c == "z") {
            return false;
        }

        if shift {
            self.redo();
        } else {
            self.undo();
        }

        self.session.sync_to_workspace();
        self.session.update_coord_selection();
        self.emit_session_update(ctx, false);
        ctx.request_render();
        ctx.set_handled();
        true
    }

    fn handle_zoom_shortcuts(
        &mut self,
        ctx: &mut EventCtx<'_>,
        cmd: bool,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::Key;

        if !cmd {
            return false;
        }

        if matches!(key, Key::Character(c) if c == "+" || c == "=") {
            let new_zoom = (self.session.viewport.zoom * 1.1).min(settings::editor::MAX_ZOOM);
            self.session.viewport.zoom = new_zoom;
            tracing::info!("Zoom in: new zoom = {:.2}", new_zoom);
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        if matches!(key, Key::Character(c) if c == "-" || c == "_") {
            let new_zoom = (self.session.viewport.zoom / 1.1).max(settings::editor::MIN_ZOOM);
            self.session.viewport.zoom = new_zoom;
            tracing::info!("Zoom out: new zoom = {:.2}", new_zoom);
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        false
    }

    fn handle_fit_to_window(
        &mut self,
        ctx: &mut EventCtx<'_>,
        cmd: bool,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::Key;

        if !cmd || !matches!(key, Key::Character(c) if c == "0") {
            return false;
        }

        self.session.viewport_initialized = false;
        tracing::debug!("Fit to window: resetting viewport");
        ctx.request_render();
        ctx.set_handled();
        true
    }

    fn handle_convert_hyperbezier(
        &mut self,
        ctx: &mut EventCtx<'_>,
        cmd: bool,
        shift: bool,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::Key;

        if !cmd || !shift {
            return false;
        }

        if !matches!(key, Key::Character(c) if c.eq_ignore_ascii_case("h")) {
            return false;
        }

        tracing::info!(
            "Cmd+Shift+H pressed - attempting to convert \
             hyperbezier to cubic"
        );

        if self.convert_selected_hyper_to_cubic() {
            tracing::info!(
                "Converted hyperbezier paths to cubic"
            );
            self.session.sync_to_workspace();
            self.emit_session_update(ctx, false);
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        tracing::warn!("No hyperbezier paths to convert");
        false
    }

    fn handle_save(
        &self,
        ctx: &mut EventCtx<'_>,
        cmd: bool,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::Key;

        if !cmd || !matches!(key, Key::Character(c) if c == "s") {
            return false;
        }

        self.emit_session_update(ctx, true);
        ctx.set_handled();
        true
    }

    fn handle_copy_points(
        &mut self,
        ctx: &mut EventCtx<'_>,
        cmd: bool,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::Key;

        if !cmd || !matches!(key, Key::Character(c) if c == "c") {
            return false;
        }

        if self.session.selection.is_empty() {
            return false;
        }

        // Collect paths that contain any selected point
        let selection = &self.session.selection;
        let copied: Vec<_> = self
            .session
            .paths
            .iter()
            .filter(|path| match path {
                crate::path::Path::Cubic(c) => c.points.iter().any(|pt| selection.contains(&pt.id)),
                crate::path::Path::Quadratic(q) => {
                    q.points.iter().any(|pt| selection.contains(&pt.id))
                }
                crate::path::Path::Hyper(h) => h.points.iter().any(|pt| selection.contains(&pt.id)),
            })
            .cloned()
            .collect();

        if !copied.is_empty() {
            self.point_clipboard = Some(copied);
        }

        ctx.set_handled();
        true
    }

    fn handle_paste_points(
        &mut self,
        ctx: &mut EventCtx<'_>,
        cmd: bool,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use crate::model::EntityId;
        use crate::path::{CubicPath, HyperPath, PathPoint, PathPoints, QuadraticPath};
        use masonry::core::keyboard::Key;
        use std::sync::Arc;

        if !cmd || !matches!(key, Key::Character(c) if c == "v") {
            return false;
        }

        let clipboard = match &self.point_clipboard {
            Some(c) => c.clone(),
            None => return false,
        };

        // Small offset so pasted contours are visually distinct
        let offset = kurbo::Vec2::new(20.0, 20.0);

        // Clone each path with fresh EntityIds and offset
        let mut new_paths: Vec<crate::path::Path> = Vec::new();
        let mut new_selection = crate::editing::Selection::new();

        for path in &clipboard {
            match path {
                crate::path::Path::Cubic(cubic) => {
                    let new_points: Vec<PathPoint> = cubic
                        .points
                        .iter()
                        .map(|pt| {
                            let id = EntityId::next();
                            new_selection.insert(id);
                            PathPoint {
                                id,
                                point: pt.point + offset,
                                typ: pt.typ,
                            }
                        })
                        .collect();
                    new_paths.push(crate::path::Path::Cubic(CubicPath::new(
                        PathPoints::from_vec(new_points),
                        cubic.closed,
                    )));
                }
                crate::path::Path::Quadratic(quad) => {
                    let new_points: Vec<PathPoint> = quad
                        .points
                        .iter()
                        .map(|pt| {
                            let id = EntityId::next();
                            new_selection.insert(id);
                            PathPoint {
                                id,
                                point: pt.point + offset,
                                typ: pt.typ,
                            }
                        })
                        .collect();
                    new_paths.push(crate::path::Path::Quadratic(QuadraticPath::new(
                        PathPoints::from_vec(new_points),
                        quad.closed,
                    )));
                }
                crate::path::Path::Hyper(hyper) => {
                    let new_points: Vec<PathPoint> = hyper
                        .points
                        .iter()
                        .map(|pt| {
                            let id = EntityId::next();
                            new_selection.insert(id);
                            PathPoint {
                                id,
                                point: pt.point + offset,
                                typ: pt.typ,
                            }
                        })
                        .collect();
                    let mut new_hyper =
                        HyperPath::from_points(PathPoints::from_vec(new_points), hyper.closed);
                    new_hyper.after_change();
                    new_paths.push(crate::path::Path::Hyper(new_hyper));
                }
            }
        }

        // Append pasted paths to session
        let paths_vec = Arc::make_mut(&mut self.session.paths);
        paths_vec.extend(new_paths);

        // Select the pasted points
        self.session.selection = new_selection;

        self.record_edit(EditType::Normal);
        self.session.sync_to_workspace();
        self.session.update_coord_selection();
        self.emit_session_update(ctx, false);
        ctx.request_render();
        ctx.set_handled();
        true
    }

    fn handle_delete_points(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::{Key, NamedKey};

        if self.session.text_mode_active {
            return false;
        }

        if !matches!(
            key,
            Key::Named(NamedKey::Backspace) | Key::Named(NamedKey::Delete)
        ) {
            return false;
        }

        // Delete selected background image if present
        if self
            .session
            .background_image
            .as_ref()
            .is_some_and(|bg| bg.selected)
        {
            self.session.background_image = None;
            self.emit_session_update(ctx, false);
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        self.session.delete_selection();
        self.record_edit(EditType::Normal);
        self.session.sync_to_workspace();
        self.session.update_coord_selection();
        self.emit_session_update(ctx, false);
        ctx.request_render();
        ctx.set_handled();
        true
    }

    fn handle_toggle_point_type(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::Key;

        if self.session.text_mode_active {
            return false;
        }

        if !matches!(key, Key::Character(c) if c == "t") {
            return false;
        }

        self.session.toggle_point_type();
        self.record_edit(EditType::Normal);
        self.session.sync_to_workspace();
        self.session.update_coord_selection();
        self.emit_session_update(ctx, false);
        ctx.request_render();
        ctx.set_handled();
        true
    }

    fn handle_reverse_contours(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::Key;

        if self.session.text_mode_active {
            return false;
        }

        if !matches!(key, Key::Character(c) if c == "r") {
            return false;
        }

        self.session.reverse_contours();
        self.record_edit(EditType::Normal);
        self.session.sync_to_workspace();
        self.emit_session_update(ctx, false);
        ctx.request_render();
        ctx.set_handled();
        true
    }

    /// Cmd+Shift+I: Import a background image via file dialog.
    ///
    /// This is a workaround for the lack of drag-and-drop support in
    /// masonry. winit 0.30 supports `WindowEvent::DroppedFile` but
    /// masonry_winit discards it in a `_ => ()` catch-all. Once an
    /// `on_dropped_file()` hook is added to masonry's `AppDriver`
    /// trait, this shortcut can be supplemented with drag-and-drop.
    /// See: https://github.com/linebender/xilem (upstream PR needed)
    fn handle_import_image(
        &mut self,
        ctx: &mut EventCtx<'_>,
        cmd: bool,
        shift: bool,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::Key;

        if !cmd || !shift {
            return false;
        }

        if !matches!(key, Key::Character(c) if c.eq_ignore_ascii_case("i")) {
            return false;
        }

        let path = rfd::FileDialog::new()
            .set_title("Import Background Image")
            .add_filter("Images", &["png", "jpg", "jpeg"])
            .pick_file();

        let path = match path {
            Some(p) => p,
            None => return true, // Dialog cancelled
        };

        match crate::editing::BackgroundImage::load(
            &path,
            self.session.ascender,
            self.session.descender,
            self.session.glyph.width,
        ) {
            Ok(bg_image) => {
                tracing::info!(
                    "Imported background image: {}",
                    path.display()
                );
                self.session.background_image = Some(bg_image);
                self.emit_session_update(ctx, false);
                ctx.request_render();
            }
            Err(e) => {
                tracing::error!(
                    "Failed to import image: {e}"
                );
            }
        }

        ctx.set_handled();
        true
    }

    /// Cmd+Shift+L: Toggle background image lock
    fn handle_toggle_image_lock(
        &mut self,
        ctx: &mut EventCtx<'_>,
        cmd: bool,
        shift: bool,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::Key;

        if !cmd || !shift {
            return false;
        }

        if !matches!(key, Key::Character(c) if c.eq_ignore_ascii_case("l")) {
            return false;
        }

        if let Some(bg) = &mut self.session.background_image {
            bg.locked = !bg.locked;
            // Deselect when locking
            if bg.locked {
                bg.selected = false;
            }
            tracing::info!(
                "Background image locked: {}",
                bg.locked
            );
            self.emit_session_update(ctx, false);
            ctx.request_render();
            ctx.set_handled();
            return true;
        }

        false
    }

    fn handle_tool_switching(
        &mut self,
        ctx: &mut EventCtx<'_>,
        cmd: bool,
        shift: bool,
        key: &masonry::core::keyboard::Key,
    ) -> bool {
        use masonry::core::keyboard::Key;

        if self.session.text_mode_active || cmd || shift {
            return false;
        }

        let tool_id = match key {
            Key::Character(c) if c == "v" => Some(crate::tools::ToolId::Select),
            Key::Character(c) if c == "p" => Some(crate::tools::ToolId::Pen),
            Key::Character(c) if c == "h" => Some(crate::tools::ToolId::HyperPen),
            Key::Character(c) if c == "k" => Some(crate::tools::ToolId::Knife),
            _ => None,
        };

        let tool_id = match tool_id {
            Some(id) => id,
            None => return false,
        };

        let select_tool = crate::tools::ToolBox::for_id(crate::tools::ToolId::Select);
        let mut tool = std::mem::replace(&mut self.session.current_tool, select_tool);
        self.mouse.cancel(&mut tool, &mut self.session);
        self.mouse = crate::editing::Mouse::new();

        self.session.current_tool = crate::tools::ToolBox::for_id(tool_id);

        self.emit_session_update(ctx, false);
        ctx.request_render();
        ctx.set_handled();
        true
    }

    /// Handle arrow keys for nudging
    pub(super) fn handle_arrow_keys(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &masonry::core::keyboard::Key,
        shift: bool,
        ctrl: bool,
    ) {
        use masonry::core::keyboard::{Key, NamedKey};

        let (dx, dy) = match key {
            Key::Named(NamedKey::ArrowLeft) => (-1.0, 0.0),
            Key::Named(NamedKey::ArrowRight) => (1.0, 0.0),
            // Design space: Y increases upward
            Key::Named(NamedKey::ArrowUp) => (0.0, 1.0),
            Key::Named(NamedKey::ArrowDown) => (0.0, -1.0),
            _ => return,
        };

        // Check if we have a component selected (takes priority
        // over points)
        if self.session.selected_component.is_some() {
            let amount = if ctrl {
                settings::nudge::CMD
            } else if shift {
                settings::nudge::SHIFT
            } else {
                settings::nudge::BASE
            };
            let delta = kurbo::Vec2::new(dx * amount, dy * amount);
            self.session.move_selected_component(delta);
        } else {
            self.session.nudge_selection(dx, dy, shift, ctrl);
            self.session.snap_selection_to_grid();
        }

        self.record_edit(EditType::Drag);
        self.session.sync_to_workspace();
        self.session.update_coord_selection();
        self.emit_session_update(ctx, false);
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
    pub(super) fn handle_text_mode_input(
        &mut self,
        ctx: &mut EventCtx<'_>,
        key: &masonry::core::keyboard::Key,
        cmd: bool,
    ) -> bool {
        use masonry::core::keyboard::{Key, NamedKey};

        tracing::debug!(
            "[handle_text_mode_input] key={:?}, text_mode_active={}, has_buffer={}",
            key,
            self.session.text_mode_active,
            self.session.text_buffer.is_some()
        );

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
                        let cursor_pos = self
                            .session
                            .text_buffer
                            .as_ref()
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

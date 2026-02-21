// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Pointer event handlers for EditorWidget

use super::EditorWidget;
use crate::model::{read_workspace, write_workspace};
use crate::settings;
use kurbo::Point;
use masonry::core::{EventCtx, ScrollDelta};

impl EditorWidget {
    // ============================================================================
    // POINTER EVENT HANDLERS
    // ============================================================================

    /// Handle pointer down event
    pub(super) fn handle_pointer_down(
        &mut self,
        ctx: &mut EventCtx<'_>,
        state: &masonry::core::PointerState,
    ) {
        tracing::debug!(
            "[EditorWidget::on_pointer_event] Down at {:?}, \
             current_tool: {:?}",
            state.position,
            self.session.current_tool.id()
        );

        ctx.request_focus();
        ctx.capture_pointer();

        let local_pos = ctx.local_position(state.position);
        let design_pos = self.session.viewport.screen_to_design(local_pos);

        if self.handle_double_click(ctx, local_pos, design_pos) {
            return;
        }

        if self.handle_kern_mode_activation(ctx, state, design_pos) {
            return;
        }

        self.dispatch_tool_mouse_down(ctx, local_pos, state);
        // Propagate selection change to panels immediately
        self.session.update_coord_selection();
        self.emit_session_update(ctx, false);
    }

    fn handle_double_click(
        &mut self,
        ctx: &mut EventCtx<'_>,
        local_pos: Point,
        design_pos: Point,
    ) -> bool {
        if !self.is_double_click(design_pos) {
            return false;
        }

        if self.handle_point_double_click(ctx, local_pos) {
            return true;
        }

        if self.handle_component_double_click(ctx, local_pos) {
            return true;
        }

        if let Some(sort_index) = self.find_sort_at_position(design_pos) {
            tracing::info!("Double-click detected on sort {}", sort_index);
            self.activate_sort(sort_index);
            self.emit_session_update(ctx, false);
            ctx.request_render();
            return true;
        }

        false
    }

    /// Double-click on a point toggles smooth ↔ corner
    fn handle_point_double_click(&mut self, ctx: &mut EventCtx<'_>, local_pos: Point) -> bool {
        let hit = match self.session.hit_test_point(local_pos, None) {
            Some(h) => h,
            None => return false,
        };

        // Select just this point
        self.session.selection = crate::editing::Selection::new();
        self.session.selection.insert(hit.entity);

        // Toggle smooth ↔ corner
        self.session.toggle_point_type();

        self.record_edit(crate::editing::EditType::Normal);
        self.session.sync_to_workspace();
        self.emit_session_update(ctx, false);
        ctx.request_render();
        true
    }

    fn handle_component_double_click(&mut self, ctx: &mut EventCtx<'_>, local_pos: Point) -> bool {
        let component_id = match self.session.hit_test_component(local_pos) {
            Some(id) => id,
            None => return false,
        };

        let component = match self
            .session
            .glyph
            .components
            .iter()
            .find(|c| c.id == component_id)
        {
            Some(c) => c,
            None => return false,
        };

        let base_name = component.base.clone();
        tracing::info!(
            "Double-click on component '{}' - adding to buffer",
            base_name
        );

        if !self.session.add_glyph_to_buffer(&base_name) {
            return false;
        }

        self.session.clear_component_selection();
        self.emit_session_update(ctx, false);
        ctx.request_render();
        true
    }

    fn handle_kern_mode_activation(
        &mut self,
        ctx: &mut EventCtx<'_>,
        state: &masonry::core::PointerState,
        design_pos: Point,
    ) -> bool {
        if !self.session.text_mode_active || !state.modifiers.shift() {
            return false;
        }

        let sort_index = match self.find_sort_at_position(design_pos) {
            Some(idx) => idx,
            None => return false,
        };

        if sort_index == 0 {
            return false;
        }

        let current_kern = self.get_current_kern_value(sort_index);
        tracing::info!(
            "Entering kern mode for sort {}, current kern = {}",
            sort_index,
            current_kern
        );

        self.kern_mode_active = true;
        self.kern_sort_index = Some(sort_index);
        self.kern_start_x = design_pos.x;
        self.kern_original_value = current_kern;
        self.kern_current_offset = 0.0;

        self.activate_sort(sort_index);
        self.emit_session_update(ctx, false);
        ctx.request_render();
        true
    }

    fn get_current_kern_value(&self, sort_index: usize) -> f64 {
        let buffer = match &self.session.text_buffer {
            Some(b) => b,
            None => return 0.0,
        };

        let (curr_sort, prev_sort) = match (buffer.get(sort_index), buffer.get(sort_index - 1)) {
            (Some(c), Some(p)) => (c, p),
            _ => return 0.0,
        };

        let (curr_name, prev_name) = match (&curr_sort.kind, &prev_sort.kind) {
            (
                crate::sort::SortKind::Glyph { name: c, .. },
                crate::sort::SortKind::Glyph { name: p, .. },
            ) => (c, p),
            _ => return 0.0,
        };

        let workspace_arc = match &self.session.workspace {
            Some(ws) => ws,
            None => return 0.0,
        };
        let workspace = read_workspace(workspace_arc);

        let prev_glyph = workspace.get_glyph(prev_name);
        let curr_glyph = workspace.get_glyph(curr_name);

        crate::model::kerning::lookup_kerning(
            &workspace.kerning,
            &workspace.groups,
            prev_name,
            prev_glyph.and_then(|g| g.right_group.as_deref()),
            curr_name,
            curr_glyph.and_then(|g| g.left_group.as_deref()),
        )
    }

    fn dispatch_tool_mouse_down(
        &mut self,
        ctx: &mut EventCtx<'_>,
        local_pos: Point,
        state: &masonry::core::PointerState,
    ) {
        use crate::editing::{Modifiers, MouseButton, MouseEvent};
        use crate::tools::{ToolBox, ToolId};

        let mods = Modifiers {
            shift: state.modifiers.shift(),
            ctrl: state.modifiers.ctrl(),
            alt: state.modifiers.alt(),
            meta: state.modifiers.meta(),
        };

        let mouse_event = MouseEvent::with_modifiers(local_pos, Some(MouseButton::Left), mods);

        let select_tool = ToolBox::for_id(ToolId::Select);
        let mut tool = std::mem::replace(&mut self.session.current_tool, select_tool);
        self.mouse
            .mouse_down(mouse_event, &mut tool, &mut self.session);
        self.session.current_tool = tool;

        ctx.request_render();
    }

    /// Handle pointer move event
    pub(super) fn handle_pointer_move(
        &mut self,
        ctx: &mut EventCtx<'_>,
        current: &masonry::core::PointerState,
    ) {
        ctx.request_focus();
        let local_pos = ctx.local_position(current.position);

        if self.kern_mode_active {
            self.handle_kern_mode_drag(ctx, local_pos);
            return;
        }

        self.dispatch_tool_mouse_move(ctx, local_pos);
        self.maybe_request_render(ctx);
        self.maybe_emit_throttled_update(ctx);
    }

    fn handle_kern_mode_drag(&mut self, ctx: &mut EventCtx<'_>, local_pos: Point) {
        let design_pos = self.session.viewport.screen_to_design(local_pos);
        self.kern_current_offset = design_pos.x - self.kern_start_x;

        self.apply_kern_value();

        self.emit_session_update(ctx, false);
        ctx.request_render();
    }

    fn apply_kern_value(&mut self) {
        let sort_index = match self.kern_sort_index {
            Some(idx) => idx,
            None => return,
        };

        let buffer = match &self.session.text_buffer {
            Some(b) => b,
            None => return,
        };

        let (curr_sort, prev_sort) = match (buffer.get(sort_index), buffer.get(sort_index - 1)) {
            (Some(c), Some(p)) => (c, p),
            _ => return,
        };

        let (curr_name, prev_name) = match (&curr_sort.kind, &prev_sort.kind) {
            (
                crate::sort::SortKind::Glyph { name: c, .. },
                crate::sort::SortKind::Glyph { name: p, .. },
            ) => (c, p),
            _ => return,
        };

        let workspace_arc = match &self.session.workspace {
            Some(ws) => ws,
            None => return,
        };

        let new_kern_value = self.kern_original_value + self.kern_current_offset;
        let mut workspace = write_workspace(workspace_arc);

        if new_kern_value == 0.0 {
            if let Some(first_pairs) = workspace.kerning.get_mut(prev_name) {
                first_pairs.remove(curr_name);
            }
        } else {
            workspace
                .kerning
                .entry(prev_name.clone())
                .or_default()
                .insert(curr_name.clone(), new_kern_value);
        }
    }

    fn dispatch_tool_mouse_move(&mut self, _ctx: &mut EventCtx<'_>, local_pos: Point) {
        use crate::editing::MouseEvent;
        use crate::tools::{ToolBox, ToolId};

        let mouse_event = MouseEvent::new(local_pos, None);
        let select_tool = ToolBox::for_id(ToolId::Select);
        let mut tool = std::mem::replace(&mut self.session.current_tool, select_tool);
        self.mouse
            .mouse_moved(mouse_event, &mut tool, &mut self.session);
        self.session.current_tool = tool;
    }

    fn maybe_request_render(&self, ctx: &mut EventCtx<'_>) {
        use crate::tools::ToolId;

        let needs_render = ctx.is_active() || self.session.current_tool.id() == ToolId::Pen;
        if needs_render {
            ctx.request_render();
        }
    }

    fn maybe_emit_throttled_update(&mut self, ctx: &mut EventCtx<'_>) {
        if !ctx.is_active() {
            return;
        }

        self.drag_update_counter += 1;
        let throttle = settings::performance::DRAG_UPDATE_THROTTLE;

        if self.drag_update_counter.is_multiple_of(throttle) {
            self.session.update_coord_selection();
            self.emit_session_update(ctx, false);
        }
    }

    /// Handle pointer up event
    pub(super) fn handle_pointer_up(
        &mut self,
        ctx: &mut EventCtx<'_>,
        state: &masonry::core::PointerState,
    ) {
        let local_pos = ctx.local_position(state.position);

        if self.kern_mode_active {
            self.handle_kern_mode_release(ctx);
            return;
        }

        self.dispatch_tool_mouse_up(ctx, local_pos, state);
        self.finish_pointer_up(ctx);
    }

    fn handle_kern_mode_release(&mut self, ctx: &mut EventCtx<'_>) {
        let final_kern_value = self.kern_original_value + self.kern_current_offset;
        tracing::info!("Kern mode released: final value = {}", final_kern_value);

        self.kern_mode_active = false;
        self.kern_sort_index = None;
        self.kern_original_value = 0.0;
        self.kern_current_offset = 0.0;

        self.emit_session_update(ctx, false);
        ctx.request_render();
    }

    fn dispatch_tool_mouse_up(
        &mut self,
        _ctx: &mut EventCtx<'_>,
        local_pos: Point,
        state: &masonry::core::PointerState,
    ) {
        use crate::editing::{Modifiers, MouseButton, MouseEvent};
        use crate::tools::{ToolBox, ToolId};

        let mods = Modifiers {
            shift: state.modifiers.shift(),
            ctrl: state.modifiers.ctrl(),
            alt: state.modifiers.alt(),
            meta: state.modifiers.meta(),
        };

        let mouse_event = MouseEvent::with_modifiers(local_pos, Some(MouseButton::Left), mods);

        let select_tool = ToolBox::for_id(ToolId::Select);
        let mut tool = std::mem::replace(&mut self.session.current_tool, select_tool);
        self.mouse
            .mouse_up(mouse_event, &mut tool, &mut self.session);

        if let Some(edit_type) = tool.edit_type() {
            self.session.snap_selection_to_grid();
            self.record_edit(edit_type);
            self.session.sync_to_workspace();
        }

        self.session.current_tool = tool;
    }

    fn finish_pointer_up(&mut self, ctx: &mut EventCtx<'_>) {
        self.session.update_coord_selection();
        self.drag_update_counter = 0;

        self.emit_session_update(ctx, false);
        ctx.release_pointer();
        ctx.request_render();
    }

    /// Handle pointer cancel event
    pub(super) fn handle_pointer_cancel(&mut self, ctx: &mut EventCtx<'_>) {
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
    pub(super) fn handle_scroll_zoom(&mut self, ctx: &mut EventCtx<'_>, delta: &ScrollDelta) {
        // Extract the Y component of the scroll delta
        // Negative Y = scroll up = zoom in
        // Positive Y = scroll down = zoom out
        let scroll_y = match delta {
            ScrollDelta::LineDelta(_x, y) => *y,
            ScrollDelta::PixelDelta(pos) => (pos.y / 10.0) as f32, // Scale down pixel deltas
            ScrollDelta::PageDelta(_x, y) => *y * 3.0,             // Page scrolls are bigger
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
            .clamp(settings::editor::MIN_ZOOM, settings::editor::MAX_ZOOM);

        self.session.viewport.zoom = new_zoom;
        tracing::debug!(
            "Scroll zoom: scroll_y={:.2}, new zoom={:.2}",
            scroll_y,
            new_zoom
        );

        ctx.request_render();
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
            let distance =
                ((position.x - last_pos.x).powi(2) + (position.y - last_pos.y).powi(2)).sqrt();

            time_diff < DOUBLE_CLICK_TIME_MS && distance < DOUBLE_CLICK_DISTANCE_PX
        } else {
            false
        };

        if is_double {
            // Reset tracking so the next click starts fresh
            // and doesn't cascade into triple/quadruple clicks
            self.last_click_time = None;
            self.last_click_position = None;
        } else {
            self.last_click_time = Some(now);
            self.last_click_position = Some(position);
        }

        is_double
    }

    /// Find which sort is at the given design-space position
    ///
    /// Returns the index of the sort, or None if no sort was clicked
    fn find_sort_at_position(&self, position: Point) -> Option<usize> {
        let buffer = self.session.text_buffer.as_ref()?;

        // Check text direction for RTL support
        let is_rtl = self.session.text_direction.is_rtl();

        // For RTL: calculate total width first so we can start from the right
        let total_width = if is_rtl {
            self.calculate_buffer_width()
        } else {
            0.0
        };

        let mut x_offset = if is_rtl { total_width } else { 0.0 };
        let mut baseline_y = 0.0;
        let upm_height = self.session.ascender - self.session.descender;

        // Track previous glyph for kerning lookup
        let mut prev_glyph_name: Option<String> = None;
        let mut prev_glyph_group: Option<String> = None;

        for (index, sort) in buffer.iter().enumerate() {
            match &sort.kind {
                crate::sort::SortKind::Glyph {
                    name,
                    advance_width,
                    ..
                } => {
                    // For RTL: move x left BEFORE processing this glyph
                    if is_rtl {
                        x_offset -= advance_width;
                    }

                    // Apply kerning if we have a previous glyph
                    if let Some(prev_name) = &prev_glyph_name
                        && let Some(workspace_arc) = &self.session.workspace
                    {
                        let workspace = read_workspace(workspace_arc);

                        // Get current glyph's left kerning group
                        let curr_group = workspace
                            .get_glyph(name)
                            .and_then(|g| g.left_group.as_deref());

                        // Look up kerning value
                        let kern_value = crate::model::kerning::lookup_kerning(
                            &workspace.kerning,
                            &workspace.groups,
                            prev_name,
                            prev_glyph_group.as_deref(),
                            name,
                            curr_group,
                        );

                        if is_rtl {
                            x_offset -= kern_value;
                        } else {
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

                    // For LTR: advance x forward AFTER processing
                    if !is_rtl {
                        x_offset += advance_width;
                    }

                    // Update previous glyph info for next iteration
                    prev_glyph_name = Some(name.clone());
                    if let Some(workspace_arc) = &self.session.workspace {
                        let workspace = read_workspace(workspace_arc);
                        prev_glyph_group = workspace
                            .get_glyph(name)
                            .and_then(|g| g.right_group.clone());
                    }
                }
                crate::sort::SortKind::LineBreak => {
                    x_offset = if is_rtl { total_width } else { 0.0 };
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
        // Calculate RTL info early, before mutable borrow of buffer
        let is_rtl = self.session.text_direction.is_rtl();
        let total_width = if is_rtl {
            self.calculate_buffer_width()
        } else {
            0.0
        };

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
            crate::sort::SortKind::Glyph {
                name, codepoint, ..
            } => {
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

        let workspace_guard = read_workspace(workspace);
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
            .map(crate::path::Path::from_contour)
            .collect();

        let mut x_offset = if is_rtl { total_width } else { 0.0 };
        let mut prev_glyph_name: Option<String> = None;
        let mut prev_glyph_group: Option<String> = None;

        // Iterate through all sorts up to and including the target sort
        // For RTL, we need to include the target because x_offset is decremented BEFORE drawing
        let end_index = if is_rtl { sort_index + 1 } else { sort_index };

        for i in 0..end_index {
            if let Some(sort) = buffer.get(i) {
                match &sort.kind {
                    crate::sort::SortKind::Glyph {
                        name,
                        advance_width,
                        ..
                    } => {
                        // For RTL: move x left BEFORE processing this glyph
                        if is_rtl {
                            x_offset -= advance_width;
                        }

                        // Apply kerning if we have a previous glyph
                        if let Some(prev_name) = &prev_glyph_name {
                            // Get current glyph's left kerning group
                            let curr_group = workspace_guard
                                .get_glyph(name)
                                .and_then(|g| g.left_group.as_deref());

                            // Look up kerning value
                            let kern_value = crate::model::kerning::lookup_kerning(
                                &workspace_guard.kerning,
                                &workspace_guard.groups,
                                prev_name,
                                prev_glyph_group.as_deref(),
                                name,
                                curr_group,
                            );

                            if is_rtl {
                                x_offset -= kern_value;
                            } else {
                                x_offset += kern_value;
                            }
                        }

                        // For LTR: advance x forward AFTER processing
                        if !is_rtl {
                            x_offset += advance_width;
                        }

                        // Update previous glyph info for next iteration
                        prev_glyph_name = Some(name.clone());
                        prev_glyph_group = workspace_guard
                            .get_glyph(name)
                            .and_then(|g| g.right_group.clone());
                    }
                    crate::sort::SortKind::LineBreak => {
                        // Reset x_offset for new line
                        x_offset = if is_rtl { total_width } else { 0.0 };
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
}

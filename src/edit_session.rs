// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Edit session - manages editing state for a single glyph

use crate::components::CoordinateSelection;
use crate::hit_test::{self, HitTestResult};
use crate::hyper_path::HyperPath;
use crate::path::Path;
use crate::selection::Selection;
use crate::shaping::{ArabicShaper, GlyphProvider, TextDirection};
use crate::sort::SortBuffer;
use crate::tools::{ToolBox, ToolId};
use crate::viewport::ViewPort;
use crate::workspace::{Glyph, Workspace};
use kurbo::{Point, Rect};
use std::sync::{Arc, RwLock};

// CoordinateSelection has been moved to components::coordinate_panel
// module

/// Editing session for text buffer editing
///
/// This holds all the state needed to edit a text buffer, including the
/// outline data for the active sort, selection, viewport, and metadata.
///
/// The session is no longer tied to a specific glyph - instead it tracks
/// which sort in the buffer is currently active for editing.
#[derive(Debug, Clone)]
pub struct EditSession {

    /// Path to the UFO file
    #[allow(dead_code)]
    pub ufo_path: std::path::PathBuf,

    /// The original glyph data for the active sort (for metadata, unicode, etc.)
    /// None when no sort is active
    pub glyph: Arc<Glyph>,

    /// The editable path representation (converted from active sort's glyph contours)
    /// Empty when no sort is active
    pub paths: Arc<Vec<Path>>,

    /// Currently selected entities (points, paths, etc.)
    pub selection: Selection,

    /// Currently selected component (if any)
    /// Components are selected separately from points since they have
    /// different selection/drag behavior
    pub selected_component: Option<crate::entity_id::EntityId>,

    /// Coordinate selection (for the coordinate pane)
    pub coord_selection: CoordinateSelection,

    /// Current editing tool
    pub current_tool: ToolBox,

    /// Viewport transformation
    pub viewport: ViewPort,

    /// Whether the viewport has been initialized (to avoid
    /// recalculating on every frame)
    pub viewport_initialized: bool,

    /// Font metrics (for drawing guides)
    #[allow(dead_code)] // Stored for potential future use
    pub units_per_em: f64,
    pub ascender: f64,
    pub descender: f64,
    pub x_height: Option<f64>,
    pub cap_height: Option<f64>,

    /// Text buffer for multi-glyph editing (Phase 2+)
    /// When Some, the session can switch between single-glyph and text editing modes
    pub text_buffer: Option<SortBuffer>,

    /// Whether text editing mode is currently active
    /// When true, render and interact with text buffer (cursor, typing, etc.)
    /// When false, use traditional single-glyph editing (select/pen tools)
    pub text_mode_active: bool,

    /// Reference to the workspace for character-to-glyph mapping (Phase 5+)
    /// Optional because not all sessions need text editing capabilities
    /// Wrapped in RwLock to allow updates during editing
    pub workspace: Option<Arc<RwLock<Workspace>>>,

    /// Index of the active sort in the buffer
    /// None when no sort is active (e.g., empty buffer)
    pub active_sort_index: Option<usize>,

    /// Unicode value of the active sort (e.g., "U+0052" for "R")
    /// None when no sort is active
    pub active_sort_unicode: Option<String>,

    /// Glyph name of the active sort (e.g., "R")
    /// Used as backup when unicode is not available
    /// None when no sort is active
    pub active_sort_name: Option<String>,

    /// X-offset position of the active sort in the text buffer
    /// Used to translate hit-testing coordinates so tools work correctly
    /// on sorts that aren't at position 0
    pub active_sort_x_offset: f64,

    /// Text direction for rendering and cursor movement
    /// Defaults to LTR, can be toggled via toolbar when text tool is active
    pub text_direction: TextDirection,
}

impl EditSession {
    /// Create a new editing session for a glyph (legacy, use new_with_text_buffer instead)
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        glyph_name: String,
        ufo_path: std::path::PathBuf,
        glyph: Glyph,
        units_per_em: f64,
        ascender: f64,
        descender: f64,
        x_height: Option<f64>,
        cap_height: Option<f64>,
    ) -> Self {
        // Convert glyph contours to editable paths
        let paths: Vec<Path> = glyph
            .contours
            .iter()
            .map(Path::from_contour)
            .collect();

        // Get unicode for display
        let unicode_value = glyph.codepoints.first().map(|cp| {
            format!("U+{:04X}", *cp as u32)
        });

        Self {
            ufo_path,
            glyph: Arc::new(glyph),
            paths: Arc::new(paths),
            selection: Selection::new(),
            selected_component: None,
            coord_selection: CoordinateSelection::default(),
            current_tool: ToolBox::for_id(ToolId::Select),
            viewport: ViewPort::new(),
            viewport_initialized: false,
            units_per_em,
            ascender,
            descender,
            x_height,
            cap_height,
            text_buffer: None,
            text_mode_active: false,
            workspace: None,
            active_sort_index: None,  // No buffer, no active sort
            active_sort_unicode: unicode_value,
            active_sort_name: Some(glyph_name),
            active_sort_x_offset: 0.0,
            text_direction: TextDirection::default(),
        }
    }

    /// Create a new editing session with text buffer initialized
    ///
    /// This creates a session with a text buffer containing the initial glyph as the first sort.
    /// The session starts in select mode (text_mode_active = false) with the first sort active.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_text_buffer(
        glyph_name: String,
        ufo_path: std::path::PathBuf,
        glyph: Glyph,
        units_per_em: f64,
        ascender: f64,
        descender: f64,
        x_height: Option<f64>,
        cap_height: Option<f64>,
    ) -> Self {
        // Convert glyph contours to editable paths
        let paths: Vec<Path> = glyph
            .contours
            .iter()
            .map(Path::from_contour)
            .collect();

        // Get unicode for display
        let unicode_value = glyph.codepoints.first().map(|cp| {
            format!("U+{:04X}", *cp as u32)
        });

        // Create text buffer with initial sort
        let mut buffer = SortBuffer::new();
        let initial_sort = crate::sort::Sort::new_glyph(
            glyph_name.clone(),
            glyph.codepoints.first().copied(),
            glyph.width,
            true, // First sort is active by default
        );
        buffer.insert(initial_sort);

        Self {
            ufo_path,
            glyph: Arc::new(glyph),
            paths: Arc::new(paths),
            selection: Selection::new(),
            selected_component: None,
            coord_selection: CoordinateSelection::default(),
            current_tool: ToolBox::for_id(ToolId::Select),
            viewport: ViewPort::new(),
            viewport_initialized: false,
            units_per_em,
            ascender,
            descender,
            x_height,
            cap_height,
            text_buffer: Some(buffer),
            text_mode_active: false, // Start in select mode (not text mode)
            workspace: None,
            active_sort_index: Some(0), // First sort is active
            active_sort_unicode: unicode_value,
            active_sort_name: Some(glyph_name),
            active_sort_x_offset: 0.0, // First sort is at position 0
            text_direction: TextDirection::default(),
        }
    }

    /// Enter text editing mode
    ///
    /// This switches the session to text buffer editing mode where multiple glyphs
    /// can be edited in a line.
    pub fn enter_text_mode(&mut self) {
        if self.text_buffer.is_some() {
            self.text_mode_active = true;
        }
    }

    /// Exit text editing mode
    ///
    /// This switches back to single-glyph editing mode, where only the active
    /// sort's glyph is editable.
    pub fn exit_text_mode(&mut self) {
        self.text_mode_active = false;
    }

    /// Check if text mode is available (text buffer exists)
    pub fn has_text_buffer(&self) -> bool {
        self.text_buffer.is_some()
    }

    /// Get the line height for text layout (UPM - descender)
    #[allow(dead_code)]
    pub fn line_height(&self) -> f64 {
        self.units_per_em - self.descender
    }

    /// Create a Sort from a character using the workspace character map (Phase 5)
    ///
    /// Looks up the glyph for the given character and creates a Sort.
    /// If the character matches the currently active sort, uses the live edited
    /// glyph width instead of the workspace version.
    ///
    /// Returns None if:
    /// - No workspace is available
    /// - Character has no mapped glyph
    /// - Glyph data cannot be found
    pub fn create_sort_from_char(&self, c: char) -> Option<crate::sort::Sort> {
        tracing::debug!("[create_sort_from_char] Trying to create sort for character: '{}' (U+{:04X})", c, c as u32);

        let workspace_lock = self.workspace.as_ref()?;
        let workspace = workspace_lock.read().unwrap();

        tracing::debug!("[create_sort_from_char] Workspace has {} glyphs", workspace.glyphs.len());

        // Find a glyph with this codepoint
        let (glyph_name, glyph) = workspace.glyphs.iter()
            .find(|(_, g)| g.codepoints.contains(&c))?;

        tracing::debug!("[create_sort_from_char] Found glyph: '{}' for character '{}'", glyph_name, c);

        // Check if this character matches the currently active sort's character
        // If so, use the current glyph (with live edits) instead of workspace version
        let advance_width = if self.active_sort_unicode.as_ref()
            .and_then(|u| u.strip_prefix("U+").and_then(|hex| u32::from_str_radix(hex, 16).ok()))
            .and_then(char::from_u32)
            .map(|active_char| active_char == c)
            .unwrap_or(false)
        {
            // Active sort matches this character - use current glyph width
            self.glyph.width
        } else {
            // Different character - use workspace version
            glyph.width
        };

        Some(crate::sort::Sort::new_glyph(
            glyph_name.clone(),
            Some(c),
            advance_width,
            false, // New sorts are inactive by default
        ))
    }

    /// Create a Sort from a character with Arabic shaping support.
    ///
    /// When text direction is RTL and the character is Arabic, this method:
    /// 1. Determines the correct positional form based on buffer contents
    /// 2. Uses the shaped glyph name (with suffix like .init, .medi, .fina)
    /// 3. Returns a sort with the shaped glyph name
    ///
    /// For LTR or non-Arabic characters, falls back to `create_sort_from_char`.
    pub fn create_shaped_sort_from_char(&self, c: char) -> Option<crate::sort::Sort> {
        // Only use Arabic shaping for RTL mode and Arabic characters
        if self.text_direction != TextDirection::RightToLeft
            || !crate::shaping::is_arabic(c)
        {
            return self.create_sort_from_char(c);
        }

        let workspace_lock = self.workspace.as_ref()?;
        let workspace = workspace_lock.read().unwrap();
        let font = WorkspaceGlyphProvider::new(&workspace);

        // Get the current text from buffer to determine context
        let buffer = self.text_buffer.as_ref()?;
        let cursor_pos = buffer.cursor();

        // Build character array from buffer for context-aware shaping
        let mut chars: Vec<char> = buffer
            .iter()
            .filter_map(|sort| match &sort.kind {
                crate::sort::SortKind::Glyph { codepoint, .. } => *codepoint,
                _ => None,
            })
            .collect();

        // Insert the new character at cursor position for shaping
        chars.insert(cursor_pos, c);

        // Shape the new character
        let shaper = ArabicShaper::new();
        let shaped = shaper.shape_char_at(&chars, cursor_pos, &font)?;

        tracing::debug!(
            "[create_shaped_sort_from_char] Character '{}' shaped to '{}' ({:?})",
            c,
            shaped.glyph_name,
            shaped.form
        );

        Some(crate::sort::Sort::new_glyph(
            shaped.glyph_name,
            Some(c),
            shaped.advance_width,
            false,
        ))
    }

    /// Reshape sorts in the buffer around a position after an edit.
    ///
    /// This updates the glyph names of affected sorts to reflect their new
    /// positional forms. Call this after inserting or deleting a character.
    ///
    /// Returns the range of indices that were updated.
    pub fn reshape_buffer_around(&mut self, position: usize) -> Option<(usize, usize)> {
        // Only reshape in RTL mode
        if self.text_direction != TextDirection::RightToLeft {
            return None;
        }

        let workspace_lock = self.workspace.as_ref()?.clone();
        let workspace = workspace_lock.read().unwrap();
        let font = WorkspaceGlyphProvider::new(&workspace);

        let buffer = self.text_buffer.as_mut()?;

        // Build character array from buffer
        let chars: Vec<char> = buffer
            .iter()
            .filter_map(|sort| match &sort.kind {
                crate::sort::SortKind::Glyph { codepoint, .. } => *codepoint,
                _ => None,
            })
            .collect();

        if chars.is_empty() {
            return None;
        }

        // Determine range to reshape (position ± 1, clamped)
        let start = position.saturating_sub(1);
        let end = (position + 1).min(chars.len());

        let shaper = ArabicShaper::new();

        // Reshape each character in the affected range
        for i in start..end {
            if i >= chars.len() {
                continue;
            }

            let c = chars[i];

            // Only reshape Arabic characters
            if !crate::shaping::is_arabic(c) {
                continue;
            }

            if let Some(shaped) = shaper.shape_char_at(&chars, i, &font) {
                // Update the sort at this position
                if let Some(sort) = buffer.get_mut(i)
                    && let crate::sort::SortKind::Glyph { name, advance_width, .. } = &mut sort.kind
                        && *name != shaped.glyph_name {
                            tracing::debug!(
                                "[reshape_buffer_around] Updated sort {}: '{}' -> '{}'",
                                i,
                                name,
                                shaped.glyph_name
                            );
                            *name = shaped.glyph_name.clone();
                            *advance_width = shaped.advance_width;
                        }
            }
        }

        Some((start, end))
    }

    /// Find and activate the sort at a given position (Phase 7)
    ///
    /// Hit tests the position against each sort's bounding box and activates
    /// the clicked sort. This loads the glyph's paths from the workspace into
    /// session.paths for editing. Returns true if a sort was found and activated.
    pub fn activate_sort_at_position(&mut self, pos: Point) -> bool {
        // First, find which sort was clicked (if any) and its x-offset
        let sort_to_activate: Option<(usize, String, Option<char>, f64)> = {
            let buffer = match &self.text_buffer {
                Some(buf) => buf,
                None => return false,
            };

            // Calculate sort positions and check for hit
            let mut x_offset = 0.0;

            let mut found_sort = None;
            for (index, sort) in buffer.iter().enumerate() {
                match &sort.kind {
                    crate::sort::SortKind::Glyph { name, advance_width, codepoint } => {
                        // Check if click is within this sort's bounds
                        let sort_left = x_offset;
                        let sort_right = x_offset + advance_width;
                        let sort_top = self.ascender;
                        let sort_bottom = self.descender;

                        if pos.x >= sort_left && pos.x <= sort_right
                            && pos.y >= sort_bottom && pos.y <= sort_top
                        {
                            // Found the clicked sort - capture x_offset too
                            found_sort = Some((index, name.clone(), *codepoint, x_offset));
                            break;
                        }

                        x_offset += advance_width;
                    }
                    crate::sort::SortKind::LineBreak => {
                        x_offset = 0.0;
                    }
                }
            }
            found_sort
        };

        // If we found a sort to activate, load its paths
        if let Some((index, glyph_name, codepoint, x_offset)) = sort_to_activate {
            let workspace_lock = match &self.workspace {
                Some(ws) => ws,
                None => {
                    tracing::warn!("No workspace available to load glyph paths");
                    return false;
                }
            };
            let workspace = workspace_lock.read().unwrap();

            let glyph = match workspace.glyphs.get(&glyph_name) {
                Some(g) => g,
                None => {
                    tracing::warn!("Glyph '{}' not found in workspace", glyph_name);
                    return false;
                }
            };

            // Convert contours to paths
            let paths: Vec<Path> = glyph
                .contours
                .iter()
                .map(Path::from_contour)
                .collect();

            // Update session state with loaded paths AND glyph
            self.paths = std::sync::Arc::new(paths);
            self.glyph = std::sync::Arc::new(glyph.clone()); // Update glyph so to_glyph() has correct metadata
            self.active_sort_index = Some(index);
            self.active_sort_name = Some(glyph_name.clone());
            self.active_sort_unicode = codepoint.map(|c| format!("U+{:04X}", c as u32));
            self.active_sort_x_offset = x_offset;

            // Update buffer to mark this sort as active
            if let Some(buffer) = &mut self.text_buffer {
                buffer.set_active_sort(index);
            }

            tracing::info!(
                "Activated sort {} (glyph: {}, {} paths loaded, x_offset: {})",
                index,
                glyph_name,
                self.paths.len(),
                x_offset
            );

            return true;
        }

        false
    }

    /// Compute the coordinate selection from the current selection
    ///
    /// This calculates the bounding box of all selected points and
    /// updates the coord_selection field.
    pub fn update_coord_selection(&mut self) {
        if self.selection.is_empty() {
            self.coord_selection = CoordinateSelection::default();
            return;
        }

        let bbox = Self::calculate_selection_bbox(
            &self.paths,
            &self.selection,
        );

        match bbox {
            Some((count, frame)) => {
                self.coord_selection = CoordinateSelection::new(
                    count,
                    frame,
                    // Preserve the current quadrant selection
                    self.coord_selection.quadrant,
                );
            }
            None => {
                self.coord_selection = CoordinateSelection::default();
            }
        }
    }


    /// Hit test for a point at screen coordinates
    ///
    /// Returns the EntityId of the closest point within max_dist
    /// screen pixels
    pub fn hit_test_point(
        &self,
        screen_pos: Point,
        max_dist: Option<f64>,
    ) -> Option<HitTestResult> {
        let max_dist = max_dist.unwrap_or(hit_test::MIN_CLICK_DISTANCE);

        tracing::debug!(
            "[hit_test_point] screen_pos=({}, {}), offset={}, max_dist={}",
            screen_pos.x,
            screen_pos.y,
            self.active_sort_x_offset,
            max_dist
        );

        // Collect all points from all paths as screen coordinates
        // Apply active sort x-offset so hit-testing matches rendering position
        let candidates: Vec<_> = self.paths.iter().flat_map(|path| {
            Self::path_to_hit_candidates(path, &self.viewport, self.active_sort_x_offset)
        }).collect();

        tracing::debug!(
            "[hit_test_point] Found {} candidates",
            candidates.len()
        );

        if let Some(first) = candidates.first() {
            tracing::debug!(
                "[hit_test_point] First candidate: pos=({}, {})",
                first.1.x,
                first.1.y
            );
        }

        // Find closest point in screen space
        let result = hit_test::find_closest(screen_pos, candidates.into_iter(), max_dist);

        if let Some(ref hit) = result {
            tracing::debug!(
                "[hit_test_point] Hit found: entity={:?}, distance={}",
                hit.entity,
                hit.distance
            );
        } else {
            tracing::debug!("[hit_test_point] No hit found");
        }

        result
    }

    /// Hit test for path segments at screen coordinates
    ///
    /// Returns the closest segment within max_dist screen pixels,
    /// along with the parametric position (t) on that segment where
    /// the nearest point lies.
    ///
    /// The parameter t ranges from 0.0 (start of segment) to 1.0
    /// (end of segment).
    pub fn hit_test_segments(
        &self,
        screen_pos: Point,
        max_dist: f64,
    ) -> Option<(crate::path_segment::SegmentInfo, f64)> {
        // Convert screen position to design space
        let mut design_pos = self.viewport.screen_to_design(screen_pos);

        // Adjust for active sort offset - subtract offset so coordinates match paths at (0,0)
        design_pos.x -= self.active_sort_x_offset;

        let closest_segment = Self::find_closest_segment(
            &self.paths,
            design_pos,
        );

        // Check if the closest segment is within max_dist
        closest_segment.and_then(|(segment_info, t, dist_sq)| {
            // Convert max_dist from screen pixels to design units
            let max_dist_design = max_dist / self.viewport.zoom;
            let max_dist_sq = max_dist_design * max_dist_design;

            if dist_sq <= max_dist_sq {
                Some((segment_info, t))
            } else {
                None
            }
        })
    }

    /// Hit test for a component at screen coordinates
    ///
    /// Returns the EntityId of the component if the point is inside its filled area.
    /// Components are tested in reverse order so topmost components are hit first.
    pub fn hit_test_component(
        &self,
        screen_pos: Point,
    ) -> Option<crate::entity_id::EntityId> {
        use kurbo::Shape;

        // Convert screen position to design space
        let mut design_pos = self.viewport.screen_to_design(screen_pos);

        // Adjust for active sort offset
        design_pos.x -= self.active_sort_x_offset;

        // Get workspace to resolve component base glyphs
        let workspace = self.workspace.as_ref()?;
        let workspace_guard = workspace.read().ok()?;

        // Test each component in reverse order (topmost first)
        for component in self.glyph.components.iter().rev() {
            // Look up the base glyph
            let base_glyph = workspace_guard.glyphs.get(&component.base)?;

            // Build the component's path with transform applied
            let mut component_path = kurbo::BezPath::new();
            for contour in &base_glyph.contours {
                let path = crate::path::Path::from_contour(contour);
                let transformed = component.transform * path.to_bezpath();
                component_path.extend(transformed);
            }

            // Check if point is inside the component's path
            // winding() returns non-zero for points inside a filled region
            if component_path.winding(design_pos) != 0 {
                return Some(component.id);
            }
        }

        None
    }

    /// Move the selected component by a delta in design space
    ///
    /// This modifies the component's transform to translate it by the given delta.
    pub fn move_selected_component(&mut self, delta: kurbo::Vec2) {
        let Some(selected_id) = self.selected_component else {
            return;
        };

        // Get mutable access to the glyph
        let glyph = Arc::make_mut(&mut self.glyph);

        // Find and update the component with the selected ID
        for component in &mut glyph.components {
            if component.id == selected_id {
                component.translate(delta.x, delta.y);
                break;
            }
        }
    }

    /// Clear the component selection
    pub fn clear_component_selection(&mut self) {
        self.selected_component = None;
    }

    /// Select a component by its EntityId
    pub fn select_component(&mut self, id: crate::entity_id::EntityId) {
        // Clear point selection when selecting a component
        self.selection = Selection::new();
        self.selected_component = Some(id);
    }

    /// Add a glyph to the text buffer by name (for component base glyph editing)
    ///
    /// This looks up the glyph in the workspace and inserts it into the text buffer
    /// at the current cursor position. Used when double-clicking a component to
    /// add its base glyph to the buffer for editing.
    ///
    /// Returns true if the glyph was added successfully.
    pub fn add_glyph_to_buffer(&mut self, glyph_name: &str) -> bool {
        // Get the glyph from workspace
        let workspace = match &self.workspace {
            Some(ws) => ws.clone(),
            None => return false,
        };

        let workspace_guard = workspace.read().unwrap();
        let glyph = match workspace_guard.glyphs.get(glyph_name) {
            Some(g) => g.clone(),
            None => {
                tracing::warn!("Glyph '{}' not found in workspace", glyph_name);
                return false;
            }
        };

        // Get the advance width
        let advance_width = glyph.width as f64;

        // Get the first codepoint if any
        let codepoint = glyph.codepoints.first().copied();

        // Drop the lock before we modify the buffer
        drop(workspace_guard);

        // Create a new sort for this glyph
        let sort = crate::sort::Sort::new_glyph(
            glyph_name.to_string(),
            codepoint,
            advance_width,
            false, // Not active initially
        );

        // Insert into buffer
        if let Some(buffer) = &mut self.text_buffer {
            buffer.insert(sort);
            tracing::info!("Added glyph '{}' to buffer for editing", glyph_name);
            true
        } else {
            false
        }
    }

    /// Move selected points by a delta in design space
    ///
    /// This mutates the paths using Arc::make_mut, which will clone
    /// the path data if there are other references to it.
    ///
    /// When moving on-curve points, their adjacent off-curve control
    /// points (handles) are also moved to maintain curve shape. This
    /// is standard font editor behavior.
    pub fn move_selection(&mut self, delta: kurbo::Vec2) {
        if self.selection.is_empty() {
            return;
        }

        use crate::entity_id::EntityId;
        use std::collections::HashSet;

        // We need to mutate paths, so convert Arc<Vec<Path>> to
        // mutable Vec
        let paths_vec = Arc::make_mut(&mut self.paths);

        // Build a set of IDs to move:
        // - All selected points
        // - Adjacent off-curve points of selected on-curve points
        let mut points_to_move: HashSet<EntityId> =
            self.selection.iter().copied().collect();

        // First pass: identify adjacent off-curve points of selected
        // on-curve points
        Self::collect_adjacent_off_curve_points(
            paths_vec,
            &self.selection,
            &mut points_to_move,
        );

        // Second pass: move all identified points
        Self::apply_point_movement(paths_vec, &points_to_move, delta);
    }

    /// Nudge selected points in a direction
    ///
    /// Nudge amounts:
    /// - Normal: 1 unit
    /// - Shift: 10 units
    /// - Cmd/Ctrl: 100 units
    pub fn nudge_selection(
        &mut self,
        dx: f64,
        dy: f64,
        shift: bool,
        ctrl: bool,
    ) {
        let multiplier = if ctrl {
            100.0
        } else if shift {
            10.0
        } else {
            1.0
        };

        let delta = kurbo::Vec2::new(dx * multiplier, dy * multiplier);
        self.move_selection(delta);
    }

    /// Delete selected points
    ///
    /// This removes selected points from paths. If all points in a
    /// path are deleted, the entire path is removed.
    pub fn delete_selection(&mut self) {
        if self.selection.is_empty() {
            return;
        }

        // Get mutable access to paths
        let paths_vec = Arc::make_mut(&mut self.paths);

        // Filter out paths that become empty after deletion
        paths_vec.retain_mut(|path| {
            Self::retain_path_after_deletion(path, &self.selection)
        });

        // Clear selection since deleted points are gone
        self.selection = Selection::new();
    }

    /// Toggle point type between smooth and corner for selected
    /// on-curve points
    pub fn toggle_point_type(&mut self) {
        if self.selection.is_empty() {
            return;
        }

        let paths_vec = Arc::make_mut(&mut self.paths);

        for path in paths_vec.iter_mut() {
            Self::toggle_points_in_path(path, &self.selection);
        }
    }

    /// Reverse the direction of all paths
    pub fn reverse_contours(&mut self) {
        let paths_vec = Arc::make_mut(&mut self.paths);

        for path in paths_vec.iter_mut() {
            match path {
                Path::Cubic(cubic) => {
                    let points = cubic.points.make_mut();
                    points.reverse();
                }
                Path::Quadratic(quadratic) => {
                    let points = quadratic.points.make_mut();
                    points.reverse();
                }
                Path::Hyper(hyper) => {
                    let points = hyper.points.make_mut();
                    points.reverse();
                    hyper.after_change();
                }
            }
        }
    }

    /// Insert a point on a segment at position t
    ///
    /// This adds a new on-curve point to the path containing the
    /// given segment, at the parametric position t along that
    /// segment.
    ///
    /// For line segments: inserts one on-curve point
    /// For cubic curves: subdivides the curve, inserting 1 on-curve
    /// and 2 off-curve points
    ///
    /// Returns true if the point was successfully inserted.
    pub fn insert_point_on_segment(
        &mut self,
        segment_info: &crate::path_segment::SegmentInfo,
        t: f64,
    ) -> bool {
        use crate::path_segment::Segment;

        // Find the path containing this segment
        let paths_vec = Arc::make_mut(&mut self.paths);

        for path in paths_vec.iter_mut() {
            if let Some(points) =
                Self::find_path_containing_segment(path, segment_info)
            {
                match segment_info.segment {
                    Segment::Line(_line) => {
                        return Self::insert_point_on_line(
                            points,
                            segment_info,
                            t,
                        );
                    }
                    Segment::Cubic(cubic_bez) => {
                        return Self::insert_point_on_cubic(
                            points,
                            segment_info,
                            cubic_bez,
                            t,
                        );
                    }
                    Segment::Quadratic(quad_bez) => {
                        return Self::insert_point_on_quadratic(
                            points,
                            segment_info,
                            quad_bez,
                            t,
                        );
                    }
                }
            }
        }

        false
    }

    /// Convert the current editing state back to a Glyph
    ///
    /// This creates a new Glyph with the edited paths converted back
    /// to contours, preserving all other metadata from the original
    /// glyph.
    pub fn to_glyph(&self) -> Glyph {
        use crate::workspace::Glyph;

        // Convert paths back to contours
        let contours: Vec<crate::workspace::Contour> =
            self.paths.iter().map(|path| path.to_contour()).collect();

        // Create updated glyph with new contours but preserve other
        // metadata (including components)
        Glyph {
            name: self.glyph.name.clone(),
            width: self.glyph.width,
            height: self.glyph.height,
            codepoints: self.glyph.codepoints.clone(),
            contours,
            components: self.glyph.components.clone(),
            left_group: self.glyph.left_group.clone(),
            right_group: self.glyph.right_group.clone(),
        }
    }

    /// Sync current edits to the workspace immediately
    ///
    /// This updates the workspace with the current editing state so that
    /// all instances of the glyph in the text buffer show the latest edits.
    /// Should be called after any edit operation (move, delete, add points, etc.)
    pub fn sync_to_workspace(&mut self) {
        // Only sync if we have an active sort and workspace
        let glyph_name = match &self.active_sort_name {
            Some(name) => name.clone(),
            None => return,
        };

        let workspace_lock = match &self.workspace {
            Some(ws) => ws,
            None => return,
        };

        // Get the updated glyph
        let updated_glyph = self.to_glyph();

        // Update both the session's glyph and the workspace
        self.glyph = Arc::new(updated_glyph.clone());

        // Update the workspace
        let mut workspace = workspace_lock.write().unwrap();
        workspace.glyphs.insert(glyph_name, updated_glyph);
    }

    // ===== HELPER METHODS =====

    /// Calculate the bounding box of selected points
    fn calculate_selection_bbox(
        paths: &[Path],
        selection: &Selection,
    ) -> Option<(usize, Rect)> {
        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut count = 0;

        for path in paths.iter() {
            Self::collect_selected_points_from_path(
                path,
                selection,
                &mut min_x,
                &mut max_x,
                &mut min_y,
                &mut max_y,
                &mut count,
            );
        }

        if min_x.is_finite() {
            let frame = Rect::new(min_x, min_y, max_x, max_y);
            Some((count, frame))
        } else {
            None
        }
    }

    /// Collect selected points from a path for bounding box
    /// calculation
    fn collect_selected_points_from_path(
        path: &Path,
        selection: &Selection,
        min_x: &mut f64,
        max_x: &mut f64,
        min_y: &mut f64,
        max_y: &mut f64,
        count: &mut usize,
    ) {
        let points_iter: Box<dyn Iterator<Item = _>> = match path {
            Path::Cubic(cubic) => {
                Box::new(cubic.points.iter())
            }
            Path::Quadratic(quadratic) => {
                Box::new(quadratic.points.iter())
            }
            Path::Hyper(hyper) => {
                Box::new(hyper.points.iter())
            }
        };

        for pt in points_iter {
            if selection.contains(&pt.id) {
                *min_x = (*min_x).min(pt.point.x);
                *max_x = (*max_x).max(pt.point.x);
                *min_y = (*min_y).min(pt.point.y);
                *max_y = (*max_y).max(pt.point.y);
                *count += 1;
            }
        }
    }

    /// Convert a path to hit test candidates (for point hit testing)
    ///
    /// The offset_x parameter allows translating points in design space before
    /// converting to screen coordinates. This is used for active sorts in text
    /// buffers that aren't positioned at x=0.
    fn path_to_hit_candidates(
        path: &Path,
        viewport: &ViewPort,
        offset_x: f64,
    ) -> Vec<(crate::entity_id::EntityId, Point, bool)> {
        match path {
            Path::Cubic(cubic) => cubic
                .points()
                .iter()
                .map(|pt| {
                    // Apply x-offset in design space before converting to screen
                    let offset_point = Point::new(pt.point.x + offset_x, pt.point.y);
                    let screen_pt = viewport.to_screen(offset_point);
                    (pt.id, screen_pt, pt.is_on_curve())
                })
                .collect(),
            Path::Quadratic(quadratic) => quadratic
                .points()
                .iter()
                .map(|pt| {
                    // Apply x-offset in design space before converting to screen
                    let offset_point = Point::new(pt.point.x + offset_x, pt.point.y);
                    let screen_pt = viewport.to_screen(offset_point);
                    (pt.id, screen_pt, pt.is_on_curve())
                })
                .collect(),
            Path::Hyper(hyper) => hyper
                .points()
                .iter()
                .map(|pt| {
                    // Apply x-offset in design space before converting to screen
                    let offset_point = Point::new(pt.point.x + offset_x, pt.point.y);
                    let screen_pt = viewport.to_screen(offset_point);
                    (pt.id, screen_pt, pt.is_on_curve())
                })
                .collect(),
        }
    }

    /// Find the closest segment to a design space point
    fn find_closest_segment(
        paths: &[Path],
        design_pos: kurbo::Point,
    ) -> Option<(
        crate::path_segment::SegmentInfo,
        f64,
        f64,
    )> {
        let mut closest: Option<(
            crate::path_segment::SegmentInfo,
            f64,
            f64,
        )> = None;

        for path in paths.iter() {
            Self::process_path_segments(path, design_pos, &mut closest);
        }
        closest
    }

    /// Process segments from a single path and update closest segment
    fn process_path_segments(
        path: &Path,
        design_pos: kurbo::Point,
        closest: &mut Option<(
            crate::path_segment::SegmentInfo,
            f64,
            f64,
        )>,
    ) {
        match path {
            Path::Cubic(cubic) => {
                Self::process_path_segment_iterator(
                    cubic.iter_segments(),
                    design_pos,
                    closest,
                );
            }
            Path::Quadratic(quadratic) => {
                Self::process_path_segment_iterator(
                    quadratic.iter_segments(),
                    design_pos,
                    closest,
                );
            }
            Path::Hyper(hyper) => {
                Self::process_path_segment_iterator(
                    hyper.iter_segments(),
                    design_pos,
                    closest,
                );
            }
        }
    }

    /// Process an iterator of segments and update closest segment
    fn process_path_segment_iterator<I>(
        segments: I,
        design_pos: kurbo::Point,
        closest: &mut Option<(
            crate::path_segment::SegmentInfo,
            f64,
            f64,
        )>,
    ) where
        I: Iterator<Item = crate::path_segment::SegmentInfo>,
    {
        for segment_info in segments {
            let (t, dist_sq) = segment_info.segment.nearest(design_pos);
            Self::update_closest_segment(
                closest,
                segment_info,
                t,
                dist_sq,
            );
        }
    }

    /// Update the closest segment if this one is closer
    fn update_closest_segment(
        closest: &mut Option<(crate::path_segment::SegmentInfo, f64, f64)>,
        segment_info: crate::path_segment::SegmentInfo,
        t: f64,
        dist_sq: f64,
    ) {
        match closest {
            None => {
                *closest = Some((segment_info, t, dist_sq));
            }
            Some((_, _, best_dist_sq)) => {
                if dist_sq < *best_dist_sq {
                    *closest = Some((segment_info, t, dist_sq));
                }
            }
        }
    }

    /// Collect adjacent off-curve points for selected on-curve points
    fn collect_adjacent_off_curve_points(
        paths: &[Path],
        selection: &Selection,
        points_to_move: &mut std::collections::HashSet<
            crate::entity_id::EntityId,
        >,
    ) {
        for path in paths.iter() {
            match path {
                Path::Cubic(cubic) => {
                    Self::collect_adjacent_for_cubic(
                        cubic,
                        selection,
                        points_to_move,
                    );
                }
                Path::Quadratic(quadratic) => {
                    Self::collect_adjacent_for_quadratic(
                        quadratic,
                        selection,
                        points_to_move,
                    );
                }
                Path::Hyper(hyper) => {
                    Self::collect_adjacent_for_hyper(
                        hyper,
                        selection,
                        points_to_move,
                    );
                }
            }
        }
    }

    /// Collect adjacent off-curve points for a cubic path
    fn collect_adjacent_for_cubic(
        cubic: &crate::cubic_path::CubicPath,
        selection: &Selection,
        points_to_move: &mut std::collections::HashSet<
            crate::entity_id::EntityId,
        >,
    ) {
        let points: Vec<_> = cubic.points.iter().collect();
        let len = points.len();

        for i in 0..len {
            let point = points[i];

            // If this on-curve point is selected, mark its adjacent
            // off-curve points
            if point.is_on_curve() && selection.contains(&point.id) {
                // Check previous point
                if let Some(prev_i) =
                    Self::get_previous_index(i, len, cubic.closed)
                    && prev_i < len && points[prev_i].is_off_curve() {
                        points_to_move.insert(points[prev_i].id);
                    }

                // Check next point
                if let Some(next_i) = Self::get_next_index(i, len, cubic.closed)
                    && next_i < len && points[next_i].is_off_curve() {
                        points_to_move.insert(points[next_i].id);
                    }
            }
        }
    }

    /// Collect adjacent off-curve points for a quadratic path
    fn collect_adjacent_for_quadratic(
        quadratic: &crate::quadratic_path::QuadraticPath,
        selection: &Selection,
        points_to_move: &mut std::collections::HashSet<
            crate::entity_id::EntityId,
        >,
    ) {
        let points: Vec<_> = quadratic.points.iter().collect();
        let len = points.len();

        for i in 0..len {
            let point = points[i];

            // If this on-curve point is selected, mark its adjacent
            // off-curve points
            if point.is_on_curve() && selection.contains(&point.id) {
                // Check previous point
                if let Some(prev_i) =
                    Self::get_previous_index(i, len, quadratic.closed)
                    && prev_i < len && points[prev_i].is_off_curve() {
                        points_to_move.insert(points[prev_i].id);
                    }

                // Check next point
                if let Some(next_i) =
                    Self::get_next_index(i, len, quadratic.closed)
                    && next_i < len && points[next_i].is_off_curve() {
                        points_to_move.insert(points[next_i].id);
                    }
            }
        }
    }

    /// Collect adjacent off-curve points for a hyper path
    fn collect_adjacent_for_hyper(
        hyper: &HyperPath,
        selection: &Selection,
        points_to_move: &mut std::collections::HashSet<
            crate::entity_id::EntityId,
        >,
    ) {
        let points: Vec<_> = hyper.points.iter().collect();
        let len = points.len();

        for i in 0..len {
            let point = points[i];

            // If this on-curve point is selected, mark its adjacent
            // off-curve points
            if point.is_on_curve() && selection.contains(&point.id) {
                // Check previous point
                if let Some(prev_i) =
                    Self::get_previous_index(i, len, hyper.closed)
                    && prev_i < len && points[prev_i].is_off_curve() {
                        points_to_move.insert(points[prev_i].id);
                    }

                // Check next point
                if let Some(next_i) =
                    Self::get_next_index(i, len, hyper.closed)
                    && next_i < len && points[next_i].is_off_curve() {
                        points_to_move.insert(points[next_i].id);
                    }
            }
        }
    }

    /// Get the previous index in a path (with wrapping for closed
    /// paths)
    fn get_previous_index(
        current: usize,
        len: usize,
        closed: bool,
    ) -> Option<usize> {
        if current > 0 {
            Some(current - 1)
        } else if closed {
            Some(len - 1)
        } else {
            None
        }
    }

    /// Get the next index in a path (with wrapping for closed paths)
    fn get_next_index(
        current: usize,
        len: usize,
        closed: bool,
    ) -> Option<usize> {
        if current + 1 < len {
            Some(current + 1)
        } else if closed {
            Some(0)
        } else {
            None
        }
    }

    /// Apply point movement to paths
    fn apply_point_movement(
        paths: &mut [Path],
        points_to_move: &std::collections::HashSet<
            crate::entity_id::EntityId,
        >,
        delta: kurbo::Vec2,
    ) {
        for path in paths.iter_mut() {
            match path {
                Path::Cubic(cubic) => {
                    let points = cubic.points.make_mut();
                    Self::move_points_in_list(points, points_to_move, delta);
                }
                Path::Quadratic(quadratic) => {
                    let points = quadratic.points.make_mut();
                    Self::move_points_in_list(points, points_to_move, delta);
                }
                Path::Hyper(hyper) => {
                    let points = hyper.points.make_mut();
                    Self::move_points_in_list(points, points_to_move, delta);
                    hyper.after_change();
                }
            }
        }
    }

    /// Move points in a point list by delta
    fn move_points_in_list(
        points: &mut [crate::point::PathPoint],
        points_to_move: &std::collections::HashSet<
            crate::entity_id::EntityId,
        >,
        delta: kurbo::Vec2,
    ) {
        for point in points.iter_mut() {
            if points_to_move.contains(&point.id) {
                point.point = Point::new(
                    point.point.x + delta.x,
                    point.point.y + delta.y,
                );
            }
        }
    }

    /// Retain a path after deletion (remove selected points)
    fn retain_path_after_deletion(
        path: &mut Path,
        selection: &Selection,
    ) -> bool {
        match path {
            Path::Cubic(cubic) => {
                let points = cubic.points.make_mut();
                points.retain(|point| !selection.contains(&point.id));
                points.len() >= 2
            }
            Path::Quadratic(quadratic) => {
                let points = quadratic.points.make_mut();
                points.retain(|point| !selection.contains(&point.id));
                points.len() >= 2
            }
            Path::Hyper(hyper) => {
                let points = hyper.points.make_mut();
                points.retain(|point| !selection.contains(&point.id));
                let len = points.len();
                hyper.after_change();
                len >= 2
            }
        }
    }

    /// Toggle point types in a path
    fn toggle_points_in_path(path: &mut Path, selection: &Selection) {
        match path {
            Path::Cubic(cubic) => {
                let points = cubic.points.make_mut();
                Self::toggle_points_in_list(points, selection);
            }
            Path::Quadratic(quadratic) => {
                let points = quadratic.points.make_mut();
                Self::toggle_points_in_list(points, selection);
            }
            Path::Hyper(hyper) => {
                let points = hyper.points.make_mut();
                Self::toggle_points_in_list(points, selection);
                hyper.after_change();
            }
        }
    }

    /// Toggle point types in a point list
    fn toggle_points_in_list(
        points: &mut [crate::point::PathPoint],
        selection: &Selection,
    ) {
        for point in points.iter_mut() {
            if selection.contains(&point.id) {
                // Only toggle on-curve points
                if let crate::point::PointType::OnCurve { smooth } =
                    &mut point.typ
                {
                    *smooth = !*smooth;
                }
            }
        }
    }

    /// Find the path containing a segment and return its points
    fn find_path_containing_segment<'a>(
        path: &'a mut Path,
        segment_info: &crate::path_segment::SegmentInfo,
    ) -> Option<&'a mut Vec<crate::point::PathPoint>> {
        match path {
            Path::Cubic(cubic) => {
                if Self::cubic_contains_segment(cubic, segment_info) {
                    Some(cubic.points.make_mut())
                } else {
                    None
                }
            }
            Path::Quadratic(quadratic) => {
                if Self::quadratic_contains_segment(quadratic, segment_info) {
                    Some(quadratic.points.make_mut())
                } else {
                    None
                }
            }
            Path::Hyper(hyper) => {
                if Self::hyper_contains_segment(hyper, segment_info) {
                    Some(hyper.points.make_mut())
                } else {
                    None
                }
            }
        }
    }

    /// Check if a cubic path contains a specific segment
    fn cubic_contains_segment(
        cubic: &crate::cubic_path::CubicPath,
        segment_info: &crate::path_segment::SegmentInfo,
    ) -> bool {
        for seg in cubic.iter_segments() {
            if seg.start_index == segment_info.start_index
                && seg.end_index == segment_info.end_index
            {
                return true;
            }
        }
        false
    }

    /// Check if a quadratic path contains a specific segment
    fn quadratic_contains_segment(
        quadratic: &crate::quadratic_path::QuadraticPath,
        segment_info: &crate::path_segment::SegmentInfo,
    ) -> bool {
        for seg in quadratic.iter_segments() {
            if seg.start_index == segment_info.start_index
                && seg.end_index == segment_info.end_index
            {
                return true;
            }
        }
        false
    }

    /// Check if a hyper path contains a specific segment
    fn hyper_contains_segment(
        hyper: &HyperPath,
        segment_info: &crate::path_segment::SegmentInfo,
    ) -> bool {
        for seg in hyper.iter_segments() {
            if seg.start_index == segment_info.start_index
                && seg.end_index == segment_info.end_index
            {
                return true;
            }
        }
        false
    }

    /// Insert a point on a line segment
    fn insert_point_on_line(
        points: &mut Vec<crate::point::PathPoint>,
        segment_info: &crate::path_segment::SegmentInfo,
        t: f64,
    ) -> bool {
        use crate::entity_id::EntityId;
        use crate::point::{PathPoint, PointType};

        let point_pos = segment_info.segment.eval(t);
        let new_point = PathPoint {
            id: EntityId::next(),
            point: point_pos,
            typ: PointType::OnCurve { smooth: false },
        };

        // Insert between start and end
        let insert_idx = segment_info.end_index;
        points.insert(insert_idx, new_point);

        true
    }

    /// Insert a point on a cubic curve segment
    fn insert_point_on_cubic(
        points: &mut Vec<crate::point::PathPoint>,
        segment_info: &crate::path_segment::SegmentInfo,
        cubic_bez: kurbo::CubicBez,
        t: f64,
    ) -> bool {
        use crate::path_segment::Segment;

        // For a cubic curve, subdivide it using de Casteljau
        // algorithm
        let (left, right) = Segment::subdivide_cubic(cubic_bez, t);

        // Create the new points from subdivision
        let new_points = Self::create_cubic_subdivision_points(
            left,
            right,
        );

        // Calculate how many points are between start and end
        let points_between =
            Self::calculate_points_between(
                segment_info.start_index,
                segment_info.end_index,
                points.len(),
            );

        // Remove the old control points
        if points_between > 0 {
            for _ in 0..points_between {
                points.remove(segment_info.start_index + 1);
            }
        }

        // Insert the new points after start_index
        let mut insert_idx = segment_info.start_index + 1;
        for new_point in new_points {
            points.insert(insert_idx, new_point);
            insert_idx += 1;
        }

        true
    }

    /// Create points from cubic curve subdivision
    fn create_cubic_subdivision_points(
        left: kurbo::CubicBez,
        right: kurbo::CubicBez,
    ) -> Vec<crate::point::PathPoint> {
        use crate::entity_id::EntityId;
        use crate::point::{PathPoint, PointType};

        vec![
            PathPoint {
                id: EntityId::next(),
                point: left.p1,
                typ: PointType::OffCurve { auto: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: left.p2,
                typ: PointType::OffCurve { auto: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: left.p3, // Same as right.p0
                typ: PointType::OnCurve { smooth: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: right.p1,
                typ: PointType::OffCurve { auto: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: right.p2,
                typ: PointType::OffCurve { auto: false },
            },
        ]
    }

    /// Insert a point on a quadratic curve segment
    fn insert_point_on_quadratic(
        points: &mut Vec<crate::point::PathPoint>,
        segment_info: &crate::path_segment::SegmentInfo,
        quad_bez: kurbo::QuadBez,
        t: f64,
    ) -> bool {
        use crate::entity_id::EntityId;
        use crate::point::{PathPoint, PointType};
        use crate::path_segment::Segment;

        // For a quadratic curve, subdivide it using de Casteljau
        // algorithm
        let (left, right) = Segment::subdivide_quadratic(quad_bez, t);

        // Create the new points from subdivision
        let new_points = vec![
            PathPoint {
                id: EntityId::next(),
                point: left.p1,
                typ: PointType::OffCurve { auto: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: left.p2, // Same as right.p0
                typ: PointType::OnCurve { smooth: false },
            },
            PathPoint {
                id: EntityId::next(),
                point: right.p1,
                typ: PointType::OffCurve { auto: false },
            },
        ];

        // Calculate how many points are between start and end
        let points_between = Self::calculate_points_between(
            segment_info.start_index,
            segment_info.end_index,
            points.len(),
        );

        // Remove the old control point
        if points_between > 0 {
            points.remove(segment_info.start_index + 1);
        }

        // Insert the new points after start_index
        let mut insert_idx = segment_info.start_index + 1;
        for new_point in new_points {
            points.insert(insert_idx, new_point);
            insert_idx += 1;
        }

        true
    }

    /// Calculate how many points are between start and end indices
    fn calculate_points_between(
        start_index: usize,
        end_index: usize,
        total_len: usize,
    ) -> usize {
        if end_index > start_index {
            end_index - start_index - 1
        } else {
            // Handle wrap-around for closed paths
            total_len - start_index - 1 + end_index
        }
    }
}

// ===== WORKSPACE GLYPH PROVIDER =====

/// Adapter that provides glyph information from the Workspace to the shaping engine.
///
/// This implements the `GlyphProvider` trait, allowing the Arabic shaper (and future
/// script shapers) to query glyph existence and advance widths without being coupled
/// to the Workspace type.
pub struct WorkspaceGlyphProvider<'a> {
    workspace: &'a Workspace,
}

impl<'a> WorkspaceGlyphProvider<'a> {
    /// Create a new provider wrapping a workspace reference.
    pub fn new(workspace: &'a Workspace) -> Self {
        Self { workspace }
    }
}

impl<'a> GlyphProvider for WorkspaceGlyphProvider<'a> {
    fn has_glyph(&self, name: &str) -> bool {
        self.workspace.glyphs.contains_key(name)
    }

    fn advance_width(&self, name: &str) -> Option<f64> {
        self.workspace.glyphs.get(name).map(|g| g.width)
    }

    fn base_glyph_for_codepoint(&self, c: char) -> Option<String> {
        // Find a glyph that has this codepoint
        self.workspace
            .glyphs
            .iter()
            .find(|(_, g)| g.codepoints.contains(&c))
            .map(|(name, _)| name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Glyph;

    fn create_test_glyph() -> Glyph {
        Glyph {
            name: "a".to_string(),
            width: 500.0,
            height: Some(700.0),
            codepoints: vec!['a'],
            contours: vec![],
            components: vec![],
            left_group: None,
            right_group: None,
        }
    }

    #[test]
    fn test_session_without_text_buffer() {
        let glyph = create_test_glyph();
        let session = EditSession::new(
            "a".to_string(),
            std::path::PathBuf::from("/test.ufo"),
            glyph,
            1000.0,
            800.0,
            -200.0,
            Some(500.0),
            Some(700.0),
        );

        assert!(!session.has_text_buffer());
        assert!(!session.text_mode_active);
        assert_eq!(session.glyph.name, "a");
    }

    #[test]
    fn test_session_with_text_buffer() {
        let glyph = create_test_glyph();
        let session = EditSession::new_with_text_buffer(
            "a".to_string(),
            std::path::PathBuf::from("/test.ufo"),
            glyph,
            1000.0,
            800.0,
            -200.0,
            Some(500.0),
            Some(700.0),
        );

        assert!(session.has_text_buffer());
        assert!(!session.text_mode_active); // Starts in single-glyph mode

        // Verify text buffer has one sort
        let buffer = session.text_buffer.as_ref().unwrap();
        assert_eq!(buffer.len(), 1);

        // Verify the sort is the initial glyph
        let sort = buffer.get(0).unwrap();
        assert_eq!(sort.glyph_name(), Some("a"));
        assert_eq!(sort.advance_width(), Some(500.0));
        assert!(sort.is_active);
    }

    #[test]
    fn test_text_mode_toggle() {
        let glyph = create_test_glyph();
        let mut session = EditSession::new_with_text_buffer(
            "a".to_string(),
            std::path::PathBuf::from("/test.ufo"),
            glyph,
            1000.0,
            800.0,
            -200.0,
            Some(500.0),
            Some(700.0),
        );

        assert!(!session.text_mode_active);

        // Enter text mode
        session.enter_text_mode();
        assert!(session.text_mode_active);

        // Exit text mode
        session.exit_text_mode();
        assert!(!session.text_mode_active);
    }

    #[test]
    fn test_line_height() {
        let glyph = create_test_glyph();
        let session = EditSession::new(
            "a".to_string(),
            std::path::PathBuf::from("/test.ufo"),
            glyph,
            1000.0,  // UPM
            800.0,   // ascender
            -200.0,  // descender
            Some(500.0),
            Some(700.0),
        );

        // Line height = UPM - descender = 1000 - (-200) = 1200
        assert_eq!(session.line_height(), 1200.0);
    }

    #[test]
    fn test_enter_text_mode_without_buffer() {
        let glyph = create_test_glyph();
        let mut session = EditSession::new(
            "a".to_string(),
            std::path::PathBuf::from("/test.ufo"),
            glyph,
            1000.0,
            800.0,
            -200.0,
            Some(500.0),
            Some(700.0),
        );

        assert!(!session.has_text_buffer());

        // Should not enter text mode if no buffer
        session.enter_text_mode();
        assert!(!session.text_mode_active);
    }
}


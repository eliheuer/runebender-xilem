// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Edit session - manages editing state for a single glyph

mod hit_testing;
mod path_editing;
mod text_buffer;

use super::selection::Selection;
use super::viewport::ViewPort;
use crate::components::CoordinateSelection;
use crate::model::workspace::{Glyph, Workspace};
use crate::path::Path;
use crate::shaping::{GlyphProvider, TextDirection};
use crate::sort::SortBuffer;
use crate::tools::{ToolBox, ToolId};
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
    pub selected_component: Option<crate::model::EntityId>,

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
        let paths: Vec<Path> = glyph.contours.iter().map(Path::from_contour).collect();

        // Get unicode for display
        let unicode_value = glyph
            .codepoints
            .first()
            .map(|cp| format!("U+{:04X}", *cp as u32));

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
            active_sort_index: None, // No buffer, no active sort
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
        let paths: Vec<Path> = glyph.contours.iter().map(Path::from_contour).collect();

        // Get unicode for display
        let unicode_value = glyph
            .codepoints
            .first()
            .map(|cp| format!("U+{:04X}", *cp as u32));

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
    pub fn select_component(&mut self, id: crate::model::EntityId) {
        // Clear point selection when selecting a component
        self.selection = Selection::new();
        self.selected_component = Some(id);
    }
}

// ============================================================================
// WORKSPACE GLYPH PROVIDER
// ============================================================================

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
    use crate::model::workspace::Glyph;

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
            mark_color: None,
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
            1000.0, // UPM
            800.0,  // ascender
            -200.0, // descender
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

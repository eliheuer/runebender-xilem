// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Theme colors and constants
//!
//! All colors use hexadecimal format: Color::from_rgb8(0xRR, 0xGG, 0xBB)

use masonry::vello::peniko::Color;

// ============================================================================
// BASE COLORS -- Generic colors for UI, a dark to light gradient by default
// ============================================================================
const BASE_A: Color = Color::from_rgb8(0x10, 0x10, 0x10);
const BASE_B: Color = Color::from_rgb8(0x20, 0x20, 0x20);
const BASE_C: Color = Color::from_rgb8(0x30, 0x30, 0x30);
const BASE_D: Color = Color::from_rgb8(0x40, 0x40, 0x40);
const BASE_E: Color = Color::from_rgb8(0x50, 0x50, 0x50);
const BASE_F: Color = Color::from_rgb8(0x60, 0x60, 0x60);
const BASE_G: Color = Color::from_rgb8(0x70, 0x70, 0x70);
const BASE_H: Color = Color::from_rgb8(0x80, 0x80, 0x80);
const BASE_I: Color = Color::from_rgb8(0x90, 0x90, 0x90);
const BASE_J: Color = Color::from_rgb8(0xa0, 0xa0, 0xa0);
const BASE_K: Color = Color::from_rgb8(0xb0, 0xb0, 0xb0);
const BASE_L: Color = Color::from_rgb8(0xc0, 0xc0, 0xc0);
const BASE_M: Color = Color::from_rgb8(0xd0, 0xd0, 0xd0);
const BASE_N: Color = Color::from_rgb8(0xe0, 0xe0, 0xe0);
const BASE_O: Color = Color::from_rgb8(0xf0, 0xf0, 0xf0);

// ============================================================================
// GLOBAL BACKGROUNDS -- Used for welcome screen, grid view, editor canvas, etc
// ============================================================================
const APP_BACKGROUND: Color = BASE_B;

// ============================================================================
// UI TEXT AND LABELS
// ============================================================================
const PRIMARY_UI_TEXT: Color = BASE_I;
const SECONDARY_UI_TEXT: Color = BASE_G;

// ============================================================================
// UI PANELS (Toolbar, Coordinate Panel, Glyph Preview)
// ============================================================================
const PANEL_BACKGROUND: Color = BASE_C;
const PANEL_OUTLINE: Color = BASE_F;
const GLYPH_PREVIEW_COLOR: Color = BASE_J;

// Coordinate Panel specific
const COORDINATE_PANEL_GRID_LINE: Color = BASE_I;

// ============================================================================
// TOOLBAR BUTTONS AND ICONS (Edit Mode, Shapes, Workspace)
// ============================================================================
// Button backgrounds (the filled rectangle behind each icon)
const TOOLBAR_BUTTON_UNSELECTED: Color = BASE_E; // 0x50 - Darker gray
const TOOLBAR_BUTTON_HOVERED: Color = BASE_H;    // 0x80 - Light gray
const TOOLBAR_BUTTON_SELECTED: Color = BASE_J;   // 0xa0 - Lighter gray

// Button outlines (the border around each button)
const TOOLBAR_BUTTON_OUTLINE: Color = BASE_A; // 0x10 - Very dark gray

// Icon colors (the SVG path stroke/fill inside each button)
const TOOLBAR_ICON_UNSELECTED: Color = BASE_I;   // 0x90 - Light gray
const TOOLBAR_ICON_HOVERED: Color = BASE_K;      // 0xb0 - Lighter gray
const TOOLBAR_ICON_SELECTED: Color = BASE_B;     // 0x20 - Very dark gray

// ============================================================================
// GLYPH GRID VIEW
// ============================================================================
// Grid cell backgrounds
const GRID_CELL_BACKGROUND: Color = BASE_C;
const GRID_CELL_OUTLINE: Color = BASE_F;
const GRID_CELL_SELECTED_BACKGROUND: Color = Color::from_rgb8(0x14, 0x64, 0x14);
const GRID_CELL_SELECTED_OUTLINE: Color = Color::from_rgb8(0x90, 0xee, 0x90);

// Glyph rendering in grid
const GRID_GLYPH_COLOR: Color = BASE_J;
const GRID_CELL_TEXT: Color = BASE_L;

// ============================================================================
// PATHS AND OUTLINES
// ============================================================================
const PATH_STROKE: Color = BASE_L;
const PATH_FILL: Color = BASE_F;
const PATH_PREVIEW_FILL: Color = BASE_L;

// ============================================================================
// COMPONENT COLORS
// Components are rendered in a distinct color to differentiate from editable paths
// ============================================================================
const COMPONENT_FILL: Color = Color::from_rgb8(0x66, 0x99, 0xCC); // Blue-gray tint

// ============================================================================
// METRICS GUIDES
// ============================================================================
const METRICS_GUIDE: Color = BASE_F;

// ============================================================================
// GRID
// ============================================================================
const GRID_LINE: Color = BASE_D;

// ============================================================================
// CONTROL POINT HANDLES
// ============================================================================
const HANDLE_LINE: Color = BASE_I;

// ============================================================================
// POINT COLORS
// ============================================================================

const SMOOTH_POINT_INNER: Color = Color::from_rgb8(0x57, 0x9a, 0xff);
const SMOOTH_POINT_OUTER: Color = Color::from_rgb8(0x44, 0x28, 0xec);

// Corner on-curve points (squares) - GREEN
const CORNER_POINT_INNER: Color = Color::from_rgb8(0x6a, 0xe7, 0x56);
const CORNER_POINT_OUTER: Color = Color::from_rgb8(0x20, 0x8e, 0x56);

const OFFCURVE_POINT_INNER: Color = Color::from_rgb8(0xcc, 0x99, 0xff);
const OFFCURVE_POINT_OUTER: Color = Color::from_rgb8(0x99, 0x00, 0xff);

// Hyperbezier on-curve points (cyan/teal - between smooth blue and offcurve purple)
const HYPER_POINT_INNER: Color = Color::from_rgb8(0x66, 0xcc, 0xdd);
const HYPER_POINT_OUTER: Color = Color::from_rgb8(0x00, 0x99, 0xaa);

const SELECTED_POINT_INNER: Color = Color::from_rgb8(0xff, 0xee, 0x55);
const SELECTED_POINT_OUTER: Color = Color::from_rgb8(0xff, 0xaa, 0x33);

// ============================================================================
// SELECTION RECTANGLE (Marquee)
// ============================================================================
const SELECTION_RECT_FILL: Color = Color::from_rgba8(0xff, 0xaa, 0x33, 0x20);
const SELECTION_RECT_STROKE: Color = Color::from_rgb8(0xff, 0xaa, 0x33);

// ============================================================================
// PUBLIC API - Don't edit below this line unless you know what you're doing
// ============================================================================

/// Grayscale gradient - generic neutral colors for UI
/// Use these base colors for consistent theming throughout the application
#[allow(dead_code)]
pub mod base {
    use super::Color;
    pub const A: Color = super::BASE_A;
    pub const B: Color = super::BASE_B;
    pub const C: Color = super::BASE_C;
    pub const D: Color = super::BASE_D;
    pub const E: Color = super::BASE_E;
    pub const F: Color = super::BASE_F;
    pub const G: Color = super::BASE_G;
    pub const H: Color = super::BASE_H;
    pub const I: Color = super::BASE_I;
    pub const J: Color = super::BASE_J;
    pub const K: Color = super::BASE_K;
    pub const L: Color = super::BASE_L;
    pub const M: Color = super::BASE_M;
    pub const N: Color = super::BASE_N;
    pub const O: Color = super::BASE_O;
}

/// Global application background color
pub mod app {
    use super::Color;
    pub const BACKGROUND: Color = super::APP_BACKGROUND;
}

/// Colors for editor canvas
pub mod canvas {
    use super::Color;
    pub const BACKGROUND: Color = super::APP_BACKGROUND;
}

/// Colors for UI text
pub mod text {
    use super::Color;
    pub const PRIMARY: Color = super::PRIMARY_UI_TEXT;
    pub const SECONDARY: Color = super::SECONDARY_UI_TEXT;
}

/// Colors for UI panels (toolbar, coordinate panel, glyph preview, etc.)
pub mod panel {
    use super::Color;
    pub const BACKGROUND: Color = super::PANEL_BACKGROUND;
    pub const OUTLINE: Color = super::PANEL_OUTLINE;
    pub const GLYPH_PREVIEW: Color = super::GLYPH_PREVIEW_COLOR;
}

/// Colors for toolbar buttons and icons (Edit Mode, Shapes, Workspace toolbars)
///
/// Each toolbar button has 3 visual components:
/// 1. Background fill - the button's background color
/// 2. Outline - the border around the button
/// 3. Icon - the SVG path color inside the button
///
/// Each component can be styled independently in different button states.
pub mod toolbar {
    use super::Color;

    // ===== BUTTON BACKGROUNDS =====
    /// Button background when unselected and not hovered
    pub const BUTTON_UNSELECTED: Color = super::TOOLBAR_BUTTON_UNSELECTED;
    /// Button background when hovered
    pub const BUTTON_HOVERED: Color = super::TOOLBAR_BUTTON_HOVERED;
    /// Button background when selected
    pub const BUTTON_SELECTED: Color = super::TOOLBAR_BUTTON_SELECTED;

    // ===== BUTTON OUTLINES =====
    /// Button outline/border color (same for all states)
    pub const BUTTON_OUTLINE: Color = super::TOOLBAR_BUTTON_OUTLINE;

    // ===== ICON COLORS =====
    /// Icon color when button is unselected and not hovered
    pub const ICON_UNSELECTED: Color = super::TOOLBAR_ICON_UNSELECTED;
    /// Icon color when button is hovered
    pub const ICON_HOVERED: Color = super::TOOLBAR_ICON_HOVERED;
    /// Icon color when button is selected
    pub const ICON_SELECTED: Color = super::TOOLBAR_ICON_SELECTED;
}

/// Colors and sizes for coordinate panel
pub mod coordinate_panel {
    use super::Color;
    pub const GRID_LINE: Color = super::COORDINATE_PANEL_GRID_LINE;
    #[allow(dead_code)]
    pub const TEXT: Color = super::PRIMARY_UI_TEXT;

    // Dot colors - selected (lighter for better visibility)
    pub const DOT_SELECTED_INNER: Color = super::BASE_H; // 0x80 - Light gray
    // 0x90 - Light gray, matches text
    pub const DOT_SELECTED_OUTER: Color = super::BASE_I;

    // Dot colors - unselected (darker gray)
    pub const DOT_UNSELECTED_INNER: Color = super::BASE_C; // 0x30 - Dark gray
    // 0x90 - Light gray, matches text
    pub const DOT_UNSELECTED_OUTER: Color = super::BASE_I;

    // Sizes (matching Runebender)
    pub const PADDING: f64 = 16.0; // Increased from 8px for more even margins
    // Larger selector for better visibility
    pub const SELECTOR_SIZE: f64 = 72.0;
    pub const DOT_RADIUS: f64 = 10.0; // Reduced from 6px for smaller circles
    pub const STROKE_WIDTH: f64 = 1.0; // Match container outline width
}

/// Colors for glyph grid view
pub mod grid {
    use super::Color;

    pub const CELL_BACKGROUND: Color = super::GRID_CELL_BACKGROUND;
    pub const CELL_OUTLINE: Color = super::GRID_CELL_OUTLINE;
    pub const CELL_SELECTED_BACKGROUND: Color =
        super::GRID_CELL_SELECTED_BACKGROUND;
    pub const CELL_SELECTED_OUTLINE: Color = super::GRID_CELL_SELECTED_OUTLINE;
    #[allow(dead_code)]
    pub const CELL_TEXT: Color = super::GRID_CELL_TEXT;
    pub const GLYPH_COLOR: Color = super::GRID_GLYPH_COLOR;

    /// Editor canvas grid lines
    #[allow(dead_code)]
    pub const LINE: Color = super::GRID_LINE;
}

/// Colors for paths and outlines
pub mod path {
    use super::Color;
    pub const STROKE: Color = super::PATH_STROKE;
    #[allow(dead_code)]
    pub const FILL: Color = super::PATH_FILL;
    pub const PREVIEW_FILL: Color = super::PATH_PREVIEW_FILL;
}

/// Colors for components (references to other glyphs)
pub mod component {
    use super::Color;
    /// Fill color for components - distinct from editable paths
    pub const FILL: Color = super::COMPONENT_FILL;
}

/// Colors for font metrics guides
pub mod metrics {
    use super::Color;
    pub const GUIDE: Color = super::METRICS_GUIDE;
}

/// Colors for control point lines (handles)
pub mod handle {
    use super::Color;
    pub const LINE: Color = super::HANDLE_LINE;
}

/// Colors for points
pub mod point {
    use super::Color;
    pub const SMOOTH_INNER: Color = super::SMOOTH_POINT_INNER;
    pub const SMOOTH_OUTER: Color = super::SMOOTH_POINT_OUTER;
    pub const CORNER_INNER: Color = super::CORNER_POINT_INNER;
    pub const CORNER_OUTER: Color = super::CORNER_POINT_OUTER;
    pub const OFFCURVE_INNER: Color = super::OFFCURVE_POINT_INNER;
    pub const OFFCURVE_OUTER: Color = super::OFFCURVE_POINT_OUTER;
    pub const HYPER_INNER: Color = super::HYPER_POINT_INNER;
    pub const HYPER_OUTER: Color = super::HYPER_POINT_OUTER;
    pub const SELECTED_INNER: Color = super::SELECTED_POINT_INNER;
    pub const SELECTED_OUTER: Color = super::SELECTED_POINT_OUTER;
}

/// Colors for selection rectangle (marquee)
pub mod selection {
    use super::Color;
    pub const RECT_FILL: Color = super::SELECTION_RECT_FILL;
    pub const RECT_STROKE: Color = super::SELECTION_RECT_STROKE;
}

/// Tool preview styles (for consistent visual feedback across tools)
pub mod tool_preview {
    use super::Color;

    // Colors for tool preview lines and markers
    /// Primary preview color (used for knife line, pen preview, etc.)
    pub const LINE_COLOR: Color = super::SELECTED_POINT_OUTER;
    /// Intersection/marker color (slightly darker for contrast)
    pub const MARKER_COLOR: Color = super::SELECTED_POINT_OUTER;
    /// Preview dot fill color
    pub const DOT_FILL: Color = super::SELECTED_POINT_OUTER;

    // Stroke widths
    /// Width of preview lines (pen preview, knife line)
    pub const LINE_WIDTH: f64 = 2.0;
    /// Width of intersection markers
    pub const MARKER_WIDTH: f64 = 2.0;

    // Dash patterns
    /// Dash pattern for preview lines [dash_length, gap_length]
    pub const LINE_DASH: [f64; 2] = [4.0, 4.0];
    /// Dash offset
    pub const LINE_DASH_OFFSET: f64 = 0.0;

    // Sizes
    /// Radius of preview dots
    pub const DOT_RADIUS: f64 = 4.0;
    /// Size of intersection markers (half-length)
    pub const MARKER_SIZE: f64 = 4.0;
    /// Radius of close zone indicator
    pub const CLOSE_ZONE_RADIUS: f64 = 8.0;
    /// Radius of snap indicator
    pub const SNAP_INDICATOR_RADIUS: f64 = 6.0;
}

/// Sizes for rendering
pub mod size {
    /// Radius for smooth on-curve points
    pub const SMOOTH_POINT_RADIUS: f64 = 4.5;
    /// Radius for smooth on-curve points when selected
    pub const SMOOTH_POINT_SELECTED_RADIUS: f64 = 5.5;

    /// Half-size for corner on-curve points (square)
    pub const CORNER_POINT_HALF_SIZE: f64 = 3.5;
    /// Half-size for corner on-curve points when selected
    pub const CORNER_POINT_SELECTED_HALF_SIZE: f64 = 4.5;

    /// Radius for off-curve control points
    pub const OFFCURVE_POINT_RADIUS: f64 = 3.0;
    /// Radius for off-curve control points when selected
    pub const OFFCURVE_POINT_SELECTED_RADIUS: f64 = 4.0;

    /// Radius for hyperbezier on-curve points
    pub const HYPER_POINT_RADIUS: f64 = 4.0;
    /// Radius for hyperbezier on-curve points when selected
    pub const HYPER_POINT_SELECTED_RADIUS: f64 = 5.0;

    /// Width of path strokes
    pub const PATH_STROKE_WIDTH: f64 = 1.0;

    /// Width of control point lines
    pub const HANDLE_LINE_WIDTH: f64 = 1.0;

    /// Width of metric guide lines
    pub const METRIC_LINE_WIDTH: f64 = 1.0;

    // ===== UI Layout =====
    /// Margin from screen edges for floating UI panels
    pub const UI_PANEL_MARGIN: f64 = 16.0;
    /// Gap between adjacent UI panels/toolbars
    pub const UI_PANEL_GAP: f64 = 16.0;

    // ===== Toolbar dimensions =====
    /// Size of toolbar buttons (width and height)
    pub const TOOLBAR_ITEM_SIZE: f64 = 48.0;
    /// Space between toolbar buttons
    pub const TOOLBAR_ITEM_SPACING: f64 = 6.0;
    /// Padding around the entire toolbar
    pub const TOOLBAR_PADDING: f64 = 8.0;
    /// Padding inside buttons for icons
    pub const TOOLBAR_ICON_PADDING: f64 = 12.0;
    /// Rounded corner radius for toolbar buttons
    pub const TOOLBAR_BUTTON_RADIUS: f64 = 6.0;
    /// Border thickness for toolbar buttons and panels
    pub const TOOLBAR_BORDER_WIDTH: f64 = 1.5;
    /// Rounded corner radius for all panels (toolbars, sidebars, etc.)
    pub const PANEL_RADIUS: f64 = 11.0;
}

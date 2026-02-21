// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Application settings and configuration constants.
//!
//! This module holds non-visual settings that stay stable across theme
//! changes. Visual styling (colors, sizes) belongs in `theme.rs`.

// ============================================================================
// EDITOR SETTINGS
// ============================================================================
/// Minimum zoom level (2% of original size)
const MIN_ZOOM: f64 = 0.02;

/// Maximum zoom level (50x original size)
const MAX_ZOOM: f64 = 50.0;

// ============================================================================
// DESIGN GRID SETTINGS
// ============================================================================
// Two zoom levels with different grid densities:
//   Mid zoom:   fine=8, coarse=32
//   Close zoom: fine=2, coarse=8

/// Minimum zoom for mid-level grid (coarser)
const DESIGN_GRID_MID_MIN_ZOOM: f64 = 0.8;
/// Fine grid spacing at mid zoom (design units)
const DESIGN_GRID_MID_FINE: f64 = 8.0;
/// Coarse grid spacing at mid zoom (subdivisions of fine)
const DESIGN_GRID_MID_COARSE_N: u32 = 4; // 4 × 8 = 32

/// Minimum zoom for close-level grid (finer)
const DESIGN_GRID_CLOSE_MIN_ZOOM: f64 = 4.0;
/// Fine grid spacing at close zoom (design units)
const DESIGN_GRID_CLOSE_FINE: f64 = 2.0;
/// Coarse grid spacing at close zoom (subdivisions of fine)
const DESIGN_GRID_CLOSE_COARSE_N: u32 = 4; // 4 × 2 = 8

// ============================================================================
// SNAP TO GRID SETTINGS
// ============================================================================
/// Whether snap-to-grid is enabled for all point movement
const SNAP_TO_GRID_ENABLED: bool = true;

/// Grid spacing for snapping (design units)
const SNAP_TO_GRID_SPACING: f64 = 2.0;

// ============================================================================
// NUDGE SETTINGS
// ============================================================================
/// Base nudge amount in design units (arrow key)
const NUDGE_BASE: f64 = 2.0;

/// Shift-arrow nudge amount in design units
const NUDGE_SHIFT: f64 = 8.0;

/// Ctrl/Cmd-arrow nudge amount in design units
const NUDGE_CMD: f64 = 32.0;

// ============================================================================
// PERFORMANCE SETTINGS
// ============================================================================
/// Throttle drag updates to every Nth frame to reduce Xilem rebuild churn.
///
/// During drags we emit `SessionUpdate` on each mouse move. That forces a
/// full Xilem view rebuild and tanks performance. Throttling keeps visual
/// feedback smooth while skipping redundant rebuilds. The canvas still
/// repaints every frame—only the heavy rebuild path is throttled.
///
/// Higher values = better performance, lower responsiveness
/// Lower values = better responsiveness, worse performance
const DRAG_UPDATE_THROTTLE: u32 = 3;

// ============================================================================
// PUBLIC API - Don't edit below this line unless you know what you're doing
// ============================================================================

/// Editor settings (zoom, viewport, etc.)
pub mod editor {
    /// Minimum zoom level (2% of original size)
    pub const MIN_ZOOM: f64 = super::MIN_ZOOM;

    /// Maximum zoom level (50x original size)
    pub const MAX_ZOOM: f64 = super::MAX_ZOOM;
}

/// Design grid overlay settings (unit grid shown when zoomed in)
///
/// Two detail levels that activate at different zoom thresholds:
/// - Mid zoom: coarser grid (fine=8, coarse=32)
/// - Close zoom: finer grid (fine=2, coarse=8)
pub mod design_grid {
    /// Mid-zoom level (coarser grid)
    pub mod mid {
        pub const MIN_ZOOM: f64 = super::super::DESIGN_GRID_MID_MIN_ZOOM;
        pub const FINE: f64 = super::super::DESIGN_GRID_MID_FINE;
        pub const COARSE_N: u32 = super::super::DESIGN_GRID_MID_COARSE_N;
    }

    /// Close-zoom level (finer grid)
    pub mod close {
        pub const MIN_ZOOM: f64 = super::super::DESIGN_GRID_CLOSE_MIN_ZOOM;
        pub const FINE: f64 = super::super::DESIGN_GRID_CLOSE_FINE;
        pub const COARSE_N: u32 = super::super::DESIGN_GRID_CLOSE_COARSE_N;
    }
}

/// Snap-to-grid settings for all point movement (drag, nudge)
pub mod snap {
    /// Whether snap-to-grid is enabled
    pub const ENABLED: bool = super::SNAP_TO_GRID_ENABLED;

    /// Grid spacing to snap to (design units)
    pub const SPACING: f64 = super::SNAP_TO_GRID_SPACING;
}

/// Nudge amounts for arrow key point movement
pub mod nudge {
    /// Base nudge (arrow key alone)
    pub const BASE: f64 = super::NUDGE_BASE;

    /// Shift-arrow nudge
    pub const SHIFT: f64 = super::NUDGE_SHIFT;

    /// Cmd-arrow nudge
    pub const CMD: f64 = super::NUDGE_CMD;
}

/// Performance optimization settings
pub mod performance {
    /// Throttle drag updates to every Nth frame.
    ///
    /// - 1 disables throttling (update every frame).
    /// - 3 updates every third frame (~67% fewer rebuilds).
    pub const DRAG_UPDATE_THROTTLE: u32 = super::DRAG_UPDATE_THROTTLE;
}

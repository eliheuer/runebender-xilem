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

/// Zoom scale factor for scroll wheel sensitivity
#[allow(dead_code)]
const ZOOM_SCALE: f64 = 0.001;

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

    /// Zoom scale factor for scroll wheel sensitivity
    #[allow(dead_code)]
    pub const ZOOM_SCALE: f64 = super::ZOOM_SCALE;
}

/// Performance optimization settings
pub mod performance {
    /// Throttle drag updates to every Nth frame.
    ///
    /// - 1 disables throttling (update every frame).
    /// - 3 updates every third frame (~67% fewer rebuilds).
    pub const DRAG_UPDATE_THROTTLE: u32 = super::DRAG_UPDATE_THROTTLE;
}

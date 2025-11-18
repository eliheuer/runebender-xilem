// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Generic toolbars - shared functionality for all toolbars
//!
//! This module provides common toolbar functionality that can be reused
//! by different toolbar types (edit mode toolbar, workspace toolbar,
//! etc.). This is a generic module covering all toolbars of this style.

use kurbo::{Affine, BezPath, Rect, RoundedRect, Shape, Size};
use masonry::util::{fill_color, stroke};
use masonry::vello::Scene;

use crate::theme::panel::{
    BACKGROUND as COLOR_PANEL,
    BUTTON_OUTLINE as COLOR_BUTTON_BORDER,
    OUTLINE as COLOR_PANEL_BORDER,
};
use crate::theme::size::{
    TOOLBAR_BORDER_WIDTH, TOOLBAR_BUTTON_RADIUS, TOOLBAR_ICON_PADDING,
    TOOLBAR_ITEM_SIZE, TOOLBAR_ITEM_SPACING, TOOLBAR_PADDING,
};
use crate::theme::toolbar::{
    BUTTON_HOVERED, BUTTON_SELECTED, BUTTON_UNSELECTED, ICON, ICON_HOVERED,
    ICON_SELECTED,
};

/// State for a single button in a toolbar
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonState {
    /// Whether this button is currently hovered
    pub is_hovered: bool,
    /// Whether this button is currently selected
    pub is_selected: bool,
}

impl ButtonState {
    /// Create a new button state
    pub fn new(is_hovered: bool, is_selected: bool) -> Self {
        Self {
            is_hovered,
            is_selected,
        }
    }

    /// Create default button state (not hovered, not selected)
    pub fn default() -> Self {
        Self {
            is_hovered: false,
            is_selected: false,
        }
    }
}

impl Default for ButtonState {
    fn default() -> Self {
        Self::default()
    }
}

/// Calculate toolbar size based on number of buttons
pub fn calculate_toolbar_size(button_count: usize) -> Size {
    let width = TOOLBAR_PADDING * 2.0
        + button_count as f64 * TOOLBAR_ITEM_SIZE
        + (button_count.saturating_sub(1)) as f64 * TOOLBAR_ITEM_SPACING;
    let height = TOOLBAR_ITEM_SIZE + TOOLBAR_PADDING * 2.0;
    Size::new(width, height)
}

/// Get the rect for a button by index
pub fn button_rect(index: usize) -> Rect {
    let x = TOOLBAR_PADDING
        + index as f64 * (TOOLBAR_ITEM_SIZE + TOOLBAR_ITEM_SPACING);
    let y = TOOLBAR_PADDING;
    Rect::new(x, y, x + TOOLBAR_ITEM_SIZE, y + TOOLBAR_ITEM_SIZE)
}

/// Paint the background panel for a toolbar
pub fn paint_panel(scene: &mut Scene, size: Size) {
    let panel_rect = size.to_rect();
    let panel_rrect = RoundedRect::from_rect(panel_rect, 8.0);

    // Solid opaque background - darker than buttons but brighter
    // than canvas
    fill_color(scene, &panel_rrect, COLOR_PANEL);

    // Draw panel border - inset slightly to prevent corner
    // artifacts
    let border_inset = TOOLBAR_BORDER_WIDTH / 2.0;
    let inset_rect = panel_rect.inset(-border_inset);
    let inset_rrect = RoundedRect::from_rect(inset_rect, 8.0);
    stroke(
        scene,
        &inset_rrect,
        COLOR_PANEL_BORDER,
        TOOLBAR_BORDER_WIDTH,
    );
}

/// Paint a toolbar button with the given state
pub fn paint_button(
    scene: &mut Scene,
    button_rect: Rect,
    state: ButtonState,
) {
    let button_rrect =
        RoundedRect::from_rect(button_rect, TOOLBAR_BUTTON_RADIUS);

    // Determine button background color based on state
    let bg_color = if state.is_selected {
        BUTTON_SELECTED
    } else if state.is_hovered {
        BUTTON_HOVERED
    } else {
        BUTTON_UNSELECTED
    };
    fill_color(scene, &button_rrect, bg_color);

    // Draw button border
    stroke(
        scene,
        &button_rrect,
        COLOR_BUTTON_BORDER,
        TOOLBAR_BORDER_WIDTH,
    );
}

/// Paint an icon in a toolbar button with state-based coloring
pub fn paint_icon(
    scene: &mut Scene,
    icon: BezPath,
    button_rect: Rect,
    state: ButtonState,
) {
    let icon_bounds = icon.bounding_box();
    let icon_center = icon_bounds.center();
    let button_center = button_rect.center();

    // Scale icon to fit with padding
    let icon_size = icon_bounds.width().max(icon_bounds.height());
    let target_size = TOOLBAR_ITEM_SIZE - TOOLBAR_ICON_PADDING * 2.0;
    let scale = target_size / icon_size;

    // Create transform: scale then translate to center
    let transform = Affine::translate((button_center.x, button_center.y))
        * Affine::scale(scale)
        * Affine::translate((-icon_center.x, -icon_center.y));

    // Determine icon color based on state
    let icon_color = if state.is_selected {
        ICON_SELECTED
    } else if state.is_hovered {
        ICON_HOVERED
    } else {
        ICON
    };

    fill_color(scene, &(transform * icon), icon_color);
}



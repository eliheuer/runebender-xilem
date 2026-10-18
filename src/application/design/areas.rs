// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Where an area of the window departs from the base design system.
//!
//! The base answers for every area: theme colors, the control states in
//! [`super::states`], the measurement scales in this module's parent.
//! An area that needs something else says so here, once, with its
//! reason, as a value derived from those tokens rather than a new
//! number. A view uses the area's value; it never writes the override
//! at its call site.
//!
//! An agent adding a new area starts with no overrides. Add one only
//! when the base visibly fails the area, and write down why.

/// The bar under the center column.
pub(crate) mod footer {
    use crate::application::design::{ControlSize, FOOTER_HEIGHT, Space};

    /// Controls in the footer are one step shorter than panel controls.
    ///
    /// Why: the footer is a 28 px bar that also holds 16 px icons; a
    /// 28 px button would touch both rules. The icon tile height leaves
    /// the same clearance above and below as the icons have.
    pub(crate) const CONTROL_HEIGHT: f64 = ControlSize::Icon.px();

    /// Space between a footer control's text and its sides.
    ///
    /// Why: at the compact height the panel inset would read as a
    /// squeezed pill; the medium step keeps the text optically centered.
    pub(crate) const CONTROL_INSET: f64 = Space::Md.px();

    const _: () = assert!(
        CONTROL_HEIGHT < FOOTER_HEIGHT,
        "a footer control fits inside the footer"
    );
}

/// The graph canvas: nodes as cards, wires between them.
pub(crate) mod graph {
    use crate::application::design::theme::Palette;
    use crate::application::design::{
        GRID_CELL_SELECTED_SHADOW_OFFSET, GRID_CELL_SHADOW_OFFSET, TextSize,
    };

    /// A node's corner radius, at zoom 1.
    ///
    /// Why: a node is a card on a ground, like a glyph tile, so it takes
    /// the theme's card radius (`geometry.radius`) rather than the
    /// control radius. It scales with the node as the canvas zooms.
    pub(crate) fn node_radius(pal: &Palette) -> f64 {
        pal.corner_radius
    }

    /// A node's hard shadow, in screen pixels.
    ///
    /// Why: the same offsets as a glyph tile's, so cards read alike
    /// across the window. The selected node lifts one step, as a
    /// selected tile does.
    pub(crate) fn shadow_offset(selected: bool) -> f64 {
        if selected {
            GRID_CELL_SELECTED_SHADOW_OFFSET
        } else {
            GRID_CELL_SHADOW_OFFSET
        }
    }

    /// A wire's width in screen pixels at `zoom`.
    ///
    /// Why: a wire is what the graph says, not an outline, so it takes
    /// the theme's emphasized stroke. It scales with the canvas and never
    /// gets thinner than the normal stroke.
    pub(crate) fn wire_width(pal: &Palette, zoom: f64) -> f64 {
        (pal.stroke_emphasis * zoom).max(pal.stroke)
    }

    /// How far the alignment dots are mixed from the canvas toward the outline.
    ///
    /// Why: the dots sit behind ports and wires; at full outline ink they
    /// compete with both.
    pub(crate) const GRID_DOT_MIX: f32 = 0.4;

    /// Canvas text size at `zoom`, in pixels: the body size scaled with
    /// the canvas.
    ///
    /// Why: node text belongs to the drawing, so it zooms with it, but
    /// it stops shrinking where it can no longer be read and stops
    /// growing where it would crowd the node.
    pub(crate) fn text_px(zoom: f64) -> f32 {
        const MIN: f64 = 6.0;
        const MAX: f64 = 40.0;
        crate::application::view::render::px32(
            (f64::from(TextSize::Body.px()) * zoom).clamp(MIN, MAX),
        )
    }
}

/// The neural model strip: a diagram of strands and nodes over the ink.
pub(crate) mod model_strip {
    /// The selected strand's line, and the keyline around it.
    ///
    /// Why: a strand is drawn over glyph ink of any weight; a 1.5 px line
    /// in a 3 px keyline stays visible on both the ink and the ground.
    pub(crate) const STRAND: f64 = 1.5;
    pub(crate) const STRAND_KEYLINE: f64 = 3.0;

    /// The active node's half ring, and the keyline around it.
    ///
    /// Why: it is the handle the pointer drags, so it reads heavier than
    /// the strand it sits on.
    pub(crate) const RING: f64 = 3.5;
    pub(crate) const RING_KEYLINE: f64 = 5.5;
}

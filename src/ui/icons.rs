// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Application icon geometry shared by the native and browser editors.

use std::collections::HashMap;
use std::sync::OnceLock;

use kurbo::{Affine, BezPath, Rect};

use crate::formats::icon_ufo;

/// One application icon: outline geometry from the bundled icon UFO.
#[derive(Debug)]
pub struct Icon {
    /// The glyph's display frame in Y-down coordinates.
    pub view_box: Rect,
    /// The icon outline in the same Y-down space as `view_box`.
    pub path: BezPath,
    /// Stroke instead of fill, for open-path icons.
    pub stroke: bool,
}

impl Icon {
    /// Fit this icon into a control frame without changing its proportions.
    pub fn fitted_path(&self, frame: Rect) -> BezPath {
        let source = self.view_box;
        let scale = (frame.width() / source.width()).min(frame.height() / source.height());
        let x = frame.x0 + (frame.width() - source.width() * scale) / 2.0;
        let y = frame.y0 + (frame.height() - source.height() * scale) / 2.0;
        let transform = Affine::translate((x, y))
            * Affine::scale(scale)
            * Affine::translate((-source.x0, -source.y0));
        transform * self.path.clone()
    }
}

/// The shared icon set, keyed by glyph name ("select", "pen", "grid", …).
pub fn icons() -> &'static HashMap<String, Icon> {
    static ICONS: OnceLock<HashMap<String, Icon>> = OnceLock::new();
    ICONS.get_or_init(|| {
        icon_ufo::embedded_icons()
            .into_iter()
            .map(|glyph| {
                (
                    glyph.name,
                    Icon {
                        view_box: glyph.frame,
                        path: glyph.path,
                        stroke: glyph.stroke,
                    },
                )
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Point, Shape as _};
    use std::collections::HashSet;

    fn is_inked(icon: &Icon, x: f64, y: f64) -> bool {
        icon.path.winding(Point::new(x * 48.0, y * 48.0 - 768.0)) != 0
    }

    #[test]
    fn parses_all_icons() {
        let source = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons/icons.ufo");
        let font = norad::Font::load(source).expect("editable icon UFO loads");
        assert_eq!(font.default_layer().iter().count(), 51);
        let icons = icons();
        assert_eq!(icons.len(), 51);
        let mut assigned = HashSet::new();
        for glyph in font.default_layer().iter() {
            let codepoints = glyph.codepoints.iter().collect::<Vec<_>>();
            assert_eq!(
                codepoints.len(),
                1,
                "{} needs one PUA code point",
                glyph.name()
            );
            let codepoint = u32::from(codepoints[0]);
            assert!((0xE000..=0xF8FF).contains(&codepoint));
            assert!(assigned.insert(codepoint), "duplicate U+{codepoint:04X}");
        }
        for name in [
            "select",
            "select-menu",
            "select-all-layers",
            "lasso",
            "shapes-menu",
            "menu-down",
            "pen",
            "brush",
            "knife",
            "measure",
            "shapes",
            "shape-metaball",
            "flip-h",
            "rot-cw",
            "union",
            "save",
            "plus",
            "minus",
            "grid",
            "list",
            "eye-open",
            "eye-closed",
            "invert",
            "sidebar-open",
            "sidebar-closed",
            "disclosure-bullet",
            "disclosure-closed",
            "disclosure-open",
            "menu-check",
            "menu-chevron",
            "coordinate-grid",
            "coordinate-dot",
            "coordinate-dot-selected",
            "node-resize",
        ] {
            let icon = icons.get(name).unwrap_or_else(|| panic!("missing {name}"));
            assert!(!icon.path.elements().is_empty());
            assert!(icon.view_box.width() > 0.0 && icon.view_box.height() > 0.0);
        }
    }

    #[test]
    fn control_icon_counters_and_states_render() {
        let icons = icons();
        let grid = &icons["grid"];
        assert!(is_inked(grid, 3.8, 3.8));
        assert!(!is_inked(grid, 5.5, 5.5));

        let eye_open = &icons["eye-open"];
        assert!(is_inked(eye_open, 8.0, 8.0));
        assert!(!is_inked(eye_open, 4.0, 8.0));
        assert!(is_inked(&icons["eye-closed"], 8.0, 8.0));

        let invert = &icons["invert"];
        assert!(is_inked(invert, 5.0, 8.0));
        assert!(!is_inked(invert, 11.0, 8.0));

        assert!(is_inked(&icons["sidebar-open"], 6.0, 8.0));
        assert!(is_inked(&icons["sidebar-closed"], 4.25, 8.0));
        for name in ["sidebar-open", "sidebar-closed"] {
            let panel = &icons[name];
            assert!(is_inked(panel, 1.5, 8.0), "{name} includes its frame");
            assert!(is_inked(panel, 8.0, 2.5), "{name} includes its top edge");
            assert!(!is_inked(panel, 11.0, 8.0), "{name} has an open interior");
            assert!(!is_inked(panel, 1.0, 2.0), "{name} has rounded corners");
        }

        let dot = &icons["coordinate-dot"];
        assert!(is_inked(dot, 8.0, 1.2));
        assert!(!is_inked(dot, 8.0, 8.0));
        assert!(is_inked(&icons["coordinate-dot-selected"], 8.0, 8.0));

        let lasso = &icons["lasso"];
        assert!(is_inked(lasso, 2.5, 5.8));
        assert!(!is_inked(lasso, 8.0, 5.8));
    }
}

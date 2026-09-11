# Changelog

All notable changes to runebender-xilem. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project will use [Semantic Versioning](https://semver.org/) once
releases begin.

## [Unreleased]

- Align search and inspector inputs with shared insets and optical baseline positioning; route inspector text and labels through the bundled UI-font helpers.

- Preserve the native macOS editor menus at startup by disabling winit's replacement default menu.

- Give glyph search equal padding and matching Virtua Grotesk typography for placeholder and typed text, preventing clipped descenders.

- Remove the doubled right edge on the navigation tabs and keep scrollbar overlays outside the viewport clip during scrolling, preserving Masonry scrolling behavior.

- Match the mark strip to GPUI with equal slots, smaller circles, selection rings, and a centered drawn clear cross; restore vertical panel dividers.
- Use the native application properties for headless visual captures.

- Apply the bundled Virtua Grotesk at 13px consistently to UI labels and editable fields.

- Complete the desktop menu system: shared command metadata, native macOS and
  accessible in-window menus, working file/glyph/path/filter/view commands,
  stateful submenus, keyboard navigation, and focused-text shortcut precedence.

- Consolidate Core and its headless CLI into the Xilem Cargo workspace, preserving Core history and the existing theme.

- Make the main checkout the primary Xilem development location; retain GPUI as a reference and fallback.

- Keep wheel and trackpad scrolling without visible scrollbar overlays; verified during pointer movement, two-axis scrolling, and resizing.

- Correct the header separator and outline the navigation strip; restore the Chat tab with an explicit unavailable state.

- Match navigation tab heights, top-only selected corners, and icon sizing to GPUI.

- Remove the extra titlebar icon and tighten category sidebar rows to the GPUI reference.

- Count the primary overview selection and contain glyph captions within their tiles.

- Retain the live text buffer across tool switches without consuming outline-tool input.

- Match GPUI continuity rings while preserving the underlying point markers.

- Draw curvature combs with GPUI-normalized geometry and theme-colored, outlined teeth.

- Add Fit graph and refit newly opened node files; keep glyph-grid controls in Font view.

- Fit the initial node viewport and keep connection wires and ports visible above cards.

- Use shared neutral theme colors for sliders and header tab selection.

- Align category sidebar insets and full-width rules; group font counts under Filters.

- Share working Glyphs, Axes, and Local AI navigation across font, node, and glyph workspaces.

- Match GPUI thumbnail ink centering, em-relative scale, and compact caption spacing; keep tall marks inside their cells.

- Fit glyph rows to the viewport, give rail thumbnails GPUI proportions, and propagate grid size/theme changes on rebuild.

- Give word proofs the GPUI initial drawing height and start additional inspector sections folded.

- Add a contour-selection Shapes rail, inverted word proofs, connected coordinate reference controls, and GPUI-style glyph and node shadows.

- Add editable shaped word previews with cached Vello CPU blur, larger outline previews, shared rail search controls, and clearer selected-state and metrics styling.

- Add selection width/height editing around the chosen reference point and record inspector undo immediately.

- Align inspector section borders, spacing, ordering, and path-operation groups with GPUI; fit overview tiles across the available width.

- Keep GPUI live-node files out of the disk runner; experiments remain available through MCP.


No releases yet. `AGENTS.md` has the checklist for the first one.
Until then, `main` is the only line and this section stays open.

### Changed

- Align the Xilem header, editor rail, and node canvas with GPUI's shared visual hierarchy.

- Shared live experiment, kerning and drawing tools through MCP; refresh open sessions after application and preserve undo. Uses the core Designbot proof interface.

- Core pin updated for revision-checked agent proposals. Installing a guarded
  proposal skips glyphs whose foreground changed after the proposal was made.

- Themes: Midnight removed, Gray is the default. Dark, Gray, Light.
- Undo lives in core. `Session` holds an `EditHistory` from
  `runebender_core::document::history` instead of its own `UndoState`,
  and records, undoes, and discards through it.

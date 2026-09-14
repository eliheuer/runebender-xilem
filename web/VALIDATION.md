# Browser preview validation — 2026-09-14

The preview renders the existing Xilem/Masonry widgets, using the same editing
commands and Core font model as the native application.

Verified in isolated headless Chrome on macOS at 1280×800:

- Starts with all 863 bundled Virtua Grotesk glyphs.
- Double-click opens an outline; dragging a point changes its font coordinates.
- Undo restores the original coordinates and redo restores the edit.
- Wheel zoom, panel resizing, and switching to Nodes work.
- Open gives desktop-only feedback. Save preserves the session's edited points.
- Search opens the matching glyph; Light and Dark retain visible point markers.
- No JavaScript page errors in the interaction check (`web/smoke.cjs`).

Native validation: workspace tests passed (511 tests, 4 ignored), formatting,
Clippy with warnings denied, and documentation build passed. The native Cargo
manifest and lockfile are unchanged by this browser work.

`cargo vet --locked` was run on the native workspace. It reports nine existing
unvetted dependencies: block2 0.6.2, rfd 0.17.2, and the pinned Xilem revision's
masonry, masonry_core, masonry_testing, masonry_winit, tree_arena, xilem, and
xilem_core. No audit or exemption was invented to mark these as vetted. The
separate browser workspace has not undergone a full dependency audit.

This milestone does not certify every browser or input method. File import and
export, persistent edits, OS clipboard integration, accessibility tree forwarding,
IME composition, and local process execution remain outside this preview.

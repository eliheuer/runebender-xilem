# In-window header and menu readability

The application menu code used four missing palette roles. The palette returned
white for those unknown names, causing white frames and separators, washed-out
disabled rows, and incorrect selected text. Menus now use the resolved outline,
text, muted text, and selected-control tokens. Disabled items retain readable
text without an active background. The menu text is vertically centered.

The browser, Linux, Windows, and macOS's optional in-window menu mode now share
one 30px header. Menus stay on the left; the document name and save state sit
immediately before the tools/tabs on the right. Long names elide to fit the
available space. The identity retains accessible alt text. macOS normally keeps
its OS menu and existing document titlebar.

Dark's header previously halved a light selection color. Its existing named
titlebar surface now gives the text sufficient contrast. Gray and Light retain
their header surfaces.

## Evidence

Direct 2× browser captures; view at 100% for pixel-level inspection:

- [Gray Filter menu](filter-gray-2x.png)
- [Light Filter menu](filter-light-2x.png)
- [Dark Filter menu](filter-dark-2x.png)
- [Gray File menu with shortcuts](file-gray-2x.png)
- [1000px editor header with elided filename](editor-1000-2x.png)
- [1920px editor header](editor-1920-2x.png)

## Checks

All 12 native menu tests pass: keyboard navigation, popup dismissal, dispatch,
focus restoration, accessibility activation, and contrast. Enabled, selected,
and header text have at least 4.5:1 contrast; disabled text has at least 3:1 in
Gray, Light, and Dark.

The real-browser quality matrix passes at 1×, 2×, and 1.25×. It checks edits,
undo/redo, zoom, actual divider movement, nodes, input, responsive sizing, and
actual painted theme changes selected through the menu. WASM Clippy passes with
warnings denied and the established UI type-complexity/argument-count allowances.
Use `web/menu-appearance.cjs` for these menu captures and `web/quality.cjs` for
the interaction matrix. Native Linux/Windows rendering has not been launched;
the common in-window path was exercised in Chrome and the native widget harness.

The release source uses a frozen committed desktop baseline plus these changes.
Concurrent desktop work and homepage screenshot assets are excluded.

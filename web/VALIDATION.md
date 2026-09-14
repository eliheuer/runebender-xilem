# Browser preview validation — 2026-09-14

The preview renders the existing Xilem/Masonry widgets, using the same editing
commands and Core font model as the native application.

Initial interactive preview verification (before the resolution/performance pass),
in isolated headless Chrome on macOS at 1280×800:

- Starts with all 863 bundled Virtua Grotesk glyphs.
- Double-click opens an outline; dragging a point changes its font coordinates.
- Undo restores the original coordinates and redo restores the edit.
- Wheel zoom, panel resizing, and switching to Nodes work.
- Open gives desktop-only feedback. Save preserves the session's edited points.
- Search opens the matching glyph; Light and Dark retain visible point markers.
- No JavaScript page errors in the interaction check (`web/smoke.cjs`).

Native validation from that earlier preview phase: workspace tests passed (511 tests, 4 ignored), formatting,
Clippy with warnings denied, and documentation build passed. The native Cargo
manifest and lockfile are unchanged by this browser work.

`cargo vet --locked` was run on the native workspace. It reports nine existing
unvetted dependencies: block2 0.6.2, rfd 0.17.2, and the pinned Xilem revision's
masonry, masonry_core, masonry_testing, masonry_winit, tree_arena, xilem, and
xilem_core. No audit or exemption was invented to mark these as vetted. The
separate browser workspace has not undergone a full dependency audit.

## Resolution and interaction pass

The follow-up browser build renders physical display pixels, enables Vello CPU's
WASM SIMD pipeline, and responds to framework repaint/animation requests. It uses
native cursors and input hit testing. A four-node, two-link Core graph starts ready
for editing in the Nodes view; its proof/export execution remains desktop-only.

`quality.cjs` checks the real browser at 1×, 2×, and 1.25×, including changing to
1.5× without reloading. It verifies actual point edits, undo/redo, painted zoom,
painted panel-divider movement, Gray/Light/Dark, focus loss, DOM paste and
composition, graph movement with retained links, and window resizing. It asserts
that the canvas backing size equals its CSS size times devicePixelRatio and that
there is no continuous idle repaint loop. Frame timings and screenshots are in
`docs/browser-quality/2026-09-14/`.

The browser-only Rust host passes rustfmt and WASM Clippy with warnings denied
(`--no-deps`, allowing the existing UI's too_many_arguments and type_complexity
patterns). No native Rust sources or dependency manifests/lockfiles changed in
this pass. The build uses a committed desktop source snapshot, excluding ongoing
native UI work in the shared checkout.

This does not certify Safari, Firefox, every OS IME, or full native parity.
Composition/paste tests dispatch DOM events; OS clipboard permissions and real
IME candidate windows still need manual checks. Accessibility tree forwarding,
user-file import/export, persistent edits, and local processes remain outside
this preview. The current native divider has a narrow draggable edge and can retain the default
cursor before dragging; the test
measures a real drag on that edge rather than merely checking a resize cursor.

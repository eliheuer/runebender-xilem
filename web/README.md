# Runebender in the browser

This is the desktop application's Xilem/Masonry widget tree, compiled to WASM.
`src/browser.rs` embeds that tree in a browser host, sends real input through
Masonry, and paints its retained scene into a canvas using Vello CPU. There is
no screenshot player and no separate JavaScript implementation of the editor.

The bundled Virtua Grotesk source opens in memory. Edits last for this tab;
refreshing or Reset Demo restores the bundled font. Desktop filesystem access,
local AI processes, and saving are outside this first browser version.

## Build

Install the Rust `wasm32-unknown-unknown` target and wasm-bindgen-cli 0.2.127,
then run `./web/build.sh` from the repository root. `RUNEBENDER_WASM_BINDGEN`
can select a matching executable without replacing the system installation.
Serve `web/` over HTTP and open `index.html`. Deploy only `index.html`,
`app.js`, `pkg/`, and `licenses/`; build caches and source font JSON are not
separate runtime requests.

## Framework compatibility

Native builds continue to use the pinned upstream Xilem unchanged.
`prepare.py` copies the same revision's `xilem_masonry` adapter into the ignored
browser build directory. Its Rust source is unchanged. The generated manifest
removes its unconditional Tokio `rt-multi-thread` feature, which cannot compile
for WASM. This host uses a current-thread context and does not start desktop
async task pumps. `compat/` supplies the re-exports used by the desktop source.

The browser is a separate Cargo workspace and lockfile so this manifest
adaptation and browser dependencies cannot change native dependency selection.
The font is under the SIL Open Font License in `licenses/`.

## Browser interaction check

With the demo served and Playwright installed, run `node web/smoke.cjs`. It drives
Chrome through glyph opening, outline dragging, undo/redo, zoom, a splitter drag,
and the Nodes view, and checks that Open gives desktop-only feedback. Set
`RUNEBENDER_DEMO_URL` for a different host, `RUNEBENDER_CHROME` for a Chrome
executable, or `RUNEBENDER_PLAYWRIGHT` for an existing Playwright installation.
Set `RUNEBENDER_IFRAME=1` when checking the website's full-screen editor wrapper.

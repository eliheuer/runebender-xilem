# Runebender in the browser

This is the desktop application's Xilem/Masonry widget tree, compiled to WASM.
`src/app/browser.rs` embeds that tree in a browser host, sends real input through
Masonry, and paints its retained scene into a canvas using Vello CPU. There is
no screenshot player and no separate JavaScript implementation of the editor.

The bundled Virtua Grotesk source opens in memory. Edits last for this tab;
refreshing restores the bundled font. Desktop filesystem access,
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

## Browser quality checks

Build with `./web/build.sh`, then serve `web/` on port 4326. The build enables
WebAssembly SIMD so Vello CPU uses its vectorized renderer. No WebGPU or
cross-origin-isolation headers are required. Browser dependencies remain separate
from the desktop lockfile.

Lint browser-specific code with warnings denied while excluding the generated upstream adapter:

```sh
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS='-C target-feature=+simd128' \
cargo clippy --manifest-path web/Cargo.toml --locked \
  --target wasm32-unknown-unknown --no-deps -- -D warnings
```

With Playwright installed, run `node web/quality.cjs`. The default matrix covers
1×, 2×, and 1.25× displays; 1000–1440px windows; density changes without reload;
actual outline dragging and undo/redo; painted zoom; splitter cursors; themes;
keyboard focus; paste; composition events; and the Nodes canvas. It also checks
that the app does not continually repaint while idle.

Environment variables:

- `RUNEBENDER_DEMO_URL`: another local or published URL.
- `RUNEBENDER_IFRAME=1`: test the website's full-screen wrapper.
- `RUNEBENDER_CHROME`: an installed Chrome executable.
- `RUNEBENDER_PLAYWRIGHT`: an existing Playwright module installation.
- `RUNEBENDER_DPRS`: comma-separated display densities, default `1,2,1.25`.
- `RUNEBENDER_PROOFS`: save screenshots and frame timing measurements here.

`smoke.cjs` and `themes.cjs` remain aliases for this shared check, at 1×.
Browser frame timings are local measurements, not guarantees for other machines.
The composition checks dispatch DOM events; they do not certify every OS IME.

The canvas tracks physical resolution independently of CSS layout. Masonry receives
that same scale for layout, painting, and pointer input. Browser resize, resolution,
and focus events keep it in sync. Mouse motion is coalesced per animation frame;
only requested repaints run the renderer. A hidden native text input bridges paste
and composition to the focused Masonry widget. The bundled Nodes example is editable
in memory; executing its proof/export workflow requires the desktop application.

For menu appearance checks, run `node web/menu-appearance.cjs` with the same
Playwright/Chrome variables and `RUNEBENDER_PROOFS` set to a capture directory.
The browser uses the shared in-window header; macOS normally keeps its OS menus.

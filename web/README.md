# Runebender in the browser

This is the desktop application's Xilem/Masonry widget tree, compiled to WASM.
`src/application/browser.rs` embeds that tree in a browser host, sends real input through
Masonry, and paints its retained scene into a canvas using Vello CPU. There is
no screenshot player and no separate JavaScript implementation of the editor.

The bundled Virtua Grotesk source opens in memory. In Chrome and Edge, File → Open
picks a folder with the File System Access API: a UFO folder, or a folder that holds one
designspace with its UFOs. Save writes the files back into that folder, and Save As
writes into a new folder. Dropping a folder on the canvas opens it too. Other browsers
keep the bundled font only, and edits there last for the tab.
Local AI processes stay outside the browser version.
Export compiles the current in-memory font in Rust and downloads a TTF.
Compilation is currently synchronous in the browser; a worker host remains future work.

## Fonts on disk

`src/font/persistence/memory.rs` decodes and encodes a complete UFO package from
`(path, bytes)` pairs: every layer, images, data, and files the specification does not
name, which are written back unchanged. The page reads the folder, packs the files into
one byte bundle, and `BrowserEditor::open_bundle` opens it as a project. Save asks
`BrowserEditor::save_bundle` for the files to write; the manifest also names the glyph
directories whose unlisted `.glif` files are stale, and the page removes those. Original
GLIF file names and layer directories are kept, so a saved UFO matches the opened one
file for file. The page remembers the folder in IndexedDB and reopens it when Chrome
kept the permission, as it does for an installed app.

Run `node web/folder.cjs` with the Playwright variables below to check this path: it
opens the Virtua Grotesk sources through the test hooks, edits a point, and compares the
saved file set with the opened one.

## Build

Install the Rust `wasm32-unknown-unknown` target and wasm-bindgen-cli 0.2.127,
then run `./web/build.sh` from the repository root. `RUNEBENDER_WASM_BINDGEN`
can select a matching executable without replacing the system installation.
Serve `web/` over HTTP and open `index.html`. Deploy only `index.html`,
`app.js`, `pkg/`, `licenses/`, `manifest.webmanifest`, and `icons/`;
build caches and source font JSON are not separate runtime requests.

## Installable web app

`manifest.webmanifest` and `icons/` make the deployed page installable.
Chrome, Edge, and ChromeOS show an install button in the address bar when the
page is served over HTTPS. The installed app opens in its own window with the
Runebender icon. There is no service worker, so the app needs a network
connection to start. The icons come from `assets/icons/runebender.png`:

```sh
magick assets/icons/runebender.png -resize 192x192 web/icons/icon-192.png
magick assets/icons/runebender.png -resize 512x512 web/icons/icon-512.png
magick -size 512x512 xc:'#15131a' \( assets/icons/runebender.png -resize 400x400 \) \
  -gravity center -composite web/icons/icon-512-maskable.png
```

On runebender.org the install entry point is `/wasm-editor/`, which has its
own manifest in the website repository and wraps this page in an iframe.

## Framework compatibility

Native builds continue to use the pinned Xilem revision unchanged.
`prepare.py` reads that pin from the root lockfile, copies the same revision's
`xilem_masonry` adapter into the ignored browser build directory, and pins
`web/Cargo.toml` and `web/compat/Cargo.toml` to it. Its Rust source is unchanged.
The generated manifest removes its unconditional Tokio `rt-multi-thread` feature,
which cannot compile for WASM. After the desktop pin moves, run `./web/build.sh`
without `--locked` once to refresh `web/Cargo.lock`. This host uses a current-thread context and does not start desktop
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
keyboard focus; paste; composition events; and the Graph canvas. It also checks
that the application does not continually repaint while idle.

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
and composition to the focused Masonry widget. The bundled Graph example is editable
in memory; executing its proof/export workflow requires the desktop application.

For menu appearance checks, run `node web/menu-appearance.cjs` with the same
Playwright/Chrome variables and `RUNEBENDER_PROOFS` set to a capture directory.
The browser uses the shared in-window header; macOS normally keeps its OS menus.

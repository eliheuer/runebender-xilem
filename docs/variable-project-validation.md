# Variable Project validation

Date: 2026-09-18.
Baseline: `5c37be7717e780aac3fcb369b033148f57026ef9`.
Implementation: `9370ce4624b87285a56a4ecc64bf71708f4cbcb4`.
The [decision record](variable-project-decision.md) describes the selected backend, upstream blockers and supported formats.

## Native gates

| Check | Result |
|---|---|
| `cargo fmt --all --check` | Pass |
| `bash .github/copyright.sh` | Pass, including newly tracked modules |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Pass |
| `cargo test --workspace --locked -- --test-threads=1` | 559 passed; 4 existing tests requiring adjacent source checkouts or local models ignored |
| `cargo test --test variable_project --locked` | 9 passed after extending the round-trip fixture with PNG data and a point identifier |
| `cargo doc --workspace --no-deps --locked` | Pass |
| `cargo build --workspace --release --locked` | Pass |
| `cargo deny --locked check advisories` | Pass with refreshed RustSec data |
| `git diff --check` | Pass |

The full native suite used `RUNEBENDER_TEST_FONTS` pointing to the existing Virtua Grotesk source directory.
Its Unix-socket integration test ran outside the filesystem sandbox and passed.
The ignored tests are not counted as runtime model coverage.
Cargo reports the existing future-compatibility notice for `block` 0.1.6; workspace warning-denied checks pass.

## Headless visual checks

Gray and Light captures opened Virtua Grotesk's Designspace on `R` at `wght=500`, with the Axes section expanded.
Both were inspected at 1100 by 720 pixels.
The canvas and proof strip render the interpolated outline, the axis value reads 500, and the UI identifies the state as interpolated.
The native application was not brought to the foreground.
These captures do not establish native pointer, IME, accessibility or GPU behavior.

Reproduce with the repository's headless renderer:

```sh
RUNEBENDER_SCREENSHOT=/tmp/variable-project-gray.png \
RUNEBENDER_SIZE=1100x720 RUNEBENDER_THEME=gray RUNEBENDER_GLYPH=R \
RUNEBENDER_EXPAND=Axes RUNEBENDER_AXIS=wght=500 \
target/debug/runebender "$RUNEBENDER_TEST_FONTS/VirtuaGrotesk.designspace"
```

Use `RUNEBENDER_THEME=light` and a different output path for the Light capture.

## Browser gates

`./web/build.sh` passes with wasm-bindgen-cli 0.2.127 and the locked browser workspace.
The browser Clippy command from `web/README.md` also passes with warnings denied.
`web/quality.cjs` passes against the fresh local bundle in headless Chrome at all three default display densities.
It exercises real point dragging and history, zoom, splitters, themes, focus loss, paste/composition events, Nodes movement, viewport resizing, display-density changes and idle rendering.
Gray and Light browser captures were also inspected.

| Density | Frames rendered during test | Median render time | P95 render time |
|---|---|---|---|
| 1× | 139 | 4.0 ms | 5.8 ms |
| 2× | 142 | 8.6 ms | 11.0 ms |
| 1.25× | 137 | 5.1 ms | 6.9 ms |

These are local measurements during the interaction test, not a performance guarantee or a benchmark of every source size.
The tested WASM SHA-256 is `601aeacf9e302a022de144a96780eef56f172473a8799a416d24cef9c1a4f6eb`.
The browser remains an in-memory demo and does not acquire filesystem save support from this migration.

## Review boundaries

Review the owned glyph/layer store and guard reconciliation in `document/variable.rs`, history and persistence in `document/project.rs`, and private numeric adapters in `document/axis.rs` and `document/var_model.rs`.
Review `formats/designspace.rs` and `formats/babelfont_import.rs` together with their explicit unsupported-format errors.
Existing source tools retain full Norad projections, so memory cost and edit reconciliation are visible migration debt.
No claim is made that Rust Babelfont JSON, every Python package field, or all Designspace extensions are editable.
No sibling repository, remote branch, deployment, or merge is part of this change.

## Counterpunch pipeline continuation, 2026-09-18

Implementation: `5cbf51beabd29385801a244a16df2ca90df011a3`.
The earlier sections record the initial glyph-local ownership migration.
This continuation adds canonical Babelfont geometry, preserving UFO adapters, complete in-memory fontc compilation, variable shaping/export, and source authoring.
The [capability and research record](counterpunch-parity.md) distinguishes the implemented scope from remaining Counterpunch/Fontra gaps.

The native suite passes 569 tests, with four existing tests requiring local models or an adjacent full-font checkout ignored.
The new six-test compiler suite checks variable outlines and advances, kerning applied once, ligatures, live mark anchors, overlapping and fractional Designspace rules, revision reuse, stale-worker invalidation and font-wide feature editing independent of master selection.
The 12-test variable-project suite includes source creation at an intermediate location, exact original-layer preservation, source identity across reorder/removal, save/reopen and guarded structural undo.
An application test additionally removes and restores a source before undoing its earlier glyph edit.

The headless CLI compiled the existing Virtua Grotesk Designspace into a 141,696-byte variable TTF without saving its source files.
Its tables include GDEF, GPOS, GSUB, HVAR, STAT, fvar and gvar.
A local debug invocation took 0.91 seconds wall-clock; this is a single local measurement, not a general performance guarantee.

```sh
target/debug/runebender compile "$RUNEBENDER_TEST_FONTS/VirtuaGrotesk.designspace" \
  --out /tmp/runebender-live-variable.ttf
```

The output path must be new; the CLI refuses to overwrite it.
Desktop Export instead writes a TTF under the source directory's `exports/` folder, using an immutable snapshot of unsaved edits.

Gray and Light captures were inspected for the expanded Masters controls at 1100 by 900 pixels and standalone-UFO Layers controls at 1100 by 720 pixels.
An additional Gray capture shows `wght=500`, the interpolated glyph and shaped `AV` in the proof strip.
These are headless rendering checks; native pointer, OS IME, accessibility and GPU behavior are not certified by them.

The browser quality test now downloads a TTF before and after a real pointer edit and compares the exported `glyf` table to prove unsaved geometry reaches export.
It also verifies the TrueType header, table bounds and the presence of cmap, glyf, hmtx and GPOS.

### Final continuation gates

| Check | Result |
|---|---|
| Formatting, copyright and whitespace checks | Pass |
| Native warning-denied Clippy and documentation | Pass |
| Full native test suite | 569 passed; 4 existing tests ignored |
| Native optimized build | Pass |
| Locked dependency advisory check | Pass |
| Browser optimized build and warning-denied Clippy | Pass |
| Browser interaction and export checks | Pass at all three display densities |

The browser's export-before-and-after-edit check runs at 1× density; its other interaction checks run at all three densities.
The final Gray and Light browser captures were inspected.

| Density | Frames rendered during test | Median render time | P95 render time |
|---|---|---|---|
| 1× | 144 | 3.9 ms | 5.5 ms |
| 2× | 144 | 9.0 ms | 11.1 ms |
| 1.25× | 136 | 5.2 ms | 6.9 ms |

The final tested WASM SHA-256 is `270f16af547676cc292c4c492be3b54a6da226e9981fc27818f0db7867825f58`.
These are working-checkout checks, not a new clean-checkout proof or a native input/accessibility certification.
The existing `block` 0.1.6 future-compatibility notice remains visible.

# Variable-first Project

Decision date: 2026-09-18.
Baseline: `5c37be7717e780aac3fcb369b033148f57026ef9`.

## Evidence and pins

| Reference | Inspected revision | Relevant code |
|---|---|---|
| Counterpunch editor | `1cc976ae88de2f7b95c823796b39040fede30188` | [model facade](https://github.com/counterpunchspace/editor/blob/1cc976ae88de2f7b95c823796b39040fede30188/webapp/js/babelfont-model.ts), [patch/history bridge](https://github.com/counterpunchspace/editor/blob/1cc976ae88de2f7b95c823796b39040fede30188/webapp/js/patch-sync-engine.ts), [Yjs schema](https://github.com/counterpunchspace/editor/blob/1cc976ae88de2f7b95c823796b39040fede30188/webapp/js/change-bridge-ydoc.ts) |
| Counterpunch Rust bridge | same | [manifest](https://github.com/counterpunchspace/editor/blob/1cc976ae88de2f7b95c823796b39040fede30188/babelfont-fontc-build/Cargo.toml), [reconstruction and compilation](https://github.com/counterpunchspace/editor/blob/1cc976ae88de2f7b95c823796b39040fede30188/babelfont-fontc-build/src/lib.rs), [interpolation](https://github.com/counterpunchspace/editor/blob/1cc976ae88de2f7b95c823796b39040fede30188/babelfont-fontc-build/src/interpolation.rs) |
| Counterpunch Babelfont fork | `df18a2f6ab348e71d7d91f96090ff0ee8638776c` | [yanone fork](https://github.com/yanone/babelfont-rs/tree/df18a2f6ab348e71d7d91f96090ff0ee8638776c) |
| Counterpunch Norad fork | `cdd7469a1f4d5459cf5c264079c04ac6149f23aa` | [FontSource/FontSink](https://github.com/yanone/norad/tree/cdd7469a1f4d5459cf5c264079c04ac6149f23aa) |
| Counterpunch fontc family | `484e26c5674ee4a6f5fed5e00aa5b71de3823610` | [lockfile](https://github.com/counterpunchspace/editor/blob/1cc976ae88de2f7b95c823796b39040fede30188/babelfont-fontc-build/Cargo.lock) |
| Fontra | `65eb043dbac9e41482ab40034b26757e2541e39d` | [classes](https://github.com/fontra/fontra/blob/65eb043dbac9e41482ab40034b26757e2541e39d/src/fontra/core/classes.py), [backend protocols](https://github.com/fontra/fontra/blob/65eb043dbac9e41482ab40034b26757e2541e39d/src/fontra/core/protocols.py) |
| Upstream Babelfont, selected | `29bdedbbfa7d3150b651dbd7c94fce6b79677ca4` | [model](https://github.com/simoncozens/babelfont-rs/tree/29bdedbbfa7d3150b651dbd7c94fce6b79677ca4/babelfont/src), [manifest](https://github.com/simoncozens/babelfont-rs/blob/29bdedbbfa7d3150b651dbd7c94fce6b79677ca4/babelfont/Cargo.toml) |

Counterpunch's TypeScript objects are getters/setters over Babelfont-shaped JSON.
PatchSyncEngine coordinates JSON edits, Yjs transactions, per-glyph/layer/font undo scopes, semantic change metadata, and window synchronization.
The Yjs schema uses glyph/layer maps, ordered shapes, and indexed anchor/guide maps.
The worker receives Yjs updates; Rust `yrs` state is reconstructed into canonical JSON and then cached `babelfont::Font` values, including subset caches with revision invalidation.
Compilation clones/filters those values and runs fontc/fontir/fontbe in a worker; shaped previews consume compiled bytes.
Interpolation delegates to Babelfont's multi-axis model with explicit component resolution and a separate metrics interpolation path.
This is evidence for a model/backend boundary, not for replacing the Rust editor with JSON or a CRDT.

Counterpunch's fork history identifies its reasons: in-memory FontSource/FontSink I/O, CRDT identifiers, array node serialization at the JS boundary, component-alignment preservation, and compile-time FIP001 subtraction.
Its [source-save dispatch](https://github.com/counterpunchspace/editor/blob/1cc976ae88de2f7b95c823796b39040fede30188/webapp/js/font-manager.ts) only saves `.babelfont` JSON and `.glyphs`; UFO, Designspace, Glyphspackage, VFJ, and SFD are imports and unsupported original-format saves throw.
Runebender cannot adopt that save restriction.
Counterpunch and Fontra are GPL-3.0 references; their implementation code is not copied here.

Fontra separates font-wide axes/sources/kerning/features from `VariableGlyph { axes, sources, layers }`.
A `GlyphSource` addresses a named layer and location, optionally relative to a font source; a layer contains a static glyph.
Its readable backend returns variable glyphs independently of font-wide data; the writable protocol adds put/delete methods, while watch, images, and shaper data have explicit capabilities.
This supports keeping storage, glyph-local layers, and source participation distinct.

Upstream Babelfont 0.2.1 is MIT OR Apache-2.0, Rust 1.85+, with a 2026-09-10 HEAD and ongoing converter work.
It models font axes, mapped design locations, masters, glyph-local axes, default/associated/free layers, intermediate locations, components, anchors, features, and kerning.
Feature-gated converters cover UFO/Designspace, Glyphs, FontLab, FontForge, Fontra, RoboCJK, TTF and VFB; support is converter-specific, not a round-trip guarantee.
The dependency can build without its compiler; WASM has UUID JS support, but the default feature set also enables Rayon and fontc.
The selected core disables defaults and enables `types`, `glyphs` and `fontir`: omitting `glyphs` produces unresolved imports in three upstream filters, and `types` annotations are imported unconditionally.

## Preservation blocker and choice

[UFO conversion](https://github.com/simoncozens/babelfont-rs/blob/29bdedbbfa7d3150b651dbd7c94fce6b79677ca4/babelfont/src/convertors/ufo.rs) narrows advances to `f32` and kerning to `i16`; glyph note, height, image and identifier handling are incomplete.
[Designspace loading](https://github.com/simoncozens/babelfont-rs/blob/29bdedbbfa7d3150b651dbd7c94fce6b79677ca4/babelfont/src/convertors/designspace.rs) uses `filter_map(...ok())` for axes and sources, iterates the default UFO's glyph list, and leaves uservalue handling as a TODO.
These are blockers to using that converter as authoritative editable storage.
`cargo test --test babelfont_contract --locked` reproduces precision and fractional-kerning limitations against the actual pinned dependency.

Evaluate these choices against Runebender's goal of being the best possible font editor for Eli's tastes and type-design work.
Counterpunch and Fontra provide technical references, while Runebender's workflow and source-preservation needs determine the design.
Live compilation makes previews reflect the font being edited, shaped variable previews expose positioning and substitution behavior, and interpolated source creation supports further design work.
Those benefits justify the capabilities independently of their presence in another editor.
Runebender keeps those operations in Rust and treats UFO/Designspace as editable source formats.
Babelfont geometry and the preserving Norad adapter are implementation choices to assess on correctness, fidelity and maintainability, not requirements to match Counterpunch's architecture.

## Implemented boundary

The selected dependency is upstream Babelfont at the exact revision above, with `default-features = false` and `types,glyphs,fontir` enabled.
The direct fontc dependency is pinned to 1.0.0 with default features disabled, matching Babelfont's fontir/fontbe family.
Enabling it required the compatible ICU 2.1 normalizer, properties and segmenter versions in both lockfiles; Parley accepts that range.
The added compiler graph passes the RustSec advisory check; this is dependency selection and advisory review, not a claim of a full third-party source audit.
`document::variable` owns Babelfont glyph geometry and a preserving Norad projection for metadata, exact advances and exact affine coefficients.
`document::babelfont` reconciles geometry through that boundary without narrowing the saved UFO values.
Source-wide feature text, groups and exact fractional kerning have canonical ownership by stable source identity; other source metadata remains in preservation templates while its migration is incomplete.
This is not yet a complete migration of every editing algorithm and metadata field to Babelfont APIs.
The [migration checklist](babelfont-migration-checklist.md) defines the remaining work, its order and the evidence required to call that migration complete.

Existing Norad editing algorithms use scoped source guards.
Guards reconcile edits into Babelfont before another Project operation can run.
Save materializes the canonical geometry into the preserving UFO payload.
The compatibility projections cost memory and a comparison pass per scoped edit.
`SourceId`, `LayerId`, `VariableGlyph` and `GlyphSource` separate stable identity, source order and interpolation participation.

The compiler snapshot includes all live masters and intermediate layers, axes, instances, features, anchors, groups and kerning.
Babelfont's IR source feeds fontc directly, generating a complete TTF with variable outlines, advances and positioning.
The UFO-derived snapshot bypasses Glyphs-specific feature rewriting: the pinned upstream FEA AST panics on fractional conditions, while fontc accepts the original feature text.
Overlapping Designspace rule regions are partitioned at OpenType coordinate precision so all applicable lookups run in document order.
HarfRust shapes those bytes at the same normalized location used by Skrifa to draw their outlines.
Desktop preview uses one background compiler with a single replaceable pending snapshot; stale results never publish.
Preview location changes reuse the compiled revision.
Export and the `compile --out` command use the same pipeline, including unsaved edits, without Python or external build scripts.
Feature drafts are checked with the same complete compiler and stored in the default source independently of editor selection.
Other sources retain their existing feature files.
OpenType quantization occurs at compilation; it does not alter source-file precision.

Source commands add an interpolated UFO, rename or relocate an existing source, reorder sources, and remove non-default sources with undo/redo.
Creating a full source at an intermediate location preserves the former sparse layer as auxiliary data.
Removing a source changes the Designspace and retains its on-disk UFO.
Glyph auxiliary layers can be copied or removed independently.
Structural undo refuses to overwrite later content edits; those edits must be undone first.
Source removal and reordering are guarded while live experiment branches retain source-index references.

Interpolation checks contour segmentation and point types, component base order, matching anchors, finite values and distinct locations.
It varies horizontal/vertical advances, contours, anchors and component affine coefficients.
Components resolve recursively at the target location, reporting missing or cyclic references.
Pair kerning resolves each source's explicit/group fallback and retains fractional values in the editable model.
Non-varying metadata comes from the default source, and auxiliary layer metadata remains untouched.

## Format contract

| Input | Editable representation and save behavior |
|---|---|
| UFO | One variable Project source; all layers, exact glyph payloads, font info, libs, features, groups, kerning, images and data remain in UFO adapters. |
| Designspace | Continuous axes and maps, full sources, sparse layer sources within those UFOs, named instances, rules and typed metadata remain in the document; save writes canonical UFOs and edited Designspace metadata. |
| Python Babelfont NFSF directory | Multiple sources, mapped axes, static instances, intermediate/background/named layers, supported names/metrics, contours, components, anchors, Unicode, export flags, feature text and fractional kerning import into new UFO/Designspace destinations; the source package is never rewritten. |
| Rust Babelfont JSON | Explicitly rejected; it is a different format from the existing Python importer and currently lacks a lossless editable adapter. |
| Glyphs and compiled TTF/OTF | Existing conversion/import paths remain behind Project; this change does not turn those existing importers into lossless original-format editors. |

Unknown Python package fields fail during decoding.
Localized names beyond the default string, unsupported metrics, guides/hints, feature objects, variable-instance ranges and contour transforms fail explicitly.
Original layer/source identifiers and background status are retained under one documented UFO lib key.
Includes in Python package feature text remain unsupported because the relative source tree is not copied.
Glyph-specific intermediate locations sharing a UFO layer name must agree; conflicting locations are rejected rather than merged.

Designspace XML is checked before typed decoding so unsupported elements/attributes cannot disappear on save.
Coordinates that cannot round-trip through Norad's Designspace numeric representation are rejected.
Discrete axes, cross-axis mappings, anisotropic coordinates, unknown/duplicate source axes, missing mapped-default sources, missing files/layers, and layer-only UFOs without a full source fail explicitly.
Re-interpolation from multiple remaining sources also requires a default source; one remaining source can still be copied directly.
Designspace rules are preserved and supplied to the compiled variable shaper.
This is an explicit supported subset, not a claim of universal Designspace or Babelfont compatibility.

## Regression evidence

`tests/babelfont_contract.rs` demonstrates the upstream width and kerning blockers against the selected dependency.
`tests/variable_project.rs` uses a mapped two-axis fixture with an intermediate layer and an auxiliary-only glyph.
It exercises exact geometry, anchors, recursive components, sparse participation, fractional group kerning, active-source independence, layer history, invalid input, and complete supported source round-trips.
The Babelfont importer tests open a multi-source package, save and reopen its new Designspace, and compare original package bytes.
Existing CLI, shaping, live-document, source switching, save-as, reload, metadata and undo tests remain part of the full native gate.
The browser quality matrix checks actual dragging, undo/redo, text input, themes and idle rendering against the shared Project code.

## Baseline inventory

`src/document/project.rs` combines Master UFO ownership, paint caches, undo, loaders, Designspace metadata and interpolation in one module.
65 source files mention Norad; application `FontModel::font_mut`, direct `Project::masters`, live edits, experiments, and save/reload expose the main mutation routes.
The original interpolation blends advance and contour points only, compares flattened lengths, and takes components/anchors from the active master.
The original Designspace loader ignores axis maps, deduplicates full sources by filename, and silently omits layer sources with no full source in the same file.
The original `formats::babelfont_import` only reads Python NFSF directory packages, rejects axes/instances/multiple masters/additional layers, and drops guides/hints/production names/application metadata.
Compiled TTF/OTF import, Glyphs conversion, shaping and existing UFO metadata adapters remain required compatibility paths.

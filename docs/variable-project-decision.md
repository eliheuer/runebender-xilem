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
Native core builds without the compiler; WASM has UUID JS support, but the default feature set also enables Rayon and fontc.
The selected core disables defaults and enables `types` and `glyphs`: omitting `glyphs` produces unresolved imports in three upstream filters, and `types` annotations are imported unconditionally.

## Preservation blocker and choice

[UFO conversion](https://github.com/simoncozens/babelfont-rs/blob/29bdedbbfa7d3150b651dbd7c94fce6b79677ca4/babelfont/src/convertors/ufo.rs) narrows advances to `f32` and kerning to `i16`; glyph note, height, image and identifier handling are incomplete.
[Designspace loading](https://github.com/simoncozens/babelfont-rs/blob/29bdedbbfa7d3150b651dbd7c94fce6b79677ca4/babelfont/src/convertors/designspace.rs) uses `filter_map(...ok())` for axes and sources, iterates the default UFO's glyph list, and leaves uservalue handling as a TODO.
These are blockers to using that converter as authoritative editable storage.
`cargo test --test babelfont_contract --locked` reproduces precision and fractional-kerning limitations against the actual pinned dependency.

Use a Runebender-owned Project with glyph-local layer ownership and explicit source/location identity.
Keep exact UFO glyph payloads and format metadata in preservation adapters; do not round-trip user files through Babelfont to obtain a variable model.
Keep Babelfont behind a private, checked adapter so native JSON and computational use can be extended or the dependency replaced without UI type changes.
Existing tools use guarded source projections while they migrate to glyph/layer operations; mutable access must commit back to the Project before save, interpolation, or another source edit.
Undo, redo and external reload must cross the same boundary.
Never treat a missing source, an invalid coordinate map, or incompatible interpolation structures as successful conversion.

## Implemented boundary

The selected dependency is upstream Babelfont at the exact revision above, with `default-features = false` and `types,glyphs` enabled.
The private coordinate adapter uses its `Axis` conversions.
The variation adapter uses the same `fontdrasil` 1.0.0 backend as that revision, with `RoundingBehaviour::None` for editable f64 values.
This replaces the local hand-written variation-model implementation without converting glyph geometry to Babelfont's narrower layer model.
Neither dependency's types appear in the application-facing API.

Project now owns a glyph-keyed store of layers and glyph-free source metadata templates.
`SourceId`, `LayerId`, `VariableGlyph` and `GlyphSource` describe identity and participation independently of editor selection.
`glyph_sources` includes only layers that participate in that glyph's model, so intermediate and missing non-default layers are genuinely glyph-local.
`edit_layer` and `undo_layer` address a layer directly; default-layer history remains shared with the existing editor commands.
Source order is fixed until reload.

Existing Norad editing algorithms run on source projections through scoped guards.
Guards reconcile changed glyphs, added/deleted layers and source metadata into the owned store before another Project call is possible.
The projections deliberately retain full payloads while those tools migrate, which costs memory and a comparison pass on each scoped edit.
Save and interpolation read canonical glyph layers, not whichever projection is selected in the editor.
Live edits, experiments, proposal installation, source switching, browser edits, undo and reload have been routed through this boundary.
The old source cache and single-source operations now live in `document/source.rs`.

Interpolation checks contour segmentation and point types, component base order, unique matching anchors, finite values and distinct locations.
It varies horizontal/vertical advances, contour coordinates, anchor positions and component affine coefficients.
Components resolve recursively at the target location, with missing/cyclic references reported as errors.
Pair kerning resolves each source's explicit/group fallback before interpolation and keeps fractional values.
Non-varying metadata comes from the default source, and auxiliary layer metadata remains untouched by interpolation.
The application no longer has a separate point-only interpolation path.

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
Designspace rules are preserved and retain the existing preview substitution path.
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

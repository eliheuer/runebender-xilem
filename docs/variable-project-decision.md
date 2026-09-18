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

## Baseline inventory

`src/document/project.rs` combines Master UFO ownership, paint caches, undo, loaders, Designspace metadata and interpolation in one module.
65 source files mention Norad; application `FontModel::font_mut`, direct `Project::masters`, live edits, experiments, and save/reload expose the main mutation routes.
The original interpolation blends advance and contour points only, compares flattened lengths, and takes components/anchors from the active master.
The original Designspace loader ignores axis maps, deduplicates full sources by filename, and silently omits layer sources with no full source in the same file.
The original `formats::babelfont_import` only reads Python NFSF directory packages, rejects axes/instances/multiple masters/additional layers, and drops guides/hints/production names/application metadata.
Compiled TTF/OTF import, Glyphs conversion, shaping and existing UFO metadata adapters remain required compatibility paths.

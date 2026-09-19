# Counterpunch and Fontra capability comparison

Runebender's goal is to be the best possible font editor for Eli's tastes and type-design work, as described in [Design](../DESIGN.md).
This comparison helps evaluate useful capabilities and identify limitations that could affect that work.
Counterpunch provides references for live compilation and source authoring; Fontra provides references for glyph-local sources and layers.
Each idea needs an independent reason to belong in Runebender, based on workflow, correctness, responsiveness and source preservation.
Differences listed here are evidence for product decisions, not an automatic requirement to reproduce another editor's features or architecture.

## Current scope

| Capability | Runebender implementation | Remaining boundary |
|---|---|---|
| Babelfont model | Canonical glyph geometry, axis conversions and a complete compiler snapshot | Existing tools and exact UFO metadata use preserving Norad projections |
| Live compilation | Babelfont/fontc compiles unsaved sources; desktop coalesces background work | Full-font builds rather than Counterpunch's subset caches; synchronous browser compilation |
| Variable text | HarfRust shapes compiled GSUB/GPOS and advances; Skrifa draws the same variable outlines | Broader script/font corpus testing remains valuable |
| Binary export | Same Rust pipeline, native TTF output and browser download | VARC and exhaustive OpenType metadata parity are not claimed |
| Source editing | Interpolated creation, rename/location changes, reorder, removal and undo/redo | Existing continuous axes only; import-existing-source and axis-authoring UI remain |
| Glyph layers | Stable source/layer identity, sparse intermediates, auxiliary copy/removal and history | Fontra's complete local-axis/source authoring workflow remains broader |
| Source formats | Preservation-aware UFO/Designspace saving | Unsupported extensions fail explicitly; see the format contract |

Designspace rules follow the [documented ordered, OR/AND semantics](https://fonttools.readthedocs.io/en/latest/designspaceLib/xml.html#rules-element), including overlapping and fractional regions.
The tests in `tests/variable_compile.rs` exercise variable outlines, advances, kerning, ligatures, anchors, Designspace substitutions and stale-worker invalidation.
`tests/variable_project.rs` covers source identity, interpolation, layer preservation, structural history and source round-trips.
The [validation record](variable-project-validation.md) distinguishes executed checks from product goals.

## Why Counterpunch does not yet save UFO/Designspace

The inspected Counterpunch revision is `1cc976ae88de2f7b95c823796b39040fede30188`.
Its [README](https://github.com/counterpunchspace/editor/blob/1cc976ae88de2f7b95c823796b39040fede30188/README.md) describes UFO/Designspace saving as in development.
Its [save dispatch](https://github.com/counterpunchspace/editor/blob/1cc976ae88de2f7b95c823796b39040fede30188/webapp/js/font-manager.ts) serializes Babelfont JSON and Glyphs text, and explicitly refuses unsupported original formats.

The history shows active UFO work rather than a stated rejection of the format.
[The July 5 change](https://github.com/counterpunchspace/editor/commit/a653291924ad0328ce5276fa8403fbaed3fcde64) added in-memory UFO/Designspace import, UFO entry serialization, a worker handler and a round-trip test.
The Rust bridge still exposes `save_font_as_ufo_entries`, which serializes a single master.
[The July 21 change](https://github.com/counterpunchspace/editor/commit/6a7ec4c0f303b78573ecf5e5adf62b9ada5b4264) added Glyphs serialization and replaced an unconditional Babelfont-JSON save path with format-aware dispatch.
The UFO directory and complete multi-source Designspace save path have not been connected through that dispatch.

The evidence therefore points to unfinished serializer/filesystem integration.
That is an inference from the code and history, not an explanation stated by the authors, and no source found establishes an intentional policy against UFO/Designspace.
Runebender already has native Norad persistence; adopting the computational architecture does not require inheriting that missing integration.

## Source authoring references

Counterpunch's [Babelfont facade](https://github.com/counterpunchspace/editor/blob/1cc976ae88de2f7b95c823796b39040fede30188/webapp/js/babelfont-model.ts) creates a master by interpolating layers at a target location, updates axis bounds/mappings when necessary, and groups the changes in a transaction.
Removal deletes the master and associated layers together.
Runebender implements the corresponding interpolated-source transaction with stable identities, preserving UFO persistence and explicit undo.
Its current UI does not yet extend axis bounds automatically.
Fontra's [glyph model](https://github.com/fontra/fontra/blob/65eb043dbac9e41482ab40034b26757e2541e39d/src/fontra/core/classes.py) additionally supports glyph-local sources and axes.
These GPL projects are behavioral references; their implementation code is not copied into Runebender.

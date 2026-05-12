# runebender-core

Platform-independent core types for the [Runebender][rb] font editor.

The two front-ends — native [`runebender-xilem`][xilem] and
WASM/Vue [`runebender-comfy`][comfy] — both depend on this crate so
they share a single source of truth for the editing state and data
model that doesn't depend on the host UI framework.

## Status

Early. Only modules with no `kurbo` geometry in their public API live
here right now:

```
src/
├── editing/
│   ├── edit_types.rs    # EditType enum (undo grouping)
│   ├── selection.rs     # Selection set (Arc<BTreeSet<EntityId>>)
│   └── undo.rs          # UndoState<T> (generic undo/redo)
└── model/
    ├── entity_id.rs     # EntityId (unique IDs for points/paths)
    └── kerning.rs       # UFO-spec kerning lookup
```

The rest (`path/`, `viewport`, `mouse`, `hit_test`, `workspace`,
`glyph_renderer`) is still duplicated in each consumer because of a
`kurbo` version conflict:

- `runebender-xilem` is pinned to `kurbo = "0.12"` by `masonry 0.4`.
- `runebender-comfy` is on `kurbo = "0.13"` by `peniko 0.5` / `vello`.

A library can only declare one `kurbo` version; until the xilem-side
ecosystem catches up to 0.13 (the upcoming `masonry-2` transition),
the geometry-touching modules stay duplicated. When the alignment
happens, they'll move here.

## License

Apache-2.0 — matches `runebender-xilem` (the source of the ported
modules) and stays GPL-3.0-compatible so `runebender-comfy` can
include it.

[rb]: https://github.com/eliheuer/runebender-xilem
[xilem]: https://github.com/eliheuer/runebender-xilem
[comfy]: https://github.com/eliheuer/runebender-comfy

# Native anchor recipes

These examples are pure Python workers for the version-one script envelope.
They receive one immutable JSON capture on standard input and emit one typed JSON result on standard output.
Human-readable diagnostics go to standard error.

The workers never open a font, create a mutable font wrapper, open a socket, write a source file, apply an edit, or invent authorization.
The editor runner owns those boundaries.

The version-one input envelope is:

```json
{
  "schema_version": 1,
  "job_id": "job-1",
  "input_hash": "capture-sha256",
  "source": 7,
  "parameters": {
    "recipe": "move_named_anchors",
    "glyphs": ["A"],
    "anchor_names": ["top"],
    "dx": 0.5,
    "dy": -1.25
  },
  "layers": [
    {
      "guard": {
        "glyph": "A",
        "glyph_id": "glyph-a",
        "layer": "Regular",
        "expected_revision": "rev-a"
      },
      "width": 600.0,
      "anchors": [
        {"id": "anchor-top", "name": "top", "x": 50.125, "y": 600.5}
      ]
    }
  ]
}
```

`source` is explicit and is never inferred from the active editor source.
`glyphs` is the selected glyph scope; an empty list is an intentional empty scope.
An empty `anchor_names` list means all named anchors for the move recipe.
The list recipe can include unnamed anchors; the move recipe always ignores them.

Run the same self-contained worker the editor runner invokes with `python -I recipe.py`:

```sh
python3 -I scripts/recipes/anchor_recipes.py < scripts/recipes/fixture-list-input.json
python3 -I scripts/recipes/anchor_recipes.py < scripts/recipes/fixture-move-input.json
```

The `move_named_anchors` recipe in `anchor_recipes.py` emits `edits` matching the native `AgentLayerEdits` shape:
each entry has `target` with the exact captured `AgentLayerGuard` and ordered `operations` using `set_anchor`, `anchor_id`, `x` and `y`.
The result's `reads` array contains exact `AgentLayerGuard` values for read-only layers, and `report` is a deterministic human-readable string containing glyph, source, layer, name, id and coordinates.
No-op offsets return an empty edit list.
The 64 guarded-layer and 256-operation limits are checked before a result is accepted.

The runtime owner confirmed the `guard` layer-envelope name and the result fields.
This worker does not add a second Rust protocol or change the native adapter.

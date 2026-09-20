# Python anchor recipe worker

This worker implements the Python-only examples phase approved in `agent-scripting-workflow.md`.
It owns only `scripts/recipes/` and this document.
It does not add Rust scripting, a mutable font wrapper, a socket client, source-file writes, automatic Apply, authorization, model calls, or credentials.

## Frozen runner contract

The recipes consume the runner's version-one `ScriptRecipeInput` JSON envelope.
The top-level fields are `schema_version`, `job_id`, `input_hash`, `source`, `parameters`, and `layers`.
`parameters` is an open JSON map owned by the selected recipe.
This worker uses `recipe`, `glyphs`, `anchor_names`, `include_unnamed`, `dx`, and `dy`.
The explicit `source` is a non-negative source identity and is never inferred from the active editor source.

Each layer has `guard`, `width`, and `anchors`.
`guard` is the exact serialized `AgentLayerGuard` with `glyph`, `glyph_id`, `layer`, and `expected_revision`.
Each anchor has `id`, optional `name`, `x`, and `y`.
Duplicate anchor names are valid because edits address the distinct stable `id` values.

The result has exactly the runner-owned fields `schema_version`, `job_id`, `input_hash`, `report`, `reads`, and `edits`.
`report` is one deterministic string containing the result and, for anchor reads, glyph, source, layer, name, id, and coordinates.
`reads` contains `AgentLayerGuard` values for read-only layers.
`edits` contains native-shaped `AgentLayerEdits` values with `target` and ordered `operations`.
Each move operation is `{"op":"set_anchor","anchor_id":...,"x":...,"y":...}`.
The worker never creates `actor`, `operation_key`, `authorization`, or `expected_document_epoch` values.
The host creates those values only for an explicit reviewed Apply action.

The canonical runner branch confirmed these names after the initial worker inventory.
The recipes therefore use the shared schema rather than creating a parallel result format.

## Recipes

`anchor_recipes.py` is self-contained and uses only the Python standard library.
It can run in the same isolated mode used by the native runner:

```sh
python3 -I scripts/recipes/anchor_recipes.py < scripts/recipes/fixture-list-input.json
python3 -I scripts/recipes/anchor_recipes.py < scripts/recipes/fixture-move-input.json
```

The input's `parameters.recipe` selects `list_anchors` or `move_named_anchors`.
The list recipe reports named and unnamed anchors for the selected glyphs.
The move recipe offsets existing named anchors by finite `dx` and `dy` values.
An empty `glyphs` list is an intentional empty scope.
An empty `anchor_names` list means all named anchors for the move recipe.
An offset of zero is a no-op and produces no edits.
No source fallback, cross-source loop, or hidden selection expansion is performed.

The worker sorts layers, anchors, reads, edits, and operations by stable identities before serializing the result.
The result echoes `job_id` and `input_hash` from the immutable capture.
All guard values are copied unchanged into native-shaped edit targets.
Malformed, nonfinite, unknown, cross-source, and oversized requests are rejected or reported as unsupported without partial output.
The worker accepts at most 64 guarded layer entries and 256 operations.
It never splits an oversized proposal into hidden partial batches.

Diagnostics and structured output are separate.
Human-readable status is written to stderr.
Exactly one JSON result is written to stdout.
The result contains no font geometry beyond the captured anchor coordinates needed for the report and proposal.

## Fixture and acceptance evidence

`fixture-move-input.json` contains a synthetic source 7 capture with fractional coordinates, duplicate `top` names with distinct IDs, an unnamed anchor, and an unselected glyph.
`fixture-move-expected.json` is the canonical three-operation proposal.
`fixture-list-input.json` checks the read-only report and unnamed-anchor display.
`test_anchor_recipes.py` covers deterministic ordering, fractional offsets, duplicate names, unnamed anchors, empty scope, no-op handling, nonfinite parameters, unsupported scope, exact guard preservation, and both bounded limits.

`acceptance_harness.py` runs the same isolated subprocess entry point used by the runner and verifies one JSON stdout line, stderr diagnostics, and the canonical fixture result.
Its report marks pure recipe tests as `pass` while marking application evidence and model evidence as `not_run` unless the coordinator supplies a reviewed runtime evidence JSON file.
Optional runtime evidence may record source manifests, receipts, and undo observations, but the harness does not create a live host or perform an Apply.
`writes_performed` remains false for this worker.

Run the pure worker checks with:

```sh
python3 -m unittest discover -s scripts/recipes -p 'test_*.py'
python3 scripts/recipes/acceptance_harness.py
```

These checks are not native application acceptance.
They do not prove Python interpreter availability in the editor, subprocess cancellation, UI preview behavior, guarded Apply, native undo, source preservation, or model interpretation.
Those require the coordinator's reviewed runtime, UI, and disposable-font fixtures.

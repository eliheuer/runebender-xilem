# Native Nodes canvas worker

Status: bounded layout and content-projection seam delivered for integration.

## Ownership

This worker owns native `application/view/canvas/nodes.rs`, `application/view/panels/nodes.rs`, and toolkit-independent Nodes layout and hit testing in `ui/nodes.rs`.
The worker does not own graph execution, Python process management, live font handles, Script library persistence, or font mutation.

## Content projection

`ui::nodes::NodeContentMap` is a session-only projection keyed by graph node ID.
`NodeContent::Script` carries source text, a content hash, and visible execution state.
`NodeContent::Image` carries an immutable PNG result, a retained previous image, and visible execution state.
`ImmutablePng` records PNG bytes, pixel dimensions, and the output hash that identifies the renderer capture.
`ContentState` distinguishes idle, running, current, stale, and bounded error output.
The projection contains no executor, mutable Project reference, font path, or live handle.

## Interaction contract

`node_region_hit` identifies header, ordinary body, embedded content, and resize regions after normal port hit testing.
Only the header may begin a graph move.
The native canvas receives the projection separately from graph and run-row state, so output refreshes need not rewrite a graph.
Code and image bodies remain inert until their focused child widgets are registered.
Code and image content must receive their own focused child-widget events before the graph canvas processes keyboard or pointer editing.
Canvas code changes should emit a distinct typed graph-value edit for `live.python` field `code`.
`MoveNode` and resizing are presentation-only graph edits and must not change semantic hashes, invalidate output, or trigger execution.

## Current limitation

This phase supplies the shared geometry and state seam only.
The Script UI worker must provide the shared real multiline buffer, and the live graph worker must publish `GraphSnapshot` output projections before native child widgets can render editable code and decoded proof PNGs.
No image preview in this phase claims an executed font proof.

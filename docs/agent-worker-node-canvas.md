# Native Nodes canvas worker

Status: native child widgets and deterministic headless content fixture implemented.
Compilation, native Gray/Light capture, and browser acceptance remain pending the coordinator's combined Workspace integration.

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
Running image nodes prefer the current immutable PNG and otherwise retain the previous accepted PNG.

## Interaction contract

`node_region_hit` identifies header, ordinary body, embedded content, and resize regions after normal port hit testing.
Only the header may begin a graph move.
The native canvas receives the projection separately from graph and run-row state, so output refreshes need not rewrite a graph.
Code and image bodies remain inert until their focused child widgets are registered.
Code and image content must receive their own focused child-widget events before the graph canvas processes keyboard or pointer editing.
Canvas code changes should emit a distinct typed graph-value edit for `live.python` field `code`.
`MoveNode` and resizing are presentation-only graph edits and must not change semantic hashes, invalidate output, or trigger execution.

The Python child is Masonry's real multiline `TextArea`, preserving platform focus, selection, clipboard, newline behavior.
Local text Undo/Redo is not implemented by the pinned TextArea; those keys are consumed to prevent accidental font-history changes.
It emits the complete authoritative projected value as `NodesEvent::EditCode` without running the graph.
PNG bytes are decoded only after their recorded dimensions match the image and are rendered by a clipped child widget.
Pointer drag pans the proof, scroll zooms it from 25% to 800%, and double-click returns to its fitted 100% view.
The node resize handle is outside the child rectangle, so resizing cannot become a text or image gesture.

## Native integration

`RUNEBENDER_NODES_CONTENT_FIXTURE=1` opens the actual comparison graph with editable Python and two checkerboard PNG children for native Gray and Light capture.
Those images use the explicit identity `ui-fixture-not-a-font-proof` and never claim executed-font evidence.
The native Comparison entry reads the canonical Workspace-owned `GraphSession` snapshot.
It projects retained Python state, reports, and exact paired proof PNGs into `NodeContentMap` without giving the canvas an executor or live font handle.
Inline code commits and completed header drags each become one revision-guarded interactive graph edit; resizing remains presentation-only.
Run and explicit Apply use the same typed live-command adapter as agent requests, and Apply remains unavailable until both current compiled-family proofs complete.
The comparison bar shows the stable `live.font` source and an editable, non-empty glyph-name scope initialized from the explicit overview selection.
Native Run, Cancel, Clear results, and Apply commands live outside the view in `editor/tools/nodes_controls.rs`.
Clear releases only terminal handles started by the native surface, leaving agent-owned work alone.
Completed proof PNG bytes are copied into an `Arc` once per artifact identity and reused across view rebuilds.
The live pump also removes cached bytes when native or external agent release drops the canonical handle.
Reports and graph diagnostics use bounded wrapping portals instead of extending the toolbar on one line.
Topology edits are converted into guarded graph patches against the exact snapshot displayed by the canvas, so an intervening agent edit rejects rather than being overwritten.
Inline code commits and completed header drags use that same displayed-snapshot guard.

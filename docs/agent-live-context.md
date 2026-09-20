# Live document context checkpoint

The native Unix editor serves the current unsaved canonical `Project` through a private mailbox.
The application thread captures context and executes font operations; the socket worker never owns a second font model.
The CLI and stdio MCP adapters use the same endpoint.
This checkpoint does not complete the [live editing milestone](agent-interface-plan.md).

## Connect and identify

Use `editor_sessions` and `editor_connect` in MCP, or pass an explicit socket path to `runebender agent call TOOL --session PATH --args JSON`.
Connecting returns `project_info`, including stable source IDs, current display indices, document revision, and socket document epoch.
A source ID keeps its meaning across reorder; a display index does not.
A removed source fails instead of selecting its replacement.
Live tools reject disk-only `master` arguments.

Every result delivered by the document mailbox includes `document_epoch`, `live_schema_version: 1`, and `server_version` (the package version).
The epoch identifies this endpoint lifetime, including across reopen and process restart.
It is an identity guard, not an authentication credential.
Pass `expected_document_epoch` with the epoch you read to reject another lifetime before the handler executes.
Omitting it retains compatibility with older tools, whose socket paths still select an explicit document.
The receipt-backed `agent_apply`, `agent_receipt` and `agent_history` tools require it.
The adapter never follows whichever window becomes active or reconnects to a different endpoint automatically.
Transport failures before application dispatch may lack an epoch and document revision.

## Coherent application context

Call `editor_context` with an empty object or the optional epoch guard.
The result includes:

- `document_revision`: the canonical Project revision at capture.
- `context_revision`: SHA-256 of the serialized context, for equality comparisons, not a monotonic counter or an accepted write precondition.
  The context includes the exact socket document epoch, so identical font and UI values in a replacement lifetime produce a different context revision.
- `context.source_id`, `glyph_id`, `glyph`, `layer`, `mode`, `tab_id`, and `tool`.
- `context.selection`: point, component, anchor, and overview glyph identities.
- `context.text`: editor/preview text, direction setting, disabled features, script and language settings.
- `context.location`: axis tags and user-coordinate values.
- `context.busy_gesture`: whether a canvas gesture has a private uncommitted draft.

The context is captured in one application-thread call.
Glyph reads inspect committed canonical state; an unfinished pointer gesture is not included in that state.
The existing foreground install/apply/undo operations reject while a gesture is active.
No edit implicitly targets the current selection.

The widget owns caret and text selection ranges, so both are null and `widget_text_ranges` is false.
Script, language and direction describe selected settings, not a resolved bidi/shaping run analysis.
The context has no arbitrary auxiliary-layer canvas selection; `auxiliary_layer_canvas_selection` is false.
A headless engine-only host returns `unsupported_context` instead of inventing application state.

## Canonical glyph reads

`read_glyph` adds `glyph_id` and `source_id` for root reads.
`contour_ids` corresponds to the existing contour array; points, component transforms and anchors include `id` strings.
These are opaque session identities; clients must scope them by document epoch, source/layer, and branch where applicable.
They survive supported rename and nonstructural edits, but are not persistent identifiers for save/reopen.
Disk commands can load a fresh Project for each call, so these IDs do not establish identity between disk calls.
Legacy proposal operations use explicit glyph names and revision-scoped point indices.
The [receipt-backed transaction tools](agent-live-transactions.md) use guarded logical glyph identities and stable point/anchor IDs for a bounded batch.
Experiment reads identify their branch and retain geometry identities but do not currently expose a logical root glyph ID.

Canonical live responses report `document_revision` and `saved=false`.
The latter means this call did not save source files, not that every byte of the document differs from disk.
Use `source_id` for uniform source identity; the legacy `source` field is retained and may contain a path or an integer depending on the older operation.

## Current transport limits

Socket requests and CLI/MCP input frames are bounded to 8 MiB.
The socket queue still has one pending slot and the editor response timeout remains 30 seconds.
A timeout or disconnect does not prove an operation failed to commit.
`agent_apply` now provides grouped atomic publication and in-memory retry receipts; `agent_receipt` reconciles a lost apply response.
Independent edit cancellation remains unsupported.
Do not blindly repeat legacy proposal/experiment mutations or history replay after a lost response.

MCP negotiation recognizes `2024-11-05`, `2025-03-26`, `2025-06-18`, and `2025-11-25`, and falls back to `2025-11-25` for an unknown version.
Only the tools capability is advertised.
This follows the [MCP version negotiation rule](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle#version-negotiation); it is not a claim of task, cancellation, streaming or remote transport support.
Argument validation remains operation-specific; complete generated-schema validation is pending.

## Acceptance coverage

The application test `live_socket_context_unsaved_apply_and_editor_undo` uses a real Unix socket serviced by `Workspace::call_live`, the same dispatch path used by the Xilem mailbox pump.
It reads an unsaved 12-unit width change, creates a revision-checked proposal, rejects the wrong epoch before install, applies under explicit authorization, checks grid/session refresh, and restores both through Undo install.
It checks context-hash stability and change, two separate document epochs, and absence of a saved font path.
Canonical inspection coverage also verifies identity retention through a width edit and glyph rename.
CLI/MCP process tests cover bounded input and protocol negotiation.
These checks do not certify native pointer/IME behavior, actual model-client image delivery, compiled proof lineage, or the remaining transaction milestone.

The original context checkpoint passed 808 regular tests and both opt-in real-font tests (810 executed); the two local-model tests remain unrun.
Strict native and browser Clippy, warnings-denied documentation, the native release build, dependency advisories, formatting and copyright checks passed.
The browser quality matrix passed at DPR 1, 2 and 1.25, including unsaved-outline export, drag, undo/redo and themes.
Gray and Light native headless captures were inspected.
All 3,034 original Virtua source files retained their initial SHA-256 hashes; tests used a disposable copy.
Local logs, source manifests and captures are stored under `/private/tmp/runebender-agent-interface-20260920-phase1a`.

## Disposable application fixture

`runebender agent fixture --duration-seconds 300` starts a synthetic native Workspace with a private Unix endpoint and no foreground window.
It accepts no font path and has no save control.
The synthetic `A` has a rectangle contour and top anchor, with an unsaved canonical width change from 400 to 412 before Workspace construction.
This does not simulate native pointer or IME input.

Keep stdin open and read the first stdout line for the readiness JSON, including `session`, `glyph`, `source_id`, `initial_advance`, `unsaved_advance` and `fixture_version: 1`.
Send agent calls to that explicit socket through the ordinary CLI or MCP adapter.
A separate stdin control channel accepts newline-delimited JSON with `action` equal to `state`, `undo`, `redo` or `shutdown`.
These are fixture controls, not production agent tools.

State, undo and redo responses expose `canonical_advance`, `cache_advance`, `session_advance`, layer history depths, document revision and `source_exists`.
Undo and redo execute `Workspace::undo_active_edit`, allowing the harness to compare application state against the agent's read after an install.
Frames are limited to 1024 bytes; lifetime is limited to 1–3600 seconds.
Stdin EOF, shutdown or the deadline terminates the fixture and removes its endpoint.

The process integration test `application_fixture_refreshes_and_undoes_an_agent_edit` passes against the fixture executable.
It reads width 412 through the socket, installs width 430, verifies canonical/cache/session agreement, then checks ordinary undo to 412 and redo to 430.
It also verifies that the synthetic source path does not exist and that shutdown removes the socket.
Evidence and a pinned executable are under `/private/tmp/runebender-agent-fixture-20260920`; this test does not establish actual desktop or OMP model-client use.

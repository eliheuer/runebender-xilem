# Guarded live graph session and Python comparison adapter

This phase owns the reusable graph session in `document::nodes_session`, the first comparison definitions in `document::nodes_live` and the thin native execution adapter in `application::editor::tools::nodes_execution`.
It does not create another font model, Python runner, proof compiler, receipt ledger or history implementation.
The original Project and disk sources remain unchanged throughout graph traversal.

## Graph session contract

`GraphSession` owns the one canonical editable `NodeGraph`, graph-only Undo and Redo, exact graph and document-lifetime identity, full layout revision, independent semantic revision and semantic hash.
Layout moves increment the full revision without invalidating execution semantics.
An edit followed by graph Undo still increments the semantic revision, so an old completion cannot publish through an ABA-identical content hash.

Interactive canvas edits use `mutate_interactive` with a full revision guard and do not consume agent retry capacity.
Agent graph mutations use `mutate` with actor and operation-key receipts.
Both paths share the same atomic patch implementation, graph validation, limits and graph-only history.
Run, mutation, interactive mutation and cancellation request schemas are returned by discovery from the typed Rust definitions.

The graph is limited to 64 nodes, 128 links and 1 MiB of serialized state.
An atomic patch is limited to 128 edits and 1 MiB of serialized request state.
Python code is limited to 256 KiB and its parameter object to 64 KiB.
Graph history, mutation receipts, active runs, run receipts, outputs, errors, identifiers, reports and error text all have discoverable hard bounds.

The first executable topology is exactly base `live.font` to unchanged `live.proof`, plus the same base through one `live.python` to a changed `live.proof`.
Automatic `live.apply`, disk install, additional Python nodes and ambiguous topologies reject before queue submission.
One run identity binds the graph semantic revision and hash, document epoch and revision, source, canonical family input, exact code, typed parameters, script input and identical proof recipes.
Cancellation targets one selected run and late completion after graph, document or font change becomes stale without published output.

Completed comparison output requires one derived FontVersion identity and both compiled-family proofs.
The unchanged proof must carry the exact captured base canonical input hash.
The changed proof must carry the exact derived family input hash published by the Python node.
Source-only scope, swapped branches, repeated output kinds, malformed failures and oversized worker errors become a bounded terminal failure instead of leaving a run active.
A bounded report may share the Python node with its required FontVersion output.

## Native execution adapter

`LiveGraphExecution` borrows the Workspace-owned `ScriptJobQueue` and inspects only handles it submitted.
It never drains another consumer's completions.
The adapter retains no more than eight unreleased graph runs and discards a completed Python handle as soon as its strict result is consumed.

Submission validates the recipe input and graph parameters, captures the base compiled-family input, validates the typed proof recipe and axis coordinates, then starts the durable graph run and submits exact code to the shared Python queue.
A successful `ScriptRecipeResult` stages one `AgentEditRequest` against current Project state without committing it.
`CompileProofInput::with_staged_edit` overlays that guarded transaction onto the complete captured family for the derived proof while preserving every other source, axis, feature, kerning and component dependency.
The unchanged input and derived input are retained together with one identical typed proof recipe.

`take_proof_request` transfers a clone of that exact proof request once and moves the adapter into `ProofsRunning`.
The central proof bridge should submit `base_input` and `derived_input` to the existing process-wide compiled-proof service, retain at most eight paired handles and return their exact PNG, canonical-input and compiled-font identities through `publish_proofs`.
The bridge must cancel or discard its own handles on selected-run cancellation and must transfer any non-discardable running handle to the existing abandoned-handle set.
No second proof queue should be created.

`result_summary` exposes the retained bounded script report and diagnostics to the native canvas and agent status response.
`apply_request` is available only after both proofs complete and builds a separate `AgentEditRequest` from the retained guarded recipe result.
It does not commit directly or mint authorization.
The native Apply action or authorized agent boundary should pass that request to the common Workspace `agent_apply` receipt adapter, which remains the only owner of publication, retry identity and ordinary font Undo.

## Workspace ownership and pump seam

Workspace should own one `GraphSession` for the open graph and one `LiveGraphExecution` for its live run state.
The intended accessors are `Workspace::live_graph_session` and `Workspace::live_graph_session_mut` so canvas and MCP adapters address the same state without copying another graph representation.
Opening or creating a graph should construct the session with a host-generated graph ID, the current native document epoch and the actual registry including supported live node definitions.
Opening another graph or document replaces both session and execution adapter after cancelling or abandoning their owned queue handles.

The canvas should render `GraphSession::snapshot` and send committed code edits, parameter changes, connections and completed drags through `mutate_interactive`.
The agent adapter should call discovery, snapshot and receipt-backed mutation methods on that same session.
Saving should serialize only the snapshot graph, never run handles, receipts or captured font versions.

The existing application pump should call `LiveGraphExecution::poll_scripts`, dispatch each single-use proof request to the central proof bridge, inspect only the bridge's retained proof handles and publish terminal proof identities back through the adapter.
The pump should then invalidate the Nodes view from the current session and adapter status.
Changing a node or Project after submission must leave previous images visibly stale until an explicit rerun rather than replacing them silently.

The Workspace registration and proof-queue bridge remain coordinator-owned shared-file integration.
The current disk-workflow `run_nodes` path remains separate and must not be used to execute this live topology.

## Validation

The focused graph-session filter passed 17 tests.
The execution adapter passed two native Python process tests, including full-family staged overlay without root mutation and stale rejection after a Project change.
The combined `nodes_` filter passed 31 library tests, two binary adapter tests and four existing CLI workflow tests.

```text
CARGO_TARGET_DIR=/Users/eli/.codex/worktrees/5d82/runebender-xilem/target CARGO_BUILD_JOBS=2 RUNEBENDER_TEST_FONTS=/private/tmp/runebender-desktop-virtua-20260920/sources cargo test --locked nodes_ -- --test-threads=1
```

Strict workspace Clippy is required after the final shared integration.
The initial strict pass identified only the large graph error representation and led to boxing its diagnostics slice without changing its serialized shape.
The browser build, native Gray and Light screenshots and disposable Virtua end-to-end Apply and Undo scenario remain coordinator acceptance work after Workspace and proof-bridge integration.

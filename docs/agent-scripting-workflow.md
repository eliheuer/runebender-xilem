# Native script authoring and execution

Status: approved implementation direction, not completed functionality.
On 2026-09-20 the user approved chat-generated scripts, a persistent Scripts panel, in-editor editing and running, and initially exploring Python and Rust against the same system.
The user then explicitly narrowed this phase to Python only to keep it simple.
This document defines the bounded first delivery and the interface shared by the parallel workers.
The coordinator owns integration, scope changes and final acceptance.

## Product flow

Chat produces a named Python code artifact that can be opened and saved to Scripts.
Generating, opening or saving a script never executes it.
Scripts lists ordinary local files with a name, description and language.
The first library is a user-selected directory, outside font source packages; project-shared libraries can use the same directory abstraction later.
Code opens in an editor area large enough to work in, with explicit unsaved state, save-conflict handling and retained manual edits.
Assistant revisions are explicit replacements or diffs against a known script revision; streaming prose must not overwrite an edited buffer.
Run reads fresh explicit document scope and parameters and starts a background job.
A report shows bounded output without changing the font.
An editing recipe produces a preview of proposed changes; Apply publishes one guarded native edit with one undo group.
An error, cancellation, stale document or malformed result never applies a partial proposal.

The first examples list anchors and move existing named anchors by a parameterized offset.
The first mutation scope is selected glyphs in one explicitly chosen source and its exact layers.
Read-only reports may cover more glyphs within bounded capture limits.
The existing atomic adapter accepts at most 64 guarded layer entries and 256 operations in one source.
Reject larger or cross-source mutations with an actionable limit; do not split them into secretly partial batches.
Whole-font and all-master mutations require a later engine-backed atomic scope extension and remain tracked work.

## One language-neutral boundary

Rust Project remains the only live font model.
Python is an optional external interpreter.
It runs outside the editor process, receives no mutable Project pointer and introduces no second history implementation.
The editor does not require Python to start, load, edit, save or compile fonts.
Use a versioned JSON recipe contract and the ordinary guarded edit adapter.
The same recipe executable and input envelope must also run from a CLI harness.

The host captures an immutable, bounded input on the application thread from canonical reads.
The worker receives one JSON input on stdin, writes one JSON result to stdout and bounded human-readable diagnostics to stderr.
Python helpers can redirect ordinary print output to stderr while writing the structured result explicitly.
The input has schema_version 1, job_id, input_hash, explicit source, parameters and layers.
Each layer contains its existing AgentLayerGuard, width and an anchor list with stable id, optional name and x/y.
The host retains document epoch/revision and script-content/runtime identity with the job, rather than storing live bindings in reusable script files.
The input_hash binds the actual captured input including scope and parameters; the host also records the script content hash.
The result echoes schema_version, job_id and input_hash and contains a report plus optional reads and edits using AgentLayerGuard and AgentLayerEdits.
Workers must agree exact serialized names through the runner owner's typed definitions before expanding implementation.
Unknown fields, nonfinite numbers, extra frames, excessive output, invalid IDs, changed targets and oversized proposals reject.
The host verifies every returned target and dependency against the captured scope, then rechecks canonical revisions before Apply.
Recipes cannot mint authorization, choose a different document, broaden scope or replace operation identity through their output.
The host creates actor, operation_key, expected_document_epoch and authorization only for the explicit Apply action.
Receipts and normal editor undo remain the source of truth after dispatch or timeout.

This is a process boundary, not an operating-system sandbox.
Running user-authored code has the user's filesystem privileges unless a real sandbox is separately implemented.
Do not pass source font paths, private endpoints or model credentials as recipe inputs.
Use a temporary working directory, explicit executable arguments without a shell, bounded concurrency/logs/output, deadlines and child cleanup.
Document honest child-process cancellation limits and do not advertise descendant containment without evidence.
Native execution is unavailable in the browser; the shared view must present that limit and continue to build.

## Python-only delivery

Python is the first complete convenience path for short reports and anchor recipes.
The interpreter is explicitly selected or discovered, with a useful unavailable state and no automatic install.
The Python helper consumes immutable JSON and produces proposals; it is not an editable font wrapper.
Do not implement, prototype or schedule Rust scripting in this phase.
The neutral protocol can support another client later without a second editing architecture.
Do not add an embedded Python runtime or automatic dependency downloads.
An absent interpreter must not disable ordinary editor use.

## Parallel ownership

The runner worker owns new document/script_recipe.rs, application/platform/script_jobs.rs and script_library.rs, their module registrations and adjacent tests.
It publishes the typed input/output and library/job API early and does not edit Workspace, chat, views or the live cancellation dispatcher.
The UI worker owns application/editor/tools/scripts.rs, view/panels/scripts.rs, chat artifact presentation/state, Workspace and minimal view/host registration.
It consumes the runner API, keeps font changes in the existing application edit path, and does not create a second process runner or library store.
The examples worker owns scripts/recipes/, client examples and recipe conformance tests, including Python anchor examples and subprocess failure conformance.
It does not change Rust application code, credentials, global tool installations or external model trials.
Shared integration edits must be coordinated before touching another worker's files.
Each worker commits only its bounded phase, reports exact tests and limitations, and removes its continuation when ready for review.
The coordinator reviews and integrates completed work, runs combined gates, pushes coherent phases to main as authorized and archives integrated workers.

## Acceptance and remaining work

- [ ] Chat code artifact survives streaming completion and opens without executing.
- [ ] Save, reopen, rename and manual code edits persist; external changes cannot silently overwrite a draft.
- [ ] Anchor report includes exact glyph/source/layer/name/position and leaves document history unchanged.
- [ ] Parameterized anchor move produces a reviewable preview and one Apply/Undo group.
- [ ] Changing the font, source, selection, script or parameters cannot silently reuse an obsolete preview.
- [ ] Exceptions, timeout, cancel, malformed output and oversized results leave font and source files unchanged.
- [ ] Python examples produce deterministic proposals from the same captured input.
- [ ] Python exceptions and a missing interpreter have honest UI/report behavior.
- [ ] Native Gray/Light headless captures and browser regression checks pass after integration.
- [ ] Disposable Virtua application-adapter trial preserves original files and validates ordinary undo.

This delivery does not close the broader variable/multilingual context, cross-source transactions, full client/platform matrix or remote transport work in agent-interface-plan.md.
Actual model image interpretation and native foreground pointer/IME acceptance remain separate evidence.
The latest user approval releases implementation and disposable local recipe trials; it does not authorize bypassing the earlier automatic review block on credentialed OMP image submissions or interrupting the desktop with a foreground GUI.

## Requested Nodes comparison workflow

The user next requested a ComfyUI-like graph with a base font branching into an unchanged specimen and a Python-transformed specimen.
The Python node should show editable code inside its box, and image output nodes should be movable side by side for comparison.
This is a requested next product phase, not an implemented capability or an instruction to duplicate the current workers.

The intended graph is captured base FontVersion → specimen A, and the same captured FontVersion → Python script → derived FontVersion → specimen B.
Run captures one immutable baseline shared by both paths, including unsaved editor changes.
The script produces a guarded proposal that the host stages into a derived version, without changing its input or the open document.
Applying a chosen result to the open document is a separate explicit operation using canonical transactions, conflict checks, receipts and ordinary undo.
The script runner and result validator are the same ones used by the Scripts panel; the graph changes where the proposal is evaluated, not who owns font mutations.

The graph's font wire is a typed handle to a canonical Rust-owned font version.
Babelfont-backed geometry stays behind the Project adapters, including exact metadata and stable source/layer identities.
Python receives immutable scoped input and returns operations; it does not receive a shared mutable Python Babelfont object or the original font path.
The public graph should remain variable-family aware even though the first mutation adapter is limited to one explicit source.
Current experiment versions represent a stable source, so a full-family compiled specimen needs a canonical family overlay that preserves every other source and dependency.
Do not label a source-only outline grid as a fully compiled variable-font proof.

Use the existing compiled-proof and Designbot path first.
Both specimen nodes share explicit text, size, features, language/direction, variation location and renderer identity when doing A/B comparison.
Keep captured font/script/parameter hashes with the displayed images and mark obsolete results stale until rerun.
Late completions cannot replace results from newer graph runs.
Retain previous images during recomputation with their state visible.
An image output can be embedded in a resizable draggable node with zoom/open controls.
The script node uses a real multiline editor with proper focus, selection, clipboard and code undo, plus an expand action using the same buffer as the Scripts editor.
Editing code must not drag the node or trigger a graph run automatically.
Loading a saved script copies a known revision into the graph; updating the library or refreshing from it is explicit.
Saved graphs retain script text, parameters, connections and layout, but never stale session handles or implicit permission to execute.

DrawBot is a possible later renderer adapter using a temporary compiled font artifact and explicit rendering recipe.
It is not a required dependency for the first Nodes comparison loop and is distinct from the existing Designbot adapter.
Renderer differences must remain visible rather than being mistaken for script-induced font changes.

The current engine declares live.font, live.fork, live.proof and live.apply, but native editor/tools/nodes.rs removes live types from the palette and rejects their execution through the disk runner.
The custom native Nodes canvas currently has no registered child widgets for inline editing.
Therefore this phase requires a native live-graph scheduler, derived-version recipe adapter, full-family proof overlay where needed, and real code/image node content; it is not just adding a node label.
Keep this phase queued behind the shared runner contract and current review, then split engine/scheduler and canvas/editor work with explicit ownership.

Acceptance must show the original branch unchanged, a visible scripted anchor difference in a suitable mark-attachment specimen, identical proof settings, correct stale/cancel behavior, graph save/reopen without live bindings, and an explicit Apply/Undo that changes only the chosen target.

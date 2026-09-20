# Native Nodes, Python recipes and agent access

Status: integrated native testing candidate on 2026-09-20; remaining acceptance below is still open.
This extends the [Python workflow](agent-scripting-workflow.md) and [agent acceptance plan](agent-interface-plan.md).
The user asked for a coordinated Nodes pass, scheduled workers and lessons from ComfyUI while keeping the architecture small and organized.
Python remains the only user scripting language in this phase.

## Current native boundary

The native Workspace owns one guarded GraphSession, shared by the canvas and nine live Nodes tools.
The native runner captures one full-family baseline and stages validated Python edits into a derived family before compiling both specimen outputs.
Real multiline TextArea children and immutable PNG children are integrated, including guarded code edits, presentation-only moves/resizing, Run/Cancel/Clear and separate Apply.
The same shared Python queue, compiler queue, Project transaction boundary and ordinary font history serve the UI and agent calls.
The disk workflow runner remains separate and cannot silently execute a live graph.

At source checkpoint `edb5e3e`, 932 native tests passed with four ignored, strict all-target Clippy passed, and warnings-denied documentation passed.
An actual stdio MCP trial on a disposable full Virtua Grotesk designspace used every Nodes tool, verified exact PNG bytes and captured hashes, applied a 100-unit A width change, retried it, and undid it through ordinary Workspace history.
All 1,744 family files remained unchanged in both input and copy, including after process cleanup.
Evidence is `/private/tmp/runebender-noon-candidate/transport-2/virtua-mcp/evidence.json`; this is transport and application evidence, not model interpretation or foreground interaction.

Live comparison save/reopen passes native persistence tests, with source binding chosen explicitly at Open and no persisted session authority.
Foreground file-picker interaction remains part of the UI/UX pass.
Expansion into the shared Scripts editor and exposed graph Undo/Redo controls remain unfinished.
The graph engine has guarded Undo/Redo; the shared source editor now implements bounded local text history independently from graph and font history.
Native Gray/Light visual cleanup and the final combined browser/build checks remain in progress.

## ComfyUI evidence and lessons

This is a focused public-documentation/source-guidance review, not an installation or a complete ComfyUI code audit.
The following observations were checked on 2026-09-20.

| Observed behavior | Decision for Runebender |
|---|---|
| Official local MCP discovers installed node definitions, validates workflows and submits/monitors jobs through comfy-cli. | Discover the actual native registry; use one engine contract from UI, CLI and MCP. |
| The MCP repository explicitly keeps product behavior in the CLI rather than reimplementing it in the MCP wrapper. | Keep transport adapters thin; font and graph semantics stay in Rust modules. |
| The server validates a prompt before queueing and returns an execution identity and node-specific errors. | Separate validation from Run; return typed errors addressing node, port and field, plus a bounded run handle. |
| WebSocket messages distinguish progress, cached nodes, interruption, failure and overall success; executed specifically describes a UI output update. | Define explicit node/run terminal states independently of whether a node paints an image. |
| The custom-node guide warns that nodes requiring direct client/server interaction cannot be used through its API. | Every supported execution operation must work headlessly; canvas widgets are views over the same state. |

Sources: [official MCP tools](https://docs.comfy.org/agent-tools/mcp), [local MCP architecture guidance](https://github.com/Comfy-Org/comfy-mcp/blob/main/AGENTS.md), [server routes](https://docs.comfy.org/development/comfyui-server/comms_routes), [execution messages](https://docs.comfy.org/development/comfyui-server/comms_messages), and [custom-node API limits](https://docs.comfy.org/custom-nodes/overview).
These sources support specific design lessons, not a blanket conclusion that the entire ComfyUI architecture is poor.
Runebender does not integrate the ComfyUI runtime, install ComfyUI packages or copy its plugin ecosystem.
Avoid an extensible marketplace, arbitrary custom routes, automatic package installation, dynamic graph expansion and duplicate workflow representations in this delivery.

## One graph and execution contract

Keep NodeGraph as the canonical editable and persisted graph representation.
An internal validated execution plan may omit layout, but is derived from that same graph and is never a separately authored API workflow.
Graph session identity, graph semantic revision, document epoch and captured font revision are explicit and independent.
Opening another document or graph cannot silently retarget an existing job or agent request.
Node IDs remain stable during edits; inputs/outputs are discovered from the actual registry.
Maintain a semantic content hash separately from layout changes so moving a preview does not rerun Python or invalidate a font result.
Graph authoring changes must be atomic, revision-guarded and undoable without executing the graph.
Code-editor undo, graph undo and font undo have explicit focus/command routing.

Run starts from one immutable canonical font capture for both comparison branches.
The first supported topology is base → proof and base → Python recipe → derived version → proof.
Python output is validated using ScriptRecipeResult, then materialized on an isolated derived version by Rust.
Root font edits and disk installs are not traversal side effects: the live runner rejects effectful install/apply nodes as automatic execution steps.
Applying a selected derived result is a separate application command using the existing font authorization, epoch checks, receipt/retry and ordinary undo path.
No request or script output invents authorization for a different scope.

Use bounded graph size, queue, result retention and logs with explicit stale, failed, cancelled, completed and unavailable states.
A run binds graph semantics, script content, parameters, exact font input and proof recipe.
Changing an upstream input invalidates dependent outputs; dragging a node does not.
Prefer correctness over elaborate caching in the first implementation.
A deterministic recipe may reuse results only with a complete key; nondeterministic work cannot be treated as deterministic merely because the same code was submitted.
Late completion cannot overwrite a newer run's output.
Cancel affects the selected run only, never another graph or unrelated editor operation.
Poll stable job status/results before adding another subscription protocol.

The canonical model remains variable-family aware; the initial Python mutation scope is one explicit source within existing atomic limits.
A derived family's compiled proof must preserve all other sources, features, kerning and component dependencies through a canonical overlay.
That overlay passes the source and midpoint mark-attachment scenario in `/private/tmp/runebender-nodes-virtua-anchor-20260920-1250/evidence.json`.
A 100-unit Regular anchor edit moves the noncomposing mark by 100 units at the source and 50 at the midpoint, with unchanged original files and reversible Apply.
The existing compiled proof worker and Designbot adapter remain the first rendering path.
Proof results carry the same revision/hash/recipe metadata to the canvas and agent image response.

## Agent integration owned by the coordinator

Implement a small live graph service shared with the UI, then expose it through the existing native mailbox/CLI/MCP adapters.
Do not add an agent-only graph engine or use screenshots as the graph editing protocol.
The intended capabilities are registry/context discovery, graph read, guarded patch/validate, explicit run, job status/cancel, exact result/image retrieval and separate selected-result Apply.
Exact wire names remain provisional until the engine worker publishes its typed API.
Graph authoring and execution are distinct permissions and actions; editing code or loading a graph never runs it.
Mutating graph requests require exact graph/document identity, expected revision and bounded operation-key retry identity.
Human edits between read and patch reject stale agent changes rather than replacing the full graph.
An agent sees node-specific structured errors and the same current/stale output displayed by the user.
Applying font changes continues to use the existing font receipt and history semantics.
The first scripted agent scenario constructs the bounded comparison graph, changes one parameter, validates/runs, retrieves both images, then explicitly applies and undoes the chosen result on disposable sources.

## Parallel implementation ownership

The live-graph engine worker owns document/nodes_live.rs, new document/nodes_session.rs, graph type additions in document/nodes.rs and the native run adapter in application/editor/tools/nodes.rs or a focused sibling module.
It publishes data-only graph command/run/output interfaces early.
It coordinates minimal module/Workspace registration before touching shared files.
The existing script runtime worker retains script_recipe.rs, script_jobs.rs and script_library.rs; the existing Scripts UI worker retains script buffers, chat and their Workspace fields.
The Nodes canvas worker owns application/view/canvas/nodes.rs, application/view/panels/nodes.rs and layout/hit-testing in ui/nodes.rs.
It consumes engine status and script buffer interfaces and does not implement process execution, font mutation or a competing script store.
The coordinator owns live graph agent adapters, compiled family overlay integration, shared registrations, final review/acceptance and main promotion.
Workers must ask for a coordinated seam change rather than independently editing each other's modules.

## Bounded first acceptance

- [x] One captured base version feeds unchanged and scripted specimen outputs with identical rendering settings.
- [ ] Code is visible and editable in the Python node with correct focus, selection, clipboard, multiline input and expansion into the shared editor.
- [ ] Image nodes can be dragged/resized beside each other without rerunning or changing the font.
- [x] Native and agent calls discover/read/patch the same graph and reject stale patches.
- [ ] Script exceptions, malformed output, cancellation and document replacement leave the original version and disk sources unchanged.
- [x] Source-scoped anchor changes are visible in an appropriate mark-attachment specimen with truthful compiled-family lineage.
- [ ] Current/stale/error state matches across node previews, job status and MCP images.
- [x] Explicit Apply yields one existing receipt/history group and ordinary Undo restores the selected change.
- [ ] Graph save/reopen preserves code, settings and positions without persisting session handles or auto-running.
- [ ] Native Gray/Light headless evidence, browser regression checks and a disposable real-font scenario pass.

Full native pointer/IME/platform acceptance and actual model image interpretation remain separate from unit/process tests.
This phase does not promise every old disk workflow node can operate on a live font version.

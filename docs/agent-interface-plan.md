# Implementation gates and acceptance plan

This plan accompanies the [research report](agent-interface-research.md) and [client matrix](agent-client-matrix.md).
The [coordination record](agent-interface-coordination.md) assigns the active implementation work.
The original research phase changed documentation only.
Implementation began after the explicit migration release recorded below.
Milestone 1 remains incomplete; its first context and identity checkpoint is implemented and validated.

## Gate 0: explicit migration release

- [x] Receive an explicit release from orchestration task `01a0b6c4-c0a7-7951-917f-7ed2ccda4428` with the exact final main commit.
- [x] Verify that commit is present on `/Users/eli/GH/repos/runebender-xilem` main and upstream, and review the migration's final validation evidence.
- [x] Inventory the research worktree and preserve unrelated changes before moving its base forward through an appropriate isolated branch.
- [x] Re-read the final `ARCHITECTURE.md`, relevant module headers, `DESIGN.md`, and browser boundary.
- [x] Re-audit canonical identity lifetime, snapshots, source/layer transactions, history, compile jobs and wire schemas.
- [x] Recheck the refresh-field mismatch and experiment proof revision finding; orchestration has already routed them to the migration owner.
  Do not duplicate their fixes in this task.
- [x] Record a revised implementation map against the released commit, including which audit gaps migration has already closed.

A testing checkpoint, a passing partial suite, a quiet migration task or elapsed time is not this gate.
Research readiness is not permission to prototype, add dependencies or start production changes.

## Released baseline and first implementation checkpoint

Orchestration released final main `e40bd4ce338cb8270f2356a515946e52d2b6b21b` explicitly.
Local main and GitHub main matched that commit, and its tree matched documentation completion `d864e211d73f134e53094928e496a8b4c97ee690`.
The [migration final proof](babelfont-migration-final-proof.md) records the migration validation.
The research branch incorporated it through merge `dbf2072`; main was not changed.

The migrated code already fixes stable-source refresh routing and branch kerning proof revisions.
It also removes the production whole-source projections described in the historical audit.
The stable component-selection history regression coverage remains intact.
The implementation keeps canonical `Project` authoritative and uses its existing revision, stable identities, layer transactions, and history.

The first checkpoint adds `editor_context` in the application adapter and exposes stable glyph/contour/point/component/anchor identities in canonical reads.
The Unix mailbox supplies a document epoch and rejects a mismatched optional `expected_document_epoch` before dispatch.
The context includes active source/layer/glyph, selected object IDs, tab, tool, text/features, user axis values and gesture state.
Its `context_revision` is a content hash, not a monotonic history counter; returning to identical context within the same document epoch can reproduce the hash.
Caret/selection ranges owned by the text widget remain explicitly unavailable, and there is no arbitrary auxiliary-layer canvas selector.

Live schemas no longer advertise disk `master` indices; live calls reject that field and expose `source_id` on experiment results too.
Canonical live results include `document_revision` and `saved=false`.
The socket envelope reports schema version 1 and the package version; this is not an executable hash.
MCP input is bounded to 8 MiB and initialization chooses a supported version instead of echoing an arbitrary version.
The native application now exposes strict receipt-backed atomic apply and shared ordinary/targeted grouped history through `agent_apply`, `agent_receipt` and `agent_history`.
Full validation of legacy schemas remains pending; independent edit cancellation is implemented and covered by socket and commit-race tests.
Asynchronous compiled proof artifacts and direct MCP PNG delivery are implemented; actual model image receipt remains a separate gate.
The engine transaction/proof primitives, real OMP proposal trials and the desktop receipt-backed edit trial are implemented.
See [the live context wire notes](agent-live-context.md) for the implemented contract and its limits.

## Milestone 1: one correct live editing loop

Bound the first deliverable to local Unix native editing through the existing stdio adapter, on disposable fonts.
Use one source and a small set of existing glyph operations, while making every identity and result explicit enough to extend later.
The designer can keep editing; the agent can inspect an unsaved change, propose a width/point/anchor adjustment, see before/after compiled proofs, apply within existing authorization, and undo.
It must detect a conflict and resolve timeout ambiguity without duplicate edits.

### 1A. Coherent source/context and protocol envelope

- [ ] Replace stale master/index assumptions in the live wire boundary with explicit stable identities and uniform result fields.
- [ ] Add document epoch, document/context revision, capabilities, limits and server build/schema identity.
- [x] Capture a minimal coherent UI context: source, layer, active glyph/selection, text/features/location, and busy gesture state.
- [x] Keep context in the application adapter; toolkit-independent target/edit types belong in the font engine.
- [ ] Negotiate supported MCP versions, bound input, validate schemas server-side, and produce structured actionable errors.
- [ ] Update setup documentation from actual generated schemas; keep disk and live examples visibly distinct.

Likely files after re-audit: `document/live.rs`, `document/agent.rs`, `document/live_socket.rs`, `application/platform/live.rs`, `application/cli.rs`, and `application/workspace.rs`.
Prefer a small session module only if combining lifecycle responsibilities in `live.rs` would obscure the existing font operations.

Acceptance: two open documents have different epochs; a client's binding never changes because the other window becomes active.
An unsaved 12-unit width change appears in a read while disk bytes remain identical.
Reorder sources and change selection between read and mutation; the request still targets its explicit source/object or rejects a stale precondition.

### 1B. A narrow atomic transaction and receipt

- [x] Stage supported operations with canonical drafts and check all preconditions before mutation.
- [x] Publish one guaranteed atomic bounded batch with one named history group; reject unsupported scope before editing.
- [x] Return actor, operation ID, before/after revisions, changed IDs, history handle and `saved=false`.
- [x] Add a bounded session receipt ledger with idempotency payload checks and status queries.
- [x] Reconcile timeout/disconnect by status; implement cancellation before commit and explicit too-late/committed outcomes.
- [x] Carry already-granted scoped authorization through the sequence; do not ask repeatedly for the same authorized edit.
- [x] Make targeted undo conflict-aware and integrate with ordinary editor undo so one operation cannot be undone twice.

The [live transaction contract](agent-live-transactions.md) documents the implemented boundary.
Real-socket tests cover a disconnected apply caller, exact receipt replay without duplicate refresh/history, stale dependencies, point/anchor identities, grouped history conflicts, source reorder, inactive-source refresh, overview ordering and active gestures.
Independent connection and commit-race tests cover cancellation; current receipts expose immutable changed layer addresses and changed width/point/anchor identities derived from final canonical deltas.

Likely owners: canonical draft/history APIs, `edit_batch.rs`, `proposal.rs`, `experiments.rs`, plus the session adapter.
Do not describe existing per-glyph installs as atomic or rebuild their geometry in an adapter.

Acceptance: invalid operation three leaves all targets, revisions, histories and dirty flags unchanged.
Disconnect after commit but before response; retrying the same key yields the original receipt and one undo item.
Changing payload under the same key rejects.
A later human edit blocks overlapping targeted undo but unrelated edits survive.
Restart the editor and report old receipts as unknown under the new epoch rather than replaying.

### 1C. One compiled proof lineage

- [ ] Expose an immutable compiled snapshot handle with font-byte hash, document/version identity, compiler digest and recipe.
- [x] Use existing variable compiler/shaper/outlines for agent proofing; retain source-grid proof separately.
- [x] Produce actual PNG content plus glyph IDs/names, clusters, advances and offsets from the same snapshot.
- [x] Make rendering/compilation asynchronous where required so the UI thread only captures and commits.
- [x] Keep source-only branch proof limits explicit until a full canonical family overlay exists.
- [x] Validate before/after source edits in a variable family at source and midpoint locations where supported by the first implementation.

Acceptance: a changed unsaved advance/anchor appears in shaping and image from the same bytes as export.
An older compilation finishing late cannot overwrite current proof identity.
A failed compile gives an error and no falsely current image.
An agent with a vision-capable client receives the PNG and identifies a visual-only synthetic marker.

The full-family Virtua anchor trial at `/private/tmp/runebender-nodes-virtua-anchor-20260920-1250/evidence.json` validates a 100-unit Regular anchor change at the source and a 50-unit change at the midpoint, in both shaped offsets and returned PNGs.
It also verifies ordinary Undo, immutable retry receipts and unchanged source files.
This closes the source/midpoint scenario only; reusable compiled-font handles and actual model interpretation remain open.

### 1D. Real client end to end

The [OMP client trials](agent-client-trials.md) now pass real model read and bounded edit calls against the synthetic Workspace, including fixture-driven ordinary undo/redo.
The Codex desktop task now passes a real Virtua Grotesk copy trial, including receipt lookup and exact retry, with independent ordinary application undo/redo.
Compiled image transport now passes a real socket and stdio MCP process test.
Actual model image receipt and OMP trials of the new receipt tools remain pending.

- [ ] Run the bounded scenario through a local Codex/ChatGPT desktop task and OMP CLI with disposable fixtures and isolated configuration, as selected by the user.
- [ ] Save redacted transport transcript, receipts, binary/client hashes, images and disk manifests.
- [x] Test via the real document/UI adapter; an in-process `live::call` test alone is insufficient.
  The native Workspace-backed desktop trial and file-backed host tests cover application dispatch, cache refresh and history; native pointer/IME behavior remains separate.
- [ ] If a client is unavailable or needs interactive login, report it as not tested and retain the ready harness instead of claiming success.

Milestone 1 is complete only when the unsaved-edit → read → proposal → proof → authorized apply → UI update → undo chain works, plus a stale-write and disconnect-after-commit case.
This is a bounded first working milestone, not a promise of full multi-client or aesthetic capability overnight.

## Milestone 2: complete variable and multilingual context

- [ ] Extend stable glyph/layer/point identity addressing through rename, auxiliary layers, intermediate layers and source changes.
- [ ] Include component dependency and read-set revisions in proposals.
- [ ] Build a canonical family snapshot overlay for experiments, retaining other sources and source-specific metadata.
- [ ] Add explicit linked structural transactions only with validated correspondence; do not infer point correspondence from equal indices.
- [ ] Expose script/language, bidi runs, glyph tokens, caret ranges, feature choices and user/design/normalized axis coordinates.
- [ ] Add bounded `changes_since` polling with overflow/resync and cache invalidation, then optional subscriptions.
- [ ] Complete atomic cross-source/metadata groups and redo semantics with failure injection.

Acceptance: Latin, Hebrew and Arabic proofs across Regular/midpoint/Bold agree with compiled export and retain exact source metadata after an authorized save/reopen test.
Source deletion, changed feature text, changed component base, or renamed glyph invalidates dependent work explicitly.
Two agents making disjoint edits can succeed; overlapping or dependency-conflicting edits do not silently merge.

## Milestone 3: repeatable procedures and local jobs

The user approved the [native scripting workflow](agent-scripting-workflow.md) on 2026-09-20, then explicitly narrowed implementation to Python only.
The first product delivery is chat artifact → saved script → report or preview → guarded Apply → ordinary Undo.
Three isolated workers own runtime/library, native chat/Scripts UI and Python anchor examples respectively.
This releases implementation and disposable recipe trials; it does not imply that these features already exist or that the broader milestones are complete.

- [x] Add a thin live Python example using session/schema/receipt APIs, with no editable font wrapper or duplicated geometry.
  `scripts/live_spacing.py` prepares an explicit guarded width batch and supports apply, receipt reconciliation and undo without source-file edits.
- [ ] Make deterministic operation recipes reusable from CLI and nodes with explicit parameters and inputs.
- [ ] Connect live snapshot exports to existing isolated model workers, preserving the no-live-root-path boundary.
- [ ] Add job queue/status, progress, cancellation, timeout, bounded logs and worker exit classification.
- [ ] Record model/runtime/executable hashes, input hashes, seed/settings, operation receipts and proof recipes.
- [ ] Reuse experiment/Nodes UI and preserve recipe persistence without stale session bindings.
- [ ] Define optional durable recovery separately from saving the font; test journal crash points before advertising recovery.

Acceptance: kill or cancel a worker mid-job; root and source files remain unchanged, the job reaches an honest terminal state, and no partial artifact is installed.
A job finishing after a relevant designer edit produces a stale proposal rather than an overwrite.
Replay a deterministic recipe and compare canonical deltas; rerunning inference is labeled probabilistic even when a seed is present.

## Milestone 4: remaining clients and transport boundaries

- [ ] Run Claude Code, Codex CLI/local desktop, OMP, Pi CLI, Pi with adapter, and a locally installed model through the capability matrix.
- [ ] Test actual model image delivery independently of tool transport and text rendering.
- [ ] Evaluate Secure MCP Tunnel with ChatGPT on a restricted disposable session before deciding whether a public HTTP gateway is necessary.
- [ ] Implement a generic remote gateway only with document-sharing scope, authentication, origin validation, bounded resources and reconnect tests.
- [ ] Implement browser-to-WASM-Project routing independently of native IPC; test tab close/reload and pending operation behavior.
- [ ] Add a Windows transport only with Windows runtime evidence.

Acceptance: revoking a remote document grant stops new calls, reconnecting does not broaden it, and another user's/session's receipt cannot be queried.
A browser agent reads an unsaved WASM edit from the actual tab; a stale tab handle fails without switching to server or disk data.
An unsupported platform or client capability remains visibly unsupported.

## Disposable-font acceptance harness

The harness is a planned implementation deliverable; no harness code or test fonts were created during this research phase.
Use small synthetic UFO/Designspace fixtures for precise assertions and a private copied multilingual family for realistic geometry and performance.
Never point mutating tests at the real Virtua sources.
Record fixture provenance and hashes, and avoid committing restricted third-party fonts.

The fixture set should contain at least two masters, a mapped axis, an intermediate glyph source, an auxiliary layer, a component chain, fractional kerning, groups, features, image/data resources and arbitrary preserved lib data.
Include stable UFO identifiers, contour/point metadata, nontrivial component transforms and unusual but valid values to catch lossy projection.
Before live work, hash every source file; every no-save scenario must leave the whole disk manifest unchanged.
In explicit save/reopen scenarios, compare untouched files byte-for-byte when preservation promises it, and compare complete structured values for touched serializations.
Distinguish semantic preservation from byte preservation; report a serializer's documented normalization rather than claiming impossible byte equality.

| Scenario | Action | Required evidence |
|---|---|---|
| Unsaved state | Change n in the editor, read via a real client | Live value differs from disk; exact session/source; no file changes |
| Layer scope | Edit an auxiliary layer while foreground and intermediate source exist | Correct layer IDs; no neighboring layer mutation |
| Source stability | Reorder/remove sources after reading | Surviving IDs keep meaning; removed target rejects |
| Selection drift | Select different points after an agent captures context | Explicit original target or stale rejection, never the new selection by accident |
| Gesture boundary | Send edit while pointer drag is in progress | Busy/committed-snapshot semantics; no split gesture/history corruption |
| Batch failure | Invalid/nonfinite/stale final operation | Zero publication, unchanged revision/history/dirty state |
| Internal failure | Inject failure at each publication/history boundary | Complete rollback or explicit recovery state; no false atomic success |
| Two agents | Concurrent disjoint and overlapping writes | Disjoint allowed when dependencies match; conflicting set rejected |
| Read dependency | Modify a component base/reference glyph during proposal work | Stale dependency detected even if target GLIF did not change |
| Timeout ambiguity | Drop response immediately after commit | One receipt, one mutation, one undo item on retry |
| Cancellation | Cancel queued work, running worker and post-commit request | Distinct terminal outcomes; no claim that post-commit cancellation undid work |
| Editor death | Kill before commit, after commit, and during proof | New epoch; no silent replay; stale artifacts labeled; recovery limit explicit |
| Undo/redo | Agent batch, designer edit, targeted undo and normal undo/redo | Unrelated edit retained; overlapping conflict; no double reversal |
| Latin | `AVATAR office` with kern/ligature toggles | IDs, clusters, advances, proof and font hash agree |
| Hebrew | `שָׁלוֹם` plus mixed Latin/digits/punctuation | Explicit Hebrew script/language, RTL runs, mark attachment and selection ranges |
| Arabic | `سلام` and marked joining samples | Joining/ligatures, mark offsets, Arabic language setting, text-size and display proofs |
| Variation | Each text at both masters and midpoint | Same compiled bytes for all locations; proper mapping, outlines and HVAR/GPOS |
| Feature failure | Introduce invalid feature draft on the copy | Clear compile failure; root/published features unchanged where draft-only |
| Structure | Propose incompatible single-source topology | Policy rejection or explicit incompatibility; no implicit linked edits |
| Preservation | Edit width/anchor only; later save and reopen copy | Exact untouched values/resources/layers, deterministic diff and recovery |
| Local worker | Crash, timeout, malformed output, changed input | No root write; bounded diagnostics; stale or failed job; output quarantined |
| Large context | Inventory thousands of glyphs and repeated small reads | Bounded pagination; measured latency/tokens; no truncated JSON treated as complete |
| Image delivery | Place a random visual marker absent from text output | Client/model identifies image-only fact; missing images reported honestly |
| Remote scope | Denied target, revoked grant, reconnect, invalid origin | Correct authorization failure, no alternate filesystem route |

Run transport fault injection against a real socket/adapter process, not only mocked function calls.
Run headless application integration to cover UI-cache/history invalidation and Gray/Light captures after idle auto-hide when UI changes.
Native pointer/IME/GPU interaction remains a separate manual gate requiring the user's agreement before foreground interruption.

## Validation and acceptance reporting

After implementation, run the repository's required checks at the released toolchain: formatting, copyright, strict locked Clippy, docs, serial workspace tests, release build and dependency advisories.
Run `web/build.sh` and the documented browser Clippy/quality checks whenever shared sources change.
Use a clean temporary checkout for the final reproducibility proof, with no local path patches.
Record command, commit, fixture manifest, platform, outcome and known limits; an ignored test is not passing runtime coverage.

Keep three separate verdicts:

1. **Transport:** the intended client connected, discovered schemas, received values/images and recovered from errors.
2. **Editing correctness:** the correct unsaved document changed exactly once, preserved source data, detected conflicts and undid correctly.
3. **Type-design quality:** a qualified reviewer finds the result useful at relevant sizes, strings, scripts and interpolation locations.

For quality trials, use fixed briefs, approved reference glyphs, baseline proofs and blinded before/after comparisons where practical.
Compare Astra and the user's local model on the same bounded task and log time, context, proposals, revisions and unresolved observations.
Do not auto-approve a design because tool calls or font validation passed.
A poor optical decision with perfect transaction behavior is a successful protocol test and a failed design result.

## Research completion record

The architecture, matrix, source audit and staged scenarios are complete as a research deliverable.
No implementation milestone is complete.
At research completion, no client connection, model-quality result or new runtime test was claimed.
Implementation validation belongs to the checkpoint record above and the live context wire notes.
The explicit final-main release and re-audit are recorded above.
The next implementation work is the remainder of Milestone 1; research evidence remains historical, and transport tests do not establish model design quality.

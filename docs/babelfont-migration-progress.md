# Babelfont migration progress

Status: **IN PROGRESS — M00 complete; paused before M01 for main promotion**.
The definition of complete and milestone dependencies remain in [the checklist](babelfont-migration-checklist.md).
No model ownership has changed yet.

## Continuation checkout

- Worktree: `/Users/eli/.codex/worktrees/790d/runebender-xilem`.
- Branch: `codex/babelfont-migration`.
- Baseline: `624879a1c447d3e9f012c34f4b5cb091bb0df6cb`.
- Setup verified that clean starting commit `5c37be7717e780aac3fcb369b033148f57026ef9` was an ancestor, created the isolated branch, and fast-forwarded it to the exact baseline.
- The first implementation run started clean; main and the originating task's branch were not changed.
- The checklist contains 15 milestones and 76 acceptance steps.

## M00 — Establish the continuation and measurable baseline

Run date: 2026-09-18 America/Los_Angeles (2026-09-19 UTC).
Evidence commit: `Record the Babelfont migration baseline and behavior contracts` (the commit introducing this file).
Resolve its exact ID with `git log --diff-filter=A --format=%H -- docs/babelfont-migration-progress.md`; this avoids a self-referential commit hash.
There is no model implementation commit in M00.
Affected paths: this log, the checklist and [the reviewed inventory](babelfont-migration-inventory.md).
All production callers remain on the baseline implementation.

Read AGENTS, ARCHITECTURE, DESIGN, the variable-project decision, the checklist and `web/README.md`.
Read the [Linebender formatting scheme](https://linebender.org/wiki/formatting-scheme/) before writing documentation; prose uses one sentence per source line.
The reported 569 passed / four ignored full native suite is historical evidence from the preceding task, not a result executed in this checkout.
The four ignored tests do not provide runtime coverage.
No full native gate, browser build, visual proof or performance claim is made for M00.

### Inventory method

Ran `rg -l '\bnorad\b' src tests examples web --glob '*.rs'` and inspected production signatures, mutation guards, histories and their callers.
The inventory distinguishes live model use, codecs, tests, comments and candidate obsolete compatibility methods.
It also follows callers without a literal Norad import; a text-count reduction is not migration completion.
Each runtime family has a migration owner, including `ui/theme.rs` and live operations currently under `formats/`.
The reviewed inventory is versioned evidence; raw search output is in `/tmp/runebender-babelfont-migration-m00/norad-occurrences.txt` and `compatibility-callers.txt` for this run only.

### Behavior to preserve

| Family | Baseline behavior and inspected implementation | Acceptance evidence |
|---|---|---|
| Drag grouping | `Session::record(Drag)` queues the pre-edit glyph only when entering a gesture; `DragUp` closes it. Point movement uses positions captured at gesture start. Anchor, component and advance drags share the grouping mechanism. `sync_session_from` drains pending records into the source's per-glyph history and clears metadata redo on a new record. | Session tests for anchor/metric transactions, component editing and source history; `document::history` tests. |
| No-op history | A failed knife cut removes its pending record, or emits `DiscardLast` if the record was already drained. Parameterized filters record only real changes. This is not a claim that every no-op gesture already suppresses history: `begin_point_drag` records before movement. | Session filter tests and history discard test; code inspection of `knife_cut` and `begin_point_drag`. |
| Auxiliary history | `Project::edit_layer` clones the target, rejects unchanged payloads and renames, and records default-layer changes in the source history. Other layers use `VariableData.histories` keyed by `LayerId`, with per-glyph stacks. `undo_layer` replays without switching the active source. | `layer_edits_and_history_round_trip_all_source_data` and `guarded_legacy_edits_commit_to_canonical_layers_before_save`. |
| Metadata history | Rename, Unicode and font-data history are separate workspace stacks. Replay requires the selected glyph name and current glyph undo depth to match the recorded context. Unicode/groups/kerning/features snapshots carry source IDs and are reordered against current IDs; a changed source set rejects replay. Rename preserves the glyph's history name. | Inspector metadata tests and host cross-master/source-reorder tests. |
| Source undo | Structural history captures masters, variable data, source names/locations, brace sources, Designspace and active selection. Undo/redo compares expected font contents, paths, source names/locations and Designspace; a mismatch restores the popped step and rejects replay. Restore retains the larger source-ID counter. Removal retains UFO files. Live experiments prevent source removal/reorder while they reference source indices. | Variable-project source-authoring and source-undo tests; host source-command test. |
| Proposal apply | Installation is per glyph, with optional selection and structure checks. Missing foregrounds and stale proposals carrying base revisions are skipped and retained. Legacy proposals without a base revision still follow the existing compatibility contract. Installation copies contours, components, anchors and width, preserving foreground height, Unicode, note, guides, image and glyph lib; installed proposals leave the layer and an empty layer is removed. The source wrapper records one undo step per installed glyph. | Edit-batch stale/atomic/metadata tests, proposal tests and source proposal-history test. |
| Experimental conflicts | Forks keep a root baseline, including parent forks. Apply validates all selected glyphs and optional kerning before mutating the root; duplicate selections, stale glyph revisions, structure failures, changed root groups/kerning and an empty apply fail. Unrelated root edits survive. Apply records ordinary glyph undo plus a whole-apply record. `undo_apply` rejects changed affected glyphs or changed kerning/group revisions; it leaves unrelated edits alone and itself records glyph history. Versions remain session-only. | `document::experiments` tests and existing live entry points. |

`EditHistory::amend` replaces the recorded snapshot; its test deliberately undoes to the amended value.
The current Session drag path does not call it, so it must not be mistaken for the mechanism that preserves a drag's starting value.
The inspected `GlyphSnapshot` includes contours, components, anchors, exact advances, Unicode, note, guidelines, image and lib.
M05 must preserve those fields together while eliminating Norad storage.

### Executed checks

The focused baseline command completed successfully:

```sh
RUNEBENDER_TEST_FONTS=/Users/eli/GH/repos/virtua-grotesk/sources \
  cargo test --locked --test babelfont_contract --test variable_project --test variable_compile
```

Result: Babelfont contract 2 passed, variable compilation 6 passed, variable project 12 passed; zero failed or ignored.
The first build finished in 1m 48s and emitted a future-incompatibility notice for dependency `block v0.1.6`.
Log: `/tmp/runebender-babelfont-migration-m00/focused.log`; the counts and commands in this versioned document are the durable evidence if temporary logs expire.
Cargo used this worktree's ignored `target/`; no duplicate build was started.
Process-list inspection was denied by the sandbox, but the build was tracked to successful exit through its returned session.
The fixtures use disposable directories and the real font path is available read-only for tests that need it.

The following commands ran with the same `RUNEBENDER_TEST_FONTS` environment and `-- --test-threads=1` appended to each command.
Every row passed with zero failures and zero ignored tests.
Log names are relative to `/tmp/runebender-babelfont-migration-m00/`.

| Command | Passed | Log |
|---|---:|---|
| `cargo test --locked --lib document::history::` | 6 | `history.log` |
| `cargo test --locked --lib document::experiments::` | 4 | `experiments.log` |
| `cargo test --locked --lib document::edit_batch::` | 4 | `edit-batch.log` |
| `cargo test --locked --lib document::proposal::` | 5 | `proposal.log` |
| `cargo test --locked --lib document::source::tests::a_proposal_installs_one_undo_step_per_glyph` | 1 | `proposal-history.log` |
| `cargo test --locked --bin runebender application::editor::session::tests::` | 15 | `session.log` |
| `cargo test --locked --bin runebender application::editor::inspector::size_tests::` | 8 | `inspector-corrected.log` |
| `cargo test --locked --bin runebender application::platform::host::tests::source_commands_preserve_glyph_history_across_removal_and_reorder` | 1 | `source-history.log` |
| `cargo test --locked --bin runebender application::platform::host::tests::unicode_and_rename_undo_atomically_across_masters` | 1 | `metadata-history.log` |

Total: 65 selected tests passed, including the 20 focused baseline tests.
An initial inspector filter used `inspector::tests::` and matched zero tests; that invocation is excluded from the total.
Inspected `cargo test --locked --bin runebender -- --list`, corrected the filter to `inspector::size_tests::`, and executed all eight matching tests.
No ignored or filtered-out tests are counted as coverage.

Inventory validation found 74 `src/` Rust files with a literal whole-word Norad mention and verified that every one has a path entry in the reviewed inventory.
The inventory also documents indirect production routes, fixture-only occurrences, comment-only occurrences and candidate obsolete compatibility methods.
Documentation validation includes `git diff --check` and explicit whitespace checks on the newly added files.
M00's four acceptance steps are complete; M01–M14 remain unchecked.

### Next action

The originating task requested a pause after this coherent M00 commit while it promotes authorized work to main.
Do not start M01 until that task reports promotion complete.
Then begin M01 with the exact-value and identity ownership contract and adversarial preservation fixtures.
There is no known external blocker.

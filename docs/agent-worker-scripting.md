# Agent scripting worker report

This worker updates the bounded native Xilem client harness and the procedural scripting tests.

The scope is limited to `scripts/agent_client_harness.py`,
`scripts/test_agent_client_harness.py`, `scripts/live_spacing.py`,
`scripts/test_live_spacing.py`, and this report.

The harness requires an explicitly supplied Runebender executable and Unix session endpoint.
It never discovers a font by filename, falls back to disk operations, saves a source, or changes client configuration.

## Receipt-backed harness phase

The harness now discovers and requires `project_info`, `editor_context`, `read_glyph`,
`agent_apply`, `agent_receipt`, and `agent_history`.

The preparation phase binds one stable source ID and one document epoch,
reads the target glyph and its canonical `glyph_id`, layer name, and revision,
and constructs a bounded `set_width` request with explicit authorization,
an actor, an operation key, and a grouped history name.

The apply phase sends that exact prepared request once.

It then sends the identical request again and requires the original receipt,
`replayed: true`, and `root_changed: false`.

It looks up the receipt through `agent_receipt`, checks the applied history state,
undoes the group through `agent_history`, and verifies the receipt reports `undone`.

The optional application fixture still exercises ordinary application undo and redo
through its separate control channel.

The fixture checks canonical, application-cache, and active-session advances at each step.

It also checks that the synthetic source path remains absent before and after the edit,
undo, and redo sequence.

The apply phase includes a distinct invalid-authorization request and a distinct stale-revision request.

Both must fail without changing the canonical document.

After targeted undo, the harness rereads the glyph at its original width,
checks canonical/cache/session equality in fixture mode,
and requires the receipt body to remain byte-for-byte equal to the original immutable receipt.

The request remains procedural and thin.

It does not introduce mutable font wrappers, source-file edits, or a second transaction implementation.

## Honest capability boundary

The report marks compiled proof as pending.

No proof image call is attempted until the coordinator supplies the frozen proof API and validated binary.

The report also keeps disconnect-after-commit injection and cancellation pending.

Exact retry and receipt lookup are transport/recovery checks,
but they are not a substitute for a deliberately dropped socket response.

The harness distinguishes transport success from editing correctness and from model success.

It does not claim that a tool response reached a model,
that a model received an image block,
or that a model made a useful type-design judgment.

The pending capability fields are readiness signals for the coordinator.

They are not invented tool names and they never turn an unavailable proof or cancellation path into a pass.

## Validation

The focused Python checks are:

```text
python3 -m py_compile scripts/agent_client_harness.py scripts/test_agent_client_harness.py scripts/live_spacing.py scripts/test_live_spacing.py
python3 -m unittest scripts.test_agent_client_harness scripts.test_live_spacing -v
git diff --check
```

A real fixture trial should use a disposable application fixture and a pinned executable.

The original font sources must remain outside the fixture workflow and must be checked by a source manifest when a file-backed trial is added.

This worker does not claim a new runtime fixture result until it has run against the coordinator's frozen receipt/runtime checkpoint.

The receipt-runtime checkpoint was available for a bounded fixture run.

The command was:

```text
python3 scripts/agent_client_harness.py \
  --binary /private/tmp/runebender-desktop-virtua-20260920/runebender \
  --fixture \
  --fixture-duration-seconds 120 \
  --output-dir /private/tmp/runebender-agent-client-harness-20260920-desktop2 \
  --glyph A \
  --width 430 \
  --apply
```

It passed preparation, receipt-backed apply, exact retry, receipt lookup, targeted undo,
ordinary application undo and redo, cache/session agreement, and source-path absence.

The redacted report is `/private/tmp/runebender-agent-client-harness-20260920-desktop2/report.json`.

The executable SHA-256 is `4c15a68cc31d1d4466bea6d107076e079e575b84febafb45a039246f130dae89`.

The compiled proof, disconnect-after-commit injection, cancellation, and actual model image-delivery gates remain pending.

The pinned desktop-trial checkpoint was rerun with narrowly scoped IPC escalation and passed the same fixture scenario.

Its observed version is `runebender 0.1.0`.

Its known provenance is the coordinator-provided preserved desktop-trial checkpoint at
`/private/tmp/runebender-desktop-virtua-20260920/runebender`.

The source commit for that preserved executable is not encoded in the binary and was not independently established by this worker.

# Agent client harness worker record

Worker branch: `codex/agent-client-harness`.

Checkpoint: `170d14a756af58a041caa44878447c0fb03adbc8`.

This worker adds `scripts/agent_client_harness.py` and its focused unit tests.

The harness accepts an explicit Runebender executable and an explicit Unix live-session endpoint.
It never discovers a font from a filename and never falls back to disk operations.

It first probes the locally installed `codex`, `claude`, `omp` and `pi` command names with `--version`.
The probe does not read provider credentials, call login, edit client configuration, or start a model conversation.

It then captures the live tool schema through `RUNEBENDER_LIVE_SESSION=<endpoint> <binary> agent tools`.
The schema is hashed after canonical JSON serialization.

The bounded scenario calls `project_info`, `editor_context`, `read_glyph`, `propose_edits` and `proof` on the supplied endpoint.
With `--apply`, it performs the explicitly requested `user-approved` proposal installation, rereads the glyph, and submits a stale proposal using the pre-apply revision.

The scenario writes a redacted `report.json` and, when the current editable proof returns SVG content, `live-proof.svg` in a newly created output directory.
Credentials, authorization strings, local home/worktree prefixes and bulky SVG/image payloads are redacted from the JSON transcript.

Use a disposable application-owned fixture and a binary built from the same checkout for the full scenario:

```sh
python3 scripts/agent_client_harness.py \
  --binary /absolute/path/to/runebender \
  --session /absolute/path/to/session.sock \
  --output-dir /private/tmp/runebender-agent-client-evidence \
  --glyph live_test \
  --width 760 \
  --apply
```

Use `python3 scripts/agent_client_harness.py --probe-only` to repeat the local client availability check without an endpoint.

The coordinator's unpublished test fixture can be owned by the harness with `--fixture`.
This starts `<binary> agent fixture`, reads its readiness line, uses the reported session socket for agent IPC, and keeps fixture control lines on the fixture process's separate stdin/stdout channel.

The fixture control channel supports `state`, `undo`, `redo` and `shutdown`.
When `--apply` is supplied in fixture mode, the harness checks canonical, cache and active-session advances after apply, after real application undo, and after real application redo.

Example fixture run:

```sh
python3 scripts/agent_client_harness.py \
  --binary /absolute/path/to/runebender \
  --fixture \
  --output-dir /private/tmp/runebender-agent-client-fixture-evidence \
  --fixture-commit 999e6db \
  --glyph A \
  --width 760 \
  --apply
```

The fixture seeds a canonical unsaved `400` to `412` advance before constructing the application workspace.
It is a synthetic application/IPC test host and does not simulate native pointer or IME input.

## Capability boundary at this checkpoint

The following are real schema-backed transport checks:

- endpoint epoch returned by `project_info`;
- coherent application context returned by `editor_context`;
- unsaved canonical glyph read with a revision;
- revision-checked proposal creation;
- editable SVG proof delivery;
- authorized proposal installation when `--apply` is supplied;
- stale proposal rejection after the authorized apply.

The following remain pending integration hooks and are recorded as pending by the harness rather than treated as passing:

- receipt lookup and retry deduplication;
- immutable compiled-snapshot proof and PNG delivery from that snapshot;
- grouped atomic apply and one transaction receipt;
- disconnect after commit and recovery by receipt;
- cancellation state transitions;
- application UI refresh and ordinary or targeted undo.

The current `proof` tool is therefore reported as an editable SVG proof, not a compiled-proof success.
The current endpoint has no UI undo call, so a successful font mutation from this harness cannot be described as UI refresh or undo evidence.
Fixture mode changes only the last statement because its separate control channel invokes the real application undo path and reports cache/session state.

## Local client availability evidence

The read-only probes ran on 2026-09-19 Pacific from this worker environment.

| Client | Observed result | Evidence |
|---|---|---|
| Codex CLI | PATH wrapper failed, but the bundled desktop runtime is usable for CLI probes | `/opt/homebrew/bin/codex` (`baefc109b871e73a7bab298ee19b8bf73c8b647c4f8649a9794fc5db01db17b9`) failed because its vendored arm64 executable was missing (`ENOENT`). `/Applications/ChatGPT.app/Contents/Resources/codex` (`c147aa90d34139599711fb568102ceefc6319ca1ac5cb6f4056ca46a1834edd9`) returned `codex-cli 0.153.4` and help successfully. |
| Claude Code | Installed, version `2.1.261` | `claude --version` succeeded. `claude mcp get runebender` found the project `.mcp.json` entry but reported it pending approval. |
| OhMyPi / OMP | Installed, version `18.1.10` | `omp --version` and `omp --help` succeeded. No provider login or model run was attempted. |
| Pi CLI | Not available on `PATH` | `command -v pi` returned no executable. |

The project `.mcp.json` points to the unpinned command `runebender mcp --live`.
The harness deliberately requires an absolute binary path so evidence cannot accidentally use a stale PATH executable.

No Claude, Codex, OMP or Pi model conversation was run.
No client received a proof image through a model host, so image understanding is not claimed.

The bundled Codex runtime is evidence of a usable local CLI executable, not evidence of authenticated provider access or a successful Runebender MCP conversation.

## Priority real-client trials

The first real-client trials should use a Codex task chat in the ChatGPT desktop application and the OMP CLI.
They should use the same coordinator-owned fixture endpoint and the same pinned Runebender executable, but separate transcripts and evidence directories.

OpenAI's MCP documentation says that the ChatGPT desktop application, Codex CLI and IDE extension share the host MCP configuration, and that a trusted project may use `.codex/config.toml` for project-scoped servers.
The current official setup path is [OpenAI's MCP documentation](https://learn.chatgpt.com/docs/extend/mcp?surface=app).

Do not count the current unpinned `runebender` entry shown by `codex mcp list --json` as a valid trial configuration.
It is useful inventory evidence only because it may resolve to a stale PATH binary.

### Common fixture preparation

The coordinator starts the fixture with the pinned binary and keeps its control pipe open.
The first readiness line supplies the exact `session`, `glyph`, `source_id`, and seeded unsaved advance.

Record that readiness line, the pinned binary SHA-256, the client executable SHA-256, and a fresh redacted transcript directory before starting either client.

The first read-only smoke prompt should be:

```text
Use only the Runebender MCP server for this transport smoke test.
Call editor_sessions, connect to the exact disposable fixture session supplied by the test operator, then call project_info, editor_context, and read_glyph for the supplied glyph.
Do not call propose_edits, proposal_install, export_proof, save, or any filesystem fallback.
Report the document epoch, stable source id, glyph revision, and unsaved advance.
Do not make a visual or type-design judgment.
```

The expected read is the fixture's seeded unsaved value, not its disk value, and `source_exists` must remain false in fixture control state.

### Codex desktop task chat

Review one of these setup paths before changing configuration.
Preserve the exact command vector and environment from the validated trial manifest; do not replace a pinned executable with a PATH lookup.

```toml
[mcp_servers.runebender]
command = "/absolute/path/to/pinned/runebender"
args = ["mcp", "--live"]
```

Use the desktop application's MCP server settings to add or replace the server with the pinned STDIO command, save, and restart the MCP host before opening the trial task.
Alternatively, place the same server table in a trusted disposable project's `.codex/config.toml`.
Do not use `codex mcp add` against the user's normal configuration for this trial.

In the new Codex task chat, confirm `/mcp` lists the Runebender server, then send the common read-only smoke prompt.
The desktop task passes the transport gate only when the task transcript shows the exact fixture session was selected and the returned values match the readiness line.

For the authorized edit trial, send a separate prompt that names the exact glyph, source id, desired width, and the already-granted bounded authorization.
Require the task to read first, propose with that revision, install once, reread the unsaved value, and stop without saving.
Run fixture `state`, `undo`, `state`, `redo`, and `state` through the separate control pipe and attach those responses to the task transcript.

Keep image delivery separate from this first edit trial.
If the task calls `proof`, record whether the MCP response contains an image block and whether the model's actual response refers to the image.
Do not claim visual inspection from a successful tool call or a text-only proof result.

### OMP CLI

Use a disposable OMP profile and disposable project directory for the trial.
The locally installed OMP help confirms `--profile`, `--cwd`, `--no-session`, `--mode json`, and `--print` options.
OMP's preferred project configuration is `.omp/mcp.json`; the repository root `.mcp.json` is only a fallback and still points to an unpinned command.

Create this reviewed configuration in the disposable trial directory only:

```json
{
  "mcpServers": {
    "runebender": {
      "type": "stdio",
      "command": "/absolute/path/to/pinned/runebender",
      "args": ["mcp", "--live"]
    }
  }
}
```

Start OMP with an isolated profile and no persistent session:

```sh
omp --profile runebender-trial \
  --cwd /private/tmp/runebender-omp-trial \
  --no-session
```

Before asking the model to edit, run `/mcp reload`, `/mcp list`, and `/mcp test runebender` in that OMP session.
Then send the common read-only smoke prompt and record the JSON/text transcript separately from the tool-test output.

For the authorized edit trial, use OMP's JSON output mode and an explicit bounded prompt equivalent to the Codex task prompt.
Do not enable auto-approve for this trial.
If OMP requires provider login, record `not tested: provider authentication required` rather than copying credentials into the fixture or repository.

The OMP tool gate, model-response gate, image-receipt gate, and edit-correctness gate are separate verdicts.
An OMP model answer that mentions a proof is not image-delivery evidence unless the captured tool response contains the image block received by the model host.

### Evidence table for both clients

Record these fields independently for Codex desktop and OMP:

| Gate | Passing evidence |
|---|---|
| Host hookup | Client lists the pinned Runebender STDIO server and starts it without PATH fallback. |
| Tool discovery | `editor_sessions`, `editor_connect`, `project_info`, `editor_context`, `read_glyph`, and live proof schemas are present. |
| Correct document | The exact fixture endpoint is selected; epoch and stable source id match readiness. |
| Unsaved read | Glyph advance equals the fixture's seeded unsaved value; `source_exists` remains false. |
| Image delivery | The actual client/model receives an image block; text-only metrics do not count. |
| Authorized edit | Read → propose → one install → reread changes the requested glyph exactly once and remains unsaved. |
| UI refresh/undo | Fixture control state proves cache/session alignment across undo and redo. |
| Recovery | Receipts, disconnect-after-commit and cancellation remain pending until their hooks exist. |

Do not combine the Codex desktop and OMP verdicts into a generic “MCP works” result.

## Validation

The harness unit tests passed:

```text
python3 -m py_compile scripts/agent_client_harness.py scripts/test_agent_client_harness.py
python3 -m unittest scripts.test_agent_client_harness -v
git diff --check
```

The local client probe also completed with `--probe-only`.
The Workspace-backed fixture run is now validated with the coordinator's pinned executable.
The actual Codex desktop task and OMP model trials remain separate follow-up work.

## Fixture runtime evidence

The bounded fixture scenario passed on 2026-09-20 UTC through the approved IPC path.

```text
python3 scripts/agent_client_harness.py \
  --binary /private/tmp/runebender-agent-fixture-20260920/runebender \
  --fixture \
  --fixture-duration-seconds 300 \
  --fixture-commit 999e6db \
  --output-dir /private/tmp/runebender-agent-client-fixture-20260920-run5 \
  --glyph A \
  --width 430 \
  --apply
```

The binary SHA-256 was `a20c8e89400e7d5f02581dde6f512ea8ae26064b2af76302fce817e2dec6e759`.
The report is `/private/tmp/runebender-agent-client-fixture-20260920-run5/report.json` and the editable proof artifact is `/private/tmp/runebender-agent-client-fixture-20260920-run5/live-proof.svg`.

The run passed schema discovery, epoch and source binding, context, unsaved `A` read at 412 units, proposal, editable SVG proof, authorization rejection, authorized 412-to-430 edit, stale-write rejection, application cache/session refresh, ordinary undo, redo, and no-source-write checks.
The schema digest was `2a9a031c28a35413619e2231dcfa96b5f53039a84f468a58d944281baaedf4e7`.

Receipt lookup, compiled-snapshot proof, grouped atomic apply, disconnect-after-commit, cancellation, and actual model image receipt remain explicitly pending.

The shared build lease was occupied by another task during this worker pass, so no competing build was started.

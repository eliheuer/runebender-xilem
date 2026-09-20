# Live agent client trial evidence

These trials test a synthetic native Workspace through its real Unix mailbox and MCP adapter.
They do not open a foreground GUI or save a font, and do not complete [Milestone 1](agent-interface-plan.md).

## OMP CLI

On 2026-09-20 UTC, OMP 18.1.10 using `openai-codex/gpt-5.6-luna` completed two real model conversations.
The read-only trial made five MCP calls, selected the exact fixture endpoint, and reported the source ID, document epoch, glyph revision and unsaved width 412.
The edit trial made seven MCP calls: connect, project info, context, glyph read, proposal, one authorized install and glyph reread.
It changed only synthetic `A` width from 412 to 430.
Fixture controls then invoked ordinary application undo to 412 and redo to 430; canonical, cache and editing-session values agreed at every checkpoint.
The synthetic source path remained absent.

The client used a disposable project's `.omp/mcp.json` with an absolute pinned Runebender executable, `mcp --live`, and an explicitly selected fixture socket.
Built-in tools, extension discovery, skills, rules and session persistence were disabled for the trial.
The existing OMP model access was used without copying credentials or modifying the user's MCP configuration.
A preliminary fresh-profile attempt exited before any model or MCP call because that isolated profile had no models configured; it does not imply that the normal profile was unavailable.

The fixture executable was built from `999e6db`, with SHA-256 `a20c8e89400e7d5f02581dde6f512ea8ae26064b2af76302fce817e2dec6e759`.
OMP's executable SHA-256 was `f93613f5cc66a22e4368e955f7d121b9508306d4153b2932318e5d45ecf13a44`.
Local evidence includes `outcome.json`, `reviewed-transcript.json`, readiness, project configuration and the bounded trial driver in these directories:

- `/private/tmp/runebender-agent-omp-existing-auth-20260920`: successful read-only model trial.
- `/private/tmp/runebender-agent-omp-edit-20260920`: successful bounded model edit plus application undo/redo.
- `/private/tmp/runebender-agent-omp-smoke-20260920`: preliminary fresh-profile failure, retained separately.

The passing edit transcript contains exactly one `proposal_install` and no filesystem fallback or save call.
The proof shown by the earlier deterministic harness is source SVG; neither OMP model trial tested compiled PNG delivery or visual interpretation.
Receipt lookup, retry deduplication, grouped live apply, cancellation and disconnect recovery remain pending.

## Codex desktop task

On 2026-09-20 UTC, the desktop task “Try Virtua Grotesk through Runebender MCP” loaded the actual Runebender MCP tools and completed a live transaction trial.
The task ID is `01a0bf07-bd97-7821-b5c0-fff48e09cff4`.
It connected to an explicitly supplied headless native Workspace containing a disposable copy of the real Virtua Grotesk designspace and both masters.
The host uses the same application dispatch and history as native Xilem; no foreground window or native pointer/IME interaction was tested.

The task verified the project, document epoch and stable source, read the red `.notdef` glyph in Regular, and changed its width from 600 to 602 through `agent_apply`.
It submitted the identical request again and verified `replayed: true`, `root_changed: false`, an unchanged revision and the same immutable receipt.
Receipt lookup and glyph reread confirmed the result.
These were actual desktop MCP calls, without a CLI fallback for the font operations.

The coordinator then exercised ordinary application Undo, Redo and Undo through the host control channel.
Canonical, cache and active-session widths agreed at 600, 602 and finally 600.
All 1,744 copied source files and their originals matched their pre-trial SHA-256 hashes afterward.
The trial does not change or grade the original font, and it does not establish compiled-image delivery or native window behavior.

Evidence is in `/private/tmp/runebender-desktop-virtua-20260920/desktop-trial.json`, `ordinary-history.json` and `source-manifest.json`.
The MCP adapter was pinned to `ec6d32e`; the file-backed native host was built from the subsequent desktop-host changes on the same Babelfont-based branch.
The preserved host binary SHA-256 is `4c15a68cc31d1d4466bea6d107076e079e575b84febafb45a039246f130dae89`.

The intended editor is the native Xilem application in `~/GH/repos/runebender-xilem`, as confirmed by the user.
At the trial, its main branch was `e40bd4ce338cb8270f2356a515946e52d2b6b21b`, which is an ancestor of the agent branch.
The agent branch had no changes to the view/widget sources or `DESIGN.md` relative to that baseline.
Existing PATH and checkout build artifacts can predate current sources; always build or select an explicitly validated executable rather than inferring its version from the checkout directory.

To start a disposable background native Workspace without opening a window:

```sh
/absolute/path/to/validated/runebender agent serve \
  --font /absolute/path/to/copied/VirtuaGrotesk.designspace \
  --glyph .notdef \
  --duration-seconds 3600
```

Keep stdin open and connect MCP or the [procedural example](agent-procedural-editing.md) to the exact socket printed in the readiness JSON.
The separate stdin channel accepts one JSON object per line with `action` equal to `state`, `undo`, `redo` or `shutdown`.
The host never saves and discards its unsaved state on exit, EOF or deadline.
Use the normal native editor when a visible editing window and its save workflow are required.

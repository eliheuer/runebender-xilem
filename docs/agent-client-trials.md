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

The desktop task trial is not yet run.
The current coordinator task does not expose Runebender MCP tools in its active tool inventory.
The installed bundled Codex CLI is a separate verified executable and is not evidence that the desktop task has loaded the server.
Use the [prepared connection instructions](agent-worker-clients.md#codex-desktop-task-chat) with a pinned server, then verify actual desktop tool calls against a fresh disposable fixture.

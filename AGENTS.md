# AGENTS.md

Runebender's native editor, headless tools, and reusable font engine live in the root `runebender` Cargo package.
The browser host is a separate Cargo workspace under `web/` that reuses the root sources through a small compatibility package.
Sibling repositories are not workspace members; do not modify them unless a task explicitly spans repositories.

## Agent access

This file is the entry point for any coding agent working in the repository.
The root `.mcp.json` declares `runebender mcp --live`, the editor's local MCP command.
Clients that read `.mcp.json` can use it directly; other clients can register the same command in their own settings.
The tiny `.codex/config.toml` repeats that command so Codex can discover it automatically in a trusted checkout.
See [MCP and the live editor](https://runebender.org/docs/mcp.html) for setup and document-selection guidance.
No agent-specific directory is required to understand or work on this repository.

Before changing Rust or documentation, read and follow the
[Linebender formatting scheme](https://linebender.org/wiki/formatting-scheme/).
The repository records its stable rustfmt settings in `.rustfmt.toml`, and CI verifies them.

## Architecture

Read [the architecture guide](https://runebender.org/docs/architecture.html) before moving code or adding a subsystem.
It is the canonical source map and change-routing guide for humans and agents.

The root package produces the `runebender` executable and its library target.
A font path starts the Xilem editor; a subcommand runs headlessly before window setup.
Modules under `src/analysis`, `src/font`, `src/automation`, `src/formats`, `src/outline`, `src/text`, `src/ui`, and `src/workflows` contain reusable font and toolkit-independent editor behavior.

The in-memory document is `font::project::Project`, which owns variable glyphs and source metadata.
Exact UFO-only values remain in private glyph-layer preservation records.
Norad values exist only inside explicit import, export, proposal and serialization adapters.
Use Project layer and source operations for mutations, and Project save for open documents.
Babelfont and fontdrasil types stay behind the font-engine adapters.
Keep application state and platform work out of the font-engine modules.
Keep font mutations, analysis, formats, shaping, interpolation, selection, and undo in those modules when they can be shared.

Read the module header for the area you change.
Keep one concern per file where that remains clearer than another layer of indirection.

## Build and test

The root toolchain is pinned in `rust-toolchain.toml`, and primary Linux/macOS CI must use the same version with warnings denied.
The Windows workflow is a second-class diagnostic on current stable: keep its failures visible and document support limits, but do not add platform-specific complexity unless the task is about Windows.

Use focused checks while iterating on a change.
Run the broader checks relevant to the affected code before completing the work.
The following commands describe the full native CI suite, not a requirement for every small edit:

```sh
cargo fmt --all --check
bash .github/scripts/copyright.sh
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo doc --workspace --no-deps --locked
cargo test --workspace --locked -- --test-threads=1
cargo build --workspace --release --locked
cargo deny --locked check advisories
```

On hosts with less than 24 GiB of physical RAM, never overlap Cargo build, check, Clippy, doc, or test commands, including background jobs and subagents.
Wait for one Cargo command to finish before starting the next; do not overlap browser smoke checks with a Cargo build on these hosts.
On these hosts, set `CARGO_BUILD_JOBS=2` for Cargo commands; leave higher-memory hosts at their normal Cargo defaults unless memory pressure occurs.

Set `RUNEBENDER_TEST_FONTS` to a directory containing test UFOs and a designspace when the adjacent Virtua Grotesk checkout is unavailable.
Do not count ignored model tests as passing runtime coverage.

For a clean-checkout proof, clone or archive the repository into a temporary directory and run the documented commands there.
Do not add local path patches to a committed Cargo configuration.

## Interface work

Read [the design principles](https://runebender.org/docs/design-principles.html) before changing a view.
Use `view::theme` for colors, `view::design` for measurements, and `view::recipes` for repeated controls.
Read [the theme guide](https://runebender.org/docs/themes.html) before changing colors; it names the Base UI and Rainbow palettes and traces their tokens through the editor.
Views read workspace state; commands own intent; the font engine owns font behavior.

Use the headless screenshot path for visual checks.
For ordinary UI work, inspect only the default theme (currently Gray), and wait for idle auto-hide before accepting a capture.
Check other themes when working on themes or when the user explicitly requests them.
A headless image does not prove native pointer, IME, accessibility, or GPU behavior.
Prefer headless validation; launch a foreground GUI only when the task requires interactive evidence and the user has agreed to the interruption.

The browser reuses the desktop widget tree but has an in-memory font and a separate Cargo workspace.
When shared sources change in a way that affects the browser, run its build and smoke checks as described in `web/README.md`.

## Platform boundaries

- macOS uses `muda` for the operating-system menu bar.
- Linux, Windows, and the browser use the in-window Masonry menu.
- Live editor sockets are Unix-only; keep headless file commands portable.
- File dialogs are an application concern.
  Font-engine modules must not depend on them.

Do not turn a failing platform check green by removing it.
Record an honest support limit in the [known limitations guide](https://runebender.org/docs/known-limitations.html) when a failure cannot be reproduced or fixed in scope.

## Rust conventions

- Follow the current Linebender canonical lint set and the formatting scheme linked above.
- Every public library item needs a useful doc comment.
- An in-place edit returns whether or how much it changed.
- A UFO lib key has one constant, reader, and writer.
- Tests live beside the code they verify.
- Edition 2024; line width 100; no `unsafe` in workspace code.
- Prefer a reasoned `expect` or a structural fix to a local lint allowance.

## Documentation conventions

- Put each prose sentence on its own source line, as required by the Linebender formatting scheme.
- Update the website's architecture, design, or known-limitations page when a change invalidates its guidance.

## Dependency policy

Commit `Cargo.lock` and use `--locked` in CI.
Git dependencies must name an exact revision, not a branch.
CI runs `cargo deny --locked check advisories`; every ignored advisory needs a comment explaining the affected dependency and removal condition.
Review dependency additions and upgrades explicitly rather than treating generated policy files as evidence that their code was audited.

## Changes and Git

Preserve unrelated working-tree changes.
Stage explicit paths, never `git add -A`.
Commit coherent phases with messages that explain why, and do not add agent co-author trailers.
Do not force-push.
Inspect a worktree and preserve its changes before removing it, and only remove it when the user has authorized that cleanup.

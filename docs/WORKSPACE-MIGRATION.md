# Unified Runebender workspace

The root package is `runebender`: one executable for the Xilem editor and
headless subcommands. Core lives in `crates/runebender-core` as a library,
with GUI and CLI dependencies kept out of that crate.
New development belongs here on main. The legacy Core and GPUI repositories
are reference/fallback snapshots, not parallel development targets.

## Provenance

Core was imported using git subtree without squashing, from commit
`1d3b2a3468b1976e2a8461834e1dcd1fbf7a50a2`. Its original history remains reachable.
The existing uncommitted theme from the original Core checkout was copied into
the workspace to preserve the editor appearance; the original was not changed.
The single root Cargo.lock is authoritative. Nested historical CI and supply-chain
files are import provenance; the root CI and supply-chain configuration govern
this workspace.

## Commands

- Editor: `cargo run --release -- <font>`
- Headless CLI: `cargo run -- --help`
- Tests: `cargo test --workspace -- --test-threads=1`
- Lints: `cargo clippy --workspace --all-targets -- -D warnings`
- Docs: `cargo doc --workspace --no-deps`

Set RUNEBENDER_TEST_FONTS to the Virtua Grotesk sources, or place that repository
beside this repository. CI explicitly checks out its fixtures.

## Local migration details

The obsolete parent Cargo paths override pointing at standalone Core was removed;
a backup is in /private/tmp/runebender-parent-cargo-config-before-consolidation.toml.
Other parent overrides were preserved. Existing nested worktrees received an empty
workspace declaration in their manifests, preserving independent Cargo resolution.
Their code changes are not integrated here. Future worktrees should live outside
this repository directory.

## Verification

Workspace check, all-target Clippy, documentation, CLI help, and 404 tests passed.
Cargo metadata confirms Core resolves inside this repository. The menu worktree
metadata still resolves independently. Cargo vet reports seven missing audits for
pinned Xilem/Masonry dependencies; the same seven fail on the pre-migration visual
parity baseline. No audit exemptions were invented. Nothing was pushed.

## Single application command

The root package and executable are now `runebender`. The CLI adapter moved
from Core into `src/cli.rs`; its process tests moved to the workspace `tests/`.
Core stays a library. `runebender` or `runebender Font.designspace` opens the
editor; subcommands such as `info`, `proof`, `agent`, and `mcp` finish before
window setup. Existing scripts and MCP configurations should replace the
`runebender-core` executable with `runebender`. Local chat uses the running
application, retaining `RUNEBENDER_CORE` only as an explicit runner override.

Local validation: 503 tests passed, four opt-in tests remained ignored;
workspace Clippy with warnings denied and documentation generation passed.
Cargo metadata reports one installable binary. A disposable Git workspace
with the same binary/library/example layout accepted `cargo install --git`
without a package argument. No dependency versions changed.

`cargo vet --locked` reports nine missing audits: block2 0.6.2, rfd 0.17.2,
and the seven pinned Xilem/Masonry packages. No audit exemptions were added.
This check is not green and remains a release gate.

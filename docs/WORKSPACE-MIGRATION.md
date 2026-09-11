# Unified Runebender workspace

The root package remains the Xilem application. Core and its headless CLI live
in `crates/runebender-core`, with GUI dependencies kept out of that crate.
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
- Headless CLI: `cargo run -p runebender-core -- --help`
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

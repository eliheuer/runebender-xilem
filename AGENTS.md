# AGENTS.md

Context for AI coding agents working on `runebender-core`. Evergreen
info only. Task-specific plans live under `.agents/`. New agents:
read this top-to-bottom before touching code, then check `.agents/`
for active plans and `.agents/active/` for in-flight claims by other
agents.

Agent-name-agnostic: Codex (native `AGENTS.md` convention),
Claude Code (via `CLAUDE.md` → here), and any other human-driven
agent that reads this file get the same instructions.

## What this is

`runebender-core` is the platform-independent crate shared by the
two Runebender front-ends. It holds editing state, undo/redo,
selection, entity IDs, kerning, and glyph categorization — anything
that doesn't need to know whether it's running natively or in WASM.

Apache-2.0 licensed so both consumers can include it (xilem is
Apache-2.0; comfy is GPL-3.0 and Apache-2.0 is GPL-compatible).

## Sister repos

All assumed to be siblings under `~/GH/repos/`:

| Repo | License | Role |
|---|---|---|
| `runebender-core` | Apache-2.0 | **This repo.** Shared editing/model crate. |
| `runebender-xilem` | Apache-2.0 | Native consumer. Source of the ported modules; canonical UI/UX. |
| `runebender-comfy` | GPL-3.0 | WASM/Vue consumer (ComfyUI custom node). |

Both consumers depend on this crate via local
`path = "../runebender-core"`. Re-test both consumers after any
non-trivial change here — regressions ripple.

## ⚠ Load-bearing gotcha: the kurbo version split

This is why `runebender-core` is currently small.

- `runebender-xilem` is pinned to **kurbo 0.12** (via masonry 0.4).
- `runebender-comfy` is on **kurbo 0.13** (forced by peniko 0.5 /
  vello 0.8).

A library crate can only declare one `kurbo` version. Until the
xilem ecosystem catches up to 0.13 (the masonry-2 transition), this
crate can only host modules whose public API contains NO
`kurbo::Point`, `kurbo::Affine`, `kurbo::BezPath`, etc.

**Eligible to live here:** selection, undo, edit_types, entity_id,
kerning, glyph categorization.

**Not eligible (until kurbo aligns):** `path/`, `viewport`,
`hit_test`, `mouse`, `workspace`, `glyph_renderer`. These are
duplicated in each consumer.

If you find yourself adding a `kurbo::` type to a public signature
in this crate, stop — it belongs in the consumer repos for now.

## Layout

```
src/
├── lib.rs              # module declarations + re-exports
├── category.rs         # GlyphCategory enum + Unicode mapping
├── editing/
│   ├── edit_types.rs   # EditType enum (undo grouping)
│   ├── selection.rs    # Selection set (Arc<BTreeSet<EntityId>>)
│   └── undo.rs         # UndoState<T> (generic undo/redo)
└── model/
    ├── entity_id.rs    # EntityId (unique IDs for points/paths)
    └── kerning.rs      # UFO-spec kerning lookup
```

## Build and test

```sh
cargo build
cargo test           # ~22 tests, keep them green
cargo fmt
cargo clippy
```

No WASM target needed here — consumers compile for whichever target
they need. Don't add `#[cfg(target_arch = "wasm32")]` gates; this
crate must compile cleanly on every platform its consumers target.

## Conventions

- **Edition 2024, MSRV 1.88** (matches both consumers).
- **No kurbo types in public APIs.** See the load-bearing gotcha.
- **Test coverage for new modules.** Both consumers depend on this;
  regressions ripple. Aim for unit tests in the same file.
- **No `unsafe` without a comment explaining the invariant.**
- **Line width:** target 80 chars, 100 max (matches xilem).
- **Function order:** public before private, constructors first.

## Git workflow

- **Commit locally as you work, push only when a phase is coherent.**
  Don't push every commit. Squash iteration commits before pushing.
- Don't squash commits that have already been pushed.
- Do not include `Co-Authored-By: Claude` (or similar agent
  attribution) in commit messages.

## Multi-agent coordination

Multiple agents (Claude Code, Codex, Hermes, future others) may be
working in this repo concurrently — possibly across machines. The
protocol uses git as the lowest-common-denominator coordination
channel and lives in `.agents/active/`.

**Before starting any non-trivial task:**

1. **Pull `main` and skim `.agents/active/*.md`.** Each file is a
   claim by an agent currently working on something. If your task
   overlaps an existing claim's `touches:` list, pick a different
   slice or check with the human.
2. **Write your own claim file** to `.agents/active/<slug>.md` using
   `.agents/active/_template.md`. `<slug>` is short kebab-case. One
   file per concurrent task.
3. **Commit and push the claim immediately.** This is an explicit
   exception to the "push at milestones" rule — the claim is
   coordination state, not feature work, and is useless if other
   agents can't see it. Commit message: `claim: <slug>`.
4. **Work in a git worktree, not the main checkout:**
   ```sh
   git fetch origin
   git worktree add ~/Temp/worktrees/runebender-core-<slug> \
     -b agent/<slug> origin/main
   ```
   Worktrees isolate `target/` and Cargo lockfile churn. `~/Temp/` is
   user-policy for scratch dirs.
5. **Bump `last_touched:`** in the claim file when you resume after
   an idle stretch (hour+).
6. **Delete the claim file** when you finish, hand off, or abandon.
   Commit + push the deletion. A claim with `last_touched:` older
   than ~24h is stale — don't silently reclaim, ping the human first.

When the feature work merges, the worktree can be removed:
`git worktree remove ~/Temp/worktrees/runebender-core-<slug>`.

**Cross-repo work:** if your task spans this repo plus
`runebender-xilem` or `runebender-comfy`, file the claim in your
primary repo and list cross-repo paths in `touches:` (e.g.,
`../runebender-comfy/rust-core/src/wasm_api.rs`). Skim the other
repos' `.agents/active/` too.

Long-lived multi-session plans live at `.agents/<NAME>.md`, not
under `active/`. `active/` is only for in-flight claims.

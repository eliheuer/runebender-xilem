<!--
Template for in-flight task claims.

How to use:
  1. Copy to `.agents/active/<slug>.md` (kebab-case, e.g.
     `kerning-groups-parser`).
  2. Fill in the frontmatter and the three sections below.
  3. Commit and push immediately so other agents see the claim.
     Commit message: `claim: <slug>`.
  4. Bump `last_touched:` if you resume after an idle stretch.
  5. Delete the file when you finish, hand off, or abandon.
     Commit + push the deletion.

See `AGENTS.md` → "Multi-agent coordination" for the full protocol.
The frontmatter below is the example; replace it wholesale.
-->

---
slug: kerning-groups-parser
agent: Claude Code (eli@laptop)
branch: agent/kerning-groups-parser
worktree: ~/Temp/worktrees/runebender-core-kerning-groups-parser
started: 2026-05-14
last_touched: 2026-05-14
touches:
  - src/model/kerning.rs
---

## Goal

One short paragraph: what "done" looks like for this task.

## Status

Short bullets on where you are right now — what's landed locally,
what's blocked, what's next. Update as you go.

- [ ] step one
- [ ] step two

## Notes

Anything other agents would benefit from knowing: load-bearing
decisions, dead ends you already explored, dependencies on other
in-flight work (link by slug: see `another-slug`), files you're
about to touch but haven't yet.

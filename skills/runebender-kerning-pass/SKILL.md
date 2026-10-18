---
name: runebender-kerning-pass
description: Kern a set of pairs in a session-only experiment in the open Runebender editor, compare specimens A/B, and apply only on request. Use when asked to kern, fix pairs or check kerning.
surface: runebender-live-v1
---

# Kerning pass

Pairs or text: {{pairs}}. Source id: {{source}}.

Kerning changes happen in an experiment. The root font changes only through `experiment_apply` after the designer asks.

1. Connect and call `project_info`. Use the source id above.
2. Call `read_kerning` for the root. Keep the `revision` and note the groups so you kern group to group where groups exist.
3. Call `experiment_fork` with a short name, for example `kern-<date>`, and a reason. Fork once; make every change in that branch.
4. Call `specimen` for the root and for the branch with the same text, built from the pairs, with controls between them. Look at both images. Decide values from the image, not from the numbers alone.
5. Call `experiment_kern` on the branch with the `read_kerning` revision, a reason, and the pairs. Use `null` as a value to remove a pair. Keep values on the project grid.
6. Repeat step 4 until the branch reads better than the root. Keep the text identical between runs.
7. Report the pairs with old and new values. Call `experiment_apply` with `kerning` true and `glyphs` empty only when the designer asks, with `authorization` set to `user-approved`. `experiment_undo_apply` reverts that application.

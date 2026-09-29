# Virtua Regular brush trial

This trial tests temporary sketch ink, calibrated tracing and guarded candidate comparison on a scratch Regular font.
It does not yet demonstrate useful AI-generated Arabic drawings.
The local-model adapter has offline calibration tests, but the Brush button currently runs deterministic img2bez tracing.
Cloud generation, local Draft controls and autonomous optical refinement remain pending.

## Start the scratch session

The coordinator retains the tested native executable and copied font in `/private/tmp/runebender-kaf-trial-sjiv0m10/` before cleaning Cargo artifacts.
These are temporary local testing files, not release assets.
Launch only when ready for a native window:

```sh
RUNEBENDER_TOOL=text RUNEBENDER_GLYPH=kaf-ar.medi RUNEBENDER_TEXT='مكتبة' RUNEBENDER_TEXT_SELECTION=2:4 /private/tmp/runebender-kaf-trial-sjiv0m10/runebender-tested /private/tmp/runebender-kaf-trial-sjiv0m10/VirtuaGrotesk-Regular.ufo
```

## Try one glyph

1. In Text, select the medial kaf occurrence in مكتبة before creating a comparison graph.
2. Switch to Brush and sketch the intended form.
   Widths are font units; Erase and Clear ink affect only the temporary sketch.
3. Enter `kaf-ar.init` as the approved reference and explain its relevance, for example `Approved kaf entry stroke and upper arm`.
4. Choose **Trace to draft in Nodes**, then **Check status**.
   Inspect any error; Cancel stops a pending trace, while Release or Retry becomes available after it settles.
5. Choose **Open comparison**, then **Run** to produce unchanged and changed proofs.
   Inspect the Arabic word and outline before selecting the changed proof or Python node and choosing **Apply**.
6. Try Cmd+Z and Cmd+Shift+Z in Nodes to verify Undo and Redo.

If an existing comparison contains the default Latin proof, reopen the scratch font and select the Arabic occurrence before creating another graph.
The current UI rejects mismatched proof context rather than silently comparing the wrong text.
Temporary ink is not saved, and switching glyphs clears it.
Keep ink away from the sketch image edge; clipped ink is rejected.
No candidate is automatically graded green, and approved green/purple glyphs remain protected.

## Feedback to collect

Check brush placement, width selection, erase behavior, visible worker errors, repeat tracing, proof usefulness and Apply/Undo.
Native pointer behavior and visual drawing quality require this hands-on review; unit tests and headless captures do not establish them.

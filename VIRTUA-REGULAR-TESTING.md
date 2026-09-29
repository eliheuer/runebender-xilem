# Virtua Regular brush trial

This trial tests temporary sketch ink, calibrated tracing and guarded candidate comparison on a scratch Regular font.
It does not yet demonstrate useful AI-generated Arabic drawings.
The Brush panel offers deterministic img2bez tracing and **Draft with Virtua** using a copied local checkpoint.
The model library is `~/runebender/models`; see [Local models](LOCAL-MODELS.md) for package and runtime setup.
Cloud generation and autonomous optical refinement remain pending.

## Open Virtua Grotesk

Open the native Runebender editor on `~/GH/repos/virtua-grotesk/sources/VirtuaGrotesk.designspace`.
Choose the Regular source and the target glyph in the app.
For the first Arabic test, use the text `مكتبة` and select its medial kaf occurrence in Text before switching to Brush.
The copied model is in `~/runebender/models/virtua-12m-v1` and appears in the Brush panel as **Virtua 12M v1**.
The font source in this path is your working project; do not save an experimental candidate unless you decide to keep it.

The coordinator's earlier scratch trial and retained model evidence remain in `/private/tmp/runebender-kaf-trial-sjiv0m10/` for diagnosis, but that folder is not needed to use the editor.

## Try one glyph

1. In Text, select the medial kaf occurrence in مكتبة before creating a comparison graph.
2. Switch to Brush and sketch the intended form.
   Widths are font units; Erase and Clear ink affect only the temporary sketch.
3. Enter `kaf-ar.init` as the approved reference and explain its relevance, for example `Approved kaf entry stroke and upper arm`.
4. For deterministic tracing choose **Trace to draft**.
   For local inference, choose Virtua 12M v1 in the model controls, enter `U+0643` as the explicit kaf conditioning hint, leave identity at 1.0 initially, and choose **Draft with Virtua**.
   Then choose **Check status**.
   Inspect any error; Cancel stops a pending trace, while Release or Retry becomes available after it settles.
5. Choose **Open comparison**, then **Run** to produce unchanged and changed proofs.
   Inspect the Arabic word and outline before selecting the changed proof or Python node and choosing **Apply**.
6. Try Cmd+Z and Cmd+Shift+Z in Nodes to verify Undo and Redo.

If an existing comparison contains the default Latin proof, reselect the Arabic occurrence in Text before creating another graph.
The current UI rejects mismatched proof context rather than silently comparing the wrong text.
Temporary ink is not saved, and switching glyphs clears it.
Keep ink away from the sketch image edge; clipped ink is rejected.
No candidate is automatically graded green, and approved green/purple glyphs remain protected.

## Feedback to collect

Check brush placement, width selection, erase behavior, visible worker errors, repeat tracing, proof usefulness and Apply/Undo.
Native pointer behavior and visual drawing quality require this hands-on review; unit tests and headless captures do not establish them.

## Current model evidence

Clean1 inference succeeded through the calibrated native adapter on a scratch medial kaf raster.
The output had two contours and 14 points but lost most of the intended form; it is rejected as a drawing.
This establishes runtime connectivity, not Arabic drawing quality.
The checkpoint has U_0643 but lacks the kaf-ar.medi name token.
The selected green reference remains review context and is not fed to this model.
Keep failures and corrections as evidence for future training; do not auto-approve the output.

A saved Latin e sketch from FontGarden's clean1/generalize packet also ran through the native adapter: two contours, 28 points, recognizable e shape.
This control uses three samples, temperature 0.5, identity 1.0 and seed 0; the Brush uses these same sampling defaults.
Neither trial establishes optical approval or release readiness.

# Design

Runebender's goal is to be the best possible font editor for Eli's tastes and type-design work.
Features, interactions and architecture should earn their place by improving that experience.
Counterpunch, Fontra and other editors provide useful references to evaluate; matching their feature lists or copying their designs is not the product goal.
A Rust implementation and first-class UFO/Designspace support are deliberate choices in service of that goal.

Runebender is a dense working tool.
Its interface should keep the glyph primary, make state legible without decoration, and remain stable while values change.

## Name the role

Use a semantic token, never a one-off visual value.

- Colors come from the font engine's `themes/runebender.theme.json` through
  `src/application/view/theme.rs`.
- Space, sizes, radii, strokes, and type come from `src/application/view/design.rs`.
- Repeated control structures belong in `src/application/view/recipes.rs`.

A new color needs a named role in every shipped theme. A new measurement needs
a place in the application scale and a reason it is not an existing token.

## Color

Themes are authored in OKLCH. Gray is the default; Light is the required
contrast check; Dark alone can hide mistakes.

- Hue on the canvas must carry meaning and be reinforced by shape.
- Selection in chrome uses value contrast, not an arbitrary accent.
- Structural keylines stay distinct from filled glyph and proof ink.
- Warning and error colors are reserved for those states.
- Every token resolves in every theme; no per-view fallback colors.

## Space and size

- Use the closed spacing and control-size scales.
- Put spacing between siblings in one place; do not combine internal padding
  and external margins to approximate a gap.
- Align controls to shared heights.
- Keep chrome on whole logical pixels. Canvas geometry may use the scaled
  drawing transform.
- Content changes must not make panels jump or reorder under the pointer.

## Type

The interface uses one family and a small named type scale. Color and placement
carry hierarchy more often than size.

- Use the shared label, input, and selectable-prose helpers.
- Right-align stacked numeric values and preserve meaningful units.
- Truncate single-line labels intentionally; wrap prose intentionally.
- Do not set a local font family or size in an ordinary view.

## Canvas and chrome

The canvas explains the current glyph. The chrome explains the application.

Canvas marks compete with the outline, so keep them thin, quiet, and directly
relevant to the active edit. Panels, menus, tabs, buttons, and persistent status
belong in chrome. Avoid cards, shadows, hover decoration, or animation on top of
the drawing unless the mark communicates editing state that cannot live
elsewhere.

## Words

- Sentence case for labels, buttons, and menu items.
- Commands are verbs; labels are nouns without colons.
- Report the result: “Saved 3 glyphs,” not “Save complete.”
- No exclamation marks, apologies, or conversational filler in status text.
- Error text says what failed and what the user can do next.

## Common failures

**Themed by hand.** One literal color looks correct in the current theme and
breaks the others.

**Off the scale.** A near-duplicate gap or radius makes the system harder to
reason about.

**Chrome on the canvas.** Application decoration obscures the work.

**Decoration without information.** A divider, icon, or color reads as a signal
even when it has no meaning.

**Layout that moves.** Dynamic widths and surprise controls make familiar
targets hard to aim at.

**One unusually clever control.** Local novelty costs more than it saves in a
dense editor.

## Visual verification

Render the actual application widget tree headlessly in Gray and Light:

```sh
RUNEBENDER_SCREENSHOT=/tmp/runebender-gray.png \
RUNEBENDER_THEME=gray RUNEBENDER_SIZE=1100x720 \
cargo run --locked -- path/to/Font.designspace

RUNEBENDER_SCREENSHOT=/tmp/runebender-light.png \
RUNEBENDER_THEME=light RUNEBENDER_SIZE=1100x720 \
cargo run --locked -- path/to/Font.designspace
```

Inspect both images at the same logical and device-pixel size. Headless CPU
rendering is visual evidence only; it does not establish native GPU, input,
accessibility, or platform behavior.

## Sources and variable proofing

The Masters section owns source selection and authoring for an open Designspace.
The source name field and axis sliders define the next Add source or Apply operation.
Add source interpolates a complete UFO at that location; Up and Down change display order, and Remove retains source files on disk.
Undo sources and Redo restore structural transactions and report when later edits must be undone first.
The Layers section exposes auxiliary-layer copy and removal in both standalone UFO and Designspace documents.

The proof strip accepts text at master and intermediate locations.
Its shaping, advances and outlines use the same compiled variable font as export.
The Axes section reports compilation progress or errors instead of presenting an unsuccessful compile as a completed variable preview.

The Features section edits the default source's font-wide feature text and checks drafts with the complete variable compiler.

# Designing this editor

The shared rules are
[DESIGN.md in runebender-core](https://github.com/eliheuer/runebender-core/blob/main/DESIGN.md):
name a token rather than a value, keep the canvas quiet, and the
mistakes worth knowing by name. Read that first. This file is the
part that is specific to Xilem.

## Where the tokens are

| What | Where |
|---|---|
| Colour | `view/theme.rs`, resolved from core's `themes/runebender.theme.json` |
| Space, size, radius, stroke, type | `view/design.rs`, this application's own scale |
| Repeated view shapes | `view/recipes.rs` |
| Drawing on the canvas | `view/canvas/` |

Xilem takes a number wherever a measurement is needed, so the scale
is application code here rather than something the framework ships.
`design.rs` is that scale: `Space`, `ControlSize`, `Stroke`,
`Radius`, `TextSize`, and `Region`, which says what each kind of
container measures. Use those names. A measurement that is not in
the scale is either a new entry in the scale, argued for, or the
wrong measurement.

This is the clearest difference from runebender-gpui, where the
framework ships the scale and this file does not exist. Keeping the
vocabulary in one place is what makes that comparison honest.

## Conventions

- Call a `theme::` accessor for colour and a `design::` token for
  everything else. Never a bare number in a view.
- Views are rebuilt every frame from state. They read the workspace;
  they do not hold state of their own.
- A shape that appears twice becomes a recipe in `recipes.rs`.
- `Region` decides padding and gap for a container. Do not set them
  by hand next to a region that already sets them.

## Looking at it

`cargo run --bin screenshot` renders one frame to a PNG with no
window, which is how a change is checked here. Check it in Gray and
in Light.

Do not launch the GUI while the user is at the machine.

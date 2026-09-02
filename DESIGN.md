# Design

How the Runebender interface is designed: what a change should look
like, and how to tell a good one from a bad one. For anyone, human or
agent, changing what a person sees.

The rules below are shared with
[runebender-gpui](https://github.com/eliheuer/runebender-gpui),
the other Runebender editor.
The two look the same on purpose, so keep the shared sections in step:
a difference between these two files should be a difference the
framework forces, not one that crept in.

## The one rule

Name a token. Never name a value.

A colour, a corner radius, a stroke width, a gap, a text size: each
has a name in the shared theme, and the name is what the code says.
The moment a literal `0x808080` or a bare `7.0` appears in a view,
four themes stop agreeing and nobody can find the value again.

Every token comes from `themes/runebender.theme.json` in this
repository. The editors resolve the same file, so a change lands in
all of them at once and none can drift. If you need a colour that is
not there, add the token to the file and give it a name that says
what it is for (`point.smooth.fill`), not what it looks like
(`light_blue`).

## Colour

Colour is authored in OKLCH. Lightness and chroma mean the same
thing at every hue there, so a set of colours reads as one family
rather than a pile. Three themes ship: Dark, Gray (the default), and
Light. More come once the token system is settled.

- A token is named after its job. `metrics.baseline`, not `red_line`.
- Hue carries meaning on the canvas: a corner point and a smooth
  point are told apart by shape and colour together, never by colour
  alone.
- A selected control in the chrome is inverted, ink on the panel's
  fill, never tinted. A hue that reads on one theme is invisible on
  another and to some eyes on all of them. The accent is for meaning
  (a better score, a live run), not for "this one is picked".
- Every new token gets a value in all three themes. A theme that
  falls back is a theme that looks broken in one place.
- Check a change in Gray and in Light. Dark hides low contrast.

## Space and size

Space comes from a closed scale, not from arithmetic. Two panels
that are eight apart and nine apart look like a mistake, and it is
the kind of mistake nobody can see but everybody feels.

- Space between things, never padding inside one thing plus a margin
  outside another. Pick one and keep it.
- Controls line up on a shared height. A row of controls that are
  within two pixels of each other is worse than a row that is
  obviously different.
- Round to whole pixels. The canvas draws on a scaled grid; the
  chrome does not.

## Type

One typeface for the interface, at three or four sizes, and one of
them is the default. Weight carries emphasis, not size. A label and
its value are the same size; the label is dimmer.

Numbers in the interface are what a designer reads all day. They are
right-aligned when stacked, they keep a fixed number of decimals so
the column does not jump, and they never lose their unit.

## The canvas and the chrome

They are two design problems and mixing them is the most common way
to make the editor feel wrong.

**The canvas** shows the glyph. Everything drawn there competes with
the outline for attention, so it earns its place or it goes: thin
rules, low contrast, no fills behind anything, no shadow, nothing
animated. If a designer cannot see the shape, nothing else you did
matters.

**The chrome** is the panels, bars, and menus. It is dense, quiet,
and predictable. It does not move when a value changes. A panel that
resizes itself as numbers grow is a panel nobody can aim at.

Something belongs on the canvas only if it is about this glyph at
this moment. Everything else is chrome.

## Words

Interface text is part of the design.

- Sentence case for everything: menu items, labels, buttons.
- A command is a verb: "Add extremes", not "Extremes".
- A label is a noun, with no colon.
- Say what happened, not that something happened: "Saved 3 glyphs",
  not "Save complete".
- No exclamation marks, no apologies, no "Oops".
- The status line reports; it does not chat.

## Mistakes with names

These are the ways generated interface work goes wrong. Each one
looks reasonable in isolation.

**Themed by hand.** A view that reads a token for most colours and
names one literal for the odd case. It looks right in the theme you
were in, and only that one.

**Off the scale.** A gap of 10 where the scale has 8 and 12, because
10 looked better on this screen. Now the scale has a hole and the
next person adds 11.

**Chrome on the canvas.** A rounded panel, a drop shadow, or a hover
highlight drawn over the glyph. It reads as an application feature
sitting on the artwork.

**Decoration standing in for information.** An icon that means
nothing, a divider that separates nothing, a colour that carries no
meaning. Every mark in an editor is read as a signal, so a mark with
no meaning is a lie.

**Layout that moves.** A panel that changes width with its content,
a list that reorders while the pointer is over it, a control that
appears on hover in a place a click was heading.

**Cleverness in one place.** A single control designed better than
everything around it is worse than a plain one. Consistency is what
lets a person stop looking.

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
container measures. Use those names. A measurement that is not in the
scale is either a new entry in the scale, argued for, or the wrong
measurement.

This is the clearest difference from runebender-gpui, where the
framework ships the scale and this file does not exist. Keeping the
vocabulary in one place is what makes that comparison honest.

Views are rebuilt every frame from state. They read the workspace;
they do not hold state of their own. A shape that appears twice
becomes a recipe in `recipes.rs`. `Region` decides padding and gap
for a container, so do not set them by hand next to one that already
does.

## Looking at it

`cargo run --bin screenshot` renders one frame to a PNG with no
window, which is how a change is checked here. Check it in Gray and
in Light.

Do not launch the GUI while the user is at the machine.

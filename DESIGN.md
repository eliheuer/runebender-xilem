# Designing the Runebender interface

For anyone, human or agent, changing what the editors look like.
The code layout document says where a file goes. This one says what
to put in it so the result looks like Runebender and not like a
generic application with a font in it.

Both editors follow this. What differs between them is in each
repository's own `DESIGN.md`: the vocabulary the framework gives you.

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
rather than a pile. Four themes ship: Dark, Midnight, Gray (the
default), and Light.

- A token is named after its job. `metrics.baseline`, not `red_line`.
- Hue carries meaning on the canvas: a corner point and a smooth
  point are told apart by shape and colour together, never by colour
  alone.
- Every new token gets a value in all four themes. A theme that
  falls back is a theme that looks broken in one place.
- Check a change in Gray and in Light. Dark hides low contrast.

## Space and size

Space comes from a closed scale, not from arithmetic. Two panels
that are eight apart and nine apart look like a mistake, and it is
the kind of mistake nobody can see but everybody feels.

The scale is the same idea in both editors, but the spelling differs
by framework. See each repository's `DESIGN.md`.

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

## Checking your work

Look at it. A screenshot of the changed region, in Gray and Light,
next to the version before it. In runebender-xilem,
`cargo run --bin screenshot` renders a frame with no window. In
runebender-gpui, `RB_OPEN_GLYPH=<name>` starts in the editor on that
glyph so a capture needs no clicks.

Do not launch the GUI while somebody is working at the machine.

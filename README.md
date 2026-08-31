# runebender-xilem

A font editor built on [Xilem](https://github.com/linebender/xilem) and
the Linebender stack. It shares its editing engine,
[runebender-core](https://github.com/eliheuer/runebender-core), with
[runebender-gpui](https://github.com/eliheuer/runebender-gpui), the
main Runebender editor.

This front-end is behind the GPUI one and a catch-up pass is
planned; the GPUI build's `PARITY.md` is the feature bar.

**This repository is one half of a controlled comparison.** The same
editor, the same core, the same design, built twice: once on Xilem here,
once on GPUI in runebender-gpui. The goal is to keep the two as close as
a person can, so that what differs is the framework and not the app.

It builds against **upstream Xilem**, pinned to a revision, and not
against a fork. That is deliberate: a comparison is only worth something
if it measures Xilem itself. Where Xilem has no answer for something the
editor needs, the answer lives here in application code, and that code is
part of the result. Two examples, both in this repository:

- `src/design.rs`, about four hundred lines of spacing scale, control
  sizes, radii, type scale, and a table of what each kind of container
  measures. None of it is about fonts. GPUI ships this (`px_1`..`px_4`,
  `text_xs`, `rounded_md`) so the equivalent file does not exist there.
- `src/screenshot.rs`, a headless render path, because Xilem has none and
  every visual decision here is made by looking at a PNG.

Ideas prototyped inside the framework, rather than around it, live in
[xix](https://github.com/eliheuer/xix), a fork used as a laboratory. The
intent is that anything that works there is offered upstream.

The old Druid-to-Xilem port that used to live in this repository stopped
in August 2026 and is preserved at the tag `v0-druid-port-2026-08`.

## Run it

```sh
cargo run --release -- path/to/Font.ufo
```

A `.designspace` opens its first master. Any UFO works; try one from a
font project you have.

To render one frame with no window, which is how review screenshots and
agents see the editor:

```sh
RUNEBENDER_SCREENSHOT=out.png cargo run --release -- path/to/Font.designspace
```

Editing writes back to the UFO on save, so **work on a copy** if you
care about the source.

## What works

Overview grid (with glyph name + codepoint labels, mark colors, a
search + category sidebar) and a glyph editor with these tools:

- **Select** — click a point, shift-click to add, drag to move, drag
  empty space for a marquee. Middle/right-drag pans; the wheel zooms.
- **Pen** — click for corner points, click-drag for smooth points with
  bezier handles, click the first point to close.
- **HyperPen** — click to place hyperbezier points (the curve is solved
  automatically, no handles); alt-click for a corner; click the first
  point to close.
- **Rect / Ellipse** — drag to draw.
- **Knife** — drag a line to cut contours.
- **Measure** — shows segment/stem lengths and side bearings.

### Keyboard

Window-level shortcuts work regardless of focus; the canvas adds its
own while it has focus. Path operations such as flip, rotate, and
remove overlap live in the menu.

| Key | Action |
| --- | --- |
| v / p / b / u / o / e / m | tools: Select, Pen, HyperPen, Rect, Ellipse, Knife, Measure |
| Cmd/Ctrl+S | save |
| Cmd/Ctrl+N | new font |
| Cmd/Ctrl+C, Cmd/Ctrl+V | copy, paste contours |
| Cmd/Ctrl+D | duplicate selection |
| Escape | return to the overview |
| Arrows | nudge selection (Shift = ×10), canvas only |
| Delete / Backspace | delete selected points, canvas only |
| Cmd/Ctrl+A | select all, canvas only |
| Cmd/Ctrl+Z, Cmd/Ctrl+Shift+Z | undo, redo, canvas only |

Click a glyph in the overview to select it, click again to open the
editor. The toolbar's `‹ Overview` returns.

## Layout

The layout mirrors runebender-gpui file for file where the two share
a concern, so a change in one editor is easy to carry to the other:
`workspace.rs`, `actions.rs`, `launch.rs`, and the `view/`, `edit/`,
`platform/`, and `widgets/` directories, each with a `mod.rs` that
says what belongs in it. `AGENTS.md` has the table, and
[runebender.org/docs/code-layout.html](https://runebender.org/docs/code-layout.html)
the long version.

## Develop against local xix / core

Nothing in this repository points at a local path. To build against
sibling checkouts of xix and runebender-core, put a
`.cargo/config.toml` in the directory **above** the repositories:

```toml
paths = ["runebender-core"]

[patch."https://github.com/eliheuer/xix"]
xilem = { path = "xix/xilem" }
masonry = { path = "xix/masonry" }
```

## License

Apache-2.0 OR MIT, the Linebender convention.

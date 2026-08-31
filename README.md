# Runebender Core

[![CI](https://github.com/eliheuer/runebender-core/actions/workflows/ci.yml/badge.svg)](https://github.com/eliheuer/runebender-core/actions/workflows/ci.yml)

The font library behind the [Runebender][rb] font editor: everything
the editor knows how to do to a font, with no interface attached.

Point edits, segment surgery, overlap removal, components and anchors,
kerning with group fallback, curvature analysis, shaping, measurement,
interpolation. The editors share it. Each owns its window, its
input, and its drawing, and calls this crate for the rest. It also
ships `runebender`, a command-line tool, so the same code runs in a
script, a build, or an agent's hands with no window at all.

One rule decides what belongs: **if an edit changes a font, it goes
here.** The test is not whether the code is about drawing. It is
whether another front-end would need it. Where a click lands is the
shell's business. What that click does to the outline is this crate's.

Two things follow. The hard parts are testable without opening a
window, so they have tests that run in milliseconds. And a new
front-end is a new interface rather than a second editor.

## The command line

```sh
cargo install --git https://github.com/eliheuer/runebender-core
```

Reading commands answer one question about a source and print it:

```sh
runebender info Font.ufo
runebender glyphs Font.ufo
runebender measure Font.ufo --glyph eight
runebender spacing Font.ufo
runebender check --a Light.ufo --b Bold.ufo
```

Editing commands run an operation the editor runs from a menu, and
save what changed:

```sh
runebender clean Family.designspace
runebender overlap sources/*.ufo --glyphs cent,euro
runebender offset Font.ufo --by -4
runebender convert Font.ufo --to quad
runebender realign Family.designspace
runebender rename Family.designspace --from uni0041 --to A
runebender kern Font.ufo --left A --right V --set -80
```

Four rules make them safe to run across a library:

- **A designspace stands for its sources.** One path names a whole
  family, and a source two masters share is opened once.
- **`--dry-run` writes nothing and exits 1 when there is work
  waiting.** Nothing means exit 0, so a script can tell the two apart.
- **A source is written only when it changed.** An operation that
  finds nothing to do leaves the bytes alone, so a sweep does not
  churn a repository.
- **`--json` on every command**, with the names of the glyphs that
  changed, so an agent can read the result rather than parse a table.

Exit codes are 0 ok, 1 findings, 2 usage, 4 failed, matching
[font-ml][fml] so the two are driven the same way.

Across a library, with a check first and the work in parallel:

```sh
# What would change, family by family, without touching anything.
for ds in ~/fonts/*/sources/*.designspace; do
  runebender clean --dry-run --json "$ds"
done | jq -s 'map(select(.edits > 0)) | .[].sources[].source'

# Then do it, eight at a time.
ls ~/fonts/*/sources/*.designspace | xargs -P 8 -n 1 runebender clean
```

An operation you can only reach by opening a window is one a script, a
build, or an agent cannot use.

## Layout

Directories group the modules by what they do to a font, and each
`mod.rs` says what belongs in it. `src/lib.rs` carries the map; the long form, with the conventions and the CI gate, is at
[runebender.org/docs/code-layout.html][layout].

```
src/
├── outline/     what changes a shape: glyph_ops, point_ops, segment_ops,
│                knife, cleanup, effects, convert, embolden, glyph_paths,
│                and path/ for the segment maths
├── analysis/    what reads a font: measure, optical, spacing, curve,
│                category, search
├── formats/     lib_keys, metrics_keys, mark_color, color_font, svg,
│                binary_import, glyphs_import, image_trace
├── document/    project (Master, Project), var_model, composites,
│                font_memory, new_font, and model/ (kerning, metadata)
├── text/        shape (harfrust over the font's own features),
│                joining (Arabic rules), buffer (the Text tool)
├── ui/          theme, theme_oklch, sidebar, editing/
├── testing/     fonts.rs, where the tests find Virtua Grotesk
└── bin/runebender/ the command line: read.rs reports, edit.rs
                 writes, sources.rs expands a designspace
```

## Checks

CI runs `cargo fmt --check`, `cargo clippy --all-targets` and
`cargo doc` with warnings denied, and the tests on Linux and macOS
and at the minimum Rust. `unsafe` is forbidden in the manifest.

The tests load Virtua Grotesk from its own repository, [virtua-grotesk][vg], cloned
beside this repository or named by `RUNEBENDER_TEST_FONTS`.

## The format is the model

Sources are edited as UFO through [norad][norad], rather than read into
a private model and written back. Nothing is lost in translation, and
another tool can read the sources mid-session. The cost is that the
file's shape is the editor's shape, awkward parts of UFO included.

This is also why a script or an agent can work alongside a running
editor: the editor reloads what changed on disk.

## Two versions of kurbo

This crate carries `kurbo` twice, as `kurbo_09` and through its
consumers. Front-ends are pinned to their toolkit's version, and a library can
only declare one. Rather than hold every front-end to the oldest, the
geometry types are pinned here and converted at the boundary. Every
consumer is on kurbo 0.13 now, so the only conversion left is around
the hyperbezier solver.

## Front-ends

| | |
| --- | --- |
| [runebender-gpui][gpui] | The main editor. Native and browser from one codebase. |
| [runebender-xilem][xilem] | The same editor on Xilem, kept in step for a framework comparison. |
| [runebender-web][web] | The old web editor. Deprecated in favour of runebender-gpui. |
| [runebender-druid][druid] | The original, kept as project history. |

## License

Apache-2.0 OR MIT, the Linebender convention.

[rb]: https://runebender.org
[gpui]: https://github.com/eliheuer/runebender-gpui
[xilem]: https://github.com/eliheuer/runebender-xilem
[web]: https://github.com/eliheuer/runebender-web
[druid]: https://github.com/linebender/runebender
[norad]: https://github.com/linebender/norad
[fml]: https://github.com/eliheuer/font-ml
[vg]: https://github.com/eliheuer/virtua-grotesk
[layout]: https://runebender.org/docs/code-layout.html

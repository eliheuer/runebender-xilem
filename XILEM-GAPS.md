# What Xilem costs, measured against the same editor on GPUI

<!-- Copyright 2026 the Runebender Authors -->
<!-- SPDX-License-Identifier: Apache-2.0 OR MIT -->

Two builds of one font editor, sharing an engine (`runebender-core`) and
a design: this one on upstream Xilem, and `runebender-gpui` on GPUI plus
`gpui-component`. The GPUI build is at near-parity with the web editor
and tracks it in its own `PARITY.md`. This file records what stands
between this build and that one, and separates three different things
that all feel the same while you are working:

1. **Blockers.** Xilem cannot do it today, and no amount of application
   code fixes it.
2. **Taxes.** Xilem can do it, but the application pays for what the
   framework does not carry.
3. **Just unfinished.** Application work, nobody's fault but ours.

Audited 2026-08-26 against runebender-gpui's `PARITY.md`.

## 1. Blockers

**No native menu bar, and no way to build one.** Xilem is built on
winit, which has no menu support of any kind, so Masonry never inherited
menus. The GPUI build has a real macOS menu bar, and one action list
drives both the menu items and their accelerators. A font editor with no
File menu is not a font editor anyone will use, and this is the single
largest difference in how the two builds feel.

**No global shortcut dispatch.** Masonry routes a key event to the
focused widget and bubbles it up the ancestor chain. There is no place to
register a window-level shortcut. Every application therefore invents
one, and this one is `src/shortcuts.rs` (263 lines): a widget that wraps
the entire application, catches what the focused widget did not consume,
matches a keymap, and submits an action. It works, and it is a fake.

**In-window popups exist one layer down, and are unreachable from
application code.** Masonry has a `Layer` trait and
`create_attached_layer`, used by its own tooltip and selector menu. There
is no Xilem view that exposes it, so a right-click context menu cannot be
built in application code the normal way. This editor paints its context
menu into the editor canvas by hand and hit-tests it manually. That is
why the menu cannot escape the canvas bounds.

**No headless render path.** Every visual decision here is made by
looking at a PNG, and an agent cannot see its own work any other way.
Xilem offers nothing, so `src/screenshot.rs` (82 lines) assembles one out
of public API: build the root view against a `ViewCtx`, hand the widget
to a `TestHarness`, rebuild once so canvas views fill their scene, then
rasterize with Vello CPU. It works, and it turned up its own API wart:
the harness needs a root with a concrete widget type, so the application
root has to be wrapped in a `sized_box` first.

**The root view can get too big for the linker.** Adding a file watcher
around the menu pump around the shortcut host, all of them ordinary
view combinators, produced this:

```
ld: Assertion failed: (name.size() <= maxLength),
    function makeSymbolStringInPlace, file SymbolString.cpp, line 74.
```

Not a compile error. A link error, after everything compiled, on a
clean build. Xilem views are monomorphized and nest their types, so
three wrappers around an application-sized tree made a mangled symbol
name longer than the macOS linker accepts. The fix is to erase the type
with `.boxed()`, which works and takes one line, but it has to be
discovered from a linker assertion, and nothing in the API suggests
that composing views has a depth limit. This is the generics problem
the Xilem maintainer's review describes, arriving in a form nobody
predicted.

## 2. Taxes

**The design system, 410 lines.** `src/design.rs` is a spacing scale,
control sizes, radii, strokes, a type scale, and a table of what each
kind of container measures. None of it is about fonts, and every
application on Xilem writes its own version. GPUI ships this
(`px_1`..`px_4`, `text_xs`, `rounded_md`), so the equivalent file does
not exist in the other build. Without it, this editor drifted to twenty
distinct spacing values and four text sizes; with it, the panels state a
gap or an inset in four places, all of them "no gap".

**No parts.** Xilem has a widget set, not an application parts list.
There is no list row, no field, no section header, no panel, no card, no
dialog, no toast, no table, no tree. `src/ui.rs` is this editor's version
of six of them. The GPUI build imports 60-plus from `gpui-component`,
which is why it can spend its lines on font editing.

**Icon buttons, 175 lines.** A toolbar tile that paints a vector icon and
reports clicks is a widget the framework should have.

**Canvas scaffolding.** `editor.rs` and `grid.rs` are 1,322 lines, and a
large part of both is viewport math, screen-to-document conversion, and a
drag state machine written by hand. This is not purely a Xilem tax, since
GPUI has no canvas primitive either and its build pays a similar cost.
It is the largest shared gap in both frameworks, and the reason the xix
fork has an `Island` widget.

## 3. Just unfinished

Application work that no framework blocks: `.glyphs` import, live
file-watch reload, the tab strip, multiple edit sessions, preview mode,
background layers, image trace, kerning groups, new-font-from-template.

## 4. What the numbers say so far

This build is 5,363 lines against the GPUI build's 26,879, and the
comparison is meaningless until they do the same things, because the
difference is mostly missing features. The number that is meaningful
today is the scaffolding: **996 lines here (design 410, shortcuts 263,
icon buttons 175, screenshot 82, text labels 66) that exist only because
the framework does not carry them**, against roughly zero in the GPUI
build, which gets the equivalents from GPUI itself and from
`gpui-component`.

Two of the four blockers (menus, global shortcuts) are one problem, and
the 2026 ecosystem survey independently concluded that a maintained
winit-compatible shell facade "would give every winit-based framework
that for free". It is not a Runebender problem or an Xilem problem. It
is the ecosystem's missing layer.

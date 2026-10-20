# Panel language

A panel section is built from `recipes::Row` values grouped under captions.
There is no other way to put content in a section.
The vocabulary, its rules, and every measurement live in [`src/application/design/recipes/rows.rs`](../src/application/design/recipes/rows.rs); this page is the summary.
`tests/panel_vocabulary.rs` fails the build when a panel module reaches past the vocabulary to a bare label, column, or button.

## The design system

Panels are one part of the design system.
Its entry point is [`src/application/design/mod.rs`](../src/application/design/mod.rs).
Styles come from three layers:

1. The theme files in `assets/themes/default/` give every color and shape value.
2. `src/application/design/` gives the scales, the control states, and the themed stock widgets.
3. `src/application/design/areas.rs` gives each exception for an area of the window, with its reason.

A control names a `ControlKind`.
The state table then gives it a look at rest, under the pointer, pressed, selected, disabled, and with keyboard focus.
`tests/design_system.rs` fails the build when a view sets a color, opacity, stroke, radius, or text size by itself.
That test counts the older exceptions, and each count can only go down.

## Four rules

1. A section is one question. Its title is a short noun phrase in title case. It holds groups, and each group answers one part of the question.
2. A group is a caption and its rows. Rows sit 4 pixels apart. Groups are separated by a hairline that stops at the section margin, with 12 pixels above and below. Sections are separated by the full-width rule. Three spacings, two dividers, no others.
3. Prose is state, not instruction. A section never explains itself at the top. `Row::Meta` is one muted line under the thing it describes. Instructions live where the action is.
4. One `Row::Primary` per section. Everything else is `Row::Secondary` or `Row::Quiet`. Facts go behind `Row::Details`.

Captions and titles are title case, never all caps. Nothing in the view uppercases text, and the gate test keeps it that way.

## The rows

| Row | Looks like | Use for |
| --- | --- | --- |
| `Meta` | One muted line | What the row above is: "14 words, 2 spare" |
| `Prose` | One line in body ink | A note or a result |
| `Field` | Caption over an input | A value you type |
| `Picker` | The current choice with a chevron | One of several models or files; never a stack of toggles |
| `Chips` | Small outlined pills over as many lines as needed | A set of words or tags |
| `Choice` | Segmented control | Two or three exclusive options |
| `Primary` | Filled button, full width | The one thing the section is for |
| `Secondary` | Outlined button, full width | The other way to do it |
| `Quiet` | Muted text button | Rare or reversible: Stop, Cancel, Undo |
| `Status` | One sentence and a thin bar | Something in progress |
| `Tiles` | Squares with a caption each | Samples, layers, masters |
| `KeyValue` | Name left, value right | Facts: steps, loss, folder |
| `Details` | Disclosure, closed by default | Rows nobody needs every day |
| `Slider` | Caption and a slider | A value on a range |
| `Legacy` | A view from an older module | Only while that module moves to the vocabulary |

If a feature needs a row that is not here, add it to `Row` once, with a reason in its doc comment.

## Build a section

```rust
use crate::application::design::recipes::{self, Group, Row, click};
use crate::application::workspace::Workspace;
use xilem::WidgetView;

fn width_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let model = Group::new(
        "Model",
        vec![
            Row::Picker { choices: vec!["shapes-base".into()], chosen: Some(0), on_pick: std::sync::Arc::new(|_, _| {}) },
            Row::Meta("14 words, 2 spare".into()),
        ],
    );
    let train = Group::new(
        "Train",
        vec![Row::Primary { text: "Train From This Font".into(), on_click: click(|app| app.shapes_train(Default::default())) }],
    );
    recipes::section_of(app, "Local AI Shapes", "Local AI Shapes", vec![model, train])
}
```

`recipes::section_of` owns the header, collapse state, inset, group dividers, and the full-width rule.
`recipes::section_with_details` adds a Details strip under the section rule.
`recipes::panel_stack` stacks sections.
The reference section is `src/application/view/panels/ai_shapes.rs`; the Local AI rail in `panels/local_ai.rs` is the second.
Views read workspace state and forward edits to existing workspace intents; font mutations and undo stay in the commands and font engine.

## Older sections

Sections written before the vocabulary use `recipes::panel_section`, `recipes::section`, `recipes::panel_group`, `recipes::field`, and `design::column` and `design::row` with a `Region` directly.
They are listed in `tests/panel_vocabulary.rs` and leave that list as they move.
Until then: `panel_group` owns an 8-pixel inset and the full-width divider; `section` owns the header and a 4-pixel gap; section bodies add no outer padding.
Read colors from `design::theme::Palette` and measurements from `design` instead of assigning local colors, radii, or pixel values.
Let the dock own its width; section contents fit the available width.

## Built-in theme baseline

Theme files begin with `formatVersion`, `id`, and `name`, followed by the Base UI and Rainbow palette definitions.
The built-in Rainbow palette contains the seven mark-picker hues: red, orange, yellow, green, blue, purple, and pink.
The `[drawing]` section follows the palettes and controls how their colors appear on glyph tiles and outline points.
It contains `markStep`, `markStyle`, `markOutline`, `markInk`, `pointStyle`, `pointOutline`, and `pointHalo`.
Existing custom themes can keep these settings at the top level, but must not also define the same setting in `[drawing]`.
Dark, Dark Gray, Gray, Light Gray, Light, Strawberry, and Campfire explicitly define panel, tile, and control rounding, slider states, header ink, backdrop tint, and panel shadows.
Gray uses 6-pixel tile corners; the other built-ins use 8-pixel tile corners.
All built-ins use 10-pixel panel corners and 4-pixel control corners while keeping their own palettes and point styles.
Sidebar tabs use the glyph-tile radius up to 4 pixels for their top corners.
The active tab's concave bottom flares are 2 pixels larger, because outer curves read as even only when slightly larger than the corners they surround.
The strip uses 6-pixel tab gaps and outer insets, and a flare never exceeds that inset.
Icons that are inset in their frame draw at 14 pixels and icons that fill their frame draw at 12, so all tab icons look the same size.
The active icon sits lower than the inactive icons.
Optional `text.inactiveTabInk` supplies solid inactive icon ink; older themes retain their previous opacity treatment.
Panel outlines sit outside the content clip, with matching concentric radii and paint bounds that include the full stroke.
Selected glyph and sidebar labels use the theme's yellow mark; other selected controls use `controlSelectedInk`.
Both inks must remain readable against `controlSelected`, including in Dark.
Dark uses charcoal glyph tiles with colored outlines and matching glyph/caption ink; the other built-ins retain filled marks.
The native grid must honor `[drawing] markStyle` rather than assuming a filled mark.
The floating metrics card follows the same mark treatment.
The inspector outline preview uses `surfaces.glyphPreview`, falling back to `surfaces.canvas` in existing custom themes.
Gray uses `baseUi.08` for that preview while its main drawing canvas stays `baseUi.07`.
The preview boundary owns a full-strength section keyline inside the scroll viewport.
Inspector preview markers use 75% of the main editor point radius.
Optional `roles.proofInk` separates proof-strip type from neutral `previewFill` in the editing canvas.
Optional `roles.headerActiveInk` colors active title-bar tools and tabs without changing the document title.
Optional `roles.controlSelectedOutline` colors selected control keylines.
Optional `roles.savedInk` and `roles.unsavedInk` keep save-state text readable without changing glyph mark colors.
Older custom themes retain their previous colors when these roles are absent.
Dark uses green active controls, yellow proof type, and subdued neutral field borders to keep the glyph primary.
Dark Gray uses Gray's filled tiles, neutral proof ink, and yellow selected labels on charcoal panels.
Its colored ring points keep their edges visible on a dark canvas.
Both dark themes use slider knobs one Base UI step darker than their panels and brighten the fill during interaction.
Light pairs pale gray window chrome with near-white panels and a white drawing surface.
Light Gray uses the same light chrome with softer gray panels and canvas; its fields are one Base UI stop above the panel.
Both light themes and Strawberry use a deep green Saved label and dim red Not saved label, with at least 4.5:1 contrast against their panels.
Strawberry applies a rose Base UI ramp across the window, panels, controls, and ink.
Campfire applies an ember-brown and copper Base UI ramp with cream ink.
These showcase themes preserve Gray's Rainbow mark colors and shared geometry.
Native title-bar appearance follows the brightness of the opaque window ground at startup and after theme changes.
Light, Light Gray, and Strawberry request light native appearance; the other built-ins request dark appearance.
The macOS backdrop uses the matching Vibrant Light or Vibrant Dark appearance.
Linux keeps flat application surfaces and requests the same native appearance where the window manager supports it.
Compare the glyph grid and edit mode in all built-ins at the same viewport when adding or changing a built-in theme.
Headless proofs cover solid-mode layout and colors; wallpaper blur still needs native review.

## Window backdrops

All built-ins enable the optional macOS `[window] blurBackground` setting.
Keep it `false` or omit it for the supported solid background; Gray uses `surfaces.app` and `surfaces.header`, both `baseUi.02`.
View > Translucent Window flips the setting while the application runs, for comparing the two; the theme's own value applies again at the next start.
Linux, browser, and headless hosts always use flat colors.
The native experiment places an AppKit behind-window effect below the GPU view and adds a `surfaces.backdropTint` overlay.
That optional color defaults to `surfaces.app` in older themes.
Dark uses the approved `#404040` tint; Dark Gray uses `#505050`; Gray uses `#808080` at 50% opacity to keep the window ground lighter over dark wallpaper.
Dark and Dark Gray use 25% opacity; Light uses a `#F0F0F0` tint at 65% to stay light over dark wallpaper.
Light Gray uses `#E5E5E5` at 65%, Strawberry uses `#F3B8C4` at 75%, and Campfire uses `#774D2F` at 38%.
Unsupported hosts keep their opaque theme colors.
`window.blurTintOpacity` is a diagnostic opacity from 0 to 1, with a default of 0.8 when omitted.
On 2026-09-30, the user's desktop capture `1852-005` confirmed wallpaper blur in the full editor with transparent startup and zero application tint.
`window.shadowPanels = false` suppresses only main-panel shadows when the native backdrop is active.
Solid mode retains `geometry.shadowPanel`; glyph tile shadows and other application shadows are independent.
The independent tint gently lifts dark wallpaper and reduces its color influence.
To use the backdrop, enable `blurBackground`; its color and opacity are independent of the solid background.
Run with `RUNEBENDER_FRAME_STATS=1` to have the window render every frame and report its frame cadence on stderr, which is how the backdrop's cost is measured.
The original zero-overlay test can be reproduced with `blurTintOpacity = 0.0`.
Earlier 0.8 and 0.65 tint settings appeared washed out.
The small Xilem example in `examples/macos_vibrancy.rs` reproduced bright, jagged edges on translucent text and shapes.
An isolated comparison with Vello 0.10.0 and wgpu 30.0.1 still reproduced those artifacts.
Backporting [wgpu #9922](https://github.com/gfx-rs/wgpu/pull/9922) made the demo and editor text smooth, as confirmed by the user on-screen.
The native dependencies now pin a Xilem fork that applies the equivalent Metal premultiplication workaround to the supported renderer versions.
Remove that workaround when the supported dependency graph includes the upstream correction.
[Xilem #1852](https://github.com/linebender/xilem/issues/1852) is closed with these findings.
Floating panels must clip their complete content subtree to the theme radius.
Painting a flat background over their square corners fails when the real backdrop varies, so the panel frame paints only its outline and outside shadow.
The rounded clip brackets child painting with matching pre-paint and post-paint clips; keep panel descendants in the inline paint layer.
The user accepted the rebuilt native result after the rounded clipping change.
Verify the result on the actual desktop, since isolated window captures showed flat gray even when the minimal probe visibly blurred.
The experiment and tested settings are recorded beside `set_backdrop` in `src/application/platform/window.rs`.

## Center-panel footers

Use `recipes::panel_footer(pal, controls)` for the center bottom bar in overview and editor modes.
The middle of the bar is the document readout: the font's path and save state, centred.
The path gives way to the file name when the bar is narrow, and the name is elided below that; a transient note replaces the readout while it shows.
Bare footer icons show hover and press in their ink, never as a tile behind them.
It owns the full-width upper divider, the 28-pixel height, centered controls, and the outer inset.
The 10-pixel side inset gives controls optical clearance from the curved panel corners; the 16-pixel icons remain vertically centered with 6 pixels above and below.
Neutral slider tracks use the shared 2-pixel `SLIDER_TRACK_THICKNESS`, independently of the thumb size and hit area.
Keep icon clusters in unpadded `Region::Inline` rows and use `STATUS_ICON_SIZE` for compact footer buttons.
Do not add a local footer height or horizontal padding; change the shared recipe and geometry when the footer contract changes.

## Check a new section

Check the expanded and collapsed section, including its last control and the next divider, in a headless screenshot using the default Gray theme.
Check its narrowest supported dock width and any other mode that reuses the same contents.
Run focused tests for changed interaction or layout behavior and the repository formatting checks.
Reuse shared recipe tests for common spacing instead of adding identical tests to every section.
Headless screenshots do not verify native pointer, input method, accessibility, or GPU behavior; validate those separately when the change requires it.
See [`AGENTS.md`](../AGENTS.md) for the full validation guidance and the [design principles](https://runebender.org/docs/design-principles.html) for the broader visual rules.

## Headless screenshots

Set `RUNEBENDER_SCREENSHOT` to a PNG path to render one frame of the editor and exit without a window:

```sh
RUNEBENDER_SCREENSHOT=/tmp/shot.png cargo run --release -- path/to/Font.designspace
```

The capture uses the same text and font settings as the window.
These variables control what the frame shows:

| Variable | Effect |
| --- | --- |
| `RUNEBENDER_SIZE=1100x720` | Logical window size; the default is 1100 by 720. |
| `RUNEBENDER_SCALE=2` | Device-pixel scale at the same logical layout. |
| `RUNEBENDER_THEME=<id>` | Theme ID; the default is Gray. |
| `RUNEBENDER_GLYPH=<name>` | Open this glyph in the editor. |
| `RUNEBENDER_SELECTED=<name>` | Select this glyph in the grid. |
| `RUNEBENDER_SELECTALL=1` | Select all points in the open glyph. |
| `RUNEBENDER_EXPAND=<key>` | Expand the panel section with this collapse key. |
| `RUNEBENDER_COLLAPSED=<key,...>` | Collapse these panel sections. |
| `RUNEBENDER_HIDDEN_DOCKS=<left,right>` | Fold these docks away, as the footer toggles do. |
| `RUNEBENDER_RAIL=<ai\|chat\|scripts\|shapes>` | Open this tool pane. |
| `RUNEBENDER_VIEW=<option,...>` | Turn on canvas view options: `comb`, `continuity`, `colorize`, `handles`, `segments`, `bearings`, `popcount`. |
| `RUNEBENDER_AXIS=wght=500,wdth=80` | Set the design-space location. |
| `RUNEBENDER_MASTER=<name>` | Make this master active, by its style name. |

The code in `src/application/launch.rs` and `src/application/platform/host.rs` reads these variables.
Search for `RUNEBENDER_` in `src` to find more specialized ones.

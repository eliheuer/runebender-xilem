# Adding a panel section

Panel sections share a layout contract so a new section inherits balanced spacing, theme styling, and collapse behavior.
Start with the recipes in [`src/application/view/recipes.rs`](src/application/view/recipes.rs) rather than assembling another panel frame.
The same contract applies to the left sidebar and right inspector.

## Choose the composition

- `recipes::panel_section(app, key, title, body)` builds a complete collapsible panel section.
- `recipes::section(app, key, title, body)` builds the header and collapsible contents without an outer inset or divider.
- `recipes::panel_group(pal, body)` adds the shared outer inset and full-width divider to contents that do not need a collapse header.
- `recipes::panel_stack((section, ...))` stacks complete sections without adding space beside their dividers or another outer inset.

`panel_section` composes `panel_group` and `section`, so do not wrap it in another `panel_group`.
Use a stable section key for the existing `Workspace` collapse state and a separate display title for its label.
Choose a unique key for unrelated sections; reuse a key only when both dock appearances should share collapse state.
When a section appears in both docks, reuse its contents and apply the same recipe in each location.
Keep a deliberate exception, such as the compact node inspector header, in `section_with_header_height` with a named measurement from `design`.

## Spacing and styling

`panel_group` owns an 8-pixel inset on all four sides and the dividing line that spans the panel width.
This leaves the same space between the last field and the divider as between that field and the panel sides.
`section` owns the standard header and a 4-pixel gap before its expanded contents.
Section bodies have no outer padding; adding it duplicates the recipe's inset.
Standalone tool panes use `Region::Panel` to own their inset; the rail must not add another wrapper inset around them.

Use `design::column` and `design::row` to state the kind of layout:

| Region | Gap | Typical contents |
| --- | --- | --- |
| `Region::Form` | 8 pixels | Groups of fields and form rows |
| `Region::Inline` | 4 pixels | Labels, icons, and small controls on one line |
| `Region::List` | 2 pixels | Dense rows or a field's caption and input |

Use `recipes::field` for an edit that can commit on each change and `recipes::field_enter` when the whole value must be entered before committing.
These recipes provide the standard input height, caption typography, theme colors, outline, and control radius.
Wrap direct `text_input` views with `input_typography(input, pal.text)` so their retained text editor receives theme changes as well as their placeholder label.
The wrapper bridges the pinned Xilem input's color-update behavior without replacing focused fields or losing selections.
Use `recipes::labeled_control(pal, caption, controls)` for a shared caption over a control row; empty field captions leave no blank label row.
For equal columns in a `Region::Form` row, wrap each field in `recipes::field_column(field)`; the row shares its available width after gaps, and the fields grow when the dock widens.
Use the shared button and list recipes for the same reason.
Read colors from `view::theme::Palette` and measurements from `view::design` instead of assigning local colors, radii, or pixel values.
Let the dock own its width; section contents should fit the available width without repeating the dock's default or minimum width.
If a reusable control needs a new spacing rule, change its recipe or introduce a named design measurement with a reason rather than tuning each call site.

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
Optional `roles.savedInk` and `roles.unsavedInk` keep save-state text readable on pale headers without changing glyph mark colors.
Older custom themes retain their previous colors when these roles are absent.
Dark uses green active controls, yellow proof type, and subdued neutral field borders to keep the glyph primary.
Dark Gray uses Gray's filled tiles, neutral proof ink, and yellow selected labels on charcoal panels.
Its colored ring points keep their edges visible on a dark canvas.
Both dark themes use slider knobs one Base UI step darker than their panels and brighten the fill during interaction.
Light pairs pale gray window chrome with near-white panels and a white drawing surface.
Light Gray uses the same light chrome with softer gray panels and canvas; its fields are one Base UI stop above the panel.
Both light themes and Strawberry use a deep green Saved label and dim red Not saved label, with at least 4.5:1 contrast against their headers.
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
It owns the full-width upper divider, the 28-pixel height, centered controls, and the outer inset.
The 10-pixel side inset gives controls optical clearance from the curved panel corners; the 16-pixel icons remain vertically centered with 6 pixels above and below.
Neutral slider tracks use the shared 2-pixel `SLIDER_TRACK_THICKNESS`, independently of the thumb size and hit area.
Keep icon clusters in unpadded `Region::Inline` rows and use `STATUS_ICON_SIZE` for compact footer buttons.
Do not add a local footer height or horizontal padding; change the shared recipe and geometry when the footer contract changes.

## Example

This section uses the existing editor width buffer and commit method.
Only show it in the glyph editor, where `set_advance_from_buf` applies to the active editing session.
For an overview edit, use the corresponding overview command instead.

```rust
use crate::application::view::{design, recipes};
use crate::application::workspace::Workspace;
use xilem::WidgetView;

fn glyph_width_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let body = design::column(
        design::Region::Form,
        (recipes::field(
            pal,
            "Width",
            app.advance_buf.clone(),
            |app: &mut Workspace, value| app.set_advance_from_buf(value),
        ),),
    );
    recipes::panel_section(app, "Glyph width", "Glyph width", body)
}
```

Views read workspace state and forward edits to existing workspace intents.
Keep font mutations and undo behavior in the commands and font engine rather than putting them in a layout closure.
For an operation such as renaming, let the change callback update the input buffer and the Enter callback invoke the rename intent.

## Check a new section

Check the expanded and collapsed section, including its last control and the next divider, in a headless screenshot using the default Gray theme.
Check its narrowest supported dock width and any other mode that reuses the same contents.
Run focused tests for changed interaction or layout behavior and the repository formatting checks.
Reuse shared recipe tests for common spacing instead of adding identical tests to every section.
Headless screenshots do not verify native pointer, input method, accessibility, or GPU behavior; validate those separately when the change requires it.
See [`AGENTS.md`](AGENTS.md) for the full validation guidance and the [design principles](https://runebender.org/docs/design-principles.html) for the broader visual rules.

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
| `RUNEBENDER_RAIL=<ai\|chat\|scripts\|shapes>` | Open this tool pane. |
| `RUNEBENDER_VIEW=<option,...>` | Turn on canvas view options: `comb`, `continuity`, `colorize`, `handles`, `segments`, `bearings`, `popcount`. |
| `RUNEBENDER_AXIS=wght=500,wdth=80` | Set the design-space location. |
| `RUNEBENDER_MASTER=<name>` | Make this master active, by its style name. |

The code in `src/application/launch.rs` and `src/application/platform/host.rs` reads these variables.
Search for `RUNEBENDER_` in `src` to find more specialized ones.

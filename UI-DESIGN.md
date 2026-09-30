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
Use `recipes::labeled_control(pal, caption, controls)` for a shared caption over a control row; empty field captions leave no blank label row.
For equal columns in a `Region::Form` row, wrap each field in `recipes::field_column(field)`; the row shares its available width after gaps, and the fields grow when the dock widens.
Use the shared button and list recipes for the same reason.
Read colors from `view::theme::Palette` and measurements from `view::design` instead of assigning local colors, radii, or pixel values.
Let the dock own its width; section contents should fit the available width without repeating the dock's default or minimum width.
If a reusable control needs a new spacing rule, change its recipe or introduce a named design measurement with a reason rather than tuning each call site.

## Window backdrops

The experimental `[window] blurBackground` setting is disabled in Gray while native text and panel-edge artifacts remain unresolved.
Keep it `false` or omit it for the supported solid background; Gray uses `surfaces.app` and `surfaces.header`, both `baseUi.02`.
Native window transparency is chosen at startup, so enabling the experiment from a solid window requires restarting the application.
Linux, browser, and headless hosts always use flat colors.
The native experiment places an AppKit behind-window effect below the GPU view and adds a `surfaces.app` tint.
`window.blurTintOpacity` is a diagnostic opacity from 0 to 1, with a default of 0.8 when omitted.
On 2026-09-30, the user's desktop capture `1852-005` confirmed wallpaper blur in the full editor with transparent startup and zero application tint.
Gray preserves `blurTintOpacity = 0.0` from that capture, with `blurBackground = false` for everyday work.
To reproduce the backdrop, enable `blurBackground` and restart; keep other application changes intact.
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

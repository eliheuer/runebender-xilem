# Runebender themes

A Runebender theme is one portable `.theme.json` file.
Built-in examples are in [builtin](builtin/).
The native editor reads installed files at startup; the browser bundles the three built-in themes.
The format has `formatVersion: 1` so future versions can change without guessing how to read an older file.

## The two palettes

| Name | JSON section | What to say when requesting a change |
| --- | --- | --- |
| **Base UI** | `baseUi` | “Make Base UI 20 warmer.” Numbered stops progress from darkest to lightest in the built-in themes. A custom theme can tint or replace each stop. |
| **Glyph Grid** | `glyphGrid` | “Change Glyph Grid red.” Red, orange, yellow, green, blue, purple, and pink are stable mark categories; teal is also available for editor roles. Each hue has `dim`, `base`, `bright`, and `deep` steps. |

`surfaces`, `text`, and `roles` say where those palette colors are used.
For example, `gray.roles.pointSmooth` is `glyphGrid.blue.base`, while `gray.surfaces.panel` is `baseUi.20`.
A role can also use an explicit color such as `#AABBCC`.
The theme's `markStep` chooses which Glyph Grid step appears on glyph cells.

Runebender writes simple canonical `public.markColor` values to UFO files (`1,0,0,1` for red, for example).
These values are compatibility tags, not display colors; each theme controls the visible Glyph Grid palette.
The editor still recognizes older values already saved in UFOs.

## Make and use a theme

```sh
runebender theme init --from gray --id my-theme --name "My Theme" --out my-theme.theme.json
runebender theme validate my-theme.theme.json
RUNEBENDER_THEME_PATH="$PWD/my-theme.theme.json" RUNEBENDER_THEME=my-theme runebender
```

The `init` command creates a new file and never overwrites an existing one.
Use `--from light` or `--from dark` to start from those built-in themes.
`runebender theme list` and `runebender --json theme list` show discovered themes.
The native View → Theme menu includes installed themes, and Cycle Theme visits them after the built-ins.
Restart the editor after editing a theme file.

The native editor searches `$XDG_CONFIG_HOME/runebender/themes/`, or `~/.config/runebender/themes/` when `XDG_CONFIG_HOME` is unset.
It reads files ending in `.theme.json` directly in that directory.
`RUNEBENDER_THEME_PATH` adds one file or a platform-separated list of files and directories.
These paths can point into a Git checkout or a dotfiles repository; a symlink in the config directory works too.
`RUNEBENDER_THEME` selects an installed theme by its `id`.
An unknown ID uses Gray and prints a diagnostic.
Invalid files are skipped with a path and error message; they do not prevent the editor from opening.
Installed IDs must be unique, use only ASCII letters, digits, `-` or `_`, and contain at most 64 characters.
Display names must contain 1–80 visible characters.

## Edit the file

Each theme file owns its Base UI stops, Glyph Grid hues and steps, surface colors, text colors, semantic roles, point and mark styles, and geometry.
It does not depend on another theme file, so one file is enough to share a theme.
The built-ins use `oklch(L C H)` values, with lightness `L` from 0 to 1 and hue `H` in degrees.
You can also use `#RRGGBB` or `#RRGGBBAA` for a Base UI stop or a direct surface, text, or role value.
References use `baseUi.01` or `glyphGrid.red.base` syntax.
The parser reports missing roles, bad references, unsupported format versions, and invalid colors by theme and key.
Unknown JSON fields are rejected so misspelled settings do not silently disappear.

The shape fields in `geometry` are pixel measurements: `radius`, `radiusControl`, `stroke`, and `strokeEmphasis`.
`markStyle` is `fill` or `border`; `pointStyle` is `fill` or `ring`.
`markOutline`, `markInk`, and `pointOutline` are optional colors; `pointHalo` is a boolean.
All seven named Glyph Grid marks must remain in `glyphGrid.marks` exactly once, because glyph mark names are saved font metadata.

Start a color change at the use site under `src/application/view/`, then follow its named color in `src/application/view/theme.rs` to the file's `surfaces`, `text`, or `roles` section.
`src/ui/theme.rs` is the toolkit-independent parser and resolver.
`src/application/platform/themes.rs` discovers native files.
Check Gray and Light captures at the same size after a built-in change.

## Sharing and external desktop themes

A theme repository can contain its `.theme.json` file, a preview image, and a README; Runebender needs only the JSON file.
This is enough to share through GitHub or dotfiles and gives a future gallery a stable file to index.
Do not download and execute code to install a color theme.

An Omarchy theme repository can keep a Runebender `.theme.json` beside its `colors.toml`, then expose that file through `RUNEBENDER_THEME_PATH` or a config-directory symlink.
The two formats are separate today; Runebender does not automatically read `colors.toml` or follow Omarchy theme switches.
A small generator or Omarchy template can translate its color values into Runebender's Base UI stops later, without changing Runebender's file format.

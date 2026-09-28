# Local Regular Arabic trial preflight

Date: 2026-09-28.
This is read-only preflight evidence, not F1 acceptance.
No scratch copy, candidate, shaped proof, human grade, source-font edit, or runtime execution was created.

## Current source grades

The authoritative field is each Regular UFO GLIF's `public.markColor`.
The green value is `0.09,0.72,0.44,1`; green relatives below are therefore approved and must not be edited.
The blue value is `0,0.67,0.91,1`; it is an awaiting-grade value, not an automatic target.
The only red Arabic entries use `1,0.29,0.24,1`:

| Red glyph | Source path | Current structure | Trial suitability |
| --- | --- | --- | --- |
| `arabicNumberSign` U+0600 | `/Users/eli/GH/repos/virtua-grotesk/sources/VirtuaGrotesk-Regular.ufo/glyphs/arabicNumberSign.glif` | one contour, six points, no advance | Red, but a sign rather than a joining letterform; no same-construction green Arabic reference found. |
| `arabicSignSanah` U+0601 | `/Users/eli/GH/repos/virtua-grotesk/sources/VirtuaGrotesk-Regular.ufo/glyphs/arabicSignSanah.glif` | one contour, six points, no advance | Same limitation. |
| `arabicFootnoteMarker` U+0602 | `/Users/eli/GH/repos/virtua-grotesk/sources/VirtuaGrotesk-Regular.ufo/glyphs/arabicFootnoteMarker.glif` | one contour, eight points, no advance | Same limitation. |
| `arabicSignSafha` U+0603 | `/Users/eli/GH/repos/virtua-grotesk/sources/VirtuaGrotesk-Regular.ufo/glyphs/arabicSignSafha.glif` | one contour, six points, no advance | Same limitation. |

There is no eligible red Arabic joining form in the current source inventory.
Do not substitute a blue composition: for example, `beh-ar`, `theh-ar`, `jeem-ar`, `khah-ar`, `sheen-ar`, `ghain-ar`, `feh-ar`, `qaf-ar`, `noon-ar`, and `yeh-ar` are blue (`0,0.67,0.91,1`), including component-built outlines.

Relevant green construction references exist, but cannot make those blue composites eligible:

- Beh-family skeletons: `behDotless-ar` (one contour, 24 points, advance 944), `behDotless-ar.init` (one contour, 14 points, advance 248), and `behDotless-ar.medi` (one contour, 26 points, advance 408).
- Hah/jeem bowl family: `hah-ar` (one contour, 39 points, advance 704), `hah-ar.init` (33 points), and `hah-ar.medi` (44 points).
- Feh/qaf bowl family: `fehDotless-ar` (two contours, 44 points, advance 961), `fehDotless-ar.init` (36 points, advance 480), `fehDotless-ar.medi` (33 points, advance 512), and `qafDotless-ar` (two contours, 44 points, advance 904).

Coordinator decision: defer choosing the first target while Eli is asked for a preferred glyph/form.
Arabic signs remain within the existing Arabic scope; a same-construction green glyph is useful evidence, not an added requirement for user authorization.
The simplest red candidate identified here is `arabicNumberSign`; its unfamiliar construction still needs reference and proof research before a useful trial.

## Local sketch runtime inventory

- Installed environment: `/Users/eli/GH/repos/font-garden-lab/.venv/bin/python` (Python 3.14.5); filesystem inspection found MLX 0.31.2, fontTools, Pillow, scipy, and scikit-image.
- Concrete default candidate: `/Users/eli/GH/repos/font-garden-lab/runs/clean1/` with `config.json`, `vocab.txt`, and `weights.safetensors` (54,199,266 bytes).
  `/Users/eli/GH/repos/font-garden-lab/runs/sketch1` is only a symlink to `clean1`; pin `clean1`, not the alias.
- Arabic-oriented candidate: `/Users/eli/GH/repos/font-garden-lab/runs/sketchpre/` with `config.json`, `vocab.txt`, and `weights.safetensors` (54,728,338 bytes).
  Its notes describe it as multi-script, Unicode-conditioned generalization; no fresh Virtua Arabic result was run or accepted here.
- Avoid `/Users/eli/GH/repos/font-garden-lab/runs/sketch4/` for this trial: its notes identify it as the Latin-only model.
- The unsafe path is `glyphlab.sketch2glyph --install`, which directly splices a blue draft into `~/GH/repos/virtua-grotesk/sources`.
  A future detached trial needs a calibrated scratch PNG, target name/Unicode, advance, target height, y offset, left sidebearing, a pinned run directory, and invocation without `--install`.
  The script also requires its tracing dependency path to be executable; this preflight did not invoke it, download anything, or verify an inference result.

## Missing evidence before F1

A selected eligible target, a scratch source copy, measured reference package, calibrated input image, pinned-runtime invocation receipt, detached candidate, and matched shaped reading-size proofs remain absent.

Coordinator spot-check used glyphs/contents.plist to resolve filenames.
`qaf-ar.glif` is blue and contains two components; `qafD_otless-ar.glif`, `fehD_otless-ar.glif`, `behD_otless-ar.glif`, and `hah-ar.glif` have the stated green color.

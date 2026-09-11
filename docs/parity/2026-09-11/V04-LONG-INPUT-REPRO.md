# V04 long-input caret reproducer

The application deliberately clips single-line inputs so long glyph names and
other values cannot expand an inspector row. That contains the layout, but the
pinned Masonry `TextArea` cannot yet keep the insertion caret visible after it
moves beyond the clipped viewport.

## Upstream boundary

The workspace pins Xilem/Masonry revision
`b81d8d7a631849def6eeab282561439b963862e5`. In
`masonry/src/widgets/text_area.rs`, the documentation for both
`TextArea::with_word_wrap` and `TextArea::set_word_wrap` says that an unwrapped
text area does not currently support scrolling to the cursor. Runebender's
single-line `TextInput` fields use that unwrapped behavior.

## Minimal reproducer

1. Place a Masonry `TextInput` in a `170 × 28` logical-pixel box.
2. Disable word wrapping and enable clipping, as a normal single-line field
   does.
3. Set its text to `lam_alefH_amzabelow-ar.fina`.
4. Focus the input and press End, or move the insertion point to the logical
   end with the right-arrow key.
5. Observe that the row remains bounded, but the text viewport does not scroll
   horizontally and the insertion caret is outside the visible field.

## Runebender impact

This affects the glyph-name inspector first, and can also affect long group,
reference, graph, or chat values. Existing tests prove that the shared 28 px
control height contains selected `gyp` descenders and signed numeric text, and
that placeholder and entered text share ink bounds. A macOS system-font test
also proves that Arabic beh and alef resolve to distinct glyphs. Those results
do not prove end-caret visibility for long text.

V04 therefore remains open. The upstream opportunity for Q03 is a small
horizontal viewport/scroll-to-caret implementation in `TextArea`, followed by
a focused Runebender test for long Latin and Arabic-associated glyph names and
a supervised native caret/selection pass.

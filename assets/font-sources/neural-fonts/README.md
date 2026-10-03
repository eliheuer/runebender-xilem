# Neural fonts

A `.nufo` is a UFO directory whose glyphs are items: a word, a phrase or a line of calligraphy drawn as one outline.
Each item carries its text and labels that say which ink belongs to each letter, in the `com.runebender.neuralItem` glyph lib key.
Runebender opens a `.nufo` in a neural mode without advance boxes or metrics.

```sh
cargo run -- assets/font-sources/neural-fonts/NastaliqDemo.nufo
```

`NastaliqDemo.nufo` holds one empty canvas, `ba-basic`.
Place a picture to trace with Place image… in the inspector.

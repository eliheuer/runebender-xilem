# Neural fonts

A `.nufo` is a UFO directory whose glyphs are items: a word, a phrase or a line of calligraphy drawn as one outline.
Each item carries its text and labels that say which ink belongs to each letter, in the `com.runebender.neuralItem` glyph lib key.
Runebender opens a `.nufo` in a neural mode without advance boxes or metrics.

```sh
cargo run -- assets/font-sources/neural-fonts/NastaliqDemo.nufo
```

`NastaliqDemo.nufo` holds one item with a placed picture to trace and label.
The picture is calligraphy by Mishkín-Qalam (1826–1912), public domain, via Wikimedia Commons.

# Font sources

Sources that Runebender opens, grouped by kind.
Compiled fonts that the program embeds stay in `../fonts`.

- `static-fonts/`: single-master OpenType sources. `Shapes.ufo` is the sample the Local AI Shapes tool trains from; the macOS bundle carries it under `Resources/font-sources/static-fonts/`.
- `variable-fonts/`: Designspace projects with their UFO masters.
- `neural-fonts/`: neural sources (`.nufo`), whose items are words and phrases of calligraphy with labels for training.

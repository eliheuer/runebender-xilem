# Proof renderer integration

Runebender uses the installed Designbot CLI for node and MCP proofs. Runebender
supplies a version-1 data-only scene; Designbot returns raw PNG or PDF output.
The GUI builds snapshots and runs rendering on a background worker. The MCP adapter
runs rendering in its own process. Neither route saves the live font.

The resvg prototype was removed, including its transitive rustybuzz dependency.
No new cargo-vet exemptions were added. Runebender shapes live text with harfrust;
Designbot's text stack uses Parley and harfrust. Designbot's dependency maintenance
remains the responsibility of that project, rather than entering the core library.

Install a Designbot release supporting this command:

```sh
designbot render-scene --png proof.png < scene.json
designbot render-scene --pdf proof.pdf < scene.json
```

Scenes contain version, width, height, paths (SVG path data, not SVG documents),
and optional labels. Coordinates are y-up. There are no script expressions, file
references, or arbitrary commands. The protocol bounds dimensions and input size.
The core adapter has a 30-second timeout and removes temporary rendering files.
Set DESIGNBOT_BIN to choose a specific executable; otherwise it uses PATH.

The Designbot CLI scene mode always uses raw output. Its existing script command
and social-media rendering behavior are unchanged.

Validation includes synthetic PNG/PDF inspection, OMP image delivery, independent
A/B kerning branches with an unchanged root, and PDF export through MCP. These
checks establish the transport and edit contract, not the quality of AI design.

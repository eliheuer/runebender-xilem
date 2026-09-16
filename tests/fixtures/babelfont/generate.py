"""Generate our synthetic import fixture with upstream babelfont 3.1.3."""
from pathlib import Path
from types import SimpleNamespace
from datetime import datetime
from babelfont.convertors.nfsf import Babelfont
from babelfont import Font, Master, Glyph, Layer, Shape, Node, Anchor

font = Font()
font.date = datetime(2020, 1, 1)
font.names.familyName.set_default("Babelfont Test")
font.upm = 1000
master = Master(name="Regular", id="M1")
master.metrics = {"ascender": 800, "descender": -200, "capHeight": 700, "xHeight": 500}
master.kerning = {("A", "V"): -80}
font.masters.append(master)
for name, codepoint in [("A", 65), ("V", 86)]:
    glyph = Glyph(name=name, codepoints=[codepoint])
    layer = Layer(width=600, _master="M1", id=name+"-M1")
    layer.shapes = [Shape(nodes=[Node(0, 0, "l"), Node(200, 0, "o"), Node(300, 300, "o"), Node(400, 400, "cs"), Node(0, 400, "l")])]
    layer.anchors = [Anchor(name="top", x=300, y=700)]
    glyph.layers = [layer]
    font.glyphs.append(glyph)
alt = Glyph(name="A.alt", exported=False)
alt.layers = [Layer(width=650, _master="M1", id="A.alt-M1", shapes=[Shape(ref="A", transform=[1, 0, 0, 1, 25, 0])])]
font.glyphs.append(alt)
Babelfont.save(font, SimpleNamespace(filename=str(Path(__file__).parent / "Basic.babelfont"), scratch={}))

# Keep upstream JSON semantics while satisfying repository whitespace checks.
for path in (Path(__file__).parent / "Basic.babelfont").rglob("*"):
    if path.is_file():
        path.write_text("\n".join(line.rstrip() for line in path.read_text().splitlines()) + "\n")

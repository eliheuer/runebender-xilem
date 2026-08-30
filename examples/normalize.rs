//! Normalize a UFO to pure norad/runebender form: strip the
//! Glyphs-app export leftovers (`com.schriftgestaltung.*` keys at
//! font and glyph level) and rewrite the whole font through norad,
//! so later editor saves are byte-stable. Keys the toolchain still
//! reads are kept: `com.github.googlei18n.ufo2ft.filters` (build
//! behavior), `com.glyphsapp.component.alignment` (component
//! anchor alignment), and `com.runebender.*`.
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: normalize <font.ufo>");
    let mut font = norad::Font::load(&path).expect("load");

    let stripped: Vec<String> = font
        .lib
        .keys()
        .filter(|k| k.starts_with("com.schriftgestaltung."))
        .cloned()
        .collect();
    for key in &stripped {
        font.lib.remove(key);
    }
    println!("{path}: {} font lib keys stripped", stripped.len());

    let mut glyph_keys = 0;
    let layer_names: Vec<norad::Name> = font.layers.iter().map(|l| l.name().clone()).collect();
    for layer_name in layer_names {
        let Some(layer) = font.layers.get_mut(&layer_name) else {
            continue;
        };
        let names: Vec<norad::Name> = layer.iter().map(|g| g.name().clone()).collect();
        for name in names {
            let Some(glyph) = layer.get_glyph_mut(&name) else {
                continue;
            };
            let keys: Vec<String> = glyph
                .lib
                .keys()
                .filter(|k| k.starts_with("com.schriftgestaltung."))
                .cloned()
                .collect();
            for key in keys {
                glyph.lib.remove(&key);
                glyph_keys += 1;
            }
        }
    }
    println!("{path}: {glyph_keys} glyph lib keys stripped");

    font.save(&path).expect("save");
    println!("{path}: normalized");
}

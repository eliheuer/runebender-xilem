// Real OpenType shaping for the editor's text buffer.
//
// harfrust shapes a *compiled* font, and what we have is a UFO being
// edited. So we build a font on the fly that has everything shaping
// needs and nothing it doesn't: a cmap, advances, and the layout tables
// compiled from the source's own features.fea by fea-rs. No outlines —
// the editor draws those itself from the live paths, and shaping never
// looks at them.
//
// That means the font's own rules do the work: init/medi/fina come from
// its `init`/`medi`/`fina` features rather than our joining table, and
// required ligatures like lam-alef come from `rlig`, which no amount of
// per-character logic would have produced.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fea_rs::{
    Compiler, GlyphMap,
    compile::{NopFeatureProvider, NopVariationInfo},
    parse::{SourceLoadError, SourceResolver},
};
use harfrust::{Direction, FontRef, ShaperData, UnicodeBuffer, script};
use write_fonts::{
    FontBuilder,
    tables::{
        cmap::Cmap,
        head::Head,
        hhea::Hhea,
        hmtx::{Hmtx, LongMetric},
        maxp::Maxp,
    },
    types::GlyphId,
};

/// One glyph as the shaper needs to see it.
#[derive(Debug, Clone)]
pub struct ShapingGlyph {
    pub name: String,
    pub advance: f64,
    /// Codepoints that map to this glyph. Usually zero or one.
    pub unicodes: Vec<u32>,
}

/// Everything needed to build a shaping font for one master.
#[derive(Debug, Clone)]
pub struct ShapingSource {
    pub units_per_em: f64,
    /// Glyph order. Index is the glyph id, so `.notdef` belongs first.
    pub glyphs: Vec<ShapingGlyph>,
    /// The master's features.fea, verbatim.
    pub features: String,
}

/// One glyph the shaper produced.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedGlyph {
    /// Index into the source's glyph order.
    pub glyph_id: u16,
    /// Which input character this came from. A ligature reports the
    /// cluster of its first character, so several glyphs can share one
    /// cluster and one glyph can stand for several characters.
    pub cluster: u32,
    pub x_advance: f64,
    pub x_offset: f64,
    pub y_offset: f64,
}

/// A compiled shaping font, ready to shape with.
#[derive(Debug)]
pub struct ShapingFont {
    bytes: Vec<u8>,
    names: Vec<String>,
}

/// features.fea handed to fea-rs from memory: there is no filesystem in
/// wasm, and the feature text arrives from the host as a string.
struct InMemoryFea {
    root: PathBuf,
    text: Arc<str>,
}

impl SourceResolver for InMemoryFea {
    fn get_contents(&self, path: &Path) -> Result<Arc<str>, SourceLoadError> {
        if path == self.root {
            Ok(self.text.clone())
        } else {
            // `include()` would need the rest of the source tree, which we
            // do not have in the browser.
            Err(SourceLoadError::new(
                path.to_path_buf(),
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "include() is not supported while shaping from memory",
                ),
            ))
        }
    }
}

const FEA_ROOT: &str = "features.fea";

impl ShapingFont {
    /// Compile a font for shaping. Fails if the feature file does not
    /// compile — which it will, halfway through an edit, so callers are
    /// expected to keep working with whatever they had.
    pub fn build(source: &ShapingSource) -> Result<Self, String> {
        if source.glyphs.is_empty() {
            return Err("no glyphs to shape with".into());
        }
        let upem = source.units_per_em;
        let upem_u16 = if (16.0..=16384.0).contains(&upem) {
            upem as u16
        } else {
            1000
        };

        let names: Vec<String> = source.glyphs.iter().map(|g| g.name.clone()).collect();
        let glyph_map: GlyphMap = names.iter().map(|name| name.as_str()).collect();

        let compilation =
            Compiler::<NopFeatureProvider, NopVariationInfo>::new(FEA_ROOT, &glyph_map)
                .with_resolver(InMemoryFea {
                    root: PathBuf::from(FEA_ROOT),
                    text: Arc::from(source.features.as_str()),
                })
                .compile()
                .map_err(|e| format!("features.fea: {e}"))?;

        let mut builder = FontBuilder::new();

        // Header tables. Only the fields shaping reads are meaningful:
        // units per em, glyph count, and the advance widths.
        let mut head = Head {
            units_per_em: upem_u16,
            ..Default::default()
        };
        head.index_to_loc_format = 0;
        builder.add_table(&head).map_err(|e| e.to_string())?;

        let glyph_count =
            u16::try_from(source.glyphs.len()).map_err(|_| "more than 65535 glyphs".to_string())?;
        builder
            .add_table(&Maxp::new(glyph_count))
            .map_err(|e| e.to_string())?;

        let metrics: Vec<LongMetric> = source
            .glyphs
            .iter()
            .map(|glyph| LongMetric {
                advance: glyph.advance.round().clamp(0.0, u16::MAX as f64) as u16,
                side_bearing: 0,
            })
            .collect();
        let mut hhea = Hhea::default();
        hhea.number_of_h_metrics = glyph_count;
        let upem_i16 = i16::try_from(upem_u16).unwrap_or(1000);
        hhea.ascender = (upem_i16 / 5 * 4).into();
        hhea.descender = (-(upem_i16 / 5)).into();
        builder.add_table(&hhea).map_err(|e| e.to_string())?;
        builder
            .add_table(&Hmtx::new(metrics, Vec::new()))
            .map_err(|e| e.to_string())?;

        let mappings: Vec<(char, GlyphId)> = source
            .glyphs
            .iter()
            .enumerate()
            .flat_map(|(gid, glyph)| {
                glyph.unicodes.iter().filter_map(move |cp| {
                    char::from_u32(*cp).map(|ch| (ch, GlyphId::new(gid as u32)))
                })
            })
            .collect();
        builder
            .add_table(&Cmap::from_mappings(mappings).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;

        if let Some(gsub) = compilation.gsub.as_ref() {
            builder.add_table(gsub).map_err(|e| e.to_string())?;
        }
        if let Some(gpos) = compilation.gpos.as_ref() {
            builder.add_table(gpos).map_err(|e| e.to_string())?;
        }
        if let Some(gdef) = compilation.gdef.as_ref() {
            builder.add_table(gdef).map_err(|e| e.to_string())?;
        }

        Ok(Self {
            bytes: builder.build(),
            names,
        })
    }

    /// Shape one run of text in one direction.
    pub fn shape(&self, text: &str, right_to_left: bool) -> Result<Vec<ShapedGlyph>, String> {
        self.shape_with_features(text, right_to_left, &[])
    }

    /// Shape with per-feature overrides: (tag, on) pairs handed to
    /// the shaper over the whole run; tags not listed keep the
    /// shaper's defaults.
    pub fn shape_with_features(
        &self,
        text: &str,
        right_to_left: bool,
        features: &[(String, bool)],
    ) -> Result<Vec<ShapedGlyph>, String> {
        self.shape_with_options(text, right_to_left, features, None, None)
    }

    /// Shape with explicit script and language overrides on top of
    /// the feature list. `script` is an ISO 15924 tag ("arab",
    /// "latn"); `language` a BCP 47 tag ("ur", "sd"). Either may be
    /// None to keep the direction-derived default. Language-specific
    /// OpenType rules (languagesystem arab URD) only fire when the
    /// language is set here.
    pub fn shape_with_options(
        &self,
        text: &str,
        right_to_left: bool,
        features: &[(String, bool)],
        script: Option<&str>,
        language: Option<&str>,
    ) -> Result<Vec<ShapedGlyph>, String> {
        let font = FontRef::new(&self.bytes).map_err(|e| format!("shaping font: {e}"))?;
        let data = ShaperData::new(&font);
        let shaper = data.shaper(&font).build();

        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(if right_to_left {
            Direction::RightToLeft
        } else {
            Direction::LeftToRight
        });
        match script.map(str::trim).filter(|s| s.len() == 4) {
            Some(tag) => {
                let mut bytes = [b' '; 4];
                bytes.copy_from_slice(tag.as_bytes());
                if let Some(script) =
                    harfrust::Script::from_iso15924_tag(harfrust::Tag::new(&bytes))
                {
                    buffer.set_script(script);
                }
            }
            None => {
                if right_to_left {
                    buffer.set_script(script::ARABIC);
                }
            }
        }
        if let Some(lang) = language
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .and_then(|l| l.parse::<harfrust::Language>().ok())
        {
            buffer.set_language(lang);
        }

        let overrides: Vec<harfrust::Feature> = features
            .iter()
            .filter(|(tag, _)| tag.len() == 4 && tag.is_ascii())
            .map(|(tag, on)| {
                let mut bytes = [b' '; 4];
                bytes.copy_from_slice(tag.as_bytes());
                harfrust::Feature::new(harfrust::Tag::new(&bytes), u32::from(*on), ..)
            })
            .collect();

        let shaped = shaper.shape(buffer, &overrides);
        let infos = shaped.glyph_infos();
        let positions = shaped.glyph_positions();
        Ok(infos
            .iter()
            .zip(positions.iter())
            .map(|(info, pos)| ShapedGlyph {
                glyph_id: info.glyph_id as u16,
                cluster: info.cluster,
                x_advance: pos.x_advance as f64,
                x_offset: pos.x_offset as f64,
                y_offset: pos.y_offset as f64,
            })
            .collect())
    }

    /// Glyph name for a shaped glyph id.
    pub fn glyph_name(&self, glyph_id: u16) -> Option<&str> {
        self.names.get(glyph_id as usize).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a shaping source out of the bundled test UFO: its glyph
    /// order, advances, codepoints and features.fea, exactly as the
    /// editor would hand them over.
    fn virtua_grotesk() -> ShapingSource {
        let ufo_dir = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../runebender-web/assets/test-fonts/VirtuaGrotesk-Regular.ufo"
        );
        let font = norad::Font::load(ufo_dir).expect("test UFO loads");
        let features = std::fs::read_to_string(format!("{ufo_dir}/features.fea"))
            .expect("test UFO has features.fea");

        // .notdef first, then the font's own glyph order.
        let order: Vec<String> = std::iter::once(".notdef".to_string())
            .chain(
                font.layers
                    .default_layer()
                    .iter()
                    .map(|glyph| glyph.name().to_string())
                    .filter(|name| name != ".notdef"),
            )
            .collect();

        let glyphs = order
            .iter()
            .map(|name| {
                let glyph = font.layers.default_layer().get_glyph(name.as_str());
                ShapingGlyph {
                    name: name.clone(),
                    advance: glyph.map(|g| g.width).unwrap_or(0.0),
                    unicodes: glyph
                        .map(|g| g.codepoints.iter().map(|c| c as u32).collect())
                        .unwrap_or_default(),
                }
            })
            .collect();

        ShapingSource {
            units_per_em: font
                .font_info
                .units_per_em
                .map(|upem| *upem)
                .unwrap_or(1000.0),
            glyphs,
            features,
        }
    }

    fn shaped_names(font: &ShapingFont, text: &str, rtl: bool) -> Vec<String> {
        font.shape(text, rtl)
            .expect("shaping succeeds")
            .iter()
            .map(|g| font.glyph_name(g.glyph_id).unwrap_or("?").to_string())
            .collect()
    }

    #[test]
    fn language_override_fires_language_specific_rules() {
        let mut src = virtua_grotesk();
        // A locl rule that only applies for Urdu: alef becomes the
        // lam_alef glyph (visually silly, unambiguous in a test).
        // languagesystem statements must precede every feature
        // block, so lift the originals out and re-emit them first.
        let (systems, rest): (Vec<&str>, Vec<&str>) = src
            .features
            .lines()
            .partition(|l| l.trim_start().starts_with("languagesystem"));
        src.features = format!(
            "{}\nlanguagesystem arab URD;\n{}\n\
             feature locl {{\n\
             script arab;\n\
             language URD;\n\
             sub alef-ar by lam_alef-ar;\n\
             }} locl;\n",
            systems.join("\n"),
            rest.join("\n")
        );
        let font = ShapingFont::build(&src).expect("shaping font builds");
        let urdu = font
            .shape_with_options("\u{0627}", true, &[], Some("arab"), Some("ur"))
            .expect("shaping succeeds");
        let names: Vec<_> = urdu
            .iter()
            .map(|g| font.glyph_name(g.glyph_id).unwrap_or("?"))
            .collect();
        assert_eq!(names, ["lam_alef-ar"]);
        // Without the language, the default rules leave alef alone.
        assert_eq!(shaped_names(&font, "\u{0627}", true), ["alef-ar"]);
    }

    #[test]
    fn lam_alef_shapes_to_the_ligature() {
        let font = ShapingFont::build(&virtua_grotesk()).expect("shaping font builds");
        // لا — the required ligature, which positional forms alone cannot
        // produce. It is one glyph, not two.
        assert_eq!(
            shaped_names(&font, "\u{0644}\u{0627}", true),
            ["lam_alef-ar"]
        );
    }

    #[test]
    fn arabic_positional_forms_come_from_the_font() {
        let font = ShapingFont::build(&virtua_grotesk()).expect("shaping font builds");
        // بب — the font's own init/fina features choose the forms, not
        // our joining table. Glyphs come back in visual order, so for RTL
        // the final form (leftmost) is first.
        assert_eq!(
            shaped_names(&font, "\u{0628}\u{0628}", true),
            ["beh-ar.fina", "beh-ar.init"]
        );
    }

    #[test]
    fn calt_fires_so_contextual_alternates_are_visible_in_the_editor() {
        // Arabic dot placement depends on what follows: a beh before alef
        // wants its dot centred, a beh before another dotted letter wants it
        // out of the way. That is a calt rule, and it is only worth writing
        // if the editor applies it — otherwise the font would look right
        // everywhere except where it is drawn.
        let mut source = virtua_grotesk();
        source.features.push_str(
            "\nfeature calt {\nscript arab;\nlanguage dflt;\n\
             sub beh-ar.init' alef-ar.fina by beh-ar.fina;\n} calt;\n",
        );
        let font = ShapingFont::build(&source).expect("shaping font builds");
        // با — visual order, so the alef comes back first
        assert_eq!(
            shaped_names(&font, "\u{0628}\u{0627}", true),
            ["alef-ar.fina", "beh-ar.fina"],
            "calt did not fire: contextual alternates would be invisible here"
        );
    }

    #[test]
    fn latin_shapes_one_glyph_per_character() {
        let font = ShapingFont::build(&virtua_grotesk()).expect("shaping font builds");
        assert_eq!(shaped_names(&font, "Ab", false), ["A", "b"]);
    }

    #[test]
    fn shaped_glyphs_carry_advances_and_clusters() {
        let font = ShapingFont::build(&virtua_grotesk()).expect("shaping font builds");
        let shaped = font
            .shape("\u{0644}\u{0627}", true)
            .expect("shaping succeeds");
        assert_eq!(shaped.len(), 1);
        // The ligature stands for both characters, and reports the
        // cluster of the first.
        assert_eq!(shaped[0].cluster, 0);
        assert!(shaped[0].x_advance > 0.0);
    }

    #[test]
    fn a_broken_feature_file_is_an_error_not_a_panic() {
        let mut source = virtua_grotesk();
        source.features = "feature liga { sub nonexistent by alsoMissing; } liga;".into();
        assert!(ShapingFont::build(&source).is_err());
    }
}

/// Where a failed feature-file compile goes. Silent in the browser
/// console rather than fatal: the file will not compile halfway through
/// an edit, and typing has to keep working.
pub fn log_shaping_failure(message: &str) {
    // Hosts that want browser-console output can wrap this; core
    // stays free of platform crates.
    let _ = message;
}

// Copyright 2026 the Runebender Xix Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The font model: a loaded font plus a denormalized per-glyph cache
//! for painting (outline, ink box, advance, mark). One master for now.

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use kurbo::{BezPath, Rect};
use runebender_core::category::GlyphCategory;
use runebender_core::glyph_paths;
use runebender_core::theme_oklch::{load_theme, mark_label_for_glyph};

/// Everything the grid and previews need for one glyph, without touching norad.
pub struct GlyphEntry {
    pub name: String,
    pub codepoint: Option<char>,
    pub advance: f64,
    /// Full outline (contours plus resolved components), design space.
    pub outline: Arc<BezPath>,
    /// Ink bounding box of the outline (empty for blank glyphs).
    pub ink: Rect,
    pub mark: Option<String>,
    pub category: GlyphCategory,
}

pub struct FontModel {
    pub font: Arc<norad::Font>,
    pub source: PathBuf,
    pub glyphs: Vec<GlyphEntry>,
    pub units_per_em: f64,
    pub ascender: f64,
    pub descender: f64,
    pub x_height: f64,
    pub cap_height: f64,
    /// All masters (from a designspace); a single UFO has one.
    masters: Vec<norad::Font>,
    pub master_names: Vec<String>,
    pub master_paths: Vec<PathBuf>,
    pub active: usize,
}

impl FontModel {
    pub fn open(path: &FsPath) -> Result<Self, String> {
        if path.extension().is_some_and(|e| e == "designspace") {
            let doc = norad::designspace::DesignSpaceDocument::load(path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let dir = path.parent().unwrap_or(FsPath::new(".")).to_path_buf();
            let mut masters = Vec::new();
            let mut names = Vec::new();
            let mut paths = Vec::new();
            for src in &doc.sources {
                let ufo_path = dir.join(&src.filename);
                let font = norad::Font::load(&ufo_path)
                    .map_err(|e| format!("{}: {e}", ufo_path.display()))?;
                names.push(
                    src.name.clone().unwrap_or_else(|| {
                        font.font_info.style_name.clone().unwrap_or_else(|| "master".into())
                    }),
                );
                paths.push(ufo_path);
                masters.push(font);
            }
            if masters.is_empty() {
                return Err("designspace has no sources".into());
            }
            Ok(Self::from_masters(masters, names, paths, 0))
        } else {
            let font = norad::Font::load(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let name = font.font_info.style_name.clone().unwrap_or_else(|| "Regular".into());
            Ok(Self::from_masters(vec![font], vec![name], vec![path.to_path_buf()], 0))
        }
    }

    fn from_masters(
        masters: Vec<norad::Font>,
        master_names: Vec<String>,
        master_paths: Vec<PathBuf>,
        active: usize,
    ) -> Self {
        let source = master_paths[active].clone();
        let mut model = Self::from_font(masters[active].clone(), source);
        model.masters = masters;
        model.master_names = master_names;
        model.master_paths = master_paths;
        model.active = active;
        model
    }

    /// Switch the active master, saving the current in-memory edits back to it.
    pub fn set_active(&mut self, index: usize) {
        if index >= self.masters.len() || index == self.active {
            return;
        }
        // Preserve edits to the current master.
        self.masters[self.active] = (*self.font).clone();
        let rebuilt = Self::from_font(self.masters[index].clone(), self.master_paths[index].clone());
        self.font = rebuilt.font;
        self.glyphs = rebuilt.glyphs;
        self.units_per_em = rebuilt.units_per_em;
        self.ascender = rebuilt.ascender;
        self.descender = rebuilt.descender;
        self.x_height = rebuilt.x_height;
        self.cap_height = rebuilt.cap_height;
        self.source = self.master_paths[index].clone();
        self.active = index;
    }

    pub fn from_font(font: norad::Font, source: PathBuf) -> Self {
        let (upm, ascender, descender, x_height, cap_height) = {
            let info = &font.font_info;
            let upm = info.units_per_em.map(|u| u.as_f64()).unwrap_or(1000.0);
            (
                upm,
                info.ascender.unwrap_or(upm * 0.8),
                info.descender.unwrap_or(-upm * 0.2),
                info.x_height.unwrap_or(upm * 0.5),
                info.cap_height.unwrap_or(upm * 0.7),
            )
        };
        let theme = load_theme("dark");
        let mut names: Vec<String> = font.iter_names().map(|n| n.to_string()).collect();
        names.sort();
        let glyphs = names
            .iter()
            .filter_map(|name| {
                let glyph = font.get_glyph(name)?;
                let outline = glyph_paths::glyph_to_bezpath(glyph, &font);
                let ink = if outline.elements().is_empty() {
                    Rect::ZERO
                } else {
                    outline.control_box()
                };
                let mark = theme.as_ref().and_then(|t| mark_label_for_glyph(glyph, t));
                Some(GlyphEntry {
                    name: name.clone(),
                    codepoint: glyph.codepoints.iter().next(),
                    advance: glyph.width,
                    outline: Arc::new(outline),
                    ink,
                    mark,
                    category: glyph
                        .codepoints
                        .iter()
                        .next()
                        .map(GlyphCategory::from_codepoint)
                        .unwrap_or(GlyphCategory::Other),
                })
            })
            .collect();
        Self {
            font: Arc::new(font),
            source,
            glyphs,
            units_per_em: upm,
            ascender,
            descender,
            x_height,
            cap_height,
            masters: Vec::new(),
            master_names: Vec::new(),
            master_paths: Vec::new(),
            active: 0,
        }
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.glyphs.iter().position(|g| g.name == name)
    }

    /// Create an empty glyph and refresh the cache. Returns false if the name
    /// is empty or already exists.
    pub fn add_glyph(&mut self, name: &str, default_advance: f64) -> bool {
        let name = name.trim();
        if name.is_empty() || self.font.get_glyph(name).is_some() {
            return false;
        }
        let Some(font) = Arc::get_mut(&mut self.font) else {
            return false;
        };
        let mut glyph = norad::Glyph::new(name);
        glyph.width = default_advance;
        // If the name is a single character, encode it.
        if name.chars().count() == 1 {
            if let Some(c) = name.chars().next() {
                glyph.codepoints = norad::Codepoints::new([c]);
            }
        }
        font.default_layer_mut().insert_glyph(glyph);
        let font = Arc::try_unwrap(std::mem::replace(&mut self.font, Arc::new(norad::Font::default())))
            .unwrap_or_else(|arc| (*arc).clone());
        let source = self.source.clone();
        *self = Self::from_font(font, source);
        true
    }

    /// Rename a glyph in the font (updates references) and refresh the cache.
    pub fn rename_glyph(&mut self, old: &str, new: &str) -> bool {
        let ok = Arc::get_mut(&mut self.font)
            .map(|f| runebender_core::glyph_ops::rename_glyph(f, old, new))
            .unwrap_or(false);
        if ok {
            let font = Arc::try_unwrap(std::mem::replace(&mut self.font, Arc::new(norad::Font::default())))
                .unwrap_or_else(|arc| (*arc).clone());
            let source = self.source.clone();
            *self = Self::from_font(font, source);
        }
        ok
    }

    pub fn save(&self) -> Result<(), String> {
        self.font
            .save(&self.source)
            .map_err(|e| format!("{}: {e}", self.source.display()))
    }

    /// Replace the glyph at `index` (in the font and the cache) after an edit.
    pub fn replace_glyph(&mut self, index: usize, glyph: norad::Glyph) {
        let Some(entry) = self.glyphs.get_mut(index) else {
            return;
        };
        // Update the font's copy so component references and saving stay correct.
        if let Some(slot) = Arc::get_mut(&mut self.font).and_then(|f| f.get_glyph_mut(&entry.name)) {
            *slot = glyph.clone();
        }
        let outline = glyph_paths::glyph_to_bezpath(&glyph, &self.font);
        entry.ink = if outline.elements().is_empty() {
            Rect::ZERO
        } else {
            outline.control_box()
        };
        entry.advance = glyph.width;
        entry.outline = Arc::new(outline);
    }
}

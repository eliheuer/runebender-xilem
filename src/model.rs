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
#[derive(Clone)]
pub struct Axis {
    pub name: String,
    pub tag: String,
    pub min: f64,
    pub default: f64,
    pub max: f64,
    /// avar-style piecewise map, (user_input, design_output) pairs. Empty = identity.
    pub map: Vec<(f64, f64)>,
}

impl Axis {
    /// Map a user-coordinate value to design coordinates via the piecewise map.
    pub fn user_to_design(&self, v: f64) -> f64 {
        if self.map.len() < 2 {
            return v;
        }
        let m = &self.map;
        if v <= m[0].0 {
            return m[0].1;
        }
        if v >= m[m.len() - 1].0 {
            return m[m.len() - 1].1;
        }
        for w in m.windows(2) {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            if v >= x0 && v <= x1 {
                let t = if (x1 - x0).abs() < 1e-9 { 0.0 } else { (v - x0) / (x1 - x0) };
                return y0 + t * (y1 - y0);
            }
        }
        v
    }

    /// Inverse of `user_to_design`: map a design-coordinate value back to
    /// user coordinates via the piecewise map. Identity when unmapped.
    pub fn design_to_user(&self, v: f64) -> f64 {
        if self.map.len() < 2 {
            return v;
        }
        let m = &self.map;
        if v <= m[0].1 {
            return m[0].0;
        }
        if v >= m[m.len() - 1].1 {
            return m[m.len() - 1].0;
        }
        for w in m.windows(2) {
            let (x0, y0) = w[0];
            let (x1, y1) = w[1];
            let (lo, hi) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
            if v >= lo && v <= hi {
                let t = if (y1 - y0).abs() < 1e-9 { 0.0 } else { (v - y0) / (y1 - y0) };
                return x0 + t * (x1 - x0);
            }
        }
        v
    }
}

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
    pub axes: Vec<Axis>,
    pub master_locations: Vec<std::collections::HashMap<String, f64>>,
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
            let axes: Vec<Axis> = doc.axes.iter().map(|a| Axis {
                name: a.name.clone(),
                tag: a.tag.clone(),
                min: a.minimum.unwrap_or(a.default) as f64,
                default: a.default as f64,
                max: a.maximum.unwrap_or(a.default) as f64,
                map: a.map.as_ref().map(|ms| {
                    let mut v: Vec<(f64, f64)> = ms.iter().map(|m| (m.input as f64, m.output as f64)).collect();
                    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                    v
                }).unwrap_or_default(),
            }).collect();
            let master_locations: Vec<std::collections::HashMap<String, f64>> = doc.sources.iter().map(|src| {
                src.location.iter().filter_map(|d| d.xvalue.map(|v| (d.name.clone(), v as f64))).collect()
            }).collect();
            let mut model = Self::from_masters(masters, names, paths, 0);
            model.axes = axes;
            model.master_locations = master_locations;
            Ok(model)
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
            axes: Vec::new(),
            master_locations: Vec::new(),
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

    pub fn save(&mut self) -> Result<(), String> {
        // Flush the active master's edits back into the masters list, then
        // save every master to its UFO.
        if self.active < self.masters.len() {
            self.masters[self.active] = (*self.font).clone();
        }
        for (font, path) in self.masters.iter().zip(self.master_paths.iter()) {
            font.save(path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        Ok(())
    }

    /// The given master's axis location in USER coordinates, one per axis,
    /// mapping its stored design-coord location back through the axis map.
    pub fn master_axis_values(&self, index: usize) -> Vec<f64> {
        let loc = self.master_locations.get(index);
        self.axes
            .iter()
            .map(|ax| match loc.and_then(|l| l.get(&ax.name)) {
                Some(d) => ax.design_to_user(*d),
                None => ax.default,
            })
            .collect()
    }

    /// Interpolate `glyph_name` at the given user-unit axis location. Returns
    /// the interpolated outline (design space) or None if incompatible.
    /// Composite glyphs interpolate both their component offsets and, through
    /// recursion, each component's base outline.
    pub fn interpolate_outline(
        &self,
        glyph_name: &str,
        location: &std::collections::HashMap<String, f64>,
    ) -> Option<BezPath> {
        if self.masters.len() < 2 || self.axes.is_empty() {
            return None;
        }
        // Normalized master locations and target, shared by every recursion.
        // Master locations are stored in design coords; the target arrives in
        // user coords. Both normalize against the design-space extents so
        // avar-mapped axes interpolate correctly.
        use runebender_core::var_model::normalize_value;
        let norm_design = |loc: &std::collections::HashMap<String, f64>| -> std::collections::HashMap<String, f64> {
            self.axes.iter().map(|ax| {
                let dmin = ax.user_to_design(ax.min);
                let ddef = ax.user_to_design(ax.default);
                let dmax = ax.user_to_design(ax.max);
                let v = loc.get(&ax.name).copied().unwrap_or(ddef);
                (ax.name.clone(), normalize_value(v, dmin, ddef, dmax))
            }).collect()
        };
        let target_design: std::collections::HashMap<String, f64> = self.axes.iter().map(|ax| {
            let v = location.get(&ax.name).copied().unwrap_or(ax.default);
            (ax.name.clone(), ax.user_to_design(v))
        }).collect();
        let locations: Vec<_> = self.master_locations.iter().map(&norm_design).collect();
        let target = norm_design(&target_design);
        self.interpolate_outline_depth(glyph_name, &locations, &target, 0)
    }

    fn interpolate_outline_depth(
        &self,
        glyph_name: &str,
        locations: &[std::collections::HashMap<String, f64>],
        target: &std::collections::HashMap<String, f64>,
        depth: u8,
    ) -> Option<BezPath> {
        use runebender_core::var_model::VariationModel;
        if depth > 8 {
            return None;
        }
        let glyphs: Vec<&norad::Glyph> = self
            .masters
            .iter()
            .map(|f| f.get_glyph(glyph_name))
            .collect::<Option<Vec<_>>>()?;
        // Value vector: width, then each contour point x/y, then each
        // component's x/y offset (matching runebender-web's interpolateGlif).
        let vector = |g: &norad::Glyph| -> Vec<f64> {
            let mut v = vec![g.width];
            for c in &g.contours {
                for p in &c.points {
                    v.push(p.x);
                    v.push(p.y);
                }
            }
            for comp in &g.components {
                v.push(comp.transform.x_offset);
                v.push(comp.transform.y_offset);
            }
            v
        };
        let vectors: Vec<Vec<f64>> = glyphs.iter().map(|g| vector(g)).collect();
        let width = vectors[0].len();
        if vectors.iter().any(|v| v.len() != width) {
            return None; // incompatible masters: fall back to no preview
        }
        let model = VariationModel::new(locations);
        let out = model.interpolate(&vectors, target);
        // Rebuild on the active master's structure as a template.
        let mut g = glyphs.get(self.active).copied()?.clone();
        let mut i = 1usize; // skip width
        for c in &mut g.contours {
            for p in &mut c.points {
                p.x = out[i];
                p.y = out[i + 1];
                i += 2;
            }
        }
        let mut path = glyph_paths::contours_to_bezpath(&g);
        // Resolve components: interpolate each base recursively and apply the
        // interpolated offset (keeping the template's scale/skew).
        for comp in &g.components {
            let (dx, dy) = (out[i], out[i + 1]);
            i += 2;
            let mut xform = comp.transform.clone();
            xform.x_offset = dx;
            xform.y_offset = dy;
            if let Some(base) =
                self.interpolate_outline_depth(&comp.base, locations, target, depth + 1)
            {
                path.extend((glyph_paths::component_affine(&xform) * base).elements().iter().copied());
            }
        }
        Some(path)
    }

    /// Outline and advance of `glyph_name` in master `index`, for the
    /// layer thumbnails in the inspector.
    pub fn master_glyph(&self, index: usize, glyph_name: &str) -> Option<(BezPath, f64)> {
        let font = self.masters.get(index)?;
        let glyph = font.get_glyph(glyph_name)?;
        Some((glyph_paths::glyph_to_bezpath(glyph, font), glyph.width))
    }

    /// Short display names for the masters: the common family prefix is
    /// dropped, so "Bricolage Grotesque 96pt ExtraBold" reads as
    /// "96pt ExtraBold" in a narrow inspector.
    pub fn short_master_names(&self) -> Vec<String> {
        let names = &self.master_names;
        if names.len() < 2 {
            return names.clone();
        }
        // The longest common prefix, cut back to a word boundary.
        let first = names[0].as_str();
        let mut cut = first.len();
        for other in &names[1..] {
            let common = first
                .char_indices()
                .zip(other.chars())
                .take_while(|((_, a), b)| a == b)
                .map(|((i, a), _)| i + a.len_utf8())
                .last()
                .unwrap_or(0);
            cut = cut.min(common);
        }
        let cut = first[..cut].rfind(' ').map(|i| i + 1).unwrap_or(0);
        names
            .iter()
            .map(|n| {
                let short = n[cut.min(n.len())..].trim();
                if short.is_empty() { n.clone() } else { short.to_string() }
            })
            .collect()
    }

    /// Outlines of `glyph_name` in the masters listed in `which`, for the
    /// ghost overlay. The inspector's Layers section owns that set.
    pub fn reference_outlines(&self, glyph_name: &str, which: &std::collections::HashSet<usize>) -> Vec<BezPath> {
        self.masters
            .iter()
            .enumerate()
            .filter(|(i, _)| which.contains(i) && *i != self.active)
            .filter_map(|(_, font)| {
                font.get_glyph(glyph_name)
                    .map(|g| glyph_paths::glyph_to_bezpath(g, font))
            })
            .collect()
    }

    /// Outlines of `glyph_name` in every master except the active one,
    /// for the ghost overlay.
    #[allow(dead_code)]
    pub fn ghost_outlines(&self, glyph_name: &str) -> Vec<BezPath> {
        self.masters
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != self.active)
            .filter_map(|(_, font)| {
                font.get_glyph(glyph_name)
                    .map(|g| glyph_paths::glyph_to_bezpath(g, font))
            })
            .collect()
    }

    /// The name of the UFO background layer, if the font has one.
    ///
    /// UFOs in the wild use either spelling, and the web editor reads
    /// both, so this does too.
    fn background_layer(font: &norad::Font) -> Option<norad::Name> {
        for candidate in ["public.background", "background"] {
            if let Ok(name) = norad::Name::new(candidate)
                && font.layers.get(&name).is_some()
            {
                return Some(name);
            }
        }
        None
    }

    /// The glyph's outline in the background layer, as a path.
    pub fn background_outline(&self, glyph: &str) -> Option<BezPath> {
        let layer = Self::background_layer(&self.font)?;
        let background = self.font.layers.get(&layer)?.get_glyph(glyph)?;
        Some(glyph_paths::glyph_to_bezpath(background, &self.font))
    }

    /// Copy contours into the glyph's background layer, creating the
    /// layer the first time.
    pub fn send_to_background(&mut self, glyph: &str, contours: Vec<norad::Contour>, width: f64) {
        let Some(font) = Arc::get_mut(&mut self.font) else {
            return;
        };
        let Ok(layer) = font.layers.get_or_create_layer("public.background") else {
            return;
        };
        let mut background = norad::Glyph::new(glyph);
        background.width = width;
        background.contours = contours;
        layer.insert_glyph(background);
    }

    /// The contours held in the background layer for this glyph.
    pub fn background_contours(&self, glyph: &str) -> Option<Vec<norad::Contour>> {
        let layer = Self::background_layer(&self.font)?;
        let background = self.font.layers.get(&layer)?.get_glyph(glyph)?;
        Some(background.contours.clone())
    }

    /// Empty the glyph's background layer.
    pub fn clear_background(&mut self, glyph: &str) {
        let Some(layer) = Self::background_layer(&self.font) else {
            return;
        };
        if let Some(font) = Arc::get_mut(&mut self.font)
            && let Some(layer) = font.layers.get_mut(&layer)
        {
            layer.remove_glyph(glyph);
        }
    }

    /// Another glyph's outline, for the reference underlay.
    pub fn glyph_outline(&self, glyph: &str) -> Option<Arc<BezPath>> {
        let index = self.index_of(glyph)?;
        Some(self.glyphs[index].outline.clone())
    }

    /// The kerning group this glyph belongs to on one side, if any.
    ///
    /// `first_side` is the left side in left-to-right text: `public.kern1`.
    pub fn kern_group(&self, glyph: &str, first_side: bool) -> String {
        runebender_core::glyph_ops::kern_group(&self.font, glyph, first_side)
            .map(|name| name.to_string())
            .unwrap_or_default()
    }

    /// Put the glyph in a kerning group on one side, in every master.
    ///
    /// Kerning groups are font-wide, and a designspace's masters have to
    /// agree about them or the kerning will not interpolate, so this
    /// writes all of them rather than only the active one.
    pub fn set_kern_group(&mut self, glyph: &str, first_side: bool, group: &str) -> bool {
        let mut changed = false;
        if let Some(font) = Arc::get_mut(&mut self.font) {
            changed |= runebender_core::glyph_ops::set_kern_group(font, glyph, first_side, group);
        }
        for master in &mut self.masters {
            runebender_core::glyph_ops::set_kern_group(master, glyph, first_side, group);
        }
        changed
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

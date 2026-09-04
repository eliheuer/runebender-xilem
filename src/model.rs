// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The font model: core's `Project` (one `Master` per source, each
//! with its own undo pile), plus the denormalized per-glyph cache the
//! grid paints from. The shell reads the active master through
//! [`FontModel::font`] and writes through [`FontModel::font_mut`]; the
//! masters, axes and locations are the project's.

use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;

use kurbo::{BezPath, Rect};
use runebender_core::analysis::category::GlyphCategory;
use runebender_core::document::project::{Master, Project};
use runebender_core::outline::glyph_paths;

/// One designspace axis, in user coordinates with its map into design
/// coordinates. Core's `AxisInfo` keeps only the design-space extents;
/// the map is read off the designspace document here.
#[derive(Clone, Debug)]
pub(crate) struct Axis {
    pub name: String,
    pub tag: String,
    pub min: f64,
    pub default: f64,
    pub max: f64,
    /// avar-style piecewise map, (`user_input`, `design_output`) pairs. Empty = identity.
    pub map: Vec<(f64, f64)>,
}

impl Axis {
    /// Map a user-coordinate value to design coordinates via the piecewise map.
    pub(crate) fn user_to_design(&self, v: f64) -> f64 {
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
                let t = if (x1 - x0).abs() < 1e-9 {
                    0.0
                } else {
                    (v - x0) / (x1 - x0)
                };
                return y0 + t * (y1 - y0);
            }
        }
        v
    }

    /// Inverse of `user_to_design`: map a design-coordinate value back to
    /// user coordinates via the piecewise map. Identity when unmapped.
    pub(crate) fn design_to_user(&self, v: f64) -> f64 {
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
                let t = if (y1 - y0).abs() < 1e-9 {
                    0.0
                } else {
                    (v - y0) / (y1 - y0)
                };
                return x0 + t * (x1 - x0);
            }
        }
        v
    }
}
/// Everything the grid and previews need for one glyph, without touching norad.
#[derive(Clone)]
pub(crate) struct GlyphEntry {
    pub name: String,
    pub codepoint: Option<char>,
    pub advance: f64,
    /// Full outline (contours plus resolved components), shared with core's entry.
    pub outline: Arc<BezPath>,
    /// Ink box of the outline (zero when empty).
    pub ink: Rect,
    pub mark: Option<String>,
    pub category: GlyphCategory,
}

impl GlyphEntry {
    fn from_core(entry: &runebender_core::document::project::GlyphEntry) -> Self {
        Self {
            name: entry.name.to_string(),
            codepoint: entry.codepoint,
            advance: entry.advance,
            outline: entry.path.clone(),
            ink: if entry.path.elements().is_empty() {
                Rect::ZERO
            } else {
                entry.ink
            },
            mark: entry.mark.as_deref().map(str::to_string),
            category: entry
                .codepoint
                .map(GlyphCategory::from_codepoint)
                .unwrap_or(GlyphCategory::Other),
        }
    }
}

pub(crate) struct FontModel {
    /// Core's project: the masters, the designspace, the undo piles.
    pub project: Project,
    /// The active master's glyphs, in core's order, so an index here
    /// is an index into the master.
    pub glyphs: Vec<GlyphEntry>,
    pub axes: Vec<Axis>,
}

impl FontModel {
    pub(crate) fn open(path: &FsPath) -> Result<Self, String> {
        let project = Project::load(path)?;
        Ok(Self::from_project(project))
    }

    pub(crate) fn from_project(project: Project) -> Self {
        let axes = project
            .ds_doc
            .as_ref()
            .map(|doc| {
                doc.axes
                    .iter()
                    .map(|a| Axis {
                        name: a.name.clone(),
                        tag: a.tag.clone(),
                        min: a.minimum.unwrap_or(a.default) as f64,
                        default: a.default as f64,
                        max: a.maximum.unwrap_or(a.default) as f64,
                        map: a
                            .map
                            .as_ref()
                            .map(|ms| {
                                let mut v: Vec<(f64, f64)> = ms
                                    .iter()
                                    .map(|m| (m.input as f64, m.output as f64))
                                    .collect();
                                v.sort_by(|a, b| {
                                    a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)
                                });
                                v
                            })
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut model = Self {
            project,
            glyphs: Vec::new(),
            axes,
        };
        model.rebuild_cache();
        model
    }

    /// Rebuild every shell entry from the active master's cache.
    pub(crate) fn rebuild_cache(&mut self) {
        self.glyphs = self
            .master()
            .glyphs
            .iter()
            .map(GlyphEntry::from_core)
            .collect();
    }

    /// Refresh one shell entry from the master's, after an edit.
    pub(crate) fn refresh_entry(&mut self, index: usize) {
        if let Some(entry) = self.master().glyphs.get(index) {
            let fresh = GlyphEntry::from_core(entry);
            if let Some(slot) = self.glyphs.get_mut(index) {
                *slot = fresh;
            }
        }
    }

    // ---- the active master ----

    pub(crate) fn master(&self) -> &Master {
        self.project.active_font()
    }

    pub(crate) fn master_mut(&mut self) -> &mut Master {
        self.project.active_font_mut()
    }

    /// The active master's font, to read.
    pub(crate) fn font(&self) -> &norad::Font {
        &self.master().font
    }

    /// The active master's font, to write. Marks the master dirty; a
    /// caller that changes glyph outlines refreshes the cache after.
    pub(crate) fn font_mut(&mut self) -> &mut norad::Font {
        let master = self.master_mut();
        master.dirty = true;
        &mut master.font
    }

    pub(crate) fn source(&self) -> &FsPath {
        &self.master().source_path
    }

    pub(crate) fn active(&self) -> usize {
        self.project.active
    }

    pub(crate) fn units_per_em(&self) -> f64 {
        self.master().units_per_em
    }

    pub(crate) fn ascender(&self) -> f64 {
        self.master().ascender
    }

    pub(crate) fn descender(&self) -> f64 {
        self.master().descender
    }

    pub(crate) fn master_names(&self) -> Vec<String> {
        self.project
            .master_names
            .iter()
            .map(|n| n.to_string())
            .collect()
    }

    pub(crate) fn master_name(&self, index: usize) -> String {
        self.project
            .master_names
            .get(index)
            .map(|n| n.to_string())
            .unwrap_or_default()
    }

    pub(crate) fn master_paths(&self) -> Vec<PathBuf> {
        self.project
            .masters
            .iter()
            .map(|m| m.source_path.clone())
            .collect()
    }

    /// Switch the active master. Each master keeps its own edits, so
    /// nothing is flushed; the cache is rebuilt for the new one.
    pub(crate) fn set_active(&mut self, index: usize) {
        if index >= self.project.masters.len() || index == self.project.active {
            return;
        }
        self.project.active = index;
        self.project.snap_location_to_master(index);
        self.rebuild_cache();
    }

    pub(crate) fn index_of(&self, name: &str) -> Option<usize> {
        self.master().name_map.get(name).copied()
    }

    /// Add an empty glyph to every master.
    ///
    /// Encoded when a codepoint is given, or when the name is a single
    /// character. A glyph that exists in one master and not another is a
    /// designspace that does not build, so this writes all of them.
    pub(crate) fn add_glyph(
        &mut self,
        name: &str,
        default_advance: f64,
        unicode: Option<u32>,
    ) -> bool {
        let name = name.trim();
        if name.is_empty() || self.font().get_glyph(name).is_some() {
            return false;
        }
        let codepoint = unicode.and_then(char::from_u32).or_else(|| {
            (name.chars().count() == 1)
                .then(|| name.chars().next())
                .flatten()
        });
        for master in &mut self.project.masters {
            if master.font.get_glyph(name).is_some() {
                continue;
            }
            if let Some(index) = master.add_glyph(name, default_advance)
                && let Some(c) = codepoint
            {
                master.edit_glyph(index, |g| g.codepoints = norad::Codepoints::new([c]));
            }
        }
        self.project.recheck_compat(name);
        self.rebuild_cache();
        true
    }

    /// Add every glyph in `targets` that the font does not have yet, and
    /// report how many were added.
    pub(crate) fn add_missing(&mut self, targets: &[(String, Option<u32>)]) -> usize {
        let advance = (self.units_per_em() * 0.5).round();
        let mut added = 0;
        for (name, unicode) in targets {
            if self.add_glyph(name, advance, *unicode) {
                added += 1;
            }
        }
        added
    }

    /// Rename a glyph, in every master.
    ///
    /// `runebender_core` does the work inside one font: the glyph, the
    /// components that place it, group memberships, and kerning keys on
    /// either side. This applies that to all the masters, because a
    /// designspace whose sources disagree about a glyph name does not
    /// build.
    pub(crate) fn rename_glyph(&mut self, old: &str, new: &str) -> bool {
        let mut renamed = false;
        for master in &mut self.project.masters {
            if runebender_core::document::font_ops::rename_glyph(&mut master.font, old, new) {
                master.dirty = true;
                master.history.clear_glyph(old);
                master.refresh_from_font();
                renamed = true;
            }
        }
        if renamed {
            self.rebuild_cache();
        }
        renamed
    }

    /// Save every master to its UFO.
    pub(crate) fn save(&mut self) -> Result<(), String> {
        for master in &mut self.project.masters {
            master
                .save()
                .map_err(|e| format!("{}: {e}", master.source_path.display()))?;
        }
        Ok(())
    }

    /// The given master's axis location in USER coordinates, one per axis,
    /// mapping its stored design-coord location back through the axis map.
    pub(crate) fn master_axis_values(&self, index: usize) -> Vec<f64> {
        let loc = self.project.master_locations.get(index);
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
    pub(crate) fn interpolate_outline(
        &self,
        glyph_name: &str,
        location: &std::collections::HashMap<String, f64>,
    ) -> Option<BezPath> {
        if self.project.masters.len() < 2 || self.axes.is_empty() {
            return None;
        }
        // Normalized master locations and target, shared by every recursion.
        // Master locations are stored in design coords; the target arrives in
        // user coords. Both normalize against the design-space extents so
        // avar-mapped axes interpolate correctly.
        use runebender_core::document::var_model::normalize_value;
        let norm_design = |loc: &std::collections::HashMap<String, f64>| -> std::collections::HashMap<String, f64> {
            self.axes.iter().map(|ax| {
                let dmin = ax.user_to_design(ax.min);
                let ddef = ax.user_to_design(ax.default);
                let dmax = ax.user_to_design(ax.max);
                let v = loc.get(&ax.name).copied().unwrap_or(ddef);
                (ax.name.clone(), normalize_value(v, dmin, ddef, dmax))
            }).collect()
        };
        let target_design: std::collections::HashMap<String, f64> = self
            .axes
            .iter()
            .map(|ax| {
                let v = location.get(&ax.name).copied().unwrap_or(ax.default);
                (ax.name.clone(), ax.user_to_design(v))
            })
            .collect();
        let locations: Vec<_> = self
            .project
            .master_locations
            .iter()
            .map(&norm_design)
            .collect();
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
        use runebender_core::document::var_model::VariationModel;
        if depth > 8 {
            return None;
        }
        let glyphs: Vec<&norad::Glyph> = self
            .project
            .masters
            .iter()
            .map(|m| m.font.get_glyph(glyph_name))
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
        let mut g = glyphs.get(self.project.active).copied()?.clone();
        let mut i = 1_usize; // skip width
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
            let mut xform = comp.transform;
            xform.x_offset = dx;
            xform.y_offset = dy;
            if let Some(base) =
                self.interpolate_outline_depth(&comp.base, locations, target, depth + 1)
            {
                path.extend(
                    (glyph_paths::component_affine(&xform) * base)
                        .elements()
                        .iter()
                        .copied(),
                );
            }
        }
        Some(path)
    }

    /// How many masters the family has.
    pub(crate) fn master_count(&self) -> usize {
        self.project.masters.len()
    }

    /// One master's font, the active one's edits included.
    pub(crate) fn master_font(&self, index: usize) -> Option<&norad::Font> {
        self.project.masters.get(index).map(|m| &m.font)
    }

    /// Runs `f` over every master's font, for a write that has to land
    /// on all of them (groups, kerning).
    pub(crate) fn for_each_master(&mut self, mut f: impl FnMut(&mut norad::Font)) {
        for master in &mut self.project.masters {
            f(&mut master.font);
            master.dirty = true;
        }
    }

    pub(crate) fn master_glyph(&self, index: usize, glyph_name: &str) -> Option<(BezPath, f64)> {
        let master = self.project.masters.get(index)?;
        let glyph = master.font.get_glyph(glyph_name)?;
        Some((
            glyph_paths::glyph_to_bezpath(glyph, &master.font),
            glyph.width,
        ))
    }

    /// Short display names for the masters: the common family prefix is
    /// dropped, so "Bricolage Grotesque 96pt `ExtraBold`" reads as
    /// "96pt `ExtraBold`" in a narrow inspector.
    pub(crate) fn short_master_names(&self) -> Vec<String> {
        let names = self.master_names();
        if names.len() < 2 {
            return names;
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
                if short.is_empty() {
                    n.clone()
                } else {
                    short.to_string()
                }
            })
            .collect()
    }

    /// Outlines of `glyph_name` in the chosen masters, for the
    /// ghost overlay. The inspector's Layers section owns that set.
    pub(crate) fn reference_outlines(
        &self,
        glyph_name: &str,
        which: &std::collections::HashSet<usize>,
    ) -> Vec<BezPath> {
        self.project
            .masters
            .iter()
            .enumerate()
            .filter(|(i, _)| which.contains(i) && *i != self.project.active)
            .filter_map(|(_, master)| {
                master
                    .font
                    .get_glyph(glyph_name)
                    .map(|g| glyph_paths::glyph_to_bezpath(g, &master.font))
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
    pub(crate) fn background_outline(&self, glyph: &str) -> Option<BezPath> {
        let font = self.font();
        let layer = Self::background_layer(font)?;
        let background = font.layers.get(&layer)?.get_glyph(glyph)?;
        Some(glyph_paths::glyph_to_bezpath(background, font))
    }

    /// Copy contours into the glyph's background layer, creating the
    /// layer the first time.
    pub(crate) fn send_to_background(
        &mut self,
        glyph: &str,
        contours: Vec<norad::Contour>,
        width: f64,
    ) {
        let font = self.font_mut();
        let Ok(layer) = font.layers.get_or_create_layer("public.background") else {
            return;
        };
        let mut background = norad::Glyph::new(glyph);
        background.width = width;
        background.contours = contours;
        layer.insert_glyph(background);
    }

    /// The contours held in the background layer for this glyph.
    pub(crate) fn background_contours(&self, glyph: &str) -> Option<Vec<norad::Contour>> {
        let font = self.font();
        let layer = Self::background_layer(font)?;
        let background = font.layers.get(&layer)?.get_glyph(glyph)?;
        Some(background.contours.clone())
    }

    /// Empty the glyph's background layer.
    pub(crate) fn clear_background(&mut self, glyph: &str) {
        let Some(layer) = Self::background_layer(self.font()) else {
            return;
        };
        if let Some(layer) = self.font_mut().layers.get_mut(&layer) {
            layer.remove_glyph(glyph);
        }
    }

    /// Another glyph's outline, for the reference underlay.
    pub(crate) fn glyph_outline(&self, glyph: &str) -> Option<Arc<BezPath>> {
        let index = self.index_of(glyph)?;
        Some(self.glyphs[index].outline.clone())
    }

    /// The kerning group this glyph belongs to on one side, if any.
    ///
    /// `first_side` is the left side in left-to-right text: `public.kern1`.
    pub(crate) fn kern_group(&self, glyph: &str, first_side: bool) -> String {
        runebender_core::document::font_ops::kern_group(self.font(), glyph, first_side)
            .map(|name| name.to_string())
            .unwrap_or_default()
    }

    /// Put the glyph in a kerning group on one side, in every master.
    ///
    /// Kerning groups are font-wide, and a designspace's masters have to
    /// agree about them or the kerning will not interpolate, so this
    /// writes all of them rather than only the active one.
    pub(crate) fn set_kern_group(&mut self, glyph: &str, first_side: bool, group: &str) -> bool {
        let mut changed = false;
        self.for_each_master(|font| {
            changed |=
                runebender_core::document::font_ops::set_kern_group(font, glyph, first_side, group);
        });
        changed
    }

    /// How many glyphs are marked for export.
    ///
    /// A glyph is skipped when its lib says so, which is how both Glyphs
    /// and the UFO spec record it. The GPUI build shows this count at
    /// the head of its filter list.
    pub(crate) fn exporting_count(&self) -> usize {
        let font = self.font();
        self.glyphs
            .iter()
            .filter(|entry| {
                font.get_glyph(&entry.name)
                    .and_then(|glyph| glyph.lib.get("public.skipExport"))
                    .and_then(|value| value.as_boolean())
                    != Some(true)
            })
            .count()
    }

    /// How many glyphs the masters disagree about, by core's check.
    pub(crate) fn incompatible_count(&self) -> usize {
        if self.project.masters.len() < 2 {
            return 0;
        }
        self.glyphs
            .iter()
            .filter(|entry| !self.project.check_compat(&entry.name))
            .count()
    }

    /// The font's headline metadata, as label and value pairs.
    ///
    /// The GPUI build shows this whenever no glyph is picked, and it is
    /// most of what its right panel holds in the overview. These are
    /// read here rather than edited: writing them back means a form per
    /// master and a rule about which values are per-master, which the
    /// editor does not have yet.
    pub(crate) fn info_rows(&self) -> Vec<(&'static str, String)> {
        let info = &self.font().font_info;
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        let number = |value: Option<f64>| value.map(|v| format!("{v:.0}")).unwrap_or_default();
        vec![
            ("Family Name", text(&info.family_name)),
            ("Style Name", text(&info.style_name)),
            ("UPM", number(info.units_per_em.map(|v| v.as_f64()))),
            ("Italic Angle", number(info.italic_angle)),
            ("Ascender", number(info.ascender)),
            ("Descender", number(info.descender)),
            ("x-Height", number(info.x_height)),
            ("Cap Height", number(info.cap_height)),
            (
                "typoAsc",
                number(info.open_type_os2_typo_ascender.map(f64::from)),
            ),
            (
                "typoDesc",
                number(info.open_type_os2_typo_descender.map(f64::from)),
            ),
            (
                "hheaAsc",
                number(info.open_type_hhea_ascender.map(f64::from)),
            ),
            (
                "hheaDesc",
                number(info.open_type_hhea_descender.map(f64::from)),
            ),
            (
                "winAsc",
                number(info.open_type_os2_win_ascent.map(f64::from)),
            ),
            (
                "winDesc",
                number(info.open_type_os2_win_descent.map(f64::from)),
            ),
        ]
    }

    /// Set the advance of a glyph that is not open in the editor.
    ///
    /// The overview panel edits the selected cell directly, so this works
    /// from an index rather than from a session. Only the active master
    /// changes: an advance is a per-master measurement.
    pub(crate) fn set_glyph_advance(&mut self, index: usize, width: f64) -> bool {
        let Some(entry) = self.glyphs.get(index) else {
            return false;
        };
        if entry.advance == width {
            return false;
        }
        self.master_mut().set_advance(index, width);
        self.refresh_entry(index);
        true
    }

    /// Set the codepoints of a glyph, in every master.
    ///
    /// Unlike the advance, a codepoint is not a per-master measurement.
    /// Masters that disagree about which character a glyph encodes
    /// produce a family that does not build.
    pub(crate) fn set_glyph_unicode(&mut self, index: usize, text: &str) -> bool {
        let Some(entry) = self.glyphs.get(index) else {
            return false;
        };
        let name = entry.name.clone();
        let trimmed = text.trim();
        let codepoints: Vec<char> = if trimmed.is_empty() {
            Vec::new()
        } else {
            match u32::from_str_radix(trimmed, 16)
                .ok()
                .and_then(char::from_u32)
            {
                Some(c) => vec![c],
                // Not a hex codepoint yet. Typing "004" on the way to
                // "0041" should not clear the glyph's encoding.
                None => return false,
            }
        };
        for master in &mut self.project.masters {
            if let Some(&i) = master.name_map.get(&name) {
                master.edit_glyph(i, |g| {
                    g.codepoints = norad::Codepoints::new(codepoints.iter().copied());
                });
            }
        }
        if let Some(entry) = self.glyphs.get_mut(index) {
            entry.codepoint = codepoints.first().copied();
        }
        true
    }

    /// Replace the glyph at `index` in the active master after an edit,
    /// and refresh its cache entry.
    pub(crate) fn replace_glyph(&mut self, index: usize, glyph: norad::Glyph) {
        if self.glyphs.get(index).is_none() {
            return;
        }
        self.master_mut().edit_glyph(index, |slot| *slot = glyph);
        self.refresh_entry(index);
        let name = self.glyphs[index].name.clone();
        self.project.recheck_compat(&name);
    }
}

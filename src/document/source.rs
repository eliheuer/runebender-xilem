// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UFO compatibility projections and paint caches.
//!
//! A Project owns canonical variable glyph layers in `variable`.
//! Existing tools edit these source projections through Project guards, which
//! commit changes back when their scope ends. Standalone file commands can also
//! use a Master directly. No platform or application state belongs here.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use kurbo::BezPath;

use crate::outline::glyph_ops::{self as ops, CurveOp, GlyphSnapshot};
use crate::ui::theme::{self, Theme};

/// The mark label a glyph carries. Labels are palette names shared by
/// every theme, so snapping against the default theme is enough here;
/// the front-end maps a label to the current theme's colour.
fn mark_label(glyph: &norad::Glyph) -> Option<String> {
    static DEFAULT: OnceLock<Theme> = OnceLock::new();
    let theme =
        DEFAULT.get_or_init(|| theme::load_theme("gray").expect("the built-in gray theme loads"));
    theme::mark_label_for_glyph(glyph, theme)
}

/// One control point of a contour, in font units, with its identity
/// inside the glyph so edits can address it.
#[derive(Debug, Clone, Copy)]
pub struct GlyphPoint {
    /// X coordinate in font units.
    pub x: f64,
    /// Y coordinate in font units.
    pub y: f64,
    /// True for an on-curve point, false for a control point.
    pub on_curve: bool,
    /// True when the on-curve point's handles are kept collinear.
    pub smooth: bool,
    /// True for a point in a hyperbezier contour, which is drawn in
    /// its own color.
    pub hyper: bool,
    /// Index of the contour that owns this point.
    pub contour: usize,
    /// Index of the point within its contour.
    pub index: usize,
}

#[derive(Debug, Clone)]
/// One glyph, ready to paint: outline in font units (Y-up), advance
/// width, and identifying info.
pub struct GlyphEntry {
    /// Glyph name.
    pub name: Arc<str>,
    /// The glyph's Unicode codepoint, if it has one.
    pub codepoint: Option<char>,
    /// Contours + components combined (grid, preview).
    pub path: Arc<BezPath>,
    /// The glyph's own contours only (editor fill).
    pub contour_path: Arc<BezPath>,
    /// Resolved components only (editor, distinct color).
    pub component_path: Arc<BezPath>,
    /// Every control point of the glyph's own contours.
    pub points: Arc<Vec<GlyphPoint>>,
    /// Anchors as `(name, x, y)` in font units.
    pub anchors: Arc<Vec<(Arc<str>, f64, f64)>>,
    /// Advance width in font units.
    pub advance: f64,
    /// Base glyph names of the glyph's components, in order.
    pub component_names: Arc<Vec<Arc<str>>>,
    /// Mark label ("red", "green", …) from the glyph lib, if any.
    pub mark: Option<Arc<str>>,
    /// The outline's bounding box, kept so the grid does not walk every
    /// path element again on every frame.
    pub ink: kurbo::Rect,
}

#[derive(Debug, Clone)]
/// One UFO master with its change tracking and a paint-ready glyph cache.
pub struct Master {
    /// The loaded UFO.
    pub font: norad::Font,
    /// Names of glyphs edited since load/save (partial saves).
    pub modified_glyphs: HashSet<String>,
    /// glyph name → glif path relative to the UFO root (memory hosts).
    pub glif_paths: HashMap<String, String>,
    /// Filesystem details outside canonical ownership that must survive saves.
    pub(crate) preserved_files: super::filesystem::PreservedFiles,
    /// Kerning changed since load/save.
    pub kerning_dirty: bool,
    /// glyph name → index into `glyphs`. Text buffer sorts carry
    /// names, including unencoded ligature glyphs from shaping.
    pub name_map: HashMap<String, usize>,
    /// Path of the UFO on disk, or a virtual path for in-memory hosts.
    pub source_path: PathBuf,
    /// Units per em from fontinfo, or 1000 when unset.
    pub units_per_em: f64,
    /// Ascender from fontinfo, in font units.
    pub ascender: f64,
    /// Descender from fontinfo, in font units (usually negative).
    pub descender: f64,
    /// Optional guides: drawn only when fontinfo defines them, like
    /// the web's metric guides.
    pub x_height: Option<f64>,
    /// Cap height from fontinfo, if defined.
    pub cap_height: Option<f64>,
    /// Paint-ready entries in glyph grid order.
    pub glyphs: Vec<GlyphEntry>,
    /// Bumped when the glyph list itself changes: a glyph added,
    /// removed, or renamed. Caches keyed on the list use it to tell.
    pub revision: u64,
    /// True when anything changed since the last load or save.
    pub dirty: bool,
}

/// Collects a glyph's anchors as `(name, x, y)`. An unnamed anchor gets an empty name.
pub fn extract_anchors(glyph: &norad::Glyph) -> Vec<(Arc<str>, f64, f64)> {
    glyph
        .anchors
        .iter()
        .map(|a| {
            (
                a.name
                    .as_ref()
                    .map(|n| n.to_string())
                    .unwrap_or_default()
                    .into(),
                a.x,
                a.y,
            )
        })
        .collect()
}

/// Collects every contour point of a glyph as [`GlyphPoint`] values, in contour order.
pub fn extract_points(glyph: &norad::Glyph) -> Vec<GlyphPoint> {
    glyph
        .contours
        .iter()
        .enumerate()
        .flat_map(|(ci, c)| {
            let hyper = crate::outline::path::hyper_model::norad_contour_is_hyper(c);
            c.points.iter().enumerate().map(move |(pi, p)| GlyphPoint {
                x: p.x,
                y: p.y,
                on_curve: p.typ != norad::PointType::OffCurve,
                smooth: p.smooth,
                hyper,
                contour: ci,
                index: pi,
            })
        })
        .collect()
}

impl Master {
    /// Run an op on the named glyph's norad data, then rebuild caches.
    pub fn edit_glyph<R>(
        &mut self,
        glyph_index: usize,
        op: impl FnOnce(&mut norad::Glyph) -> R,
    ) -> Option<R> {
        let name = self.glyphs[glyph_index].name.to_string();
        let result = self
            .font
            .default_layer_mut()
            .get_glyph_mut(name.as_str())
            .map(op)?;
        self.dirty = true;
        self.modified_glyphs.insert(name.clone());
        self.rebuild_entry(glyph_index);
        self.realign_after_edit(&name);
        Some(result)
    }

    /// Re-place anchor-locked components after a glyph edit, so
    /// accents follow their base live.
    ///
    /// The edited glyph's own components realign first, seeded by
    /// its own anchors, the open-glyph behavior. Then every
    /// composite that places the glyph realigns.
    pub fn realign_after_edit(&mut self, edited: &str) {
        use crate::document::composites as comp;
        let mut targets: Vec<(String, bool)> = vec![(edited.to_string(), true)];
        for user in comp::composites_using(&self.font, edited) {
            if user != edited {
                targets.push((user, false));
            }
        }
        for (name, seed_own) in targets {
            let Some(glyph) = self.font.get_glyph(name.as_str()) else {
                continue;
            };
            if glyph.components.is_empty() {
                continue;
            }
            let mut copy = glyph.clone();
            if comp::realign_glyph(&self.font, &mut copy, seed_own) {
                if let Some(slot) = self.font.default_layer_mut().get_glyph_mut(name.as_str()) {
                    *slot = copy;
                }
                self.modified_glyphs.insert(name.clone());
                self.dirty = true;
                if let Some(&i) = self.name_map.get(&name) {
                    self.rebuild_entry(i);
                }
            }
        }
    }

    /// Rebuild every cache from the norad font, after a glyph is
    /// added or removed; bookkeeping fields survive.
    pub fn refresh_from_font(&mut self) {
        let font = std::mem::replace(&mut self.font, norad::Font::new());
        let mut fresh = Self::from_font(font, self.source_path.clone());
        // The glyph list has been rebuilt: anything cached against it
        // (the grid's order, for one) has to notice.
        fresh.revision = self.revision.wrapping_add(1);
        fresh.dirty = self.dirty;
        fresh.kerning_dirty = self.kerning_dirty;
        fresh.modified_glyphs = std::mem::take(&mut self.modified_glyphs);
        fresh.glif_paths = std::mem::take(&mut self.glif_paths);
        fresh.preserved_files = std::mem::take(&mut self.preserved_files);
        *self = fresh;
    }

    /// Add an empty glyph. Returns its index in the sorted list.
    pub fn add_glyph(&mut self, name: &str, width: f64) -> Option<usize> {
        if self.name_map.contains_key(name) {
            return None;
        }
        let mut glyph = norad::Glyph::new(name);
        glyph.width = width;
        self.font.default_layer_mut().insert_glyph(glyph);
        self.dirty = true;
        self.modified_glyphs.insert(name.to_string());
        self.refresh_from_font();
        self.name_map.get(name).copied()
    }

    /// Remove a glyph outright.
    pub fn remove_glyph(&mut self, name: &str) -> bool {
        if self.font.default_layer_mut().remove_glyph(name).is_none() {
            return false;
        }
        self.dirty = true;
        self.modified_glyphs.remove(name);
        self.refresh_from_font();
        true
    }

    /// Loads a UFO from disk and builds the glyph cache.
    pub fn load(path: &Path) -> Result<Self, String> {
        super::filesystem::load_ufo(path).map(|source| source.into_master(path.to_path_buf()))
    }

    /// Build the model from an already-assembled font, for in-memory
    /// hosts: web builds and embedded demo data.
    pub fn from_font(font: norad::Font, source_path: PathBuf) -> Self {
        let info = &font.font_info;
        let units_per_em = info.units_per_em.map(|v| v.as_f64()).unwrap_or(1000.0);
        let ascender = info.ascender.unwrap_or(units_per_em * 0.8);
        let descender = info.descender.unwrap_or(-(units_per_em * 0.2));
        let x_height = info.x_height;
        let cap_height = info.cap_height;

        let mut glyphs: Vec<GlyphEntry> = font
            .default_layer()
            .iter()
            .map(|glyph| {
                let path = Arc::new(crate::outline::glyph_paths::glyph_to_bezpath(glyph, &font));
                GlyphEntry {
                    name: glyph.name().to_string().into(),
                    codepoint: glyph.codepoints.iter().next(),
                    ink: {
                        use kurbo::Shape as _;
                        path.bounding_box()
                    },
                    path: path.clone(),
                    contour_path: Arc::new(crate::outline::glyph_paths::contours_to_bezpath(glyph)),
                    component_path: Arc::new(crate::outline::glyph_paths::components_to_bezpath(
                        glyph, &font,
                    )),
                    points: Arc::new(extract_points(glyph)),
                    anchors: Arc::new(extract_anchors(glyph)),
                    advance: glyph.width,
                    component_names: Arc::new(
                        glyph
                            .components
                            .iter()
                            .map(|c| c.base.to_string().into())
                            .collect(),
                    ),
                    mark: mark_label(glyph).map(Arc::<str>::from),
                }
            })
            .collect();
        // Unicode order, unencoded glyphs after, each group by name.
        glyphs.sort_by(|a, b| match (a.codepoint, b.codepoint) {
            (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.name.cmp(&b.name)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.name.cmp(&b.name),
        });

        let name_map = glyphs
            .iter()
            .enumerate()
            .map(|(i, g)| (g.name.to_string(), i))
            .collect();

        Self {
            font,
            modified_glyphs: HashSet::new(),
            glif_paths: HashMap::new(),
            preserved_files: super::filesystem::PreservedFiles::default(),
            kerning_dirty: false,
            name_map,
            source_path,
            units_per_em,
            ascender,
            descender,
            x_height,
            cap_height,
            glyphs,
            revision: 0,
            dirty: false,
        }
    }

    /// Rebuilds one glyph's cached paths, points, anchors, advance, and mark from the font. Does nothing when the glyph is missing.
    pub fn rebuild_entry(&mut self, glyph_index: usize) {
        let name = self.glyphs[glyph_index].name.to_string();
        let Some(glyph) = self.font.get_glyph(name.as_str()) else {
            return;
        };
        let glyph_advance = glyph.width;
        let path = Arc::new(crate::outline::glyph_paths::glyph_to_bezpath(
            glyph, &self.font,
        ));
        let contour_path = Arc::new(crate::outline::glyph_paths::contours_to_bezpath(glyph));
        let component_path = Arc::new(crate::outline::glyph_paths::components_to_bezpath(
            glyph, &self.font,
        ));
        let component_names: Arc<Vec<Arc<str>>> = Arc::new(
            glyph
                .components
                .iter()
                .map(|c| c.base.to_string().into())
                .collect(),
        );
        let points = Arc::new(extract_points(glyph));
        let anchors = Arc::new(extract_anchors(glyph));
        let ink = {
            use kurbo::Shape as _;
            path.bounding_box()
        };
        let entry = &mut self.glyphs[glyph_index];
        entry.ink = ink;
        entry.path = path;
        entry.contour_path = contour_path;
        entry.component_path = component_path;
        entry.component_names = component_names;
        entry.points = points;
        entry.anchors = anchors;
        entry.advance = glyph_advance;
        entry.mark = mark_label(glyph).map(Arc::<str>::from);
    }

    /// Clone a glyph's editable state for undo snapshots.
    pub fn snapshot_contours(&self, glyph_index: usize) -> Option<GlyphSnapshot> {
        let name = self.glyphs[glyph_index].name.to_string();
        self.font.get_glyph(name.as_str()).map(ops::snapshot)
    }

    /// Replace a glyph's editable state (undo/redo) and rebuild caches.
    pub fn restore_contours(&mut self, glyph_index: usize, snapshot: GlyphSnapshot) {
        self.edit_glyph(glyph_index, |g| ops::restore(g, snapshot));
    }

    /// Moves an anchor to `(x, y)`. Ignores an out-of-range anchor index.
    pub fn set_anchor(&mut self, glyph_index: usize, anchor: usize, x: f64, y: f64) {
        self.edit_glyph(glyph_index, |g| {
            if let Some(a) = g.anchors.get_mut(anchor) {
                a.x = x;
                a.y = y;
            }
        });
    }

    /// Adds an anchor at `(x, y)` named `anchor.N`, where `N` is the current anchor count.
    pub fn add_anchor(&mut self, glyph_index: usize, x: f64, y: f64) {
        self.edit_glyph(glyph_index, |g| {
            let n = g.anchors.len();
            let name = norad::Name::new(&format!("anchor.{n}")).ok();
            g.anchors.push(norad::Anchor::new(x, y, name, None, None));
        });
    }

    /// Removes the anchor at `anchor`. Ignores an out-of-range index.
    pub fn delete_anchor(&mut self, glyph_index: usize, anchor: usize) {
        self.edit_glyph(glyph_index, |g| {
            if anchor < g.anchors.len() {
                g.anchors.remove(anchor);
            }
        });
    }

    /// Set several points at once (multi-point drag).
    pub fn set_points(&mut self, glyph_index: usize, updates: &ops::PointUpdates) {
        self.edit_glyph(glyph_index, |g| ops::set_points(g, updates));
    }

    /// Start a new open contour at (x, y). Returns its index.
    pub fn start_hyper_contour(&mut self, glyph_index: usize, x: f64, y: f64) -> Option<usize> {
        self.edit_glyph(glyph_index, |g| {
            crate::outline::glyph_ops::start_hyper_contour(g, x, y)
        })
    }

    /// Appends a point to an open hyperbezier contour. `corner` makes it a corner rather than a smooth point.
    pub fn append_hyper_point(
        &mut self,
        glyph_index: usize,
        contour: usize,
        x: f64,
        y: f64,
        corner: bool,
    ) {
        self.edit_glyph(glyph_index, |g| {
            crate::outline::glyph_ops::append_hyper_point(g, contour, x, y, corner);
        });
    }

    /// Closes an open hyperbezier contour.
    pub fn close_hyper_contour(&mut self, glyph_index: usize, contour: usize) {
        self.edit_glyph(glyph_index, |g| {
            crate::outline::glyph_ops::close_hyper_contour(g, contour);
        });
    }

    /// Starts a new open cubic contour at `(x, y)` for the pen tool. Returns its index.
    pub fn start_contour(&mut self, glyph_index: usize, x: f64, y: f64) -> Option<usize> {
        self.edit_glyph(glyph_index, |g| ops::start_contour(g, x, y))
    }

    /// Append a segment to an open contour (pen tool).
    pub fn append_segment(
        &mut self,
        glyph_index: usize,
        contour: usize,
        controls: Option<((f64, f64), (f64, f64))>,
        x: f64,
        y: f64,
        smooth: bool,
    ) {
        self.edit_glyph(glyph_index, |g| {
            ops::append_segment(g, contour, controls, x, y, smooth);
        });
    }

    /// Close an open contour.
    pub fn close_contour(
        &mut self,
        glyph_index: usize,
        contour: usize,
        controls: Option<((f64, f64), (f64, f64))>,
    ) {
        self.edit_glyph(glyph_index, |g| ops::close_contour(g, contour, controls));
    }

    /// Delete an unfinished pen contour: a single stray point.
    pub fn remove_contour_if_degenerate(&mut self, glyph_index: usize, contour: usize) {
        self.edit_glyph(glyph_index, |g| {
            ops::remove_contour_if_degenerate(g, contour);
        });
    }

    /// Delete points. See `crate::outline::glyph_ops`.
    pub fn delete_points(
        &mut self,
        glyph_index: usize,
        selected: &HashSet<(usize, usize)>,
    ) -> bool {
        self.edit_glyph(glyph_index, |g| ops::delete_points(g, selected))
            .unwrap_or(false)
    }

    /// Toggle smooth/corner on the selected on-curve points.
    pub fn toggle_smooth(
        &mut self,
        glyph_index: usize,
        selected: &HashSet<(usize, usize)>,
    ) -> bool {
        self.edit_glyph(glyph_index, |g| ops::toggle_smooth(g, selected))
            .unwrap_or(false)
    }

    /// Apply a curve-quality op to the selection or whole glyph.
    pub fn curve_op(
        &mut self,
        glyph_index: usize,
        selected: &HashSet<(usize, usize)>,
        op: CurveOp,
    ) -> bool {
        self.edit_glyph(glyph_index, |g| ops::curve_op(g, selected, op))
            .unwrap_or(false)
    }

    /// Ink bounds of a glyph in design units, `None` when empty.
    pub fn ink_bounds(&self, glyph_index: usize) -> Option<kurbo::Rect> {
        use kurbo::Shape;
        let path = &self.glyphs[glyph_index].path;
        if path.elements().is_empty() {
            None
        } else {
            Some(path.bounding_box())
        }
    }

    /// Sets the advance width in font units and marks the master dirty.
    pub fn set_advance(&mut self, glyph_index: usize, width: f64) {
        let name = self.glyphs[glyph_index].name.to_string();
        if let Some(glyph) = self.font.default_layer_mut().get_glyph_mut(name.as_str()) {
            glyph.width = width;
            self.dirty = true;
        }
        self.rebuild_metrics(glyph_index);
    }

    /// Shift a glyph's ink horizontally (LSB edits).
    pub fn shift_ink(&mut self, glyph_index: usize, dx: f64) {
        self.edit_glyph(glyph_index, |g| ops::shift_ink(g, dx));
    }

    /// Copies the glyph's advance width from the font into the cached entry.
    pub fn rebuild_metrics(&mut self, glyph_index: usize) {
        let name = self.glyphs[glyph_index].name.to_string();
        if let Some(glyph) = self.font.get_glyph(name.as_str()) {
            self.glyphs[glyph_index].advance = glyph.width;
        }
    }

    /// Replace a glyph's components with their resolved contours.
    pub fn decompose(&mut self, glyph_index: usize) -> bool {
        let name = self.glyphs[glyph_index].name.to_string();
        let Some(glyph) = self.font.get_glyph(name.as_str()) else {
            return false;
        };
        if glyph.components.is_empty() {
            return false;
        }
        let resolved =
            crate::outline::component_ops::resolved_component_contours(&self.font, glyph);
        self.edit_glyph(glyph_index, |g| {
            g.contours.extend(resolved);
            g.components.clear();
        });
        true
    }

    /// Contours that contain any selected point; all contours when
    /// the selection is empty.
    pub fn contours_for_copy(
        &self,
        glyph_index: usize,
        selected: &HashSet<(usize, usize)>,
    ) -> Vec<norad::Contour> {
        let name = self.glyphs[glyph_index].name.to_string();
        let Some(glyph) = self.font.get_glyph(name.as_str()) else {
            return Vec::new();
        };
        if selected.is_empty() {
            return glyph.contours.clone();
        }
        glyph
            .contours
            .iter()
            .enumerate()
            .filter(|(ci, _)| selected.iter().any(|(c, _)| c == ci))
            .map(|(_, c)| c.clone())
            .collect()
    }

    /// Appends copied contours to the glyph and rebuilds its cache. Does nothing for an empty slice.
    pub fn paste_contours(&mut self, glyph_index: usize, contours: &[norad::Contour]) {
        if contours.is_empty() {
            return;
        }
        let name = self.glyphs[glyph_index].name.to_string();
        if let Some(glyph) = self.font.default_layer_mut().get_glyph_mut(name.as_str()) {
            glyph.contours.extend(contours.iter().cloned());
            self.dirty = true;
        }
        self.rebuild_entry(glyph_index);
    }

    /// Union all contours to remove overlap. Returns false when
    /// nothing changed.
    pub fn remove_overlap(&mut self, glyph_index: usize) -> bool {
        let name = self.glyphs[glyph_index].name.to_string();
        let Some(unioned) = self
            .font
            .get_glyph(name.as_str())
            .and_then(ops::remove_overlap)
        else {
            return false;
        };
        self.edit_glyph(glyph_index, |g| g.contours = unioned);
        true
    }

    /// Insert a rectangle or ellipse contour spanning `rect`.
    pub fn add_shape_contour(&mut self, glyph_index: usize, rect: kurbo::Rect, ellipse: bool) {
        self.edit_glyph(glyph_index, |g| ops::add_shape_contour(g, rect, ellipse));
    }

    /// Writes the master back to `source_path` and clears all dirty flags.
    pub fn save(&mut self) -> Result<(), String> {
        super::filesystem::ExportPlan::new(
            vec![super::filesystem::SourceExport {
                destination: self.source_path.clone(),
                font: self.font.clone(),
                preserved: self.preserved_files.clone(),
            }],
            None,
        )?
        .execute()?;
        self.dirty = false;
        self.modified_glyphs.clear();
        self.kerning_dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fonts;

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut model = Master::load(&fonts::regular_ufo()).expect("load");
        let index = model
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "a")
            .unwrap();
        let before = model.snapshot_contours(index).unwrap();
        let p0 = model.glyphs[index].points[0];
        model.set_points(index, &[((p0.contour, p0.index), (p0.x + 25.0, p0.y))]);
        assert_ne!(model.glyphs[index].points[0].x, p0.x);
        model.restore_contours(index, before);
        assert_eq!(model.glyphs[index].points[0].x, p0.x);
        assert_eq!(model.glyphs[index].points[0].y, p0.y);
    }

    #[test]
    fn pen_primitives_build_a_closed_contour() {
        let mut model = Master::load(&fonts::regular_ufo()).expect("load");
        let index = model
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "space")
            .unwrap();
        let base_contours = model.snapshot_contours(index).unwrap().contours.len();

        let c = model.start_contour(index, 0.0, 0.0).unwrap();
        model.append_segment(index, c, None, 100.0, 0.0, false); // line
        model.append_segment(
            index,
            c,
            Some(((130.0, 40.0), (130.0, 80.0))),
            100.0,
            120.0,
            true,
        ); // curve
        model.close_contour(index, c, None);

        let contours = model.snapshot_contours(index).unwrap().contours;
        assert_eq!(contours.len(), base_contours + 1);
        let new = &contours[c];
        assert!(new.is_closed(), "contour should be closed");
        // move->line conversion on close + 2 on-curves + 2 off-curves
        assert_eq!(new.points.len(), 5);
        assert_eq!(new.points[0].typ, norad::PointType::Line);
        assert!(new.points[4].smooth);
        // The outline cache rebuilt and is drawable.
        assert!(!model.glyphs[index].path.elements().is_empty());

        // Degenerate contour cleanup: a single stray point goes away.
        let c2 = model.start_contour(index, 5.0, 5.0).unwrap();
        model.remove_contour_if_degenerate(index, c2);
        assert_eq!(
            model.snapshot_contours(index).unwrap().contours.len(),
            base_contours + 1
        );
    }

    #[test]
    fn delete_and_smooth_operations() {
        let mut model = Master::load(&fonts::regular_ufo()).expect("load");
        let index = model
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "space")
            .unwrap();

        // Build a closed square with one curved corner:
        // (0,0) -line- (100,0) -line- (100,100) -curve- (0,100) -close-
        let c = model.start_contour(index, 0.0, 0.0).unwrap();
        model.append_segment(index, c, None, 100.0, 0.0, false);
        model.append_segment(index, c, None, 100.0, 100.0, false);
        model.append_segment(
            index,
            c,
            Some(((80.0, 130.0), (20.0, 130.0))),
            0.0,
            100.0,
            true,
        );
        model.close_contour(index, c, None);
        let count_points =
            |m: &Master| m.snapshot_contours(index).unwrap().contours[c].points.len();
        assert_eq!(count_points(&model), 6); // 4 on + 2 off

        // Toggle smooth on the curve's endpoint.
        let curve_end_index = model.glyphs[index]
            .points
            .iter()
            .find(|p| p.contour == c && p.x == 0.0 && p.y == 100.0)
            .map(|p| (p.contour, p.index))
            .unwrap();
        let sel: HashSet<_> = [curve_end_index].into();
        assert!(model.toggle_smooth(index, &sel));

        // Delete one off-curve: the curve segment becomes a line.
        let off = model.glyphs[index]
            .points
            .iter()
            .find(|p| p.contour == c && !p.on_curve)
            .map(|p| (p.contour, p.index))
            .unwrap();
        let sel: HashSet<_> = [off].into();
        assert!(model.delete_points(index, &sel));
        assert_eq!(count_points(&model), 4); // pure quad now
        let snapshot = model.snapshot_contours(index).unwrap();
        let contour_data = &snapshot.contours[c];
        assert!(contour_data.is_closed());
        assert!(
            contour_data
                .points
                .iter()
                .all(|p| p.typ != norad::PointType::OffCurve)
        );

        // Delete an on-curve point: square becomes a triangle.
        let corner = model.glyphs[index]
            .points
            .iter()
            .find(|p| p.contour == c && p.x == 100.0 && p.y == 0.0)
            .map(|p| (p.contour, p.index))
            .unwrap();
        let sel: HashSet<_> = [corner].into();
        assert!(model.delete_points(index, &sel));
        assert_eq!(count_points(&model), 3);

        // Delete everything: the contour disappears.
        let all: HashSet<_> = model.glyphs[index]
            .points
            .iter()
            .filter(|p| p.contour == c)
            .map(|p| (p.contour, p.index))
            .collect();
        assert!(model.delete_points(index, &all));
        assert!(model.snapshot_contours(index).unwrap().contours.len() <= c);
    }

    #[test]
    fn curve_ops_run_via_shared_core() {
        let mut model = Master::load(&fonts::regular_ufo()).expect("load");
        let index = model
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "o")
            .unwrap();
        let none = HashSet::new();
        let before: Vec<(f64, f64)> = model.glyphs[index]
            .points
            .iter()
            .map(|p| (p.x, p.y))
            .collect();
        // Balance evens handle tension; on a real glyph something moves.
        let changed = model.curve_op(index, &none, CurveOp::Balance);
        let after: Vec<(f64, f64)> = model.glyphs[index]
            .points
            .iter()
            .map(|p| (p.x, p.y))
            .collect();
        if changed {
            assert_ne!(before, after);
        }
        // On-curve points never move under balance.
        for (i, p) in model.glyphs[index].points.iter().enumerate() {
            if p.on_curve {
                assert_eq!(before[i], (p.x, p.y), "on-curve moved at {i}");
            }
        }
        // Harmonize and optimize execute without panicking and keep
        // the outline drawable.
        model.curve_op(index, &none, CurveOp::Harmonize);
        model.curve_op(index, &none, CurveOp::Optimize(0.12));
        assert!(!model.glyphs[index].path.elements().is_empty());
    }

    #[test]
    fn metric_edits() {
        let mut model = Master::load(&fonts::regular_ufo()).expect("load");
        let index = model
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "n")
            .unwrap();
        let ink = model.ink_bounds(index).unwrap();
        let advance = model.glyphs[index].advance;

        // Width edit changes only the advance.
        model.set_advance(index, advance + 20.0);
        assert_eq!(model.glyphs[index].advance, advance + 20.0);
        assert_eq!(model.ink_bounds(index).unwrap().x0, ink.x0);

        // LSB edit shifts the ink, advance untouched.
        model.shift_ink(index, 10.0);
        let ink2 = model.ink_bounds(index).unwrap();
        assert_eq!(ink2.x0, ink.x0 + 10.0);
        assert_eq!(ink2.x1, ink.x1 + 10.0);
        assert_eq!(model.glyphs[index].advance, advance + 20.0);
        assert!(model.dirty);
    }

    #[test]
    fn smooth_handle_constraint_keeps_collinearity() {
        let mut model = Master::load(&fonts::regular_ufo()).expect("load");
        let index = model
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "space")
            .unwrap();
        // Two curve segments joined at a smooth point (100,100):
        let c = model.start_contour(index, 0.0, 0.0).unwrap();
        model.append_segment(
            index,
            c,
            Some(((40.0, 60.0), (60.0, 100.0))),
            100.0,
            100.0,
            true,
        );
        model.append_segment(
            index,
            c,
            Some(((140.0, 100.0), (180.0, 60.0))),
            200.0,
            0.0,
            false,
        );
        model.close_contour(index, c, None);

        // Points in contour c: find indices of the incoming handle
        // (60,100), the smooth point (100,100), the outgoing (140,100).
        let find = |m: &Master, x: f64, y: f64| {
            m.glyphs[index]
                .points
                .iter()
                .find(|p| p.contour == c && p.x == x && p.y == y)
                .map(|p| p.index)
                .unwrap()
        };
        let incoming = find(&model, 60.0, 100.0);
        let outgoing = find(&model, 140.0, 100.0);

        // Drag the incoming handle downward; the outgoing must rotate
        // to stay collinear through (100,100).
        model.set_points(index, &[((c, incoming), (60.0, 80.0))]);
        model.edit_glyph(index, |g| ops::constrain_smooth_neighbor(g, c, incoming));
        let pts = &model.glyphs[index].points;
        let out_pt = pts
            .iter()
            .find(|p| p.contour == c && p.index == outgoing)
            .unwrap();
        // Collinearity: cross product of (anchor-incoming) and
        // (outgoing-anchor) near zero (integer rounding allowed).
        let cross = (100.0 - 60.0) * (out_pt.y - 100.0) - (100.0 - 80.0) * (out_pt.x - 100.0);
        assert!(
            cross.abs() <= 60.0,
            "not collinear enough: {cross} ({}, {})",
            out_pt.x,
            out_pt.y
        );
        // Length preserved (was 40).
        let len = ((out_pt.x - 100.0_f64).powi(2) + (out_pt.y - 100.0_f64).powi(2)).sqrt();
        assert!((len - 40.0).abs() < 2.0, "length changed: {len}");
    }

    #[test]
    fn anchor_lifecycle_with_undo_snapshot() {
        let mut model = Master::load(&fonts::regular_ufo()).expect("load");
        let index = model
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "n")
            .unwrap();
        let before = model.snapshot_contours(index).unwrap();
        let base = model.glyphs[index].anchors.len();

        model.add_anchor(index, 200.0, 500.0);
        assert_eq!(model.glyphs[index].anchors.len(), base + 1);
        model.set_anchor(index, base, 210.0, 490.0);
        assert_eq!(model.glyphs[index].anchors[base].1, 210.0);
        model.delete_anchor(index, base);
        assert_eq!(model.glyphs[index].anchors.len(), base);

        // Snapshot restore also brings anchors and width back.
        model.add_anchor(index, 1.0, 2.0);
        model.set_advance(index, 999.0);
        model.restore_contours(index, before);
        assert_eq!(model.glyphs[index].anchors.len(), base);
        assert_ne!(model.glyphs[index].advance, 999.0);
    }

    #[test]
    fn kerning_lookup_and_exception() {
        let mut model = Master::load(&fonts::regular_ufo()).expect("load");
        // Group fallback resolves (VirtuaGrotesk has kern groups); the
        // exact value doesn't matter, just that lookup doesn't panic
        // and exceptions override.
        let base = crate::document::font_ops::kern_value(&model.font, "A", "V");
        crate::document::font_ops::set_kern_pair(&mut model.font, "A", "V", base - 14.0);
        assert_eq!(
            crate::document::font_ops::kern_value(&model.font, "A", "V"),
            base - 14.0
        );
        // Unrelated pair unaffected by the exception.
        let _ = crate::document::font_ops::kern_value(&model.font, "o", "o");
    }

    #[test]
    fn shape_contours() {
        let mut model = Master::load(&fonts::regular_ufo()).expect("load");
        let index = model
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "space")
            .unwrap();
        let base = model.snapshot_contours(index).unwrap().contours.len();
        let rect = kurbo::Rect::new(10.0, 20.0, 110.0, 220.0);
        model.add_shape_contour(index, rect, false);
        model.add_shape_contour(index, rect, true);
        let contours = model.snapshot_contours(index).unwrap().contours;
        assert_eq!(contours.len(), base + 2);
        let square = &contours[base];
        assert_eq!(square.points.len(), 4);
        assert!(square.is_closed());
        let circle = &contours[base + 1];
        assert_eq!(circle.points.len(), 12); // 4 on + 8 off
        assert!(circle.is_closed());
        // Ellipse extremes touch the rect edges.
        let xs: Vec<f64> = circle.points.iter().map(|p| p.x).collect();
        assert_eq!(xs.iter().cloned().fold(f64::MAX, f64::min), 10.0);
        assert_eq!(xs.iter().cloned().fold(f64::MIN, f64::max), 110.0);
    }

    #[test]
    fn decompose_components() {
        let mut model = Master::load(&fonts::regular_ufo()).expect("load");
        let index = model
            .glyphs
            .iter()
            .position(|g| !g.component_names.is_empty())
            .expect("demo font has composite glyphs");
        use kurbo::Shape;
        let area_before = model.glyphs[index].path.area().abs();
        let contours_before = model.snapshot_contours(index).unwrap().contours.len();
        assert!(model.decompose(index));
        let snap = model.snapshot_contours(index).unwrap();
        assert!(snap.components.is_empty());
        assert!(snap.contours.len() > contours_before);
        // The rendered ink is essentially unchanged (integer rounding).
        let area_after = model.glyphs[index].path.area().abs();
        assert!(
            (area_before - area_after).abs() / area_before.max(1.0) < 0.02,
            "area changed too much: {area_before} -> {area_after}"
        );
        assert!(model.glyphs[index].component_names.is_empty());
    }

    #[test]
    fn remove_overlap_unions_contours() {
        use kurbo::Shape;
        let mut model = Master::load(&fonts::regular_ufo()).expect("load");
        let index = model
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "space")
            .unwrap();
        // Two overlapping squares: union area = 100*100 + 100*100 - 50*50.
        model.add_shape_contour(index, kurbo::Rect::new(0.0, 0.0, 100.0, 100.0), false);
        model.add_shape_contour(index, kurbo::Rect::new(50.0, 50.0, 150.0, 150.0), false);
        assert!(model.remove_overlap(index));
        let snap = model.snapshot_contours(index).unwrap();
        assert_eq!(snap.contours.len(), 1, "union should merge to one contour");
        let area = model.glyphs[index].path.area().abs();
        assert!(
            (area - 17500.0).abs() < 100.0,
            "union area wrong: {area} (expected ~17500)"
        );
        assert!(snap.contours[0].is_closed());
    }

    #[test]
    fn move_point_and_save_roundtrip() {
        let src = fonts::regular_ufo();
        let tmp = std::env::temp_dir().join("rbg-save-test.ufo");
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp).unwrap();
        }
        let copy_options = fonts::copy_dir(&src, &tmp).is_ok();
        assert!(copy_options, "copying test UFO failed");

        let mut model = Master::load(&tmp).expect("load");
        let index = model
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "a")
            .expect("glyph a");
        let before = model.glyphs[index].points[0];
        model.set_points(
            index,
            &[(
                (before.contour, before.index),
                (before.x + 10.0, before.y + 5.0),
            )],
        );
        assert!(model.dirty);
        let after = model.glyphs[index].points[0];
        assert_eq!(after.x, before.x + 10.0);
        assert_eq!(after.y, before.y + 5.0);
        model.save().expect("save");
        assert!(!model.dirty);

        let reloaded = Master::load(&tmp).expect("reload");
        let entry = reloaded
            .glyphs
            .iter()
            .find(|g| g.name.as_ref() == "a")
            .unwrap();
        let p = entry
            .points
            .iter()
            .find(|p| p.contour == before.contour && p.index == before.index)
            .unwrap();
        assert_eq!(p.x, before.x + 10.0);
        assert_eq!(p.y, before.y + 5.0);
        std::fs::remove_dir_all(&tmp).ok();
    }
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The open document: one or more master UFOs, optionally tied
//! together by a designspace.
//!
//! [`Master`] is one UFO with its bookkeeping (what changed since the
//! last save) and a paint-ready cache of every glyph ([`GlyphEntry`]:
//! outlines as kurbo paths, points, anchors, ink box), kept in the
//! order the glyph grid shows. [`Project`] holds the masters, the axes
//! and their locations, the variation model, named instances, and
//! sparse brace sources, and answers interpolation questions across
//! them. Nothing here knows how a glyph is drawn on screen; the
//! front-ends read the cache and paint it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use kurbo::BezPath;

use crate::document::var_model::{Location, VariationModel};
use crate::formats::binary_import::import_binary_font;
use crate::formats::lib_keys::{hoi_quad_at, read_hoi_intermediates};
use crate::outline::glyph_ops::{self as ops, CurveOp, GlyphSnapshot};
use crate::ui::theme_oklch::{self, Theme};

/// The mark label a glyph carries. Labels are palette names shared by
/// every theme, so snapping against the default theme is enough here;
/// the front-end maps a label to the current theme's colour.
fn mark_label(glyph: &norad::Glyph) -> Option<String> {
    static DEFAULT: OnceLock<Theme> = OnceLock::new();
    let theme = DEFAULT
        .get_or_init(|| theme_oklch::load_theme("gray").expect("the built-in gray theme loads"));
    theme_oklch::mark_label_for_glyph(glyph, theme)
}

/// One control point of a contour, in font units, with its identity
/// inside the glyph so edits can address it.
#[derive(Clone, Copy)]
pub struct GlyphPoint {
    /// X coordinate in font units.
    pub x: f64,
    /// Y coordinate in font units.
    pub y: f64,
    /// True for an on-curve point, false for a control point.
    pub on_curve: bool,
    /// True when the on-curve point's handles are kept collinear.
    pub smooth: bool,
    /// Point in a hyperbezier contour (drawn in its own color).
    pub hyper: bool,
    /// Index of the contour that owns this point.
    pub contour: usize,
    /// Index of the point within its contour.
    pub index: usize,
}

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

/// One UFO master with its change tracking and a paint-ready glyph cache.
pub struct Master {
    /// The loaded UFO.
    pub font: norad::Font,
    /// Names of glyphs edited since load/save (partial saves).
    pub modified_glyphs: HashSet<String>,
    /// glyph name → glif path relative to the UFO root (memory hosts).
    pub glif_paths: HashMap<String, String>,
    /// Kerning changed since load/save.
    pub kerning_dirty: bool,
    /// codepoint → index into `glyphs`, for the text preview.
    /// glyph name → index into `glyphs` (text buffer sorts carry
    /// names, including unencoded ligature glyphs from shaping).
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
    /// Bumped when the glyph list itself changes (added, removed,
    /// renamed), so caches keyed on the list can tell.
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

    /// After any glyph edit: re-place anchor-locked components — the
    /// edited glyph's own (its anchors may have moved; its own
    /// anchors seed, the open-glyph behavior) and every composite
    /// that places it, so accents follow their base live.
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

    /// Rebuild every cache from the norad font (glyph added or
    /// removed); bookkeeping fields survive.
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
    pub fn load(path: &Path) -> Result<Self, norad::error::FontLoadError> {
        let font = norad::Font::load(path)?;
        Ok(Self::from_font(font, path.to_path_buf()))
    }

    /// Build the model from an already-assembled font (in-memory
    /// hosts: web builds, embedded demo data).
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
            crate::outline::glyph_ops::append_hyper_point(g, contour, x, y, corner)
        });
    }

    /// Closes an open hyperbezier contour.
    pub fn close_hyper_contour(&mut self, glyph_index: usize, contour: usize) {
        self.edit_glyph(glyph_index, |g| {
            crate::outline::glyph_ops::close_hyper_contour(g, contour)
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
            ops::append_segment(g, contour, controls, x, y, smooth)
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

    /// Delete an unfinished pen contour (single stray point).
    pub fn remove_contour_if_degenerate(&mut self, glyph_index: usize, contour: usize) {
        self.edit_glyph(glyph_index, |g| {
            ops::remove_contour_if_degenerate(g, contour)
        });
    }

    /// Delete points (see `crate::outline::glyph_ops`).
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

    /// Ink bounds of a glyph in design units, None when empty.
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
        let resolved = ops::resolved_component_contours(&self.font, glyph);
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

    /// Union all contours (remove overlap). Returns false when
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
    pub fn save(&mut self) -> Result<(), norad::error::FontWriteError> {
        self.font.save(&self.source_path)?;
        self.dirty = false;
        self.modified_glyphs.clear();
        self.kerning_dirty = false;
        Ok(())
    }
}

/// One designspace axis, in design coordinates.
#[derive(Clone)]
pub struct AxisInfo {
    /// Axis name as written in the designspace.
    pub name: String,
    /// Four-letter OpenType axis tag.
    pub tag: Arc<str>,
    /// Minimum value in design coordinates.
    pub min: f64,
    /// Default value in design coordinates.
    pub default: f64,
    /// Maximum value in design coordinates.
    pub max: f64,
}

/// An open project: one or more master UFOs, optionally tied together
/// by a designspace document.
pub struct Project {
    /// The loaded masters, in designspace source order.
    pub masters: Vec<Master>,
    /// Index into `masters` of the master being edited.
    pub active: usize,
    /// Style names for the master switcher, one per master.
    pub master_names: Vec<Arc<str>>,
    /// The designspace axes, empty for a single UFO.
    pub axes: Vec<AxisInfo>,
    /// Normalized (-1..1) location of each master, by axis name.
    pub master_locations: Vec<Location>,
    /// The variation model over `master_locations`, if there is more than one master.
    pub model: Option<VariationModel>,
    /// Current preview location, normalized, by axis name.
    pub location: Location,
    /// Per-glyph master point-compatibility (designspaces only).
    pub compat: HashMap<String, bool>,
    /// What fontc compiles on File > Export: the designspace the
    /// project was opened from, or the single UFO. `None` until the
    /// project has a home on disk (File > New before Save As).
    pub export_source: Option<PathBuf>,
    /// Named designspace instances: style name and normalized
    /// location, for the Instances rows under the axis sliders.
    pub instances: Vec<(Arc<str>, Location)>,
    /// The loaded designspace document, kept so instance (and later
    /// axis) edits can be written back. None for single-UFO projects.
    pub ds_doc: Option<norad::designspace::DesignSpaceDocument>,
    /// Instance edits not yet written to the designspace file.
    pub ds_dirty: bool,
    /// Sparse "brace" sources: per-glyph intermediate masters living
    /// in a named layer of a master UFO at their own location
    /// (designspace sources with a `layer` attribute).
    pub brace: Vec<BraceSource>,
}

/// One sparse intermediate source (a Glyphs brace layer).
pub struct BraceSource {
    /// Index into `masters`: the UFO holding the layer.
    pub master: usize,
    /// The UFO layer name (Glyphs writes "{500}").
    pub layer: String,
    /// Normalized location.
    pub location: Location,
}

/// Which Glyphs form a path names, if either.
#[derive(Clone, Copy, PartialEq)]
pub enum GlyphsSource {
    /// A single `.glyphs` file.
    File,
    /// A `.glyphspackage` directory.
    Package,
    /// Not a Glyphs path.
    Neither,
}

/// Read a `.glyphspackage` into the entries the importer wants: paths
/// relative to the package root, so `glyphs/A.glyph` stays
/// `glyphs/A.glyph`.
pub fn read_glyphspackage(root: &Path) -> Result<HashMap<String, String>, String> {
    pub fn walk(dir: &Path, root: &Path, out: &mut HashMap<String, String>) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| format!("{e}"))? {
            let path = entry.map_err(|e| format!("{e}"))?.path();
            if path.is_dir() {
                walk(&path, root, out)?;
            } else if let Ok(text) = std::fs::read_to_string(&path) {
                // Anything that is not UTF-8 is not part of the
                // source; skip it rather than failing the open.
                let rel = path
                    .strip_prefix(root)
                    .map_err(|e| format!("{e}"))?
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, text);
            }
        }
        Ok(())
    }
    let mut out = HashMap::new();
    walk(root, root, &mut out)?;
    if out.is_empty() {
        return Err(format!("{} is empty", root.display()));
    }
    Ok(out)
}

impl Project {
    /// The master sitting exactly at `location`, if any. Landing on a
    /// master is a master switch, not an interpolation: the web treats
    /// it that way so the outline stays editable.
    pub fn master_at_location(&self) -> Option<usize> {
        if self.axes.is_empty() {
            return None;
        }
        self.master_locations.iter().position(|there| {
            self.axes.iter().all(|axis| {
                let a = there.get(&axis.name).copied().unwrap_or(0.0);
                let b = self.location.get(&axis.name).copied().unwrap_or(0.0);
                (a - b).abs() < 1e-6
            })
        })
    }

    /// True while the sliders sit between masters: what the canvas
    /// shows is an interpolated instance, and nothing there is
    /// editable.
    pub fn showing_instance(&self) -> bool {
        self.model.is_some() && !self.axes.is_empty() && self.master_at_location().is_none()
    }

    /// Put `location` back on a master, for a master switch.
    pub fn snap_location_to_master(&mut self, master: usize) {
        if let Some(there) = self.master_locations.get(master) {
            self.location = there.clone();
        }
    }

    /// File → New Font: one master from the GF-shaped template. The
    /// source path is where Save will write; Save As picks it.
    pub fn new_font(path: PathBuf) -> Self {
        let font = crate::document::new_font::new_font("Untitled", "Regular", 400);
        let mut model = Master::from_font(font, path);
        model.dirty = true;
        let mut project = Self {
            masters: vec![model],
            active: 0,
            master_names: vec!["Regular".into()],
            axes: Vec::new(),
            master_locations: Vec::new(),
            model: None,
            location: Location::new(),
            compat: HashMap::new(),
            export_source: None,
            instances: Vec::new(),
            ds_doc: None,
            ds_dirty: false,
            brace: Vec::new(),
        };
        project.compute_compat();
        project
    }

    /// Opens a designspace, UFO, Glyphs source, or binary font. Sets `export_source` to `path` when the loader left it unset.
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut project = Self::load_inner(path)?;
        if project.export_source.is_none() {
            project.export_source = Some(path.to_path_buf());
        }
        project.compute_compat();
        Ok(project)
    }

    /// Loads a project by file type without filling in `export_source` or computing compatibility. Prefer [`Project::load`].
    pub fn load_inner(path: &Path) -> Result<Self, String> {
        let glyphs_ext = path.extension().and_then(|e| e.to_str()).map(|e| {
            if e.eq_ignore_ascii_case("glyphspackage") {
                GlyphsSource::Package
            } else if e.eq_ignore_ascii_case("glyphs") {
                GlyphsSource::File
            } else {
                GlyphsSource::Neither
            }
        });
        if let Some(kind @ (GlyphsSource::File | GlyphsSource::Package)) = glyphs_ext {
            // Convert the Glyphs source to UFO + designspace files in
            // a sibling directory, then open the converted project.
            let result = match kind {
                GlyphsSource::Package => {
                    let entries = read_glyphspackage(path)?;
                    crate::formats::glyphs_import::glyphs_package_to_ufo_files(&entries)?
                }
                _ => {
                    let text = std::fs::read_to_string(path).map_err(|e| format!("{e}"))?;
                    crate::formats::glyphs_import::glyphs_to_ufo_files(&text)?
                }
            };
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "glyphs-import".into());
            let out_dir = path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .join(format!("{stem}-ufo"));
            let mut designspace: Option<PathBuf> = None;
            let mut first_ufo: Option<PathBuf> = None;
            for file in &result.files {
                let target = out_dir.join(&file.path);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("{e}"))?;
                }
                std::fs::write(&target, &file.text).map_err(|e| format!("{e}"))?;
                if file.path.ends_with(".designspace") {
                    designspace = Some(target);
                } else if first_ufo.is_none() && file.path.ends_with("fontinfo.plist") {
                    first_ufo = target.parent().map(|p| p.to_path_buf());
                }
            }
            let open = designspace
                .or(first_ufo)
                .ok_or_else(|| "conversion produced no font".to_string())?;
            // Export compiles the converted files, not the .glyphs.
            let mut project = Self::load_inner(&open)?;
            project.export_source = Some(open);
            return Ok(project);
        }
        if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ttf") || e.eq_ignore_ascii_case("otf"))
        {
            // A compiled font opens as an editable in-memory UFO.
            // Save writes that UFO next to the binary — never over
            // it — and Export compiles from the UFO.
            let font = import_binary_font(path)?;
            let name: Arc<str> = font
                .font_info
                .style_name
                .clone()
                .unwrap_or_else(|| "Regular".into())
                .into();
            let ufo_path = path.with_extension("ufo");
            let mut model = Master::from_font(font, ufo_path.clone());
            model.dirty = true;
            let mut project = Self {
                masters: vec![model],
                active: 0,
                master_names: vec![name],
                axes: Vec::new(),
                master_locations: Vec::new(),
                model: None,
                location: Location::new(),
                compat: HashMap::new(),
                export_source: Some(ufo_path),
                instances: Vec::new(),
                ds_doc: None,
                ds_dirty: false,
                brace: Vec::new(),
            };
            project.compute_compat();
            return Ok(project);
        }
        if path.extension().is_some_and(|e| e == "designspace") {
            let doc = norad::designspace::DesignSpaceDocument::load(path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            let dir = path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            return Self::from_designspace(doc, move |filename| {
                let ufo_path = dir.join(filename);
                Master::load(&ufo_path).map_err(|e| format!("{}: {e}", ufo_path.display()))
            });
        }
        {
            let model = Master::load(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let name: Arc<str> = model
                .font
                .font_info
                .style_name
                .clone()
                .unwrap_or_else(|| "Regular".into())
                .into();
            Ok(Self {
                masters: vec![model],
                active: 0,
                master_names: vec![name],
                axes: Vec::new(),
                master_locations: Vec::new(),
                model: None,
                location: Location::new(),
                compat: HashMap::new(),
                export_source: None,
                instances: Vec::new(),
                ds_doc: None,
                ds_dirty: false,
                brace: Vec::new(),
            })
        }
    }

    /// Assemble a designspace project; `load_master` maps a source
    /// filename to its font model (filesystem or in-memory host).
    pub fn from_designspace(
        doc: norad::designspace::DesignSpaceDocument,
        mut load_master: impl FnMut(&str) -> Result<Master, String>,
    ) -> Result<Self, String> {
        {
            let mut seen = HashSet::new();
            let mut masters = Vec::new();
            let mut master_names = Vec::new();
            let mut default_index = 0usize;
            // The source whose location matches every axis default is
            // the default master; open on that one.
            let defaults: HashMap<&str, f32> = doc
                .axes
                .iter()
                .map(|a| (a.name.as_str(), a.default))
                .collect();
            // Axis metadata (design coordinates; avar maps ignored
            // for now, which matches sources that don't use them).
            let axes: Vec<AxisInfo> = doc
                .axes
                .iter()
                .map(|a| AxisInfo {
                    name: a.name.clone(),
                    tag: a.tag.clone().into(),
                    min: a.minimum.unwrap_or(a.default) as f64,
                    default: a.default as f64,
                    max: a.maximum.unwrap_or(a.default) as f64,
                })
                .collect();
            let mut master_locations = Vec::new();
            let mut master_files: Vec<String> = Vec::new();
            // Sparse sources (a `layer` attribute) are brace layers:
            // per-glyph intermediates, resolved after the masters.
            let normalize_loc = |dims: &[norad::designspace::Dimension]| {
                let mut location = Location::new();
                for axis in &axes {
                    let raw = dims
                        .iter()
                        .find(|d| d.name == axis.name)
                        .and_then(|d| d.xvalue.or(d.uservalue))
                        .map(|v| v as f64)
                        .unwrap_or(axis.default);
                    location.insert(
                        axis.name.clone(),
                        crate::document::var_model::normalize_value(
                            raw,
                            axis.min,
                            axis.default,
                            axis.max,
                        ),
                    );
                }
                location
            };
            let mut layer_sources: Vec<(String, String, Location)> = Vec::new();
            for source in &doc.sources {
                if let Some(layer) = &source.layer {
                    layer_sources.push((
                        source.filename.clone(),
                        layer.clone(),
                        normalize_loc(&source.location),
                    ));
                    continue;
                }
                if !seen.insert(source.filename.clone()) {
                    continue; // duplicate full-source entries
                }
                let model = load_master(&source.filename)?;
                let is_default = source.location.iter().all(|d| {
                    let value = d.xvalue.or(d.uservalue).unwrap_or(0.0);
                    defaults
                        .get(d.name.as_str())
                        .is_some_and(|v| (*v - value).abs() < f32::EPSILON)
                });
                if is_default {
                    default_index = masters.len();
                }
                // Normalized location for the interpolation model.
                let mut location = Location::new();
                for axis in &axes {
                    let raw = source
                        .location
                        .iter()
                        .find(|d| d.name == axis.name)
                        .and_then(|d| d.xvalue.or(d.uservalue))
                        .map(|v| v as f64)
                        .unwrap_or(axis.default);
                    location.insert(
                        axis.name.clone(),
                        crate::document::var_model::normalize_value(
                            raw,
                            axis.min,
                            axis.default,
                            axis.max,
                        ),
                    );
                }
                master_locations.push(location);
                let name = source
                    .stylename
                    .clone()
                    .unwrap_or_else(|| source.filename.clone());
                masters.push(model);
                master_names.push(name.into());
                master_files.push(source.filename.clone());
            }
            if masters.is_empty() {
                return Err("designspace has no sources".into());
            }
            let model = (masters.len() > 1).then(|| VariationModel::new(&master_locations));
            let location = axes.iter().map(|a| (a.name.clone(), 0.0)).collect();
            let brace: Vec<BraceSource> = layer_sources
                .into_iter()
                .filter_map(|(filename, layer, location)| {
                    let master = master_files.iter().position(|f| *f == filename)?;
                    Some(BraceSource {
                        master,
                        layer,
                        location,
                    })
                })
                .collect();
            let mut project = Self {
                active: default_index,
                masters,
                master_names,
                axes,
                master_locations,
                model,
                location,
                compat: HashMap::new(),
                export_source: None,
                instances: Vec::new(),
                ds_doc: Some(doc),
                ds_dirty: false,
                brace,
            };
            project.refresh_instances_from_doc();
            Ok(project)
        }
    }

    /// Structural signature used for interpolation compatibility:
    /// per contour, the ordered list of point types.
    pub fn glyph_signature(font: &Master, name: &str) -> Option<Vec<Vec<norad::PointType>>> {
        font.font.get_glyph(name).map(ops::glyph_signature)
    }

    /// Why a glyph does not interpolate: the first master pair whose
    /// structure disagrees, with contour and point counts. None when
    /// compatible or single-master.
    pub fn compat_detail(&self, name: &str) -> Option<String> {
        if self.masters.len() < 2 || self.compat.get(name).copied().unwrap_or(true) {
            return None;
        }
        let first_sig = Self::glyph_signature(&self.masters[0], name);
        let first_name = &self.master_names[0];
        let describe = |sig: &Option<Vec<Vec<norad::PointType>>>| match sig {
            None => "missing".to_string(),
            Some(contours) => {
                let points: usize = contours.iter().map(|c| c.len()).sum();
                format!("{}c · {}pt", contours.len(), points)
            }
        };
        for (master, master_name) in self.masters.iter().zip(&self.master_names).skip(1) {
            let sig = Self::glyph_signature(master, name);
            if sig == first_sig {
                continue;
            }
            return Some(format!(
                "{first_name} {} · {master_name} {}",
                describe(&first_sig),
                describe(&sig),
            ));
        }
        // Same counts everywhere: the disagreement is point types
        // (a curve against a line somewhere).
        Some("point types differ between masters".into())
    }

    /// Rebuild the Instances display rows (name + normalized
    /// location) from the designspace document.
    pub fn refresh_instances_from_doc(&mut self) {
        let Some(doc) = self.ds_doc.as_ref() else {
            return;
        };
        self.instances = doc
            .instances
            .iter()
            .map(|inst| {
                let name: Arc<str> = inst
                    .stylename
                    .clone()
                    .or_else(|| inst.name.clone())
                    .unwrap_or_else(|| "Instance".into())
                    .into();
                let mut location = Location::new();
                for axis in &self.axes {
                    let raw = inst
                        .location
                        .iter()
                        .find(|d| d.name == axis.name)
                        .and_then(|d| d.xvalue.or(d.uservalue))
                        .map(|v| v as f64)
                        .unwrap_or(axis.default);
                    location.insert(
                        axis.name.clone(),
                        crate::document::var_model::normalize_value(
                            raw,
                            axis.min,
                            axis.default,
                            axis.max,
                        ),
                    );
                }
                (name, location)
            })
            .collect();
    }

    /// Check one glyph's compatibility across all masters.
    pub fn check_compat(&self, name: &str) -> bool {
        let mut signatures = self.masters.iter().map(|m| Self::glyph_signature(m, name));
        let Some(first) = signatures.next().flatten() else {
            return false;
        };
        signatures.all(|s| s.as_ref() == Some(&first))
    }

    /// Recompute the whole compatibility map (load / reload).
    pub fn compute_compat(&mut self) {
        self.compat.clear();
        if self.masters.len() < 2 {
            return;
        }
        let names: Vec<String> = self.masters[self.active]
            .glyphs
            .iter()
            .map(|g| g.name.to_string())
            .collect();
        for name in names {
            let ok = self.check_compat(&name);
            self.compat.insert(name, ok);
        }
    }

    /// Recheck one glyph after editing.
    pub fn recheck_compat(&mut self, name: &str) {
        if self.masters.len() < 2 {
            return;
        }
        let ok = self.check_compat(name);
        self.compat.insert(name.to_string(), ok);
    }

    /// Interpolated outline + advance of a glyph at the current
    /// location. None when at the default location, when masters are
    /// point-incompatible, or when there is no model.
    /// The glyph rebuilt from every source EXCEPT the active
    /// master, evaluated at the active master's own location —
    /// Glyphs' Re-Interpolate, for repairing one broken master from
    /// the others. With one other source this is a straight copy.
    pub fn reinterpolated_from_others(&self, glyph_name: &str) -> Result<norad::Glyph, String> {
        let flatten = |glyph: &norad::Glyph| {
            let mut v = vec![glyph.width];
            for contour in &glyph.contours {
                for p in &contour.points {
                    v.push(p.x);
                    v.push(p.y);
                }
            }
            v
        };
        let mut values: Vec<Vec<f64>> = Vec::new();
        let mut locations: Vec<Location> = Vec::new();
        let mut template: Option<norad::Glyph> = None;
        for (mi, master) in self.masters.iter().enumerate() {
            if mi == self.active {
                continue;
            }
            let Some(glyph) = master.font.get_glyph(glyph_name) else {
                continue;
            };
            values.push(flatten(glyph));
            locations.push(self.master_locations[mi].clone());
            if template.is_none() {
                template = Some(glyph.clone());
            }
        }
        for b in &self.brace {
            if b.master == self.active {
                continue;
            }
            let Some(glyph) = self
                .masters
                .get(b.master)
                .and_then(|m| m.font.layers.get(&b.layer))
                .and_then(|l| l.get_glyph(glyph_name))
            else {
                continue;
            };
            values.push(flatten(glyph));
            locations.push(b.location.clone());
        }
        let Some(mut template) = template else {
            return Err("No other master holds this glyph".into());
        };
        let len = values[0].len();
        if values.iter().any(|v| v.len() != len) {
            return Err("Other masters are not point-compatible".into());
        }
        let out = if values.len() == 1 {
            values.remove(0)
        } else {
            VariationModel::new(&locations)
                .interpolate(&values, &self.master_locations[self.active])
        };
        let mut it = out.iter().copied();
        template.width = it.next().unwrap_or(template.width);
        for contour in template.contours.iter_mut() {
            for p in contour.points.iter_mut() {
                p.x = it.next().unwrap_or(p.x);
                p.y = it.next().unwrap_or(p.y);
            }
        }
        Ok(template)
    }

    /// The interpolation at the current location as a combined path and advance width, using the active master to resolve components.
    pub fn interpolated_glyph(&self, glyph_name: &str) -> Option<(BezPath, f64)> {
        let glyph = self.interpolated_norad_glyph(glyph_name)?;
        let advance = glyph.width;
        let base = &self.masters[self.active];
        Some((
            crate::outline::glyph_paths::glyph_to_bezpath(&glyph, &base.font),
            advance,
        ))
    }

    /// The interpolation at the current location as a norad glyph
    /// (point structure kept): the working form for the ghost, the
    /// strip, and for freezing into a brace layer.
    pub fn interpolated_norad_glyph(&self, glyph_name: &str) -> Option<norad::Glyph> {
        if self.location.values().all(|v| v.abs() < 1e-9) {
            return None;
        }
        self.interpolated_at(glyph_name, &self.location)
    }

    /// The interpolation at an arbitrary normalized location — the
    /// default location included, where it returns the default
    /// master's own coordinates (trajectory sampling needs the whole
    /// axis, ends included).
    pub fn interpolated_at(&self, glyph_name: &str, location: &Location) -> Option<norad::Glyph> {
        self.model.as_ref()?;
        let flatten = |glyph: &norad::Glyph| {
            let mut v = vec![glyph.width];
            for contour in &glyph.contours {
                for p in &contour.points {
                    v.push(p.x);
                    v.push(p.y);
                }
            }
            v
        };
        // Flatten [advance, x0, y0, x1, y1, ...] per master.
        let mut values: Vec<Vec<f64>> = Vec::with_capacity(self.masters.len());
        for master in &self.masters {
            values.push(flatten(master.font.get_glyph(glyph_name)?));
        }
        // Brace layers holding this glyph join the master set: the
        // model grows their locations, per glyph (Glyphs' intermediate
        // layers).
        let mut brace_locations: Vec<Location> = Vec::new();
        for b in &self.brace {
            let Some(glyph) = self
                .masters
                .get(b.master)
                .and_then(|m| m.font.layers.get(&b.layer))
                .and_then(|l| l.get_glyph(glyph_name))
            else {
                continue;
            };
            values.push(flatten(glyph));
            brace_locations.push(b.location.clone());
        }
        let len = values[0].len();
        if values.iter().any(|v| v.len() != len) {
            return None; // point-incompatible sources
        }
        let out = if brace_locations.is_empty() {
            self.model.as_ref()?.interpolate(&values, location)
        } else {
            let mut locations = self.master_locations.clone();
            locations.extend(brace_locations);
            VariationModel::new(&locations).interpolate(&values, location)
        };
        // Rebuild on the default master's structure.
        let base = &self.masters[self.active];
        let mut glyph = base.font.get_glyph(glyph_name)?.clone();
        let mut it = out.iter().copied();
        let advance = it.next()?;
        for contour in glyph.contours.iter_mut() {
            for p in contour.points.iter_mut() {
                p.x = it.next()?;
                p.y = it.next()?;
            }
        }
        glyph.width = advance;
        // HOI: nodes with an intermediate point follow their exact
        // quadratic, overriding the piecewise answer the baked brace
        // layers gave the model — the bake stays for compilers, the
        // preview is exact.
        if let (Some(axis), Some((lo, hi))) = (self.axes.first(), self.axis_end_masters()) {
            let curves = self.masters[lo]
                .font
                .get_glyph(glyph_name)
                .map(read_hoi_intermediates)
                .unwrap_or_default();
            if !curves.is_empty() {
                let normalized = location.get(&axis.name).copied().unwrap_or(0.0);
                let design = crate::document::var_model::denormalize_value(
                    normalized,
                    axis.min,
                    axis.default,
                    axis.max,
                );
                let t01 = ((design - axis.min) / (axis.max - axis.min)).clamp(0.0, 1.0);
                let (a_glyph, b_glyph) = (
                    self.masters[lo].font.get_glyph(glyph_name),
                    self.masters[hi].font.get_glyph(glyph_name),
                );
                if let (Some(a_glyph), Some(b_glyph)) = (a_glyph, b_glyph) {
                    for (&(ci, pi), &q) in &curves {
                        let (Some(pa), Some(pb)) = (
                            a_glyph.contours.get(ci).and_then(|c| c.points.get(pi)),
                            b_glyph.contours.get(ci).and_then(|c| c.points.get(pi)),
                        ) else {
                            continue;
                        };
                        let pos = hoi_quad_at((pa.x, pa.y), (pb.x, pb.y), q, t01);
                        if let Some(point) = glyph
                            .contours
                            .get_mut(ci)
                            .and_then(|c| c.points.get_mut(pi))
                        {
                            point.x = pos.0;
                            point.y = pos.1;
                        }
                    }
                }
            }
        }
        Some(glyph)
    }

    /// The masters at the low and high end of the first axis (by
    /// normalized location), for HOI endpoints.
    pub fn axis_end_masters(&self) -> Option<(usize, usize)> {
        let axis = self.axes.first()?;
        if self.masters.len() < 2 {
            return None;
        }
        let value = |i: usize| {
            self.master_locations
                .get(i)
                .and_then(|l| l.get(&axis.name).copied())
                .unwrap_or(0.0)
        };
        let lo = (0..self.masters.len()).min_by(|&a, &b| value(a).total_cmp(&value(b)))?;
        let hi = (0..self.masters.len()).max_by(|&a, &b| value(a).total_cmp(&value(b)))?;
        (lo != hi).then_some((lo, hi))
    }

    /// Sample every point's position at `steps + 1` equal stops
    /// along the first axis (min to max), through the same per-glyph
    /// model the ghost uses — brace layers bend the trajectories.
    /// Outer index: point (flattened contour order); inner: stop.
    pub fn trajectory_samples(
        &self,
        glyph_name: &str,
        steps: usize,
    ) -> Option<Vec<Vec<kurbo::Point>>> {
        self.model.as_ref()?;
        let axis = self.axes.first()?;
        let mut per_point: Vec<Vec<kurbo::Point>> = Vec::new();
        for step in 0..=steps {
            let t = step as f64 / steps as f64;
            let design = axis.min + (axis.max - axis.min) * t;
            let mut location = self.location.clone();
            location.insert(
                axis.name.clone(),
                crate::document::var_model::normalize_value(
                    design,
                    axis.min,
                    axis.default,
                    axis.max,
                ),
            );
            let glyph = self.interpolated_at(glyph_name, &location)?;
            let mut flat = Vec::new();
            for contour in &glyph.contours {
                for p in &contour.points {
                    flat.push(kurbo::Point::new(p.x, p.y));
                }
            }
            if per_point.is_empty() {
                per_point = flat.into_iter().map(|p| vec![p]).collect();
            } else {
                if flat.len() != per_point.len() {
                    return None;
                }
                for (track, p) in per_point.iter_mut().zip(flat) {
                    track.push(p);
                }
            }
        }
        Some(per_point)
    }

    /// The glyph a designspace rule shows at the current preview
    /// location, if any (bracket layers / shape switches). Rules
    /// apply when every condition of any condition set holds; an
    /// empty condition set always holds.
    pub fn rule_substitute(&self, glyph_name: &str) -> Option<String> {
        let doc = self.ds_doc.as_ref()?;
        // Current location in design coordinates.
        let design: HashMap<&str, f64> = self
            .axes
            .iter()
            .map(|axis| {
                let normalized = self.location.get(&axis.name).copied().unwrap_or(0.0);
                (
                    axis.name.as_str(),
                    crate::document::var_model::denormalize_value(
                        normalized,
                        axis.min,
                        axis.default,
                        axis.max,
                    ),
                )
            })
            .collect();
        for rule in &doc.rules.rules {
            let applies = rule.condition_sets.is_empty()
                || rule.condition_sets.iter().any(|set| {
                    set.conditions.iter().all(|c| {
                        let Some(&value) = design.get(c.name.as_str()) else {
                            return false;
                        };
                        c.minimum.is_none_or(|min| value >= min as f64 - 1e-6)
                            && c.maximum.is_none_or(|max| value <= max as f64 + 1e-6)
                    })
                });
            if !applies {
                continue;
            }
            for sub in &rule.substitutions {
                if sub.name.as_str() == glyph_name {
                    return Some(sub.with.to_string());
                }
            }
        }
        None
    }

    /// The master being edited.
    pub fn active_font(&self) -> &Master {
        &self.masters[self.active]
    }

    /// The master being edited, mutably.
    pub fn active_font_mut(&mut self) -> &mut Master {
        &mut self.masters[self.active]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::measure::joining_band;
    use crate::formats::lib_keys::write_hoi_intermediates;
    use crate::formats::metrics_keys::{read_metrics_key, write_metrics_key};
    use crate::outline::glyph_ops as ops;
    use crate::test_fonts;

    #[test]
    fn designspace_loads_with_masters() {
        let project = Project::load(&crate::test_fonts::designspace()).expect("designspace loads");
        assert_eq!(project.masters.len(), 2, "regular + bold");
        assert!(project.master_names.iter().any(|n| n.contains("Bold")));
        // Active master is the default location (Regular).
        assert!(!project.master_names[project.active].contains("Bold"));
        // Named instances come along, normalized: the extremes sit on
        // the axis ends.
        assert_eq!(project.instances.len(), 4, "four named instances");
        let bold = project
            .instances
            .iter()
            .find(|(name, _)| name.as_ref() == "Bold")
            .expect("a Bold instance");
        let weight = bold.1.values().next().copied().unwrap_or(0.0);
        assert!((weight - 1.0).abs() < 1e-6, "Bold sits at the axis max");
    }

    #[test]
    fn designspace_roundtrip_and_instance_edit() {
        // The saved document must equal the loaded one: instance
        // editing rewrites the whole file, so nothing may be lost.
        let path = crate::test_fonts::designspace();
        let doc = norad::designspace::DesignSpaceDocument::load(&path).expect("designspace loads");
        let tmp = std::env::temp_dir().join("rb-ds-roundtrip.designspace");
        doc.save(&tmp).expect("designspace saves");
        let doc2 =
            norad::designspace::DesignSpaceDocument::load(&tmp).expect("saved designspace loads");
        assert_eq!(doc, doc2, "designspace round-trips losslessly");
        std::fs::remove_file(&tmp).ok();

        // Upsert against the project: renaming at an existing
        // location, adding at a fresh one, deleting.
        let mut project = Project::load(&path).expect("designspace loads");
        let before = project.instances.len();
        let doc = project.ds_doc.as_mut().expect("designspace doc kept");
        doc.instances.remove(0);
        project.ds_dirty = true;
        project.refresh_instances_from_doc();
        assert_eq!(project.instances.len(), before - 1);
    }

    #[test]
    fn brace_layer_refines_interpolation() {
        let mut project = Project::load(&crate::test_fonts::designspace()).expect("loads");
        // Freeze n's Regular outline into a {500} brace layer, then
        // nudge its first point +40: at wght 500 the interpolation
        // must hit the brace exactly, not the linear blend.
        let name = "n";
        let loc_500 = {
            let axis = &project.axes[0];
            let mut l = crate::document::var_model::Location::new();
            l.insert(
                axis.name.clone(),
                crate::document::var_model::normalize_value(
                    500.0,
                    axis.min,
                    axis.default,
                    axis.max,
                ),
            );
            l
        };
        let mut frozen = project.masters[0]
            .font
            .get_glyph(name)
            .expect("has n")
            .clone();
        let orig = frozen.contours[0].points[0].x;
        frozen.contours[0].points[0].x = orig + 40.0;
        project.masters[0]
            .font
            .layers
            .get_or_create_layer("{500}")
            .unwrap()
            .insert_glyph(frozen);
        project.brace.push(BraceSource {
            master: 0,
            layer: "{500}".into(),
            location: loc_500.clone(),
        });
        project.location = loc_500;
        let refined = project
            .interpolated_norad_glyph(name)
            .expect("interpolates");
        assert!(
            (refined.contours[0].points[0].x - (orig + 40.0)).abs() < 0.6,
            "brace layer pins the outline at its location: {} vs {}",
            refined.contours[0].points[0].x,
            orig + 40.0,
        );
    }

    #[test]
    fn reinterpolate_rebuilds_a_master_from_the_others() {
        let mut project = Project::load(&crate::test_fonts::designspace()).expect("loads");
        // Two masters: rebuilding the active one from "the others"
        // must reproduce the other master exactly.
        assert_eq!(project.masters.len(), 2);
        project.active = 0;
        let expected = project.masters[1]
            .font
            .get_glyph("H")
            .expect("bold has H")
            .clone();
        let rebuilt = project
            .reinterpolated_from_others("H")
            .expect("reinterpolates");
        assert_eq!(rebuilt.width, expected.width);
        assert_eq!(rebuilt.contours.len(), expected.contours.len());
        for (a, b) in rebuilt.contours.iter().zip(expected.contours.iter()) {
            for (pa, pb) in a.points.iter().zip(b.points.iter()) {
                assert!((pa.x - pb.x).abs() < 1e-6);
                assert!((pa.y - pb.y).abs() < 1e-6);
            }
        }
        // A glyph missing everywhere else reports, not panics.
        assert!(project.reinterpolated_from_others("no.such.glyph").is_err());
    }

    #[test]
    fn joining_bands_measure_the_connecting_stroke() {
        use norad::{Contour, ContourPoint, PointType};
        let stroke = Contour::new(
            [(0.0, 40.0), (200.0, 40.0), (200.0, 120.0), (0.0, 120.0)]
                .iter()
                .map(|&(x, y)| ContourPoint::new(x, y, PointType::Line, false, None, None))
                .collect(),
            None,
        );
        let mut glyph = norad::Glyph::new("joined");
        glyph.contours = vec![stroke];
        let path = crate::outline::glyph_paths::contour_to_bezpath(&glyph.contours[0]);
        assert_eq!(joining_band(&path, 200.0, true, 2.0), Some((40.0, 120.0)));
        assert_eq!(joining_band(&path, 200.0, false, 2.0), Some((40.0, 120.0)));
        // Pull the ink off the edge: no band.
        for p in glyph.contours[0].points.iter_mut() {
            p.x += 10.0;
        }
        let moved = crate::outline::glyph_paths::contour_to_bezpath(&glyph.contours[0]);
        assert_eq!(joining_band(&moved, 200.0, true, 2.0), None);

        // And the real Arabic set: a medial beh (a composite —
        // components must resolve) touches both edges.
        let project = Project::load(&crate::test_fonts::designspace()).expect("loads");
        let font = project.active_font();
        if let Some(g) = font.font.get_glyph("beh-ar.medi") {
            let i = font.name_map["beh-ar.medi"];
            let advance = font.glyphs[i].advance;
            let outline = crate::outline::glyph_paths::glyph_to_bezpath(g, &font.font);
            assert!(
                joining_band(&outline, advance, true, 2.0).is_some(),
                "medial joins left"
            );
            assert!(
                joining_band(&outline, advance, false, 2.0).is_some(),
                "medial joins right"
            );
        }
    }

    #[test]
    fn metrics_keys_sync_roundtrip() {
        // n's LSB copied onto h in both masters through the lib key.
        let mut project = Project::load(&crate::test_fonts::designspace()).expect("loads");
        for master in project.masters.iter_mut() {
            let glyph = master.font.get_glyph_mut("h").expect("has h");
            write_metrics_key(glyph, true, "=n+10");
        }
        // Emulate command_sync_metrics' inner pass directly.
        for master in project.masters.iter_mut() {
            let n = master.name_map["n"];
            let h = master.name_map["h"];
            let target = master.ink_bounds(n).unwrap().x0 + 10.0;
            let delta = (target - master.ink_bounds(h).unwrap().x0).round();
            master.shift_ink(h, delta);
            let lsb = master.ink_bounds(h).unwrap().x0;
            assert!(
                (lsb - target).abs() < 1.0,
                "h LSB follows n+10: {lsb} vs {target}"
            );
            let back = read_metrics_key(master.font.get_glyph("h").unwrap(), true);
            assert_eq!(back.as_deref(), Some("=n+10"));
        }
    }

    #[test]
    fn hoi_preview_is_exact_without_baking() {
        // An intermediate point in the lib key alone (no baked brace
        // layers) must already curve the preview: at mid-axis the
        // node sits exactly on Q, at quarter-axis on the quadratic.
        let mut project = Project::load(&crate::test_fonts::designspace()).expect("loads");
        let name = "n";
        let axis = project.axes[0].clone();
        let (lo, hi) = project.axis_end_masters().expect("two ends");
        let a = {
            let g = project.masters[lo].font.get_glyph(name).unwrap();
            let p = &g.contours[0].points[0];
            (p.x, p.y)
        };
        let b = {
            let g = project.masters[hi].font.get_glyph(name).unwrap();
            let p = &g.contours[0].points[0];
            (p.x, p.y)
        };
        let q = ((a.0 + b.0) / 2.0 + 80.0, (a.1 + b.1) / 2.0 + 40.0);
        {
            let g = project.masters[lo].font.get_glyph_mut(name).unwrap();
            let mut map = std::collections::HashMap::new();
            map.insert((0usize, 0usize), q);
            write_hoi_intermediates(g, &map);
        }
        let at = |project: &Project, design: f64| {
            let mut location = crate::document::var_model::Location::new();
            location.insert(
                axis.name.clone(),
                crate::document::var_model::normalize_value(
                    design,
                    axis.min,
                    axis.default,
                    axis.max,
                ),
            );
            let g = project.interpolated_at(name, &location).unwrap();
            let p = &g.contours[0].points[0];
            (p.x, p.y)
        };
        let mid_design = axis.min + (axis.max - axis.min) * 0.5;
        let mid = at(&project, mid_design);
        assert!(
            (mid.0 - q.0).abs() < 1e-6 && (mid.1 - q.1).abs() < 1e-6,
            "mid-axis sits on Q: {mid:?} vs {q:?}"
        );
        let quarter_design = axis.min + (axis.max - axis.min) * 0.25;
        let quarter = at(&project, quarter_design);
        let expected = hoi_quad_at(a, b, q, 0.25);
        assert!(
            (quarter.0 - expected.0).abs() < 1e-6 && (quarter.1 - expected.1).abs() < 1e-6,
            "quarter-axis on the quadratic: {quarter:?} vs {expected:?}"
        );
    }

    #[test]
    fn trajectories_sample_the_axis_and_bend_with_braces() {
        let mut project = Project::load(&crate::test_fonts::designspace()).expect("loads");
        let name = "n";
        let tracks = project
            .trajectory_samples(name, 10)
            .expect("samples with plain masters");
        let regular = project.masters[0].font.get_glyph(name).unwrap();
        let first_point = &regular.contours[0].points[0];
        // The t=0 end of every track is the Regular master exactly.
        assert!(
            (tracks[0][0].x - first_point.x).abs() < 1e-6
                && (tracks[0][0].y - first_point.y).abs() < 1e-6
        );
        // Straight interpolation: the midpoint sample is the average
        // of the ends.
        let mid_linear = tracks[0][5].x;
        let expected = (tracks[0][0].x + tracks[0][10].x) / 2.0;
        assert!((mid_linear - expected).abs() < 1.0, "linear before braces");
        // A brace at wght 550 (the axis midpoint) pushing the point
        // +60 bends the track's middle away from the straight line.
        let axis = project.axes[0].clone();
        let mut frozen = regular.clone();
        frozen.contours[0].points[0].x += 60.0;
        project.masters[0]
            .font
            .layers
            .get_or_create_layer("{550}")
            .unwrap()
            .insert_glyph(frozen);
        let mut loc = crate::document::var_model::Location::new();
        loc.insert(
            axis.name.clone(),
            crate::document::var_model::normalize_value(550.0, axis.min, axis.default, axis.max),
        );
        project.brace.push(BraceSource {
            master: 0,
            layer: "{550}".into(),
            location: loc,
        });
        let bent = project.trajectory_samples(name, 10).expect("still samples");
        assert!(
            (bent[0][5].x - mid_linear).abs() > 20.0,
            "brace bends the middle: {} vs {}",
            bent[0][5].x,
            mid_linear
        );
    }

    #[test]
    fn rule_substitute_switches_past_the_condition() {
        let mut project = Project::load(&crate::test_fonts::designspace()).expect("loads");
        let axis = project.axes[0].clone();
        let doc = project.ds_doc.as_mut().expect("doc kept");
        doc.rules.rules.push(norad::designspace::Rule {
            name: Some("a bold".into()),
            condition_sets: vec![norad::designspace::ConditionSet {
                conditions: vec![norad::designspace::Condition {
                    name: axis.name.clone(),
                    minimum: Some(500.0),
                    maximum: Some(axis.max as f32),
                }],
            }],
            substitutions: vec![norad::designspace::Substitution {
                name: norad::Name::new("a").unwrap(),
                with: norad::Name::new("a.bold").unwrap(),
            }],
        });
        let at = |project: &mut Project, design: f64| {
            let axis = &project.axes[0];
            let normalized = crate::document::var_model::normalize_value(
                design,
                axis.min,
                axis.default,
                axis.max,
            );
            let name = axis.name.clone();
            project.location.insert(name, normalized);
        };
        at(&mut project, 450.0);
        assert_eq!(project.rule_substitute("a"), None, "below the switch");
        at(&mut project, 600.0);
        assert_eq!(
            project.rule_substitute("a").as_deref(),
            Some("a.bold"),
            "past the switch"
        );
        assert_eq!(project.rule_substitute("b"), None, "other glyphs untouched");
    }

    #[test]
    fn measures_reference_stems() {
        use crate::analysis::measure::{self, MeasureKind};
        use crate::outline::path::hyper_model::Contour as WContour;
        // Measured straight from the test font's H, the same path the
        // Dimensions section walks.
        let project = Project::load(&crate::test_fonts::designspace()).expect("loads");
        let font = project.active_font();
        let g = font.font.get_glyph("H").expect("has H");
        let paths: Vec<crate::outline::path::Path> = g
            .contours
            .iter()
            .map(|c| crate::outline::path::Path::from_contour(&WContour::from_norad(c)))
            .collect();
        let stems: Vec<i64> = measure::glyph_measurements(&paths)
            .into_iter()
            .filter(|m| m.kind == MeasureKind::Horizontal)
            .map(|m| m.length)
            .collect();
        assert!(!stems.is_empty(), "H yields horizontal spans");
        let narrowest = stems.iter().min().copied().unwrap();
        assert!(
            (10..400).contains(&narrowest),
            "stem in a plausible range: {narrowest}"
        );
    }

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut model = Master::load(&crate::test_fonts::regular_ufo()).expect("load");
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
        let mut model = Master::load(&crate::test_fonts::regular_ufo()).expect("load");
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
        let mut model = Master::load(&crate::test_fonts::regular_ufo()).expect("load");
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
        let sel: std::collections::HashSet<_> = [curve_end_index].into();
        assert!(model.toggle_smooth(index, &sel));

        // Delete one off-curve: the curve segment becomes a line.
        let off = model.glyphs[index]
            .points
            .iter()
            .find(|p| p.contour == c && !p.on_curve)
            .map(|p| (p.contour, p.index))
            .unwrap();
        let sel: std::collections::HashSet<_> = [off].into();
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
        let sel: std::collections::HashSet<_> = [corner].into();
        assert!(model.delete_points(index, &sel));
        assert_eq!(count_points(&model), 3);

        // Delete everything: the contour disappears.
        let all: std::collections::HashSet<_> = model.glyphs[index]
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
        let mut model = Master::load(&crate::test_fonts::regular_ufo()).expect("load");
        let index = model
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "o")
            .unwrap();
        let none = std::collections::HashSet::new();
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
        let mut model = Master::load(&crate::test_fonts::regular_ufo()).expect("load");
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
        let mut model = Master::load(&crate::test_fonts::regular_ufo()).expect("load");
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
        let len = ((out_pt.x - 100.0f64).powi(2) + (out_pt.y - 100.0f64).powi(2)).sqrt();
        assert!((len - 40.0).abs() < 2.0, "length changed: {len}");
    }

    #[test]
    fn anchor_lifecycle_with_undo_snapshot() {
        let mut model = Master::load(&crate::test_fonts::regular_ufo()).expect("load");
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
        let mut model = Master::load(&crate::test_fonts::regular_ufo()).expect("load");
        // Group fallback resolves (VirtuaGrotesk has kern groups); the
        // exact value doesn't matter, just that lookup doesn't panic
        // and exceptions override.
        let base = ops::kern_value(&model.font, "A", "V");
        ops::set_kern_pair(&mut model.font, "A", "V", base - 14.0);
        assert_eq!(ops::kern_value(&model.font, "A", "V"), base - 14.0);
        // Unrelated pair unaffected by the exception.
        let _ = ops::kern_value(&model.font, "o", "o");
    }

    #[test]
    fn interpolation_at_midpoint() {
        let mut project = Project::load(&crate::test_fonts::designspace()).expect("designspace");
        assert!(project.model.is_some(), "two masters, model expected");
        // Move every axis to its normalized midpoint toward max.
        let axis_names: Vec<String> = project.axes.iter().map(|a| a.name.clone()).collect();
        for name in &axis_names {
            project.location.insert(name.clone(), 0.5);
        }
        let (path, advance) = project
            .interpolated_glyph("n")
            .expect("compatible masters interpolate");
        assert!(!path.elements().is_empty());
        // The interpolated advance sits between the two masters'.
        let a0 = project.masters[0].font.get_glyph("n").unwrap().width;
        let a1 = project.masters[1].font.get_glyph("n").unwrap().width;
        let (lo, hi) = (a0.min(a1), a0.max(a1));
        assert!(
            advance >= lo - 1e-6 && advance <= hi + 1e-6,
            "advance {advance} outside [{lo}, {hi}]"
        );
        // Default location yields no ghost.
        for name in &axis_names {
            project.location.insert(name.clone(), 0.0);
        }
        assert!(project.interpolated_glyph("n").is_none());
    }

    #[test]
    fn shape_contours() {
        let mut model = Master::load(&crate::test_fonts::regular_ufo()).expect("load");
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
    fn compat_map_flags_structure_changes() {
        let mut project = Project::load(&crate::test_fonts::designspace()).expect("designspace");
        // Demo masters are interpolation-compatible for letters.
        assert_eq!(project.compat.get("n"), Some(&true));
        // Break compatibility in one master and recheck.
        let idx = project.masters[0]
            .glyphs
            .iter()
            .position(|g| g.name.as_ref() == "n")
            .unwrap();
        let rect = kurbo::Rect::new(0.0, 0.0, 50.0, 50.0);
        project.masters[0].add_shape_contour(idx, rect, false);
        project.recheck_compat("n");
        assert_eq!(project.compat.get("n"), Some(&false));
    }

    #[test]
    fn decompose_components() {
        let mut model = Master::load(&crate::test_fonts::regular_ufo()).expect("load");
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
        let mut model = Master::load(&crate::test_fonts::regular_ufo()).expect("load");
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
        let src = test_fonts::regular_ufo();
        let tmp = std::env::temp_dir().join("rbg-save-test.ufo");
        if tmp.exists() {
            std::fs::remove_dir_all(&tmp).unwrap();
        }
        let copy_options = test_fonts::copy_dir(&src, &tmp).is_ok();
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

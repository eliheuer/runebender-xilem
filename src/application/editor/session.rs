// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Edit sessions: one glyph, its selection, viewport, and undo stack,
//! and the tabs that hold them: opening a glyph, parking and resuming,
//! switching masters, and the axis location.
//!
//! Point selection uses stable canonical identities.
//! The remaining outline algorithms still operate on a compatibility `norad::Glyph` projection,
//! with tuple indices confined to adapters inside this module until their draft APIs land.

use crate::application::editor::tools::metaballs;
use crate::application::font_model::FontModel;
use crate::application::platform::host;
use crate::application::view::canvas::grid::cells_of;
use crate::application::view::panels::sections::metric_bufs;
use crate::application::workspace::{MetadataEdit, Mode, Tab, TextContext, Tool, Workspace};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use masonry::kurbo::{self as kurbo, BezPath, Point, Rect};
use runebender::document::project::CanonicalLayerTransaction;
use runebender::document::{AnchorId, ComponentId, ContourId, LayerPointType, LayerView, PointId};
use runebender::outline::glyph_paths;
use runebender::outline::glyph_paths::round_units;
use runebender::ui::editing::viewport::ViewPort;

/// Boolean operation kinds, mapped to `linesweeper::BinaryOp` internally.
#[derive(Clone, Copy)]
pub(crate) enum BoolOp {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Metrics {
    pub upm: f64,
    pub ascender: f64,
    pub descender: f64,
    pub x_height: f64,
    pub cap_height: f64,
}

impl Metrics {
    pub(crate) fn of(font: &norad::Font) -> Self {
        let info = &font.font_info;
        let upm = info.units_per_em.map(|u| u.as_f64()).unwrap_or(1000.0);
        Self {
            upm,
            ascender: info.ascender.unwrap_or(upm * 0.8),
            descender: info.descender.unwrap_or(-upm * 0.2),
            x_height: info.x_height.unwrap_or(upm * 0.5),
            cap_height: info.cap_height.unwrap_or(upm * 0.7),
        }
    }

    fn of_canonical(info: &runebender::document::model::font_info::CanonicalFontInfo) -> Self {
        let metrics = info.metrics.resolved();
        Self {
            upm: metrics.units_per_em,
            ascender: metrics.ascender,
            descender: metrics.descender,
            x_height: metrics.x_height,
            cap_height: metrics.cap_height,
        }
    }
}

/// One point, as the editor sees it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PointView {
    pub id: PointId,
    pub point: Point,
    pub on_curve: bool,
    pub smooth: bool,
    pub start: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SessionSyncOutcome {
    Changed,
    Unchanged,
    Rejected,
}

#[derive(Clone)]
pub(crate) struct Session {
    pub glyph_name: String,
    pub metaballs: metaballs::MetaballSelection,
    pub metaball_preview: BezPath,
    /// Components, resolved against the font at session creation.
    pub components: BezPath,
    /// Resolved path for each top-level component, for selection and feedback.
    component_cache: Vec<runebender::outline::component_ops::ResolvedDocumentComponent>,
    /// Fresh no-op transaction used to start one owned canonical edit.
    canonical_base: Option<CanonicalLayerTransaction>,
    /// Completed canonical edit waiting for the workspace Project to commit it.
    pub(crate) pending_canonical: Option<CanonicalLayerTransaction>,
    pending_canonical_label: Option<&'static str>,
    /// A rejected transaction whose layer could not be reloaded cannot fall through a later
    /// compatibility sync from the same widget-owned session.
    sync_rejected: bool,
    active_point_drag: Option<CanonicalPointDrag>,
    active_component_drag: Option<CanonicalComponentDrag>,
    active_anchor_drag: Option<CanonicalGesture>,
    active_metric_drag: Option<CanonicalGesture>,
    active_metaball_drag: Option<CanonicalGesture>,
    pub metrics: Metrics,
    pub selection: HashSet<PointId>,
    pub viewport: ViewPort,
    pub fitted: bool,
    in_drag: bool,
    /// The ordinary contour the pen is currently extending, if any.
    pub active_contour: Option<usize>,
    /// In-progress pen points (on- and off-curve), materialized into
    /// `active_contour` on each change.
    pen: Vec<PenPt>,
    /// The stable canonical contour the hyperbezier pen is extending.
    active_hyper_contour: Option<ContourId>,
    /// The currently selected anchor, if any.
    pub selected_anchor: Option<AnchorId>,
    /// The selected top-level component, if any.
    pub selected_component: Option<ComponentId>,
    /// Last flip or rotation, re-applied by Duplicate + Repeat.
    last_transform: Option<kurbo::Affine>,
}

#[derive(Clone)]
struct CanonicalPointDrag {
    transaction: CanonicalLayerTransaction,
    origins: Vec<(PointId, Point)>,
    changed: bool,
}

#[derive(Clone)]
struct CanonicalGesture {
    transaction: CanonicalLayerTransaction,
    changed: bool,
}

#[derive(Clone)]
struct CanonicalComponentDrag {
    gesture: CanonicalGesture,
    component_cache: Vec<runebender::outline::component_ops::ResolvedDocumentComponent>,
    components: BezPath,
}

/// One point in the pen's in-progress buffer.
#[derive(Clone, Copy)]
struct PenPt {
    point: Point,
    off: bool,
    smooth: bool,
}

impl Session {
    /// Build an inactive session with metrics from the canonical active source.
    pub(crate) fn inactive_from_model(font: &FontModel) -> Self {
        let mut session = Self::inactive(font.font());
        session.metrics = Metrics::of_canonical(font.font_info());
        session
    }

    /// Build an editor session with metrics from the canonical active source.
    pub(crate) fn new_from_model(font: &FontModel, name: &str) -> Option<Self> {
        Self::new_from_project(&font.project, name, Metrics::of_canonical(font.font_info()))
    }

    /// Makes the inactive session held while the overview has no glyph to open.
    ///
    /// The editor only reads this session in [`Mode::Editor`]. Keeping an
    /// inert session here avoids making every editor-facing view optional when
    /// a valid UFO has no glyphs yet; opening the first glyph replaces it.
    pub(crate) fn inactive(font: &norad::Font) -> Self {
        Self {
            glyph_name: String::new(),
            metaball_preview: BezPath::new(),
            components: BezPath::new(),
            component_cache: Vec::new(),
            canonical_base: None,
            pending_canonical: None,
            pending_canonical_label: None,
            sync_rejected: false,
            active_point_drag: None,
            active_component_drag: None,
            active_anchor_drag: None,
            active_metric_drag: None,
            active_metaball_drag: None,
            metaballs: metaballs::MetaballSelection::default(),
            metrics: Metrics::of(font),
            selection: HashSet::new(),
            viewport: ViewPort::new(),
            fitted: false,
            in_drag: false,
            active_contour: None,
            pen: Vec::new(),
            active_hyper_contour: None,
            selected_anchor: None,
            selected_component: None,
            last_transform: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn new(font: &norad::Font, name: &str) -> Option<Self> {
        let project = runebender::document::project::Project::from_source(
            runebender::document::project::Master::from_font(
                font.clone(),
                std::path::PathBuf::from("memory.ufo"),
            ),
        );
        Self::new_from_project(&project, name, Metrics::of(font))
    }

    fn new_from_project(
        project: &runebender::document::project::Project,
        name: &str,
        metrics: Metrics,
    ) -> Option<Self> {
        let source = project.source_id(project.active)?;
        let layer_id = project.document_source(source)?.default_layer();
        project.document_layer(name, &layer_id)?;
        let address = runebender::document::variable::GlyphLayerAddress {
            glyph: name.to_owned(),
            layer: layer_id,
        };
        let component_cache = resolved_document_components(project, &address).ok()?;
        let components = combined_component_path(&component_cache);
        let canonical_base = project.begin_document_layer_transaction(&address).ok();
        let mut session = Self {
            glyph_name: name.to_string(),
            metaball_preview: BezPath::new(),
            components,
            component_cache,
            canonical_base,
            pending_canonical: None,
            pending_canonical_label: None,
            sync_rejected: false,
            active_point_drag: None,
            active_component_drag: None,
            active_anchor_drag: None,
            active_metric_drag: None,
            active_metaball_drag: None,
            metaballs: metaballs::MetaballSelection::default(),
            metrics,
            selection: HashSet::new(),
            viewport: ViewPort::new(),
            fitted: false,
            in_drag: false,
            active_contour: None,
            pen: Vec::new(),
            active_hyper_contour: None,
            selected_anchor: None,
            selected_component: None,
            last_transform: None,
        };
        session.refresh_metaball_preview();
        Some(session)
    }

    #[cfg(test)]
    fn legacy_point(&self, id: PointId) -> Option<(usize, usize)> {
        self.current_layer()?
            .contours()
            .enumerate()
            .find_map(|(contour, points)| {
                points
                    .points()
                    .position(|candidate| candidate.id() == id)
                    .map(|point| (contour, point))
            })
    }

    #[cfg(test)]
    fn legacy_selection(&self) -> HashSet<(usize, usize)> {
        self.selection
            .iter()
            .filter_map(|id| self.legacy_point(*id))
            .collect()
    }

    pub(crate) fn canonical_selection(&self, layer: LayerView<'_>) -> Vec<PointId> {
        let available: HashSet<_> = layer
            .contours()
            .flat_map(|contour| contour.points().map(|point| point.id()))
            .collect();
        self.selection
            .iter()
            .filter(|id| available.contains(id))
            .copied()
            .collect()
    }

    pub(crate) fn select_canonical_points(&mut self, layer: LayerView<'_>, selected: &[PointId]) {
        let available: HashSet<_> = layer
            .contours()
            .flat_map(|contour| contour.points().map(|point| point.id()))
            .collect();
        self.selection = selected
            .iter()
            .filter(|id| available.contains(id))
            .copied()
            .collect();
    }

    pub(crate) fn contour_selected(&self, contour: usize) -> bool {
        self.current_layer()
            .and_then(|layer| layer.contours().nth(contour))
            .is_some_and(|contour| {
                contour
                    .points()
                    .any(|point| self.selection.contains(&point.id()))
            })
    }

    pub(crate) fn contour_point_counts(&self) -> Vec<usize> {
        self.current_layer()
            .into_iter()
            .flat_map(|layer| layer.contours().map(|contour| contour.points().count()))
            .collect()
    }

    pub(crate) fn component_references(&self) -> Vec<String> {
        self.current_layer()
            .into_iter()
            .flat_map(|layer| {
                layer
                    .components()
                    .map(|component| component.reference().to_owned())
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn point_id_at(&self, contour: usize, point: usize) -> Option<PointId> {
        self.current_layer()?
            .contours()
            .nth(contour)?
            .points()
            .nth(point)
            .map(|point| point.id())
    }

    pub(crate) fn select_contour(&mut self, contour: usize) -> usize {
        self.selection = self
            .current_layer()
            .and_then(|layer| layer.contours().nth(contour))
            .into_iter()
            .flat_map(|contour| contour.points().map(|point| point.id()))
            .collect();
        self.selection.len()
    }

    pub(crate) fn advance(&self) -> f64 {
        self.current_layer().map_or(0.0, LayerView::width)
    }

    pub(crate) fn has_image(&self) -> bool {
        self.current_layer()
            .is_some_and(|layer| layer.image().is_some())
    }

    pub(crate) fn codepoint(&self) -> Option<char> {
        self.current_layer()?.codepoints().next()
    }

    /// Materialize a detached UFO value only for an application boundary that still consumes the
    /// legacy codec. It is never retained as Session state.
    pub(crate) fn compatibility_glyph(&self) -> Option<norad::Glyph> {
        self.current_transaction()
            .map(CanonicalLayerTransaction::compatibility_glyph)
    }

    pub(crate) fn anchor_points(&self) -> Vec<(AnchorId, Point)> {
        self.current_layer()
            .into_iter()
            .flat_map(|layer| {
                layer
                    .anchors()
                    .map(|anchor| (anchor.id(), anchor.position()))
            })
            .collect()
    }

    pub(crate) fn handle_lines(&self) -> Vec<kurbo::Line> {
        let Some(layer) = self.current_layer() else {
            return Vec::new();
        };
        let mut lines = Vec::new();
        for contour in layer.contours() {
            let points: Vec<_> = contour
                .points()
                .map(|point| (point.position(), point.point_type()))
                .collect();
            let count = points.len();
            for index in 0..count {
                if !matches!(points[index].1, LayerPointType::OffCurve) {
                    continue;
                }
                for neighbor in [(index + count - 1) % count, (index + 1) % count] {
                    if !matches!(points[neighbor].1, LayerPointType::OffCurve) {
                        lines.push(kurbo::Line::new(points[index].0, points[neighbor].0));
                    }
                }
            }
        }
        lines
    }

    pub(crate) fn start_markers(&self) -> Vec<(PointId, Point, Point)> {
        self.current_layer()
            .into_iter()
            .flat_map(|layer| {
                layer.contours().filter_map(|contour| {
                    if !contour.is_closed() {
                        return None;
                    }
                    let points = contour.points().collect::<Vec<_>>();
                    let point = points
                        .iter()
                        .position(|point| point.point_type() != LayerPointType::OffCurve)?;
                    let next = (point + 1) % points.len();
                    let from = points[point].position();
                    let to = points[next].position();
                    (from.distance(to) > 0.001).then_some((points[point].id(), from, to))
                })
            })
            .collect()
    }

    pub(crate) fn segment_bounds(&self) -> Vec<Rect> {
        use kurbo::Shape as _;
        self.current_layer()
            .into_iter()
            .flat_map(runebender::outline::segment_ops::ordinary_layer_segments)
            .map(|segment| segment.seg.bounding_box())
            .collect()
    }

    pub(crate) fn metaball_data(
        &self,
    ) -> Result<
        runebender::document::model::glyph_metadata::Metaballs,
        runebender::document::DocumentEditError,
    > {
        self.current_layer()
            .ok_or(runebender::document::DocumentEditError::MissingLayer)?
            .metaballs()
    }

    pub(crate) fn set_image(&mut self, image: Option<norad::Image>) -> bool {
        self.stage_canonical_edit("set image", move |draft| Ok(draft.set_image(image)))
    }

    fn current_layer(&self) -> Option<LayerView<'_>> {
        self.current_transaction()
            .map(|transaction| transaction.draft().view())
    }

    fn current_transaction(&self) -> Option<&CanonicalLayerTransaction> {
        self.active_point_drag
            .as_ref()
            .map(|drag| &drag.transaction)
            .or_else(|| {
                self.active_component_drag
                    .as_ref()
                    .map(|drag| &drag.gesture.transaction)
            })
            .or_else(|| {
                self.active_anchor_drag
                    .as_ref()
                    .map(|drag| &drag.transaction)
            })
            .or_else(|| {
                self.active_metric_drag
                    .as_ref()
                    .map(|drag| &drag.transaction)
            })
            .or_else(|| {
                self.active_metaball_drag
                    .as_ref()
                    .map(|drag| &drag.transaction)
            })
            .or(self.pending_canonical.as_ref())
            .or(self.canonical_base.as_ref())
    }

    fn stage_canonical_edit(
        &mut self,
        label: &'static str,
        edit: impl FnOnce(
            &mut runebender::document::LayerEditDraft,
        ) -> Result<bool, runebender::document::DocumentEditError>,
    ) -> bool {
        let Some(mut transaction) = self.canonical_base.clone() else {
            return false;
        };
        if edit(transaction.draft_mut()) != Ok(true) {
            return false;
        }
        self.pending_canonical = Some(transaction);
        self.pending_canonical_label = Some(label);
        true
    }

    pub(crate) fn stage_canonical_string_edit(
        &mut self,
        label: &'static str,
        edit: impl FnOnce(&mut runebender::document::LayerEditDraft) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let Some(mut transaction) = self.canonical_base.clone() else {
            return Ok(false);
        };
        if !edit(transaction.draft_mut())? {
            return Ok(false);
        }
        self.pending_canonical = Some(transaction);
        self.pending_canonical_label = Some(label);
        Ok(true)
    }

    /// Run one legacy outline algorithm against a detached UFO codec value, then immediately
    /// reconcile its result into an owned canonical transaction.
    pub(crate) fn compatibility_edit(
        &mut self,
        label: &'static str,
        edit: impl FnOnce(&mut norad::Glyph) -> bool,
    ) -> bool {
        self.compatibility_edit_result(label, |glyph| Ok(edit(glyph)))
            .unwrap_or(false)
    }

    pub(crate) fn compatibility_edit_result(
        &mut self,
        label: &'static str,
        edit: impl FnOnce(&mut norad::Glyph) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let Some(mut transaction) = self.canonical_base.clone() else {
            return Ok(false);
        };
        let mut glyph = transaction.compatibility_glyph();
        if !edit(&mut glyph)? {
            return Ok(false);
        }
        if !transaction
            .reconcile_compatibility_glyph(&glyph)
            .map_err(|error| error.to_string())?
        {
            return Ok(false);
        }
        self.pending_canonical = Some(transaction);
        self.pending_canonical_label = Some(label);
        Ok(true)
    }

    /// Whether a gesture currently owns the session's undo transaction.
    #[cfg_attr(
        not(unix),
        allow(dead_code, reason = "the live document mailbox is Unix-only")
    )]
    pub(crate) fn gesture_in_progress(&self) -> bool {
        self.in_drag
    }

    /// Shift all points and anchors horizontally (left-sidebearing drag).
    pub(crate) fn shift_glyph(&mut self, dx: f64) {
        if !dx.is_finite() || dx == 0.0 {
            return;
        }
        if self.active_metric_drag.is_none() {
            let Some(transaction) = self.canonical_base.clone() else {
                return;
            };
            self.active_metric_drag = Some(CanonicalGesture {
                transaction,
                changed: false,
            });
        }
        let drag = self.active_metric_drag.as_mut().expect("initialized");
        let Ok(changed) = drag.transaction.draft_mut().shift_points_and_anchors_x(dx) else {
            return;
        };
        drag.changed |= changed;
        self.in_drag = true;
    }

    pub(crate) fn set_advance(&mut self, w: f64) {
        let width = w.max(0.0);
        if !w.is_finite() || self.advance() == width {
            return;
        }
        let _ = self.stage_canonical_edit("set advance", |draft| draft.set_width(width));
    }

    /// Set advance during a pointer gesture, grouped into one undo step.
    pub(crate) fn drag_advance(&mut self, w: f64) {
        let width = w.max(0.0);
        if !w.is_finite() || self.advance() == width {
            return;
        }
        if self.active_metric_drag.is_none() {
            let Some(transaction) = self.canonical_base.clone() else {
                return;
            };
            self.active_metric_drag = Some(CanonicalGesture {
                transaction,
                changed: false,
            });
        }
        let drag = self.active_metric_drag.as_mut().expect("initialized");
        let Ok(changed) = drag.transaction.draft_mut().set_width(width) else {
            return;
        };
        drag.changed |= changed;
        self.in_drag = true;
    }

    /// Close an advance or sidebearing pointer transaction.
    pub(crate) fn end_metric_drag(&mut self) {
        if let Some(drag) = self.active_metric_drag.take()
            && drag.changed
        {
            self.pending_canonical = Some(drag.transaction);
            self.pending_canonical_label = Some("metric drag");
        }
        self.in_drag = false;
    }

    pub(crate) fn cancel_metric_drag(&mut self) {
        self.active_metric_drag = None;
        self.in_drag = false;
    }

    pub(crate) fn store_metaballs(
        &mut self,
        source: runebender::document::model::glyph_metadata::Metaballs,
        drag: bool,
    ) -> Result<bool, String> {
        if !drag {
            return Ok(self
                .stage_canonical_edit("edit metaballs", move |draft| draft.set_metaballs(source)));
        }
        if self.active_metaball_drag.is_none() {
            let Some(transaction) = self.canonical_base.clone() else {
                return Ok(false);
            };
            self.active_metaball_drag = Some(CanonicalGesture {
                transaction,
                changed: false,
            });
        }
        let gesture = self.active_metaball_drag.as_mut().expect("initialized");
        let changed = gesture
            .transaction
            .draft_mut()
            .set_metaballs(source)
            .map_err(|error| error.to_string())?;
        gesture.changed |= changed;
        self.in_drag = true;
        Ok(changed)
    }

    pub(crate) fn end_metaball_drag(&mut self) {
        if let Some(drag) = self.active_metaball_drag.take()
            && drag.changed
        {
            self.pending_canonical = Some(drag.transaction);
            self.pending_canonical_label = Some("metaball drag");
        }
        self.in_drag = false;
    }

    pub(crate) fn cancel_metaball_drag(&mut self) {
        self.active_metaball_drag = None;
        self.in_drag = false;
        self.refresh_metaball_preview();
    }

    pub(crate) fn outline_arc(&self) -> Arc<BezPath> {
        Arc::new(self.outline())
    }

    pub(crate) fn components_arc(&self) -> Arc<BezPath> {
        Arc::new(self.components.clone())
    }

    pub(crate) fn selected_component_path(&self) -> Option<&BezPath> {
        let selected = self.selected_component?;
        self.component_cache
            .iter()
            .find(|component| component.id == selected)
            .map(|component| &component.path)
    }

    pub(crate) fn component_at(&self, point: Point) -> Option<ComponentId> {
        use kurbo::Shape as _;
        self.component_cache
            .iter()
            .rev()
            .find(|component| component.path.contains(point))
            .map(|component| component.id)
    }

    pub(crate) fn select_component(&mut self, index: usize) -> bool {
        let Some(component) = self.component_cache.get(index) else {
            return false;
        };
        self.selection.clear();
        self.selected_anchor = None;
        self.selected_component = Some(component.id);
        true
    }

    pub(crate) fn select_component_id(&mut self, id: ComponentId) -> bool {
        let Some(index) = self
            .component_cache
            .iter()
            .position(|component| component.id == id)
        else {
            return false;
        };
        self.select_component(index)
    }

    pub(crate) fn component_selected(&self, index: usize) -> bool {
        self.component_cache
            .get(index)
            .is_some_and(|component| Some(component.id) == self.selected_component)
    }

    fn selected_component_index(&self) -> Option<usize> {
        let selected = self.selected_component?;
        self.component_cache
            .iter()
            .position(|component| component.id == selected)
    }

    pub(crate) fn selected_component_aligned(&self) -> Option<bool> {
        let selected = self.selected_component?;
        self.canonical_base
            .as_ref()?
            .draft()
            .view()
            .components()
            .find(|component| component.id() == selected)
            .map(|component| !component.alignment_disabled())
    }

    fn rebuild_combined_components(&mut self) {
        self.components = combined_component_path(&self.component_cache);
    }

    pub(crate) fn drag_component_by(&mut self, dx: f64, dy: f64) -> bool {
        if self.selected_component.is_none()
            || self.selected_component_aligned() == Some(true)
            || !dx.is_finite()
            || !dy.is_finite()
            || (dx == 0.0 && dy == 0.0)
        {
            return false;
        }
        let selected = self.selected_component.expect("checked above");
        if self.active_component_drag.is_none() {
            let Some(transaction) = self.canonical_base.clone() else {
                return false;
            };
            self.active_component_drag = Some(CanonicalComponentDrag {
                gesture: CanonicalGesture {
                    transaction,
                    changed: false,
                },
                component_cache: self.component_cache.clone(),
                components: self.components.clone(),
            });
            self.in_drag = true;
        }
        let changed = {
            let drag = self.active_component_drag.as_mut().expect("initialized");
            let Some(transform) = drag
                .gesture
                .transaction
                .draft()
                .view()
                .components()
                .find(|component| component.id() == selected)
                .map(|component| component.transform())
            else {
                return false;
            };
            let Ok(changed) = drag
                .gesture
                .transaction
                .draft_mut()
                .set_component_transform(selected, kurbo::Affine::translate((dx, dy)) * transform)
            else {
                return false;
            };
            drag.gesture.changed |= changed;
            changed
        };
        if changed {
            let Some(index) = self.selected_component_index() else {
                return false;
            };
            if let Some(component) = self.component_cache.get_mut(index) {
                component.path = kurbo::Affine::translate((dx, dy)) * component.path.clone();
            }
            self.rebuild_combined_components();
        }
        changed
    }

    pub(crate) fn end_component_drag(&mut self) {
        if let Some(drag) = self.active_component_drag.take()
            && drag.gesture.changed
        {
            self.pending_canonical = Some(drag.gesture.transaction);
            self.pending_canonical_label = Some("component drag");
        }
        self.in_drag = false;
    }

    pub(crate) fn cancel_component_drag(&mut self) {
        let Some(drag) = self.active_component_drag.take() else {
            return;
        };
        self.component_cache = drag.component_cache;
        self.components = drag.components;
        self.in_drag = false;
    }

    pub(crate) fn outline(&self) -> BezPath {
        let mut path = self.current_layer().map_or_else(BezPath::new, |layer| {
            glyph_paths::ordinary_layer_contours_to_bezpath(layer)
        });
        path.extend(self.metaball_preview.clone());
        path
    }

    pub(crate) fn points(&self) -> Vec<PointView> {
        let mut out = Vec::new();
        let Some(layer) = self.current_layer() else {
            return out;
        };
        for contour in layer.contours() {
            for (index, point) in contour.points().enumerate() {
                let point_type = point.point_type();
                let on_curve = !matches!(point_type, LayerPointType::OffCurve);
                out.push(PointView {
                    id: point.id(),
                    point: point.position(),
                    on_curve,
                    smooth: on_curve && point.is_smooth(),
                    start: index == 0,
                });
            }
        }
        out
    }

    pub(crate) fn point_count(&self) -> usize {
        self.current_layer()
            .map(|layer| {
                layer
                    .contours()
                    .map(|contour| contour.points().count())
                    .sum()
            })
            .unwrap_or_default()
    }

    pub(crate) fn outline_is_empty(&self) -> bool {
        self.current_layer().is_none_or(|layer| {
            layer.contours().next().is_none() && layer.components().next().is_none()
        })
    }

    // ---- edits ----

    /// Rebase the session on the canonical layer while retaining stable selections.
    pub(crate) fn reload_from_project(
        &mut self,
        project: &runebender::document::project::Project,
        address: &runebender::document::variable::GlyphLayerAddress,
    ) -> bool {
        if project
            .document_layer(&address.glyph, &address.layer)
            .is_none()
        {
            return false;
        }
        let Ok(component_cache) = resolved_document_components(project, address) else {
            return false;
        };
        let canonical_base = project.begin_document_layer_transaction(address).ok();
        self.reload_parts(component_cache, canonical_base);
        true
    }

    fn reload_parts(
        &mut self,
        component_cache: Vec<runebender::outline::component_ops::ResolvedDocumentComponent>,
        canonical_base: Option<CanonicalLayerTransaction>,
    ) {
        let selected_ids = self.selection.clone();
        self.components = combined_component_path(&component_cache);
        self.component_cache = component_cache;
        self.selected_component = self.selected_component.filter(|selected| {
            self.component_cache
                .iter()
                .any(|component| component.id == *selected)
        });
        self.canonical_base = canonical_base;
        self.pending_canonical = None;
        self.pending_canonical_label = None;
        self.sync_rejected = false;
        self.active_point_drag = None;
        self.active_component_drag = None;
        self.active_anchor_drag = None;
        self.active_metric_drag = None;
        self.active_metaball_drag = None;
        self.active_hyper_contour = self.active_hyper_contour.filter(|active| {
            self.current_layer()
                .is_some_and(|layer| layer.contours().any(|contour| contour.id() == *active))
        });
        self.selection = selected_ids;
        self.refresh_metaball_preview();
        self.in_drag = false;
        let anchors: HashSet<_> = self
            .current_layer()
            .into_iter()
            .flat_map(|layer| layer.anchors().map(|anchor| anchor.id()))
            .collect();
        self.selected_anchor = self
            .selected_anchor
            .filter(|selected| anchors.contains(selected));
        self.prune_selection();
    }

    fn prune_selection(&mut self) {
        let available: HashSet<_> = self
            .current_layer()
            .into_iter()
            .flat_map(|layer| {
                layer
                    .contours()
                    .flat_map(|contour| contour.points().map(|point| point.id()))
            })
            .collect();
        self.selection.retain(|id| available.contains(id));
    }

    pub(crate) fn begin_point_drag(&mut self) {
        let Some(transaction) = self.canonical_base.clone() else {
            return;
        };
        let selected: Vec<_> = self.selection.iter().copied().collect();
        let Ok(origins) = transaction
            .draft()
            .view()
            .point_drag_origins(&selected, false)
        else {
            return;
        };
        self.active_point_drag = Some(CanonicalPointDrag {
            transaction,
            origins,
            changed: false,
        });
        self.in_drag = true;
    }

    /// Move the selection to `total` design units from where the drag began.
    pub(crate) fn drag_points_to(&mut self, total: (f64, f64)) -> bool {
        let selected: Vec<_> = self.selection.iter().copied().collect();
        let Some(drag) = &mut self.active_point_drag else {
            return false;
        };
        let Ok(changed) = drag.transaction.draft_mut().translate_points(
            &selected,
            &drag.origins,
            kurbo::Vec2::new(total.0, total.1),
            false,
        ) else {
            return false;
        };
        drag.changed |= changed;
        changed
    }

    pub(crate) fn end_point_drag(&mut self) {
        if let Some(drag) = self.active_point_drag.take()
            && drag.changed
        {
            self.pending_canonical = Some(drag.transaction);
            self.pending_canonical_label = Some("point drag");
        }
        self.in_drag = false;
    }

    pub(crate) fn cancel_point_drag(&mut self) {
        self.active_point_drag = None;
        self.in_drag = false;
    }

    pub(crate) fn nudge(&mut self, dx: f64, dy: f64) -> bool {
        if !dx.is_finite() || !dy.is_finite() || (dx == 0.0 && dy == 0.0) {
            return false;
        }
        if let Some(component) = self.selected_component {
            if self.selected_component_aligned() == Some(true) {
                return false;
            }
            return self.stage_canonical_edit("nudge component", |draft| {
                let transform = draft
                    .view()
                    .components()
                    .find(|candidate| candidate.id() == component)
                    .map(|candidate| candidate.transform())
                    .ok_or(runebender::document::DocumentEditError::MissingComponent(
                        component,
                    ))?;
                draft.set_component_transform(
                    component,
                    kurbo::Affine::translate((dx, dy)) * transform,
                )
            });
        }
        if self.selection.is_empty() {
            return false;
        }
        let selection = self.selected_point_ids();
        self.stage_canonical_edit("nudge points", |draft| {
            draft.translate_points(&selection, &[], kurbo::Vec2::new(dx, dy), false)
        })
    }

    pub(crate) fn delete_selected(&mut self) -> bool {
        if let Some(component) = self.selected_component {
            let changed = self.stage_canonical_edit("delete component", |draft| {
                draft.remove_component(component)
            });
            if changed {
                self.selected_component = None;
            }
            return changed;
        }
        if self.selection.is_empty() {
            return false;
        }
        let selection = self.selected_point_ids();
        let changed =
            self.stage_canonical_edit("delete points", |draft| draft.delete_points(&selection));
        if changed {
            self.selection.clear();
        }
        changed
    }

    /// The first point of the pen buffer, in design space.
    pub(crate) fn pen_first_point(&self) -> Option<Point> {
        self.pen.first().map(|p| p.point)
    }

    pub(crate) fn pen_is_active(&self) -> bool {
        !self.pen.is_empty()
    }

    pub(crate) fn pen_checkpoint(&self) -> (usize, Option<usize>) {
        (self.pen.len(), self.active_contour)
    }

    pub(crate) fn cancel_pen_gesture(&mut self, point_count: usize, active_contour: Option<usize>) {
        self.pen.truncate(point_count);
        self.active_contour = active_contour;
        self.pending_canonical = None;
        self.pending_canonical_label = None;
    }

    /// Write the pen buffer into `active_contour`, creating it if needed.
    fn pen_sync(&mut self) {
        let mut points = Vec::with_capacity(self.pen.len());
        let mut prev_off = false;
        for (i, pt) in self.pen.iter().enumerate() {
            let typ = if i == 0 {
                norad::PointType::Move
            } else if pt.off {
                norad::PointType::OffCurve
            } else if prev_off {
                norad::PointType::Curve
            } else {
                norad::PointType::Line
            };
            points.push(norad::ContourPoint::new(
                pt.point.x, pt.point.y, typ, pt.smooth, None, None,
            ));
            prev_off = pt.off;
        }
        let contour_count = self
            .current_layer()
            .map(|layer| layer.contours().count())
            .unwrap_or_default();
        let contour = self
            .active_contour
            .filter(|contour| *contour < contour_count)
            .unwrap_or(contour_count);
        let changed = self.compatibility_edit("pen contour", move |glyph| {
            if contour == glyph.contours.len() {
                glyph.contours.push(norad::Contour::new(Vec::new(), None));
            }
            let Some(target) = glyph.contours.get_mut(contour) else {
                return false;
            };
            target.points = points;
            true
        });
        if changed {
            self.active_contour = Some(contour);
        }
    }

    /// Place a corner on-curve point (a plain click).
    pub(crate) fn pen_corner(&mut self, x: f64, y: f64) {
        self.pen.push(PenPt {
            point: Point::new(x, y),
            off: false,
            smooth: false,
        });
        self.pen_sync();
    }

    /// Begin a smooth point with symmetric handles at `origin`; the outgoing
    /// handle starts at `to`.
    pub(crate) fn pen_smooth_begin(&mut self, origin: Point, to: Point) {
        self.pen.push(PenPt {
            point: origin,
            off: true,
            smooth: false,
        });
        self.pen.push(PenPt {
            point: origin,
            off: false,
            smooth: true,
        });
        self.pen.push(PenPt {
            point: to,
            off: true,
            smooth: false,
        });
        self.pen_sync();
    }

    /// Update the handles of the smooth point currently being dragged.
    pub(crate) fn pen_smooth_drag(&mut self, origin: Point, to: Point) {
        let n = self.pen.len();
        if n < 3 {
            return;
        }
        self.pen[n - 1].point = to;
        self.pen[n - 3].point = Point::new(2.0 * origin.x - to.x, 2.0 * origin.y - to.y);
        self.pen_sync();
    }

    /// Close the active contour.
    pub(crate) fn pen_close(&mut self) {
        if let Some(c) = self.active_contour.take() {
            let _ = self.compatibility_edit("close pen contour", move |glyph| {
                let Some(contour) = glyph.contours.get_mut(c) else {
                    return false;
                };
                if contour.points.first().map(|point| point.typ) != Some(norad::PointType::Move)
                    || contour.points.len() <= 1
                {
                    return false;
                }
                let first = contour.points.remove(0);
                let typ = if contour
                    .points
                    .last()
                    .is_some_and(|point| point.typ == norad::PointType::OffCurve)
                {
                    norad::PointType::Curve
                } else {
                    norad::PointType::Line
                };
                contour.points.push(norad::ContourPoint::new(
                    first.x,
                    first.y,
                    typ,
                    first.smooth,
                    None,
                    None,
                ));
                true
            });
        }
        self.pen.clear();
    }

    /// End the current pen path without closing (Escape / tool switch).
    pub(crate) fn pen_cancel(&mut self) {
        self.active_contour = None;
        self.pen.clear();
        self.active_hyper_contour = None;
    }

    // ---- hyperbezier pen: on-curve points only, curve solved by the spline ----

    /// Add a hyperbezier on-curve point (smooth), starting a contour if idle.
    pub(crate) fn hyper_add(&mut self, x: f64, y: f64, corner: bool) {
        let Some(mut transaction) = self.canonical_base.clone() else {
            return;
        };
        if let Some(contour) = self.active_hyper_contour {
            if transaction
                .draft_mut()
                .append_hyper_point(contour, Point::new(x, y), corner)
                .is_err()
            {
                return;
            }
            self.pending_canonical_label = Some("append hyperbezier");
        } else {
            let Ok((contour, _)) = transaction
                .draft_mut()
                .start_hyper_contour(Point::new(x, y))
            else {
                return;
            };
            self.active_hyper_contour = Some(contour);
            self.pending_canonical_label = Some("start hyperbezier");
        }
        self.pending_canonical = Some(transaction);
    }

    pub(crate) fn hyper_close(&mut self) {
        let Some(contour) = self.active_hyper_contour else {
            return;
        };
        let Some(mut transaction) = self.canonical_base.clone() else {
            return;
        };
        if transaction
            .draft_mut()
            .close_hyper_contour(contour)
            .is_err()
        {
            return;
        }
        self.active_hyper_contour = None;
        self.pending_canonical = Some(transaction);
        self.pending_canonical_label = Some("close hyperbezier");
    }

    pub(crate) fn first_contour_point(&self) -> Option<Point> {
        let active = self.active_hyper_contour?;
        self.current_layer()?
            .contours()
            .find(|contour| contour.id() == active)?
            .points()
            .next()
            .map(|point| point.position())
    }

    pub(crate) fn hyper_is_active(&self) -> bool {
        self.active_hyper_contour.is_some()
    }

    pub(crate) fn contour_drawing_is_active(&self) -> bool {
        self.active_contour.is_some() || self.active_hyper_contour.is_some()
    }

    /// Add a closed rectangle contour.
    pub(crate) fn add_rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        let (lx, rx) = (x0.min(x1), x0.max(x1));
        let (by, ty) = (y0.min(y1), y0.max(y1));
        if (rx - lx).abs() < 1.0 || (ty - by).abs() < 1.0 {
            return;
        }
        let rect = Rect::new(lx, by, rx, ty);
        let _ = self.stage_canonical_edit("add rectangle", |draft| {
            draft.add_shape_contour(rect, false).map(|_| true)
        });
    }

    /// Add a closed ellipse contour (four cubic segments).
    pub(crate) fn add_ellipse(&mut self, x0: f64, y0: f64, x1: f64, y1: f64) {
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (rx, ry) = ((x1 - x0).abs() / 2.0, (y1 - y0).abs() / 2.0);
        if rx < 1.0 || ry < 1.0 {
            return;
        }
        let rect = Rect::new(cx - rx, cy - ry, cx + rx, cy + ry);
        let _ = self.stage_canonical_edit("add ellipse", |draft| {
            draft.add_shape_contour(rect, true).map(|_| true)
        });
    }

    /// Apply an affine to the selection (or the whole glyph if none),
    /// centered on the target bounding box.
    pub(crate) fn transform(&mut self, affine: kurbo::Affine) -> bool {
        let selection = self.selected_point_ids();
        let changed = self.stage_canonical_edit("transform points", |draft| {
            draft.transform_points(&selection, affine)
        });
        if changed {
            self.last_transform = Some(affine);
        }
        changed
    }

    pub(crate) fn flip_horizontal(&mut self) -> bool {
        self.transform(kurbo::Affine::new([-1.0, 0.0, 0.0, 1.0, 0.0, 0.0]))
    }

    pub(crate) fn flip_vertical(&mut self) -> bool {
        self.transform(kurbo::Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, 0.0]))
    }

    /// Rotate counterclockwise in the font's upward-Y coordinate system.
    pub(crate) fn rotate_90(&mut self) -> bool {
        self.transform(kurbo::Affine::new([0.0, 1.0, -1.0, 0.0, 0.0, 0.0]))
    }

    /// Rotate clockwise around the selected geometry's center.
    pub(crate) fn rotate_90_clockwise(&mut self) -> bool {
        self.transform(kurbo::Affine::new([0.0, -1.0, 1.0, 0.0, 0.0, 0.0]))
    }

    /// Expand selected contours (or all contours) under one undo record.
    pub(crate) fn expand_stroke(&mut self, width: f64) -> bool {
        if !width.is_finite() || width <= 0.0 || self.selected_component.is_some() {
            return false;
        }
        let selection = self.selected_point_ids();
        let changed = self.stage_canonical_edit("expand stroke", |draft| {
            draft.expand_stroke(&selection, width)
        });
        if changed {
            self.selection.clear();
        }
        changed
    }

    pub(crate) fn offset(&mut self, delta: f64) -> bool {
        let changed =
            self.stage_canonical_edit("offset contours", |draft| draft.offset_contours(delta));
        if changed {
            self.selection.clear();
        }
        changed
    }

    pub(crate) fn extrude(&mut self, offset: f64, angle: f64, keep_front: bool) -> bool {
        let changed = self.stage_canonical_edit("extrude contours", |draft| {
            draft.extrude_contours(offset, angle, keep_front)
        });
        if changed {
            self.selection.clear();
        }
        changed
    }

    pub(crate) fn roughen(
        &mut self,
        segment: f64,
        horizontal: f64,
        vertical: f64,
        seed: u64,
    ) -> bool {
        let selection = self.selected_point_ids();
        let changed = self.stage_canonical_edit("roughen contours", |draft| {
            draft.roughen_contours(&selection, segment, horizontal, vertical, seed)
        });
        if changed {
            self.selection.clear();
        }
        changed
    }

    pub(crate) fn reverse(&mut self) -> bool {
        let selection = self.selected_point_ids();
        self.stage_canonical_edit("reverse contours", |draft| {
            draft.reverse_contours(&selection)
        })
    }

    pub(crate) fn decompose(&mut self) -> bool {
        if self.component_cache.is_empty() {
            return false;
        }
        let Some(mut transaction) = self.canonical_base.clone() else {
            return false;
        };
        let resolved: Vec<_> = self
            .component_cache
            .iter()
            .flat_map(|component| component.contours.iter().cloned())
            .collect();
        if transaction.draft_mut().decompose_components(&resolved) != Ok(true) {
            return false;
        }
        self.pending_canonical = Some(transaction);
        self.pending_canonical_label = Some("decompose components");
        self.components = BezPath::new();
        self.component_cache.clear();
        self.selected_component = None;
        true
    }

    pub(crate) fn boolean(&mut self, op: BoolOp) -> bool {
        let op = match op {
            BoolOp::Union => linesweeper::BinaryOp::Union,
            BoolOp::Subtract => linesweeper::BinaryOp::Difference,
            BoolOp::Intersect => linesweeper::BinaryOp::Intersection,
            BoolOp::Exclude => linesweeper::BinaryOp::Xor,
        };
        let changed =
            self.stage_canonical_edit("boolean contours", |draft| draft.boolean_contours(op));
        if changed {
            self.selection.clear();
        }
        changed
    }

    pub(crate) fn remove_overlap(&mut self) -> bool {
        let changed = self.stage_canonical_edit("remove overlap", |draft| draft.remove_overlap());
        if changed {
            self.selection.clear();
        }
        changed
    }

    /// Points where a knife line from p0 to p1 crosses the outline.
    pub(crate) fn knife_hits(&self, p0: Point, p1: Point) -> Vec<Point> {
        self.current_layer().map_or_else(Vec::new, |layer| {
            runebender::outline::knife::knife_hit_points_in_layer(layer, p0, p1)
        })
    }

    /// Cut the outline along the line p0..p1.
    pub(crate) fn knife_cut(&mut self, p0: Point, p1: Point) -> bool {
        self.stage_canonical_edit("knife cut", |draft| draft.knife_cut(p0, p1))
    }

    /// The glyph's contours as core `Path`s (for measurement/analysis).
    pub(crate) fn paths(&self) -> Vec<runebender::outline::path::Path> {
        self.current_layer()
            .into_iter()
            .flat_map(|layer| {
                layer
                    .contours()
                    .map(runebender::outline::path::Path::from_document_contour)
            })
            .collect()
    }

    pub(crate) fn measurements(&self) -> Vec<runebender::analysis::measure::Measurement> {
        runebender::analysis::measure::glyph_measurements(&self.paths())
    }

    pub(crate) fn side_bearings(&self) -> Option<runebender::analysis::measure::SideBearings> {
        runebender::analysis::measure::side_bearings(&self.paths(), self.advance())
    }

    /// Bounding box of the selected points in design space, if any.
    pub(crate) fn selection_bounds(&self) -> Option<Rect> {
        let mut min = (f64::INFINITY, f64::INFINITY);
        let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for point in self
            .points()
            .into_iter()
            .filter(|point| self.selection.contains(&point.id))
        {
            min = (min.0.min(point.point.x), min.1.min(point.point.y));
            max = (max.0.max(point.point.x), max.1.max(point.point.y));
        }
        if min.0.is_finite() {
            Some(Rect::new(min.0, min.1, max.0, max.1))
        } else {
            None
        }
    }

    fn first_selected_point(&self) -> Option<PointId> {
        self.current_layer()?.contours().find_map(|contour| {
            contour
                .points()
                .find(|point| self.selection.contains(&point.id()))
                .map(|point| point.id())
        })
    }

    fn selected_point_ids(&self) -> Vec<PointId> {
        let Some(layer) = self.current_layer() else {
            return Vec::new();
        };
        self.canonical_selection(layer)
    }

    /// Make the first selected on-curve point the start of its contour.
    pub(crate) fn set_start(&mut self) -> bool {
        let Some(point) = self.first_selected_point() else {
            return false;
        };
        let changed =
            self.stage_canonical_edit("set contour start", |draft| draft.set_contour_start(point));
        if changed {
            self.selection.clear();
        }
        changed
    }

    /// Round the selected corner points (fillet).
    pub(crate) fn round_corners(&mut self) -> bool {
        let selection = self.selected_point_ids();
        let Some(mut transaction) = self.canonical_base.clone() else {
            return false;
        };
        let Ok(Some(next)) = transaction.draft_mut().round_selected_corners(&selection) else {
            return false;
        };
        self.selection = next.into_iter().collect();
        self.pending_canonical = Some(transaction);
        self.pending_canonical_label = Some("round corners");
        true
    }

    pub(crate) fn harmonize(&mut self) -> bool {
        let selection = self.selected_point_ids();
        self.stage_canonical_edit("harmonize handles", |draft| {
            draft.harmonize_handles(&selection)
        })
    }

    pub(crate) fn balance(&mut self) -> bool {
        let selection = self.selected_point_ids();
        self.stage_canonical_edit("balance handles", |draft| draft.balance_handles(&selection))
    }

    pub(crate) fn optimize(&mut self) -> bool {
        let selection = self.selected_point_ids();
        self.stage_canonical_edit("optimize handles", |draft| {
            draft.optimize_handles(&selection, 0.12)
        })
    }

    pub(crate) fn duplicate(&mut self) -> bool {
        if let Some(component) = self.selected_component {
            let Some(mut transaction) = self.canonical_base.clone() else {
                return false;
            };
            let Some((reference, transform)) = transaction
                .draft()
                .view()
                .components()
                .find(|candidate| candidate.id() == component)
                .map(|candidate| (candidate.reference().to_owned(), candidate.transform()))
            else {
                return false;
            };
            let transform = kurbo::Affine::translate((20.0, 20.0)) * transform;
            let Ok(next) = transaction.draft_mut().add_component(reference, transform) else {
                return false;
            };
            self.selected_component = Some(next);
            self.pending_canonical = Some(transaction);
            self.pending_canonical_label = Some("duplicate component");
            return true;
        }
        let selection = self.selected_point_ids();
        let Some(mut transaction) = self.canonical_base.clone() else {
            return false;
        };
        let Ok(next) = transaction
            .draft_mut()
            .duplicate_contours(&selection, kurbo::Vec2::new(20.0, 20.0))
        else {
            return false;
        };
        if next.points.is_empty() {
            return false;
        }
        self.selection = next.points.into_iter().collect();
        self.pending_canonical = Some(transaction);
        self.pending_canonical_label = Some("duplicate contours");
        true
    }

    pub(crate) fn duplicate_repeat(&mut self) -> bool {
        let transform = self.last_transform;
        if !self.duplicate() {
            return false;
        }
        if let Some(transform) = transform
            && self.selected_component.is_none()
            && let Some(transaction) = &mut self.pending_canonical
        {
            let selection: Vec<_> = self.selection.iter().copied().collect();
            let _ = transaction
                .draft_mut()
                .transform_points(&selection, transform);
        }
        true
    }

    pub(crate) fn tidy_paths(&mut self) -> bool {
        self.stage_canonical_edit("tidy paths", |draft| Ok(draft.tidy_contours() > 0))
    }

    pub(crate) fn add_extremes(&mut self) -> bool {
        let selection = self.selected_point_ids();
        self.stage_canonical_edit("add extremes", |draft| draft.add_extreme_points(&selection))
    }

    pub(crate) fn round_coordinates(&mut self) -> bool {
        self.stage_canonical_edit("round coordinates", |draft| {
            Ok(draft.round_coordinates() > 0)
        })
    }

    pub(crate) fn correct_path_direction(&mut self) -> bool {
        self.stage_canonical_edit("correct path direction", |draft| {
            draft.correct_path_directions().map(|count| count > 0)
        })
    }

    pub(crate) fn hyper_to_cubic(&mut self) -> bool {
        let selection = self.selected_point_ids();
        let changed = self.stage_canonical_edit("convert hyperbeziers", |draft| {
            draft.convert_hyper_to_cubic(&selection)
        });
        if changed {
            self.selection.clear();
        }
        changed
    }

    pub(crate) fn quads_to_cubics(&mut self) -> bool {
        self.stage_canonical_edit("convert quadratics", |draft| {
            draft.convert_quadratics_to_cubics()
        })
    }

    pub(crate) fn cubics_to_quads(&mut self) -> bool {
        self.stage_canonical_edit("convert cubics", |draft| {
            draft.convert_cubics_to_quadratics(1.0)
        })
    }

    /// Stable identity of the anchor near `p` (design space), if within `tol`.
    pub(crate) fn anchor_at(&self, p: Point, tol: f64) -> Option<AnchorId> {
        self.current_layer()?
            .anchors()
            .filter(|anchor| anchor.position().distance(p) <= tol)
            .min_by(|a, b| {
                a.position()
                    .distance(p)
                    .total_cmp(&b.position().distance(p))
            })
            .map(|anchor| anchor.id())
    }

    #[cfg(test)]
    pub(crate) fn anchor_id_at(&self, index: usize) -> Option<AnchorId> {
        self.current_layer()?
            .anchors()
            .nth(index)
            .map(|anchor| anchor.id())
    }

    pub(crate) fn add_anchor(&mut self, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        let n = self
            .current_layer()
            .map(|layer| layer.anchors().count())
            .unwrap_or_default();
        let Some(mut transaction) = self.canonical_base.clone() else {
            return;
        };
        let Ok(anchor) = transaction
            .draft_mut()
            .add_anchor(format!("anchor.{n}"), Point::new(x, y))
        else {
            return;
        };
        self.selected_anchor = Some(anchor);
        self.pending_canonical = Some(transaction);
        self.pending_canonical_label = Some("add anchor");
    }

    pub(crate) fn move_anchor(&mut self, id: AnchorId, x: f64, y: f64) {
        let Some(anchor) = self.current_layer().and_then(|layer| {
            layer
                .anchors()
                .find(|candidate| candidate.id() == id)
                .map(|anchor| anchor.position())
        }) else {
            return;
        };
        if !x.is_finite() || !y.is_finite() || (anchor.x == x && anchor.y == y) {
            return;
        }
        if self.active_anchor_drag.is_none() {
            let Some(transaction) = self.canonical_base.clone() else {
                return;
            };
            self.active_anchor_drag = Some(CanonicalGesture {
                transaction,
                changed: false,
            });
            self.in_drag = true;
        }
        let drag = self.active_anchor_drag.as_mut().expect("initialized");
        let Ok(changed) = drag
            .transaction
            .draft_mut()
            .set_anchor_position(id, Point::new(x, y))
        else {
            return;
        };
        drag.changed |= changed;
    }

    pub(crate) fn end_anchor_drag(&mut self) {
        if let Some(drag) = self.active_anchor_drag.take()
            && drag.changed
        {
            self.pending_canonical = Some(drag.transaction);
            self.pending_canonical_label = Some("anchor drag");
        }
        self.in_drag = false;
    }

    pub(crate) fn cancel_anchor_drag(&mut self) {
        self.active_anchor_drag = None;
        self.in_drag = false;
    }

    pub(crate) fn delete_selected_anchor(&mut self) -> bool {
        let Some(id) = self.selected_anchor.take() else {
            return false;
        };
        let Some(mut transaction) = self.canonical_base.clone() else {
            return false;
        };
        if transaction.draft_mut().remove_anchor(id) != Ok(true) {
            return false;
        }
        self.pending_canonical = Some(transaction);
        self.pending_canonical_label = Some("delete anchor");
        true
    }

    /// Continuity of every on-curve node: corner, kink, G1, G2, G3.
    pub(crate) fn continuity(&self) -> Vec<runebender::analysis::curve::NodeContinuity> {
        let Some(layer) = self.current_layer() else {
            return Vec::new();
        };
        let cubics = runebender::analysis::curve::cubics_from_layer(layer);
        runebender::analysis::curve::node_continuity(&cubics)
    }

    /// The outline split into strokes colored by segment length, the web
    /// editor's colorize mode.
    pub(crate) fn colored_strokes(&self) -> Vec<runebender::analysis::measure::ColoredStroke> {
        runebender::analysis::measure::colored_strokes(&self.paths())
    }

    pub(crate) fn curvature_comb(&self) -> Vec<Vec<runebender::analysis::curve::CombSample>> {
        let Some(layer) = self.current_layer() else {
            return Vec::new();
        };
        let cubics = runebender::analysis::curve::cubics_from_layer(layer);
        let maxk = runebender::analysis::curve::max_curvature(&cubics);
        if maxk <= 1e-12 {
            return Vec::new();
        }
        runebender::analysis::curve::curvature_comb(&cubics, 1.0, 74.0 / maxk, false, 16)
    }

    pub(crate) fn set_mark(&mut self, label: Option<&str>) {
        let label = label.map(str::to_owned);
        let _ = self.compatibility_edit("set glyph mark", move |glyph| {
            let before = glyph.lib.clone();
            runebender::ui::theme::set_glyph_mark(glyph, label.as_deref());
            glyph.lib != before
        });
    }

    /// The contours to copy: the ones holding a selected point, or every
    /// contour when nothing is selected. This is shared with the web editor.
    #[cfg(test)]
    pub(crate) fn contours_for_copy(&self) -> Vec<norad::Contour> {
        let Some(transaction) = self.current_transaction() else {
            return Vec::new();
        };
        let glyph = transaction.compatibility_glyph();
        if self.selection.is_empty() {
            return glyph.contours;
        }
        let selected = self.legacy_selection();
        glyph
            .contours
            .iter()
            .enumerate()
            .filter(|(index, _)| selected.iter().any(|(contour, _)| contour == index))
            .map(|(_, contour)| contour.clone())
            .collect()
    }

    /// Replace every contour, keeping the advance. Used by the swap with
    /// the background layer, which is an edit like any other.
    pub(crate) fn set_contours(&mut self, contours: Vec<norad::Contour>) -> bool {
        let changed = self.compatibility_edit("replace contours", move |glyph| {
            if glyph.contours == contours {
                return false;
            }
            glyph.contours = contours;
            true
        });
        if changed {
            self.selection.clear();
        }
        changed
    }

    /// Append contours to the glyph, and select the points they brought.
    pub(crate) fn paste_contours(&mut self, contours: &[norad::Contour]) -> bool {
        if contours.is_empty() {
            return false;
        }
        let contours = contours.to_vec();
        let Some(mut transaction) = self.canonical_base.clone() else {
            return false;
        };
        let mut glyph = transaction.compatibility_glyph();
        let first_new = glyph.contours.len();
        glyph.contours.extend(contours.iter().cloned());
        if transaction.reconcile_compatibility_glyph(&glyph) != Ok(true) {
            return false;
        }
        let ids: Vec<Vec<_>> = transaction
            .draft()
            .view()
            .contours()
            .map(|contour| contour.points().map(|point| point.id()).collect())
            .collect();
        self.selection = ids.into_iter().skip(first_new).flatten().collect();
        self.pending_canonical = Some(transaction);
        self.pending_canonical_label = Some("paste compatibility contours");
        true
    }

    pub(crate) fn select_all(&mut self) {
        self.selected_component = None;
        self.selection = self.points().into_iter().map(|p| p.id).collect();
    }
}

fn resolved_document_components(
    project: &runebender::document::project::Project,
    address: &runebender::document::variable::GlyphLayerAddress,
) -> Result<
    Vec<runebender::outline::component_ops::ResolvedDocumentComponent>,
    glyph_paths::ComponentResolveError,
> {
    let root = project
        .document_layer(&address.glyph, &address.layer)
        .expect("the addressed canonical layer exists");
    runebender::outline::component_ops::resolved_document_components(
        root,
        |name| project.document_layer(name, &address.layer),
        |name| {
            project
                .document_glyph(name)
                .map(|glyph| {
                    glyph
                        .layer_ids()
                        .filter(|candidate| candidate.source == address.layer.source)
                        .filter_map(|candidate| glyph.layer(candidate))
                        .collect()
                })
                .unwrap_or_default()
        },
    )
}

fn combined_component_path(
    components: &[runebender::outline::component_ops::ResolvedDocumentComponent],
) -> BezPath {
    components
        .iter()
        .fold(BezPath::new(), |mut path, component| {
            path.extend(component.path.iter());
            path
        })
}

impl Workspace {
    /// The OKLCH themes in menu order.
    pub(crate) const THEMES: [&'static str; 3] = ["dark", "gray", "light"];

    fn text_context(&self) -> TextContext {
        TextContext {
            editor_text: self.initial_text.clone(),
            has_text_session: self.has_text_session,
            preview_text: self.preview_text.clone(),
            direction: self.text_dir,
            features_disabled: self.text_features_disabled.clone(),
            script: self.text_script.clone(),
            language: self.text_language.clone(),
        }
    }

    pub(crate) fn restore_text_context(&mut self, context: TextContext) {
        self.initial_text = context.editor_text;
        self.has_text_session = context.has_text_session;
        self.preview_text = context.preview_text;
        self.text_dir = context.direction;
        self.text_features_disabled = context.features_disabled;
        self.text_script = context.script;
        self.text_language = context.language;
    }

    /// Stable identity of the active tab's widget-owned text buffer.
    pub(crate) fn text_context_id(&self) -> (u64, u64) {
        (
            self.document_id,
            self.tabs
                .get(self.active_tab)
                .map_or(u64::MAX, |tab| tab.text_context_id),
        )
    }

    /// Keep the active tab's plain text copy in step with the editor widget.
    pub(crate) fn set_editor_text(&mut self, text: String) {
        self.begin_text_session();
        self.initial_text = text.clone();
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.text_context.editor_text = text;
        }
    }

    /// Open the active tab's text composition without coupling it to a tool.
    pub(crate) fn begin_text_session(&mut self) {
        self.has_text_session = true;
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.text_context.has_text_session = true;
        }
    }

    /// Pick a tool while preserving any text composition already on the tab.
    pub(crate) fn select_tool(&mut self, tool: Tool) {
        self.tool_before_space_pan = None;
        self.tool = tool;
        if tool == Tool::Metaball && self.session.metaballs.selected.is_empty() {
            Arc::make_mut(&mut self.session).select_all_metaballs();
        }
        if tool == Tool::Text {
            self.begin_text_session();
        }
    }

    /// Temporarily use the Hand tool while Space is held.
    pub(crate) fn begin_space_pan(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) || self.tool_before_space_pan.is_some() {
            return;
        }
        self.tool_before_space_pan = Some(self.tool);
        self.tool = Tool::Hand;
    }

    /// Restore the tool that was active before a Space-held pan.
    pub(crate) fn end_space_pan(&mut self) {
        if let Some(tool) = self.tool_before_space_pan.take() {
            self.tool = tool;
        }
    }

    fn persistent_tool(&self) -> Tool {
        self.tool_before_space_pan.unwrap_or(self.tool)
    }

    /// Write the live session back into its tab, so switching away from
    /// it does not lose the edit, the selection, or the undo stack.
    pub(crate) fn park(&mut self) {
        let text_context = self.text_context();
        let persistent_tool = self.persistent_tool();
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.session = self.session.clone();
            tab.tool = persistent_tool;
            tab.text_context = text_context;
        }
    }

    /// Leave the editor while keeping any parked session aligned to the active master.
    pub(crate) fn settle_on_overview(&mut self) {
        self.end_space_pan();
        self.mode = Mode::Overview;
        self.selected = None;
        if self.tabs.is_empty() {
            self.active_tab = 0;
            self.session = Arc::new(Session::inactive_from_model(&self.font));
        } else {
            self.active_tab = self.active_tab.min(self.tabs.len() - 1);
            let tab = &self.tabs[self.active_tab];
            let (session, tool, text_context) =
                (tab.session.clone(), tab.tool, tab.text_context.clone());
            self.session = session;
            self.tool = tool;
            self.restore_text_context(text_context);
        }
        self.selected_points = 0;
    }

    /// Make a tab the live one.
    pub(crate) fn activate_tab(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        let (session, tool, text_context) =
            (tab.session.clone(), tab.tool, tab.text_context.clone());
        self.park();
        self.active_tab = index;
        self.session = session;
        // An overview batch can change this glyph while its tab is parked.
        if let Some(address) = self.font.active_layer_address(&self.session.glyph_name) {
            let _ =
                Arc::make_mut(&mut self.session).reload_from_project(&self.font.project, &address);
        }
        self.tool_before_space_pan = None;
        self.tool = tool;
        self.restore_text_context(text_context);
        let name = self.session.glyph_name.clone();
        if let Some(glyph) = self.font.index_of(&name) {
            self.selected = Some(glyph);
            self.mode = Mode::Editor(glyph);
        }
        self.selected_points = 0;
        self.refresh_metric_bufs();
        self.refresh_coord_bufs();
        self.name_buf = name;
        self.unicode_buf = self
            .selected
            .and_then(|i| self.font.glyphs.get(i))
            .and_then(|g| g.codepoint)
            .map(|c| format!("{:04X}", c as u32))
            .unwrap_or_default();
    }

    /// A second tab on the glyph that is open, with its own session.
    pub(crate) fn new_tab(&mut self) {
        let Some(session) = Session::new_from_model(&self.font, &self.session.glyph_name) else {
            return;
        };
        let persistent_tool = self.persistent_tool();
        self.park();
        self.tabs.push(Tab {
            text_context_id: host::NEXT_TEXT_CONTEXT_ID
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            session: Arc::new(session),
            tool: persistent_tool,
            text_context: self.text_context(),
        });
        self.activate_tab(self.tabs.len() - 1);
    }

    /// Close a tab. Closing the last one leaves the editor.
    pub(crate) fn close_tab(&mut self, index: usize) {
        if index >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            self.back_to_overview();
            return;
        }
        self.park();
        self.tabs.remove(index);
        let next = if self.active_tab > index {
            self.active_tab - 1
        } else {
            self.active_tab.min(self.tabs.len() - 1)
        };
        self.active_tab = usize::MAX; // parking again would write to the wrong tab
        self.activate_tab(next);
    }

    pub(crate) fn open_glyph(&mut self, index: usize) {
        // A glyph that already has a tab gets that tab, rather than a
        // second one on the same glyph.
        if let Some(entry) = self.font.glyphs.get(index) {
            let name = entry.name.clone();
            if let Some(existing) = self
                .tabs
                .iter()
                .position(|tab| tab.session.glyph_name == name)
            {
                self.activate_tab(existing);
                return;
            }
        }
        self.replace_active_tab_glyph(index);
    }

    /// Edit one sort from the current text buffer without changing tabs or
    /// replacing that tab's text context.
    ///
    /// A grid open follows an existing glyph tab, but a sort activation is
    /// local to the word being edited. GPUI changes only the active edit
    /// session, and Web reloads only that sort's glyph metrics; both keep the
    /// surrounding text buffer intact.
    pub(crate) fn edit_text_sort_glyph(&mut self, index: usize, tool: Tool) {
        if self.replace_active_tab_glyph(index) {
            self.select_tool(tool);
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.tool = tool;
            }
        }
    }

    /// Replace the glyph session in the active tab while retaining the tab's
    /// stable identity and parked text/preview state.
    fn replace_active_tab_glyph(&mut self, index: usize) -> bool {
        if let Some(entry) = self.font.glyphs.get(index)
            && let Some(session) = Session::new_from_model(&self.font, &entry.name)
        {
            self.advance_buf = format!("{}", round_units(session.advance()));
            let (l, r) = metric_bufs(&session);
            self.lsb_buf = l;
            self.rsb_buf = r;
            self.name_buf = entry.name.clone();
            self.unicode_buf = entry
                .codepoint
                .map(|c| format!("{:04X}", c as u32))
                .unwrap_or_default();
            self.session = Arc::new(session);
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.session = self.session.clone();
            } else {
                self.tabs.push(Tab {
                    text_context_id: host::NEXT_TEXT_CONTEXT_ID
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                    session: self.session.clone(),
                    tool: self.persistent_tool(),
                    text_context: self.text_context(),
                });
                self.active_tab = self.tabs.len() - 1;
            }
            self.selected = Some(index);
            self.selected_points = 0;
            self.mode = Mode::Editor(index);
            return true;
        }
        false
    }

    /// After an edit, pull the glyph back out of the session and refresh
    /// the model + grid cache so the overview preview matches.
    /// Replace the app's session with the island's live one (called on every
    /// editor event so save/preview see interactive edits).
    pub(crate) fn sync_session_from(&mut self, session: &mut Session) -> SessionSyncOutcome {
        if session.sync_rejected {
            return SessionSyncOutcome::Rejected;
        }
        let name = session.glyph_name.clone();
        let mut outcome = SessionSyncOutcome::Unchanged;
        let mut retain_session = true;
        if let Some(mut transaction) = session.pending_canonical.take() {
            let label = session
                .pending_canonical_label
                .take()
                .unwrap_or("canonical editor edit");
            let address = transaction.address().clone();
            let alignment = if matches!(label, "add anchor" | "anchor drag" | "delete anchor") {
                let layer = address.layer.clone();
                let project = &self.font.project;
                runebender::document::composites::realign_document_layer(
                    transaction.draft_mut(),
                    |glyph| project.document_layer(glyph, &layer),
                    true,
                )
            } else {
                Ok(false)
            };
            let undo_depth = self
                .font
                .index_of(&name)
                .map(|index| self.font.master().undo_depth(index))
                .unwrap_or_default();
            let commit = alignment.map_err(|error| error.to_string()).and_then(|_| {
                self.font
                    .project
                    .commit_document_layer_transaction(transaction)
                    .map_err(|error| error.to_string())
            });
            match commit {
                Ok(runebender::document::project::DocumentEditOutcome::Changed { .. }) => {
                    outcome = SessionSyncOutcome::Changed;
                    self.metadata_undo.push(MetadataEdit::DocumentLayer {
                        glyph: name.clone(),
                        address: address.clone(),
                        label: label.into(),
                        undo_depth,
                    });
                    self.metadata_redo.clear();
                    if !session.reload_from_project(&self.font.project, &address) {
                        self.note = "The committed glyph layer could not be reloaded".into();
                        session.sync_rejected = true;
                        outcome = SessionSyncOutcome::Rejected;
                        retain_session = false;
                    }
                }
                Ok(runebender::document::project::DocumentEditOutcome::Unchanged { .. }) => {
                    if !session.reload_from_project(&self.font.project, &address) {
                        self.note = "The unchanged glyph layer could not be reloaded".into();
                        session.sync_rejected = true;
                        outcome = SessionSyncOutcome::Rejected;
                        retain_session = false;
                    }
                }
                Err(error) => {
                    self.note = format!("The active glyph layer changed before commit: {error}");
                    outcome = SessionSyncOutcome::Rejected;
                    if !session.reload_from_project(&self.font.project, &address) {
                        self.note
                            .push_str("; the canonical layer could not be reloaded");
                        session.sync_rejected = true;
                        retain_session = false;
                    }
                }
            }
        }
        if outcome == SessionSyncOutcome::Rejected {
            if retain_session {
                self.session = Arc::new(session.clone());
                self.refresh_metric_bufs();
                self.selected_points = self.session.selection.len();
            }
            return outcome;
        }
        self.session = Arc::new(session.clone());
        // Keep the panel's advance field in step after canvas edits
        // (sidebearing/advance drags). This path is never hit by typing in
        // the field, so it does not clobber input.
        self.refresh_metric_bufs();
        self.selected_points = self.session.selection.len();
        outcome
    }

    /// Rebase the open transitional session after a canonical layer commit or replay.
    pub(crate) fn reload_canonical_layer(
        &mut self,
        address: &runebender::document::variable::GlyphLayerAddress,
    ) -> bool {
        if self.font.project.source_id(self.font.active()) != Some(address.layer.source)
            || self.session.glyph_name != address.glyph
        {
            return false;
        }
        let mut session = (*self.session).clone();
        if !session.reload_from_project(&self.font.project, address) {
            return false;
        }
        self.session = Arc::new(session);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.session = self.session.clone();
        }
        if let Some(index) = self.font.index_of(&address.glyph) {
            self.font.refresh_entry(index);
        }
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.refresh_metric_bufs();
        self.refresh_coord_bufs();
        self.selected_points = self.session.selection.len();
        self.modified = true;
        true
    }

    /// Undo or redo the open glyph on the master's pile, then reload
    /// the session from the master.
    pub(crate) fn undo_open_glyph(&mut self, redo: bool) {
        let Mode::Editor(index) = self.mode else {
            return;
        };
        if self.metadata_history_step(redo) {
            return;
        }
        let mut master = self.font.master_mut();
        let done = if redo {
            master.redo(index)
        } else {
            master.undo(index)
        };
        if !done {
            self.note = if redo {
                "Nothing to redo"
            } else {
                "Nothing to undo"
            }
            .into();
            return;
        }
        drop(master);
        self.font.refresh_entry(index);
        let Some(address) = self.font.active_layer_address(&self.session.glyph_name) else {
            return;
        };
        let mut session = (*self.session).clone();
        if !session.reload_from_project(&self.font.project, &address) {
            return;
        }
        self.session = Arc::new(session);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.session = self.session.clone();
        }
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.refresh_metric_bufs();
        self.refresh_coord_bufs();
        self.selected_points = self.session.selection.len();
        self.modified = true;
        self.note.clear();
    }

    /// Undo in the active editing context. Overview actions may change a
    /// multi-selection, so their per-glyph engine snapshots travel as one batch.
    pub(crate) fn undo_active_edit(&mut self, redo: bool) {
        if matches!(self.mode, Mode::Editor(_)) {
            self.undo_open_glyph(redo);
            return;
        }
        if !matches!(self.mode, Mode::Overview) {
            return;
        }
        let has_overview_batch = if redo {
            !self.overview_redo.is_empty()
        } else {
            !self.overview_undo.is_empty()
        };
        if !has_overview_batch && self.metadata_history_step(redo) {
            return;
        }
        let batch = if redo {
            self.overview_redo.pop()
        } else {
            self.overview_undo.pop()
        };
        let Some(batch) = batch else {
            self.note = if redo {
                "Nothing to redo"
            } else {
                "Nothing to undo"
            }
            .into();
            return;
        };
        let Some(layer) = self
            .font
            .project
            .document_source(batch.source)
            .map(|source| source.default_layer())
        else {
            self.note = "Restore the removed source before undoing its glyph edits".into();
            if redo {
                self.overview_redo.push(batch);
            } else {
                self.overview_undo.push(batch);
            }
            return;
        };
        let direction = if redo {
            runebender::document::history::HistoryDirection::Redo
        } else {
            runebender::document::history::HistoryDirection::Undo
        };
        let addresses = batch
            .glyphs
            .iter()
            .map(|glyph| runebender::document::variable::GlyphLayerAddress {
                glyph: glyph.clone(),
                layer: layer.clone(),
            })
            .collect::<Vec<_>>();
        let canonical = addresses.iter().all(|address| {
            self.font
                .project
                .can_replay_document_layer_history(address, direction)
        });
        if canonical {
            for address in &addresses {
                if self
                    .font
                    .project
                    .replay_document_layer_history(address, direction)
                    .is_err()
                {
                    self.note = "The overview edit changed before history replay".into();
                    if redo {
                        self.overview_redo.push(batch);
                    } else {
                        self.overview_undo.push(batch);
                    }
                    return;
                }
            }
        } else {
            let Some(mut master) = self.font.project.edit_source(batch.source) else {
                return;
            };
            for glyph in &batch.glyphs {
                if let Some(&index) = master.name_map.get(glyph) {
                    if redo {
                        master.redo(index);
                    } else {
                        master.undo(index);
                    }
                }
            }
        }
        if Some(batch.source) == self.font.project.source_id(self.font.active()) {
            self.font.rebuild_cache();
        }
        if redo {
            self.overview_undo.push(batch);
        } else {
            self.overview_redo.push(batch);
        }
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.modified = true;
    }

    pub(crate) fn set_master(&mut self, index: usize) {
        if index == self.font.active() || index >= self.font.master_count() {
            return;
        }
        if self.features_edited {
            self.note = "Apply or Revert feature edits before switching masters".into();
            return;
        }
        self.park();
        self.font.set_active(index);
        self.refresh_source_views();
    }

    pub(crate) fn refresh_source_views(&mut self) {
        let active_name =
            matches!(self.mode, Mode::Editor(_)).then(|| self.session.glyph_name.clone());
        let index = self.font.active();
        self.source_name_buf = self.font.master_names()[index].to_string();
        self.features_buf = self.font.feature_text().to_owned();
        self.features_status = None;
        if self.show_all_masters {
            self.reference_layers = (0..self.font.master_count())
                .filter(|master| *master != index)
                .collect();
        }
        self.axis_values = self.font.master_axis_values(index);
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        // A parked session contains one master's glyph data. Rebuild every tab
        // by glyph name so activating another tab cannot write the old master
        // into the new one. Viewports and text contexts remain tab-local.
        let font = &self.font;
        self.tabs.retain_mut(|tab| {
            let name = tab.session.glyph_name.clone();
            let Some(mut session) = Session::new_from_model(font, &name) else {
                return false;
            };
            session.viewport = tab.session.viewport.clone();
            session.fitted = tab.session.fitted;
            tab.session = Arc::new(session);
            true
        });
        if let Some(name) = active_name {
            let tab = self
                .tabs
                .iter()
                .position(|tab| tab.session.glyph_name == name);
            let glyph = self.font.index_of(&name);
            if let (Some(tab), Some(glyph)) = (tab, glyph) {
                self.active_tab = tab;
                self.session = self.tabs[tab].session.clone();
                self.selected = Some(glyph);
                self.mode = Mode::Editor(glyph);
                self.name_buf = name;
                self.unicode_buf = self
                    .session
                    .codepoint()
                    .map(|codepoint| format!("{:04X}", codepoint as u32))
                    .unwrap_or_default();
                self.refresh_metric_bufs();
                self.refresh_coord_bufs();
                self.selected_points = 0;
            } else {
                self.settle_on_overview();
            }
        }
    }

    /// The current axis location as a name->value map (user units).
    pub(crate) fn axis_location(&self) -> HashMap<String, f64> {
        self.font
            .axes
            .iter()
            .zip(&self.axis_values)
            .map(|(a, v)| (a.name.clone(), *v))
            .collect()
    }

    /// True when the sliders sit exactly on the active master's location.
    pub(crate) fn on_active_master(&self) -> bool {
        match self.font.project.master_locations.get(self.font.active()) {
            Some(m) => self.font.axes.iter().enumerate().all(|(i, a)| {
                // Axis values are user coordinates; the engine stores normalized locations.
                let cur =
                    a.user_to_normalized(self.axis_values.get(i).copied().unwrap_or(a.default));
                let mst = m.get(&a.name).copied().unwrap_or(0.0);
                (cur - mst).abs() < 1e-6
            }),
            None => true,
        }
    }

    /// The interpolated instance outline at the current axis location, shown as
    /// a read-only overlay. `None` on a master (the editable outline is enough)
    /// or when the glyph is not interpolatable.
    pub(crate) fn interp_preview(&self) -> Option<Arc<BezPath>> {
        if self.on_active_master() {
            return None;
        }
        self.font
            .interpolate_outline(&self.session.glyph_name, &self.axis_location())
            .map(Arc::new)
    }

    /// Status shown under the axis sliders when the location is between masters.
    pub(crate) fn interpolation_status(&self) -> Option<String> {
        if !self.on_active_master()
            && let Some(detail) = self.font.project.compat_detail(&self.session.glyph_name)
        {
            return Some(format!("Cannot interpolate: {detail}"));
        }
        if !self.font.project.axes.is_empty() {
            match self.font.preview_font() {
                Err(error) => return Some(format!("Variable preview unavailable: {error}")),
                Ok(None) => return Some("Compiling variable preview…".into()),
                Ok(Some(_)) => (),
            }
        }
        (!self.on_active_master()).then(|| "interpolated".into())
    }

    pub(crate) fn set_axis(&mut self, index: usize, value: f64) {
        if let Some(v) = self.axis_values.get_mut(index) {
            *v = value;
        }
        self.font.project.location = self
            .font
            .axes
            .iter()
            .zip(&self.axis_values)
            .map(|(axis, value)| (axis.name.clone(), axis.user_to_normalized(*value)))
            .collect();
    }

    pub(crate) fn refresh_open_glyph(&mut self) {
        if matches!(self.mode, Mode::Editor(_)) {
            let mut session = (*self.session).clone();
            if self.sync_session_from(&mut session) == SessionSyncOutcome::Changed {
                self.finish_open_glyph_refresh();
            }
        }
    }

    pub(crate) fn finish_open_glyph_refresh(&mut self) {
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.modified = true;
        self.note.clear();
    }

    pub(crate) fn back_to_overview(&mut self) {
        self.end_space_pan();
        self.mode = Mode::Overview;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projected_glyph(session: &Session) -> norad::Glyph {
        session
            .compatibility_glyph()
            .expect("an editor session has a canonical layer")
    }

    /// A glyph with two square contours, so copy has something to choose
    /// between.
    fn two_squares() -> Session {
        let mut font = norad::Font::new();
        let mut glyph = norad::Glyph::new("test");
        for offset in [0.0, 200.0] {
            let mut contour = norad::Contour::default();
            for (x, y) in [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)] {
                contour.points.push(norad::ContourPoint::new(
                    x + offset,
                    y,
                    norad::PointType::Line,
                    false,
                    None,
                    None,
                ));
            }
            glyph.contours.push(contour);
        }
        font.default_layer_mut().insert_glyph(glyph);
        Session::new(&font, "test").expect("glyph is there")
    }

    #[test]
    fn copy_with_no_selection_takes_every_contour() {
        let session = two_squares();
        assert_eq!(session.contours_for_copy().len(), 2);
    }

    #[test]
    fn copy_takes_the_contours_holding_a_selected_point() {
        let mut session = two_squares();
        session
            .selection
            .insert(session.point_id_at(1, 0).expect("second contour point"));
        let copied = session.contours_for_copy();
        assert_eq!(copied.len(), 1);
        // The second square starts at x = 200.
        assert_eq!(copied[0].points[0].x, 200.0);
    }

    #[test]
    fn paste_appends_and_selects_what_it_pasted() {
        let mut session = two_squares();
        let copied = session.contours_for_copy();
        assert!(session.paste_contours(&copied));
        assert_eq!(projected_glyph(&session).contours.len(), 4);
        // Every point of the two new contours, and nothing else.
        assert_eq!(session.selection.len(), 8);
        assert!(
            session
                .legacy_selection()
                .iter()
                .all(|(contour, _)| *contour >= 2)
        );
    }

    #[test]
    fn pasting_nothing_changes_nothing() {
        let mut session = two_squares();
        assert!(!session.paste_contours(&[]));
        assert_eq!(projected_glyph(&session).contours.len(), 2);
    }

    #[test]
    fn parameterized_filters_record_only_real_edits() {
        let mut unchanged = two_squares();
        assert!(!unchanged.offset(0.0));
        assert!(unchanged.pending_canonical.is_none());

        let mut offset = two_squares();
        assert!(offset.offset(10.0));
        assert!(offset.pending_canonical.is_some());

        let mut extrude = two_squares();
        assert!(extrude.extrude(20.0, 30.0, false));
        assert!(extrude.pending_canonical.is_some());

        let mut rough = two_squares();
        assert!(rough.roughen(10.0, 4.0, 4.0, 7));
        assert!(rough.pending_canonical.is_some());
    }

    #[test]
    fn stroke_expansion_targets_selected_contours_and_records_the_original() {
        let mut session = two_squares();
        let original = projected_glyph(&session).contours;
        for width in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(!session.expand_stroke(width));
        }
        assert!(session.pending_canonical.is_none());
        session
            .selection
            .insert(session.point_id_at(0, 0).expect("first contour point"));
        assert!(session.expand_stroke(20.0));
        let expanded = projected_glyph(&session).contours;
        assert_eq!(expanded.last(), original.last());
        assert_ne!(expanded, original);
        assert!(session.selection.is_empty());
        assert!(session.pending_canonical.is_some());
        let mut all = two_squares();
        assert!(all.expand_stroke(20.0));
        assert!(projected_glyph(&all).contours.len() > original.len());
    }

    #[test]
    fn clockwise_and_counterclockwise_rotations_are_opposites_on_the_selection() {
        let mut clockwise = two_squares();
        let original = projected_glyph(&clockwise).contours;
        clockwise.select_contour(0);
        let mut counterclockwise = clockwise.clone();
        assert!(clockwise.rotate_90_clockwise());
        assert!(counterclockwise.rotate_90());
        let clockwise_glyph = projected_glyph(&clockwise);
        let counterclockwise_glyph = projected_glyph(&counterclockwise);
        let cw = &clockwise_glyph.contours[0].points[0];
        let ccw = &counterclockwise_glyph.contours[0].points[0];
        assert_eq!((cw.x, cw.y), (0.0, 100.0));
        assert_eq!((ccw.x, ccw.y), (100.0, 0.0));
        assert_eq!(clockwise_glyph.contours[1], original[1]);
        assert_eq!(counterclockwise_glyph.contours[1], original[1]);
        assert!(clockwise.pending_canonical.is_some());
    }

    #[test]
    fn cleanup_command_stages_only_the_canonical_edit() {
        let mut glyph = projected_glyph(&two_squares());
        glyph.contours[0].points[0].x = 0.4;
        let mut font = norad::Font::new();
        font.default_layer_mut().insert_glyph(glyph);
        let mut session = Session::new(&font, "test").expect("glyph is there");

        assert!(session.round_coordinates());
        assert_eq!(projected_glyph(&session).contours[0].points[0].x, 0.0);
        assert!(session.pending_canonical.is_some());
        assert_eq!(session.pending_canonical_label, Some("round coordinates"));
    }

    #[test]
    fn anchor_and_metric_drags_record_one_closed_transaction() {
        let mut glyph = projected_glyph(&two_squares());
        glyph.width = 500.0;
        glyph.anchors.push(norad::Anchor::new(
            100.0,
            200.0,
            Some(norad::Name::new("top").expect("anchor name")),
            None,
            None,
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut().insert_glyph(glyph);
        let mut session = Session::new(&font, "test").expect("glyph is there");
        let anchor = session.anchor_id_at(0).expect("canonical anchor identity");

        session.selected_anchor = Some(anchor);
        session.move_anchor(anchor, 120.0, 220.0);
        session.cancel_anchor_drag();
        assert_eq!(session.anchor_points()[0].1, Point::new(100.0, 200.0));
        assert!(session.pending_canonical.is_none());

        session.move_anchor(anchor, 120.0, 220.0);
        session.move_anchor(anchor, 140.0, 240.0);
        assert!(session.gesture_in_progress());
        let draft = session
            .active_anchor_drag
            .as_ref()
            .expect("owned anchor transaction")
            .transaction
            .draft();
        assert_eq!(
            draft.view().anchors().next().unwrap().position(),
            Point::new(140.0, 240.0)
        );
        session.end_anchor_drag();
        assert!(!session.gesture_in_progress());
        assert!(session.pending_canonical.is_some());

        session.pending_canonical = None;
        session.drag_advance(520.0);
        session.drag_advance(540.0);
        assert!(session.pending_canonical.is_none());
        session.end_metric_drag();
        assert!(!session.gesture_in_progress());
        assert!(session.pending_canonical.is_some());
        assert_eq!(session.advance(), 540.0);

        session.pending_canonical = None;
        session.move_anchor(anchor, f64::NAN, 10.0);
        session.drag_advance(f64::INFINITY);
        assert!(session.pending_canonical.is_none());
    }

    #[test]
    fn decompose_undo_rebuilds_nested_transformed_component_preview() {
        use masonry::kurbo::Shape as _;
        use runebender::document::project::Master;

        let mut font = norad::Font::new();
        let mut base = norad::Glyph::new("base");
        let mut contour = norad::Contour::default();
        for (x, y) in [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)] {
            contour.points.push(norad::ContourPoint::new(
                x,
                y,
                norad::PointType::Line,
                false,
                None,
                None,
            ));
        }
        base.contours.push(contour);
        font.default_layer_mut().insert_glyph(base);
        let mut nested = norad::Glyph::new("nested");
        nested.components.push(norad::Component::new(
            norad::Name::new("base").expect("base name"),
            norad::AffineTransform {
                x_scale: 2.0,
                y_scale: 2.0,
                x_offset: 40.0,
                y_offset: 60.0,
                ..norad::AffineTransform::default()
            },
            None,
        ));
        font.default_layer_mut().insert_glyph(nested);
        let mut composite = norad::Glyph::new("composite");
        composite.components.push(norad::Component::new(
            norad::Name::new("nested").expect("nested name"),
            norad::AffineTransform {
                x_offset: 20.0,
                y_offset: 30.0,
                ..norad::AffineTransform::default()
            },
            None,
        ));
        font.default_layer_mut().insert_glyph(composite);
        let mut project = runebender::document::project::Project::from_source(Master::from_font(
            font,
            std::path::PathBuf::new(),
        ));
        let source = project.document_sources().next().unwrap();
        let address = runebender::document::variable::GlyphLayerAddress {
            glyph: "composite".into(),
            layer: source.default_layer(),
        };
        let mut session = Session::new_from_project(
            &project,
            "composite",
            Metrics::of(&project.active_font().font),
        )
        .expect("composite exists");
        let before_bounds = session.components.bounding_box();

        assert!(session.decompose());
        project
            .commit_document_layer_transaction(
                session.pending_canonical.take().expect("canonical edit"),
            )
            .unwrap();
        session.reload_from_project(&project, &address);
        assert!(session.components.elements().is_empty());
        project
            .replay_document_layer_history(
                &address,
                runebender::document::history::HistoryDirection::Undo,
            )
            .unwrap();
        session.reload_from_project(&project, &address);
        assert_eq!(projected_glyph(&session).components.len(), 1);
        assert_eq!(session.components.bounding_box(), before_bounds);
    }
}

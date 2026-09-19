// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! What the menus and shortcuts call. One method is the whole of one user-facing command.

use crate::application::editor::session;
use crate::application::editor::session::{Session, SessionSyncOutcome};
use crate::application::platform::dialogs;
use crate::application::view::canvas;
use crate::application::view::canvas::grid::cells_of;
use crate::application::view::theme::Palette;
use crate::application::widgets::shortcuts;
use crate::application::workspace::{MetadataEdit, Mode, Sel, Sort, Tool, Workspace};
use std::sync::Arc;

const SAMPLE_STRINGS: &[&str] = &[
    "HHOHOHOO",
    "nnonoonoo",
    "hamburgefonstiv",
    "HAMBURGEFONSTIV",
    "0123456789",
    "AVATAR Wave Toy Vy",
    "((\"quoted\")) [j] {f}!?",
];

impl Workspace {
    /// Add the Shapes panel's named base glyph as an undoable component.
    pub(crate) fn command_add_component(&mut self) {
        let base = self.component_base_buf.trim().to_string();
        let glyph = self.session.glyph_name.clone();
        let Some(glyph_index) = self.font.index_of(&glyph) else {
            return;
        };
        let Some(address) = self.font.active_layer_address(&glyph) else {
            self.note = "The active glyph layer is unavailable".into();
            return;
        };
        if base.is_empty()
            || base == glyph
            || self
                .font
                .project
                .document_layer(&base, &address.layer)
                .is_none()
        {
            self.note = format!("Cannot add component {base}");
            return;
        }
        let undo_depth = self.font.master().undo_depth(glyph_index);
        let Ok(mut transaction) = self.font.project.begin_document_layer_transaction(&address)
        else {
            self.note = "The active glyph layer changed before adding the component".into();
            return;
        };
        let Ok(component_id) = transaction
            .draft_mut()
            .add_component(base.clone(), kurbo::Affine::IDENTITY)
        else {
            self.note = format!("Cannot add component {base}");
            return;
        };
        let Ok(runebender::document::project::DocumentEditOutcome::Changed { .. }) = self
            .font
            .project
            .commit_document_layer_transaction(transaction)
        else {
            self.note = "The active glyph layer changed before adding the component".into();
            return;
        };
        self.metadata_undo.push(MetadataEdit::DocumentLayer {
            glyph,
            address: address.clone(),
            label: "component add".into(),
            undo_depth,
        });
        self.metadata_redo.clear();
        if self.reload_canonical_layer(&address) {
            let mut session = (*self.session).clone();
            let _ = session.select_component_id(component_id);
            self.session = Arc::new(session);
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.session = self.session.clone();
            }
        }
        self.note = format!("Added component {base}");
    }

    /// Toggle whether the selected component follows its matching anchors.
    pub(crate) fn command_toggle_component_alignment(&mut self) {
        let Some(component_id) = self.session.selected_component else {
            return;
        };
        let glyph = self.session.glyph_name.clone();
        let Some(glyph_index) = self.font.index_of(&glyph) else {
            return;
        };
        let Some(address) = self.font.active_layer_address(&glyph) else {
            self.note = "The active glyph layer is unavailable".into();
            return;
        };
        let undo_depth = self.font.master().undo_depth(glyph_index);
        let Ok(mut transaction) = self.font.project.begin_document_layer_transaction(&address)
        else {
            self.note = "The active glyph layer changed before component alignment".into();
            return;
        };
        if !transaction
            .draft()
            .view()
            .components()
            .any(|component| component.id() == component_id)
        {
            return;
        }
        let Ok(disabled) = transaction
            .draft()
            .component_alignment_disabled(component_id)
        else {
            return;
        };
        if transaction
            .draft_mut()
            .set_component_alignment_disabled(component_id, !disabled)
            != Ok(true)
        {
            return;
        }
        if disabled {
            let layer = address.layer.clone();
            let project = &self.font.project;
            if let Err(error) = runebender::document::composites::realign_document_layer(
                transaction.draft_mut(),
                |name| project.document_layer(name, &layer),
                true,
            ) {
                self.note = format!("Cannot align component: {error}");
                return;
            }
        }
        let Ok(runebender::document::project::DocumentEditOutcome::Changed { .. }) = self
            .font
            .project
            .commit_document_layer_transaction(transaction)
        else {
            self.note = "The active glyph layer changed before component alignment".into();
            return;
        };
        self.metadata_undo.push(MetadataEdit::DocumentLayer {
            glyph,
            address: address.clone(),
            label: "component alignment".into(),
            undo_depth,
        });
        self.metadata_redo.clear();
        let _ = self.reload_canonical_layer(&address);
        let aligned = disabled;
        self.note = if aligned {
            "Component locked to anchors"
        } else {
            "Component unlocked for movement"
        }
        .into();
    }

    /// Add an empty glyph with the first free `glyph(.NNN)` name and open it.
    pub(crate) fn command_new_glyph(&mut self) {
        let mut name = "glyph".to_string();
        let mut counter = 0;
        while self.font.index_of(&name).is_some() {
            counter += 1;
            name = format!("glyph.{counter:03}");
        }
        let upm = self.font.units_per_em();
        if self.font.add_glyph(&name, (upm * 0.5).round(), None) {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.modified = true;
            self.note = format!("added {name}");
            if let Some(index) = self.font.index_of(&name) {
                self.open_glyph(index);
            }
        }
    }

    /// Duplicate the selected glyph in every master and open the new copy.
    pub(crate) fn command_duplicate_glyph(&mut self) {
        let Some(index) = self.selected else {
            self.note = "select a glyph to duplicate".into();
            return;
        };
        let Some(source) = self.font.glyphs.get(index).map(|glyph| glyph.name.clone()) else {
            return;
        };
        let Some(name) = self.font.duplicate_glyph(&source) else {
            return;
        };
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.modified = true;
        self.note = format!("duplicated {source} as {name}");
        if let Some(index) = self.font.index_of(&name) {
            self.open_glyph(index);
        }
    }

    /// Remove the selected glyph from every master and discard its open tabs.
    pub(crate) fn command_remove_glyph(&mut self) {
        let Some(index) = self.selected else {
            self.note = "select a glyph to remove".into();
            return;
        };
        let Some(name) = self.font.glyphs.get(index).map(|glyph| glyph.name.clone()) else {
            return;
        };
        if !self.font.remove_glyph(&name) {
            return;
        }

        let removed_active =
            matches!(self.mode, Mode::Editor(_)) && self.session.glyph_name == name;
        self.tabs.retain(|tab| tab.session.glyph_name != name);
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
        self.multi_selected = Arc::new(std::collections::HashSet::new());
        self.selected = None;
        self.modified = true;
        self.note = format!("removed {name}");

        if removed_active {
            if self.tabs.is_empty() {
                self.mode = Mode::Overview;
                self.active_tab = 0;
            } else {
                let next = self.active_tab.min(self.tabs.len() - 1);
                self.active_tab = usize::MAX;
                self.activate_tab(next);
            }
        }
    }

    /// Rebuild the selected glyph in this master from the other masters.
    pub(crate) fn command_reinterpolate(&mut self) {
        let Some(index) = self.selected else {
            return;
        };
        let Some(name) = self.font.glyphs.get(index).map(|glyph| glyph.name.clone()) else {
            return;
        };
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            self.note = "The active source is unavailable".into();
            return;
        };
        let Some(address) = self.font.active_layer_address(&name) else {
            self.note = "The active glyph layer is unavailable".into();
            return;
        };
        let undo_depth = self.font.master().undo_depth(index);
        let outcome = match self
            .font
            .project
            .reinterpolate_document_layer(&name, source)
        {
            Ok(outcome) => outcome,
            Err(error) => {
                self.note = error;
                return;
            }
        };
        if matches!(
            outcome,
            runebender::document::project::DocumentEditOutcome::Changed { .. }
        ) {
            self.metadata_undo.push(MetadataEdit::DocumentLayer {
                glyph: name.clone(),
                address: address.clone(),
                label: "reinterpolate".into(),
                undo_depth,
            });
            self.metadata_redo.clear();
            self.font.rebuild_cache();
            if matches!(self.mode, Mode::Editor(_)) {
                let _ = self.reload_canonical_layer(&address);
                Arc::make_mut(&mut self.session).selection.clear();
            }
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.modified = true;
        }
        self.note = format!("{name}: reinterpolated from the other masters");
    }

    /// Apply Glyphs-style sidebearing formulas in every master.
    pub(crate) fn command_update_metrics(&mut self) {
        use runebender::document::project::Master;
        use runebender::formats::metrics_keys::{
            MetricsFormula, parse_metrics_key, read_metrics_key,
        };

        let mut adjusted = 0;
        for _ in 0..5 {
            let mut moved = false;
            for master in self.font.project.edit_sources().iter_mut() {
                let keyed: Vec<_> = (0..master.glyphs.len())
                    .filter_map(|index| {
                        let glyph = master.font.get_glyph(master.glyphs[index].name.as_ref())?;
                        let left = read_metrics_key(glyph, true);
                        let right = read_metrics_key(glyph, false);
                        (left.is_some() || right.is_some()).then_some((index, left, right))
                    })
                    .collect();
                for (index, left, right) in keyed {
                    let resolve = |master: &Master,
                                   formula: &MetricsFormula,
                                   want_left: bool|
                     -> Option<f64> {
                        match formula {
                            MetricsFormula::Constant(value) => Some(*value),
                            MetricsFormula::Reference { glyph, mirror, op } => {
                                let reference = master.name_map.get(glyph).copied()?;
                                let ink = master.ink_bounds(reference)?;
                                let advance = master.glyphs[reference].advance;
                                let mut value = if want_left != *mirror {
                                    ink.x0
                                } else {
                                    advance - ink.x1
                                };
                                if let Some((operator, amount)) = op {
                                    value = match operator {
                                        '+' => value + amount,
                                        '-' => value - amount,
                                        _ => value * amount,
                                    };
                                }
                                Some(value)
                            }
                        }
                    };
                    if let Some(formula) = left.as_deref().and_then(parse_metrics_key)
                        && let (Some(target), Some(ink)) =
                            (resolve(master, &formula, true), master.ink_bounds(index))
                    {
                        let delta = (target - ink.x0).round();
                        if delta != 0.0 {
                            master.shift_ink(index, delta);
                            moved = true;
                            adjusted += 1;
                        }
                    }
                    if let Some(formula) = right.as_deref().and_then(parse_metrics_key)
                        && let (Some(target), Some(ink)) =
                            (resolve(master, &formula, false), master.ink_bounds(index))
                    {
                        let advance = master.glyphs[index].advance;
                        let wanted = (ink.x1 + target).round();
                        if (advance - wanted).abs() >= 1.0 {
                            master.set_advance(index, wanted);
                            moved = true;
                            adjusted += 1;
                        }
                    }
                }
            }
            if !moved {
                break;
            }
        }
        if adjusted > 0 {
            self.font.rebuild_cache();
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.modified = true;
            if matches!(self.mode, Mode::Editor(_))
                && let Some(address) = self.font.active_layer_address(&self.session.glyph_name)
            {
                let mut session = (*self.session).clone();
                let _ = session.reload_from_project(&self.font.project, &address);
                self.session = Arc::new(session);
                self.refresh_metric_bufs();
            }
        }
        self.note = if adjusted == 0 {
            "Metrics keys: everything in sync".into()
        } else {
            format!("Metrics keys: {adjusted} sidebearings adjusted")
        };
    }

    /// Select positional Arabic forms whose joining bands disagree.
    pub(crate) fn command_check_joining(&mut self) {
        use runebender::analysis::measure::joining_band;

        let mut bands = Vec::new();
        let mut broken = Vec::new();
        for (index, entry) in self.font.glyphs.iter().enumerate() {
            let (joins_left, joins_right) = if entry.name.ends_with(".init") {
                (true, false)
            } else if entry.name.ends_with(".medi") {
                (true, true)
            } else if entry.name.ends_with(".fina") {
                (false, true)
            } else {
                continue;
            };
            let Some(glyph) = self.font.font().get_glyph(&entry.name) else {
                continue;
            };
            let outline =
                runebender::outline::glyph_paths::glyph_to_bezpath(glyph, self.font.font());
            for left in [true, false] {
                if (left && joins_left) || (!left && joins_right) {
                    match joining_band(&outline, entry.advance, left, 2.0) {
                        Some((lo, hi)) => bands.push((index, lo, hi)),
                        None => broken.push(index),
                    }
                }
            }
        }
        if bands.is_empty() && broken.is_empty() {
            self.note = "Joining: no positional forms to check".into();
            return;
        }
        let median = |mut values: Vec<f64>| {
            values.sort_by(f64::total_cmp);
            values[values.len() / 2]
        };
        let med_lo = median(bands.iter().map(|(_, lo, _)| *lo).collect());
        let med_hi = median(bands.iter().map(|(_, _, hi)| *hi).collect());
        let mut off: std::collections::HashSet<usize> = bands
            .into_iter()
            .filter(|(_, lo, hi)| (lo - med_lo).abs() > 4.0 || (hi - med_hi).abs() > 4.0)
            .map(|(index, _, _)| index)
            .collect();
        off.extend(broken);
        let count = off.len();
        self.multi_selected = Arc::new(off);
        self.selected = None;
        self.note = if count == 0 {
            format!("Joining: all forms share the {med_lo:.0}–{med_hi:.0} band")
        } else {
            format!("Joining: {count} form(s) off the {med_lo:.0}–{med_hi:.0} band (selected)")
        };
    }

    /// Derive selected composable glyphs, or every recipe when none are selected.
    pub(crate) fn command_compose_from_anchors(&mut self) {
        let names: Vec<String> = self
            .multi_selected
            .iter()
            .filter_map(|index| self.font.glyphs.get(*index))
            .map(|glyph| glyph.name.clone())
            .collect();
        let only = (!names.is_empty()).then_some(names);
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            self.note = "Compose: the active source is unavailable".into();
            return;
        };
        let plan = match runebender::document::compose::plan_project(
            &self.font.project,
            source,
            only.as_deref(),
        ) {
            Ok(plan) => plan,
            Err(error) => {
                self.note = format!("Compose: {error}");
                return;
            }
        };
        let report = if plan.replacements.is_empty() {
            plan.report
        } else {
            match runebender::document::proposal::write_composition_project(
                &mut self.font.project,
                source,
                plan,
            ) {
                Ok(report) => report,
                Err(error) => {
                    self.note = format!("Compose: {error}");
                    return;
                }
            }
        };
        let proposed = report.proposed().len();
        let current = report
            .derived
            .iter()
            .filter(|derived| derived.up_to_date)
            .count();
        self.modified |= report.proposal.is_some();
        self.note = format!("Compose: {proposed} proposed, {current} up to date");
        if !report.skipped.is_empty() {
            self.note
                .push_str(&format!(", {} skipped", report.skipped.len()));
        }
        self.refresh_proposals();
    }

    /// Make mask subtraction permanent in every master.
    pub(crate) fn command_bake_masks(&mut self) {
        let Some(index) = self.selected else {
            return;
        };
        let Some(name) = self.font.glyphs.get(index).map(|glyph| glyph.name.clone()) else {
            return;
        };
        let active_source = self.font.project.source_id(self.font.active());
        let undo_depth = self.font.master().undo_depth(index);
        let addresses = self
            .font
            .project
            .document_sources()
            .filter_map(|source| {
                let address = runebender::document::variable::GlyphLayerAddress {
                    glyph: name.clone(),
                    layer: source.default_layer(),
                };
                self.font
                    .project
                    .document_layer(&name, &address.layer)
                    .is_some()
                    .then_some((source.id(), address))
            })
            .collect::<Vec<_>>();
        let mut baked = 0;
        let mut active_address = None;
        for (source, address) in addresses {
            let Ok(mut transaction) = self.font.project.begin_document_layer_transaction(&address)
            else {
                continue;
            };
            if transaction.draft_mut().bake_masks() != Ok(true) {
                continue;
            }
            if matches!(
                self.font
                    .project
                    .commit_document_layer_transaction(transaction),
                Ok(runebender::document::project::DocumentEditOutcome::Changed { .. })
            ) {
                baked += 1;
                if Some(source) == active_source {
                    active_address = Some(address);
                }
            }
        }
        self.font.project.compute_compat();
        self.font.rebuild_cache();
        if baked > 0 {
            if let Some(address) = active_address.as_ref() {
                self.metadata_undo.push(MetadataEdit::DocumentLayer {
                    glyph: name.clone(),
                    address: address.clone(),
                    label: "bake masks".into(),
                    undo_depth,
                });
                self.metadata_redo.clear();
            }
            if matches!(self.mode, Mode::Editor(_))
                && self.session.glyph_name == name
                && let Some(address) = active_address.as_ref()
            {
                let mut session = (*self.session).clone();
                let _ = session.reload_from_project(&self.font.project, address);
                self.session = Arc::new(session);
                self.selected_points = 0;
            }
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.modified = true;
        }
        self.note = if baked == 0 {
            "No masks to bake".into()
        } else {
            format!("Masks baked in {baked} master(s)")
        };
    }

    /// Write the selected glyph as SVG beside the document source.
    pub(crate) fn command_export_glyph_svg(&mut self) {
        let Some(index) = self.selected else {
            return;
        };
        let Some(entry) = self.font.glyphs.get(index) else {
            return;
        };
        let Some(glyph) = self.font.font().get_glyph(&entry.name) else {
            return;
        };
        let path = runebender::outline::glyph_paths::glyph_to_bezpath(glyph, self.font.font());
        let svg = runebender::formats::svg::glyph_svg(
            &path,
            glyph.width,
            self.font.ascender(),
            self.font.descender(),
        );
        let directory = self
            .font
            .document_source()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let output = directory.join(format!("{}.svg", entry.name));
        self.note = match std::fs::write(&output, svg) {
            Ok(()) => format!("wrote {}", output.display()),
            Err(error) => format!("SVG export failed: {error}"),
        };
    }

    /// Pick a destination directory and keep saving each master there.
    pub(crate) fn command_save_as(&mut self) {
        let start = self
            .font
            .document_source()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let Some(directory) = dialogs::folder(start) else {
            return;
        };
        self.save_as_to(&directory);
    }

    /// Copy this document into a new directory and make that copy current.
    pub(crate) fn save_as_to(&mut self, directory: &std::path::Path) -> bool {
        if self.features_edited {
            self.note = "Apply or Revert feature edits before saving".into();
            return false;
        }
        if let Err(error) = self.font.project.save_as(directory) {
            self.note = format!("Save As refused: {error}");
            return false;
        }
        self.prepare_save_as();
        self.modified = false;
        let source = self.font.source();
        let label = source
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_else(|| source.as_os_str().to_string_lossy());
        self.note = format!("Saved {label}");
        true
    }

    /// Pick and open a nodes graph.
    pub(crate) fn command_open_nodes(&mut self) {
        let start = self
            .font
            .document_source()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        if let Some(path) = dialogs::nodes(start) {
            self.open_nodes_file(&path);
        }
    }

    /// Pick a raster image and replace the open glyph's contours with its trace.
    pub(crate) fn command_trace_image(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let start = self
            .font
            .document_source()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let Some(path) = dialogs::image(start) else {
            return;
        };
        let traced = std::fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                runebender::formats::image_trace::trace_image(
                    &bytes,
                    &runebender::formats::image_trace::TraceConfig {
                        target_height: (self.font.ascender() - self.font.descender()).max(1.0),
                        y_offset: self.font.descender(),
                        advance: self.session.advance().max(1.0),
                        ..Default::default()
                    },
                )
            });
        match traced {
            Ok(glyph) => {
                let count = glyph.contours.len();
                self.apply_op(move |session| session.replace_imported_contours(&glyph.contours));
                self.note = format!("Traced {count} contour(s)");
            }
            Err(error) => self.note = format!("Trace: {error}"),
        }
    }

    /// Pick an SVG and append its contours to the open glyph.
    pub(crate) fn command_import_svg(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let start = self
            .font
            .document_source()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let Some(path) = dialogs::svg(start) else {
            return;
        };
        let contours = std::fs::read_to_string(path)
            .map_err(|error| error.to_string())
            .and_then(|svg| {
                runebender::formats::svg::svg_to_contours(
                    &svg,
                    self.font.ascender(),
                    self.font.descender(),
                )
            });
        match contours {
            Ok(contours) => {
                let count = contours.len();
                self.apply_op(move |session| session.append_imported_contours(&contours));
                self.note = format!("Imported {count} SVG contour(s)");
            }
            Err(error) => self.note = format!("SVG import: {error}"),
        }
    }

    /// Pick a raster image, store it in the UFO, and attach it to the glyph.
    pub(crate) fn command_place_image(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let start = self
            .font
            .document_source()
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let Some(path) = dialogs::image(start) else {
            return;
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.note = format!("Place image: {error}");
                return;
            }
        };
        let decoded = match image::load_from_memory(&bytes) {
            Ok(image) => image,
            Err(error) => {
                self.note = format!("Place image: {error}");
                return;
            }
        };
        let (width, height) = (decoded.width(), decoded.height());
        let file_name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "image.png".into());
        let scale =
            ((self.font.ascender() - self.font.descender()) / f64::from(height).max(1.0)).max(1e-6);
        let placed = match norad::Image::new(
            std::path::PathBuf::from(&file_name),
            None,
            norad::AffineTransform {
                x_scale: scale,
                xy_scale: 0.0,
                yx_scale: 0.0,
                y_scale: scale,
                x_offset: 0.0,
                y_offset: self.font.descender(),
            },
        ) {
            Ok(image) => image,
            Err(error) => {
                self.note = format!("Place image: {error}");
                return;
            }
        };
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            self.note = "Place image: the active source is unavailable".into();
            return;
        };
        if let Err(error) = self.font.project.install_document_source_image(
            source,
            std::path::PathBuf::from(&file_name),
            bytes,
        ) {
            self.note = format!("Place image: {error}");
            return;
        }
        self.apply_op(move |session| session.set_image(Some(placed)));
        self.show_background = true;
        self.note = format!("Placed {file_name} · {width}×{height}px");
    }

    /// Pick a local model and run its structure-preserving bolden task.
    pub(crate) fn command_bolden_with_model(&mut self) {
        let Mode::Editor(index) = self.mode else {
            return;
        };
        let start = Self::models_dir().unwrap_or_else(|| {
            self.font
                .document_source()
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_path_buf()
        });
        let Some(directory) = dialogs::folder(&start) else {
            return;
        };
        self.load_model(&directory);
        if self.ai.dir.as_deref() == Some(directory.as_path()) {
            self.run_task("bolden", Some(index));
        }
    }

    /// Unlink the open glyph's background image, preserving the stored file.
    pub(crate) fn command_remove_image(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) || !self.session.has_image() {
            return;
        }
        self.apply_op(|session| session.set_image(None));
        self.note = "Removed image".into();
    }

    pub(crate) fn new_glyph(&mut self) {
        let name = self.filter.trim().to_string();
        let upm = self.font.units_per_em();
        if self.font.add_glyph(&name, (upm * 0.5).round(), None) {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.filter.clear();
            if let Some(i) = self.font.index_of(&name) {
                self.open_glyph(i);
            }
            self.modified = true;
        }
    }

    /// Add every glyph a coverage filter is missing, to every master.
    ///
    /// The GF sets carry a name and a codepoint per glyph, so what lands
    /// is named and encoded, which is what makes the row's count move.
    pub(crate) fn generate_missing(&mut self, index: usize) {
        let filters = runebender::ui::sidebar::builtin_filters();
        let Some(set) = filters.get(index).and_then(|f| f.glyphset.as_ref()) else {
            return;
        };
        let mut wanted: Vec<(String, Option<u32>)> = set
            .targets
            .iter()
            .map(|target| (target.name.clone(), Some(target.unicode)))
            .collect();
        for name in &set.glyph_names {
            if !wanted.iter().any(|(existing, _)| existing == name) {
                wanted.push((name.clone(), None));
            }
        }
        let added = self.font.add_missing(&wanted);
        if added > 0 {
            self.cells = Arc::new(cells_of(&self.font, &self.palette));
            self.modified = true;
        }
        self.note = match added {
            0 => "nothing missing".into(),
            1 => "added 1 glyph".into(),
            n => format!("added {n} glyphs"),
        };
    }

    /// Advance to the next theme, reloading the palette and the baked cell
    /// colors. Exercises the design-token kernel: one id swaps every role.
    pub(crate) fn cycle_theme(&mut self) {
        let i = Self::THEMES
            .iter()
            .position(|t| *t == self.theme_id)
            .unwrap_or(0);
        self.theme_id = Self::THEMES[(i + 1) % Self::THEMES.len()];
        self.palette = Arc::new(Palette::load(self.theme_id));
        self.cells = Arc::new(cells_of(&self.font, &self.palette));
    }

    pub(crate) fn command_expand_stroke(&mut self) {
        if self.interp_preview().is_some() {
            return;
        }
        if let Ok(width) = self.stroke_buf.trim().parse::<f64>() {
            self.apply_op(|session| session.expand_stroke(width));
        }
    }

    pub(crate) fn command_filter_offset(&mut self) {
        if let Ok(delta) = self.offset_buf.trim().parse::<f64>() {
            self.apply_op(|session| session.offset(delta));
        }
    }

    pub(crate) fn command_filter_extrude(&mut self) {
        let text = self.extrude_buf.trim();
        let keep_front = text.starts_with(['k', 'K']);
        let mut parts = text
            .trim_start_matches(['k', 'K'])
            .trim()
            .split(',')
            .map(str::trim);
        let Some(offset) = parts.next().and_then(|value| value.parse::<f64>().ok()) else {
            return;
        };
        let angle = parts
            .next()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(30.0);
        self.apply_op(|session| session.extrude(offset, angle, keep_front));
    }

    pub(crate) fn command_filter_roughen(&mut self) {
        let mut parts = self.roughen_buf.trim().split(',').map(str::trim);
        let Some(segment) = parts.next().and_then(|value| value.parse::<f64>().ok()) else {
            return;
        };
        let horizontal = parts
            .next()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(segment);
        let vertical = parts
            .next()
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(segment / 2.0);
        self.roughen_seed = self.roughen_seed.wrapping_add(1);
        let seed = self.roughen_seed;
        self.apply_op(|session| session.roughen(segment, horizontal, vertical, seed));
    }

    pub(crate) fn command_filter_slant(&mut self) {
        if let Ok(degrees) = self.slant_buf.trim().parse::<f64>()
            && degrees != 0.0
            && degrees.abs() < 89.0
        {
            self.apply_op(|session| {
                session.transform(kurbo::Affine::skew(degrees.to_radians().tan(), 0.0))
            });
        }
    }

    pub(crate) fn dispatch(&mut self, action: shortcuts::AppAction) {
        #[cfg(target_arch = "wasm32")]
        if crate::application::browser::desktop_action(action) {
            self.note =
                "This demo keeps edits in this tab. Use the desktop app to open or save fonts."
                    .into();
            return;
        }
        use shortcuts::AppAction as A;
        match action {
            A::Quit => unreachable!("Quit is handled at the application shell"),
            A::Save => {
                // On the nodes canvas, Save writes the graph file too.
                if matches!(self.mode, Mode::Nodes) {
                    self.save_nodes_file();
                }
                self.save();
            }
            A::OpenFont => unreachable!("Open belongs to AppState"),
            A::SaveAs => self.command_save_as(),
            A::RevertToSaved => {
                if dialogs::confirm_revert() {
                    self.revert_to_saved();
                }
            }
            A::ExportFont => self.command_export(),
            A::Undo => self.undo_active_edit(false),
            A::Redo => self.undo_active_edit(true),
            A::Overview => {
                if matches!(self.mode, Mode::Editor(_) | Mode::Nodes) {
                    self.back_to_overview();
                }
            }
            A::BeginSpacePan => self.begin_space_pan(),
            A::EndSpacePan => self.end_space_pan(),
            A::Tool(t) => {
                self.select_tool(t);
                // Picking Measure turns on what the tool is for, keeping
                // whatever curve analyses were already showing.
                if t == Tool::Measure && !self.view.measures() {
                    let measuring = canvas::editor::ViewOptions::measuring();
                    self.view = canvas::editor::ViewOptions {
                        comb: self.view.comb,
                        continuity: self.view.continuity,
                        ..measuring
                    };
                }
            }
            A::FlipHorizontal => self.apply_op(|s| s.flip_horizontal()),
            A::FlipVertical => self.apply_op(|s| s.flip_vertical()),
            A::Rotate90 => self.apply_op(|s| s.rotate_90()),
            A::RotateRight => {
                self.apply_op(|s| s.transform(kurbo::Affine::new([0.0, -1.0, 1.0, 0.0, 0.0, 0.0])));
            }
            A::Rotate180 => self.apply_op(|s| s.transform(kurbo::Affine::scale(-1.0))),
            A::RemoveOverlap => self.apply_op(|s| s.remove_overlap()),
            A::BooleanUnion => self.apply_op(|s| s.boolean(session::BoolOp::Union)),
            A::BooleanSubtract => self.apply_op(|s| s.boolean(session::BoolOp::Subtract)),
            A::BooleanIntersect => self.apply_op(|s| s.boolean(session::BoolOp::Intersect)),
            A::BooleanExclude => self.apply_op(|s| s.boolean(session::BoolOp::Exclude)),
            A::Decompose => self.apply_op(|s| s.decompose()),
            A::Duplicate => self.apply_op(|s| s.duplicate()),
            A::DuplicateRepeat => self.apply_op(|s| s.duplicate_repeat()),
            A::ReverseContours => self.apply_op(|s| s.reverse()),
            A::SetStartPoint => self.apply_op(|s| s.set_start()),
            A::TidyPaths => self.apply_op(|s| s.tidy_paths()),
            A::AddExtremes => self.apply_op(|s| s.add_extremes()),
            A::RoundCoordinates => self.apply_op(|s| s.round_coordinates()),
            A::CorrectPathDirection => self.apply_op(|s| s.correct_path_direction()),
            A::MetaballsToCubic => self.edit_metaballs(|s| s.collapse_metaballs(false)),
            A::MetaballGroupsToCubic => self.edit_metaballs(|s| s.collapse_metaballs(true)),
            A::FontMetaballsToCubic => self.collapse_font_metaballs(),
            A::HyperToCubic => self.apply_op(|s| s.hyper_to_cubic()),
            A::QuadsToCubics => self.apply_op(|s| s.quads_to_cubics()),
            A::CubicsToQuads => self.apply_op(|s| s.cubics_to_quads()),
            A::RoundCorners => self.apply_op(|s| s.round_corners()),
            A::Harmonize => self.apply_op(|s| s.harmonize()),
            A::Balance => self.apply_op(|s| s.balance()),
            A::Optimize => self.apply_op(|s| s.optimize()),
            A::FilterOffset => self.command_filter_offset(),
            A::FilterExtrude => self.command_filter_extrude(),
            A::FilterRoughen => self.command_filter_roughen(),
            A::FilterSlant => self.command_filter_slant(),
            A::NewFont => self.new_font(),
            A::CycleTheme => self.cycle_theme(),
            A::Theme(id) => {
                self.theme_id = id;
                self.palette = Arc::new(Palette::load(id));
                self.cells = Arc::new(cells_of(&self.font, &self.palette));
            }
            A::ZoomToFit => {
                let mut session = (*self.session).clone();
                session.fitted = false;
                self.session = Arc::new(session);
            }
            A::ShowAllMasters => {
                self.show_all_masters = !self.show_all_masters;
                self.reference_layers.clear();
                if self.show_all_masters {
                    self.reference_layers.extend(
                        (0..self.font.master_count()).filter(|index| *index != self.font.active()),
                    );
                }
                self.note = if self.show_all_masters {
                    "showing all masters".into()
                } else {
                    "showing selected reference masters".into()
                };
            }
            A::NextMaster | A::PreviousMaster => {
                let count = self.font.master_count();
                if count > 1 {
                    let current = self.font.active();
                    let next = if matches!(action, A::NextMaster) {
                        (current + 1) % count
                    } else {
                        (current + count - 1) % count
                    };
                    self.set_master(next);
                }
            }
            A::NextSampleString | A::PreviousSampleString => {
                let count = SAMPLE_STRINGS.len();
                self.sample_index = if matches!(action, A::NextSampleString) {
                    (self.sample_index + 1) % count
                } else {
                    (self.sample_index + count - 1) % count
                };
                self.preview_text = SAMPLE_STRINGS[self.sample_index].into();
                self.note = format!("Sample: {}", self.preview_text);
            }
            A::MeasureColorize => self.view.colorize = !self.view.colorize,
            A::MeasureHandles => self.view.handles = !self.view.handles,
            A::MeasureSegments => self.view.segments = !self.view.segments,
            A::MeasureSizes => self.view.sizes = !self.view.sizes,
            A::MeasureSpans => self.view.spans = !self.view.spans,
            A::GridDots => self.view.grid_lines = false,
            A::GridLines => self.view.grid_lines = true,
            A::MeasureSideBearings => self.view.bearings = !self.view.bearings,
            A::MeasurePopcount => self.view.popcount = !self.view.popcount,
            A::MeasureAllOn => {
                self.view.colorize = true;
                self.view.handles = true;
                self.view.segments = true;
                self.view.sizes = true;
                self.view.spans = true;
                self.view.bearings = true;
            }
            A::MeasureAllOff => {
                self.view.colorize = false;
                self.view.handles = false;
                self.view.segments = false;
                self.view.sizes = false;
                self.view.spans = false;
                self.view.bearings = false;
            }
            A::GenerateMissing => match self.sel {
                Sel::Filter(i) => {
                    let missing = self.filter_missing(i);
                    if missing == 0 {
                        self.note = "nothing missing in this filter".into();
                    } else {
                        self.generate_missing(i);
                    }
                }
                _ => self.note = "select a coverage filter in the sidebar first".into(),
            },
            A::NewGlyph => self.command_new_glyph(),
            A::DuplicateGlyph => self.command_duplicate_glyph(),
            A::RemoveGlyph => self.command_remove_glyph(),
            A::UpdateMetrics => self.command_update_metrics(),
            A::Reinterpolate => self.command_reinterpolate(),
            A::CheckJoining => self.command_check_joining(),
            A::ComposeFromAnchors => self.command_compose_from_anchors(),
            A::BakeMasks => self.command_bake_masks(),
            A::ExportGlyphSvg => self.command_export_glyph_svg(),
            A::TraceImage => self.command_trace_image(),
            A::BoldenWithModel => self.command_bolden_with_model(),
            A::PlaceImage => self.command_place_image(),
            A::ImportSvg => self.command_import_svg(),
            A::RemoveImage => self.command_remove_image(),
            A::SortByName => self.sort = Sort::Name,
            A::SortByUnicode => self.sort = Sort::Unicode,
            A::NodesTab => self.enter_nodes_mode(),
            A::NodesNew => self.new_nodes_file(),
            A::NodesOpen => self.command_open_nodes(),
            A::NodesSave => self.save_nodes_file(),
            A::NodesRun => {
                if self.nodes.graph.is_none() {
                    self.enter_nodes_mode();
                }
                self.run_nodes();
            }
            A::Copy => self.copy_contours(),
            A::Paste => self.paste_contours(),
            A::CopySelectedGlyphs => self.copy_selected_glyphs_as_text(),
            A::SelectAll => {
                if matches!(self.mode, Mode::Editor(_)) {
                    let mut session = (*self.session).clone();
                    if self.tool == Tool::Metaball {
                        session.select_all_metaballs();
                    } else {
                        session.select_all();
                    }
                    self.selected_points = session.selection.len();
                    self.session = Arc::new(session);
                }
            }
            A::DeselectAll => {
                if matches!(self.mode, Mode::Editor(_)) {
                    let mut session = (*self.session).clone();
                    session.selection.clear();
                    self.selected_points = 0;
                    self.session = Arc::new(session);
                }
            }
            A::InvertSelection => {
                if matches!(self.mode, Mode::Editor(_)) {
                    let mut session = (*self.session).clone();
                    let all: std::collections::HashSet<_> =
                        session.points().into_iter().map(|point| point.id).collect();
                    session.selection = all.difference(&session.selection).copied().collect();
                    self.selected_points = session.selection.len();
                    self.session = Arc::new(session);
                }
            }
        }
    }

    /// Copy the selected contours, or all of them when nothing is
    /// selected.
    pub(crate) fn copy_contours(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let Some(address) = self.font.active_layer_address(&self.session.glyph_name) else {
            self.note = "The active glyph layer is unavailable".into();
            return;
        };
        let Some(layer) = self
            .font
            .project
            .document_layer(&address.glyph, &address.layer)
        else {
            self.note = "The active glyph layer is unavailable".into();
            return;
        };
        let selected = self.session.canonical_selection(layer);
        let Ok(copied) = layer.copy_contours(&selected) else {
            self.note = "The point selection changed before copying".into();
            return;
        };
        self.clipboard = copied;
        self.note = match self.clipboard.len() {
            0 => "nothing to copy".into(),
            1 => "copied 1 contour".into(),
            n => format!("copied {n} contours"),
        };
    }

    /// Paste the copied contours into the open glyph, with undo.
    pub(crate) fn paste_contours(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) || self.clipboard.is_empty() {
            return;
        }
        let glyph = self.session.glyph_name.clone();
        let Some(glyph_index) = self.font.index_of(&glyph) else {
            return;
        };
        let Some(address) = self.font.active_layer_address(&glyph) else {
            self.note = "The active glyph layer is unavailable".into();
            return;
        };
        let undo_depth = self.font.master().undo_depth(glyph_index);
        let Ok(mut transaction) = self.font.project.begin_document_layer_transaction(&address)
        else {
            self.note = "The active glyph layer changed before pasting".into();
            return;
        };
        let Ok(pasted) = transaction.draft_mut().paste_contours(&self.clipboard) else {
            self.note = "The copied contours are no longer valid".into();
            return;
        };
        let Ok(runebender::document::project::DocumentEditOutcome::Changed { .. }) = self
            .font
            .project
            .commit_document_layer_transaction(transaction)
        else {
            self.note = "The active glyph layer changed before pasting".into();
            return;
        };
        self.metadata_undo.push(MetadataEdit::DocumentLayer {
            glyph,
            address: address.clone(),
            label: "paste contours".into(),
            undo_depth,
        });
        self.metadata_redo.clear();
        if self.reload_canonical_layer(&address)
            && let Some(layer) = self
                .font
                .project
                .document_layer(&address.glyph, &address.layer)
        {
            let mut session = (*self.session).clone();
            session.select_canonical_points(layer, &pasted.points);
            self.selected_points = session.selection.len();
            self.session = Arc::new(session);
            if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                tab.session = self.session.clone();
            }
        }
        self.note = format!("pasted {} contours", self.clipboard.len());
    }

    /// The encoded characters of the current glyph-grid selection.
    fn selected_glyph_text(&self) -> String {
        let mut indices: Vec<usize> = self.multi_selected.iter().copied().collect();
        if let Some(index) = self.selected
            && !indices.contains(&index)
        {
            indices.push(index);
        }
        let mut glyphs: Vec<_> = indices
            .into_iter()
            .filter_map(|index| self.font.glyphs.get(index))
            .collect();
        glyphs.sort_by(|a, b| a.name.cmp(&b.name));
        glyphs
            .into_iter()
            .filter_map(|glyph| glyph.codepoint)
            .collect()
    }

    /// Copy selected glyphs' encoded characters to the system clipboard.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn copy_selected_glyphs_as_text(&mut self) {
        use copypasta::ClipboardProvider as _;

        let text = self.selected_glyph_text();
        if text.is_empty() {
            self.note = "Nothing encoded to copy".into();
            return;
        }
        self.note = match copypasta::ClipboardContext::new()
            .and_then(|mut clipboard| clipboard.set_contents(text.clone()))
        {
            Ok(()) => format!("Copied {} character(s)", text.chars().count()),
            Err(error) => format!("Clipboard: {error}"),
        };
    }

    /// Copy the open glyph's outline into the canonical background layer.
    pub(crate) fn send_to_background(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let name = self.session.glyph_name.clone();
        let Some(index) = self.font.index_of(&name) else {
            return;
        };
        let Some(address) = self.font.active_layer_address(&name) else {
            return;
        };
        let undo_depth = self.font.master().undo_depth(index);
        let before = self.font.project.document_snapshot();
        match self
            .font
            .project
            .copy_document_layer_to_background(&address)
        {
            Ok(true) => {}
            Ok(false) => {
                self.show_background = true;
                self.note = "background is already current".into();
                return;
            }
            Err(error) => {
                self.note = format!("Background: {error}");
                return;
            }
        }
        self.metadata_undo.push(MetadataEdit::SourceStructure {
            glyph: name,
            label: "send to background".into(),
            before: Box::new(before),
            after: Box::new(self.font.project.document_snapshot()),
            undo_depth,
        });
        self.metadata_redo.clear();
        self.show_background = true;
        self.modified = true;
        self.note = "sent to background".into();
    }

    /// Exchange the outline with the background layer's copy.
    pub(crate) fn swap_background(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let name = self.session.glyph_name.clone();
        let Some(index) = self.font.index_of(&name) else {
            return;
        };
        let Some(address) = self.font.active_layer_address(&name) else {
            return;
        };
        let source = address.layer.source;
        if self
            .font
            .project
            .document_background_layer(&name, source)
            .is_none()
        {
            self.note = "no background to swap".into();
            return;
        }
        let undo_depth = self.font.master().undo_depth(index);
        let before = self.font.project.document_snapshot();
        match self
            .font
            .project
            .swap_document_layer_with_background(&address)
        {
            Ok(true) => {}
            Ok(false) => {
                self.note = "foreground and background are unchanged".into();
                return;
            }
            Err(error) => {
                self.note = format!("Background: {error}");
                return;
            }
        }
        self.metadata_undo.push(MetadataEdit::SourceStructure {
            glyph: name.clone(),
            label: "swap background".into(),
            before: Box::new(before),
            after: Box::new(self.font.project.document_snapshot()),
            undo_depth,
        });
        self.metadata_redo.clear();
        self.font.rebuild_cache();
        if !self.reload_canonical_layer(&address) {
            self.note = "The swapped foreground could not be reloaded".into();
            return;
        }
        self.modified = true;
        self.note = "swapped with background".into();
    }

    /// Empty the open glyph's background layer.
    pub(crate) fn clear_background(&mut self) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let name = self.session.glyph_name.clone();
        let Some(index) = self.font.index_of(&name) else {
            return;
        };
        let Some(source) = self.font.project.source_id(self.font.active()) else {
            return;
        };
        let undo_depth = self.font.master().undo_depth(index);
        let before = self.font.project.document_snapshot();
        match self.font.project.clear_document_background(&name, source) {
            Ok(true) => {}
            Ok(false) => {
                self.note = "no background to clear".into();
                return;
            }
            Err(error) => {
                self.note = format!("Background: {error}");
                return;
            }
        }
        self.metadata_undo.push(MetadataEdit::SourceStructure {
            glyph: name,
            label: "clear background".into(),
            before: Box::new(before),
            after: Box::new(self.font.project.document_snapshot()),
            undo_depth,
        });
        self.metadata_redo.clear();
        self.modified = true;
        self.note = "cleared background".into();
    }

    pub(crate) fn apply_op(&mut self, f: impl FnOnce(&mut Session) -> bool) {
        if !matches!(self.mode, Mode::Editor(_)) {
            return;
        }
        let mut sess = (*self.session).clone();
        if f(&mut sess) && self.sync_session_from(&mut sess) == SessionSyncOutcome::Changed {
            self.finish_open_glyph_refresh();
        }
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

    fn rectangle(name: &str, x0: f64, x1: f64) -> norad::Glyph {
        let mut glyph = norad::Glyph::new(name);
        glyph.width = 500.0;
        if let Some(character) = name.chars().next().filter(|_| name.chars().count() == 1) {
            glyph.codepoints = norad::Codepoints::new([character]);
        }
        let mut contour = norad::Contour::default();
        for (x, y) in [(x0, 0.0), (x1, 0.0), (x1, 500.0), (x0, 500.0)] {
            contour.points.push(norad::ContourPoint::new(
                x,
                y,
                norad::PointType::Line,
                false,
                None,
                None,
            ));
        }
        glyph.contours.push(contour);
        glyph
    }

    #[test]
    fn inspector_stroke_and_curve_commands_update_the_document_and_undo() {
        use norad::{Contour, ContourPoint, PointType};
        let path = std::env::temp_dir().join(format!(
            "runebender-inspector-effects-{}.ufo",
            std::process::id()
        ));
        let mut glyph = norad::Glyph::new("curve");
        glyph.width = 500.0;
        glyph.contours.push(Contour::new(
            vec![
                ContourPoint::new(0.0, 0.0, PointType::Move, false, None, None),
                ContourPoint::new(0.0, 10.0, PointType::OffCurve, false, None, None),
                ContourPoint::new(50.0, 100.0, PointType::OffCurve, false, None, None),
                ContourPoint::new(100.0, 100.0, PointType::Curve, false, None, None),
            ],
            None,
        ));
        let original = glyph.contours.clone();
        let mut font = norad::Font::new();
        font.default_layer_mut().insert_glyph(glyph);
        font.save(&path).unwrap();
        let mut app = Workspace::open(&path).unwrap();
        let index = app.font.index_of("curve").unwrap();
        app.open_glyph(index);
        for input in ["", "invalid", "NaN", "inf", "0", "-10"] {
            app.stroke_buf = input.into();
            app.command_expand_stroke();
        }
        assert!(!app.modified);
        assert_eq!(app.font.master().undo_depth(index), 0);
        app.stroke_buf = "20".into();
        app.command_expand_stroke();
        assert_ne!(projected_glyph(&app.session).contours, original);
        assert_eq!(app.session.advance(), 500.0);
        assert_eq!(app.font.master().undo_depth(index), 0);
        assert_eq!(app.metadata_undo.len(), 1);
        app.undo_active_edit(false);
        assert_eq!(projected_glyph(&app.session).contours, original);
        assert_eq!(
            norad::Font::load(&path)
                .unwrap()
                .get_glyph("curve")
                .unwrap()
                .contours,
            original
        );
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn clipboard_paste_uses_canonical_contours_and_history() {
        let path = std::env::temp_dir().join(format!(
            "runebender-canonical-clipboard-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(rectangle("A", 50.0, 450.0));
        font.save(&path).expect("the fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let index = workspace.font.index_of("A").expect("A exists");
        workspace.open_glyph(index);
        let selected = workspace
            .session
            .point_id_at(0, 0)
            .expect("the first point has canonical identity");
        Arc::make_mut(&mut workspace.session)
            .selection
            .insert(selected);
        workspace.copy_contours();
        assert_eq!(workspace.clipboard.len(), 1);

        workspace.paste_contours();
        assert_eq!(projected_glyph(&workspace.session).contours.len(), 2);
        assert_eq!(workspace.session.selection.len(), 4);
        assert!(
            workspace
                .session
                .points()
                .iter()
                .any(|point| point.id == selected),
            "the original contour keeps its stable point identity"
        );
        assert!(
            workspace
                .session
                .selection
                .iter()
                .all(|point| *point != selected)
        );
        assert_eq!(workspace.metadata_undo.len(), 1);
        workspace.undo_active_edit(false);
        assert_eq!(projected_glyph(&workspace.session).contours.len(), 1);
        workspace.undo_active_edit(true);
        assert_eq!(projected_glyph(&workspace.session).contours.len(), 2);

        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn background_actions_use_atomic_canonical_source_history() {
        let path = std::env::temp_dir().join(format!(
            "runebender-canonical-background-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(rectangle("A", 0.0, 100.0));
        font.save(&path).expect("the fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let index = workspace.font.index_of("A").expect("A exists");
        workspace.open_glyph(index);
        let source = workspace
            .font
            .project
            .source_id(0)
            .expect("source identity");
        let foreground = workspace
            .font
            .active_layer_address("A")
            .expect("foreground address");

        workspace.send_to_background();
        let (_, background) = workspace
            .font
            .project
            .document_background_layer("A", source)
            .expect("send creates the background");
        assert_eq!(background.width(), 500.0);
        assert_eq!(
            background
                .contours()
                .next()
                .unwrap()
                .points()
                .next()
                .unwrap()
                .position()
                .x,
            0.0
        );
        assert_eq!(workspace.metadata_undo.len(), 1);
        workspace.send_to_background();
        assert_eq!(workspace.metadata_undo.len(), 1, "a no-op adds no history");

        let replacement = rectangle("A", 200.0, 300.0).contours;
        workspace.apply_op(move |session| session.replace_imported_contours(&replacement));
        assert_eq!(workspace.metadata_undo.len(), 2);
        workspace.swap_background();
        assert_eq!(workspace.metadata_undo.len(), 3);
        assert_eq!(workspace.font.master().undo_depth(index), 0);
        let foreground_x = workspace
            .font
            .project
            .document_layer("A", &foreground.layer)
            .unwrap()
            .contours()
            .next()
            .unwrap()
            .points()
            .next()
            .unwrap()
            .position()
            .x;
        let background_x = workspace
            .font
            .project
            .document_background_layer("A", source)
            .unwrap()
            .1
            .contours()
            .next()
            .unwrap()
            .points()
            .next()
            .unwrap()
            .position()
            .x;
        assert_eq!((foreground_x, background_x), (0.0, 200.0));

        workspace.undo_active_edit(false);
        assert_eq!(
            projected_glyph(&workspace.session).contours[0].points[0].x,
            200.0
        );
        assert_eq!(
            workspace
                .font
                .project
                .document_background_layer("A", source)
                .unwrap()
                .1
                .contours()
                .next()
                .unwrap()
                .points()
                .next()
                .unwrap()
                .position()
                .x,
            0.0
        );
        workspace.undo_active_edit(true);
        assert_eq!(
            projected_glyph(&workspace.session).contours[0].points[0].x,
            0.0
        );

        workspace.clear_background();
        assert!(
            workspace
                .font
                .project
                .document_background_layer("A", source)
                .is_none()
        );
        workspace.undo_active_edit(false);
        assert!(
            workspace
                .font
                .project
                .document_background_layer("A", source)
                .is_some()
        );
        assert!(workspace.save());

        let reopened = Workspace::open(&path).expect("the saved fixture reopens");
        assert!(
            reopened
                .font
                .project
                .document_background_layer("A", source)
                .is_some()
        );
        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn duplicate_repeat_reapplies_the_last_transform_in_canonical_history() {
        let path = std::env::temp_dir().join(format!(
            "runebender-canonical-duplicate-repeat-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        let mut glyph = rectangle("A", 0.0, 100.0);
        glyph.contours.extend(rectangle("B", 200.0, 300.0).contours);
        font.default_layer_mut().insert_glyph(glyph);
        font.save(&path).expect("the fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let index = workspace.font.index_of("A").expect("A exists");
        workspace.open_glyph(index);
        assert_eq!(Arc::make_mut(&mut workspace.session).select_contour(0), 4);
        workspace.apply_op(|session| session.rotate_90());
        assert_eq!(workspace.metadata_undo.len(), 1);

        workspace.apply_op(|session| session.duplicate_repeat());
        let glyph = projected_glyph(&workspace.session);
        assert_eq!(glyph.contours.len(), 3);
        assert_ne!(glyph.contours[2], glyph.contours[0]);
        let duplicate_ids = (0..4)
            .map(|point| {
                workspace
                    .session
                    .point_id_at(2, point)
                    .expect("the duplicate point has canonical identity")
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(workspace.session.selection, duplicate_ids);
        assert_eq!(workspace.metadata_undo.len(), 2);
        assert_eq!(workspace.font.master().undo_depth(index), 0);

        workspace.undo_active_edit(false);
        assert_eq!(projected_glyph(&workspace.session).contours.len(), 2);
        workspace.undo_active_edit(true);
        assert_eq!(projected_glyph(&workspace.session).contours.len(), 3);

        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn hyper_pen_commits_stable_canonical_contour_steps() {
        let path = std::env::temp_dir().join(format!(
            "runebender-canonical-hyper-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(rectangle("A", 0.0, 100.0));
        font.save(&path).expect("the fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let index = workspace.font.index_of("A").expect("A exists");
        workspace.open_glyph(index);
        let mut session = (*workspace.session).clone();

        session.hyper_add(200.0, 0.0, false);
        assert_eq!(
            workspace.sync_session_from(&mut session),
            SessionSyncOutcome::Changed
        );
        assert!(session.hyper_is_active());
        assert_eq!(
            session.first_contour_point(),
            Some(kurbo::Point::new(200.0, 0.0))
        );

        session.hyper_add(300.0, 100.0, false);
        assert_eq!(
            workspace.sync_session_from(&mut session),
            SessionSyncOutcome::Changed
        );
        session.hyper_add(200.0, 200.0, true);
        assert_eq!(
            workspace.sync_session_from(&mut session),
            SessionSyncOutcome::Changed
        );
        session.hyper_close();
        assert_eq!(
            workspace.sync_session_from(&mut session),
            SessionSyncOutcome::Changed
        );
        assert!(!session.hyper_is_active());

        let glyph = projected_glyph(&session);
        assert_eq!(glyph.contours.len(), 2);
        assert_eq!(glyph.contours[1].points.len(), 3);
        assert_ne!(glyph.contours[1].points[0].typ, norad::PointType::Move);
        assert_eq!(workspace.metadata_undo.len(), 4);
        assert_eq!(workspace.font.master().undo_depth(index), 0);

        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn point_drag_cancel_noop_and_commit_use_one_canonical_history_step() {
        let path = std::env::temp_dir().join(format!(
            "runebender-canonical-point-drag-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(rectangle("A", 50.0, 450.0));
        font.save(&path).expect("the fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let index = workspace.font.index_of("A").expect("A exists");
        workspace.open_glyph(index);
        let selected = workspace.session.point_id_at(0, 0).unwrap();
        Arc::make_mut(&mut workspace.session)
            .selection
            .insert(selected);

        let mut cancelled = (*workspace.session).clone();
        cancelled.begin_point_drag();
        assert!(cancelled.drag_points_to((20.0, 10.0)));
        cancelled.cancel_point_drag();
        workspace.sync_session_from(&mut cancelled);
        assert_eq!(
            projected_glyph(&workspace.session).contours[0].points[0].x,
            50.0
        );
        assert!(workspace.metadata_undo.is_empty());

        let mut no_op = (*workspace.session).clone();
        no_op.begin_point_drag();
        no_op.end_point_drag();
        workspace.sync_session_from(&mut no_op);
        assert!(workspace.metadata_undo.is_empty());

        let mut committed = (*workspace.session).clone();
        committed.begin_point_drag();
        assert!(committed.drag_points_to((30.0, 10.0)));
        committed.end_point_drag();
        workspace.sync_session_from(&mut committed);
        assert_eq!(
            projected_glyph(&workspace.session).contours[0].points[0].x,
            80.0
        );
        assert_eq!(workspace.metadata_undo.len(), 1);
        assert_eq!(workspace.font.master().undo_depth(index), 0);
        workspace.undo_active_edit(false);
        assert_eq!(
            projected_glyph(&workspace.session).contours[0].points[0].x,
            50.0
        );

        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn rejected_session_transaction_never_falls_back_to_a_legacy_write() {
        let path = std::env::temp_dir().join(format!(
            "runebender-rejected-session-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(rectangle("A", 50.0, 450.0));
        font.save(&path).expect("the fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        let index = workspace.font.index_of("A").expect("A exists");
        workspace.open_glyph(index);
        let selected = workspace.session.point_id_at(0, 0).unwrap();
        let mut stale = (*workspace.session).clone();
        stale.selection.insert(selected);
        assert!(stale.nudge(20.0, 0.0));

        workspace
            .font
            .project
            .remove_document_glyph("A")
            .expect("the canonical glyph is removed before the stale edit commits");
        let undo_steps = workspace.metadata_undo.len();
        let legacy_steps = workspace.font.master().undo_depth(index);
        assert!(!workspace.modified);

        let outcome = workspace.sync_session_from(&mut stale);
        assert_eq!(outcome, SessionSyncOutcome::Rejected);
        assert_eq!(
            workspace.sync_session_from(&mut stale),
            SessionSyncOutcome::Rejected
        );
        assert_eq!(workspace.metadata_undo.len(), undo_steps);
        assert_eq!(workspace.font.master().undo_depth(index), legacy_steps);
        assert!(!workspace.modified);
        assert!(workspace.font.project.document_glyph("A").is_none());
        assert!(workspace.font.font().get_glyph("A").is_none());
        assert!(workspace.note.contains("could not be reloaded"));

        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }

    #[test]
    fn update_metrics_applies_reference_keys() {
        let path = std::env::temp_dir().join(format!(
            "runebender-xilem-update-metrics-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        font.default_layer_mut()
            .insert_glyph(rectangle("n", 50.0, 450.0));
        let mut h = rectangle("h", 0.0, 400.0);
        runebender::formats::metrics_keys::write_metrics_key(&mut h, true, "=n+10");
        font.default_layer_mut().insert_glyph(h);
        font.save(&path).expect("the fixture saves");

        let mut workspace = Workspace::open(&path).expect("the fixture opens");
        workspace.selected = None;
        workspace.multi_selected = Arc::new(
            [
                workspace.font.index_of("n").expect("n exists"),
                workspace.font.index_of("h").expect("h exists"),
            ]
            .into_iter()
            .collect(),
        );
        assert_eq!(workspace.selected_glyph_text(), "hn");
        workspace.command_update_metrics();
        let h = workspace.font.index_of("h").expect("h remains present");
        assert_eq!(workspace.font.master().ink_bounds(h).unwrap().x0, 60.0);
        assert!(workspace.modified);

        std::fs::remove_dir_all(path).expect("the fixture is removed");
    }
}

#[cfg(target_arch = "wasm32")]
impl Workspace {
    pub(crate) fn copy_selected_glyphs_as_text(&mut self) {
        self.note = "Clipboard is not available in this demo".into();
    }
}

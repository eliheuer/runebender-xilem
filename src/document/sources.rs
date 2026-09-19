// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Source and auxiliary-layer authoring transactions.
//!
//! Source identity survives display-order changes. Removing a source updates the
//! document; it never deletes its UFO directory. Structural undo refuses to
//! overwrite later content edits, which must be undone first.

use super::*;

#[derive(Debug, Clone)]
struct SourceFrame {
    masters: Vec<Master>,
    variable: VariableData,
    names: Vec<Arc<str>>,
    locations: Vec<Location>,
    brace: Vec<BraceSource>,
    doc: Option<norad::designspace::DesignSpaceDocument>,
    active: usize,
}

impl SourceFrame {
    fn capture(project: &Project) -> Self {
        Self {
            masters: project.masters.clone(),
            variable: project.variable.clone(),
            names: project.master_names.clone(),
            locations: project.master_locations.clone(),
            brace: project.brace.clone(),
            doc: project.ds_doc.clone(),
            active: project.active,
        }
    }

    fn matches(&self, project: &Project) -> bool {
        self.doc == project.ds_doc
            && self.names == project.master_names
            && self.locations == project.master_locations
            && self.masters.len() == project.masters.len()
            && self
                .masters
                .iter()
                .zip(&project.masters)
                .all(|(a, b)| a.font == b.font && a.source_path == b.source_path)
    }

    fn restore(self, project: &mut Project) {
        let next_source = project.variable.next_source.max(self.variable.next_source);
        project.masters = self.masters;
        project.variable = self.variable;
        project.variable.next_source = next_source;
        project.master_names = self.names;
        project.master_locations = self.locations;
        project.brace = self.brace;
        project.ds_doc = self.doc;
        project.active = self.active;
        project.finish_source_change();
    }
}

#[derive(Debug)]
struct SourceStep {
    before: SourceFrame,
    after: SourceFrame,
}

#[derive(Debug, Default)]
pub(super) struct SourceHistory {
    undo: Vec<SourceStep>,
    redo: Vec<SourceStep>,
}

impl Project {
    fn finish_source_change(&mut self) {
        self.variable.synchronize(&self.masters);
        self.model = (!self.axes.is_empty()).then(|| {
            VariationModel::new(&self.master_locations).expect("source locations were validated")
        });
        self.ds_dirty = self.ds_doc.is_some();
        for master in &mut self.masters {
            master.dirty = true;
            master.refresh_from_font();
        }
        self.snap_location_to_master(self.active);
        self.compute_compat();
    }

    fn record_source_change(&mut self, before: SourceFrame) {
        self.finish_source_change();
        self.source_history.undo.push(SourceStep {
            before,
            after: SourceFrame::capture(self),
        });
        self.source_history.redo.clear();
    }

    /// Whether a structural source/layer operation is available to undo or redo.
    pub fn has_source_history(&self, redo: bool) -> bool {
        if redo {
            !self.source_history.redo.is_empty()
        } else {
            !self.source_history.undo.is_empty()
        }
    }

    /// Undo or redo one source/layer transaction, preserving later unrelated edits.
    pub fn undo_sources(&mut self, redo: bool) -> Result<bool, String> {
        let stack = if redo {
            &mut self.source_history.redo
        } else {
            &mut self.source_history.undo
        };
        let Some(step) = stack.pop() else {
            return Ok(false);
        };
        let expected = if redo { &step.before } else { &step.after };
        if !expected.matches(self) {
            if redo {
                self.source_history.redo.push(step);
            } else {
                self.source_history.undo.push(step);
            }
            return Err(
                "Undo later glyph or metadata edits before this source/layer change".into(),
            );
        }
        if redo {
            step.after.clone().restore(self);
            self.source_history.undo.push(step);
        } else {
            step.before.clone().restore(self);
            self.source_history.redo.push(step);
        }
        Ok(true)
    }

    fn source_dimensions(
        &self,
        location: &Location,
    ) -> Result<Vec<norad::designspace::Dimension>, String> {
        if location
            .keys()
            .any(|key| !self.axes.iter().any(|axis| axis.name == *key))
        {
            return Err("source location names an unknown axis".into());
        }
        self.axes.iter().map(|axis| {
            let value = location.get(&axis.name).copied().unwrap_or(0.0);
            if !value.is_finite() || !(-1.0..=1.0).contains(&value) { return Err("source location must be inside the designspace".into()); }
            let design = super::super::var_model::denormalize_value(value, axis.min, axis.default, axis.max);
            #[expect(clippy::cast_possible_truncation, reason = "the decimal round-trip is checked before committing the Designspace coordinate")]
            let stored = design as f32;
            if stored.to_string().parse::<f64>().ok() != Some(design) {
                return Err(format!("{}: source coordinate {design} cannot round-trip through Designspace", axis.name));
            }
            Ok(norad::designspace::Dimension { name: axis.name.clone(), xvalue: Some(stored), ..Default::default() })
        }).collect()
    }

    fn normalized_source_location(&self, location: &Location) -> Location {
        self.axes
            .iter()
            .map(|axis| {
                (
                    axis.name.clone(),
                    location.get(&axis.name).copied().unwrap_or(0.0),
                )
            })
            .collect()
    }

    /// Add a complete source interpolated at normalized design coordinates.
    /// `filename` is a new UFO path relative to the Designspace file.
    pub fn add_interpolated_source(
        &mut self,
        name: &str,
        filename: &str,
        location: &Location,
    ) -> Result<SourceId, String> {
        let doc = self
            .ds_doc
            .as_ref()
            .ok_or("adding a source requires a Designspace")?;
        if name.trim().is_empty() {
            return Err("source name must not be empty".into());
        }
        let path = Path::new(filename);
        if path.extension().is_none_or(|extension| extension != "ufo")
            || !path
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return Err("choose a relative .ufo filename inside the project directory".into());
        }
        if doc
            .sources
            .iter()
            .any(|source| source.filename.eq_ignore_ascii_case(filename))
        {
            return Err("that UFO destination is already used".into());
        }
        let directory = self
            .export_source
            .as_deref()
            .and_then(Path::parent)
            .or_else(|| self.masters[0].source_path.parent())
            .unwrap_or(Path::new("."));
        let destination = directory.join(filename);
        if destination.exists() {
            return Err("the new source destination already exists".into());
        }
        let dimensions = self.source_dimensions(location)?;
        let location = self.normalized_source_location(location);
        let mut locations = self.master_locations.clone();
        locations.push(location.clone());
        VariationModel::new(&locations)?;
        let default = self
            .master_locations
            .iter()
            .position(|l| l.values().all(|v| *v == 0.0))
            .ok_or("missing default source")?;
        let mut font = self.masters[default].font.clone();
        font.layers.retain(|layer| layer.is_default());
        font.default_layer_mut().clear();
        for glyph in self.glyph_names() {
            if self.glyph_sources(glyph)?.is_empty() {
                continue;
            }
            font.default_layer_mut()
                .insert_glyph(self.try_interpolated_at(glyph, &location)?);
        }
        let pairs: std::collections::BTreeSet<_> = self
            .masters
            .iter()
            .flat_map(|m| {
                m.font.kerning.iter().flat_map(|(left, pairs)| {
                    pairs.keys().map(move |right| (left.clone(), right.clone()))
                })
            })
            .collect();
        font.kerning.clear();
        for (left, right) in pairs {
            let value = self.interpolated_kerning_at(&left, &right, &location)?;
            font.kerning.entry(left).or_default().insert(right, value);
        }
        let model = VariationModel::new(&self.master_locations)?;
        let metrics: Vec<_> = self
            .masters
            .iter()
            .map(|m| {
                vec![
                    m.font.font_info.ascender.unwrap_or(800.0),
                    m.font.font_info.descender.unwrap_or(-200.0),
                    m.font.font_info.x_height.unwrap_or(500.0),
                    m.font.font_info.cap_height.unwrap_or(700.0),
                ]
            })
            .collect();
        let metrics = model.interpolate(&metrics, &location)?;
        font.font_info.ascender = Some(metrics[0]);
        font.font_info.descender = Some(metrics[1]);
        font.font_info.x_height = Some(metrics[2]);
        font.font_info.cap_height = Some(metrics[3]);
        font.font_info.style_name = Some(name.to_owned());
        let before = SourceFrame::capture(self);
        // A full source at an intermediate location takes over participation.
        // Keep the original sparse layer and its metadata as an auxiliary layer.
        let full_sources: Vec<_> = doc
            .sources
            .iter()
            .filter(|source| source.layer.is_none())
            .collect();
        let promoted: Vec<_> = self
            .brace
            .iter()
            .filter(|source| source.location == location)
            .map(|source| {
                (
                    full_sources[source.master].filename.clone(),
                    source.layer.clone(),
                )
            })
            .collect();
        self.brace.retain(|source| source.location != location);
        self.ds_doc
            .as_mut()
            .expect("validated designspace")
            .sources
            .retain(|source| {
                !promoted.iter().any(|(filename, layer)| {
                    source.filename == *filename && source.layer.as_ref() == Some(layer)
                })
            });
        let id = SourceId(self.variable.next_source);
        self.variable.next_source += 1;
        self.variable.source_ids.push(id);
        self.masters.push(Master::from_font(font, destination));
        self.master_names.push(name.into());
        self.master_locations.push(location);
        self.ds_doc
            .as_mut()
            .expect("validated designspace")
            .sources
            .push(norad::designspace::Source {
                filename: filename.into(),
                name: Some(format!("source-{}", id.0)),
                stylename: Some(name.into()),
                location: dimensions,
                ..Default::default()
            });
        self.active = self.masters.len() - 1;
        self.record_source_change(before);
        Ok(id)
    }

    /// Remove a non-default source and its layer-source descriptors.
    /// Its on-disk UFO is retained; Undo restores the in-memory source.
    pub fn remove_source(&mut self, id: SourceId) -> Result<(), String> {
        let index = self.source_index(id).ok_or("unknown source")?;
        if self.masters.len() == 1
            || self
                .master_locations
                .get(index)
                .is_none_or(|l| l.values().all(|v| *v == 0.0))
        {
            return Err("the default source must remain in the project".into());
        }
        self.ensure_source_reindex_available()?;
        let filename = self
            .ds_doc
            .as_ref()
            .ok_or("not a Designspace")?
            .sources
            .iter()
            .filter(|source| source.layer.is_none())
            .nth(index)
            .ok_or("missing source descriptor")?
            .filename
            .clone();
        let before = SourceFrame::capture(self);
        self.ds_doc
            .as_mut()
            .expect("validated designspace")
            .sources
            .retain(|source| source.filename != filename);
        self.masters.remove(index);
        self.master_names.remove(index);
        self.master_locations.remove(index);
        self.variable.source_ids.remove(index);
        self.brace.retain_mut(|source| {
            if source.master == index {
                return false;
            }
            if source.master > index {
                source.master -= 1;
            }
            true
        });
        self.active = if self.active > index {
            self.active - 1
        } else if self.active == index {
            0
        } else {
            self.active
        };
        self.record_source_change(before);
        Ok(())
    }

    fn ensure_source_reindex_available(&self) -> Result<(), String> {
        if !self.experiments.versions.is_empty() {
            return Err(
                "Close live source experiments before removing or reordering sources".into(),
            );
        }
        Ok(())
    }

    /// Move a source to a display position without changing any source identity.
    pub fn move_source(&mut self, id: SourceId, to: usize) -> Result<bool, String> {
        let from = self.source_index(id).ok_or("unknown source")?;
        if to >= self.masters.len() {
            return Err("source position is outside the source list".into());
        }
        if from == to {
            return Ok(false);
        }
        self.ensure_source_reindex_available()?;
        let doc = self.ds_doc.as_ref().ok_or("not a Designspace")?;
        let mut sources: Vec<_> = doc
            .sources
            .iter()
            .filter(|s| s.layer.is_none())
            .cloned()
            .collect();
        let before = SourceFrame::capture(self);
        let mut order: Vec<_> = (0..self.masters.len()).collect();
        let previous = order.remove(from);
        order.insert(to, previous);
        self.active = order
            .iter()
            .position(|index| *index == self.active)
            .expect("permutation");
        for source in &mut self.brace {
            source.master = order
                .iter()
                .position(|index| *index == source.master)
                .expect("permutation");
        }
        let master = self.masters.remove(from);
        self.masters.insert(to, master);
        let name = self.master_names.remove(from);
        self.master_names.insert(to, name);
        let location = self.master_locations.remove(from);
        self.master_locations.insert(to, location);
        let id = self.variable.source_ids.remove(from);
        self.variable.source_ids.insert(to, id);
        let source = sources.remove(from);
        sources.insert(to, source);
        let doc = self.ds_doc.as_mut().expect("validated designspace");
        sources.extend(doc.sources.iter().filter(|s| s.layer.is_some()).cloned());
        doc.sources = sources;
        self.record_source_change(before);
        Ok(true)
    }

    /// Rename a source and update its normalized location as one transaction.
    pub fn update_source(
        &mut self,
        id: SourceId,
        name: &str,
        location: &Location,
    ) -> Result<(), String> {
        let index = self.source_index(id).ok_or("unknown source")?;
        if name.trim().is_empty() {
            return Err("source name must not be empty".into());
        }
        let dimensions = self.source_dimensions(location)?;
        let location = self.normalized_source_location(location);
        let mut locations = self.master_locations.clone();
        locations[index] = location.clone();
        VariationModel::new(&locations)?;
        if self.brace.iter().any(|source| source.location == location) {
            return Err("This location has an intermediate layer source; add an interpolated source there to promote it first".into());
        }
        if self.ds_doc.is_none() {
            return Err("not a Designspace".into());
        }
        let before = SourceFrame::capture(self);
        let source = self
            .ds_doc
            .as_mut()
            .expect("validated designspace")
            .sources
            .iter_mut()
            .filter(|s| s.layer.is_none())
            .nth(index)
            .expect("source descriptor");
        source.stylename = Some(name.into());
        source.location = dimensions;
        self.master_names[index] = name.into();
        self.master_locations[index] = location;
        self.masters[index].font.font_info.style_name = Some(name.into());
        self.record_source_change(before);
        Ok(())
    }

    /// Duplicate one glyph into an auxiliary UFO layer, creating that layer if needed.
    pub fn add_glyph_layer(
        &mut self,
        glyph: &str,
        from: &LayerId,
        name: &str,
    ) -> Result<LayerId, String> {
        let index = self.source_index(from.source).ok_or("unknown source")?;
        let payload = self
            .glyph_layer(glyph, from)
            .ok_or("missing source glyph layer")?;
        let mut font = self.masters[index].font.clone();
        let target = font
            .layers
            .get_or_create_layer(name)
            .map_err(|error| error.to_string())?;
        if target.contains_glyph(glyph) {
            return Err("the glyph already has that layer".into());
        }
        target.insert_glyph(payload);
        let before = SourceFrame::capture(self);
        self.masters[index].font = font;
        self.record_source_change(before);
        Ok(LayerId {
            source: from.source,
            name: name.into(),
        })
    }

    /// Remove one auxiliary glyph layer, retaining the layer and other glyphs.
    pub fn remove_glyph_layer(&mut self, glyph: &str, id: &LayerId) -> Result<(), String> {
        let index = self.source_index(id.source).ok_or("unknown source")?;
        if self.masters[index].font.default_layer().name().as_str() == id.name {
            return Err("remove the source to remove a default layer".into());
        }
        if self.glyph_layer(glyph, id).is_none() {
            return Err("missing glyph layer".into());
        }
        let before = SourceFrame::capture(self);
        self.masters[index]
            .font
            .layers
            .get_mut(&id.name)
            .expect("validated layer")
            .remove_glyph(glyph);
        self.record_source_change(before);
        Ok(())
    }
}

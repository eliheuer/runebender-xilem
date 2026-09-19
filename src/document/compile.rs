// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! In-memory OpenType compilation of the live variable document.
//!
//! Preview and export share Babelfont's fontc source. No saved UFO, subprocess,
//! Python environment or repository build script participates in this pipeline.

use std::sync::Arc;

use babelfont::convertors::fontir::{BabelfontIrSource, CompilationOptions};
use fontdrasil::coords::{DesignCoord, DesignLocation};

use super::project::Project;
use super::var_model::Location;
use super::variable::LayerId;
use crate::text::shape::ShapingFont;

type CompileResult = Result<Arc<CompiledFont>, String>;

#[derive(Debug, Default)]
pub(super) struct CompileCache {
    key: Option<(u64, String)>,
    result: Option<Result<Arc<CompiledFont>, String>>,
    pending: Option<PreviewJob>,
    queued: Option<babelfont::Font>,
}

/// One background compilation. Completion is polled by the host's event loop.
#[derive(Clone, Debug)]
pub struct PreviewJob {
    finished: Arc<std::sync::Mutex<Option<CompileResult>>>,
}

impl PreviewJob {
    /// Whether this worker has posted its result, including compile errors.
    pub fn is_finished(&self) -> bool {
        self.finished
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_some()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start(font: babelfont::Font) -> Self {
        let job = Self {
            finished: Arc::new(std::sync::Mutex::new(None)),
        };
        let output = job.finished.clone();
        std::thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                CompiledFont::build(font)
            }))
            .map_err(|_| {
                "The font compiler stopped unexpectedly; check source compatibility".to_string()
            })
            .and_then(|result| result)
            .map(Arc::new);
            *output.lock().unwrap_or_else(|error| error.into_inner()) = Some(result);
        });
        job
    }
}

/// A compiled, immutable revision, shared by shaping and binary export.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledFont {
    /// Complete OpenType font, including variable outlines and positioning.
    pub bytes: Arc<Vec<u8>>,
    /// Glyph names in the compiler's final glyph order.
    pub glyph_order: Vec<String>,
    /// Axis tags in the compiled font's coordinate order.
    pub axis_tags: Vec<String>,
}

impl CompiledFont {
    /// Compiled advances, including HVAR, for encoded and unencoded glyphs.
    pub fn advances(&self, normalized: &[f64]) -> Result<Vec<(String, f64)>, String> {
        use skrifa::{
            MetadataProvider as _,
            instance::{LocationRef, Size},
        };
        let font = skrifa::FontRef::new(&self.bytes).map_err(|error| error.to_string())?;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "OpenType coordinates use 14 fractional bits"
        )]
        let coords: Vec<_> = normalized
            .iter()
            .map(|value| skrifa::instance::NormalizedCoord::from_f32(*value as f32))
            .collect();
        let metrics = font.glyph_metrics(Size::unscaled(), LocationRef::new(&coords));
        self.glyph_order
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let id =
                    skrifa::GlyphId::new(u32::try_from(index).map_err(|_| "glyph id overflow")?);
                Ok((
                    name.clone(),
                    f64::from(metrics.advance_width(id).unwrap_or(0.0)),
                ))
            })
            .collect()
    }

    /// Draw the compiled outlines at the same normalized location as shaping.
    pub fn outlines(
        &self,
        normalized: &[f64],
    ) -> Result<Vec<(String, Arc<kurbo::BezPath>)>, String> {
        use skrifa::{
            MetadataProvider as _,
            instance::{LocationRef, Size},
            outline::{DrawSettings, OutlinePen},
        };
        #[derive(Default)]
        struct Pen(kurbo::BezPath);
        impl OutlinePen for Pen {
            fn move_to(&mut self, x: f32, y: f32) {
                self.0.move_to((f64::from(x), f64::from(y)));
            }
            fn line_to(&mut self, x: f32, y: f32) {
                self.0.line_to((f64::from(x), f64::from(y)));
            }
            fn quad_to(&mut self, x: f32, y: f32, x1: f32, y1: f32) {
                self.0
                    .quad_to((f64::from(x), f64::from(y)), (f64::from(x1), f64::from(y1)));
            }
            fn curve_to(&mut self, x: f32, y: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
                self.0.curve_to(
                    (f64::from(x), f64::from(y)),
                    (f64::from(x1), f64::from(y1)),
                    (f64::from(x2), f64::from(y2)),
                );
            }
            fn close(&mut self) {
                self.0.close_path();
            }
        }
        let font = skrifa::FontRef::new(&self.bytes).map_err(|error| error.to_string())?;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "OpenType coordinates use 14 fractional bits"
        )]
        let coords: Vec<_> = normalized
            .iter()
            .map(|v| skrifa::instance::NormalizedCoord::from_f32(*v as f32))
            .collect();
        let outlines = font.outline_glyphs();
        self.glyph_order
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let mut pen = Pen::default();
                if let Some(glyph) = outlines.get(skrifa::GlyphId::new(
                    u32::try_from(index).map_err(|_| "glyph id overflow")?,
                )) {
                    glyph
                        .draw(
                            DrawSettings::unhinted(Size::unscaled(), LocationRef::new(&coords)),
                            &mut pen,
                        )
                        .map_err(|error| error.to_string())?;
                }
                Ok((name.clone(), Arc::new(pen.0)))
            })
            .collect()
    }

    /// Compile the same snapshot used for preview or export.
    pub fn build(font: babelfont::Font) -> Result<Self, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| Self::build_inner(font)))
                .map_err(|_| {
                    "The font compiler stopped unexpectedly; check source compatibility".to_string()
                })?
        }
        #[cfg(target_arch = "wasm32")]
        Self::build_inner(font)
    }

    fn build_inner(mut font: babelfont::Font) -> Result<Self, String> {
        if font.masters.is_empty() || font.glyphs.iter().next().is_none() {
            return Err("the font needs a source and at least one glyph".into());
        }
        let options = CompilationOptions {
            dont_use_production_names: true,
            produce_varc_table: false,
            ..CompilationOptions::default()
        };
        // This snapshot already represents UFO semantics. The convenience compiler
        // applies Glyphs feature rewrites whose AST cannot parse fractional rule
        // conditions. Preserve raw FEA and pass the Babelfont IR source to fontc.
        use babelfont::filters::{FontFilter as _, RetainGlyphs};
        let features = std::mem::take(&mut font.features);
        let names = font
            .glyphs
            .iter()
            .filter(|g| g.exported)
            .map(|g| g.name.to_string())
            .collect();
        RetainGlyphs::new(names)
            .apply(&mut font)
            .map_err(|error| error.to_string())?;
        font.features = features;
        let source = BabelfontIrSource::new(font, options);
        let bytes = fontc::generate_font(Box::new(source), fontc::Options::default())
            .map_err(|error| format!("{error:#?}"))?;
        let shaping = ShapingFont::from_bytes(bytes.clone())?;
        use skrifa::MetadataProvider as _;
        let axis_tags = skrifa::FontRef::new(&bytes)
            .map_err(|error| error.to_string())?
            .axes()
            .iter()
            .map(|axis| axis.tag().to_string())
            .collect();
        Ok(Self {
            bytes: Arc::new(bytes),
            glyph_order: shaping.glyph_order().to_vec(),
            axis_tags,
        })
    }
}

impl Project {
    /// A Babelfont snapshot of all live source layers, including unsaved edits.
    /// UFO-only metadata remains in the document's persistence adapter.
    pub fn babelfont_snapshot(&self) -> Result<babelfont::Font, String> {
        self.snapshot_with_features(None)
    }

    /// Check a feature draft with all live masters, axes and Designspace rules.
    /// This does not apply the draft or change the cached preview revision.
    pub fn check_features(&self, features: &str) -> Result<(), String> {
        CompiledFont::build(self.snapshot_with_features(Some(features))?).map(|_| ())
    }

    #[expect(
        clippy::cast_possible_truncation,
        reason = "OpenType metrics are quantized only in the compiler snapshot"
    )]
    fn snapshot_with_features(
        &self,
        feature_text: Option<&str>,
    ) -> Result<babelfont::Font, String> {
        let mut font = self.variable_font().clone();
        font.axes = self
            .axes
            .iter()
            .map(|axis| axis.user.backend())
            .collect::<Result<_, _>>()?;
        let default = self.default_source_index();
        let default_id = self.source_id(default).expect("default source identity");
        let source = &self.sources()[default].font;
        let info = &source.font_info;
        font.upm = info
            .units_per_em
            .map(|v| v.as_f64())
            .unwrap_or(1000.0)
            .round() as u16;
        if !(16..=16384).contains(&font.upm) {
            return Err("units per em must be between 16 and 16384".into());
        }
        font.names.family_name = info.family_name.as_deref().unwrap_or("Untitled").into();
        font.names.preferred_subfamily_name =
            info.style_name.as_deref().unwrap_or("Regular").into();
        let features = feature_text
            .unwrap_or(
                self.document_feature_text(default_id)
                    .expect("default source metadata"),
            )
            .lines()
            .filter(|line| line.trim() != crate::text::features::INCLUDE_LINE)
            .collect::<Vec<_>>()
            .join("\n");
        font.features = babelfont::Features::from_fea(&features);
        super::compile_metadata::apply(&mut font, info)?;
        super::compile_metadata::rules(self, &mut font)?;
        font.source = Some(self.sources()[default].source_path.join("features.fea"));
        font.masters.clear();
        font.first_kern_groups.clear();
        font.second_kern_groups.clear();
        for (name, members) in &source.groups {
            let destination = if name.starts_with("public.kern1.") {
                &mut font.first_kern_groups
            } else if name.starts_with("public.kern2.") {
                &mut font.second_kern_groups
            } else {
                continue;
            };
            destination.insert(
                name.as_str().into(),
                members.iter().map(|name| name.as_str().into()).collect(),
            );
        }
        for (index, source) in self.sources().iter().enumerate() {
            let mut master = babelfont::Master::new(
                self.master_names[index].as_ref(),
                self.source_id(index)
                    .expect("source identity")
                    .0
                    .to_string(),
                self.design_location(self.master_locations.get(index).unwrap_or(&Location::new()))?,
            );
            let info = &source.font.font_info;
            super::compile_metadata::metrics(&mut master, info)?;
            for (key, value) in [
                (babelfont::MetricType::Ascender, info.ascender),
                (babelfont::MetricType::Descender, info.descender),
                (babelfont::MetricType::XHeight, info.x_height),
                (babelfont::MetricType::CapHeight, info.cap_height),
                (babelfont::MetricType::ItalicAngle, info.italic_angle),
            ] {
                if let Some(value) = value {
                    master.metrics.insert(key, value.round() as i32);
                }
            }
            for (left, pairs) in &source.font.kerning {
                for (right, value) in pairs {
                    let participant = |name: &norad::Name| {
                        if name.starts_with("public.kern") {
                            format!("@{name}")
                        } else {
                            name.to_string()
                        }
                    };
                    master.kerning.insert(
                        (participant(left).into(), participant(right).into()),
                        value.round() as i16,
                    );
                }
            }
            font.masters.push(master);
        }
        for glyph in &mut font.glyphs.0 {
            let sources = self.glyph_sources(&glyph.name)?;
            glyph.exported = !sources.is_empty();
            if super::model::glyph_metadata::skipped_exports(source)
                .any(|name| name == glyph.name.as_str())
            {
                glyph.exported = false;
            }
            for source in sources {
                let key = super::babelfont::layer_key(&source.layer);
                if let Some(layer) = glyph.get_layer_mut(&key)
                    && self.brace.iter().any(|brace| {
                        Some(brace.master) == self.source_index(source.layer.source)
                            && brace.layer == source.layer.name
                    })
                {
                    layer.location = Some(self.design_location(&source.location)?);
                }
            }
            if let Some(default_glyph) = self.glyph_layer(
                &glyph.name,
                &LayerId {
                    source: self.source_id(default).expect("default source identity"),
                    name: self.sources()[default]
                        .font
                        .default_layer()
                        .name()
                        .to_string(),
                },
            ) {
                glyph.codepoints = default_glyph.codepoints.iter().map(u32::from).collect();
                glyph.category = super::compile_metadata::glyph_category(source, &default_glyph)?;
            }
        }
        font.instances = self
            .instances
            .iter()
            .enumerate()
            .map(|(index, (name, location))| {
                Ok(babelfont::Instance {
                    id: format!("instance-{index}"),
                    name: name.as_ref().into(),
                    location: self.design_location(location)?,
                    ..babelfont::Instance::default()
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(font)
    }

    fn design_location(&self, location: &Location) -> Result<DesignLocation, String> {
        self.axes
            .iter()
            .map(|axis| {
                let tag = babelfont::Tag::new(
                    axis.tag
                        .as_bytes()
                        .try_into()
                        .map_err(|_| "invalid axis tag")?,
                );
                let normalized = location.get(&axis.name).copied().unwrap_or(0.0);
                let design = super::var_model::denormalize_value(
                    normalized,
                    axis.min,
                    axis.default,
                    axis.max,
                );
                Ok((tag, DesignCoord::new(design)))
            })
            .collect()
    }

    /// Compile current unsaved sources into a complete static or variable TTF.
    pub fn compile(&self) -> Result<CompiledFont, String> {
        CompiledFont::build(self.babelfont_snapshot()?)
    }

    /// Reuse the compiled revision when only the preview location changes.
    fn compile_key(&self) -> (u64, String) {
        (
            self.variable.revision,
            format!(
                "{:?}{:?}{:?}{:?}{:?}",
                self.axes, self.master_locations, self.master_names, self.instances, self.ds_doc
            ),
        )
    }

    /// Reuse a compiled revision for headless callers and deterministic proofs.
    pub fn compiled_preview(&self) -> Result<Arc<CompiledFont>, String> {
        let key = self.compile_key();
        let mut cache = self
            .variable
            .compiled
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if cache.key.as_ref() != Some(&key) || cache.result.is_none() {
            cache.result = Some(self.compile().map(Arc::new));
            cache.key = Some(key);
            cache.pending = None;
            cache.queued = None;
        }
        cache.result.as_ref().expect("compiled revision").clone()
    }

    /// Request the latest preview without blocking the desktop event loop.
    /// Edits during a compile replace one queued snapshot; stale results never publish.
    /// Browser hosts currently compile synchronously because Rust threads are unavailable.
    pub fn request_preview(&self) -> Result<Option<Arc<CompiledFont>>, String> {
        #[cfg(target_arch = "wasm32")]
        {
            self.compiled_preview().map(Some)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let key = self.compile_key();
            let mut cache = self
                .variable
                .compiled
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if cache.key.as_ref() != Some(&key) {
                cache.result = None;
                cache.key = Some(key);
                match self.babelfont_snapshot() {
                    Ok(font) => {
                        if cache.pending.is_some() {
                            cache.queued = Some(font);
                        } else {
                            cache.pending = Some(PreviewJob::start(font));
                        }
                    }
                    Err(error) => {
                        cache.pending = None;
                        cache.queued = None;
                        cache.result = Some(Err(error));
                    }
                }
            }
            if cache.pending.as_ref().is_some_and(PreviewJob::is_finished) {
                let job = cache.pending.take().expect("finished job");
                let result = job
                    .finished
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                    .expect("finished result");
                if let Some(font) = cache.queued.take() {
                    cache.pending = Some(PreviewJob::start(font));
                } else {
                    cache.result = Some(result);
                }
            }
            cache.result.clone().transpose()
        }
    }

    /// The current background job, if the UI should arrange a completion wakeup.
    pub fn preview_job(&self) -> Option<PreviewJob> {
        self.variable
            .compiled
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pending
            .clone()
    }
}

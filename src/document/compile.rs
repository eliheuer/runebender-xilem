// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! In-memory OpenType compilation of the live variable document.
//!
//! Preview and export share Babelfont's fontc source. No saved UFO, subprocess,
//! Python environment or repository build script participates in this pipeline.

use std::path::PathBuf;
use std::sync::Arc;

use babelfont::convertors::fontir::{BabelfontIrSource, CompilationOptions};
use fontdrasil::coords::{DesignCoord, DesignLocation};

use super::project::Project;
use super::variable::{LayerId, SourceId};
use crate::text::shape::ShapingFont;

type CompileResult = Result<Arc<CompiledFont>, String>;

#[derive(Debug, Default)]
pub(super) struct CompileCache {
    key: Option<(
        u64,
        Option<super::model::designspace::CanonicalCompilerStructure>,
    )>,
    result: Option<Result<Arc<CompiledFont>, String>>,
    pending: Option<PreviewJob>,
    queued: Option<babelfont::Font>,
}

struct CompileSourceInput {
    id: SourceId,
    name: String,
    location: DesignLocation,
    path: PathBuf,
    default_layer: LayerId,
    is_default: bool,
}

fn canonical_design_location(
    axes: &[super::model::designspace::CanonicalAxis],
    location: &super::model::designspace::CanonicalLocation,
) -> Result<DesignLocation, String> {
    axes.iter()
        .map(|axis| {
            let tag = babelfont::Tag::new(
                axis.coordinates
                    .tag
                    .as_bytes()
                    .try_into()
                    .map_err(|_| "invalid axis tag")?,
            );
            Ok((tag, DesignCoord::new(location.design(axis))))
        })
        .collect()
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

    fn snapshot_with_features(
        &self,
        feature_text: Option<&str>,
    ) -> Result<babelfont::Font, String> {
        let mut font = self.variable_font().clone();
        let structure = self.compiler_structure();
        let axes = structure
            .as_ref()
            .map(|structure| structure.axes.as_slice())
            .unwrap_or_default();
        font.axes = axes
            .iter()
            .map(|axis| {
                let mut backend = axis.coordinates.backend()?;
                backend.hidden = axis.hidden;
                for label in &axis.labels {
                    backend
                        .name
                        .insert(label.language.clone(), label.value.clone());
                }
                Ok::<_, String>(backend)
            })
            .collect::<Result<_, _>>()?;
        let compile_sources = if let Some(structure) = &structure {
            structure
                .sources
                .iter()
                .map(|source| {
                    let normalized = source.location.to_normalized(&structure.axes)?;
                    let resolved_path = self
                        .document_source_path(source.id())
                        .ok_or("canonical source identity is not loaded")?
                        .to_path_buf();
                    Ok(CompileSourceInput {
                        id: source.id(),
                        name: source.display_name().to_owned(),
                        location: canonical_design_location(&structure.axes, &source.location)?,
                        path: resolved_path,
                        default_layer: source.default_layer.clone(),
                        is_default: normalized.values().all(|value| value.abs() < 1e-9),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        } else {
            let default = self.default_source_index();
            self.document_sources()
                .enumerate()
                .map(|(index, source)| {
                    Ok(CompileSourceInput {
                        id: source.id(),
                        name: source.name().to_owned(),
                        location: DesignLocation::default(),
                        path: source.path().to_path_buf(),
                        default_layer: source.default_layer(),
                        is_default: index == default,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        };
        let default_source = compile_sources
            .iter()
            .find(|source| source.is_default)
            .ok_or("designspace has no default source")?;
        let default_id = default_source.id;
        let info = self
            .document_font_info(default_id)
            .expect("default source retains canonical font info");
        font.upm =
            super::compile_metadata::units_per_em(info.metrics.units_per_em.unwrap_or(1000.0))?;
        font.names.family_name = info
            .names
            .family_name
            .as_deref()
            .unwrap_or("Untitled")
            .into();
        font.names.preferred_subfamily_name =
            info.names.style_name.as_deref().unwrap_or("Regular").into();
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
        super::compile_metadata::rules(structure.as_ref(), &mut font)?;
        font.source = Some(default_source.path.join("features.fea"));
        font.masters.clear();
        let default_metadata = self
            .document_font_metadata(default_id)
            .expect("default source metadata");
        super::compile_metadata::apply_groups(
            &mut font,
            default_metadata
                .groups()
                .iter()
                .map(|(name, members)| (name.clone(), members.clone())),
        );
        for source in &compile_sources {
            let source_id = source.id;
            let mut master = babelfont::Master::new(
                &source.name,
                source_id.0.to_string(),
                source.location.clone(),
            );
            let info = self
                .document_font_info(source_id)
                .expect("source identity retains canonical font info");
            super::compile_metadata::metrics(&mut master, info)?;
            for (key, value) in [
                (babelfont::MetricType::Ascender, info.metrics.ascender),
                (babelfont::MetricType::Descender, info.metrics.descender),
                (babelfont::MetricType::XHeight, info.metrics.x_height),
                (babelfont::MetricType::CapHeight, info.metrics.cap_height),
                (
                    babelfont::MetricType::ItalicAngle,
                    info.metrics.italic_angle,
                ),
            ] {
                if let Some(value) = value {
                    let name = format!("{key:?}");
                    master
                        .metrics
                        .insert(key, super::compile_metadata::metric(&name, value)?);
                }
            }
            super::compile_metadata::apply_kerning(
                &mut master,
                self.document_font_metadata(source_id)
                    .expect("source metadata")
                    .kerning_pairs()
                    .map(|(left, right, value)| {
                        (
                            left.as_raw_name().to_owned(),
                            right.as_raw_name().to_owned(),
                            value,
                        )
                    }),
            )?;
            font.masters.push(master);
        }
        for glyph in &mut font.glyphs.0 {
            let sources = self.glyph_sources(&glyph.name)?;
            let source_glyph_metadata =
                self.document_source_glyph_metadata(default_id, &glyph.name);
            glyph.exported = !sources.is_empty()
                && source_glyph_metadata.is_none_or(|metadata| metadata.exported());
            for source in sources {
                let key = super::babelfont::layer_key(&source.layer);
                if let Some(sparse) = structure.as_ref().and_then(|structure| {
                    structure
                        .sparse_sources
                        .iter()
                        .find(|sparse| sparse.layer == source.layer)
                }) && let Some(layer) = glyph.get_layer_mut(&key)
                {
                    layer.location = Some(canonical_design_location(
                        &structure.as_ref().expect("sparse source structure").axes,
                        &sparse.location,
                    )?);
                }
            }
            if let Some(layer) = self.document_layer(&glyph.name, &default_source.default_layer) {
                glyph.codepoints = layer.codepoints().map(u32::from).collect();
                let explicit_category =
                    source_glyph_metadata.and_then(|metadata| metadata.category());
                glyph.category = super::compile_metadata::glyph_category_from_values(
                    &glyph.name,
                    explicit_category,
                    layer.codepoints(),
                    layer.anchors().map(|anchor| anchor.name()),
                )?;
            }
        }
        font.instances = if let Some(structure) = &structure {
            structure
                .instances
                .iter()
                .map(|instance| {
                    Ok(babelfont::Instance {
                        id: format!("instance-{}", instance.id().get()),
                        name: instance.display_name().into(),
                        location: canonical_design_location(&structure.axes, &instance.location)?,
                        ..babelfont::Instance::default()
                    })
                })
                .collect::<Result<_, String>>()?
        } else {
            Vec::new()
        };
        Ok(font)
    }

    /// Compile current unsaved sources into a complete static or variable TTF.
    pub fn compile(&self) -> Result<CompiledFont, String> {
        CompiledFont::build(self.babelfont_snapshot()?)
    }

    /// Reuse the compiled revision when only the preview location changes.
    fn compile_key(
        &self,
    ) -> (
        u64,
        Option<super::model::designspace::CanonicalCompilerStructure>,
    ) {
        (self.variable.revision, self.compiler_structure())
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

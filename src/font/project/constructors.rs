// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Project assembly around already canonical source documents.

use std::collections::{BTreeMap, HashSet};

use super::*;

impl Project {
    /// Build a new single-source document directly from typed canonical template values.
    pub fn new_canonical_font(
        path: PathBuf,
        family: &str,
        style: &str,
        weight_class: u32,
    ) -> Result<Self, String> {
        let specification = crate::font::new_font::specification(family, style, weight_class)?;
        let variable = VariableData::from_new_font(specification)?;
        Self::from_canonical_single_source(variable, path, true)
    }

    /// Finish an explicit UFO import boundary after its transient decoder has been validated.
    pub(in crate::font) fn from_ufo_boundary(
        path: PathBuf,
        font: &norad::Font,
    ) -> Result<Self, String> {
        let variable = super::super::persistence::ufo_codec::decode_source(font)?;
        Self::from_canonical_single_source(variable, path, false)
    }

    /// Finish an imported single-source document whose first save must create a new UFO.
    pub(crate) fn from_imported_ufo_boundary(
        path: PathBuf,
        font: &norad::Font,
    ) -> Result<Self, String> {
        let variable = super::super::persistence::ufo_codec::decode_source(font)?;
        Self::from_canonical_single_source(variable, path, true)
    }

    /// Decode imported Designspace sources into canonical ownership before projections exist.
    pub(crate) fn from_imported_designspace_boundary(
        document: norad::designspace::DesignSpaceDocument,
        sources: Vec<(String, norad::Font, PathBuf)>,
    ) -> Result<Self, String> {
        let mut source_map = BTreeMap::new();
        for (filename, font, path) in sources {
            if source_map.insert(filename.clone(), (font, path)).is_some() {
                return Err(format!("duplicate imported source {filename}"));
            }
        }
        let mut seen = HashSet::new();
        let filenames = document
            .sources
            .iter()
            .filter(|source| source.layer.is_none())
            .map(|source| {
                if !seen.insert(source.filename.clone()) {
                    return Err(format!(
                        "duplicate full source file {} is not editable independently",
                        source.filename
                    ));
                }
                if !source_map.contains_key(&source.filename) {
                    return Err(format!("missing imported source {}", source.filename));
                }
                Ok(source.filename.clone())
            })
            .collect::<Result<Vec<_>, String>>()?;
        if filenames.len() != source_map.len() {
            return Err("imported sources do not match Designspace sources".into());
        }
        let variable = super::super::persistence::ufo_codec::decode_sources(
            filenames
                .iter()
                .map(|filename| &source_map.get(filename).expect("checked source").0),
        )?;
        let mut inputs = BTreeMap::new();
        for filename in filenames {
            let (font, path) = source_map.remove(&filename).expect("checked source");
            let mut input = SourceInput::from_font(font, path);
            input.dirty = true;
            inputs.insert(filename, input);
        }
        Self::from_designspace_with_variable(
            document,
            |filename| {
                inputs
                    .remove(filename)
                    .ok_or("missing imported source".into())
            },
            Some(variable),
        )
    }

    fn from_canonical_single_source(
        variable: VariableData,
        path: PathBuf,
        dirty: bool,
    ) -> Result<Self, String> {
        let source = SourceId(0);
        let name: Arc<str> = variable
            .font_info(source)
            .and_then(|info| info.names.style_name.clone())
            .unwrap_or_else(|| "Regular".into())
            .into();
        let state = SourceState::new(path, dirty);
        let mut project = Self {
            sources: vec![state],
            variable,
            source_history: sources::SourceHistory::default(),
            document_history: super::super::history::DocumentHistory::default(),
            edit_transaction_history: edit_transactions::EditTransactionHistory::default(),
            source_metadata_history: super::super::history::SourceMetadataHistory::default(),
            active: 0,
            master_names: vec![name],
            axes: Vec::new(),
            master_locations: vec![Location::new()],
            model: None,
            location: Location::new(),
            compat: HashMap::new(),
            export_source: None,
            instances: Vec::new(),
            ds_dirty: false,
            brace: Vec::new(),
            experiments: super::super::experiments::Experiments::default(),
        };
        project.compute_compat();
        Ok(project)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "runebender-new-canonical-font-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn new_font_is_canonical_before_its_compatibility_projection() {
        let project = Project::new_canonical_font(
            PathBuf::from("CanonicalNew.ufo"),
            "Untitled",
            "Regular",
            400,
        )
        .unwrap();
        let source = project.source_id(0).unwrap();
        let layer = project.document_source(source).unwrap().default_layer();
        let names = project.glyph_names().collect::<Vec<_>>();
        assert_eq!(names.len(), 324);
        assert_eq!(names.first(), Some(&".notdef"));
        assert_eq!(project.document_source_is_modified(source), Some(true));
        assert_eq!(
            project.document_layer("space", &layer).unwrap().width(),
            260.0
        );
        assert_eq!(
            project
                .document_layer("space", &layer)
                .unwrap()
                .codepoints()
                .collect::<Vec<_>>(),
            [' ']
        );
        assert_eq!(project.document_layer("A", &layer).unwrap().width(), 600.0);
        assert_eq!(
            project
                .document_font_info(source)
                .unwrap()
                .names
                .family_name
                .as_deref(),
            Some("Untitled")
        );
        assert_eq!(
            project
                .document_font_info(source)
                .unwrap()
                .metrics
                .units_per_em,
            Some(1000.0)
        );
    }

    #[test]
    fn new_font_save_and_reopen_preserve_the_template() {
        let scratch = Scratch::new();
        let path = scratch.0.join("CanonicalNew.ufo");
        let mut project = Project::new_font(path.clone());
        let names = project
            .glyph_names()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        project.save().unwrap();
        let reopened = Project::load(&path).unwrap();
        let reopened_names = reopened.glyph_names().collect::<HashSet<_>>();
        assert_eq!(reopened_names.len(), names.len());
        assert!(
            names
                .iter()
                .all(|name| reopened_names.contains(name.as_str()))
        );
        let source = reopened.source_id(0).unwrap();
        let layer = reopened.document_source(source).unwrap().default_layer();
        assert_eq!(
            reopened.document_layer("space", &layer).unwrap().width(),
            260.0
        );
        assert_eq!(
            reopened
                .document_layer("A", &layer)
                .unwrap()
                .codepoints()
                .collect::<Vec<_>>(),
            ['A']
        );
    }

    #[test]
    fn binary_import_uses_canonical_construction_and_preserves_an_existing_destination() {
        let scratch = Scratch::new();
        let binary = scratch.0.join("Imported.ttf");
        std::fs::write(
            &binary,
            include_bytes!("../../../assets/fonts/VirtuaGrotesk-Regular.ttf"),
        )
        .unwrap();
        let occupied = scratch.0.join("Imported.ufo");
        std::fs::create_dir(&occupied).unwrap();
        std::fs::write(occupied.join("sentinel"), b"keep").unwrap();

        let project = Project::load(&binary).unwrap();
        let destination = scratch.0.join("Imported-import-1.ufo");
        let source = project.source_id(0).unwrap();

        assert_eq!(
            project.export_source.as_deref(),
            Some(destination.as_path())
        );
        assert_eq!(
            project.document_source_path(source),
            Some(destination.as_path())
        );
        assert_eq!(project.document_source_is_modified(source), Some(true));
        assert!(project.document_source(source).is_some());
        assert_eq!(std::fs::read(occupied.join("sentinel")).unwrap(), b"keep");
        assert!(!destination.exists());
    }

    #[test]
    fn imported_designspace_validates_canonical_sources_before_projection() {
        let document = crate::font::persistence::memory::designspace_from_str(
            r#"<designspace format="5.0">
  <axes><axis tag="wght" name="Weight" minimum="0" default="0" maximum="1"/></axes>
  <sources>
    <source filename="Regular.ufo" stylename="Regular"><location><dimension name="Weight" xvalue="0"/></location></source>
    <source filename="Bold.ufo" stylename="Bold"><location><dimension name="Weight" xvalue="1"/></location></source>
  </sources>
</designspace>"#,
        )
        .unwrap();
        let regular = imported_font("Regular", 500.0);
        let mut bold = imported_font("Bold", 700.0);
        bold.lib.insert(
            "public.skipExportGlyphs".into(),
            plist::Value::String("A".into()),
        );
        let inputs = |bold| {
            vec![
                (
                    "Regular.ufo".into(),
                    regular.clone(),
                    PathBuf::from("Import/Regular.ufo"),
                ),
                ("Bold.ufo".into(), bold, PathBuf::from("Import/Bold.ufo")),
            ]
        };

        let error =
            Project::from_imported_designspace_boundary(document.clone(), inputs(bold.clone()))
                .unwrap_err();
        assert!(error.contains("public.skipExportGlyphs"), "{error}");

        bold.lib.insert(
            "public.skipExportGlyphs".into(),
            plist::Value::Array(Vec::new()),
        );
        let project = Project::from_imported_designspace_boundary(document, inputs(bold)).unwrap();
        let bold_source = SourceId(1);
        let bold_layer = LayerId {
            source: bold_source,
            name: "public.default".into(),
        };
        assert_eq!(project.document_sources().count(), 2);
        assert!(
            project
                .document_sources()
                .all(|source| project.document_source_is_modified(source.id()) == Some(true))
        );
        assert_eq!(
            project.document_source_path(bold_source),
            Some(Path::new("Import/Bold.ufo"))
        );
        assert_eq!(
            project.document_layer("A", &bold_layer).unwrap().width(),
            700.0
        );
        assert!(project.document_designspace().is_some());
    }

    fn imported_font(style: &str, width: f64) -> norad::Font {
        let mut font = norad::Font::new();
        font.font_info.family_name = Some("Imported Family".into());
        font.font_info.style_name = Some(style.into());
        let mut glyph = norad::Glyph::new("A");
        glyph.width = width;
        glyph.codepoints.insert('A');
        font.default_layer_mut().insert_glyph(glyph);
        font
    }
}

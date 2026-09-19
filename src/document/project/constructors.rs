// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Project assembly around already canonical single-source documents.

use std::collections::HashMap;

use super::*;

impl Project {
    /// Build a new single-source document directly from typed canonical template values.
    pub fn new_canonical_font(
        path: PathBuf,
        family: &str,
        style: &str,
        weight_class: u32,
    ) -> Result<Self, String> {
        let specification = crate::document::new_font::specification(family, style, weight_class)?;
        let variable = VariableData::from_new_font(specification)?;
        Self::from_canonical_single_source(variable, path, true, HashMap::new())
    }

    /// Finish an explicit UFO import boundary after its transient decoder has been validated.
    pub(in crate::document) fn from_ufo_boundary(
        path: PathBuf,
        font: &norad::Font,
        glif_paths: HashMap<String, String>,
    ) -> Result<Self, String> {
        let variable = VariableData::from_ufo_boundary(font)?;
        Self::from_canonical_single_source(variable, path, false, glif_paths)
    }

    fn from_canonical_single_source(
        variable: VariableData,
        path: PathBuf,
        dirty: bool,
        glif_paths: HashMap<String, String>,
    ) -> Result<Self, String> {
        let source = SourceId(0);
        let font = variable
            .source_font(source)
            .ok_or("canonical source has no persistence projection")?;
        let name: Arc<str> = variable
            .font_info(source)
            .and_then(|info| info.names.style_name.clone())
            .unwrap_or_else(|| "Regular".into())
            .into();
        let mut master = Master::from_font(font, path);
        master.dirty = dirty;
        master.glif_paths = glif_paths;
        let mut project = Self {
            masters: vec![master],
            variable,
            source_history: sources::SourceHistory::default(),
            document_history: super::super::history::DocumentHistory::default(),
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
            ds_doc: None,
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
        assert!(project.sources()[0].dirty);
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
}

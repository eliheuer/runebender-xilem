// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Atomic document retargeting for Save As, including external feature includes.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use fea_rs::parse::{FileSystemResolver, SourceLoadError, SourceResolver};

use super::*;

struct SaveAsPlan {
    export: super::super::filesystem::ExportPlan,
    source_targets: Vec<PathBuf>,
    designspace: Option<(
        PathBuf,
        norad::designspace::DesignSpaceDocument,
        super::super::model::designspace::CanonicalDesignspace,
    )>,
}

impl Project {
    /// Resolve every file read by this document's relative OpenType feature includes.
    ///
    /// Paths are normalized against each source UFO and include nested dependencies. Hosts use
    /// this read-only codec boundary to watch external files alongside UFO and Designspace roots.
    pub fn feature_dependency_paths(&self) -> Result<Vec<PathBuf>, String> {
        let mut dependencies = Vec::new();
        for source in self.document_sources() {
            let text = self
                .document_feature_text(source.id())
                .ok_or("missing canonical feature text")?;
            if !text.contains("include") {
                continue;
            }
            let source_root = super::super::filesystem::destination_key(source.path())?;
            dependencies.extend(feature_dependencies(&source_root, text)?);
        }
        dependencies.sort();
        dependencies.dedup();
        Ok(dependencies)
    }

    /// Publish a complete copy into `directory`, then make the new paths current.
    ///
    /// Every UFO, optional Designspace and external relative feature include is staged before
    /// publication. Existing or aliased destinations are refused, and a failure leaves both the
    /// live Project paths and original source tree unchanged.
    pub fn save_as(&mut self, directory: &Path) -> Result<(), String> {
        let plan = SaveAsPlan::new(self, directory)?;
        let SaveAsPlan {
            export,
            source_targets,
            designspace,
        } = plan;
        export.execute()?;

        for (source, target) in self.sources.iter_mut().zip(source_targets) {
            source.source_path = target;
            source.dirty = false;
        }
        if let Some((target, _document, canonical)) = designspace {
            self.variable.install_designspace(canonical);
            self.export_source = Some(target);
            self.ds_dirty = false;
        } else {
            self.export_source = self
                .sources
                .first()
                .map(|source| source.source_path.clone());
        }
        self.variable.revision = self.variable.revision.wrapping_add(1);
        Ok(())
    }
}

impl SaveAsPlan {
    fn new(project: &Project, directory: &Path) -> Result<Self, String> {
        if !directory.is_dir() {
            return Err(format!(
                "Save As destination is not a directory: {}",
                directory.display()
            ));
        }
        let directory_key = super::super::filesystem::destination_key(directory)?;
        let source_targets = project
            .sources
            .iter()
            .map(|source| {
                source
                    .source_path
                    .file_name()
                    .map(|name| directory.join(name))
                    .ok_or_else(|| format!("invalid source path {}", source.source_path.display()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let designspace = plan_designspace(project, directory, &source_targets)?;
        let files = plan_feature_includes(project, &source_targets, &directory_key)?;
        let sources = project
            .sources
            .iter()
            .zip(&source_targets)
            .enumerate()
            .map(|(index, (source, destination))| {
                let id = project.source_id(index).expect("source identity");
                Ok(super::super::filesystem::SourceExport {
                    destination: destination.clone(),
                    font: project
                        .encode_ufo_source(id)
                        .ok_or("missing canonical source data")?,
                    preserved: source.preserved_files.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let designspace_export = designspace
            .as_ref()
            .map(|(target, document, _)| (target.clone(), document.clone()));
        let export = super::super::filesystem::ExportPlan::new(sources, designspace_export)?
            .with_files(files)?
            .require_new_destinations()?;
        Ok(Self {
            export,
            source_targets,
            designspace,
        })
    }
}

fn plan_designspace(
    project: &Project,
    directory: &Path,
    source_targets: &[PathBuf],
) -> Result<
    Option<(
        PathBuf,
        norad::designspace::DesignSpaceDocument,
        super::super::model::designspace::CanonicalDesignspace,
    )>,
    String,
> {
    let Some(mut canonical) = project.document_designspace().cloned() else {
        return Ok(None);
    };
    let source_ids = project.variable.source_ids.clone();
    canonical.edit_checked(|draft| {
        for (source, target) in source_ids.iter().copied().zip(source_targets) {
            let filename = target
                .file_name()
                .ok_or_else(|| format!("invalid source destination {}", target.display()))?
                .to_string_lossy()
                .into_owned();
            draft
                .source_mut(source)
                .ok_or_else(|| format!("missing Designspace source {}", source.0))?
                .filename = filename;
        }
        Ok(())
    })?;
    let document = canonical.to_norad()?;
    let current = project
        .export_source
        .as_deref()
        .ok_or("Designspace has no source path")?;
    let name = current
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("Untitled.designspace"));
    Ok(Some((directory.join(name), document, canonical)))
}

fn plan_feature_includes(
    project: &Project,
    source_targets: &[PathBuf],
    directory_key: &Path,
) -> Result<Vec<super::super::filesystem::FileExport>, String> {
    let mut planned: BTreeMap<PathBuf, (PathBuf, Vec<u8>)> = BTreeMap::new();
    for (index, target_source) in source_targets.iter().enumerate() {
        let source = project.source_id(index).expect("source identity");
        let text = project
            .document_feature_text(source)
            .ok_or("missing canonical feature text")?;
        if !text.contains("include") {
            continue;
        }
        let source_root =
            super::super::filesystem::destination_key(&project.sources[index].source_path)?;
        for dependency in feature_dependencies(&source_root, text)? {
            if dependency.starts_with(&source_root) {
                continue;
            }
            let relative = relative_path(&source_root, &dependency)?;
            let destination =
                super::super::filesystem::destination_key(&target_source.join(relative))?;
            if !destination.starts_with(directory_key) || destination == directory_key {
                return Err(format!(
                    "feature include would escape the Save As directory: {}",
                    dependency.display()
                ));
            }
            let bytes = std::fs::read(&dependency)
                .map_err(|error| format!("{}: {error}", dependency.display()))?;
            if let Some((previous, previous_bytes)) = planned.get(&destination) {
                if previous != &dependency || previous_bytes != &bytes {
                    return Err(format!(
                        "feature includes collide at {}",
                        destination.display()
                    ));
                }
            } else {
                planned.insert(destination, (dependency, bytes));
            }
        }
    }
    Ok(planned
        .into_iter()
        .map(
            |(destination, (_, bytes))| super::super::filesystem::FileExport { destination, bytes },
        )
        .collect())
}

fn feature_dependencies(source_root: &Path, text: &str) -> Result<Vec<PathBuf>, String> {
    let root = source_root.join("features.fea");
    let loaded = Arc::new(Mutex::new(Vec::new()));
    let resolver = FeatureResolver {
        inner: FileSystemResolver::new(source_root.to_path_buf()),
        root: root.clone(),
        root_text: text.into(),
        loaded: loaded.clone(),
    };
    fea_rs::parse::parse_root(root, None, Box::new(resolver))
        .map_err(|error| format!("could not resolve feature includes: {error}"))?;
    let mut dependencies = loaded
        .lock()
        .map_err(|_| "feature include resolver was interrupted".to_string())?
        .clone();
    dependencies.sort();
    dependencies.dedup();
    Ok(dependencies)
}

struct FeatureResolver {
    inner: FileSystemResolver,
    root: PathBuf,
    root_text: Arc<str>,
    loaded: Arc<Mutex<Vec<PathBuf>>>,
}

impl SourceResolver for FeatureResolver {
    fn get_contents(&self, path: &Path) -> Result<Arc<str>, SourceLoadError> {
        if path == self.root {
            return Ok(self.root_text.clone());
        }
        let contents = self.inner.get_contents(path)?;
        let canonical = self.inner.canonicalize(path)?;
        self.loaded
            .lock()
            .map_err(|_| SourceLoadError::new(path.to_path_buf(), "resolver was interrupted"))?
            .push(canonical);
        Ok(contents)
    }

    fn resolve_raw_path(&self, path: &Path, included_from: Option<&Path>) -> PathBuf {
        self.inner.resolve_raw_path(path, included_from)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, SourceLoadError> {
        if path == self.root {
            Ok(self.root.clone())
        } else {
            self.inner.canonicalize(path)
        }
    }
}

fn relative_path(base: &Path, target: &Path) -> Result<PathBuf, String> {
    let base_display = base.display().to_string();
    let target_display = target.display().to_string();
    let base = base.components().collect::<Vec<_>>();
    let target = target.components().collect::<Vec<_>>();
    let common = base
        .iter()
        .zip(&target)
        .take_while(|(left, right)| left == right)
        .count();
    if common == 0 {
        return Err(format!(
            "cannot relocate feature include {target_display} relative to {base_display}"
        ));
    }
    let mut relative = PathBuf::new();
    for component in &base[common..] {
        if matches!(component, Component::Normal(_)) {
            relative.push("..");
        }
    }
    for component in &target[common..] {
        relative.push(component.as_os_str());
    }
    Ok(relative)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "runebender-save-as-{label}-{}-{}",
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
    fn single_ufo_save_as_copies_nested_relative_includes_without_overwriting() {
        let scratch = Scratch::new("single");
        let source = scratch.0.join("Source.ufo");
        write_font(&source, "Regular", 500.0, "include(../shared/main.fea);");
        std::fs::create_dir(scratch.0.join("shared")).unwrap();
        std::fs::write(scratch.0.join("shared/main.fea"), "include(nested.fea);").unwrap();
        std::fs::write(
            scratch.0.join("shared/nested.fea"),
            "feature kern { pos A A -10; } kern;",
        )
        .unwrap();
        let mut project = Project::load(&source).unwrap();
        let copy = scratch.0.join("copy");
        std::fs::create_dir(&copy).unwrap();
        std::fs::create_dir(copy.join("shared")).unwrap();
        std::fs::write(copy.join("shared/main.fea"), "occupied").unwrap();

        let error = project.save_as(&copy).unwrap_err();
        assert!(error.contains("already exists"), "{error}");
        assert!(!copy.join("Source.ufo").exists());
        assert_eq!(
            project.document_source_path(SourceId(0)),
            Some(source.as_path())
        );

        std::fs::remove_dir_all(copy.join("shared")).unwrap();
        project.save_as(&copy).unwrap();
        let target = copy.join("Source.ufo");
        assert_eq!(
            project.document_source_path(SourceId(0)),
            Some(target.as_path())
        );
        assert_eq!(
            std::fs::read_to_string(copy.join("shared/main.fea")).unwrap(),
            "include(nested.fea);"
        );
        assert_eq!(
            std::fs::read_to_string(copy.join("shared/nested.fea")).unwrap(),
            "feature kern { pos A A -10; } kern;"
        );
        assert_eq!(
            std::fs::read_to_string(source.join("features.fea")).unwrap(),
            "include(../shared/main.fea);"
        );
        let reopened = Project::load(&target).unwrap();
        assert_eq!(
            reopened.document_feature_text(SourceId(0)),
            Some("include(../shared/main.fea);")
        );
    }

    #[test]
    fn designspace_save_as_copies_one_shared_include_and_reopens() {
        let scratch = Scratch::new("designspace");
        for (filename, style, width) in [
            ("Regular.ufo", "Regular", 500.0),
            ("Bold.ufo", "Bold", 700.0),
        ] {
            write_font(
                &scratch.0.join(filename),
                style,
                width,
                "include(../shared.fea);",
            );
        }
        std::fs::write(
            scratch.0.join("shared.fea"),
            "feature kern { pos A A -20; } kern;",
        )
        .unwrap();
        let designspace = scratch.0.join("Family.designspace");
        let document = crate::document::font_memory::designspace_from_str(
            r#"<designspace format="5.0">
  <axes><axis tag="wght" name="Weight" minimum="0" default="0" maximum="1"/></axes>
  <sources>
    <source filename="Regular.ufo" stylename="Regular"><location><dimension name="Weight" xvalue="0"/></location></source>
    <source filename="Bold.ufo" stylename="Bold"><location><dimension name="Weight" xvalue="1"/></location></source>
  </sources>
</designspace>"#,
        )
        .unwrap();
        document.save(&designspace).unwrap();
        let original = std::fs::read(&designspace).unwrap();
        let mut project = Project::load(&designspace).unwrap();
        let copy = scratch.0.join("copy");
        std::fs::create_dir(&copy).unwrap();

        project.save_as(&copy).unwrap();

        let target = copy.join("Family.designspace");
        assert_eq!(project.export_source.as_deref(), Some(target.as_path()));
        assert_eq!(
            project
                .document_designspace()
                .unwrap()
                .sources()
                .iter()
                .map(|source| source.filename.as_str())
                .collect::<Vec<_>>(),
            ["Regular.ufo", "Bold.ufo"]
        );
        assert_eq!(
            std::fs::read_to_string(copy.join("shared.fea")).unwrap(),
            "feature kern { pos A A -20; } kern;"
        );
        assert_eq!(std::fs::read(&designspace).unwrap(), original);
        let reopened = Project::load(&target).unwrap();
        assert_eq!(reopened.document_sources().count(), 2);
        reopened.compile().unwrap();
    }

    #[test]
    fn designspace_save_as_rejects_colliding_relative_includes_atomically() {
        let scratch = Scratch::new("include-collision");
        for (directory, filename, style, width, contents) in [
            (
                "one",
                "Regular.ufo",
                "Regular",
                500.0,
                "feature kern { pos A A -10; } kern;",
            ),
            (
                "two",
                "Bold.ufo",
                "Bold",
                700.0,
                "feature kern { pos A A -20; } kern;",
            ),
        ] {
            let root = scratch.0.join(directory);
            std::fs::create_dir(&root).unwrap();
            write_font(
                &root.join(filename),
                style,
                width,
                "include(../shared.fea);",
            );
            std::fs::write(root.join("shared.fea"), contents).unwrap();
        }
        let designspace = scratch.0.join("Family.designspace");
        let document = crate::document::font_memory::designspace_from_str(
            r#"<designspace format="5.0">
  <axes><axis tag="wght" name="Weight" minimum="0" default="0" maximum="1"/></axes>
  <sources>
    <source filename="one/Regular.ufo" stylename="Regular"><location><dimension name="Weight" xvalue="0"/></location></source>
    <source filename="two/Bold.ufo" stylename="Bold"><location><dimension name="Weight" xvalue="1"/></location></source>
  </sources>
</designspace>"#,
        )
        .unwrap();
        document.save(&designspace).unwrap();
        let mut project = Project::load(&designspace).unwrap();
        let original_paths = project
            .document_sources()
            .map(|source| source.path().to_owned())
            .collect::<Vec<_>>();
        let copy = scratch.0.join("copy");
        std::fs::create_dir(&copy).unwrap();

        let error = project.save_as(&copy).unwrap_err();

        assert!(error.contains("feature includes collide"), "{error}");
        assert_eq!(
            project
                .document_sources()
                .map(|source| source.path().to_owned())
                .collect::<Vec<_>>(),
            original_paths
        );
        assert!(!copy.join("Regular.ufo").exists());
        assert!(!copy.join("Bold.ufo").exists());
        assert!(!copy.join("Family.designspace").exists());
        assert!(!copy.join("shared.fea").exists());
    }

    fn write_font(path: &Path, style: &str, width: f64, features: &str) {
        let mut font = norad::Font::new();
        font.font_info.family_name = Some("Save As Fixture".into());
        font.font_info.style_name = Some(style.into());
        font.font_info.units_per_em = Some(1000_u32.into());
        for (name, codepoint) in [(".notdef", None), ("A", Some('A'))] {
            let mut glyph = norad::Glyph::new(name);
            glyph.width = width;
            if let Some(codepoint) = codepoint {
                glyph.codepoints.insert(codepoint);
            }
            font.default_layer_mut().insert_glyph(glyph);
        }
        font.features = features.into();
        font.save(path).unwrap();
    }
}

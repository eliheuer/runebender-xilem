// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Architecture gate for production Norad dependencies.
//!
//! This parses Rust items so `#[cfg(test)]` fixtures are ignored wherever they occur; production
//! code after a test module is still inspected. Every permitted file is an explicit import,
//! export, source-format preservation or serialization boundary.

use std::path::{Path, PathBuf};

use syn::visit::Visit as _;

const ALLOWED_BOUNDARIES: &[&str] = &[
    "font/filesystem.rs",
    "font/font_memory.rs",
    "font/model/designspace.rs",
    "font/model/font_info.rs",
    "font/model/glyph_metadata.rs",
    "font/project/constructors.rs",
    "font/project/save_as.rs",
    "font/source.rs",
    "font/source_format.rs",
    "font/ufo_codec.rs",
    "formats/babelfont_import.rs",
    "formats/binary_import.rs",
    "formats/color_font.rs",
    "formats/designspace.rs",
    "formats/glyphs_import.rs",
    "formats/image_trace.rs",
    "formats/lib_keys.rs",
    "formats/metaballs.rs",
    "formats/metrics_keys.rs",
    "formats/proposal_ufo.rs",
    "formats/svg.rs",
    "formats/ufo.rs",
];

const ALLOWED_ITEMS: &[(&str, &str)] = &[
    // Stable canonical preservation records may retain exact UFO scalar/object types privately.
    ("font/babelfont.rs", "struct PreservedPoint"),
    ("font/babelfont.rs", "struct PreservedComponent"),
    ("font/babelfont.rs", "struct PreservedAnchor"),
    ("font/babelfont.rs", "struct ObjectMetadata"),
    ("font/babelfont.rs", "ObjectMetadata::method new"),
    ("font/babelfont.rs", "struct LayerPreservation"),
    ("font/babelfont.rs", "PointView::method name"),
    // These are the item-level import/export codecs in the otherwise live canonical module.
    ("font/babelfont.rs", "LayerImage::method from_ufo"),
    ("font/babelfont.rs", "LayerImage::method to_ufo"),
    ("font/babelfont.rs", "ImportedContours::method from_ufo"),
    ("font/babelfont.rs", "fn decode_imported_contours"),
    ("font/babelfont.rs", "fn validate_ufo_contours"),
    ("font/babelfont.rs", "fn fresh_hyper_identifier"),
    ("font/babelfont.rs", "fn fresh_object_identifier"),
    ("font/babelfont.rs", "fn ufo_contour_is_hyper"),
    ("font/babelfont.rs", "fn layer_from_ufo"),
    ("font/babelfont.rs", "fn affine"),
    ("font/babelfont.rs", "fn project_contours"),
    ("font/babelfont.rs", "fn project_layer"),
    // Canonical edits may update the private preservation record without exposing Norad.
    ("font/babelfont.rs", "LayerEditDraft::method paste_contours"),
    (
        "font/babelfont.rs",
        "LayerEditDraft::method replace_interpolated_contours",
    ),
    (
        "font/babelfont.rs",
        "LayerEditDraft::method set_component_reference",
    ),
    (
        "font/babelfont.rs",
        "LayerEditDraft::method set_component_transform",
    ),
    ("font/babelfont.rs", "LayerEditDraft::method add_component"),
    (
        "font/babelfont.rs",
        "LayerEditDraft::method set_component_alignment_disabled",
    ),
    ("font/project.rs", "Project::method from_designspace"),
    (
        "font/project.rs",
        "Project::method from_designspace_with_variable",
    ),
];

const FORBIDDEN_COMPATIBILITY_IDENTIFIERS: &[&str] = &[
    "SourceEdit",
    "SourceFontEdit",
    "SourcesEdit",
    "active_font",
    "active_font_mut",
    "edit_source",
    "edit_sources",
    "editing_parts",
    "glyph_layer",
    "reconcile_layer_from_ufo",
    "reconcile_compatibility_layer",
    "refresh_glyph_projections",
    "source_snapshot",
    "synchronize_compatibility_layer",
];

#[derive(Default)]
struct NoradUse {
    items: Vec<String>,
    current: String,
    current_impl: Option<String>,
}

impl<'ast> syn::visit::Visit<'ast> for NoradUse {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if item_attrs(item).iter().any(is_test_cfg) {
            return;
        }
        let previous = std::mem::replace(&mut self.current, item_name(item));
        let previous_impl = self.current_impl.take();
        if let syn::Item::Impl(item) = item {
            self.current_impl = impl_type_name(&item.self_ty);
        }
        if let syn::Item::Use(item) = item
            && use_tree_starts_with_norad(&item.tree)
        {
            self.record();
        }
        if let syn::Item::ExternCrate(item) = item
            && item.ident == "norad"
        {
            self.record();
        }
        syn::visit::visit_item(self, item);
        self.current_impl = previous_impl;
        self.current = previous;
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        if path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == "norad")
        {
            self.record();
        }
        syn::visit::visit_path(self, path);
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        if impl_item_attrs(item).iter().any(is_test_cfg) {
            return;
        }
        let previous = std::mem::replace(
            &mut self.current,
            impl_item_name(item, self.current_impl.as_deref()),
        );
        syn::visit::visit_impl_item(self, item);
        self.current = previous;
    }
}

impl NoradUse {
    fn record(&mut self) {
        if !self.items.contains(&self.current) {
            self.items.push(self.current.clone());
        }
    }
}

fn item_attrs(item: &syn::Item) -> &[syn::Attribute] {
    match item {
        syn::Item::Const(item) => &item.attrs,
        syn::Item::Enum(item) => &item.attrs,
        syn::Item::ExternCrate(item) => &item.attrs,
        syn::Item::Fn(item) => &item.attrs,
        syn::Item::ForeignMod(item) => &item.attrs,
        syn::Item::Impl(item) => &item.attrs,
        syn::Item::Macro(item) => &item.attrs,
        syn::Item::Mod(item) => &item.attrs,
        syn::Item::Static(item) => &item.attrs,
        syn::Item::Struct(item) => &item.attrs,
        syn::Item::Trait(item) => &item.attrs,
        syn::Item::TraitAlias(item) => &item.attrs,
        syn::Item::Type(item) => &item.attrs,
        syn::Item::Union(item) => &item.attrs,
        syn::Item::Use(item) => &item.attrs,
        syn::Item::Verbatim(_) => &[],
        _ => &[],
    }
}

fn impl_item_attrs(item: &syn::ImplItem) -> &[syn::Attribute] {
    match item {
        syn::ImplItem::Const(item) => &item.attrs,
        syn::ImplItem::Fn(item) => &item.attrs,
        syn::ImplItem::Type(item) => &item.attrs,
        syn::ImplItem::Macro(item) => &item.attrs,
        syn::ImplItem::Verbatim(_) => &[],
        _ => &[],
    }
}

fn is_test_cfg(attribute: &syn::Attribute) -> bool {
    attribute.path().is_ident("cfg")
        && attribute
            .meta
            .require_list()
            .is_ok_and(|list| list.tokens.to_string() == "test")
}

fn item_name(item: &syn::Item) -> String {
    match item {
        syn::Item::Const(item) => format!("const {}", item.ident),
        syn::Item::Enum(item) => format!("enum {}", item.ident),
        syn::Item::ExternCrate(item) => format!("extern crate {}", item.ident),
        syn::Item::Fn(item) => format!("fn {}", item.sig.ident),
        syn::Item::Impl(_) => "impl".into(),
        syn::Item::Mod(item) => format!("mod {}", item.ident),
        syn::Item::Static(item) => format!("static {}", item.ident),
        syn::Item::Struct(item) => format!("struct {}", item.ident),
        syn::Item::Trait(item) => format!("trait {}", item.ident),
        syn::Item::TraitAlias(item) => format!("trait alias {}", item.ident),
        syn::Item::Type(item) => format!("type {}", item.ident),
        syn::Item::Union(item) => format!("union {}", item.ident),
        syn::Item::Use(_) => "use".into(),
        _ => "item".into(),
    }
}

fn impl_item_name(item: &syn::ImplItem, impl_type: Option<&str>) -> String {
    let owner = impl_type.map_or_else(String::new, |name| format!("{name}::"));
    match item {
        syn::ImplItem::Const(item) => format!("{owner}associated const {}", item.ident),
        syn::ImplItem::Fn(item) => format!("{owner}method {}", item.sig.ident),
        syn::ImplItem::Type(item) => format!("{owner}associated type {}", item.ident),
        syn::ImplItem::Macro(_) => format!("{owner}impl macro"),
        _ => format!("{owner}impl item"),
    }
}

fn impl_type_name(ty: &syn::Type) -> Option<String> {
    let syn::Type::Path(path) = ty else {
        return None;
    };
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn use_tree_starts_with_norad(tree: &syn::UseTree) -> bool {
    match tree {
        syn::UseTree::Name(name) => name.ident == "norad",
        syn::UseTree::Rename(rename) => rename.ident == "norad",
        syn::UseTree::Path(path) => path.ident == "norad",
        syn::UseTree::Group(group) => group.items.iter().any(use_tree_starts_with_norad),
        syn::UseTree::Glob(_) => false,
    }
}

fn rust_files(directory: &Path, output: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(directory).expect("source directory") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            rust_files(&path, output);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            output.push(path);
        }
    }
}

fn production_norad_items(source: &str) -> Vec<String> {
    let syntax = syn::parse_file(source).expect("valid Rust source");
    if syntax.attrs.iter().any(is_test_cfg) {
        return Vec::new();
    }
    let mut visitor = NoradUse::default();
    visitor.visit_file(&syntax);
    visitor.items
}

#[derive(Default)]
struct ForbiddenCompatibilityUse {
    items: Vec<String>,
    current: String,
    current_impl: Option<String>,
}

impl<'ast> syn::visit::Visit<'ast> for ForbiddenCompatibilityUse {
    fn visit_item(&mut self, item: &'ast syn::Item) {
        if item_attrs(item).iter().any(is_test_cfg) {
            return;
        }
        let previous = std::mem::replace(&mut self.current, item_name(item));
        let previous_impl = self.current_impl.take();
        if let syn::Item::Impl(item) = item {
            self.current_impl = impl_type_name(&item.self_ty);
        }
        syn::visit::visit_item(self, item);
        self.current_impl = previous_impl;
        self.current = previous;
    }

    fn visit_impl_item(&mut self, item: &'ast syn::ImplItem) {
        if impl_item_attrs(item).iter().any(is_test_cfg) {
            return;
        }
        let previous = std::mem::replace(
            &mut self.current,
            impl_item_name(item, self.current_impl.as_deref()),
        );
        syn::visit::visit_impl_item(self, item);
        self.current = previous;
    }

    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        if FORBIDDEN_COMPATIBILITY_IDENTIFIERS.contains(&ident.to_string().as_str()) {
            let occurrence = format!("{}: {ident}", self.current);
            if !self.items.contains(&occurrence) {
                self.items.push(occurrence);
            }
        }
        syn::visit::visit_ident(self, ident);
    }
}

fn production_forbidden_compatibility_items(source: &str) -> Vec<String> {
    let syntax = syn::parse_file(source).expect("valid Rust source");
    if syntax.attrs.iter().any(is_test_cfg) {
        return Vec::new();
    }
    let mut visitor = ForbiddenCompatibilityUse::default();
    visitor.visit_file(&syntax);
    visitor.items
}

#[test]
fn production_norad_is_confined_to_reviewed_boundaries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    files.sort();
    let mut violations = Vec::new();
    for path in files {
        let relative = path.strip_prefix(&root).unwrap().to_string_lossy();
        if ALLOWED_BOUNDARIES.contains(&relative.as_ref()) {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("Rust source");
        for item in production_norad_items(&source) {
            if ALLOWED_ITEMS.contains(&(relative.as_ref(), item.as_str())) {
                continue;
            }
            violations.push(format!("{relative}: {item}"));
        }
    }
    assert!(
        violations.is_empty(),
        "production Norad dependency outside reviewed codec boundaries:\n{}",
        violations.join("\n")
    );
}

#[test]
fn cfg_filter_only_excludes_exact_test_items() {
    assert!(production_norad_items("#[cfg(test)] fn fixture(_: norad::Glyph) {}").is_empty());
    assert_eq!(
        production_norad_items("#[cfg(not(test))] fn production(_: norad::Glyph) {}"),
        ["fn production"]
    );
    assert_eq!(
        production_norad_items(
            "#[cfg(any(test, feature = \"fixture\"))] fn mixed(_: norad::Glyph) {}",
        ),
        ["fn mixed"]
    );
    assert_eq!(
        production_norad_items(
            "#[cfg(test)] mod tests { fn fixture(_: norad::Glyph) {} }\n\
             fn production_after(_: norad::Glyph) {}",
        ),
        ["fn production_after"]
    );
}

#[test]
fn gate_detects_aliases_fields_and_retired_round_trip_calls() {
    assert_eq!(
        production_norad_items("use norad as ufo; struct Hidden { glyph: ufo::Glyph }"),
        ["use"]
    );
    assert_eq!(
        production_norad_items("struct Hidden { glyph: norad::Glyph }"),
        ["struct Hidden"]
    );
    assert_eq!(
        production_forbidden_compatibility_items(
            "fn hidden(project: &Project) { let _ = project.source_snapshot(); }",
        ),
        ["fn hidden: source_snapshot"]
    );
}

#[test]
fn retired_mutable_compatibility_entrypoints_cannot_return() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    files.sort();
    let mut violations = Vec::new();
    for path in files {
        let relative = path.strip_prefix(&root).unwrap().to_string_lossy();
        let source = std::fs::read_to_string(&path).expect("Rust source");
        for item in production_forbidden_compatibility_items(&source) {
            violations.push(format!("{relative}: {item}"));
        }
    }
    assert!(
        violations.is_empty(),
        "retired mutable compatibility API returned:\n{}",
        violations.join("\n")
    );
}

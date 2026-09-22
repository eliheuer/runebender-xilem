// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Compiler feature includes resolve from the loaded source, not the process directory.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use norad::{Font, Glyph};
use runebender::font::persistence::memory::designspace_from_str;
use runebender::font::project::{Project, SourceInput};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    project: Project,
    root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!(
        "runebender-compiler-include-{}-{}",
        std::process::id(),
        NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (filename, bold) in [("Regular.ufo", false), ("Bold.ufo", true)] {
        let mut font = Font::new();
        font.font_info.family_name = Some("Include Fixture".into());
        font.font_info.style_name = Some(if bold { "Bold" } else { "Regular" }.into());
        font.font_info.units_per_em = Some(1000_u32.into());
        for (name, codepoint) in [(".notdef", None), ("A", Some('A')), ("V", Some('V'))] {
            let mut glyph = Glyph::new(name);
            glyph.width = if bold { 800.0 } else { 500.0 };
            if let Some(codepoint) = codepoint {
                glyph.codepoints.insert(codepoint);
            }
            font.default_layer_mut().insert_glyph(glyph);
        }
        font.features = "include(../shared.fea);".into();
        font.save(root.join(filename)).unwrap();
    }
    std::fs::write(
        root.join("shared.fea"),
        "feature kern { pos A V -50; } kern;",
    )
    .unwrap();
    let document = designspace_from_str(
        r#"<designspace format="5.0">
          <axes><axis tag="wght" name="Weight" minimum="400" default="400" maximum="900"/></axes>
          <sources>
            <source filename="Regular.ufo" name="regular"><location><dimension name="Weight" xvalue="400"/></location></source>
            <source filename="Bold.ufo" name="bold"><location><dimension name="Weight" xvalue="900"/></location></source>
          </sources>
        </designspace>"#,
    )
    .unwrap();
    let project = Project::from_designspace(document, |filename| {
        SourceInput::load(&root.join(filename)).map_err(|error| error.to_string())
    })
    .unwrap();
    Fixture { project, root }
}

#[test]
fn snapshot_keeps_the_resolved_designspace_source_path() {
    let fixture = fixture();
    assert_eq!(
        fixture.project.babelfont_snapshot().unwrap().source,
        Some(fixture.root.join("Regular.ufo/features.fea"))
    );
}

#[test]
fn canonical_compilation_resolves_a_relative_feature_include() {
    let fixture = fixture();
    fixture.project.compile().unwrap();
}

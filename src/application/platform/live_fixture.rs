// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Synthetic unsaved application fixture using the shared headless live host.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::application::font_model::FontModel;
use crate::application::workspace::Workspace;
use runebender::document::project::Project;

fn workspace() -> Result<Workspace, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?;
    let path = std::env::temp_dir().join(format!(
        "runebender-unsaved-fixture-{}-{}.ufo",
        std::process::id(),
        nonce.as_nanos()
    ));
    let mut project = Project::new_font(path);
    project
        .add_document_glyph("A", 400.0, Some(u32::from('A')))
        .map_err(|e| e.to_string())?;
    let source = project.source_id(0).ok_or("fixture source missing")?;
    let layer = project
        .document_source(source)
        .ok_or("fixture source missing")?
        .default_layer();
    project
        .edit_document_layer("A", &layer, |draft| {
            draft.add_shape_contour(kurbo::Rect::new(40.0, 0.0, 360.0, 700.0), false)?;
            draft.add_anchor("top".into(), kurbo::Point::new(200.0, 720.0))?;
            draft.set_width(412.0)?;
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    let mut app = Workspace::from_model(FontModel::from_project(project))?;
    app.open_glyph(app.font.index_of("A").ok_or("fixture glyph missing")?);
    Ok(app)
}

/// Serve a synthetic unsaved Workspace until shutdown, stdin EOF, or the bounded deadline.
pub(crate) fn serve(duration: Duration) -> Result<(), String> {
    super::live_host::serve_workspace(
        workspace()?,
        "A",
        duration,
        json!({"fixture":true, "fixture_version":1,
            "initial_advance":400.0, "unsaved_advance":412.0}),
    )
}

// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Disposable application-backed IPC fixture with a separate bounded test-control channel.
//!
//! This host constructs the real Workspace and dispatches the same live calls as the native
//! mailbox pump. It deliberately opens no window and accepts no font path or save command.

use std::io::BufRead as _;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::application::font_model::FontModel;
use crate::application::workspace::Workspace;
use runebender::document::history::HistoryDirection;
use runebender::document::project::Project;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Control {
    State,
    Undo,
    Redo,
    Shutdown,
}

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

fn state(app: &Workspace) -> Result<Value, String> {
    let address = app
        .font
        .active_layer_address("A")
        .ok_or("fixture layer missing")?;
    let layer = app
        .font
        .project
        .document_layer("A", &address.layer)
        .ok_or("fixture glyph missing")?;
    let index = app.font.index_of("A").ok_or("fixture cache missing")?;
    Ok(json!({
        "ok":true, "fixture":true, "glyph":"A", "source_id":address.layer.source.0,
        "document_revision":app.font.project.document_revision(),
        "canonical_advance":layer.width(), "cache_advance":app.font.glyphs[index].advance,
        "session_advance":app.session.advance(),
        "undo_depth":app.font.project.document_layer_history_depth(&address, HistoryDirection::Undo),
        "redo_depth":app.font.project.document_layer_history_depth(&address, HistoryDirection::Redo),
        "modified":app.modified,
        "source_exists":app.font.project.export_source.as_ref().is_some_and(|path| path.exists()),
    }))
}

fn write(output: &mut impl std::io::Write, value: &Value) -> Result<(), String> {
    writeln!(output, "{value}")
        .and_then(|()| output.flush())
        .map_err(|e| e.to_string())
}

/// Serve a synthetic unsaved Workspace until shutdown, stdin EOF, or the bounded deadline.
pub(crate) fn serve(duration: Duration) -> Result<(), String> {
    let mut app = workspace()?;
    let server = app
        .live
        .as_ref()
        .ok_or("fixture live endpoint unavailable")?;
    let mut output = std::io::stdout().lock();
    write(
        &mut output,
        &json!({
            "ok":true, "fixture":true, "fixture_version":1,
            "session":server.path(), "glyph":"A", "source_id":0,
            "initial_advance":400.0, "unsaved_advance":412.0,
            "control_actions":["state","undo","redo","shutdown"],
            "duration_seconds":duration.as_secs(),
        }),
    )?;
    let (sender, receiver) = mpsc::sync_channel(4);
    std::thread::spawn(move || {
        let mut input = std::io::stdin().lock();
        loop {
            let mut line = String::new();
            let result = match std::io::Read::take(&mut input, 1025).read_line(&mut line) {
                Ok(0) => break,
                Ok(_) if line.len() <= 1024 && line.ends_with('\n') => {
                    serde_json::from_str::<Control>(&line).map_err(|e| e.to_string())
                }
                _ => Err("invalid or oversized fixture control frame (limit 1024 bytes)".into()),
            };
            if sender.send(result).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        match receiver.try_recv() {
            Ok(Ok(Control::Shutdown)) | Err(mpsc::TryRecvError::Disconnected) => break,
            Ok(Ok(control)) => {
                match control {
                    Control::Undo => app.undo_active_edit(false),
                    Control::Redo => app.undo_active_edit(true),
                    Control::State | Control::Shutdown => {}
                }
                write(&mut output, &state(&app)?)?;
            }
            Ok(Err(error)) => write(&mut output, &json!({"ok":false,"error":error}))?,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        if let Some(request) = app.live.as_ref().and_then(|server| server.try_recv()) {
            request.respond(|call| app.call_live(call));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    write(
        &mut output,
        &json!({"ok":true,"fixture":true,"stopped":true}),
    )
}

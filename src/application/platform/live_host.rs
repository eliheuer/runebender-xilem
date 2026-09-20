// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded headless host for the same unsaved Workspace, live calls and history as the editor.
//!
//! Loading a real font does not grant a save capability. Edits are discarded when this host exits.

use std::io::BufRead as _;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::application::font_model::FontModel;
use crate::application::workspace::Workspace;
use runebender::document::history::HistoryDirection;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Control {
    State,
    Undo,
    Redo,
    Shutdown,
}

/// Open a file-backed font in memory without a native window, source watching or saving.
pub(crate) fn serve_font(path: &Path, glyph: &str, duration: Duration) -> Result<(), String> {
    let path = path.canonicalize().map_err(|error| error.to_string())?;
    let mut app = Workspace::from_model(FontModel::open(&path)?)?;
    let index = app
        .font
        .index_of(glyph)
        .ok_or_else(|| format!("glyph not found: {glyph}"))?;
    app.open_glyph(index);
    serve_workspace(
        app,
        glyph,
        duration,
        json!({"fixture":false,"font_path":path}),
    )
}

fn state(app: &Workspace, glyph: &str) -> Result<Value, String> {
    let address = app
        .font
        .active_layer_address(glyph)
        .ok_or("selected layer missing")?;
    let layer = app
        .font
        .project
        .document_layer(glyph, &address.layer)
        .ok_or("selected glyph missing")?;
    let index = app.font.index_of(glyph).ok_or("selected cache missing")?;
    Ok(json!({
        "ok":true, "host":"headless_workspace", "glyph":glyph, "source_id":address.layer.source.0,
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

/// Pump the live mailbox while stdin supplies only state, ordinary undo/redo and shutdown.
pub(crate) fn serve_workspace(
    mut app: Workspace,
    glyph: &str,
    duration: Duration,
    mut ready: Value,
) -> Result<(), String> {
    let server = app
        .live
        .as_ref()
        .ok_or("headless live endpoint unavailable")?;
    ready["ok"] = json!(true);
    ready["host"] = json!("headless_workspace");
    ready["session"] = json!(server.path());
    ready["document_epoch"] = json!(server.document_epoch());
    ready["glyph"] = json!(glyph);
    ready["source_id"] = state(&app, glyph)?["source_id"].clone();
    ready["control_actions"] = json!(["state", "undo", "redo", "shutdown"]);
    ready["duration_seconds"] = json!(duration.as_secs());
    let mut output = std::io::stdout().lock();
    write(&mut output, &ready)?;
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
                write(&mut output, &state(&app, glyph)?)?;
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
        &json!({"ok":true,"host":"headless_workspace","stopped":true}),
    )
}

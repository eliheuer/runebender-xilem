// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Local Chat transcript, model selection, prompt, and process controls.

use crate::edit::chat::ChatEntry;
use crate::*;

pub(crate) fn chat_panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let selected = app.chat.model.clone();
    let models: Vec<_> = app
        .chat
        .installed
        .iter()
        .cloned()
        .map(|(name, path)| {
            let active = selected.as_deref() == Some(path.as_path());
            recipes::toggle(pal, name, active, move |app: &mut Workspace| {
                if app.chat.job.is_none() {
                    app.chat.model = Some(path.clone());
                }
            })
        })
        .collect();
    let no_model = app.chat.installed.is_empty().then(|| {
        label("No local chat model. Add a GGUF folder with tokenizer.json under the model root.")
            .color(pal.text_muted)
    });

    let transcript: Vec<_> = app
        .chat
        .entries
        .iter()
        .map(|entry| {
            let (text, color) = match entry {
                ChatEntry::User(text) => (format!("You: {text}"), pal.text),
                ChatEntry::Assistant(text) => (
                    if text.is_empty() {
                        "Assistant: …".into()
                    } else {
                        format!("Assistant: {text}")
                    },
                    pal.text,
                ),
                ChatEntry::Tool { name, ok, note } => (
                    format!("[{}] {name}: {note}", if *ok { "ok" } else { "error" }),
                    pal.text_muted,
                ),
                ChatEntry::Error(text) => (format!("Error: {text}"), pal.text),
            };
            label(text).color(color).boxed()
        })
        .collect();
    let transcript = portal(xcolumn(Region::List, transcript)).constrain_horizontal(true);

    let status = app
        .chat
        .busy
        .clone()
        .or_else(|| app.chat.last_speed.clone())
        .map(|text| label(text).color(pal.text_muted));
    let prompt = recipes::field_bare(
        pal,
        "Ask about the open font",
        app.chat.prompt.clone(),
        |app, value| app.chat.prompt = value,
        |app, value| {
            app.chat.prompt.clear();
            app.chat_send(value);
        },
    );
    let busy = app.chat.job.is_some();
    let send = recipes::toggle(
        pal,
        if busy { "Working…" } else { "Send" }.into(),
        !busy,
        |app| {
            if app.chat.job.is_none() {
                let prompt = std::mem::take(&mut app.chat.prompt);
                app.chat_send(prompt);
            }
        },
    );
    let cancel = busy.then(|| {
        recipes::toggle(pal, "Cancel".into(), false, |app: &mut Workspace| {
            app.chat_cancel();
        })
    });
    let clear = (!busy && !app.chat.entries.is_empty()).then(|| {
        recipes::toggle(pal, "Clear".into(), false, |app: &mut Workspace| {
            app.chat_clear();
        })
    });

    xcolumn(
        Region::Panel,
        (
            label("Local chat model").color(pal.text_muted),
            xcolumn(Region::List, models),
            no_model,
            transcript.flex(1.0),
            status,
            prompt,
            xrow(Region::Inline, (send, cancel, clear)),
        ),
    )
    .background_color(pal.panel)
}

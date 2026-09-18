// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Local Chat transcript, model selection, prompt, and process controls.

use crate::edit::chat::ChatEntry;
use crate::widgets::selectable_text::selectable_text;
use crate::{
    Dim, Dimensions, Radius, Region, Space, Stroke, Style, TextSize, WidgetView, Workspace, label,
    portal, recipes, sized_box, xcolumn, xrow,
};
use xilem::Color;
use xilem::view::FlexExt as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TranscriptKind {
    User,
    Assistant,
    Tool,
    Error,
}

fn transcript_text(entry: &ChatEntry) -> (String, TranscriptKind) {
    match entry {
        ChatEntry::User(text) => (text.clone(), TranscriptKind::User),
        ChatEntry::Assistant(text) => (
            if text.is_empty() {
                "…".into()
            } else {
                text.clone()
            },
            TranscriptKind::Assistant,
        ),
        ChatEntry::Tool { name, ok, note } => (
            format!("[{}] {name}: {note}", if *ok { "ok" } else { "error" }),
            TranscriptKind::Tool,
        ),
        ChatEntry::Error(text) => (format!("Error: {text}"), TranscriptKind::Error),
    }
}

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
            let (text, kind) = transcript_text(entry);
            let (color, background, border, radius, padding) = match kind {
                TranscriptKind::User => (
                    pal.selected_ink(),
                    pal.selected_bg(),
                    Color::TRANSPARENT,
                    Radius::Sm,
                    masonry::properties::Padding {
                        left: Space::Md.length(),
                        right: Space::Md.length(),
                        top: Space::Sm.length(),
                        bottom: Space::Sm.length(),
                    },
                ),
                TranscriptKind::Assistant => (
                    pal.text,
                    Color::TRANSPARENT,
                    Color::TRANSPARENT,
                    Radius::None,
                    Space::Sm.into(),
                ),
                TranscriptKind::Tool => (
                    pal.text_muted,
                    pal.control,
                    pal.field_outline,
                    Radius::Sm,
                    masonry::properties::Padding {
                        left: Space::Md.length(),
                        right: Space::Md.length(),
                        top: Space::Xs.length(),
                        bottom: Space::Xs.length(),
                    },
                ),
                TranscriptKind::Error => (
                    pal.role("danger"),
                    Color::TRANSPARENT,
                    Color::TRANSPARENT,
                    Radius::None,
                    Space::Sm.into(),
                ),
            };
            sized_box(
                selectable_text::<Workspace, ()>(text)
                    .color(color)
                    .text_size(TextSize::Body.px()),
            )
            .padding(padding)
            .background_color(background)
            .border_color(border)
            .border_width(if border == Color::TRANSPARENT {
                Stroke::None.length()
            } else {
                Stroke::Hairline.length()
            })
            .corner_radius(radius.length())
            .dims(Dimensions::new(Dim::Stretch, Dim::Auto))
        })
        .collect();
    let transcript =
        portal(xcolumn(Region::List, transcript).gap(Space::Sm)).constrain_horizontal(true);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_roles_do_not_repeat_speaker_labels() {
        assert_eq!(
            transcript_text(&ChatEntry::User("What is the UPM?".into())),
            ("What is the UPM?".into(), TranscriptKind::User)
        );
        assert_eq!(
            transcript_text(&ChatEntry::Assistant(String::new())),
            ("…".into(), TranscriptKind::Assistant)
        );
        assert_eq!(
            transcript_text(&ChatEntry::Tool {
                name: "project_info".into(),
                ok: true,
                note: "done".into(),
            }),
            ("[ok] project_info: done".into(), TranscriptKind::Tool)
        );
    }
}

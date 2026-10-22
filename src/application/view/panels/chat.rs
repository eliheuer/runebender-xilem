// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Local Chat transcript, model selection, prompt, and process controls.

use crate::application::design::controls::icon_button::icon_button;
use crate::application::design::recipes;
use crate::application::design::{
    ControlKind, ControlSize, ControlStyle, Region, Space, Stroke, TextSize, column as xcolumn,
    row as xrow,
};
use crate::application::editor::tools::chat::ChatEntry;
use crate::application::editor::tools::scripts::ScriptArtifact;
use crate::application::view::label;
use crate::application::widgets::selectable_text::selectable_text;
use crate::application::widgets::tail_list::tail_list;
use crate::application::workspace::Workspace;
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::{FlexSpacer, sized_box};

fn script_artifact_row(
    app: &Workspace,
    artifact: &ScriptArtifact,
) -> Box<xilem::AnyWidgetView<Workspace>> {
    let pal = &app.palette;
    let artifact = artifact.clone();
    xcolumn(
        Region::List,
        (
            label(format!("Python · {}", artifact.name)).color(pal.text),
            label("Code artifact — opening does not save or run it.")
                .text_size(TextSize::Caption.px())
                .color(pal.text_muted),
            recipes::toggle(pal, "Open".into(), false, move |app: &mut Workspace| {
                app.open_script_artifact(artifact.clone());
            }),
        ),
    )
    .padding(Space::Sm)
    .background_color(pal.control)
    .border_color(pal.field_outline)
    .border_width(Stroke::Hairline.length())
    .corner_radius(Length::px(pal.corner_radius))
    .boxed()
}

/// A tool call as a sentence a designer can read, not a log line.
fn tool_sentence(name: &str, ok: bool, note: &str) -> String {
    let running = note == "…";
    let verb = match (name, running) {
        ("font_info" | "project_info", false) => "Read the font info",
        ("font_info" | "project_info", true) => "Reading the font info",
        ("editor_context", false) => "Looked at the editor",
        ("editor_context", true) => "Looking at the editor",
        ("glyph_inventory", false) => "Listed the glyphs",
        ("glyph_inventory", true) => "Listing the glyphs",
        ("analyze_glyph", false) => "Measured a glyph",
        ("analyze_glyph", true) => "Measuring a glyph",
        ("analyze_font", false) => "Measured the font",
        ("analyze_font", true) => "Measuring the font",
        ("read_glyph", false) => "Read a glyph",
        ("read_glyph", true) => "Reading a glyph",
        ("read_kerning", false) => "Read the kerning",
        ("read_kerning", true) => "Reading the kerning",
        ("proof" | "specimen", false) => "Prepared a proof",
        ("proof" | "specimen", true) => "Preparing a proof",
        ("proposal_list", false) => "Listed the proposals",
        ("proposal_list", true) => "Listing the proposals",
        ("propose_edits", false) => "Proposed edits for review",
        ("propose_edits", true) => "Proposing edits",
        (_, false) => "Used a tool",
        (_, true) => "Using a tool",
    };
    if running {
        format!("{verb}…")
    } else if !ok {
        format!("{verb}, which failed: {note}")
    } else if name == "read_glyph" || name == "propose_edits" {
        format!("{verb}: {note}")
    } else {
        verb.to_string()
    }
}

/// A word on a control: the base button with a kind, at control height.
/// A disabled one shows it cannot act and does not answer the pointer.
fn word_button<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &crate::application::design::theme::Palette,
    kind: ControlKind,
    text: &'static str,
    enabled: bool,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let style = ControlStyle::new(pal, kind).disabled(pal, !enabled);
    sized_box(
        recipes::button(
            style,
            label(text)
                .text_size(TextSize::Body.px())
                .color(style.ink()),
            on_click,
        )
        .padding(masonry::properties::Padding::horizontal(Space::Md.length())),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Swatch)))
}

/// One entry of the conversation, or nothing for a tool call: a tool that
/// worked shows in the answer, and one that refused goes back to the model,
/// which says what it could not do. Only a turn that failed is a problem.
fn transcript_row(
    app: &Workspace,
    index: usize,
    entry: &ChatEntry,
) -> Option<Box<xilem::AnyWidgetView<Workspace>>> {
    use recipes::Message;
    let pal = &app.palette;
    let card = |kind: Message, text: String| {
        recipes::message_card(
            pal,
            kind,
            selectable_text::<Workspace, ()>(text).color(kind.ink(pal)),
        )
        .boxed()
    };
    Some(match entry {
        ChatEntry::Script(artifact) => script_artifact_row(app, artifact),
        ChatEntry::User(text) => card(Message::Question, text.clone()),
        // The answer card appears with the first words, not before.
        ChatEntry::Assistant(text) if text.trim().is_empty() => return None,
        ChatEntry::Assistant(text) => card(Message::Answer, text.clone()),
        // The detail behind an answer: a word under the card opens it as a
        // note and closes it again.
        ChatEntry::Detail { text, open } => {
            let open = *open;
            let toggle = word_button(
                pal,
                ControlKind::Quiet,
                if open { "Less" } else { "Detail" },
                true,
                move |app: &mut Workspace| {
                    if let Some(ChatEntry::Detail { open, .. }) = app.chat.entries.get_mut(index) {
                        *open = !*open;
                    }
                },
            );
            xcolumn(
                Region::List,
                (
                    open.then(|| card(Message::Note, text.clone())),
                    xrow(Region::Inline, (FlexSpacer::Flex(1.0), toggle)),
                ),
            )
            .gap(Space::Sm)
            .boxed()
        }
        // The editor's own account of a proposal, beside whatever the model says.
        ChatEntry::Tool {
            name,
            ok: true,
            note,
        } if name == "propose_edits" => card(Message::Note, note.clone()),
        ChatEntry::Tool { .. } => return None,
        ChatEntry::Error(text) => {
            match crate::application::editor::tools::chat::plain_error(text) {
                None => card(Message::Problem, text.clone()),
                // The plain sentence, and the original in small type under it.
                Some(plain) => recipes::message_card(
                    pal,
                    Message::Problem,
                    xcolumn(
                        Region::List,
                        (
                            selectable_text::<Workspace, ()>(plain)
                                .color(Message::Problem.ink(pal)),
                            selectable_text::<Workspace, ()>(text.clone())
                                .color(pal.placeholder_ink())
                                .text_size(TextSize::Caption.px()),
                        ),
                    )
                    .gap(Space::Sm),
                )
                .boxed(),
            }
        }
    })
}

/// One step of space sets the whole panel's rhythm: the inset, the gap
/// between paragraphs, and the gaps around the field and its controls.
const RHYTHM: Space = Space::Lg;

pub(crate) fn chat_panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let busy = app.chat.job.is_some();
    let external = app.chat.endpoint.is_some();
    let empty = app.chat.entries.is_empty() && app.chat.streaming_artifacts.is_empty();

    // The conversation, newest at the bottom, or the note on how to start.
    let conversation: Box<xilem::AnyWidgetView<Workspace>> = if empty {
        let note = match &app.chat.endpoint {
            Some(Err(error)) => format!("The local model server is not set up: {error}"),
            Some(Ok(_)) => "Ask about the open font below. The model can measure \
                            glyphs, read kerning, and propose edits for you to review."
                .into(),
            None if app.chat.installed.is_empty() => {
                "No local chat model is installed. Add a GGUF model folder under \
                 the model root, then come back here."
                    .into()
            }
            None => "Ask about the open font below. The model runs on this machine \
                     and can measure glyphs, read kerning, and propose edits for you \
                     to review. Try: \"How does the stem of H compare with the other \
                     capitals?\""
                .into(),
        };
        xcolumn(
            Region::Panel,
            (
                FlexSpacer::Flex(1.0),
                recipes::prose_muted(pal, note),
                FlexSpacer::Flex(1.0),
            ),
        )
        .boxed()
    } else {
        let mut rows: Vec<Box<xilem::AnyWidgetView<Workspace>>> = app
            .chat
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| transcript_row(app, index, entry))
            .collect();
        rows.extend(
            app.chat
                .streaming_artifacts
                .iter()
                .map(|artifact| script_artifact_row(app, artifact)),
        );
        // What the model is doing, as the last card of the conversation while
        // it works: the drifting wash says something is happening.
        if let Some(text) = app.chat.busy.as_deref() {
            let text = text
                .strip_prefix("Running ")
                .and_then(|rest| rest.strip_suffix('…'))
                .map_or_else(|| text.to_string(), |name| tool_sentence(name, false, "…"));
            rows.push(recipes::working_card(pal, text).boxed());
        }

        // The conversation grows up from the field, and a card the panel
        // cannot show whole is not shown at all.
        tail_list(rows, RHYTHM.px()).boxed()
    };

    // The hint is for the first question; after that the field is blank.
    let field = recipes::field_growing(
        pal,
        if empty { "Ask about the open font" } else { "" },
        app.chat.prompt.clone(),
        |app, value| app.chat.prompt = value,
        // Return while the model works keeps the text for the next turn,
        // and a blank field sends nothing.
        |app, value| {
            if app.chat.job.is_none() && !value.trim().is_empty() {
                app.chat.prompt.clear();
                app.chat_send(value);
            }
        },
        |app| {
            if app.chat.job.is_some() {
                app.chat_cancel();
            }
        },
    );

    // The installed models as a list, open while the person is choosing:
    // one row each, the chosen one inverted, in place of the chip that
    // opened it. Picking a row, the chosen one included, closes it.
    let selected = app.chat.model.clone();
    let model_name = app
        .chat
        .installed
        .iter()
        .find(|(_, path)| selected.as_deref() == Some(path.as_path()))
        .map_or("No model", |(name, _)| name.as_str())
        .to_string();
    let picking = app.chat.picking && !external;
    let picker = picking.then(|| {
        let names = app
            .chat
            .installed
            .iter()
            .map(|(name, _)| name.clone())
            .collect();
        let chosen = app
            .chat
            .installed
            .iter()
            .position(|(_, path)| selected.as_deref() == Some(path.as_path()));
        recipes::picker(
            pal,
            names,
            chosen,
            std::sync::Arc::new(|app: &mut Workspace, i: usize| {
                if app.chat.job.is_none()
                    && let Some((_, path)) = app.chat.installed.get(i)
                {
                    app.chat.model = Some(path.clone());
                }
                app.chat.picking = false;
            }),
        )
    });

    // The controls under the field: the model on the left, actions on the right.
    let model_chip = (!external && !picking).then(|| {
        recipes::chip(pal, model_name, false, |app: &mut Workspace| {
            app.chat.picking = true;
        })
    });
    let endpoint_note = external.then(|| {
        label(match &app.chat.endpoint {
            Some(Ok(endpoint)) => endpoint.model().to_string(),
            _ => "Local server".to_string(),
        })
        .text_size(TextSize::Caption.px())
        .color(pal.text_muted)
    });
    let listening = app.voice.capture.is_some();
    let flowing = app.voice.capture.as_ref().is_some_and(|c| c.started);
    let transcribing = !listening && app.voice.job.is_some();
    // Clear is the quiet cross: the row has no room for a word beside the
    // model's name and the two voice controls.
    let clear = (!busy && !empty && !listening && !transcribing).then(|| {
        icon_button(
            ControlStyle::new(pal, ControlKind::Quiet),
            "close",
            "Clear",
            |app: &mut Workspace| {
                app.chat_clear();
            },
        )
        .icon_size(ControlSize::Icon.px() * 0.5)
        .tile_size(ControlSize::Icon.px())
    });
    // The microphone and the arrow, as in a messaging app: the mic opens
    // the microphone and turns into the sound bars while it listens, which
    // stop it; the arrow sends once there is text, and is the stop square
    // while the model answers.
    let mic = (!busy && app.voice_available()).then(|| {
        let style = ControlStyle::new(pal, ControlKind::Tool)
            .selected(pal, listening)
            .disabled(pal, listening && !flowing || transcribing);
        let icon = if listening || transcribing {
            "listening"
        } else {
            "mic"
        };
        let label = if listening { "Stop listening" } else { "Talk" };
        icon_button(style, icon, label, |app: &mut Workspace| {
            if app.voice.capture.is_some() {
                app.voice_stop();
            } else if app.voice.job.is_none() {
                app.voice_start();
            }
        })
        .tile_size(ControlSize::Icon.px())
    });
    let send = if busy {
        icon_button(
            ControlStyle::new(pal, ControlKind::Primary),
            "stop",
            "Stop",
            |app: &mut Workspace| app.chat_cancel(),
        )
        .tile_size(ControlSize::Icon.px())
        .boxed()
    } else {
        let ready = !app.chat.prompt.trim().is_empty() && !listening && !transcribing;
        icon_button(
            ControlStyle::new(pal, ControlKind::Primary).disabled(pal, !ready),
            "send",
            "Send",
            |app: &mut Workspace| {
                if app.chat.job.is_none()
                    && app.voice.capture.is_none()
                    && app.voice.job.is_none()
                    && !app.chat.prompt.trim().is_empty()
                {
                    let prompt = std::mem::take(&mut app.chat.prompt);
                    app.chat_send(prompt);
                }
            },
        )
        .tile_size(ControlSize::Icon.px())
        .boxed()
    };
    let controls = xrow(
        Region::Inline,
        (
            model_chip,
            endpoint_note,
            FlexSpacer::Flex(1.0),
            clear,
            mic,
            send,
        ),
    );

    xcolumn(
        Region::Panel,
        (conversation.flex(1.0), field, picker, controls),
    )
    .gap(RHYTHM)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_calls_read_as_sentences() {
        assert_eq!(
            tool_sentence("font_info", true, "done"),
            "Read the font info"
        );
        assert_eq!(
            tool_sentence("analyze_glyph", false, "…"),
            "Measuring a glyph…"
        );
        assert_eq!(
            tool_sentence("read_glyph", true, "H: advance 700, 2 contours"),
            "Read a glyph: H: advance 700, 2 contours"
        );
        assert_eq!(
            tool_sentence("analyze_font", false, "no such source"),
            "Measured the font, which failed: no such source"
        );
        assert_eq!(tool_sentence("mystery", true, "done"), "Used a tool");
    }
}

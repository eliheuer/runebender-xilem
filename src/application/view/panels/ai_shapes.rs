// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The local AI shapes tool's section of the inspector, shown while
//! the tool is active: the model, the words it knows, and training.

use crate::application::editor::tools::ai_shapes::{TrainSource, fit_path, sample_shapes_ufo};
use crate::application::view::design::{Region, TextSize, column as xcolumn, row as xrow};
use crate::application::view::{label, recipes};
use crate::application::workspace::Workspace;
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::{canvas, sized_box};

const SAMPLE_PX: f64 = 56.0;

pub(crate) fn panel(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let muted = |text: String| {
        label(text)
            .text_size(TextSize::Body.px())
            .color(pal.text_muted)
    };
    let plain = |text: String| label(text).text_size(TextSize::Body.px()).color(pal.text);
    let st = &app.shapes;

    // The models the tool can draw with, the font's own first.
    let chosen = st.model.clone();
    let models: Vec<_> = st
        .installed
        .iter()
        .map(|m| {
            let current = chosen.as_deref() == Some(m.path.as_path());
            let path = m.path.clone();
            recipes::toggle(pal, m.name.clone(), current, move |app: &mut Workspace| {
                app.choose_shapes_model(&path);
            })
        })
        .collect();
    let no_models = st
        .installed
        .is_empty()
        .then(|| muted("No shapes model yet. Train one below.".into()));

    // The words, a few per line so the rail keeps its width.
    let words: Vec<_> = if st.words.is_empty() {
        Vec::new()
    } else {
        st.words
            .chunks(4)
            .map(|line| muted(line.join("   ")))
            .collect()
    };
    let words_title = (!st.words.is_empty()).then(|| {
        muted(format!(
            "Knows {} word{}",
            st.words.len(),
            if st.words.len() == 1 { "" } else { "s" }
        ))
    });

    let note = (!st.note.is_empty()).then(|| plain(st.note.clone()));

    // Training in flight: progress, time left, what it draws right now.
    let training = st.training.as_ref().map(|t| {
        let (done, total, _) = t.progress.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let elapsed = t.started.elapsed().as_secs_f64();
        let left = if done > 0 {
            (elapsed / done as f64 * (total.saturating_sub(done)) as f64 / 60.0).ceil()
        } else {
            0.0
        };
        let samples: Vec<_> = t
            .samples
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(word, path)| {
                let fitted = fit_path(path, SAMPLE_PX, 6.0);
                let ink = pal.text;
                let ground = pal.field();
                let outline = pal.outline;
                xcolumn(
                    Region::List,
                    (
                        sized_box(canvas(move |_: &mut Workspace, _, scene, size| {
                            use masonry::imaging::Painter;
                            use masonry::kurbo::{Rect, Shape as _, Stroke};
                            let mut painter = Painter::new(scene);
                            let frame = Rect::new(0.0, 0.0, size.width, size.height);
                            painter.fill(frame.to_path(0.1), ground).draw();
                            painter.stroke(frame, &Stroke::new(1.0), outline).draw();
                            painter.fill(&fitted, ink).draw();
                        }))
                        .dims(Dimensions::new(
                            Dim::Fixed(Length::px(SAMPLE_PX)),
                            Dim::Fixed(Length::px(SAMPLE_PX)),
                        )),
                        muted(word.clone()),
                    ),
                )
            })
            .collect();
        xcolumn(
            Region::List,
            (
                plain(format!("Training {}", t.name)),
                muted(if done == 0 {
                    "Starting…".to_string()
                } else if left > 0.0 {
                    format!("Step {done} of {total}, about {left:.0} min left")
                } else {
                    format!("Step {done} of {total}")
                }),
                (!samples.is_empty()).then(|| muted("What it draws now".into())),
                xrow(Region::Inline, samples),
                recipes::action(pal, "Stop training".into(), |app: &mut Workspace| {
                    app.shapes_cancel_training();
                }),
            ),
        )
    });

    // Training controls, when nothing is running.
    let thorough = st.thorough;
    let has_sample = sample_shapes_ufo().is_some();
    let controls = st.training.is_none().then(|| {
        xcolumn(
            Region::List,
            (
                muted("Train".into()),
                xrow(
                    Region::Inline,
                    (
                        recipes::toggle(pal, "Quick".into(), !thorough, |app: &mut Workspace| {
                            app.shapes.thorough = false;
                        }),
                        recipes::toggle(pal, "Thorough".into(), thorough, |app: &mut Workspace| {
                            app.shapes.thorough = true;
                        }),
                    ),
                ),
                muted(if thorough {
                    "Clean shapes; about 30 minutes from scratch.".into()
                } else {
                    "Rough shapes; about 12 minutes from scratch.".into()
                }),
                recipes::action(pal, "Train from this font".into(), |app: &mut Workspace| {
                    app.shapes_train(TrainSource::ThisFont);
                }),
                has_sample.then(|| {
                    recipes::action(
                        pal,
                        "Train from the sample shapes".into(),
                        |app: &mut Workspace| app.shapes_train(TrainSource::SampleFont),
                    )
                }),
            ),
        )
    });

    let how = muted("Drag a box on the glyph, type a word, press Enter.".into());
    let details = st.chosen().map(|m| muted(m.path.display().to_string()));

    recipes::section(
        app,
        "Local AI shapes tool",
        "Local AI shapes tool",
        xcolumn(
            Region::List,
            (
                how,
                muted("Model".into()),
                xcolumn(Region::List, models),
                no_models,
                words_title,
                xcolumn(Region::List, words),
                note,
                training,
                controls,
                details,
            ),
        ),
    )
}

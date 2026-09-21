// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Real pointer actions and Xilem rebuilds for metaball inspector gesture boundaries.

use std::sync::Arc;

use masonry::core::{PointerButton, PointerEvent, Widget, WidgetId, WidgetRef};
use masonry::kurbo::Point;
use masonry::widgets::Slider;
use masonry_testing::{PRIMARY_MOUSE, TestHarness};
use runebender::formats::metaballs::{Metaball, MetaballGroup, Metaballs, write_metaballs};
use xilem::core::{
    DynMessage, MessageCtx, ProxyError, RawProxy, SendMessage, View, ViewId, ViewPathTracker,
};
use xilem::view::sized_box;
use xilem::{ViewCtx, WidgetView};

use crate::application::view::default_property_set;
use crate::application::workspace::{Tool, Workspace};

#[derive(Debug)]
struct NoProxy;

impl RawProxy for NoProxy {
    fn send_message(&self, _: Arc<[ViewId]>, _: SendMessage) -> Result<(), ProxyError> {
        Ok(())
    }

    fn dyn_debug(&self) -> &dyn std::fmt::Debug {
        self
    }
}

fn sliders(widget: WidgetRef<'_, dyn Widget>) -> Vec<WidgetId> {
    let mut found = Vec::new();
    if widget.downcast::<Slider>().is_some() {
        found.push(widget.id());
    }
    for child in widget.children() {
        found.extend(sliders(child));
    }
    found
}

fn dispatch<V: WidgetView<Workspace>>(
    harness: &mut TestHarness<V::Widget>,
    view: &V,
    view_state: &mut V::ViewState,
    ctx: &mut ViewCtx,
    app: &mut Workspace,
) where
    V::Widget: Sized,
{
    while let Some((action, id)) = harness.pop_action_erased() {
        let path = ctx
            .get_id_path(id)
            .expect("the action source exists")
            .clone();
        let mut message =
            MessageCtx::new(std::mem::take(ctx.environment()), path, DynMessage(action));
        harness.edit_root_widget(|root| {
            let _ = view.message(view_state, &mut message, root, app);
        });
        let (environment, _, _) = message.finish();
        *ctx.environment() = environment;
    }
}

#[cfg(test)]
fn check_slider_rebuild(field: &str, cancel: bool) {
    let path = std::env::temp_dir().join(format!(
        "runebender-metaball-panel-{field}-{}.ufo",
        std::process::id()
    ));
    let (radius, stiffness, threshold, index, fractions) = match field {
        "Strength" => (180.0, 0.25, 0.5, 3, [0.6, 0.7]),
        "Threshold" => (180.0, 1.0, 1.5, 4, [0.3, 0.2]),
        "Radius" => (5000.0, -2.0, 0.5, 2, [0.9, 0.8]),
        "Size" => (180.0, 2.0, 0.5, 2, [0.15, 0.20]),
        "Blend" => (180.0, 2.0, 0.5, 3, [0.65, 0.80]),
        _ => panic!("unexpected fixture"),
    };
    let organic = matches!(field, "Size" | "Blend");
    let mut source = Metaballs {
        version: if organic { 3 } else { 1 },
        groups: vec![MetaballGroup {
            blend: organic.then_some(0.5),
            id: 1,
            threshold,
            balls: vec![Metaball {
                id: 1,
                x: 300.0,
                y: 300.0,
                radius,
                stiffness,
            }],
            links: Vec::new(),
        }],
    };
    if organic {
        let mut other = source.groups[0].balls[0].clone();
        other.id = 2;
        other.x = 580.0;
        source.groups[0].balls.push(other);
    }
    let mut font = norad::Font::new();
    let mut glyph = norad::Glyph::new("a");
    glyph.width = 600.0;
    glyph.codepoints.insert('a');
    write_metaballs(&mut glyph, &source).unwrap();
    font.default_layer_mut().insert_glyph(glyph);
    font.save(&path).unwrap();
    let mut app = Workspace::open(&path).unwrap();
    app.palette = Arc::new(crate::application::view::theme::Palette::load("gray"));
    app.open_glyph(0);
    app.select_tool(Tool::Metaball);
    let initial_range = app.session.metaball_slider_range(field);
    let undo_depth = app.metadata_undo.len();
    let mut ctx = ViewCtx::new(
        Arc::new(NoProxy),
        Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap(),
        ),
    );
    let logic = |app: &Workspace| sized_box(super::panel(app));
    let mut view = logic(&app);
    let (pod, mut view_state) = view.build(&mut ctx, &mut app);
    let mut harness =
        TestHarness::create_with_size(default_property_set(), pod.new_widget, (400, 900));
    let initial_sliders = sliders(harness.root_widget().as_dyn());
    assert_eq!(
        initial_sliders.len(),
        if organic { 4 } else { 5 },
        "organic groups have Size and Group blend; legacy groups keep raw controls"
    );
    let slider = initial_sliders[index];
    let at_fraction = |harness: &TestHarness<_>, fraction| {
        let widget = harness.get_widget_with_id(slider);
        let bounds = widget.ctx().content_box();
        widget.ctx().window_transform()
            * Point::new(bounds.width() * fraction, bounds.height() * 0.5)
    };
    for (step, fraction) in fractions.into_iter().enumerate() {
        harness.mouse_move(at_fraction(&harness, fraction));
        if step == 0 {
            harness.mouse_button_press(Some(PointerButton::Primary));
        }
        dispatch(&mut harness, &view, &mut view_state, &mut ctx, &mut app);
        let next = logic(&app);
        ctx.reset_changed_props();
        ctx.reset_changed_transforms();
        harness.edit_root_widget(|root| {
            next.rebuild(&view, &mut view_state, &mut ctx, root, &mut app);
        });
        view = next;
        assert_eq!(
            sliders(harness.root_widget().as_dyn()),
            initial_sliders,
            "the active slider must keep its identity and field through a rebuild"
        );
        assert!(app.session.gesture_in_progress());
        assert_eq!(app.metadata_undo.len(), undo_depth);
        assert_eq!(
            app.session.metaball_slider_range(field),
            initial_range,
            "an expanded legacy range must not shrink while the pointer is down"
        );
        let current = app.session.metaball_data().unwrap();
        let group = &current.groups[0];
        if field == "Strength" {
            assert!(group.balls[0].stiffness > group.threshold);
            assert_eq!(group.balls[0].radius, radius);
        } else if field == "Threshold" {
            assert!(group.threshold < group.balls[0].stiffness);
            assert_eq!(group.balls[0].stiffness, stiffness);
        } else if field == "Blend" {
            assert_eq!(
                group.balls, source.groups[0].balls,
                "group blend preserves every circle exactly"
            );
            assert!(group.blend.unwrap() > 0.5);
        } else if field == "Size" {
            assert_eq!(group.blend, Some(0.5));
            assert_eq!(group.balls[0].stiffness, stiffness);
            assert!(group.balls[0].radius > radius);
        } else {
            assert!(group.balls[0].radius < radius);
        }
    }
    if cancel {
        harness.process_pointer_event(PointerEvent::Cancel(PRIMARY_MOUSE));
    } else {
        harness.mouse_button_release(Some(PointerButton::Primary));
    }
    dispatch(&mut harness, &view, &mut view_state, &mut ctx, &mut app);
    assert!(!app.session.gesture_in_progress());
    if cancel {
        assert_eq!(app.session.metaball_data().unwrap(), source);
        assert_eq!(app.metadata_undo.len(), undo_depth);
    } else {
        assert_eq!(app.metadata_undo.len(), undo_depth + 1);
        app.undo_open_glyph(false);
        assert_eq!(app.session.metaball_data().unwrap(), source);
    }
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn raw_strength_crosses_threshold_without_rebinding_the_captured_slider() {
    check_slider_rebuild("Strength", false);
}

#[test]
fn raw_threshold_crosses_strength_and_pointer_cancel_restores_the_source() {
    check_slider_rebuild("Threshold", true);
}

#[test]
fn legacy_radius_range_remains_stable_across_pointer_rebuilds() {
    check_slider_rebuild("Radius", false);
}

#[test]
fn organic_group_blend_drag_keeps_circle_sizes_and_commits_once() {
    check_slider_rebuild("Blend", false);
}

#[test]
fn organic_size_drag_preserves_blend_and_cancels_without_source_changes() {
    check_slider_rebuild("Size", true);
}

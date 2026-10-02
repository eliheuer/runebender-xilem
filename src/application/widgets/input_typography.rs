// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Match Masonry's separate placeholder label to our UI font and retain themed input ink.
//!
//! The pinned Xilem `TextInput` view rebuilds `ContentColor` on its outer input,
//! although Masonry reads that property from the inner `TextArea`.
//! Bridge that update here without replacing the editor or its selection and focus.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, PaintCtx, PropertiesMut, PropertiesRef,
    RegisterCtx, StyleProperty, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use masonry::peniko::Color;
use masonry::properties::ContentColor;
use masonry::widgets::{Label, TextInput};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

fn style_placeholder(mut input: WidgetMut<'_, TextInput>) {
    let mut label = TextInput::placeholder_mut(&mut input);
    Label::insert_style(
        &mut label,
        StyleProperty::FontFamily(crate::application::view::UI_FONT_FAMILY.into()),
    );
    Label::insert_style(
        &mut label,
        StyleProperty::FontSize(crate::application::view::design::TextSize::Body.px()),
    );
}

fn style_input(mut input: WidgetMut<'_, TextInput>, text_color: Color) {
    {
        let mut text = TextInput::text_mut(&mut input);
        if text.get_prop::<ContentColor>().color != text_color {
            text.insert_prop(ContentColor::new(text_color));
        }
    }
    style_placeholder(input);
}

pub(crate) struct InputTypography {
    child: WidgetPod<TextInput>,
    text_color: Color,
}
impl InputTypography {
    fn child_mut<'a>(this: &'a mut WidgetMut<'_, Self>) -> WidgetMut<'a, TextInput> {
        this.ctx.get_mut(&mut this.widget.child)
    }
}
impl Widget for InputTypography {
    type Action = ();
    fn register_children(&mut self, ctx: &mut RegisterCtx<'_>) {
        ctx.register_child(&mut self.child);
    }
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::WidgetAdded) {
            let text_color = self.text_color;
            ctx.mutate_child_later(&mut self.child, move |input| style_input(input, text_color));
        }
    }
    fn measure(
        &mut self,
        ctx: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        cross: Option<Length>,
    ) -> Length {
        ctx.redirect_measurement(&mut self.child, axis, cross)
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, size: Size) {
        ctx.run_layout(&mut self.child, size);
        ctx.place_child(&mut self.child, Point::ORIGIN);
        ctx.derive_baselines(&self.child);
    }
    fn paint(
        &mut self,
        _: &mut PaintCtx<'_>,
        _: &PropertiesRef<'_>,
        _: &mut masonry::imaging::Painter<'_>,
    ) {
    }
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::from_slice(&[self.child.id()])
    }
}

pub(crate) struct InputTypographyView<V> {
    input: V,
    text_color: Color,
}
pub(crate) fn input_typography<V>(input: V, text_color: Color) -> InputTypographyView<V> {
    InputTypographyView { input, text_color }
}
impl<V> ViewMarker for InputTypographyView<V> {}
impl<V, State: 'static> View<State, (), ViewCtx> for InputTypographyView<V>
where
    V: WidgetView<State, Widget = TextInput>,
{
    type Element = Pod<InputTypography>;
    type ViewState = V::ViewState;
    fn build(&self, ctx: &mut ViewCtx, state: &mut State) -> (Self::Element, Self::ViewState) {
        let (child, state) = self.input.build(ctx, state);
        (
            ctx.create_pod(InputTypography {
                child: child.new_widget.to_pod(),
                text_color: self.text_color,
            }),
            state,
        )
    }
    fn rebuild(
        &self,
        prev: &Self,
        state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut State,
    ) {
        element.widget.text_color = self.text_color;
        let mut child = InputTypography::child_mut(&mut element);
        self.input
            .rebuild(&prev.input, state, ctx, child.reborrow_mut(), app);
        style_input(child, self.text_color);
    }
    fn teardown(
        &self,
        state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        self.input
            .teardown(state, ctx, InputTypography::child_mut(&mut element));
    }
    fn message(
        &self,
        state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut State,
    ) -> MessageResult<()> {
        self.input.message(
            state,
            message,
            InputTypography::child_mut(&mut element),
            app,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::view::theme::Palette;
    use masonry::properties::{ContentColor, PlaceholderColor};
    use masonry::widgets::TextArea;
    use masonry_testing::TestHarness;
    use std::sync::Arc;
    use xilem::core::{ProxyError, RawProxy, SendMessage, ViewId};
    use xilem::style::Style;

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

    fn context() -> ViewCtx {
        ViewCtx::new(
            Arc::new(NoProxy),
            Arc::new(
                tokio::runtime::Builder::new_current_thread()
                    .build()
                    .expect("create the input view's test runtime"),
            ),
        )
    }

    fn themed_input(
        pal: &Palette,
        contents: String,
    ) -> impl WidgetView<String, Widget = InputTypography> + use<> {
        input_typography(
            crate::application::view::text_input(contents, |value: &mut String, next| {
                *value = next;
            })
            .placeholder("Glyph name")
            .text_color(pal.text)
            .placeholder_color(pal.text_muted)
            .background_color(pal.field())
            .border_color(pal.field_outline),
            pal.text,
        )
    }

    #[test]
    fn theme_rebuild_updates_input_ink_without_losing_editing_state() {
        for contents in ["zero 0030 680", ""] {
            let mut value = contents.to_owned();
            let mut ctx = context();
            let mut view = themed_input(&Palette::load("light-gray"), value.clone());
            let (pod, mut state) = view.build(&mut ctx, &mut value);
            let mut harness = TestHarness::create_with_size(
                crate::application::view::default_property_set(),
                pod.new_widget,
                (280, 28),
            );
            let (input_id, text_id) = harness.edit_root_widget(|mut wrapper| {
                let mut input = InputTypography::child_mut(&mut wrapper);
                (input.id(), TextInput::text_mut(&mut input).id())
            });
            harness.focus_on(Some(text_id));
            if !contents.is_empty() {
                harness.edit_root_widget(|mut wrapper| {
                    let mut input = InputTypography::child_mut(&mut wrapper);
                    TextArea::select_byte_range(&mut TextInput::text_mut(&mut input), 5, 9);
                });
            }
            let initial = harness.render();
            let selection = harness
                .access_node(text_id)
                .expect("the retained input has an accessibility node")
                .data()
                .text_selection()
                .copied();
            if !contents.is_empty() {
                assert!(selection.is_some(), "the test must retain a real selection");
            }

            let themes = runebender::ui::theme::BUILTIN_THEME_IDS;
            for theme in std::iter::once("dark")
                .chain(themes.iter().copied())
                .chain(themes.iter().rev().copied())
                .chain(std::iter::once("light-gray"))
            {
                let pal = Palette::load(theme);
                let next = themed_input(&pal, value.clone());
                harness.edit_root_widget(|root| {
                    next.rebuild(&view, &mut state, &mut ctx, root, &mut value);
                });
                view = next;
                let actual = harness.render();

                assert_eq!(harness.root_widget().children()[0].id(), input_id);
                let text = harness
                    .get_widget_with_id(text_id)
                    .downcast::<TextArea<true>>()
                    .expect("the original editable text area remains installed");
                assert_eq!(text.get_prop::<ContentColor>().color, pal.text);
                assert_eq!(text.inner().text().to_string(), contents);
                assert_eq!(harness.focused_widget_id(), Some(text_id));
                assert_eq!(
                    harness
                        .access_node(text_id)
                        .expect("the retained input stays accessible")
                        .data()
                        .text_selection()
                        .copied(),
                    selection,
                    "theme {theme} must preserve the current text selection"
                );

                // A retained input must match the fresh rendering in the new theme,
                // including real value ink, placeholder ink, selection and caret.
                let fresh = themed_input(&pal, value.clone());
                let mut fresh_ctx = context();
                let (fresh_pod, _) = fresh.build(&mut fresh_ctx, &mut value);
                let mut fresh_harness = TestHarness::create_with_size(
                    crate::application::view::default_property_set(),
                    fresh_pod.new_widget,
                    (280, 28),
                );
                let fresh_text_id = fresh_harness.edit_root_widget(|mut wrapper| {
                    let mut input = InputTypography::child_mut(&mut wrapper);
                    TextInput::text_mut(&mut input).id()
                });
                fresh_harness.focus_on(Some(fresh_text_id));
                if !contents.is_empty() {
                    fresh_harness.edit_root_widget(|mut wrapper| {
                        let mut input = InputTypography::child_mut(&mut wrapper);
                        TextArea::select_byte_range(&mut TextInput::text_mut(&mut input), 5, 9);
                    });
                }
                assert!(
                    actual == fresh_harness.render(),
                    "retained input rendering must match fresh {theme} rendering"
                );
                if theme == "light-gray" {
                    assert!(
                        actual == initial,
                        "the original input appearance must return"
                    );
                }
            }

            if !contents.is_empty() {
                harness.keyboard_type_chars("9999");
                assert_eq!(
                    harness
                        .get_widget_with_id(text_id)
                        .downcast::<TextArea<true>>()
                        .expect("the retained text area accepts edits")
                        .inner()
                        .text()
                        .to_string(),
                    "zero 9999 680",
                    "typing after the theme changes must replace the retained selection"
                );
            }
        }
    }

    #[test]
    fn placeholder_and_typed_text_have_identical_ink_bounds() {
        let text = TextArea::new_editable("")
            .with_style(StyleProperty::FontFamily(
                crate::application::view::UI_FONT_FAMILY.into(),
            ))
            .with_style(StyleProperty::FontSize(
                crate::application::view::design::TextSize::Body.px(),
            ))
            .prepare()
            .with_props(ContentColor::new(Color::BLACK));
        let input = TextInput::from_text_area(text)
            .with_placeholder("Search glyphs")
            .with_clip(true)
            .prepare()
            .with_props(PlaceholderColor::new(Color::BLACK));
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            InputTypography {
                child: input.to_pod(),
                text_color: Color::BLACK,
            }
            .prepare(),
            (170, 28),
        );
        let placeholder = harness.render();
        harness.edit_root_widget(|mut wrapper| {
            let mut input = InputTypography::child_mut(&mut wrapper);
            TextArea::reset_text(&mut TextInput::text_mut(&mut input), "Search glyphs");
        });
        let typed = harness.render();
        harness.edit_root_widget(|mut wrapper| {
            TextInput::set_clip(&mut InputTypography::child_mut(&mut wrapper), false);
        });
        assert_eq!(
            typed,
            harness.render(),
            "clipping must not remove any glyph pixels"
        );
        assert_eq!(
            placeholder, typed,
            "placeholder typography and typed text must render identically"
        );
    }

    #[test]
    fn focused_descenders_and_numeric_text_fit_the_shared_control_height() {
        let contents = "gyp -123.45";
        let text = TextArea::new_editable(contents)
            .with_style(StyleProperty::FontFamily(
                crate::application::view::UI_FONT_FAMILY.into(),
            ))
            .with_style(StyleProperty::FontSize(
                crate::application::view::design::TextSize::Body.px(),
            ))
            .prepare()
            .with_props(ContentColor::new(Color::BLACK));
        let input = TextInput::from_text_area(text)
            .with_clip(true)
            .prepare()
            .with_props(PlaceholderColor::new(Color::BLACK));
        let mut harness = TestHarness::create_with_size(
            crate::application::view::default_property_set(),
            InputTypography {
                child: input.to_pod(),
                text_color: Color::BLACK,
            }
            .prepare(),
            (170, 28),
        );
        let mut text_id = None;
        harness.edit_root_widget(|mut wrapper| {
            let mut input = InputTypography::child_mut(&mut wrapper);
            let mut text = TextInput::text_mut(&mut input);
            text_id = Some(text.ctx.widget_id());
            TextArea::select_text(&mut text, contents);
        });
        harness.focus_on(text_id);
        let clipped = harness.render();
        harness.edit_root_widget(|mut wrapper| {
            TextInput::set_clip(&mut InputTypography::child_mut(&mut wrapper), false);
        });
        assert_eq!(
            clipped,
            harness.render(),
            "selection, caret, ascenders, and descenders fit without vertical clipping"
        );
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn ui_input_font_fallback_distinguishes_arabic_glyphs() {
        let render = |contents: &str, suffix: &str| {
            let path = std::env::temp_dir().join(format!(
                "runebender-arabic-input-{}-{suffix}.png",
                std::process::id()
            ));
            let path_string = path.to_string_lossy().into_owned();
            crate::application::platform::screenshot::render_to(
                contents.to_owned(),
                Color::BLACK,
                |state| {
                    xilem::view::sized_box(crate::application::view::text_input(
                        state.clone(),
                        |_, _| (),
                    ))
                    .width(Length::px(170.0))
                    .height(Length::px(28.0))
                },
                (170, 28),
                1.0,
                &path_string,
            );
            let image = std::fs::read(&path).expect("Arabic input screenshot should exist");
            std::fs::remove_file(path).expect("Arabic input screenshot should be removable");
            image
        };

        assert_ne!(
            render("\u{0628}", "beh"),
            render("\u{0627}", "alef"),
            "Arabic beh and alef need distinct fallback glyphs, not one tofu box"
        );
    }
}

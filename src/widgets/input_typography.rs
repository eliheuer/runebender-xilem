// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Match the pinned Masonry input's separate placeholder label to our UI font.
//! Keep its real text editor, clipping, focus and keyboard behavior intact.

use masonry::accesskit::{Node, Role};
use masonry::core::{
    AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, PaintCtx, PropertiesMut, PropertiesRef,
    RegisterCtx, StyleProperty, Update, UpdateCtx, Widget, WidgetMut, WidgetPod,
};
use masonry::kurbo::{Axis, Point, Size};
use masonry::layout::{LenReq, Length};
use masonry::widgets::{Label, TextInput};
use xilem::core::{MessageCtx, MessageResult, Mut, View, ViewMarker};
use xilem::{Pod, ViewCtx, WidgetView};

fn style_placeholder(mut input: WidgetMut<'_, TextInput>) {
    let mut label = TextInput::placeholder_mut(&mut input);
    Label::insert_style(
        &mut label,
        StyleProperty::FontFamily(crate::UI_FONT_FAMILY.into()),
    );
    Label::insert_style(
        &mut label,
        StyleProperty::FontSize(crate::TextSize::Body.px()),
    );
}

pub(crate) struct InputTypography {
    child: WidgetPod<TextInput>,
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
            ctx.mutate_child_later(&mut self.child, style_placeholder);
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

pub(crate) struct InputTypographyView<V>(V);
pub(crate) fn input_typography<V>(input: V) -> InputTypographyView<V> {
    InputTypographyView(input)
}
impl<V> ViewMarker for InputTypographyView<V> {}
impl<V> View<crate::Workspace, (), ViewCtx> for InputTypographyView<V>
where
    V: WidgetView<crate::Workspace, Widget = TextInput>,
{
    type Element = Pod<InputTypography>;
    type ViewState = V::ViewState;
    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut crate::Workspace,
    ) -> (Self::Element, Self::ViewState) {
        let (child, state) = self.0.build(ctx, state);
        (
            ctx.create_pod(InputTypography {
                child: child.new_widget.to_pod(),
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
        app: &mut crate::Workspace,
    ) {
        let mut child = InputTypography::child_mut(&mut element);
        self.0
            .rebuild(&prev.0, state, ctx, child.reborrow_mut(), app);
        style_placeholder(child);
    }
    fn teardown(
        &self,
        state: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        self.0
            .teardown(state, ctx, InputTypography::child_mut(&mut element));
    }
    fn message(
        &self,
        state: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        app: &mut crate::Workspace,
    ) -> MessageResult<()> {
        self.0.message(
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
    use masonry::properties::{ContentColor, Padding, PlaceholderColor};
    use masonry::widgets::TextArea;
    use masonry_testing::TestHarness;

    #[test]
    fn placeholder_and_typed_text_have_identical_ink_bounds() {
        let text = TextArea::new_editable("")
            .with_style(StyleProperty::FontFamily(crate::UI_FONT_FAMILY.into()))
            .with_style(StyleProperty::FontSize(crate::TextSize::Body.px()))
            .prepare()
            .with_props(ContentColor::new(masonry::peniko::Color::BLACK));
        let input = TextInput::from_text_area(text)
            .with_placeholder("Search glyphs")
            .with_clip(true)
            .prepare()
            .with_props(Padding::all(crate::Space::Sm.length()))
            .with_props(PlaceholderColor::new(masonry::peniko::Color::BLACK));
        let mut harness = TestHarness::create_with_size(
            crate::default_property_set(),
            InputTypography {
                child: input.to_pod(),
            }
            .prepare(),
            (170, 26),
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
}

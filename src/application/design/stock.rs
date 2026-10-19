// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Masonry's stock widgets, dressed in the active theme.
//!
//! This is the floor of the design system. Every stock widget a view
//! creates, from a plain `label` to a `text_input` or a `slider`, takes
//! its colors, outline, radius, and its hover, pressed, focus, and
//! disabled looks from here, without the view naming any of them. A
//! feature written against plain Xilem views therefore matches the
//! theme the day it lands.
//!
//! The set is rebuilt when the theme changes and handed to the window,
//! which swaps it in place. Views must not set these properties
//! themselves: a locally set property overrides the state layers below,
//! which is how controls lost their hover states before.

use masonry::core::{DefaultProperties, PropertyStack, Selector};
use masonry::layout::Length;
use masonry::properties::{
    Background, BarColor, BorderColor, BorderWidth, CaretColor, CheckmarkColor, ContentColor,
    CornerRadius, Padding, PlaceholderColor, SelectionColor, ThumbColor, TrackColor,
};
use masonry::widgets::{
    Button, Checkbox, Divider, Label, ProgressBar, RadioButton, Slider, TextArea, TextInput,
};

use crate::application::design::theme::Palette;
use crate::application::design::{
    Alpha, INPUT_BASELINE_OFFSET, INPUT_HORIZONTAL_INSET, INPUT_INSET,
};

/// The stock widget properties for `pal`.
pub(crate) fn widget_properties(pal: &Palette) -> DefaultProperties {
    let mut properties = masonry::theme::default_property_set();
    let stroke = BorderWidth {
        width: Length::px(pal.stroke),
    };
    let control_radius = CornerRadius {
        radius: Length::px(pal.control_radius),
    };
    let focus = BorderColor {
        color: pal.focus_ring,
    };
    let disabled_ink = ContentColor::new(pal.text.with_alpha(Alpha::Disabled.value()));
    let text_selection = SelectionColor {
        color: pal
            .role("selection")
            .with_alpha(Alpha::TextSelection.value()),
    };

    properties.insert::<Label, _>(ContentColor::new(pal.text));
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(Selector::new().with_disabled(true), disabled_ink);
        properties.insert_stack::<Label>(stack);
    }

    // A plain button is the Secondary control: the button face in an outline.
    properties.insert::<Button, _>(Background::Color(pal.button));
    properties.insert::<Button, _>(BorderColor { color: pal.outline });
    properties.insert::<Button, _>(stroke);
    properties.insert::<Button, _>(control_radius);
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_hovered(true),
            Background::Color(pal.button_hover),
        );
        stack.push_layer(
            Selector::new().with_active(true),
            Background::Color(pal.button_pressed),
        );
        stack.push_layer(Selector::new().with_focused(true), focus);
        stack.push_layer(
            Selector::new().with_disabled(true),
            (
                Background::Color(pal.button),
                BorderColor {
                    color: pal.outline.with_alpha(Alpha::Disabled.value()),
                },
            ),
        );
        properties.insert_stack::<Button>(stack);
    }

    properties.insert::<TextInput, _>(Padding {
        left: Length::px(INPUT_HORIZONTAL_INSET),
        right: Length::px(INPUT_HORIZONTAL_INSET),
        top: Length::px(INPUT_INSET + INPUT_BASELINE_OFFSET),
        bottom: Length::px(INPUT_INSET - INPUT_BASELINE_OFFSET),
    });
    properties.insert::<TextInput, _>(Background::Color(pal.field()));
    properties.insert::<TextInput, _>(BorderColor {
        color: pal.field_outline,
    });
    properties.insert::<TextInput, _>(stroke);
    properties.insert::<TextInput, _>(control_radius);
    properties.insert::<TextInput, _>(PlaceholderColor::new(pal.placeholder_ink()));
    properties.insert::<TextInput, _>(CaretColor { color: pal.text });
    properties.insert::<TextInput, _>(text_selection);
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(Selector::new().with_focused(true), focus);
        properties.insert_stack::<TextInput>(stack);
    }
    for_text_areas(&mut properties, pal, text_selection, disabled_ink);

    properties.insert::<Checkbox, _>(Background::Color(pal.field()));
    properties.insert::<Checkbox, _>(BorderColor {
        color: pal.field_outline,
    });
    properties.insert::<Checkbox, _>(stroke);
    properties.insert::<Checkbox, _>(control_radius);
    properties.insert::<Checkbox, _>(CheckmarkColor { color: pal.text });
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_hovered(true),
            Background::Color(pal.button_hover),
        );
        stack.push_layer(Selector::new().with_focused(true), focus);
        properties.insert_stack::<Checkbox>(stack);
    }
    properties.insert::<RadioButton, _>(Background::Color(pal.field()));
    properties.insert::<RadioButton, _>(BorderColor {
        color: pal.field_outline,
    });
    properties.insert::<RadioButton, _>(stroke);
    properties.insert::<RadioButton, _>(CheckmarkColor { color: pal.text });

    properties.insert::<Slider, _>(TrackColor {
        active: pal.slider_track(),
        inactive: pal.slider_track(),
    });
    properties.insert::<Slider, _>(ThumbColor(pal.slider_thumb));
    {
        let mut stack = PropertyStack::new();
        stack.push_layer(
            Selector::new().with_hovered(true),
            ThumbColor(pal.slider_thumb_active),
        );
        stack.push_layer(Selector::new().with_focused(true), focus);
        properties.insert_stack::<Slider>(stack);
    }

    properties.insert::<ProgressBar, _>(Background::Color(pal.field()));
    properties.insert::<ProgressBar, _>(BorderColor {
        color: pal.field_outline,
    });
    properties.insert::<ProgressBar, _>(stroke);
    properties.insert::<ProgressBar, _>(BarColor(pal.role("accent")));
    properties.insert::<Divider, _>(ContentColor::new(pal.outline));

    properties
}

fn for_text_areas(
    properties: &mut DefaultProperties,
    pal: &Palette,
    selection: SelectionColor,
    disabled_ink: ContentColor,
) {
    properties.insert::<TextArea<false>, _>(ContentColor::new(pal.text));
    properties.insert::<TextArea<false>, _>(CaretColor { color: pal.text });
    properties.insert::<TextArea<false>, _>(selection);
    properties.insert::<TextArea<true>, _>(ContentColor::new(pal.text));
    properties.insert::<TextArea<true>, _>(CaretColor { color: pal.text });
    properties.insert::<TextArea<true>, _>(selection);
    for editable in [false, true] {
        let mut stack = PropertyStack::new();
        stack.push_layer(Selector::new().with_disabled(true), disabled_ink);
        if editable {
            properties.insert_stack::<TextArea<true>>(stack);
        } else {
            properties.insert_stack::<TextArea<false>>(stack);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::core::{NewWidget, PointerButton, Widget as _};
    use masonry_testing::TestHarness;

    fn button_harness() -> (TestHarness<Button>, masonry::core::WidgetId) {
        let pal = Palette::load("gray");
        let widget = NewWidget::new(Button::new(Label::new("Plain").prepare()));
        let id = widget.id();
        let harness = TestHarness::create_with_size(widget_properties(&pal), widget, (120, 40));
        (harness, id)
    }

    #[test]
    fn a_plain_stock_button_gets_the_themed_states() {
        let (mut harness, id) = button_harness();
        let rest = harness.render();
        harness.mouse_move_to(id);
        let hover = harness.render();
        harness.mouse_button_press(Some(PointerButton::Primary));
        let pressed = harness.render();
        assert_ne!(rest, hover, "hover");
        assert_ne!(hover, pressed, "pressed");
        let _ = Button::new(Label::new("").prepare()).accepts_focus();
    }
}

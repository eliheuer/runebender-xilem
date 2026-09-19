// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The info panel's sections: layers, axes, paths, coordinates, curves, measure, background, marks, font info.

use crate::application::editor::session::Session;
use crate::application::view::chrome::direction_chips;
use crate::application::view::design::{
    ButtonShape, ControlSize, Radius, Region, Space, Stroke, TextSize, column as xcolumn,
    row as xrow,
};
use crate::application::view::panels::tabs::tab_chip;
use crate::application::view::recipes::button;
use crate::application::view::theme::Palette;
use crate::application::view::{design, label, recipes, text_input};
use crate::application::widgets::icon_button;
use crate::application::workspace::{Mode, Workspace};
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::{FlexSpacer, sized_box};

/// Reference underlays, distinct from document masters. GPUI keeps this
/// section folded in the overview, so the inspector stays a concise map of
/// the document until someone needs to compare outlines.
pub(crate) fn layers_section(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    if app.font.master_names().len() < 2 && !app.can_manage_glyph_layers() {
        return None;
    }
    let pal = &app.palette;
    let active = app.font.active();
    let rows: Vec<_> = app
        .font
        .short_master_names()
        .into_iter()
        .enumerate()
        .filter(|(i, _)| *i != active)
        .map(|(i, name)| {
            let shown = app.reference_layers.contains(&i);
            let (bg, fg) = if shown {
                (pal.role("reference").with_alpha(0.28), pal.text)
            } else {
                (pal.panel, pal.text_muted)
            };
            sized_box(
                button(
                    xrow(
                        Region::Inline,
                        (
                            recipes::marker(recipes::Marker::Bullet, fg),
                            label(name).text_size(TextSize::Body.px()).color(fg),
                        ),
                    ),
                    move |app: &mut Workspace| {
                        if !app.reference_layers.remove(&i) {
                            app.reference_layers.insert(i);
                        }
                    },
                )
                .background_color(bg),
            )
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::from(ControlSize::Control),
            ))
        })
        .collect();
    Some(xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Layers",
                !app.collapsed.contains("Layers"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Layers") {
                        app.collapsed.insert("Layers");
                    }
                },
            ),
            (!app.collapsed.contains("Layers")).then(|| {
                xcolumn(
                    Region::List,
                    (
                        xcolumn(Region::List, rows),
                        app.can_manage_glyph_layers()
                            .then(|| glyph_layer_controls(app)),
                    ),
                )
            }),
        ),
    ))
}

fn glyph_layer_controls(app: &Workspace) -> Box<xilem::AnyWidgetView<Workspace>> {
    let pal = &app.palette;
    xcolumn(
        Region::List,
        (
            label("Glyph layer name")
                .text_size(TextSize::Body.px())
                .color(pal.text_muted),
            text_input(app.layer_name_buf.clone(), |app: &mut Workspace, value| {
                app.layer_name_buf = value;
            })
            .text_color(pal.text)
            .background_color(pal.field()),
            xrow(
                Region::Inline,
                (
                    recipes::action(pal, "Copy to layer".into(), |app| {
                        app.change_sources("layer-add");
                    }),
                    recipes::action(pal, "Remove layer".into(), |app| {
                        app.change_sources("layer-remove");
                    }),
                ),
            ),
            xrow(
                Region::Inline,
                (
                    recipes::action(pal, "Undo layers".into(), |app| app.change_sources("undo")),
                    recipes::action(pal, "Redo".into(), |app| app.change_sources("redo")),
                ),
            ),
        ),
    )
    .boxed()
}

/// Masters: one compact row per designspace source. This is deliberately
/// separate from reference-layer controls: GPUI presents master switching as
/// a plain list, while underlays belong to the editor's Layers section.
pub(crate) fn masters_section(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    app.font.project.ds_doc.as_ref()?;
    let pal = &app.palette;
    let rows: Vec<_> = app
        .font
        .short_master_names()
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let active = i == app.font.active();
            let (bg, fg, border) = if active {
                (pal.selected_bg(), pal.selected_content_ink(), pal.outline)
            } else {
                (pal.panel, pal.text, xilem::Color::TRANSPARENT)
            };
            sized_box(
                button(
                    label(name).text_size(TextSize::Body.px()).color(fg),
                    move |app: &mut Workspace| app.set_master(i),
                )
                .padding(Space::Sm)
                .background_color(bg)
                .border_color(border)
                .border_width(if active {
                    Stroke::Hairline.length()
                } else {
                    Stroke::None.length()
                })
                .corner_radius(Radius::None.length()),
            )
            .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Icon)))
        })
        .collect();
    Some(xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Masters",
                !app.collapsed.contains("Masters"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Masters") {
                        app.collapsed.insert("Masters");
                    }
                },
            ),
            (!app.collapsed.contains("Masters")).then(|| {
                xcolumn(
                    Region::List,
                    (
                        xcolumn(Region::List, rows),
                        label("Source name")
                            .text_size(TextSize::Body.px())
                            .color(pal.text_muted),
                        text_input(app.source_name_buf.clone(), |app: &mut Workspace, value| {
                            app.source_name_buf = value;
                        })
                        .text_color(pal.text)
                        .background_color(pal.field()),
                        label("Uses the axis sliders below")
                            .text_size(TextSize::Body.px())
                            .color(pal.text_muted),
                        xrow(
                            Region::Inline,
                            (
                                recipes::action(pal, "Add source".into(), |app| {
                                    app.change_sources("add");
                                }),
                                recipes::action(pal, "Apply".into(), |app| {
                                    app.change_sources("update");
                                }),
                            ),
                        ),
                        xrow(
                            Region::Inline,
                            (
                                recipes::action(pal, "Up".into(), |app| app.change_sources("up")),
                                recipes::action(pal, "Down".into(), |app| {
                                    app.change_sources("down");
                                }),
                                recipes::action(pal, "Remove".into(), |app| {
                                    app.change_sources("remove");
                                }),
                            ),
                        ),
                        xrow(
                            Region::Inline,
                            (
                                recipes::action(pal, "Undo sources".into(), |app| {
                                    app.change_sources("undo");
                                }),
                                recipes::action(pal, "Redo".into(), |app| {
                                    app.change_sources("redo");
                                }),
                            ),
                        ),
                    ),
                )
            }),
        ),
    ))
}

/// Axes: one labeled slider per designspace axis, in the inspector.
pub(crate) fn axes_section(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    if app.font.axes.is_empty() {
        return None;
    }
    let pal = &app.palette;
    let (muted, text) = (pal.text_muted, pal.text);
    let rows: Vec<_> = app
        .font
        .axes
        .iter()
        .enumerate()
        .map(|(i, ax)| {
            let value = app.axis_values.get(i).copied().unwrap_or(ax.default);
            xcolumn(
                Region::List,
                (
                    xrow(
                        Region::Inline,
                        (
                            label(ax.tag.clone())
                                .text_size(TextSize::Body.px())
                                .color(muted),
                            FlexSpacer::Flex(1.0),
                            label(format!("{value:.0}"))
                                .text_size(TextSize::Body.px())
                                .color(text),
                        ),
                    ),
                    recipes::neutral_slider(
                        &app.palette,
                        ax.min,
                        ax.max,
                        value,
                        move |app: &mut Workspace, v| {
                            app.set_axis(i, v);
                        },
                    )
                    .width(Length::px(214.0)),
                ),
            )
        })
        .collect();
    // Distinguish a valid instance from the active-outline fallback used when
    // this glyph's masters cannot interpolate.
    let hint = app.interpolation_status().map(|status| {
        let (summary, detail) = status
            .split_once(": ")
            .map_or((status.clone(), None), |(summary, detail)| {
                (summary.to_string(), Some(detail.to_string()))
            });
        xcolumn(
            Region::List,
            (
                label(summary)
                    .text_size(TextSize::Caption.px())
                    .color(pal.role("warning")),
                detail.map(|detail| {
                    label(detail)
                        .text_size(TextSize::Caption.px())
                        .color(pal.text_muted)
                }),
            ),
        )
    });
    Some(xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Axes",
                !app.collapsed.contains("Axes"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Axes") {
                        app.collapsed.insert("Axes");
                    }
                },
            ),
            (!app.collapsed.contains("Axes")).then(|| xcolumn(Region::Form, rows)),
            (!app.collapsed.contains("Axes")).then_some(hint),
        ),
    ))
}

/// Shaping choices for the editor inspector, in the same right-panel position
/// as GPUI. These mutate the shared editor/preview text state.
pub(crate) fn shaping_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let feature = |tag: &'static str| {
        tab_chip(
            pal,
            tag.into(),
            !app.text_features_disabled.contains(tag),
            false,
            move |app: &mut Workspace| {
                if !app.text_features_disabled.remove(tag) {
                    app.text_features_disabled.insert(tag.into());
                }
            },
        )
    };
    let locale =
        |label_text: &'static str, script: Option<&'static str>, language: Option<&'static str>| {
            let active =
                app.text_script.as_deref() == script && app.text_language.as_deref() == language;
            tab_chip(
                pal,
                label_text.into(),
                active,
                false,
                move |app: &mut Workspace| {
                    app.text_script = script.map(str::to_string);
                    app.text_language = language.map(str::to_string);
                },
            )
        };
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Shaping",
                !app.collapsed.contains("Shaping"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Shaping") {
                        app.collapsed.insert("Shaping");
                    }
                },
            ),
            (!app.collapsed.contains("Shaping")).then(|| {
                xcolumn(
                    Region::Form,
                    (
                        recipes::field(
                            pal,
                            "Preview text",
                            app.preview_text.clone(),
                            |app: &mut Workspace, value| app.preview_text = value,
                        ),
                        direction_chips(app),
                        xcolumn(
                            Region::List,
                            (
                                xrow(
                                    Region::Inline,
                                    (feature("liga"), feature("rlig"), feature("kern")),
                                ),
                                xrow(Region::Inline, (feature("mark"), feature("mkmk"))),
                            ),
                        ),
                        xrow(
                            Region::Inline,
                            (
                                locale("Auto", None, None),
                                locale("Arabic", Some("arab"), Some("ar")),
                                locale("Urdu", Some("arab"), Some("ur")),
                            ),
                        ),
                    ),
                )
            }),
        ),
    )
}

pub(crate) fn transformations_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use crate::application::editor::session::BoolOp;
    use icon_button::icon_button;
    let pal = &app.palette;
    let fg = pal.text_muted;
    let fga = pal.selected_ink();
    let abg = pal.selected_bg();
    let hbg = pal.control;
    let op = move |icon: &'static str, f: fn(&mut Session) -> bool| {
        icon_button(
            icon,
            false,
            fg,
            fga,
            abg,
            hbg,
            move |app: &mut Workspace| app.apply_op(f),
        )
        .icon_size(design::TRANSFORM_ICON_SIZE)
        .tile_size(design::TRANSFORM_TILE_SIZE)
    };
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Transformations",
                !app.collapsed.contains("Transformations"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Transformations") {
                        app.collapsed.insert("Transformations");
                    }
                },
            ),
            (!app.collapsed.contains("Transformations")).then(|| {
                xcolumn(
                    Region::Section,
                    (
                        xrow(
                            Region::List,
                            (
                                FlexSpacer::Flex(1.0),
                                op("flip-h", |s| s.flip_horizontal()),
                                FlexSpacer::Flex(1.0),
                                op("flip-v", |s| s.flip_vertical()),
                                FlexSpacer::Flex(1.0),
                                op("rot-ccw", |s| s.rotate_90()),
                                FlexSpacer::Flex(1.0),
                                op("rot-cw", |s| s.rotate_90_clockwise()),
                                FlexSpacer::Flex(1.0),
                                op("duplicate", |s| s.duplicate()),
                                FlexSpacer::Flex(1.0),
                            ),
                        ),
                        xrow(
                            Region::List,
                            (
                                FlexSpacer::Flex(1.0),
                                op("duplicate-repeat", |s| s.duplicate_repeat()),
                                FlexSpacer::Flex(1.0),
                                op("union", |s| s.remove_overlap()),
                                FlexSpacer::Flex(1.0),
                                op("subtract", |s| s.boolean(BoolOp::Subtract)),
                                FlexSpacer::Flex(1.0),
                                op("intersect", |s| s.boolean(BoolOp::Intersect)),
                                FlexSpacer::Flex(1.0),
                                op("exclude", |s| s.boolean(BoolOp::Exclude)),
                                FlexSpacer::Flex(1.0),
                            ),
                        ),
                    ),
                )
                .gap(Space::Md)
            }),
        ),
    )
    .gap(Length::px(design::TRANSFORM_CONTROLS_GAP))
}

/// Curve and outline operations follow GPUI in their own disclosure below
/// geometric transformations.
pub(crate) fn path_operations_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Path Operations",
                !app.collapsed.contains("Path Operations"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Path Operations") {
                        app.collapsed.insert("Path Operations");
                    }
                },
            ),
            (!app.collapsed.contains("Path Operations")).then(|| path_operations_controls(app)),
        ),
    )
    .gap(Length::px(design::TRANSFORM_CONTROLS_GAP))
}

/// A transformation parameter shares one row with its label; Enter applies it.
fn transform_parameter(
    app: &Workspace,
    title: &'static str,
    placeholder: &'static str,
    value: String,
    on_change: fn(&mut Workspace, String),
    on_enter: fn(&mut Workspace, String),
) -> impl WidgetView<Workspace> + use<> {
    xrow(
        Region::Inline,
        (
            sized_box(label(title).color(app.palette.text_muted)).dims(Dimensions::new(
                Dim::Fixed(Length::px(design::TRANSFORM_LABEL_WIDTH)),
                Dim::Auto,
            )),
            recipes::field_bare(&app.palette, placeholder, value, on_change, on_enter).flex(1.0),
        ),
    )
}

/// Path operations belong to the same disclosure as the geometric tools.
fn path_operations_controls(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    xcolumn(
        Region::List,
        (
            xrow(
                Region::Inline,
                (
                    tbtn(pal, "Harmonize", |s| s.harmonize()).flex(1.0),
                    tbtn(pal, "Balance", |s| s.balance()).flex(1.0),
                ),
            ),
            xrow(
                Region::Inline,
                (
                    tbtn(pal, "Optimize", |s| s.optimize()).flex(1.0),
                    tbtn(pal, "Add Extremes", |s| s.add_extremes()).flex(1.0),
                ),
            ),
            xrow(
                Region::Inline,
                (
                    tbtn(pal, "Round Corners", |s| s.round_corners()).flex(1.0),
                    tbtn(pal, "Reverse", |s| s.reverse()).flex(1.0),
                ),
            ),
            transform_parameter(
                app,
                "Slant °",
                "",
                app.slant_buf.clone(),
                |app, value| app.slant_buf = value,
                |app, value| {
                    app.slant_buf = value;
                    app.command_filter_slant();
                },
            ),
            transform_parameter(
                app,
                "Stroke width",
                "",
                app.stroke_buf.clone(),
                |app, value| app.stroke_buf = value,
                |app, value| {
                    app.stroke_buf = value;
                    app.command_expand_stroke();
                },
            ),
            transform_parameter(
                app,
                "Offset ±",
                "",
                app.offset_buf.clone(),
                |app, value| app.offset_buf = value,
                |app, value| {
                    app.offset_buf = value;
                    app.command_filter_offset();
                },
            ),
            transform_parameter(
                app,
                "Extrude d,°",
                "15,30",
                app.extrude_buf.clone(),
                |app, value| app.extrude_buf = value,
                |app, value| {
                    app.extrude_buf = value;
                    app.command_filter_extrude();
                },
            ),
            transform_parameter(
                app,
                "Roughen s,h,v",
                "15,15,10",
                app.roughen_buf.clone(),
                |app, value| app.roughen_buf = value,
                |app, value| {
                    app.roughen_buf = value;
                    app.command_filter_roughen();
                },
            ),
        ),
    )
    .gap(Space::Sm)
}

/// The LSB/RSB text-buffer strings for a session.
pub(crate) fn metric_bufs(session: &Session) -> (String, String) {
    match session.side_bearings() {
        Some(sb) => (format!("{}", sb.lsb), format!("{}", sb.rsb)),
        None => (String::new(), String::new()),
    }
}

/// A labeled path-operation button.
pub(crate) fn tbtn(
    pal: &Palette,
    text: &'static str,
    f: fn(&mut Session) -> bool,
) -> impl WidgetView<Workspace> + use<> {
    recipes::action(pal, text.into(), move |app: &mut Workspace| {
        app.apply_op(f);
    })
}

/// Coordinates: the 9-point reference picker beside compact X/Y/W/H fields.
/// GPUI keeps this panel up whether or not anything is selected, so the
/// inspector does not jump.
pub(crate) fn coordinates_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use crate::application::view::design::{
        COORD_LABEL_WIDTH, COORD_PICKER_EDGE, COORD_PICKER_GAP, COORD_PICKER_INSET,
    };
    use crate::application::widgets::quadrant_picker::quadrant_grid;
    use runebender::outline::path::Quadrant;
    const QUADRANTS: [Quadrant; 9] = [
        Quadrant::TopLeft,
        Quadrant::Top,
        Quadrant::TopRight,
        Quadrant::Left,
        Quadrant::Center,
        Quadrant::Right,
        Quadrant::BottomLeft,
        Quadrant::Bottom,
        Quadrant::BottomRight,
    ];
    let pal = &app.palette;
    let dot = |q: Quadrant| {
        let active = app.coord_quadrant == q;
        let (bg, border) = if active {
            (pal.editor_control_ink(), pal.editor_control_ink())
        } else {
            (pal.panel, pal.outline)
        };
        sized_box(
            button(label(""), move |app: &mut Workspace| {
                app.coord_quadrant = q;
                app.refresh_coord_bufs();
            })
            .padding(Space::None)
            .background_color(bg)
            .border_color(border)
            .border_width(Stroke::Hairline.length())
            .corner_radius(ButtonShape::Circular.radius()),
        )
        .dims(Dimensions::fixed(
            ControlSize::Dot.length(),
            ControlSize::Dot.length(),
        ))
    };
    let row = |a: usize| {
        xrow(
            Region::List,
            (
                dot(QUADRANTS[a]),
                dot(QUADRANTS[a + 1]),
                dot(QUADRANTS[a + 2]),
            ),
        )
        .gap(Length::px(COORD_PICKER_GAP))
    };
    let picker_buttons = sized_box(
        xcolumn(Region::List, (row(0), row(3), row(6))).gap(Length::px(COORD_PICKER_GAP)),
    )
    .padding(Length::px(COORD_PICKER_INSET));
    let picker = sized_box(xilem::view::zstack((
        quadrant_grid(pal.outline),
        picker_buttons,
    )))
    .dims(Dimensions::fixed(
        Length::px(COORD_PICKER_EDGE),
        Length::px(COORD_PICKER_EDGE),
    ));
    let field = |name: &'static str, value: String, axis: usize| {
        xrow(
            Region::Inline,
            (
                sized_box(
                    label(name)
                        .text_size(TextSize::Body.px())
                        .color(pal.text_muted),
                )
                .dims(Dimensions::fixed(
                    Length::px(COORD_LABEL_WIDTH),
                    ControlSize::Icon.length(),
                )),
                sized_box(
                    text_input(value, move |app: &mut Workspace, v| {
                        if axis < 2 {
                            app.set_coord(axis, v);
                        } else {
                            app.set_coord_size(axis == 2, v);
                        }
                    })
                    .text_color(pal.text)
                    .background_color(pal.field())
                    .border_color(pal.field_outline)
                    .border_width(Stroke::Hairline.length())
                    .corner_radius(Radius::None.length()),
                )
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::from(ControlSize::Control),
                ))
                .flex(1.0),
            ),
        )
        .gap(Space::Sm)
    };
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Coordinates",
                !app.collapsed.contains("Coordinates"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Coordinates") {
                        app.collapsed.insert("Coordinates");
                    }
                },
            ),
            (!app.collapsed.contains("Coordinates")).then(|| {
                xrow(
                    Region::Inline,
                    (
                        picker,
                        xcolumn(
                            Region::List,
                            (
                                xrow(
                                    Region::Inline,
                                    (
                                        field("X", app.coord_x_buf.clone(), 0).flex(1.0),
                                        field("W", app.coord_w_buf.clone(), 2).flex(1.0),
                                    ),
                                )
                                .gap(Space::Md),
                                xrow(
                                    Region::Inline,
                                    (
                                        field("Y", app.coord_y_buf.clone(), 1).flex(1.0),
                                        field("H", app.coord_h_buf.clone(), 3).flex(1.0),
                                    ),
                                )
                                .gap(Space::Md),
                            ),
                        )
                        .gap(Space::Sm)
                        .flex(1.0),
                    ),
                )
                .gap(Space::Md)
            }),
        ),
    )
    .gap(Space::Sm)
}

pub(crate) fn curves_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let view = app.view;
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Curves",
                !app.collapsed.contains("Curves"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Curves") {
                        app.collapsed.insert("Curves");
                    }
                },
            ),
            (!app.collapsed.contains("Curves")).then(|| {
                xrow(
                    Region::Inline,
                    (
                        recipes::toggle(
                            pal,
                            "Curvature Comb".into(),
                            view.comb,
                            |app: &mut Workspace| {
                                app.view.comb = !app.view.comb;
                            },
                        )
                        .flex(1.0),
                        recipes::toggle(
                            pal,
                            "Continuity".into(),
                            view.continuity,
                            |app: &mut Workspace| {
                                app.view.continuity = !app.view.continuity;
                            },
                        )
                        .flex(1.0),
                    ),
                )
            }),
        ),
    )
    .gap(Space::Sm)
}

/// Measure: the option toggles the Measure tool works through. Picking
/// the tool turns the usual three on; the toggles are what make it
/// answer a specific question instead of drawing everything at once.
pub(crate) fn measure_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let view = app.view;
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Measure",
                !app.collapsed.contains("Measure"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Measure") {
                        app.collapsed.insert("Measure");
                    }
                },
            ),
            (!app.collapsed.contains("Measure")).then(|| {
                xrow(
                    Region::Inline,
                    (
                        recipes::toggle(
                            pal,
                            "Color".into(),
                            view.colorize,
                            |app: &mut Workspace| {
                                app.view.colorize = !app.view.colorize;
                            },
                        ),
                        recipes::toggle(
                            pal,
                            "Handles".into(),
                            view.handles,
                            |app: &mut Workspace| {
                                app.view.handles = !app.view.handles;
                            },
                        ),
                    ),
                )
            }),
            (!app.collapsed.contains("Measure")).then(|| {
                xrow(
                    Region::Inline,
                    (
                        recipes::toggle(
                            pal,
                            "Segments".into(),
                            view.segments,
                            |app: &mut Workspace| {
                                app.view.segments = !app.view.segments;
                            },
                        ),
                        recipes::toggle(
                            pal,
                            "Bearings".into(),
                            view.bearings,
                            |app: &mut Workspace| {
                                app.view.bearings = !app.view.bearings;
                            },
                        ),
                    ),
                )
            }),
            // Lengths as sums of powers of two: 96 reads as 64+32. The
            // web editor's habit, and the reason for the tier colors.
            (!app.collapsed.contains("Measure")).then(|| {
                recipes::toggle(
                    pal,
                    "Popcount sums".into(),
                    view.popcount,
                    |app: &mut Workspace| {
                        app.view.popcount = !app.view.popcount;
                    },
                )
            }),
        ),
    )
}

/// Background: the UFO's background layer, and a reference glyph. Both
/// are things to trace against, so both draw quietly and neither can be
/// selected.
pub(crate) fn background_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Background",
                !app.collapsed.contains("Background"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Background") {
                        app.collapsed.insert("Background");
                    }
                },
            ),
            (!app.collapsed.contains("Background")).then(|| {
                xcolumn(
                    Region::Inline,
                    (
                        xrow(
                            Region::Inline,
                            (
                                recipes::toggle(
                                    pal,
                                    "Background".into(),
                                    app.show_background,
                                    |app: &mut Workspace| {
                                        app.show_background = !app.show_background;
                                    },
                                )
                                .flex(1.0),
                                recipes::toggle(
                                    pal,
                                    "Mark cloud".into(),
                                    app.show_mark_cloud,
                                    |app: &mut Workspace| {
                                        app.show_mark_cloud = !app.show_mark_cloud;
                                    },
                                )
                                .flex(1.0),
                            ),
                        ),
                        xrow(
                            Region::Inline,
                            (recipes::action(
                                pal,
                                "Send to background".into(),
                                |app: &mut Workspace| {
                                    app.send_to_background();
                                },
                            )
                            .flex(1.0),),
                        ),
                        xrow(
                            Region::Inline,
                            (
                                recipes::action(pal, "Swap".into(), |app: &mut Workspace| {
                                    app.swap_background();
                                })
                                .flex(1.0),
                                recipes::action(pal, "Clear".into(), |app: &mut Workspace| {
                                    app.clear_background();
                                })
                                .flex(1.0),
                            ),
                        ),
                        xrow(
                            Region::Inline,
                            (
                                sized_box(label("Reference").color(pal.text_muted)).dims(
                                    Dimensions::new(
                                        Dim::Fixed(Length::px(design::TRANSFORM_LABEL_WIDTH)),
                                        Dim::Auto,
                                    ),
                                ),
                                recipes::field_bare(
                                    pal,
                                    "glyph name",
                                    app.reference_buf.clone(),
                                    |app, value| app.reference_buf = value,
                                    |app, value| app.reference_buf = value,
                                )
                                .flex(1.0),
                            ),
                        ),
                    ),
                )
            }),
        ),
    )
    .gap(Space::Sm)
}

pub(crate) fn mark_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let swatch = |mark_label: Option<String>, color: xilem::Color| {
        sized_box(
            button(label(""), move |app: &mut Workspace| {
                app.set_mark(mark_label.clone());
            })
            .background_color(color)
            .corner_radius(ButtonShape::Circular.radius()),
        )
        .dims(Dimensions::fixed(
            ControlSize::Row.length(),
            ControlSize::Row.length(),
        ))
    };
    let marks: Vec<_> = app
        .palette
        .mark_list()
        .into_iter()
        .map(|(name, color)| swatch(Some(name), color))
        .collect();
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Color",
                !app.collapsed.contains("Mark"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Mark") {
                        app.collapsed.insert("Mark");
                    }
                },
            ),
            (!app.collapsed.contains("Mark"))
                .then(|| xrow(Region::Inline, (swatch(None, pal.control),))),
            (!app.collapsed.contains("Mark")).then(|| xrow(Region::List, marks)),
        ),
    )
}

/// The font's metadata, shown when no glyph is picked.
pub(crate) fn font_info_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    // The names and the design metrics; the export and hinting numbers go to
    // Advanced below.
    let rows: Vec<_> = app
        .font
        .info_rows()
        .into_iter()
        .take(8)
        .map(|(name, value)| recipes::kv(pal, name.to_string(), value))
        .collect();
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle_height(
                pal,
                "Font info",
                !app.collapsed.contains("Font info"),
                if matches!(app.mode, Mode::Nodes) && app.collapsed.contains("Font info") {
                    design::NODE_VIEW_SECTION_HEADER_HEIGHT
                } else {
                    ControlSize::Row.px()
                },
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Font info") {
                        app.collapsed.insert("Font info");
                    }
                },
            ),
            (!app.collapsed.contains("Font info")).then(|| xcolumn(Region::List, rows)),
        ),
    )
}

/// The typo, hhea and win metrics: real, but not an overview of the
/// font, so the section starts collapsed.
pub(crate) fn font_advanced_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let rows: Vec<_> = app
        .font
        .info_rows()
        .into_iter()
        .skip(8)
        .map(|(name, value)| recipes::kv(pal, name.to_string(), value))
        .collect();
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Advanced",
                !app.collapsed.contains("Advanced"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Advanced") {
                        app.collapsed.insert("Advanced");
                    }
                },
            ),
            (!app.collapsed.contains("Advanced")).then(|| xcolumn(Region::List, rows)),
        ),
    )
}

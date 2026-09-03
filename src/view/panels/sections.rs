// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The info panel's sections: layers, axes, paths, coordinates, curves, measure, background, marks, font info.

use crate::*;

/// Layers: one row per master, with a thumbnail of the current glyph in
/// that master. Clicking a row switches the active master. This is the
/// gpui inspector's Layers section, and it replaces the old tab strip
/// that sat across the top of the canvas.
pub(crate) fn layers_section(app: &Workspace) -> Option<impl WidgetView<Workspace> + use<>> {
    use masonry::imaging::Painter;
    use masonry::kurbo::{Affine, Size};
    if app.font.master_names.len() < 2 {
        return None;
    }
    let pal = &app.palette;
    let glyph_name = match app.mode {
        Mode::Editor(_) => Some(app.session.glyph_name.clone()),
        Mode::Overview | Mode::Nodes => app
            .selected
            .and_then(|i| app.font.glyphs.get(i))
            .map(|g| g.name.clone()),
    };
    let (asc, desc) = (app.font.ascender, app.font.descender);
    let rows: Vec<_> = app
        .font
        .short_master_names()
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let active = i == app.font.active;
            let shown = app.reference_layers.contains(&i);
            let (bg, fg) = if active {
                (pal.selected_bg(), pal.selected_ink())
            } else {
                (pal.panel, pal.text)
            };
            // A lit thumbnail means the master is drawn as a ghost under
            // the active outline; clicking the thumbnail toggles that.
            let ink = if active || shown {
                pal.text
            } else {
                pal.text_muted
            };
            let thumb_bg = if shown {
                pal.role("reference").with_alpha(0.28)
            } else {
                pal.control
            };
            let path_and_advance = glyph_name
                .as_ref()
                .and_then(|n| app.font.master_glyph(i, n));
            let thumb = path_and_advance.map(|(path, advance)| {
                sized_box(
                    button(
                        sized_box(canvas(
                            move |_app: &mut Workspace, _ctx, scene, size: Size| {
                                let mut p = Painter::new(scene);
                                let em = (asc - desc).max(1.0);
                                let scale = (size.height / em).min(size.width / advance.max(1.0));
                                let ox = (size.width - advance * scale) / 2.0;
                                let baseline = size.height + desc * scale;
                                let t = Affine::new([scale, 0.0, 0.0, -scale, ox, baseline]);
                                p.fill(&(t * path.clone()), ink).draw();
                            },
                        ))
                        .dims(Dimensions::new(
                            Dim::from(ControlSize::Icon),
                            Dim::from(ControlSize::Icon),
                        )),
                        move |app: &mut Workspace| {
                            if !app.reference_layers.remove(&i) {
                                app.reference_layers.insert(i);
                            }
                        },
                    )
                    .background_color(thumb_bg),
                )
                .dims(Dimensions::new(
                    Dim::from(ControlSize::Control),
                    Dim::from(ControlSize::Control),
                ))
            });
            xrow(
                Region::Inline,
                (
                    thumb,
                    sized_box(
                        button(
                            label(name).text_size(TextSize::Body.px()).color(fg),
                            move |app: &mut Workspace| app.set_master(i),
                        )
                        .background_color(bg),
                    )
                    .dims(Dimensions::new(
                        Dim::Stretch,
                        Dim::from(ControlSize::Control),
                    ))
                    .flex(1.0),
                ),
            )
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
            (!app.collapsed.contains("Layers")).then(|| xcolumn(Region::List, rows)),
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
                    slider(ax.min, ax.max, value, move |app: &mut Workspace, v| {
                        app.set_axis(i, v);
                    })
                    .width(Length::px(214.0)),
                ),
            )
        })
        .collect();
    // A short hint when the location sits off any master.
    let hint = (!app.on_active_master()).then(|| {
        label("interpolated")
            .text_size(TextSize::Caption.px())
            .color(pal.role("warning"))
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

pub(crate) fn path_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use crate::edit::session::BoolOp;
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
            // Two even rows of four, the way gpui lays its icon grid out. A
            // ragged 3 / 4 / 1 grid was the panel's most visible defect.
            (!app.collapsed.contains("Transformations")).then(|| {
                xrow(
                    Region::List,
                    (
                        op("flip-h", |s| s.flip_horizontal()),
                        op("flip-v", |s| s.flip_vertical()),
                        op("rot-cw", |s| s.rotate_90()),
                        op("close", |s| s.decompose()),
                    ),
                )
            }),
            (!app.collapsed.contains("Transformations")).then(|| {
                xrow(
                    Region::List,
                    (
                        op("union", |s| s.remove_overlap()),
                        op("subtract", |s| s.boolean(BoolOp::Subtract)),
                        op("intersect", |s| s.boolean(BoolOp::Intersect)),
                        op("exclude", |s| s.boolean(BoolOp::Exclude)),
                    ),
                )
            }),
            // Labeled transform buttons, matching gpui's Transformations block.
            (!app.collapsed.contains("Transformations")).then(|| {
                xrow(
                    Region::Inline,
                    (
                        tbtn(pal, "Harmonize", |s| s.harmonize()),
                        tbtn(pal, "Balance", |s| s.balance()),
                    ),
                )
            }),
            (!app.collapsed.contains("Transformations")).then(|| {
                xrow(
                    Region::Inline,
                    (
                        tbtn(pal, "Optimize", |s| s.optimize()),
                        tbtn(pal, "Round", |s| s.round_corners()),
                    ),
                )
            }),
            (!app.collapsed.contains("Transformations"))
                .then(|| xrow(Region::Inline, (tbtn(pal, "Reverse", |s| s.reverse()),))),
        ),
    )
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
    text_button(text, move |app: &mut Workspace| app.apply_op(f)).background_color(pal.button)
}

/// Coordinates: the 9-point reference picker beside the X/Y fields, with
/// the selection's size on the right. gpui keeps this panel up whether or
/// not anything is selected, so the inspector does not jump.
pub(crate) fn coordinates_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use runebender_core::outline::path::Quadrant;
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
    let bounds = app.session.selection_bounds();
    // The picker: three rows of three dots, the active one filled accent.
    let dot = |q: Quadrant| {
        let active = app.coord_quadrant == q;
        let (bg, border) = if active {
            (pal.text, pal.text)
        } else {
            (pal.panel, pal.outline)
        };
        sized_box(
            button(label(""), move |app: &mut Workspace| {
                app.coord_quadrant = q;
                app.refresh_coord_bufs();
            })
            // Without this the button's own padding sets a minimum width
            // and the dot stretches into a pill.
            .padding(Space::None)
            .background_color(bg)
            .border_color(border)
            .border_width(Stroke::Hairline.length())
            .corner_radius(Radius::Sm.length()),
        )
        .dims(Dimensions::fixed(
            ControlSize::Dot.length(),
            ControlSize::Dot.length(),
        ))
    };
    let row = |a: usize| {
        xrow(
            Region::Card,
            (
                dot(QUADRANTS[a]),
                dot(QUADRANTS[a + 1]),
                dot(QUADRANTS[a + 2]),
            ),
        )
    };
    let picker = xcolumn(Region::Card, (row(0), row(3), row(6)));
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
                    ControlSize::Swatch.length(),
                    ControlSize::Icon.length(),
                )),
                text_input(value, move |app: &mut Workspace, v| app.set_coord(axis, v))
                    .background_color(pal.field())
                    .flex(1.0),
            ),
        )
    };
    let size_row = bounds.map(|b| {
        xrow(
            Region::Inline,
            (
                label("Size")
                    .text_size(TextSize::Body.px())
                    .color(pal.text_muted),
                FlexSpacer::Flex(1.0),
                label(format!("{:.0} x {:.0}", b.width(), b.height()))
                    .text_size(TextSize::Body.px())
                    .color(pal.text),
            ),
        )
    });
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
                                field("X", app.coord_x_buf.clone(), 0),
                                field("Y", app.coord_y_buf.clone(), 1),
                            ),
                        )
                        .flex(1.0),
                    ),
                )
            }),
            (!app.collapsed.contains("Coordinates")).then_some(size_row),
        ),
    )
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
                        recipes::toggle(pal, "Comb".into(), view.comb, |app: &mut Workspace| {
                            app.view.comb = !app.view.comb;
                        }),
                        recipes::toggle(
                            pal,
                            "G0-G3".into(),
                            view.continuity,
                            |app: &mut Workspace| {
                                app.view.continuity = !app.view.continuity;
                            },
                        ),
                    ),
                )
            }),
        ),
    )
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
    let has_background = app
        .font
        .background_contours(&app.session.glyph_name)
        .is_some();
    let show = app.show_background;
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
                xrow(
                    Region::Inline,
                    (
                        recipes::toggle(
                            pal,
                            "Show".into(),
                            show && has_background,
                            |app: &mut Workspace| {
                                app.show_background = !app.show_background;
                            },
                        ),
                        recipes::action(pal, "Send".into(), |app: &mut Workspace| {
                            app.send_to_background();
                        }),
                        recipes::action(pal, "Swap".into(), |app: &mut Workspace| {
                            app.swap_background();
                        }),
                        recipes::action(pal, "Clear".into(), |app: &mut Workspace| {
                            app.clear_background();
                        }),
                    ),
                )
            }),
            (!app.collapsed.contains("Background")).then(|| {
                recipes::field(
                    pal,
                    "Reference glyph",
                    app.reference_buf.clone(),
                    |app: &mut Workspace, v| {
                        app.reference_buf = v;
                    },
                )
            }),
        ),
    )
}

pub(crate) fn mark_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let swatch = |label: Option<String>, color: xilem::Color| {
        sized_box(
            text_button("", move |app: &mut Workspace| app.set_mark(label.clone()))
                .background_color(color),
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
                "Mark",
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

/// The font's metadata, which is what the GPUI build's right panel
/// holds when no glyph is picked.
pub(crate) fn font_info_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let rows: Vec<_> = app
        .font
        .info_rows()
        .into_iter()
        .map(|(name, value)| recipes::kv(pal, name.to_string(), value))
        .collect();
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle(
                pal,
                "Font Info",
                !app.collapsed.contains("Font Info"),
                move |app: &mut Workspace| {
                    if !app.collapsed.remove("Font Info") {
                        app.collapsed.insert("Font Info");
                    }
                },
            ),
            (!app.collapsed.contains("Font Info")).then(|| xcolumn(Region::List, rows)),
        ),
    )
}

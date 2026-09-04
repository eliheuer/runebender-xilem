// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The inspector's font-wide sections: Dimensions, Kerning, Groups,
//! Compare, Features, and the editor's Related. The same sections as
//! `view/panels/editor_info.rs` in the GPUI build, in its order, so
//! the two inspectors read the same.
//!
//! Xilem 0.4 has no multi-line text view, so Features shows the file
//! and offers Generate and Apply; editing the text by hand waits for
//! a text area.

use crate::*;

/// A folding section header with its body, the way `info.rs` builds
/// the Glyph section.
fn section<V>(app: &Workspace, title: &'static str, body: V) -> impl WidgetView<Workspace> + use<V>
where
    V: WidgetView<Workspace> + 'static,
{
    let open = !app.collapsed.contains(title);
    xcolumn(
        Region::Section,
        (
            recipes::section_toggle(&app.palette, title, open, move |app: &mut Workspace| {
                if !app.collapsed.remove(title) {
                    app.collapsed.insert(title);
                }
            }),
            open.then_some(body),
        ),
    )
}

/// Chips on as many rows as the inspector's width takes. Xilem has no
/// wrapping row, so the rows are cut by an estimate of each chip's
/// width at the one type size.
fn chip_rows<F: Fn(&mut Workspace, &str) + Clone + Send + Sync + 'static>(
    pal: &Palette,
    names: &[String],
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    const WIDTH: f64 = 224.0;
    let width_of = |s: &str| 16.0 + 7.0 * s.chars().count() as f64;
    let mut rows: Vec<Vec<_>> = vec![Vec::new()];
    let mut used = 0.0;
    for name in names {
        let w = width_of(name);
        if used + w > WIDTH && !rows.last().is_some_and(Vec::is_empty) {
            rows.push(Vec::new());
            used = 0.0;
        }
        used += w + Space::Xs.px();
        let on_click = on_click.clone();
        let owned = name.clone();
        let chip = chip(pal, name.clone(), move |app: &mut Workspace| {
            on_click(app, &owned);
        });
        rows.last_mut().expect("one row").push(chip);
    }
    let rows: Vec<_> = rows
        .into_iter()
        .map(|row| xrow(Region::List, row))
        .collect();
    xcolumn(Region::List, rows)
}

/// A small keylined chip, the GPUI build's `px_1` rounded box.
fn chip<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    sized_box(
        button(
            label(text).text_size(TextSize::Body.px()).color(pal.text),
            move |app: &mut Workspace| on_click(app),
        )
        .padding(Space::Sm)
        .background_color(pal.panel)
        .border_color(pal.outline)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::Sm.length()),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::from(ControlSize::Row)))
}

/// Dimensions: the narrowest stem and bar of the reference glyphs.
pub(crate) fn dimensions_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use runebender_core::analysis::dimensions::{REFERENCE_GLYPHS, stem_and_bar};
    let pal = &app.palette;
    let fmt = |v: Option<i64>| {
        v.map(|v| v.to_string())
            .unwrap_or_else(|| "\u{2013}".into())
    };
    let rows: Vec<_> = REFERENCE_GLYPHS
        .iter()
        .filter_map(|name| {
            let (stem, bar) = stem_and_bar(&app.font.font, name);
            if stem.is_none() && bar.is_none() {
                return None;
            }
            Some(xrow(
                Region::Inline,
                (
                    sized_box(label(*name).text_size(TextSize::Body.px()).color(pal.text))
                        .dims(Dimensions::new(Dim::Fixed(Length::px(16.0)), Dim::Auto)),
                    label(format!("stem {}", fmt(stem)))
                        .text_size(TextSize::Body.px())
                        .color(pal.text_muted),
                    label(format!("bar {}", fmt(bar)))
                        .text_size(TextSize::Body.px())
                        .color(pal.text_muted),
                ),
            ))
        })
        .collect();
    let empty = rows.is_empty().then(|| {
        label("No reference glyphs with straight stems")
            .text_size(TextSize::Body.px())
            .color(pal.text_muted)
    });
    section(
        app,
        "Dimensions",
        xcolumn(Region::List, (xcolumn(Region::List, rows), empty)),
    )
}

/// Kerning: a filter, an editor row (first, second, value; Enter
/// sets), and the pairs, capped, each with a delete.
pub(crate) fn kerning_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    const CAP: usize = 200;
    let filter = app.kern_filter_buf.trim().to_lowercase();
    let mut pairs: Vec<(String, String, f64)> = Vec::new();
    let mut hidden = 0_usize;
    for (first, seconds) in app.font.font.kerning.iter() {
        for (second, value) in seconds.iter() {
            if !filter.is_empty()
                && !first.as_str().to_lowercase().contains(&filter)
                && !second.as_str().to_lowercase().contains(&filter)
            {
                continue;
            }
            if pairs.len() >= CAP {
                hidden += 1;
                continue;
            }
            pairs.push((first.to_string(), second.to_string(), *value));
        }
    }
    let total = pairs.len() + hidden;
    let is_group =
        |name: &str| name.starts_with("public.kern1.") || name.starts_with("public.kern2.");
    let short = |name: &str| {
        name.strip_prefix("public.kern1.")
            .or_else(|| name.strip_prefix("public.kern2."))
            .map(|g| format!("@{g}"))
            .unwrap_or_else(|| name.to_string())
    };
    let rows: Vec<_> = pairs
        .iter()
        .map(|(first, second, value)| {
            let exception = !is_group(first) || !is_group(second);
            let (f2, s2) = (first.clone(), second.clone());
            let (f3, s3, v3) = (first.clone(), second.clone(), *value);
            xrow(
                Region::Inline,
                (
                    sized_box(
                        button(
                            label(format!("{} \u{00b7} {}", short(first), short(second)))
                                .text_size(TextSize::Body.px())
                                .color(if exception {
                                    pal.role("warning")
                                } else {
                                    pal.text
                                }),
                            // Loads the pair into the editor row, so
                            // adjusting one is click, type, Enter.
                            move |app: &mut Workspace| {
                                app.kern_first_buf = f3.clone();
                                app.kern_second_buf = s3.clone();
                                app.kern_value_buf = format!("{v3}");
                            },
                        )
                        .background_color(pal.panel)
                        .border_width(Stroke::None.length())
                        .padding(Space::None),
                    )
                    .dims(Dimensions::new(Dim::Fixed(Length::px(150.0)), Dim::Auto)),
                    label(format!("{value:.0}"))
                        .text_size(TextSize::Body.px())
                        .color(pal.text_muted),
                    chip(pal, "\u{00d7}".into(), move |app: &mut Workspace| {
                        app.delete_kern_pair(&f2, &s2);
                    }),
                ),
            )
        })
        .collect();
    // Fixed widths: a field left to size to its content pushes the
    // whole inspector past its column, as the Glyph section found.
    fn narrow<V: WidgetView<Workspace> + 'static>(v: V) -> impl WidgetView<Workspace> + use<V> {
        sized_box(v).dims(Dimensions::new(Dim::Fixed(Length::px(68.0)), Dim::Auto))
    }
    let editor_row = xrow(
        Region::Inline,
        (
            narrow(recipes::field_enter(
                pal,
                "",
                app.kern_first_buf.clone(),
                |app: &mut Workspace, v| app.kern_first_buf = v,
                |app: &mut Workspace, _| app.set_kern_pair_from_bufs(),
            )),
            narrow(recipes::field_enter(
                pal,
                "",
                app.kern_second_buf.clone(),
                |app: &mut Workspace, v| app.kern_second_buf = v,
                |app: &mut Workspace, _| app.set_kern_pair_from_bufs(),
            )),
            narrow(recipes::field_enter(
                pal,
                "",
                app.kern_value_buf.clone(),
                |app: &mut Workspace, v| app.kern_value_buf = v,
                |app: &mut Workspace, _| app.set_kern_pair_from_bufs(),
            )),
        ),
    );
    section(
        app,
        "Kerning",
        xcolumn(
            Region::List,
            (
                recipes::field(
                    pal,
                    "",
                    app.kern_filter_buf.clone(),
                    |app: &mut Workspace, v| {
                        app.kern_filter_buf = v;
                    },
                ),
                editor_row,
                sized_box(portal(xcolumn(Region::List, rows)))
                    .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(220.0)))),
                label(if hidden > 0 {
                    format!("{total} pairs \u{00b7} showing {CAP}")
                } else {
                    format!("{total} pairs")
                })
                .text_size(TextSize::Body.px())
                .color(pal.text_muted),
            ),
        ),
    )
}

/// Groups: a name field, then each kerning group as chips. A chip
/// removes its member; "+ sel" adds the grid selection.
pub(crate) fn groups_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let mut rows: Vec<_> = Vec::new();
    let mut shown = 0_usize;
    for (full, members) in app.font.font.groups.iter() {
        let name = full.as_str();
        let (side, short) = if let Some(s) = name.strip_prefix("public.kern1.") {
            ("L", s)
        } else if let Some(s) = name.strip_prefix("public.kern2.") {
            ("R", s)
        } else {
            continue;
        };
        shown += 1;
        if shown > 40 {
            break;
        }
        let full_owned = name.to_string();
        let short_owned = short.to_string();
        let side_first = side == "L";
        let names: Vec<String> = members.iter().take(24).map(|m| m.to_string()).collect();
        let chips = chip_rows(pal, &names, move |app: &mut Workspace, member| {
            app.remove_from_group(&full_owned, member);
        });
        let more = (members.len() > 24).then(|| {
            label(format!("+{}", members.len() - 24))
                .text_size(TextSize::Body.px())
                .color(pal.text_muted)
        });
        rows.push(xcolumn(
            Region::List,
            (
                xrow(
                    Region::Inline,
                    (
                        label(format!("@{short} \u{00b7} {side}"))
                            .text_size(TextSize::Body.px())
                            .color(pal.text),
                        chip(pal, "+ sel".into(), move |app: &mut Workspace| {
                            app.add_selection_to_group(side_first, &short_owned);
                        }),
                    ),
                ),
                chips,
                more,
            ),
        ));
    }
    section(
        app,
        "Groups",
        xcolumn(
            Region::List,
            (
                recipes::field_enter(
                    pal,
                    "",
                    app.group_name_buf.clone(),
                    |app: &mut Workspace, v| app.group_name_buf = v,
                    |app: &mut Workspace, v| {
                        app.group_name_buf = v;
                        app.new_group_from_buf();
                    },
                ),
                xcolumn(Region::List, rows),
                label("Chip removes \u{00b7} + sel adds the grid selection")
                    .text_size(TextSize::Body.px())
                    .color(pal.text_muted),
            ),
        ),
    )
}

/// One vertical metric off a font's info.
type Pick = fn(&norad::FontInfo) -> Option<f64>;

/// Compare: each other master against the active one.
pub(crate) fn compare_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    use xilem::core::one_of::Either;
    let masters = app.font.master_count();
    if masters < 2 {
        return section(
            app,
            "Compare",
            Either::A(
                label("One master \u{00b7} nothing to compare")
                    .text_size(TextSize::Body.px())
                    .color(pal.text_muted),
            ),
        );
    }
    let active = app.font.active;
    let reference = &app.font.font;
    let pair_count = |f: &norad::Font| f.kerning.values().map(|s| s.len()).sum::<usize>();
    let metric = |f: &norad::Font, pick: Pick| pick(&f.font_info).unwrap_or(0.0);
    let rows: Vec<_> = (0..masters)
        .filter(|&i| i != active)
        .filter_map(|i| {
            let master = app.font.master_font(i)?;
            let missing = reference
                .default_layer()
                .iter()
                .filter(|g| master.get_glyph(g.name()).is_none())
                .count();
            let advance_diffs = reference
                .default_layer()
                .iter()
                .filter(|g| {
                    master
                        .get_glyph(g.name())
                        .is_some_and(|m| (m.width - g.width).abs() > 0.5)
                })
                .count();
            let mut diffs: Vec<&str> = Vec::new();
            let checks: [(&str, Pick); 4] = [
                ("asc", |fi| fi.ascender),
                ("desc", |fi| fi.descender),
                ("xh", |fi| fi.x_height),
                ("cap", |fi| fi.cap_height),
            ];
            for (tag, pick) in checks {
                if (metric(master, pick) - metric(reference, pick)).abs() > 0.5 {
                    diffs.push(tag);
                }
            }
            Some(xcolumn(
                Region::List,
                (
                    label(format!(
                        "{} vs {}",
                        app.font.master_names[i], app.font.master_names[active]
                    ))
                    .text_size(TextSize::Body.px())
                    .color(pal.text),
                    // One fact a line: a label does not wrap, and one
                    // long line would push the whole inspector wide.
                    xcolumn(
                        Region::List,
                        [
                            format!(
                                "{} glyphs \u{00b7} {} missing",
                                master.default_layer().len(),
                                missing
                            ),
                            format!("{advance_diffs} advance diffs"),
                            format!(
                                "kerning {} vs {}",
                                pair_count(master),
                                pair_count(reference)
                            ),
                            if diffs.is_empty() {
                                "metrics match".to_string()
                            } else {
                                format!("metrics differ: {}", diffs.join(", "))
                            },
                        ]
                        .into_iter()
                        .map(|line| {
                            label(line)
                                .text_size(TextSize::Body.px())
                                .color(pal.text_muted)
                        })
                        .collect::<Vec<_>>(),
                    ),
                ),
            ))
        })
        .collect();
    let incompatible = app.font.incompatible_count();
    section(
        app,
        "Compare",
        Either::B(xcolumn(
            Region::List,
            (
                xcolumn(Region::List, rows),
                label(format!("{incompatible} structurally incompatible glyph(s)"))
                    .text_size(TextSize::Body.px())
                    .color(if incompatible == 0 {
                        pal.text_muted
                    } else {
                        pal.role("warning")
                    }),
            ),
        )),
    )
}

/// Features: the font's feature file, with Generate (the mark and
/// mkmk lookups core derives from anchors) and Apply. Read-only text
/// until Xilem has a text area.
pub(crate) fn features_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let text = if app.font.font.features.trim().is_empty() {
        "No feature file".to_string()
    } else {
        app.font.font.features.clone()
    };
    section(
        app,
        "Features",
        xcolumn(
            Region::List,
            (
                sized_box(portal(
                    label(text).text_size(TextSize::Body.px()).color(pal.text),
                ))
                .dims(Dimensions::new(Dim::Stretch, Dim::Fixed(Length::px(260.0))))
                .background_color(pal.field())
                .border_color(pal.field_outline)
                .border_width(Stroke::Hairline.length())
                .corner_radius(Radius::Sm.length()),
                xrow(
                    Region::Inline,
                    (
                        chip(pal, "Generate".into(), |app: &mut Workspace| {
                            app.generate_features();
                        }),
                        chip(pal, "Apply".into(), |app: &mut Workspace| {
                            app.apply_features();
                        }),
                        app.features_status.clone().map(|s| {
                            label(s)
                                .text_size(TextSize::Body.px())
                                .color(pal.text_muted)
                        }),
                    ),
                ),
            ),
        ),
    )
}

/// Related: the open glyph's components, its suffix siblings, and the
/// composites that place it, as chips that open the glyph.
pub(crate) fn related_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let name = app.session.glyph_name.clone();
    let stem = name.split('.').next().unwrap_or(&name).to_string();
    let mut groups: Vec<(&'static str, Vec<String>)> = Vec::new();
    let components: Vec<String> = app
        .font
        .font
        .get_glyph(name.as_str())
        .map(|g| g.components.iter().map(|c| c.base.to_string()).collect())
        .unwrap_or_default();
    if !components.is_empty() {
        groups.push(("Components", components));
    }
    let siblings: Vec<String> = app
        .font
        .glyphs
        .iter()
        .map(|g| g.name.clone())
        .filter(|other| *other != name && other.split('.').next() == Some(stem.as_str()))
        .take(24)
        .collect();
    if !siblings.is_empty() {
        groups.push(("Siblings", siblings));
    }
    let used_by: Vec<String> = app
        .font
        .glyphs
        .iter()
        .filter(|g| {
            app.font
                .font
                .get_glyph(g.name.as_str())
                .is_some_and(|n| n.components.iter().any(|c| c.base.as_str() == name))
        })
        .map(|g| g.name.clone())
        .take(24)
        .collect();
    if !used_by.is_empty() {
        groups.push(("Used by", used_by));
    }
    let empty = groups.is_empty().then(|| {
        label("No related glyphs")
            .text_size(TextSize::Body.px())
            .color(pal.text_muted)
    });
    let rows: Vec<_> = groups
        .into_iter()
        .map(|(title, names)| {
            let chips = chip_rows(pal, &names, |app: &mut Workspace, related| {
                if let Some(target) = app.font.index_of(related) {
                    app.open_glyph(target);
                }
            });
            xcolumn(
                Region::List,
                (
                    label(title)
                        .text_size(TextSize::Body.px())
                        .color(pal.text_muted),
                    chips,
                ),
            )
        })
        .collect();
    section(
        app,
        "Related",
        xcolumn(Region::List, (xcolumn(Region::List, rows), empty)),
    )
}

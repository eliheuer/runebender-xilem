// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! The inspector's font-wide sections: Dimensions, Kerning, Groups,
//! Compare, Features, and the editor's Related.
//!
//! Xilem 0.4 has no multi-line text view, so Features shows the file
//! and offers Generate and Apply; editing the text by hand waits for
//! a text area.

use crate::application::view::design::{
    ControlSize, Radius, Region, Space, Stroke, TextSize, column as xcolumn, row as xrow,
};
use crate::application::view::recipes::button;
use crate::application::view::theme::Palette;
use crate::application::view::{design, label, recipes, text_input};
use crate::application::widgets::scroll_viewport::portal;
use crate::application::workspace::Workspace;
use masonry::layout::{Dim, Length};
use masonry::properties::Dimensions;
use xilem::WidgetView;
use xilem::style::Style;
use xilem::view::FlexExt as _;
use xilem::view::sized_box;

/// A folding section header with its body, the way `info.rs` builds
/// the Glyph section.
fn section<V>(
    app: &Workspace,
    title: &'static str,
    body: V,
) -> impl WidgetView<Workspace, Widget = masonry::widgets::Flex> + use<V>
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

/// Group shelves use square, full-line chips; other chip consumers retain
/// their existing compact treatment.
#[derive(Clone, Copy)]
enum ChipStyle {
    Compact,
    GroupMember,
    GroupAdd,
}

/// Chips on as many rows as the inspector's width takes. Xilem has no
/// wrapping row, so the rows are cut by an estimate of each chip's
/// width at the one type size.
fn chip_rows<F: Fn(&mut Workspace, &str) + Clone + Send + Sync + 'static>(
    pal: &Palette,
    names: &[String],
    style: ChipStyle,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    const WIDTH: f64 = 224.0;
    let gap = match style {
        ChipStyle::Compact => Space::Xs,
        ChipStyle::GroupMember | ChipStyle::GroupAdd => Space::Sm,
    };
    let width_of = |s: &str| 16.0 + 7.0 * s.chars().count() as f64;
    let mut rows: Vec<Vec<_>> = vec![Vec::new()];
    let mut used = 0.0;
    for name in names {
        let w = width_of(name);
        if used + w > WIDTH && !rows.last().is_some_and(Vec::is_empty) {
            rows.push(Vec::new());
            used = 0.0;
        }
        used += w + gap.px();
        let on_click = on_click.clone();
        let owned = name.clone();
        let chip = styled_chip(pal, name.clone(), style, move |app: &mut Workspace| {
            on_click(app, &owned);
        });
        rows.last_mut().expect("one row").push(chip);
    }
    let rows: Vec<_> = rows
        .into_iter()
        .map(|row| xrow(Region::Inline, row).gap(gap))
        .collect();
    xcolumn(Region::List, rows).gap(gap)
}

/// A small keylined chip using the application's square control silhouette.
fn chip<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    styled_chip(pal, text, ChipStyle::Compact, on_click)
}

fn styled_chip<F: Fn(&mut Workspace) + Send + Sync + 'static>(
    pal: &Palette,
    text: String,
    style: ChipStyle,
    on_click: F,
) -> impl WidgetView<Workspace> + use<F> {
    let (height, ink) = match style {
        ChipStyle::Compact => (ControlSize::Row.px(), pal.text),
        ChipStyle::GroupMember => (design::GROUP_CHIP_HEIGHT, pal.text),
        ChipStyle::GroupAdd => (design::GROUP_CHIP_HEIGHT, pal.text_muted),
    };
    sized_box(
        button(
            label(text).text_size(TextSize::Body.px()).color(ink),
            move |app: &mut Workspace| on_click(app),
        )
        .padding(Space::Sm)
        .background_color(pal.panel)
        .border_color(pal.outline)
        .border_width(Stroke::Hairline.length())
        .corner_radius(Radius::None.length()),
    )
    .dims(Dimensions::new(Dim::Auto, Dim::Fixed(Length::px(height))))
}

/// Dimensions: the narrowest stem and bar of the reference glyphs.
pub(crate) fn dimensions_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    use runebender::analysis::dimensions::{REFERENCE_GLYPHS, stem_and_bar};
    let pal = &app.palette;
    let fmt = |v: Option<i64>| {
        v.map(|v| v.to_string())
            .unwrap_or_else(|| "\u{2013}".into())
    };
    let rows: Vec<_> = REFERENCE_GLYPHS
        .iter()
        .filter_map(|name| {
            let (stem, bar) = stem_and_bar(app.font.font(), name);
            if stem.is_none() && bar.is_none() {
                return None;
            }
            Some(
                xrow(
                    Region::Inline,
                    (
                        sized_box(label(*name).text_size(TextSize::Body.px()).color(pal.text))
                            .dims(Dimensions::new(
                                Dim::Fixed(Length::px(design::DIMENSIONS_GLYPH_LABEL_WIDTH)),
                                Dim::Auto,
                            )),
                        label(format!("stem {}", fmt(stem)))
                            .text_size(TextSize::Body.px())
                            .color(pal.text_muted),
                        label(format!("bar {}", fmt(bar)))
                            .text_size(TextSize::Body.px())
                            .color(pal.text_muted),
                    ),
                )
                .gap(Space::Md)
                .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row))),
            )
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
    .gap(Space::Sm)
}

/// Kerning: a filter, an editor row (first, second, value; Enter
/// sets), and the pairs, capped, each with a delete.
pub(crate) fn kerning_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    const CAP: usize = 200;
    let filter = app.kern_filter_buf.trim().to_lowercase();
    let mut pairs: Vec<(String, String, f64)> = Vec::new();
    let mut hidden = 0_usize;
    for (first, seconds) in app.font.font().kerning.iter() {
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
    let short = |name: &str| {
        name.strip_prefix("public.kern1.")
            .or_else(|| name.strip_prefix("public.kern2."))
            .map(|g| format!("@{g}"))
            .unwrap_or_else(|| name.to_string())
    };
    let rows: Vec<_> = pairs
        .iter()
        .map(|(first, second, value)| {
            let (f2, s2) = (first.clone(), second.clone());
            let (f3, s3, v3) = (first.clone(), second.clone(), *value);
            xrow(
                Region::Inline,
                (
                    button(
                        xrow(
                            Region::Inline,
                            (
                                label(format!("{} \u{00b7} {}", short(first), short(second)))
                                    .text_size(TextSize::Body.px())
                                    .color(pal.text)
                                    .prop(masonry::properties::LineBreaking::Clip)
                                    .dims(Dimensions::new(Dim::Fixed(Length::ZERO), Dim::Auto))
                                    .flex(1.0),
                                label(format!("{value:.0}"))
                                    .text_size(TextSize::Body.px())
                                    .color(pal.text_muted),
                            ),
                        ),
                        // The name and value both load the pair into the editor.
                        move |app: &mut Workspace| {
                            app.kern_first_buf = f3.clone();
                            app.kern_second_buf = s3.clone();
                            app.kern_value_buf = format!("{v3}");
                        },
                    )
                    .background_color(pal.panel)
                    .border_width(Stroke::None.length())
                    .padding(Space::None)
                    .flex(1.0),
                    button(
                        label("\u{00d7}")
                            .text_size(TextSize::Body.px())
                            .color(pal.text_muted),
                        move |app: &mut Workspace| app.delete_kern_pair(&f2, &s2),
                    )
                    .background_color(pal.panel)
                    .border_width(Stroke::None.length())
                    .padding(masonry::properties::Padding::from_vh(
                        Space::None.length(),
                        Space::Sm.length(),
                    )),
                ),
            )
            .padding(masonry::properties::Padding::from_vh(
                Space::Xs.length(),
                Space::Sm.length(),
            ))
            .dims(Dimensions::new(
                Dim::Stretch,
                Dim::Fixed(Length::px(design::KERN_PAIR_ROW_HEIGHT)),
            ))
        })
        .collect();
    // Equal flexible slots follow a resized dock without letting the text's
    // intrinsic width force the inspector wider than its splitter allows.
    let editor_row = xrow(
        Region::Inline,
        (
            recipes::field_bare(
                pal,
                "First",
                app.kern_first_buf.clone(),
                |app: &mut Workspace, v| app.kern_first_buf = v,
                |app: &mut Workspace, _| app.set_kern_pair_from_bufs(),
            )
            .flex(1.0),
            recipes::field_bare(
                pal,
                "Second",
                app.kern_second_buf.clone(),
                |app: &mut Workspace, v| app.kern_second_buf = v,
                |app: &mut Workspace, _| app.set_kern_pair_from_bufs(),
            )
            .flex(1.0),
            recipes::field_bare(
                pal,
                "Value",
                app.kern_value_buf.clone(),
                |app: &mut Workspace, v| app.kern_value_buf = v,
                |app: &mut Workspace, _| app.set_kern_pair_from_bufs(),
            )
            .flex(1.0),
        ),
    );
    section(
        app,
        "Kerning",
        xcolumn(
            Region::Inline,
            (
                recipes::field_bare(
                    pal,
                    "Filter pairs",
                    app.kern_filter_buf.clone(),
                    |app: &mut Workspace, v| {
                        app.kern_filter_buf = v;
                    },
                    |_: &mut Workspace, _| {},
                ),
                editor_row,
                sized_box(
                    portal(xcolumn(Region::List, rows).gap(Space::None)).constrain_horizontal(true),
                )
                .dims(Dimensions::new(
                    Dim::Stretch,
                    Dim::Fixed(Length::px(
                        (pairs.len() as f64 * design::KERN_PAIR_ROW_HEIGHT)
                            .min(design::KERN_LIST_MAX_HEIGHT),
                    )),
                )),
                sized_box(
                    label(if hidden > 0 {
                        format!("{total} pairs \u{00b7} showing {CAP}")
                    } else {
                        format!("{total} pairs")
                    })
                    .text_size(TextSize::Body.px())
                    .color(pal.text_muted),
                )
                .dims(Dimensions::new(Dim::Stretch, Dim::from(ControlSize::Row))),
            ),
        ),
    )
    .gap(Space::Sm)
}

/// Groups: a name field, then each kerning group as chips. A chip
/// removes its member; "+ sel" adds the grid selection.
pub(crate) fn groups_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    let mut rows: Vec<_> = Vec::new();
    let mut shown = 0_usize;
    for (full, members) in app.font.font().groups.iter() {
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
        let chips = chip_rows(
            pal,
            &names,
            ChipStyle::GroupMember,
            move |app: &mut Workspace, member| {
                app.remove_from_group(&full_owned, member);
            },
        );
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
                        styled_chip(
                            pal,
                            "+ sel".into(),
                            ChipStyle::GroupAdd,
                            move |app: &mut Workspace| {
                                app.add_selection_to_group(side_first, &short_owned);
                            },
                        ),
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
            Region::Section,
            (
                recipes::field_bare(
                    pal,
                    "new group · o or |o",
                    app.group_name_buf.clone(),
                    |app: &mut Workspace, v| app.group_name_buf = v,
                    |app: &mut Workspace, v| {
                        app.group_name_buf = v;
                        app.new_group_from_buf();
                    },
                ),
                xcolumn(Region::Inline, rows),
                label("Chip removes \u{00b7} + sel adds the grid selection")
                    .text_size(TextSize::Body.px())
                    .color(pal.text_muted),
            ),
        ),
    )
    .gap(Space::Sm)
}

/// One vertical metric off canonical source information.
type Pick = fn(&runebender::document::model::font_info::CanonicalFontMetrics) -> Option<f64>;

/// Compare: each other master against the active one.
pub(crate) fn compare_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    use xilem::core::one_of::Either;
    let readout = |text: String, ink| {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the integral control-height token is exactly representable as f32"
        )]
        let line_height = ControlSize::Row.px() as f32;
        label(text)
            .text_size(TextSize::Body.px())
            .line_height(masonry::parley::LineHeight::Absolute(line_height))
            .color(ink)
            .prop(masonry::properties::LineBreaking::WordWrap)
            .dims(Dimensions::new(Dim::Stretch, Dim::Auto))
    };
    let masters = app.font.master_count();
    if masters < 2 {
        return section(
            app,
            "Compare",
            Either::A(readout(
                "One master \u{00b7} nothing to compare".into(),
                pal.text_muted,
            )),
        )
        .gap(Space::Sm);
    }
    let active = app.font.active();
    let reference = app.font.font();
    let reference_info = app
        .font
        .font_info_at(active)
        .expect("the active source has canonical font information");
    let pair_count = |index| {
        app.font
            .font_metadata_at(index)
            .map_or(0, |metadata| metadata.kerning_pairs().count())
    };
    let metric = |index, pick: Pick| {
        app.font
            .font_info_at(index)
            .and_then(|info| pick(&info.metrics))
            .unwrap_or(0.0)
    };
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
                if (metric(i, pick) - pick(&reference_info.metrics).unwrap_or(0.0)).abs() > 0.5 {
                    diffs.push(tag);
                }
            }
            Some(xcolumn(
                Region::List,
                (
                    readout(
                        format!("{} vs {}", app.font.master_name(i), app.font.master_name(active)),
                        pal.text,
                    ),
                    readout(
                        format!(
                            "{} glyphs \u{00b7} {} missing \u{00b7} {} advance diffs \u{00b7} kerning {} vs {}{}",
                            master.default_layer().len(),
                            missing,
                            advance_diffs,
                            pair_count(i),
                            pair_count(active),
                            if diffs.is_empty() {
                                String::new()
                            } else {
                                format!(" \u{00b7} metrics differ: {}", diffs.join(", "))
                            },
                        ),
                        pal.text_muted,
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
            Region::Inline,
            (
                xcolumn(Region::Inline, rows),
                readout(
                    format!("{incompatible} structurally incompatible glyph(s)"),
                    if incompatible == 0 {
                        pal.text_muted
                    } else {
                        pal.role("warning")
                    },
                ),
            ),
        )),
    )
    .gap(Space::Sm)
}

/// Features: an editable `features.fea` draft, with explicit Apply/Revert and
/// generation of mark and mkmk lookups from anchors.
pub(crate) fn features_section(app: &Workspace) -> impl WidgetView<Workspace> + use<> {
    let pal = &app.palette;
    section(
        app,
        "Features",
        xcolumn(
            Region::List,
            (
                sized_box(portal(
                    text_input(app.features_buf.clone(), |app: &mut Workspace, value| {
                        app.edit_features(value);
                    })
                    .insert_newline(masonry::widgets::InsertNewline::OnEnter)
                    .text_color(pal.text)
                    .clip(true)
                    .background_color(pal.field())
                    .border_color(pal.field_outline)
                    .border_width(Stroke::Hairline.length())
                    .corner_radius(Radius::Sm.length()),
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
                        chip(pal, "Revert".into(), |app: &mut Workspace| {
                            app.revert_features();
                        }),
                        chip(pal, "Check".into(), |app: &mut Workspace| {
                            app.check_features();
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
        .font()
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
                .font()
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
            let chips = chip_rows(
                pal,
                &names,
                ChipStyle::Compact,
                |app: &mut Workspace, related| {
                    if let Some(target) = app.font.index_of(related) {
                        app.open_glyph(target);
                    }
                },
            );
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

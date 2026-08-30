// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Tests for the text buffer.

use super::*;

/// Typing after a seeded active sort must lay out to the right
/// of it, active index stable (regression for a GPUI overlap).
#[test]
fn typing_after_active_sort_lays_out_beside_it() {
    let path = crate::testing::fonts::regular_ufo();
    let font = norad::Font::load(path).expect("fixture UFO loads");
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(TextGlyphInventory::from_font(&font));
    buffer.set_kerning_model(TextKerningModel::from_font(&font));

    let one = font.get_glyph("one").unwrap();
    buffer.insert_glyph("one", Some('1'), one.width);
    buffer.activate_sort(0);
    assert_eq!(buffer.cursor(), 1);

    assert!(buffer.insert_character('a'));
    assert!(buffer.insert_character('s'));

    assert_eq!(buffer.active_sort(), Some(0), "active sort must stay");
    let layout = buffer.layout(1200.0);
    assert_eq!(layout.items.len(), 3);
    let one_item = layout.items.iter().find(|i| i.index == 0).unwrap();
    let a_item = layout.items.iter().find(|i| i.index == 1).unwrap();
    let s_item = layout.items.iter().find(|i| i.index == 2).unwrap();
    assert_eq!(one_item.x, 0.0);
    let kern_one_a = crate::document::font_ops::kern_value(&font, "one", "a");
    assert!(
        (a_item.x - (one.width + kern_one_a)).abs() < 1e-6,
        "a at {} expected {}",
        a_item.x,
        one.width + kern_one_a
    );
    assert!(a_item.x >= one.width - 100.0, "a must not overlap one");
    assert!(s_item.x > a_item.x);
}

/// The native constructors must agree with the norad-level
/// kerning resolution the editors already use (glyph_ops).
#[test]
fn from_font_builds_working_models() {
    let path = crate::testing::fonts::regular_ufo();
    let font = norad::Font::load(path).expect("fixture UFO loads");
    let inventory = TextGlyphInventory::from_font(&font);
    let kerning = TextKerningModel::from_font(&font);

    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(inventory);
    buffer.set_kerning_model(kerning);

    // Characters resolve to glyphs with real advances.
    assert!(buffer.insert_character('A'));
    assert!(buffer.insert_character('B'));
    let a_width = font.get_glyph("A").unwrap().width;
    let layout = buffer.layout(1200.0);
    assert_eq!(layout.items.len(), 2);
    assert_eq!(layout.items[0].advance_width, a_width);

    // Find an encoded pair with a nonzero kern and check the
    // buffer applies exactly what glyph_ops resolves.
    let mut checked = false;
    'outer: for glyph in font.default_layer().iter() {
        let Some(lc) = glyph.codepoints.iter().next() else {
            continue;
        };
        for other in font.default_layer().iter() {
            let Some(rc) = other.codepoints.iter().next() else {
                continue;
            };
            let expected = crate::document::font_ops::kern_value(
                &font,
                glyph.name().as_str(),
                other.name().as_str(),
            );
            if expected == 0.0 {
                continue;
            }
            let mut b = TextBuffer::new();
            b.set_glyph_inventory(TextGlyphInventory::from_font(&font));
            b.set_kerning_model(TextKerningModel::from_font(&font));
            assert!(b.insert_character(lc));
            assert!(b.insert_character(rc));
            let l = b.layout(1200.0);
            let gap = l.items[1].x - l.items[0].x - l.items[0].advance_width;
            assert!(
                (gap - expected).abs() < 1e-6,
                "{}/{}: gap {gap} expected {expected}",
                glyph.name(),
                other.name()
            );
            checked = true;
            break 'outer;
        }
    }
    assert!(checked, "fixture font has no kerned encoded pair");
}

#[test]
fn insert_glyph_moves_cursor_and_sets_active_sort() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);

    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer.cursor(), 2);
    assert_eq!(buffer.active_sort(), Some(1));
    assert_eq!(
        buffer.iter().last().and_then(TextSort::glyph_name),
        Some("B")
    );
}

#[test]
fn insert_inactive_glyph_preserves_active_sort() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.set_cursor(0);
    buffer.insert_inactive_glyph("B", Some('B'), 610.0);

    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(buffer.active_sort(), Some(1));
    assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("B"));
    assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("A"));
}

#[test]
fn activate_sort_preserves_cursor_position() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);
    buffer.set_cursor(0);

    assert!(buffer.activate_sort(1));

    assert_eq!(buffer.active_sort(), Some(1));
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn active_sort_flags_remain_unique_after_switch_and_insert() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);
    buffer.insert_glyph("C", Some('C'), 620.0);

    assert!(buffer.activate_sort(0));
    assert_eq!(
        buffer
            .iter()
            .enumerate()
            .filter_map(|(index, sort)| sort.active.then_some(index))
            .collect::<Vec<_>>(),
        vec![0]
    );

    assert!(buffer.activate_sort(2));
    assert_eq!(
        buffer
            .iter()
            .enumerate()
            .filter_map(|(index, sort)| sort.active.then_some(index))
            .collect::<Vec<_>>(),
        vec![2]
    );

    buffer.set_cursor(0);
    buffer.insert_glyph("D", Some('D'), 630.0);
    assert_eq!(
        buffer
            .iter()
            .enumerate()
            .filter_map(|(index, sort)| sort.active.then_some(index))
            .collect::<Vec<_>>(),
        vec![0]
    );
}

#[test]
fn insert_character_uses_glyph_inventory() {
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": { "65": "A" },
                "widths": { "A": 640 }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('A'));
    assert!(!buffer.insert_character('Z'));

    assert_eq!(buffer.len(), 1);
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(buffer.active_sort(), None);
    assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
    let TextSortKind::Glyph {
        codepoint,
        advance_width,
        ..
    } = &buffer.sort(0).expect("sort exists").kind
    else {
        panic!("expected glyph sort");
    };
    assert_eq!(*codepoint, Some('A'));
    assert_eq!(*advance_width, 640.0);
}

#[test]
fn insert_character_missing_width_uses_xilem_shaper_fallback() {
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": { "65": "A" },
                "outlines": { "A": "M0,0 L10,0" }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('A'));

    let TextSortKind::Glyph { advance_width, .. } = &buffer.sort(0).expect("sort exists").kind
    else {
        panic!("expected glyph sort");
    };
    assert_eq!(*advance_width, 500.0);
}

#[test]
fn clear_resets_direction_like_fresh_xilem_session() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("A", Some('A'), 600.0);

    buffer.clear();

    assert_eq!(buffer.direction(), TextDirection::LeftToRight);
    assert_eq!(buffer.len(), 0);
    assert_eq!(buffer.cursor(), 0);
    assert_eq!(buffer.active_sort(), None);
}

#[test]
fn auto_direction_shapes_arabic_without_pinning_rtl() {
    let mut buffer = TextBuffer::new();
    // No set_direction call: Auto mode must shape Arabic on its own.
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1605": "meem-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "meem-ar": 520,
                    "meem-ar.fina": 500
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0645}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("meem-ar.fina")
    );
}

#[test]
fn insert_character_shapes_rtl_arabic_neighbors() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1605": "meem-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "meem-ar": 520,
                    "meem-ar.fina": 500
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0645}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("meem-ar.fina")
    );
}

#[test]
fn rtl_arabic_shaping_context_crosses_line_breaks_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    buffer.insert_line_break();
    assert!(buffer.insert_character('\u{0647}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar")
    );
    assert!(matches!(
        buffer.sort(1).map(|sort| &sort.kind),
        Some(TextSortKind::LineBreak)
    ));
    assert_eq!(
        buffer.sort(2).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn rtl_arabic_insert_after_transparent_mark_reshapes_previous_letter() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1614": "fatha-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "fatha-ar": 0,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{064e}'));
    assert!(buffer.insert_character('\u{0647}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("fatha-ar")
    );
    assert_eq!(
        buffer.sort(2).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn rtl_arabic_tatweel_joins_neighbors_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1600": "tatweel-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "tatweel-ar": 250,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0640}'));
    assert!(buffer.insert_character('\u{0647}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("tatweel-ar")
    );
    assert_eq!(
        buffer.sort(2).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn rtl_arabic_positional_glyph_can_exist_without_width_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                },
                "outlines": {
                    "beh-ar.init": "M0,0 L10,0"
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0647}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn rtl_arabic_delete_transparent_mark_repairs_joining_neighbors() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1614": "fatha-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "fatha-ar": 0,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.insert_glyph("fatha-ar", Some('\u{064e}'), 0.0);
    buffer.insert_glyph("heh-ar", Some('\u{0647}'), 510.0);
    buffer.set_cursor(2);

    assert!(buffer.delete_before_cursor().is_some());
    assert!(buffer.shape_arabic_around_if_rtl(buffer.cursor()));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn rtl_arabic_insert_right_joining_sort_reshapes_next_letter() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1575": "alef-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "alef-ar": 450,
                    "alef-ar.fina": 430,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0647}'));
    buffer.set_cursor(1);
    assert!(buffer.insert_character('\u{0627}'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("alef-ar.fina")
    );
    assert_eq!(
        buffer.sort(2).and_then(TextSort::glyph_name),
        Some("heh-ar")
    );
}

#[test]
fn rtl_arabic_insert_latin_separator_breaks_joining_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "65": "A",
                    "1576": "beh-ar",
                    "1605": "meem-ar"
                },
                "widths": {
                    "A": 700,
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "meem-ar": 520,
                    "meem-ar.fina": 500
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0645}'));
    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("meem-ar.fina")
    );

    buffer.set_cursor(1);
    assert!(buffer.insert_character('A'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar")
    );
    assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("A"));
    assert_eq!(
        buffer.sort(2).and_then(TextSort::glyph_name),
        Some("meem-ar")
    );
}

#[test]
fn rtl_arabic_delete_latin_separator_repairs_joining_neighbors() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "65": "A",
                    "1576": "beh-ar",
                    "1605": "meem-ar"
                },
                "widths": {
                    "A": 700,
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "meem-ar": 520,
                    "meem-ar.fina": 500
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('A'));
    assert!(buffer.insert_character('\u{0645}'));
    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar")
    );
    assert_eq!(
        buffer.sort(2).and_then(TextSort::glyph_name),
        Some("meem-ar")
    );

    buffer.set_cursor(2);
    assert!(buffer.delete_before_cursor().is_some());
    assert!(buffer.shape_arabic_around_if_rtl(buffer.cursor()));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("meem-ar.fina")
    );
}

#[test]
fn insert_character_ltr_preserves_existing_shaped_forms() {
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "65": "A",
                    "1576": "beh-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "A": 700,
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );
    buffer.set_direction(TextDirection::RightToLeft);
    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0647}'));
    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );

    buffer.set_direction(TextDirection::LeftToRight);
    assert!(buffer.insert_character('A'));

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
    assert_eq!(buffer.sort(2).and_then(TextSort::glyph_name), Some("A"));
}

#[test]
fn delete_before_cursor_updates_active_sort() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);
    buffer.activate_sort(1);
    buffer.set_cursor(1);

    let deleted = buffer.delete_before_cursor();

    assert_eq!(deleted.as_ref().and_then(TextSort::glyph_name), Some("A"));
    assert_eq!(buffer.cursor(), 0);
    assert_eq!(buffer.active_sort(), Some(0));
}

#[test]
fn delete_before_cursor_clears_deleted_active_sort_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);
    buffer.insert_glyph("C", Some('C'), 620.0);
    buffer.activate_sort(1);
    buffer.set_cursor(2);

    let deleted = buffer.delete_before_cursor();

    assert_eq!(deleted.as_ref().and_then(TextSort::glyph_name), Some("B"));
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(buffer.active_sort(), None);
    assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
    assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("C"));
    assert!(!buffer.iter().any(|sort| sort.active));
}

#[test]
fn delete_after_cursor_clears_deleted_active_sort_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);
    buffer.insert_glyph("C", Some('C'), 620.0);
    buffer.activate_sort(1);
    buffer.set_cursor(1);

    let deleted = buffer.delete_after_cursor();

    assert_eq!(deleted.as_ref().and_then(TextSort::glyph_name), Some("B"));
    assert_eq!(buffer.cursor(), 1);
    assert_eq!(buffer.active_sort(), None);
    assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
    assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("C"));
    assert!(!buffer.iter().any(|sort| sort.active));
}

#[test]
fn line_break_preserves_active_sort() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_line_break();

    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer.cursor(), 2);
    assert_eq!(buffer.active_sort(), Some(0));
}

#[test]
fn line_break_before_active_shifts_active_sort_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 610.0);
    buffer.activate_sort(1);
    buffer.set_cursor(1);

    buffer.insert_line_break();

    assert_eq!(buffer.cursor(), 2);
    assert_eq!(buffer.active_sort(), Some(2));
    assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
    assert!(matches!(
        buffer.sort(1).map(|sort| &sort.kind),
        Some(TextSortKind::LineBreak)
    ));
    assert_eq!(buffer.sort(2).and_then(TextSort::glyph_name), Some("B"));
    assert!(buffer.sort(2).is_some_and(|sort| sort.active));
}

#[test]
fn typed_sort_before_active_shifts_active_sort() {
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": { "65": "A", "66": "B" },
                "widths": { "A": 640, "B": 650 }
            }"#,
        )
        .expect("valid glyph inventory"),
    );
    buffer.insert_glyph("B", Some('B'), 650.0);
    buffer.set_cursor(0);

    assert!(buffer.insert_character('A'));

    assert_eq!(buffer.cursor(), 1);
    assert_eq!(buffer.active_sort(), Some(1));
    assert_eq!(buffer.sort(0).and_then(TextSort::glyph_name), Some("A"));
    assert_eq!(buffer.sort(1).and_then(TextSort::glyph_name), Some("B"));
}

#[test]
fn visual_cursor_movement_respects_direction() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 600.0);
    buffer.insert_glyph("B", Some('B'), 600.0);

    buffer.move_cursor_visual_left();
    assert_eq!(buffer.cursor(), 1);

    buffer.set_direction(TextDirection::RightToLeft);
    buffer.move_cursor_visual_left();
    assert_eq!(buffer.cursor(), 2);
    buffer.move_cursor_visual_right();
    assert_eq!(buffer.cursor(), 1);
}

#[test]
fn hit_test_activates_clicked_ltr_sort() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);

    let hit = buffer.hit_test(650.0, 200.0, 1000.0, 800.0, -200.0);

    assert_eq!(hit.active_sort, Some(1));
    assert_eq!(hit.cursor, 2);
}

#[test]
fn hit_test_rejects_sort_above_ascender() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);

    let hit = buffer.hit_test(650.0, 900.0, 1000.0, 800.0, -200.0);

    assert_eq!(hit.active_sort, None);
    assert_eq!(hit.cursor, 1);
}

/// Three lines of two glyphs each, 500 units wide, 1000-unit lines.
fn three_line_buffer() -> TextBuffer {
    let mut buffer = TextBuffer::new();
    for line in 0..3 {
        if line > 0 {
            buffer.insert_line_break();
        }
        buffer.insert_glyph("A", Some('A'), 500.0);
        buffer.insert_glyph("B", Some('B'), 500.0);
    }
    buffer
}

#[test]
fn cursor_moves_up_and_down_between_lines() {
    let mut buffer = three_line_buffer();
    // Caret sits after the last glyph of the last line.
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 2);

    assert!(buffer.move_cursor_vertically(-1, 1000.0));
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 1);
    assert!(buffer.move_cursor_vertically(-1, 1000.0));
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 0);

    assert!(buffer.move_cursor_vertically(1, 1000.0));
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 1);
}

#[test]
fn cursor_keeps_its_column_when_changing_line() {
    let mut buffer = three_line_buffer();
    // Between the two glyphs of the bottom line.
    buffer.set_cursor(7);
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 2);

    assert!(buffer.move_cursor_vertically(-1, 1000.0));
    // Same offset into the line above, not its start or end.
    let (line_start, _) = buffer.line_range_for_number(1);
    assert_eq!(buffer.cursor(), line_start + 1);
}

#[test]
fn cursor_stops_at_the_first_and_last_line() {
    let mut buffer = three_line_buffer();
    assert!(!buffer.move_cursor_vertically(1, 1000.0));
    buffer.set_cursor(0);
    assert!(!buffer.move_cursor_vertically(-1, 1000.0));
}

#[test]
fn home_and_end_move_within_the_caret_line() {
    let mut buffer = three_line_buffer();
    // Sorts are [A B ↵ A B ↵ A B], so the middle line spans 3..5.
    buffer.set_cursor(4); // between the middle line's two glyphs
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 1);

    buffer.move_cursor_to_line_edge(true);
    assert_eq!(buffer.cursor(), 5);
    assert_eq!(buffer.line_number_for_sort(buffer.cursor()), 1);

    buffer.move_cursor_to_line_edge(false);
    assert_eq!(buffer.cursor(), 3);
}

#[test]
fn click_places_the_caret_between_sorts() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);

    // Left half of the first glyph: before it.
    assert_eq!(buffer.place_cursor_at(100.0, 0.0, 1000.0, 800.0, -200.0), 0);
    // Right half of the first glyph: between the two.
    assert_eq!(buffer.place_cursor_at(400.0, 0.0, 1000.0, 800.0, -200.0), 1);
    // Past the end of the run: after the last glyph.
    assert_eq!(
        buffer.place_cursor_at(2000.0, 0.0, 1000.0, 800.0, -200.0),
        2
    );
}

#[test]
fn click_places_the_caret_on_the_clicked_line() {
    let mut buffer = three_line_buffer();

    // Middle line sits one line-height below the first.
    let cursor = buffer.place_cursor_at(100.0, -1000.0, 1000.0, 800.0, -200.0);
    assert_eq!(buffer.line_number_for_sort(cursor), 1);
}

/// A buffer wired to the bundled font's inventory and features, the
/// way the editor sets one up.
fn buffer_with_shaping_font() -> TextBuffer {
    let ufo_dir = crate::testing::fonts::regular_ufo();
    let font = norad::Font::load(&ufo_dir).expect("test UFO loads");
    let features = std::fs::read_to_string(ufo_dir.join("features.fea")).expect("features.fea");

    let mut widths = HashMap::new();
    let mut unicode = HashMap::new();
    for glyph in font.layers.default_layer().iter() {
        widths.insert(glyph.name().to_string(), glyph.width);
        for codepoint in glyph.codepoints.iter() {
            unicode.insert(codepoint as u32, glyph.name().to_string());
        }
    }

    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(TextGlyphInventory {
        unicode,
        widths,
        outlines: HashMap::new(),
        features,
        units_per_em: 1000.0,
    });
    buffer
}

fn type_chars(buffer: &mut TextBuffer, text: &str) {
    for char in text.chars() {
        let name = buffer
            .glyph_inventory
            .unicode
            .get(&(char as u32))
            .cloned()
            .unwrap_or_else(|| ".notdef".to_string());
        let width = buffer
            .glyph_inventory
            .widths
            .get(&name)
            .copied()
            .unwrap_or(0.0);
        buffer.insert_glyph(name, Some(char), width);
    }
}

#[test]
fn latin_keeps_one_glyph_per_character_when_shaped_by_the_font() {
    let mut buffer = buffer_with_shaping_font();
    buffer.set_direction(TextDirection::LeftToRight);
    type_chars(&mut buffer, "Runebender.org");

    buffer.shape_arabic();

    assert_eq!(buffer.len(), 14);
    let absorbed = (0..buffer.len())
        .filter(|i| buffer.sort(*i).expect("sort").is_absorbed())
        .count();
    assert_eq!(absorbed, 0, "no Latin character should be folded away");
    assert_eq!(buffer.layout(1000.0).items.len(), 14);
    assert_eq!(buffer.sort_glyph_name(0), Some("R"));
}

#[test]
fn arabic_in_a_latin_line_still_ligates() {
    // A line whose first strong character is Latin still reads LTR,
    // but the Arabic inside it has to be shaped as its own run or
    // the script-specific features never run.
    let mut buffer = buffer_with_shaping_font();
    buffer.set_auto_direction();
    type_chars(&mut buffer, "hi \u{0644}\u{0627}");

    buffer.shape_arabic();

    assert_eq!(buffer.sort_glyph_name(3), Some("lam_alef-ar"));
    assert!(buffer.sort(4).expect("alef sort").is_absorbed());
}

#[test]
fn a_glyph_opens_beside_the_one_being_edited() {
    // Double-clicking a component puts its base next to the glyph
    // that uses it, wherever the cursor happens to be.
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);
    buffer.insert_glyph("C", Some('C'), 500.0);
    buffer.activate_sort(0);
    buffer.set_cursor(3); // cursor parked at the end

    let index = buffer.insert_glyph_after_active("acutecomb", None, 0.0);

    assert_eq!(index, 1);
    assert_eq!(buffer.sort_glyph_name(1), Some("acutecomb"));
    // ...and it is what gets edited.
    assert_eq!(buffer.active_sort(), Some(1));
    assert_eq!(buffer.cursor(), 2);
}

#[test]
fn a_glyph_opens_at_the_cursor_when_nothing_is_active() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);
    buffer.set_active_sort(None);
    buffer.set_cursor(1);

    assert_eq!(buffer.insert_glyph_after_active("C", Some('C'), 500.0), 1);
    assert_eq!(buffer.sort_glyph_name(1), Some("C"));
}

#[test]
fn lam_alef_renders_as_one_ligature_glyph() {
    let mut buffer = buffer_with_shaping_font();
    type_chars(&mut buffer, "\u{0644}\u{0627}");

    assert!(buffer.shape_arabic(), "shaping changed the buffer");
    assert_eq!(buffer.sort_glyph_name(0), Some("lam_alef-ar"));
    // The alef keeps its place in the buffer — the cursor and editing
    // still see two characters — but draws nothing.
    assert_eq!(buffer.len(), 2);
    assert!(buffer.sort(1).expect("alef sort").is_absorbed());

    // One glyph on the line, and it is the ligature.
    let layout = buffer.layout(1000.0);
    assert_eq!(layout.items.len(), 1);
    assert_eq!(layout.items[0].index, 0);
}

#[test]
fn deleting_the_lam_brings_the_alef_back() {
    let mut buffer = buffer_with_shaping_font();
    type_chars(&mut buffer, "\u{0644}\u{0627}");
    buffer.shape_arabic();

    buffer.set_cursor(1);
    buffer.delete_before_cursor();
    buffer.shape_arabic();

    assert_eq!(buffer.len(), 1);
    assert!(!buffer.sort(0).expect("alef sort").is_absorbed());
    assert_eq!(buffer.sort_glyph_name(0), Some("alef-ar"));
    assert_eq!(buffer.layout(1000.0).items.len(), 1);
}

#[test]
fn shaping_falls_back_when_the_feature_file_is_broken() {
    let mut buffer = buffer_with_shaping_font();
    let mut inventory = buffer.glyph_inventory.clone();
    inventory.features = "feature liga { sub missing by alsoMissing; } liga;".into();
    buffer.set_glyph_inventory(inventory);
    type_chars(&mut buffer, "\u{0628}\u{0628}");

    // The built-in joining rules still run, so the text stays shaped.
    assert!(buffer.shape_arabic());
    assert_eq!(buffer.sort_glyph_name(0), Some("beh-ar.init"));
    assert_eq!(buffer.sort_glyph_name(1), Some("beh-ar.fina"));
}

#[test]
fn hit_test_places_ltr_cursor_nearest_boundary() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);

    let hit = buffer.hit_test(20.0, 1200.0, 1000.0, 800.0, -200.0);

    assert_eq!(hit.active_sort, None);
    assert_eq!(hit.cursor, 0);
}

#[test]
fn hit_test_uses_xilem_exclusive_sort_max_edges() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 300.0);

    let boundary = buffer.hit_test(500.0, 100.0, 1000.0, 800.0, -200.0);
    assert_eq!(boundary.active_sort, Some(1));
    assert_eq!(boundary.cursor, 2);

    let top_edge = buffer.hit_test(250.0, 800.0, 1000.0, 800.0, -200.0);
    assert_eq!(top_edge.active_sort, None);
    assert_eq!(top_edge.cursor, 0);
}

#[test]
fn hit_test_uses_metric_box_for_ltr_line_selection() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("B", Some('B'), 500.0);

    let hit = buffer.hit_test(250.0, -300.0, 1000.0, 800.0, -200.0);

    assert_eq!(hit.active_sort, Some(2));
    assert_eq!(hit.cursor, 3);
}

#[test]
fn hit_test_uses_rtl_visual_cursor_positions() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("B", Some('B'), 500.0);

    let hit = buffer.hit_test(980.0, -1200.0, 1000.0, 800.0, -200.0);

    assert_eq!(hit.active_sort, None);
    assert_eq!(hit.cursor, 0);
}

#[test]
fn hit_test_uses_metric_box_for_rtl_line_selection() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("B", Some('B'), 500.0);

    let hit = buffer.hit_test(250.0, -300.0, 1000.0, 800.0, -200.0);

    assert_eq!(hit.active_sort, Some(2));
    assert_eq!(hit.cursor, 3);
}

#[test]
fn activate_sort_at_returns_layout_origin_for_active_sort() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("B", Some('B'), 300.0);
    buffer.set_cursor(0);

    let activation = buffer
        .activate_sort_at(300.0, -300.0, 1000.0, 800.0, -200.0)
        .expect("sort activates");

    assert_eq!(activation.index, 2);
    assert_eq!(activation.x, 200.0);
    assert_eq!(activation.y, -1000.0);
    assert_eq!(buffer.active_sort(), Some(2));
    assert_eq!(buffer.cursor(), 0);
}

#[test]
fn update_glyph_changes_existing_sort_metadata() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);

    assert!(buffer.update_glyph(0, "beh-ar.init", Some('\u{0628}'), 480.0));
    let sort = buffer.sort(0).expect("sort exists");
    assert_eq!(sort.glyph_name(), Some("beh-ar.init"));
    let TextSortKind::Glyph { advance_width, .. } = sort.kind else {
        panic!("expected glyph sort");
    };
    assert_eq!(advance_width, 480.0);
}

#[test]
fn shape_arabic_uses_positional_forms_when_available() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.insert_glyph("heh-ar", Some('\u{0647}'), 510.0);

    assert!(buffer.shape_arabic());

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn shape_arabic_resets_to_base_forms_in_ltr() {
    let mut buffer = TextBuffer::new();
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );
    buffer.insert_glyph("beh-ar.init", Some('\u{0628}'), 480.0);

    assert!(buffer.shape_arabic());

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar")
    );
}

#[test]
fn set_direction_does_not_reshape_existing_sorts_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": {
                    "1576": "beh-ar",
                    "1607": "heh-ar"
                },
                "widths": {
                    "beh-ar": 500,
                    "beh-ar.init": 480,
                    "heh-ar": 510,
                    "heh-ar.fina": 490
                }
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert!(buffer.insert_character('\u{0628}'));
    assert!(buffer.insert_character('\u{0647}'));
    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );

    buffer.set_direction(TextDirection::LeftToRight);

    assert_eq!(
        buffer.sort(0).and_then(TextSort::glyph_name),
        Some("beh-ar.init")
    );
    assert_eq!(
        buffer.sort(1).and_then(TextSort::glyph_name),
        Some("heh-ar.fina")
    );
}

#[test]
fn set_direction_only_changes_direction_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(buffer.begin_manual_kerning(1, 500.0));
    buffer.set_direction(TextDirection::RightToLeft);

    assert_eq!(buffer.direction(), TextDirection::RightToLeft);
    assert_eq!(buffer.manual_kerning_sort(), Some(1));
}

#[test]
fn set_kerning_model_keeps_manual_kerning_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(buffer.begin_manual_kerning(1, 500.0));
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "A": { "V": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    assert_eq!(buffer.manual_kerning_sort(), Some(1));
}

#[test]
fn set_glyph_inventory_keeps_manual_kerning_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(buffer.begin_manual_kerning(1, 500.0));
    buffer.set_glyph_inventory(
        serde_json::from_str(
            r#"{
                "unicode": { "65": "A", "86": "V" },
                "widths": { "A": 500, "V": 500 },
                "outlines": {}
            }"#,
        )
        .expect("valid glyph inventory"),
    );

    assert_eq!(buffer.manual_kerning_sort(), Some(1));
}

#[test]
fn update_glyph_keeps_manual_kerning_like_xilem_width_edit() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(buffer.begin_manual_kerning(1, 500.0));
    assert!(buffer.update_glyph(1, "V", Some('V'), 520.0));

    assert_eq!(buffer.manual_kerning_sort(), Some(1));
    let TextSortKind::Glyph { advance_width, .. } = &buffer.sort(1).expect("sort exists").kind
    else {
        panic!("expected glyph sort");
    };
    assert_eq!(*advance_width, 520.0);
}

#[test]
fn layout_positions_ltr_lines_and_cursor() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("B", Some('B'), 300.0);

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items.len(), 2);
    assert_eq!(layout.items[0].x, 0.0);
    assert_eq!(layout.items[0].y, 0.0);
    assert_eq!(layout.items[1].x, 0.0);
    assert_eq!(layout.items[1].y, -1000.0);
    assert_eq!(layout.cursor_x, 300.0);
    assert_eq!(layout.cursor_y, -1000.0);
}

#[test]
fn layout_places_cursor_on_empty_line_after_trailing_line_break_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 300.0);
    buffer.insert_line_break();

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items.len(), 1);
    assert_eq!(layout.cursor_x, 0.0);
    assert_eq!(layout.cursor_y, -1000.0);
}

#[test]
fn layout_applies_direct_kerning_pairs() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "A": { "V": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items[0].x, 0.0);
    assert_eq!(layout.items[1].x, 420.0);
    assert_eq!(layout.cursor_x, 920.0);
}

#[test]
fn manual_kerning_drag_updates_direct_pair() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "A": { "V": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    assert!(buffer.begin_manual_kerning(1, 500.0));
    assert_eq!(buffer.manual_kerning_sort(), Some(1));
    assert_eq!(buffer.drag_manual_kerning(530.0), Some(-50.0));

    let layout = buffer.layout(1000.0);
    assert_eq!(layout.items[1].x, 450.0);
    assert_eq!(layout.cursor_x, 950.0);
    assert!(buffer.end_manual_kerning());
    assert_eq!(buffer.manual_kerning_sort(), None);
}

#[test]
fn manual_kerning_drag_snaps_to_integer_units() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(buffer.begin_manual_kerning(1, 0.0));
    assert_eq!(buffer.drag_manual_kerning(96.16), Some(96.0));
    assert_eq!(
        buffer
            .kerning_model()
            .kerning
            .get("A")
            .and_then(|pairs| pairs.get("V"))
            .copied(),
        Some(96.0)
    );
}

#[test]
fn manual_kerning_enters_noop_session_after_line_break_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(!buffer.begin_manual_kerning(0, 0.0));
    assert!(buffer.begin_manual_kerning(2, 0.0));
    assert_eq!(buffer.manual_kerning_sort(), Some(2));
    assert_eq!(buffer.active_sort(), Some(2));
    assert_eq!(buffer.drag_manual_kerning(30.0), None);
    assert!(buffer.end_manual_kerning());
}

#[test]
fn structural_text_edits_cancel_manual_kerning() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);

    assert!(buffer.begin_manual_kerning(1, 500.0));
    assert_eq!(buffer.manual_kerning_sort(), Some(1));
    buffer.set_cursor(1);
    assert!(buffer.delete_after_cursor().is_some());
    assert_eq!(buffer.manual_kerning_sort(), None);

    buffer.insert_glyph("V", Some('V'), 500.0);
    assert!(buffer.begin_manual_kerning(1, 500.0));
    buffer.clear();
    assert_eq!(buffer.manual_kerning_sort(), None);
}

#[test]
fn layout_applies_group_kerning_pairs() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "groups": {
                    "public.kern1.A": ["A"],
                    "public.kern2.V": ["V"]
                },
                "kerning": {
                    "public.kern1.A": { "public.kern2.V": -90 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items[1].x, 410.0);
    assert_eq!(layout.cursor_x, 910.0);
}

#[test]
fn layout_applies_raw_xilem_group_names_without_public_prefix() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "groups": {
                    "leftRaw": ["A"],
                    "rightRaw": ["V"]
                },
                "kerning": {
                    "leftRaw": { "rightRaw": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items[1].x, 420.0);
    assert_eq!(layout.cursor_x, 920.0);
}

#[test]
fn layout_prioritizes_xilem_glyph_group_hints_before_other_memberships() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_glyph("V", Some('V'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "groups": {
                    "firstLeft": ["A"],
                    "hintLeft": ["A"],
                    "firstRight": ["V"],
                    "hintRight": ["V"]
                },
                "leftGroups": { "V": "hintRight" },
                "rightGroups": { "A": "hintLeft" },
                "kerning": {
                    "firstLeft": { "firstRight": -20 },
                    "hintLeft": { "hintRight": -70 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items[1].x, 430.0);
    assert_eq!(layout.cursor_x, 930.0);
}

/// Where a given sort ended up in the preview strip.
fn preview_x(items: &[TextLayoutItem], index: usize) -> f64 {
    items
        .iter()
        .find(|item| item.index == index)
        .unwrap_or_else(|| panic!("sort {index} is in the preview"))
        .x
}

/// Where a given sort ended up. Items come back in the order they
/// are drawn, which for a right-to-left run is not the order the
/// sorts were typed in.
fn item_x(layout: &TextLayout, index: usize) -> f64 {
    layout
        .items
        .iter()
        .find(|item| item.index == index)
        .unwrap_or_else(|| panic!("sort {index} was laid out"))
        .x
}

#[test]
fn latin_inside_an_arabic_line_still_reads_left_to_right() {
    // "ا this ب": an Arabic line with a Latin word in it. The word
    // sits where bidi puts it — to the left of the first Arabic
    // letter — but its own letters run left to right. Reversing the
    // whole line is what made "this is a book" read "koob a si siht".
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 100.0);
    for (name, char) in [("t", 't'), ("h", 'h'), ("i", 'i'), ("s", 's')] {
        buffer.insert_glyph(name, Some(char), 100.0);
    }
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 100.0);

    let layout = buffer.layout(1000.0);

    // The Arabic letters take the outer edges: alef (typed first) on
    // the right, beh on the left.
    assert_eq!(item_x(&layout, 0), 500.0);
    assert_eq!(item_x(&layout, 5), 0.0);
    // t-h-i-s, in that order, between them.
    assert_eq!(item_x(&layout, 1), 100.0);
    assert_eq!(item_x(&layout, 2), 200.0);
    assert_eq!(item_x(&layout, 3), 300.0);
    assert_eq!(item_x(&layout, 4), 400.0);
}

#[test]
fn digits_in_an_arabic_line_read_left_to_right() {
    // Numbers are a weaker case than letters — bidi class EN, not L
    // — and they still run left to right inside an RTL line.
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 100.0);
    buffer.insert_glyph("one", Some('1'), 100.0);
    buffer.insert_glyph("two", Some('2'), 100.0);

    let layout = buffer.layout(1000.0);

    assert_eq!(item_x(&layout, 0), 200.0);
    assert_eq!(item_x(&layout, 1), 0.0);
    assert_eq!(item_x(&layout, 2), 100.0);
}

#[test]
fn arabic_inside_a_latin_line_still_reads_right_to_left() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("a", Some('a'), 100.0);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 100.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 100.0);
    buffer.insert_glyph("b", Some('b'), 100.0);

    let layout = buffer.layout(1000.0);

    assert_eq!(item_x(&layout, 0), 0.0);
    // The Arabic pair is mirrored inside the Latin sentence.
    assert_eq!(item_x(&layout, 1), 200.0);
    assert_eq!(item_x(&layout, 2), 100.0);
    assert_eq!(item_x(&layout, 3), 300.0);
}

#[test]
fn layout_positions_rtl_from_line_width() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 300.0);

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items.len(), 2);
    // First letter typed sits at the right edge.
    assert_eq!(item_x(&layout, 0), 300.0);
    assert_eq!(item_x(&layout, 1), 0.0);
    assert_eq!(layout.cursor_x, 0.0);
    assert_eq!(layout.cursor_y, 0.0);
}

#[test]
fn manual_kerning_drag_flips_sign_in_rtl() {
    // LTR: dragging right (+40) widens the pair by +40.
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("a", Some('a'), 100.0);
    buffer.insert_glyph("b", Some('b'), 100.0);
    assert!(buffer.begin_manual_kerning(1, 500.0));
    assert_eq!(buffer.drag_manual_kerning(540.0), Some(40.0));
    buffer.end_manual_kerning();

    // RTL: the same rightward drag closes the visual gap, so the
    // logical pair's kern goes to −40.
    let mut rtl = TextBuffer::new();
    rtl.set_direction(TextDirection::RightToLeft);
    rtl.insert_glyph("alef-ar", Some('\u{0627}'), 100.0);
    rtl.insert_glyph("beh-ar", Some('\u{0628}'), 100.0);
    assert!(rtl.begin_manual_kerning(1, 500.0));
    assert_eq!(rtl.drag_manual_kerning(540.0), Some(-40.0));
}

#[test]
fn activate_sort_at_uses_rtl_kerned_layout_origin_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "alef-ar": { "beh-ar": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let activation = buffer
        .activate_sort_at(100.0, 0.0, 1000.0, 800.0, -200.0)
        .expect("kerned RTL sort activates");

    assert_eq!(activation.index, 1);
    assert_eq!(activation.x, 80.0);
    assert_eq!(activation.y, 0.0);
    assert_eq!(buffer.active_sort(), Some(1));
}

#[test]
fn rtl_layout_places_cursor_on_empty_line_after_trailing_line_break_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("A", Some('A'), 300.0);
    buffer.insert_line_break();

    let layout = buffer.layout(1000.0);

    assert_eq!(layout.items.len(), 1);
    assert_eq!(layout.cursor_x, 300.0);
    assert_eq!(layout.cursor_y, -1000.0);
}

#[test]
fn auto_direction_reads_each_line_from_its_own_script() {
    let mut buffer = TextBuffer::new();
    // Line 1 Latin, line 2 Arabic — the case a single buffer
    // direction could never get right.
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);

    assert!(buffer.direction_is_auto());
    assert_eq!(
        buffer.resolved_line_direction(0),
        TextDirection::LeftToRight
    );
    assert_eq!(
        buffer.resolved_line_direction(1),
        TextDirection::RightToLeft
    );
}

#[test]
fn auto_direction_ignores_neutral_characters() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("one", Some('1'), 500.0);
    buffer.insert_glyph("period", Some('.'), 200.0);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);

    // Digits and punctuation don't decide; the Arabic letter does.
    assert_eq!(
        buffer.resolved_line_direction(0),
        TextDirection::RightToLeft
    );
}

#[test]
fn pinning_a_direction_overrides_detection() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);
    assert_eq!(buffer.cursor_direction(), TextDirection::RightToLeft);

    buffer.set_direction(TextDirection::LeftToRight);
    assert!(!buffer.direction_is_auto());
    assert_eq!(buffer.cursor_direction(), TextDirection::LeftToRight);

    buffer.set_auto_direction();
    assert_eq!(buffer.cursor_direction(), TextDirection::RightToLeft);
}

#[test]
fn mixed_lines_lay_out_in_their_own_directions() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 400.0);

    let layout = buffer.layout(1000.0);

    // Latin line runs rightwards from the origin.
    assert_eq!(item_x(&layout, 0), 0.0);
    // Arabic line right-aligns on the widest line (700) and reads
    // right to left: first letter nearest the right edge.
    assert_eq!(item_x(&layout, 2), 400.0);
    assert_eq!(item_x(&layout, 3), 0.0);
}

#[test]
fn preview_orders_runs_left_to_right_but_fills_each_run_by_direction() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 300.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 400.0);

    let preview = buffer.preview_layout();

    // Latin run first, then the Arabic run occupying [500, 1200]
    // with its first letter on the right.
    assert_eq!(preview_x(&preview, 0), 0.0);
    assert_eq!(preview_x(&preview, 2), 900.0);
    assert_eq!(preview_x(&preview, 3), 500.0);
}

#[test]
fn layout_right_aligns_rtl_lines_on_the_widest_line() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 300.0);

    let layout = buffer.layout(1000.0);

    // Both lines share the right edge at x = 500 (the widest line),
    // so the 300-wide second line starts 200 units further left.
    assert_eq!(layout.items.len(), 2);
    assert_eq!(item_x(&layout, 0), 0.0);
    assert_eq!(layout.items[0].y, 0.0);
    assert_eq!(item_x(&layout, 2), 200.0);
    assert_eq!(layout.items[1].y, -1000.0);
    assert_eq!(layout.cursor_x, 200.0);
    assert_eq!(layout.cursor_y, -1000.0);
}

#[test]
fn layout_applies_rtl_kerning_without_shifting_line_start() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "alef-ar": { "beh-ar": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let layout = buffer.layout(1000.0);

    assert_eq!(item_x(&layout, 0), 500.0);
    assert_eq!(item_x(&layout, 1), 80.0);
    assert_eq!(layout.cursor_x, 80.0);
}

#[test]
fn rtl_multiline_layout_resets_kerning_between_lines() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "alef-ar": { "beh-ar": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let layout = buffer.layout(1000.0);

    // Right edge is the widest line: 500 + 500 = 1000 advance
    // units (kerning does not shift the line's start).
    assert_eq!(layout.items.len(), 3);
    assert_eq!(item_x(&layout, 0), 500.0);
    assert_eq!(item_x(&layout, 1), 80.0);
    // The second line kerns from scratch and right-aligns.
    assert_eq!(item_x(&layout, 3), 500.0);
    assert_eq!(layout.cursor_x, 500.0);
    assert_eq!(layout.cursor_y, -1000.0);
}

#[test]
fn preview_layout_keeps_line_breaks_in_one_strip() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("V", Some('V'), 300.0);

    let preview = buffer.preview_layout();

    assert_eq!(preview.len(), 2);
    assert_eq!(preview[0].x, 0.0);
    assert_eq!(preview[0].y, 0.0);
    assert_eq!(preview[1].x, 500.0);
    assert_eq!(preview[1].y, 0.0);

    let canvas = buffer.layout(1000.0);
    assert_eq!(canvas.items[1].x, 0.0);
    assert_eq!(canvas.items[1].y, -1000.0);
}

#[test]
fn preview_layout_breaks_kerning_across_line_breaks() {
    let mut buffer = TextBuffer::new();
    buffer.insert_glyph("A", Some('A'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("V", Some('V'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "A": { "V": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let preview = buffer.preview_layout();

    assert_eq!(preview[1].x, 500.0);
}

#[test]
fn rtl_preview_layout_keeps_line_breaks_in_one_strip() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 300.0);

    let preview = buffer.preview_layout();

    // Both lines read the same way, so the strip carries on across
    // the break: 800 units reading right to left, first glyph on the
    // right.
    assert_eq!(preview.len(), 2);
    assert_eq!(preview_x(&preview, 0), 300.0);
    assert_eq!(preview[0].y, 0.0);
    assert_eq!(preview_x(&preview, 2), 0.0);
    assert_eq!(preview[1].y, 0.0);

    let canvas = buffer.layout(1000.0);
    assert_eq!(item_x(&canvas, 0), 0.0);
    assert_eq!(canvas.items[0].y, 0.0);
    assert_eq!(item_x(&canvas, 2), 200.0);
    assert_eq!(canvas.items[1].y, -1000.0);
}

#[test]
fn rtl_preview_layout_breaks_kerning_across_line_breaks_like_xilem() {
    let mut buffer = TextBuffer::new();
    buffer.set_direction(TextDirection::RightToLeft);
    buffer.insert_glyph("alef-ar", Some('\u{0627}'), 500.0);
    buffer.insert_line_break();
    buffer.insert_glyph("beh-ar", Some('\u{0628}'), 500.0);
    buffer.set_kerning_model(
        serde_json::from_str(
            r#"{
                "kerning": {
                    "alef-ar": { "beh-ar": -80 }
                }
            }"#,
        )
        .expect("valid kerning model"),
    );

    let preview = buffer.preview_layout();

    // The pair straddles a line break, so it does not kern.
    assert_eq!(preview_x(&preview, 0), 500.0);
    assert_eq!(preview_x(&preview, 2), 0.0);
}

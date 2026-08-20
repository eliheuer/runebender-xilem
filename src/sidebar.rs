// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The glyph-grid sidebar's language and filter data, shared by all
//! Runebender editors. A port of runebender-web's glyphSidebarData.ts
//! (definitions) and the matching functions in Runebender.vue; the
//! Google Fonts glyphsets come from data/gf-glyphsets.json, generated
//! from google/fonts glyphsets by the web repo's script.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::Deserialize;

/// One Google Fonts glyphset (GF_Latin_Core, …).
#[derive(Deserialize)]
pub struct GfGlyphset {
    pub id: String,
    pub label: String,
    pub script: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "glyphNames")]
    pub glyph_names: Vec<String>,
    #[serde(rename = "expectedCount")]
    pub expected_count: usize,
}

/// The full GF glyphset list, parsed once.
pub fn gf_glyphsets() -> &'static [GfGlyphset] {
    static SETS: OnceLock<Vec<GfGlyphset>> = OnceLock::new();
    SETS.get_or_init(|| {
        serde_json::from_str(include_str!("../data/gf-glyphsets.json"))
            .expect("gf-glyphsets.json parses")
    })
}

/// A glyph a filter expects: name plus codepoint, so a differently
/// named glyph still counts when its codepoint matches.
#[derive(Clone)]
pub struct GlyphTarget {
    pub name: String,
    pub unicode: u32,
}

/// One row under a script: a set of glyphs matched by name list,
/// target list, or codepoint ranges (web SidebarCharacterFilter).
pub struct CharacterFilter {
    pub id: String,
    pub label: String,
    pub glyph_names: Vec<String>,
    pub targets: Vec<GlyphTarget>,
    pub ranges: Vec<(u32, u32)>,
    /// The full size of the set, for "present/expected" coverage.
    pub expected_count: Option<usize>,
}

/// A script group in the Languages section.
pub struct LanguageGroup {
    pub id: String,
    pub label: String,
    pub icon: String,
    /// What the group row itself selects: the script's own Unicode
    /// blocks (the glyphsets carry Latin punctuation, so OR-ing the
    /// sub-filters would drag half the Latin alphabet in).
    pub script_ranges: Vec<(u32, u32)>,
    /// Glyph-name suffix for forms with no codepoint (`beh-ar.init`).
    pub name_suffix: Option<String>,
    pub filters: Vec<CharacterFilter>,
}

/// A row in the Filters section (web SidebarBuiltinFilter).
pub struct BuiltinFilter {
    pub id: String,
    pub label: String,
    /// None = a Runebender builtin (exporting, incompatible);
    /// Some = a GF glyphset promoted into the Filters list.
    pub glyphset: Option<CharacterFilter>,
}

fn character_filter_for_glyphset(set: &GfGlyphset) -> CharacterFilter {
    CharacterFilter {
        id: set.id.clone(),
        label: set.label.clone(),
        glyph_names: set.glyph_names.clone(),
        targets: Vec::new(),
        ranges: Vec::new(),
        expected_count: Some(set.expected_count),
    }
}

fn glyphset_filters_for_script(script: &str) -> Vec<CharacterFilter> {
    gf_glyphsets()
        .iter()
        .filter(|set| set.script == script)
        .map(character_filter_for_glyphset)
        .collect()
}

// ---- Arabic name lists (Glyphs' two most-used Arabic filters, by
// glyph name: the positional forms carry no Unicode of their own) ----

const ARABIC_BASIC_SHAPE_BASES: &[&str] = &[
    "hamza-ar",
    "alef-ar",
    "behDotless-ar",
    "hah-ar",
    "dal-ar",
    "reh-ar",
    "seen-ar",
    "sad-ar",
    "tah-ar",
    "ain-ar",
    "fehDotless-ar",
    "qafDotless-ar",
    "kaf-ar",
    "lam-ar",
    "meem-ar",
    "noonghunna-ar",
    "heh-ar",
    "waw-ar",
    "alefMaksura-ar",
    "lam_alef-ar",
];

/// Dots and marks: components, so they have no positional forms.
const ARABIC_SHAPE_PARTS: &[&str] = &[
    "dotabove-ar",
    "dotbelow-ar",
    "dotcenter-ar",
    "twodotshorizontalabove-ar",
    "twodotshorizontalbelow-ar",
    "twodotsverticalabove-ar",
    "twodotsverticalbelow-ar",
    "threedotsupabove-ar",
    "threedotsupbelow-ar",
    "threedotsupcenter-ar",
    "threedotsdownabove-ar",
    "threedotsdownbelow-ar",
    "threedotsdowncenter-ar",
    "miniKeheh-ar",
    "gafsarkashabove-ar",
    "gafsarkashcenter-ar",
    "doublestroke-ar",
];

const ARABIC_BASIC_EXTRA_BASES: &[&str] = &[
    "beh-ar",
    "teh-ar",
    "theh-ar",
    "jeem-ar",
    "khah-ar",
    "thal-ar",
    "zain-ar",
    "sheen-ar",
    "dad-ar",
    "zah-ar",
    "ghain-ar",
    "feh-ar",
    "qaf-ar",
    "noon-ar",
    "yeh-ar",
    "yehHamzaabove-ar",
    "tehMarbuta-ar",
    "alefHamzaabove-ar",
    "alefHamzabelow-ar",
    "alefMadda-ar",
    "alefWasla-ar",
    "wawHamzaabove-ar",
    "alefMaksura-ar",
    "kashida-ar",
];

/// A base plus its three positional forms, as Glyphs lists them.
fn arabic_forms(bases: &[&str]) -> Vec<String> {
    bases
        .iter()
        .flat_map(|base| {
            [
                base.to_string(),
                format!("{base}.init"),
                format!("{base}.medi"),
                format!("{base}.fina"),
            ]
        })
        .collect()
}

// ---- Hebrew targets ----

const HEBREW_LETTER_NAMES: &[(u32, &str)] = &[
    (0x05d0, "alef-hb"),
    (0x05d1, "bet-hb"),
    (0x05d2, "gimel-hb"),
    (0x05d3, "dalet-hb"),
    (0x05d4, "he-hb"),
    (0x05d5, "vav-hb"),
    (0x05d6, "zayin-hb"),
    (0x05d7, "het-hb"),
    (0x05d8, "tet-hb"),
    (0x05d9, "yod-hb"),
    (0x05da, "finalkaf-hb"),
    (0x05db, "kaf-hb"),
    (0x05dc, "lamed-hb"),
    (0x05dd, "finalmem-hb"),
    (0x05de, "mem-hb"),
    (0x05df, "finalnun-hb"),
    (0x05e0, "nun-hb"),
    (0x05e1, "samekh-hb"),
    (0x05e2, "ayin-hb"),
    (0x05e3, "finalpe-hb"),
    (0x05e4, "pe-hb"),
    (0x05e5, "finaltsadi-hb"),
    (0x05e6, "tsadi-hb"),
    (0x05e7, "qof-hb"),
    (0x05e8, "resh-hb"),
    (0x05e9, "shin-hb"),
    (0x05ea, "tav-hb"),
    (0x05ef, "yodtriangle-hb"),
    (0x05f0, "vavvav-hb"),
    (0x05f1, "vavyod-hb"),
    (0x05f2, "yodyod-hb"),
    (0x05f3, "geresh-hb"),
    (0x05f4, "gershayim-hb"),
];

fn uni_target(codepoint: u32) -> GlyphTarget {
    GlyphTarget {
        name: format!("uni{codepoint:04X}"),
        unicode: codepoint,
    }
}

fn named_target(codepoint: u32, name: &str) -> GlyphTarget {
    GlyphTarget {
        name: name.to_string(),
        unicode: codepoint,
    }
}

fn range_targets(start: u32, end: u32) -> Vec<GlyphTarget> {
    (start..=end).map(uni_target).collect()
}

fn hebrew_letter_targets() -> Vec<GlyphTarget> {
    HEBREW_LETTER_NAMES
        .iter()
        .map(|&(unicode, name)| named_target(unicode, name))
        .collect()
}

fn hebrew_points_and_marks_targets() -> Vec<GlyphTarget> {
    range_targets(0x0591, 0x05c7)
}

fn hebrew_presentation_form_targets() -> Vec<GlyphTarget> {
    let mut targets = range_targets(0xfb1d, 0xfb36);
    targets.extend(range_targets(0xfb38, 0xfb3c));
    targets.push(uni_target(0xfb3e));
    targets.extend(range_targets(0xfb40, 0xfb41));
    targets.extend(range_targets(0xfb43, 0xfb44));
    targets.extend(range_targets(0xfb46, 0xfb4f));
    targets
}

const HEBREW_PRESENTATION_FORM_RANGES: &[(u32, u32)] = &[
    (0xfb1d, 0xfb36),
    (0xfb38, 0xfb3c),
    (0xfb3e, 0xfb3e),
    (0xfb40, 0xfb41),
    (0xfb43, 0xfb44),
    (0xfb46, 0xfb4f),
];

const GF_HEBREW_SUBSET_RANGES: &[(u32, u32)] = &[
    (0x0000, 0x0000),
    (0x000d, 0x000d),
    (0x0020, 0x0020),
    (0x002d, 0x002d),
    (0x00a0, 0x00a0),
    (0x0307, 0x0308),
    (0x0591, 0x05c7),
    (0x05d0, 0x05ea),
    (0x05ef, 0x05f4),
    (0x200b, 0x200f),
    (0x2010, 0x2010),
    (0x20aa, 0x20aa),
    (0x25cc, 0x25cc),
    (0xfb1d, 0xfb36),
    (0xfb38, 0xfb3c),
    (0xfb3e, 0xfb3e),
    (0xfb40, 0xfb41),
    (0xfb43, 0xfb44),
    (0xfb46, 0xfb4f),
];

fn gf_hebrew_subset_targets() -> Vec<GlyphTarget> {
    let mut targets = vec![
        named_target(0x0020, "space"),
        named_target(0x002d, "hyphen"),
        named_target(0x00a0, "nbspace"),
        named_target(0x0307, "dotaccentcomb"),
        named_target(0x0308, "dieresiscomb"),
    ];
    targets.extend(hebrew_points_and_marks_targets());
    targets.extend(hebrew_letter_targets());
    targets.extend(range_targets(0x200b, 0x200f));
    targets.push(uni_target(0x2010));
    targets.push(uni_target(0x20aa));
    targets.push(named_target(0x25cc, "dottedCircle"));
    targets.extend(hebrew_presentation_form_targets());
    targets
}

/// The Languages section, built once (web SIDEBAR_LANGUAGE_GROUPS).
pub fn language_groups() -> &'static [LanguageGroup] {
    static GROUPS: OnceLock<Vec<LanguageGroup>> = OnceLock::new();
    GROUPS.get_or_init(|| {
        let mut arabic_basic_shapes = arabic_forms(ARABIC_BASIC_SHAPE_BASES);
        arabic_basic_shapes
            .extend(ARABIC_SHAPE_PARTS.iter().map(|s| s.to_string()));
        let mut arabic_basic_bases: Vec<&str> =
            ARABIC_BASIC_SHAPE_BASES.to_vec();
        arabic_basic_bases.extend_from_slice(ARABIC_BASIC_EXTRA_BASES);
        let mut arabic_basic = arabic_forms(&arabic_basic_bases);
        arabic_basic.extend(ARABIC_SHAPE_PARTS.iter().map(|s| s.to_string()));

        let mut groups = vec![
            LanguageGroup {
                id: "Arab".into(),
                label: "Arabic".into(),
                icon: "ض".into(),
                // Arabic, Supplement, Extended-A/B, presentation forms.
                script_ranges: vec![
                    (0x0600, 0x06ff),
                    (0x0750, 0x077f),
                    (0x0870, 0x089f),
                    (0x08a0, 0x08ff),
                    (0xfb50, 0xfdff),
                    (0xfe70, 0xfeff),
                ],
                name_suffix: Some("-ar".into()),
                filters: {
                    let mut filters = vec![
                        CharacterFilter {
                            id: "Arab_BasicShapes".into(),
                            label: "Basic Shapes".into(),
                            glyph_names: arabic_basic_shapes,
                            targets: Vec::new(),
                            ranges: Vec::new(),
                            expected_count: None,
                        },
                        CharacterFilter {
                            id: "Arab_Basic".into(),
                            label: "Basic".into(),
                            glyph_names: arabic_basic,
                            targets: Vec::new(),
                            ranges: Vec::new(),
                            expected_count: None,
                        },
                    ];
                    filters.extend(glyphset_filters_for_script("Arabic"));
                    filters
                },
            },
            LanguageGroup {
                id: "Hans".into(),
                label: "Chinese".into(),
                icon: "字".into(),
                script_ranges: Vec::new(),
                name_suffix: None,
                filters: vec![CharacterFilter {
                    id: "Hans".into(),
                    label: "Chinese Han".into(),
                    glyph_names: Vec::new(),
                    targets: Vec::new(),
                    ranges: vec![(0x4e00, 0x9fff)],
                    expected_count: None,
                }],
            },
            LanguageGroup {
                id: "Cyrl".into(),
                label: "Cyrillic".into(),
                icon: "Я".into(),
                script_ranges: vec![
                    (0x0400, 0x052f),
                    (0x2de0, 0x2dff),
                    (0xa640, 0xa69f),
                ],
                name_suffix: None,
                filters: glyphset_filters_for_script("Cyrillic"),
            },
            LanguageGroup {
                id: "Deva".into(),
                label: "Devanagari".into(),
                icon: "दे".into(),
                script_ranges: Vec::new(),
                name_suffix: None,
                filters: vec![CharacterFilter {
                    id: "Deva".into(),
                    label: "Devanagari".into(),
                    glyph_names: Vec::new(),
                    targets: Vec::new(),
                    ranges: vec![(0x0900, 0x097f)],
                    expected_count: None,
                }],
            },
            LanguageGroup {
                id: "Grek".into(),
                label: "Greek".into(),
                icon: "Ω".into(),
                script_ranges: vec![(0x0370, 0x03ff), (0x1f00, 0x1fff)],
                name_suffix: None,
                filters: glyphset_filters_for_script("Greek"),
            },
            LanguageGroup {
                id: "Hebr".into(),
                label: "Hebrew".into(),
                icon: "א".into(),
                script_ranges: vec![(0x0590, 0x05ff), (0xfb1d, 0xfb4f)],
                name_suffix: Some("-hb".into()),
                filters: vec![
                    CharacterFilter {
                        id: "GF_Hebrew_Subset".into(),
                        label: "Google Fonts Hebrew".into(),
                        glyph_names: Vec::new(),
                        targets: gf_hebrew_subset_targets(),
                        ranges: GF_HEBREW_SUBSET_RANGES.to_vec(),
                        expected_count: Some(gf_hebrew_subset_targets().len()),
                    },
                    CharacterFilter {
                        id: "Hebrew_Letters".into(),
                        label: "Hebrew letters".into(),
                        glyph_names: Vec::new(),
                        targets: hebrew_letter_targets(),
                        ranges: vec![(0x05d0, 0x05ea), (0x05ef, 0x05f4)],
                        expected_count: Some(HEBREW_LETTER_NAMES.len()),
                    },
                    CharacterFilter {
                        id: "Hebrew_Points_Marks".into(),
                        label: "Hebrew points and marks".into(),
                        glyph_names: Vec::new(),
                        targets: hebrew_points_and_marks_targets(),
                        ranges: vec![(0x0591, 0x05c7)],
                        expected_count: Some(
                            hebrew_points_and_marks_targets().len(),
                        ),
                    },
                    CharacterFilter {
                        id: "Hebrew_Presentation_Forms".into(),
                        label: "Hebrew presentation forms".into(),
                        glyph_names: Vec::new(),
                        targets: hebrew_presentation_form_targets(),
                        ranges: HEBREW_PRESENTATION_FORM_RANGES.to_vec(),
                        expected_count: Some(
                            hebrew_presentation_form_targets().len(),
                        ),
                    },
                ],
            },
            LanguageGroup {
                id: "Jpan".into(),
                label: "Japanese".into(),
                icon: "あ".into(),
                script_ranges: Vec::new(),
                name_suffix: None,
                filters: vec![CharacterFilter {
                    id: "Jpan".into(),
                    label: "Kana + Han".into(),
                    glyph_names: Vec::new(),
                    targets: Vec::new(),
                    ranges: vec![(0x3040, 0x30ff), (0x4e00, 0x9fff)],
                    expected_count: None,
                }],
            },
            LanguageGroup {
                id: "Kore".into(),
                label: "Korean".into(),
                icon: "한".into(),
                script_ranges: Vec::new(),
                name_suffix: None,
                filters: vec![CharacterFilter {
                    id: "Kore".into(),
                    label: "Hangul".into(),
                    glyph_names: Vec::new(),
                    targets: Vec::new(),
                    ranges: vec![(0x1100, 0x11ff), (0xac00, 0xd7af)],
                    expected_count: None,
                }],
            },
            LanguageGroup {
                id: "Latn".into(),
                label: "Latin".into(),
                icon: "G".into(),
                script_ranges: vec![
                    (0x0041, 0x024f),
                    (0x1e00, 0x1eff),
                    (0x2c60, 0x2c7f),
                    (0xa720, 0xa7ff),
                ],
                name_suffix: None,
                filters: glyphset_filters_for_script("Latin"),
            },
            LanguageGroup {
                id: "Thai".into(),
                label: "Thai".into(),
                icon: "ก".into(),
                script_ranges: Vec::new(),
                name_suffix: None,
                filters: vec![CharacterFilter {
                    id: "Thai".into(),
                    label: "Thai".into(),
                    glyph_names: Vec::new(),
                    targets: Vec::new(),
                    ranges: vec![(0x0e00, 0x0e7f)],
                    expected_count: None,
                }],
            },
        ];
        // Remaining scripts that only exist as glyphsets.
        for (id, label, icon, script) in [
            ("Phon", "Phonetics", "ə", "Phonetics"),
            ("Tran", "TransLatin", "Ǧ", "TransLatin"),
        ] {
            let filters = glyphset_filters_for_script(script);
            if !filters.is_empty() {
                groups.push(LanguageGroup {
                    id: id.into(),
                    label: label.into(),
                    icon: icon.into(),
                    script_ranges: Vec::new(),
                    name_suffix: None,
                    filters,
                });
            }
        }
        groups
    })
}

/// The Filters section (web SIDEBAR_FILTERS): two Runebender builtins
/// plus the headline GF glyphsets.
pub fn builtin_filters() -> &'static [BuiltinFilter] {
    static FILTERS: OnceLock<Vec<BuiltinFilter>> = OnceLock::new();
    FILTERS.get_or_init(|| {
        let mut filters = vec![
            BuiltinFilter {
                id: "exporting".into(),
                label: "Exporting glyphs".into(),
                glyphset: None,
            },
            BuiltinFilter {
                id: "incompatible".into(),
                label: "Incompatible masters".into(),
                glyphset: None,
            },
        ];
        for id in [
            "GF_Arabic_Core",
            "GF_Cyrillic_Core",
            "GF_Greek_Core",
            "GF_Latin_Core",
            "GF_Latin_Plus",
        ] {
            if let Some(set) = gf_glyphsets().iter().find(|s| s.id == id) {
                filters.push(BuiltinFilter {
                    id: set.id.clone(),
                    label: set.label.clone(),
                    glyphset: Some(character_filter_for_glyphset(set)),
                });
            }
        }
        filters
    })
}

/// Does a glyph (name plus codepoints) belong to a character filter?
/// The web's glyphMatchesCharacterFilter.
pub fn glyph_matches_character_filter(
    name: &str,
    codepoints: &[u32],
    filter: &CharacterFilter,
) -> bool {
    if filter.glyph_names.iter().any(|n| n == name) {
        return true;
    }
    if codepoints.is_empty() {
        return false;
    }
    if !filter.targets.is_empty() {
        let targets: HashSet<u32> =
            filter.targets.iter().map(|t| t.unicode).collect();
        if codepoints.iter().any(|cp| targets.contains(cp)) {
            return true;
        }
    }
    if !filter.ranges.is_empty() {
        return codepoints.iter().any(|cp| {
            filter
                .ranges
                .iter()
                .any(|&(start, end)| *cp >= start && *cp <= end)
        });
    }
    false
}

/// Does a glyph belong to a script group's own row? Matches the
/// script's Unicode blocks plus the name suffix codepoint-less forms
/// use; groups with neither fall back to any-sub-filter.
pub fn glyph_matches_language_group(
    name: &str,
    codepoints: &[u32],
    group: &LanguageGroup,
) -> bool {
    if !group.script_ranges.is_empty() || group.name_suffix.is_some() {
        if let Some(suffix) = &group.name_suffix {
            let base = name.split('.').next().unwrap_or(name);
            if base.ends_with(suffix.as_str()) {
                return true;
            }
        }
        return codepoints.iter().any(|cp| {
            group
                .script_ranges
                .iter()
                .any(|&(start, end)| *cp >= start && *cp <= end)
        });
    }
    group
        .filters
        .iter()
        .any(|filter| glyph_matches_character_filter(name, codepoints, filter))
}

/// The category rows' subfilters (web CATEGORY_GROUPS), keyed by the
/// category name used in `crate::category`.
pub fn category_subfilters(category: &str) -> &'static [(&'static str, &'static str)] {
    match category {
        "Letter" => &[
            ("uppercase", "Uppercase"),
            ("lowercase", "Lowercase"),
            ("ligature", "Ligature"),
        ],
        "Number" => &[
            ("decimal", "Decimal"),
            ("fraction", "Fraction"),
            ("superior-inferior", "Superior/Inferior"),
        ],
        "Separator" => &[("space", "Space"), ("line", "Line")],
        "Punctuation" => &[
            ("quote", "Quote"),
            ("dash", "Dash"),
            ("paren", "Parenthesis"),
        ],
        "Symbol" => &[
            ("currency", "Currency"),
            ("math", "Math"),
            ("arrow", "Arrow"),
        ],
        "Mark" => &[("nonspacing", "Nonspacing"), ("spacing", "Spacing")],
        _ => &[],
    }
}

/// Category subfilters (web glyphMatchesCategorySubfilter). `category`
/// must already match; this refines within it.
pub fn glyph_matches_subfilter(
    name: &str,
    codepoints: &[u32],
    subfilter: &str,
) -> bool {
    let lower = name.to_lowercase();
    let first = codepoints.first().and_then(|&cp| char::from_u32(cp));
    match subfilter {
        "uppercase" => first.is_some_and(|c| c.is_uppercase()),
        "lowercase" => first.is_some_and(|c| c.is_lowercase()),
        "ligature" => name.contains('_') || lower.contains("liga"),
        "decimal" => codepoints.iter().any(|&cp| (0x30..=0x39).contains(&cp)),
        "fraction" => {
            lower.contains("fraction")
                || lower.contains(".dnom")
                || lower.contains(".numr")
                || first.is_some_and(|c| "¼½¾⅓⅔⁄".contains(c))
        }
        "superior-inferior" => {
            [".sups", ".subs", "superior", "inferior", ".numr", ".dnom"]
                .iter()
                .any(|p| lower.contains(p))
        }
        "space" => {
            ["space", "nbspace", "nonbreakingspace"].contains(&lower.as_str())
                || codepoints.first() == Some(&0x20)
                || codepoints.first() == Some(&0xa0)
        }
        "line" => {
            lower.contains("line")
                || codepoints.first() == Some(&0x2028)
                || codepoints.first() == Some(&0x2029)
        }
        "quote" => {
            ["quote", "quotedbl", "guillemet", "single"]
                .iter()
                .any(|p| lower.contains(p))
                || first.is_some_and(|c| "'\"‘’“”«»".contains(c))
        }
        "dash" => {
            ["dash", "hyphen", "minus"].iter().any(|p| lower.contains(p))
                || first.is_some_and(|c| "-–—−".contains(c))
        }
        "paren" => {
            ["paren", "bracket", "brace"].iter().any(|p| lower.contains(p))
                || first.is_some_and(|c| "()[]{}".contains(c))
        }
        "currency" => [
            "dollar", "cent", "sterling", "yen", "euro", "currency", "peso",
            "rupee", "won",
        ]
        .iter()
        .any(|p| lower.contains(p)),
        "math" => [
            "plus",
            "minus",
            "equal",
            "less",
            "greater",
            "divide",
            "multiply",
            "integral",
            "summation",
        ]
        .iter()
        .any(|p| lower.contains(p)),
        "arrow" => {
            lower.contains("arrow")
                || codepoints
                    .iter()
                    .any(|&cp| (0x2190..=0x21ff).contains(&cp))
        }
        "nonspacing" => lower.contains("comb") || lower.contains("mark"),
        "spacing" => !lower.contains("comb") && !lower.contains("mark"),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_glyphs() -> Vec<(String, Vec<u32>)> {
        let font = norad::Font::load(
            "../runebender-web/assets/test-fonts/VirtuaGrotesk-Regular.ufo",
        )
        .expect("fixture font");
        font.iter_layers()
            .next()
            .expect("default layer")
            .iter()
            .map(|g| {
                (
                    g.name().to_string(),
                    g.codepoints.iter().map(|c| c as u32).collect(),
                )
            })
            .collect()
    }

    fn count_filter(
        glyphs: &[(String, Vec<u32>)],
        filter: &CharacterFilter,
    ) -> usize {
        glyphs
            .iter()
            .filter(|(name, cps)| {
                glyph_matches_character_filter(name, cps, filter)
            })
            .count()
    }

    #[test]
    fn glyphsets_parse_and_expected_counts_hold() {
        let sets = gf_glyphsets();
        assert_eq!(sets.len(), 27);
        for set in sets {
            assert_eq!(set.glyph_names.len(), set.expected_count, "{}", set.id);
        }
    }

    #[test]
    fn fixture_font_coverage_matches_the_web_sidebar() {
        let glyphs = fixture_glyphs();
        let arabic = &language_groups()[0];
        assert_eq!(arabic.id, "Arab");
        // The web sidebar shows Basic Shapes 79 and Basic 152 for
        // Virtua Grotesk.
        let shapes = &arabic.filters[0];
        assert_eq!(shapes.label, "Basic Shapes");
        assert_eq!(count_filter(&glyphs, shapes), 79);
        let basic = &arabic.filters[1];
        assert_eq!(count_filter(&glyphs, basic), 152);
        // GF Arabic Core 360/383, GF Latin Core 322/324.
        let arabic_core = arabic
            .filters
            .iter()
            .find(|f| f.id == "GF_Arabic_Core")
            .unwrap();
        assert_eq!(count_filter(&glyphs, arabic_core), 360);
        assert_eq!(arabic_core.expected_count, Some(383));
        let latin_core = builtin_filters()
            .iter()
            .find(|f| f.id == "GF_Latin_Core")
            .and_then(|f| f.glyphset.as_ref())
            .unwrap();
        assert_eq!(count_filter(&glyphs, latin_core), 322);
        assert_eq!(latin_core.expected_count, Some(324));
    }

    #[test]
    fn language_group_rows_match_scripts_not_glyphset_latin() {
        let glyphs = fixture_glyphs();
        let arabic = &language_groups()[0];
        // "A" is Latin: the Arabic group row must not claim it even
        // though the GF Arabic glyphsets carry Latin punctuation.
        assert!(!glyph_matches_language_group("A", &[0x41], arabic));
        assert!(glyph_matches_language_group(
            "beh-ar.init",
            &[],
            arabic
        ));
        let count = glyphs
            .iter()
            .filter(|(name, cps)| {
                glyph_matches_language_group(name, cps, arabic)
            })
            .count();
        assert!(count > 0, "fixture font has Arabic glyphs");
    }

    #[test]
    fn subfilters_classify() {
        assert!(glyph_matches_subfilter("A", &[0x41], "uppercase"));
        assert!(!glyph_matches_subfilter("a", &[0x61], "uppercase"));
        assert!(glyph_matches_subfilter("f_i", &[], "ligature"));
        assert!(glyph_matches_subfilter("dollar", &[0x24], "currency"));
        assert!(glyph_matches_subfilter(
            "gravecomb",
            &[0x300],
            "nonspacing"
        ));
    }
}

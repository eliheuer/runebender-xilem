// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Kerning lookup algorithm
//!
//! Implements the UFO spec kerning lookup precedence:
//! 1. Glyph + glyph (highest priority)
//! 2. Glyph + group
//! 3. Group + glyph
//! 4. Group + group (lowest priority)
//! 5. Return 0.0 if no match

use std::collections::HashMap;

/// Look up the kerning value between two glyphs.
///
/// Returns the kerning adjustment value based on the UFO spec lookup precedence:
/// 1. Direct glyph-to-glyph pair (highest priority)
/// 2. Left glyph to right group pair
/// 3. Left group to right glyph pair
/// 4. Left group to right group pair (lowest priority)
/// 5. No kerning (returns 0.0)
///
/// # Arguments
/// * `kerning_pairs` - The kerning data from the workspace
/// * `groups` - The groups data from the workspace
/// * `left_glyph` - Name of the left glyph
/// * `left_group` - Optional left kerning group (public.kern1.*)
/// * `right_glyph` - Name of the right glyph
/// * `right_group` - Optional right kerning group (public.kern2.*)
///
/// # Returns
/// The kerning value to apply (positive = wider, negative = tighter), or 0.0 if no kerning
pub fn lookup_kerning(
    kerning_pairs: &HashMap<String, HashMap<String, f64>>,
    groups: &HashMap<String, Vec<String>>,
    left_glyph: &str,
    left_group: Option<&str>,
    right_glyph: &str,
    right_group: Option<&str>,
) -> f64 {
    // Priority 1: Glyph + glyph
    if let Some(value) = lookup_pair(kerning_pairs, left_glyph, right_glyph) {
        return value;
    }

    // Priority 2: Glyph + right_group
    // Check all groups that contain the right glyph
    if let Some(value) = lookup_glyph_to_group(
        kerning_pairs,
        groups,
        left_glyph,
        right_glyph,
        right_group,
        false,
    ) {
        return value;
    }

    // Priority 3: left_group + Glyph
    // Check all groups that contain the left glyph
    if let Some(value) = lookup_glyph_to_group(
        kerning_pairs,
        groups,
        right_glyph,
        left_glyph,
        left_group,
        true,
    ) {
        return value;
    }

    // Priority 4: left_group + right_group
    // Check all combinations of groups containing left and right glyphs
    if let Some(value) = lookup_group_to_group(
        kerning_pairs,
        groups,
        left_glyph,
        left_group,
        right_glyph,
        right_group,
    ) {
        return value;
    }

    // Priority 5: No kerning found
    0.0
}

/// Look up a direct glyph-to-glyph kerning pair
fn lookup_pair(
    kerning_pairs: &HashMap<String, HashMap<String, f64>>,
    first: &str,
    second: &str,
) -> Option<f64> {
    kerning_pairs.get(first)?.get(second).copied()
}

/// Look up glyph-to-group or group-to-glyph kerning
///
/// If `reverse` is false: looks up `first_glyph + group_containing_second_glyph`
/// If `reverse` is true: looks up `group_containing_first_glyph + second_glyph`
fn lookup_glyph_to_group(
    kerning_pairs: &HashMap<String, HashMap<String, f64>>,
    groups: &HashMap<String, Vec<String>>,
    first_glyph: &str,
    second_glyph: &str,
    second_group_hint: Option<&str>,
    reverse: bool,
) -> Option<f64> {
    // If we have a group hint, try that first
    if let Some(group_name) = second_group_hint {
        // Verify the glyph is actually in this group
        if let Some(group_members) = groups.get(group_name)
            && group_members.contains(&second_glyph.to_string()) {
                let value = if reverse {
                    lookup_pair(kerning_pairs, group_name, first_glyph)
                } else {
                    lookup_pair(kerning_pairs, first_glyph, group_name)
                };
                if value.is_some() {
                    return value;
                }
            }
    }

    // Search all groups for the second glyph
    for (group_name, members) in groups {
        if members.contains(&second_glyph.to_string()) {
            let value = if reverse {
                lookup_pair(kerning_pairs, group_name, first_glyph)
            } else {
                lookup_pair(kerning_pairs, first_glyph, group_name)
            };
            if value.is_some() {
                return value;
            }
        }
    }

    None
}

/// Look up group-to-group kerning
fn lookup_group_to_group(
    kerning_pairs: &HashMap<String, HashMap<String, f64>>,
    groups: &HashMap<String, Vec<String>>,
    left_glyph: &str,
    left_group_hint: Option<&str>,
    right_glyph: &str,
    right_group_hint: Option<&str>,
) -> Option<f64> {
    // Build list of groups containing left glyph
    let mut left_groups = Vec::new();
    if let Some(hint) = left_group_hint
        && let Some(members) = groups.get(hint)
            && members.contains(&left_glyph.to_string()) {
                left_groups.push(hint);
            }
    for (group_name, members) in groups {
        if members.contains(&left_glyph.to_string()) && !left_groups.contains(&group_name.as_str()) {
            left_groups.push(group_name.as_str());
        }
    }

    // Build list of groups containing right glyph
    let mut right_groups = Vec::new();
    if let Some(hint) = right_group_hint
        && let Some(members) = groups.get(hint)
            && members.contains(&right_glyph.to_string()) {
                right_groups.push(hint);
            }
    for (group_name, members) in groups {
        if members.contains(&right_glyph.to_string()) && !right_groups.contains(&group_name.as_str()) {
            right_groups.push(group_name.as_str());
        }
    }

    // Try all combinations
    for left_group in &left_groups {
        for right_group in &right_groups {
            if let Some(value) = lookup_pair(kerning_pairs, left_group, right_group) {
                return Some(value);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_kerning() -> HashMap<String, HashMap<String, f64>> {
        let mut kerning = HashMap::new();

        // Glyph + glyph pairs
        let mut a_pairs = HashMap::new();
        a_pairs.insert("V".to_string(), -50.0);
        kerning.insert("A".to_string(), a_pairs);

        // Group pairs (public.kern1.round can have multiple second members)
        let mut round_left_pairs = HashMap::new();
        round_left_pairs.insert("A".to_string(), -40.0);  // Group + glyph
        round_left_pairs.insert("public.kern2.round".to_string(), -20.0);  // Group + group
        kerning.insert("public.kern1.round".to_string(), round_left_pairs);

        // Glyph + group pairs
        let mut t_pairs = HashMap::new();
        t_pairs.insert("public.kern2.round".to_string(), -30.0);
        kerning.insert("T".to_string(), t_pairs);

        kerning
    }

    fn make_groups() -> HashMap<String, Vec<String>> {
        let mut groups = HashMap::new();
        groups.insert(
            "public.kern1.round".to_string(),
            vec!["O".to_string(), "D".to_string(), "Q".to_string()],
        );
        groups.insert(
            "public.kern2.round".to_string(),
            vec!["o".to_string(), "d".to_string(), "q".to_string()],
        );
        groups
    }

    #[test]
    fn test_glyph_to_glyph() {
        let kerning = make_kerning();
        let groups = make_groups();

        // Direct glyph-to-glyph pair
        let result = lookup_kerning(&kerning, &groups, "A", None, "V", None);
        assert_eq!(result, -50.0);
    }

    #[test]
    fn test_glyph_to_group() {
        let kerning = make_kerning();
        let groups = make_groups();

        // T + o (where o is in public.kern2.round group)
        let result = lookup_kerning(&kerning, &groups, "T", None, "o", Some("public.kern2.round"));
        assert_eq!(result, -30.0);
    }

    #[test]
    fn test_group_to_glyph() {
        let kerning = make_kerning();
        let groups = make_groups();

        // O + A (where O is in public.kern1.round group)
        let result = lookup_kerning(&kerning, &groups, "O", Some("public.kern1.round"), "A", None);
        assert_eq!(result, -40.0);
    }

    #[test]
    fn test_group_to_group() {
        let kerning = make_kerning();
        let groups = make_groups();

        // O + o (where O is in public.kern1.round and o is in public.kern2.round)
        let result = lookup_kerning(
            &kerning,
            &groups,
            "O",
            Some("public.kern1.round"),
            "o",
            Some("public.kern2.round"),
        );
        assert_eq!(result, -20.0);
    }

    #[test]
    fn test_no_kerning() {
        let kerning = make_kerning();
        let groups = make_groups();

        // No kerning defined for X + Y
        let result = lookup_kerning(&kerning, &groups, "X", None, "Y", None);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_precedence() {
        let mut kerning = make_kerning();
        let groups = make_groups();

        // Add a glyph-to-glyph pair that should override group kerning
        kerning.insert("O".to_string(), HashMap::new());
        kerning.get_mut("O").unwrap().insert("o".to_string(), -100.0);

        // O + o should use the glyph-to-glyph pair (-100) instead of group-to-group (-20)
        let result = lookup_kerning(
            &kerning,
            &groups,
            "O",
            Some("public.kern1.round"),
            "o",
            Some("public.kern2.round"),
        );
        assert_eq!(result, -100.0);
    }
}

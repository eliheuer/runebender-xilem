// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! The glyph search query language: `w>600`, `cat:Mark`, `has:anchors`.

/// One parsed search predicate: `w>600`, `cat:Mark`, `mark:red`,
/// `enc:no`, `comp:beh-ar`, `has:anchors`.
///
/// This is Counterpunch's dynamic-filter idea as search syntax.
#[derive(Clone, Debug, PartialEq)]
pub enum SearchPred {
    /// Compare the advance width with a value: `w>600`, `w<600`, `w=600`.
    Width(std::cmp::Ordering, f64),
    /// Match the glyph's category by lowercase name: `cat:mark`.
    Category(String),
    /// Match the glyph's mark label by lowercase name: `mark:red`.
    MarkLabel(String),
    /// Match whether the glyph has a Unicode codepoint: `enc:yes` or `enc:no`.
    Encoded(bool),
    /// Match glyphs that use the named glyph as a component: `comp:beh-ar`.
    UsesComponent(String),
    /// Match glyphs that have the named feature, such as `has:anchors`.
    Has(String),
}

/// Parses a whitespace-separated query into predicates. Returns `None` for an empty query, a plain-text term, or a malformed value.
pub fn parse_search_predicates(query: &str) -> Option<Vec<SearchPred>> {
    let mut preds = Vec::new();
    for term in query.split_whitespace() {
        let pred = if let Some(rest) = term.strip_prefix("w>") {
            SearchPred::Width(std::cmp::Ordering::Greater, rest.parse().ok()?)
        } else if let Some(rest) = term.strip_prefix("w<") {
            SearchPred::Width(std::cmp::Ordering::Less, rest.parse().ok()?)
        } else if let Some(rest) = term.strip_prefix("w=") {
            SearchPred::Width(std::cmp::Ordering::Equal, rest.parse().ok()?)
        } else if let Some(rest) = term.strip_prefix("cat:") {
            SearchPred::Category(rest.to_lowercase())
        } else if let Some(rest) = term.strip_prefix("mark:") {
            SearchPred::MarkLabel(rest.to_lowercase())
        } else if let Some(rest) = term.strip_prefix("enc:") {
            SearchPred::Encoded(matches!(rest, "yes" | "y" | "true"))
        } else if let Some(rest) = term.strip_prefix("comp:") {
            SearchPred::UsesComponent(rest.to_string())
        } else {
            // Any plain term means this is not a predicate query.
            let rest = term.strip_prefix("has:")?;
            SearchPred::Has(rest.to_lowercase())
        };
        preds.push(pred);
    }
    (!preds.is_empty()).then_some(preds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_predicates_parse_and_reject() {
        use std::cmp::Ordering;
        assert_eq!(
            parse_search_predicates("w>600"),
            Some(vec![SearchPred::Width(Ordering::Greater, 600.0)])
        );
        assert_eq!(
            parse_search_predicates("cat:mark enc:no"),
            Some(vec![
                SearchPred::Category("mark".into()),
                SearchPred::Encoded(false),
            ])
        );
        assert_eq!(
            parse_search_predicates("comp:beh-ar has:anchors"),
            Some(vec![
                SearchPred::UsesComponent("beh-ar".into()),
                SearchPred::Has("anchors".into()),
            ])
        );
        // Plain text stays plain text.
        assert_eq!(parse_search_predicates("beh"), None);
        assert_eq!(parse_search_predicates("w>abc"), None);
        assert_eq!(parse_search_predicates(""), None);
    }
}

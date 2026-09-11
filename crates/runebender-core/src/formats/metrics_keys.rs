// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Glyphs-style metrics keys: sidebearings derived from another glyph.

/// Lib key for the left sidebearing formula.
///
/// Metrics keys are the Glyphs spacing formulas, stored in the lib
/// keys glyphsLib round-trips:
/// `com.schriftgestaltung.Glyphs.glyph.leftMetricsKey` and
/// `rightMetricsKey`.
///
/// `=n` copies n's same sidebearing, and `=|o` copies o's opposite
/// one. `=n+10` and `=n*1.1` add arithmetic. `=50` is a constant.
pub const LEFT_METRICS_KEY: &str = "com.schriftgestaltung.Glyphs.glyph.leftMetricsKey";

/// Lib key for the right sidebearing formula. See [`LEFT_METRICS_KEY`] for the syntax.
pub const RIGHT_METRICS_KEY: &str = "com.schriftgestaltung.Glyphs.glyph.rightMetricsKey";

/// A parsed metrics-key formula.
#[derive(Clone, Debug, PartialEq)]
pub enum MetricsFormula {
    /// A fixed sidebearing value in font units, such as `=50`.
    Constant(f64),
    /// A sidebearing copied from another glyph, with optional mirroring and arithmetic.
    Reference {
        /// Name of the glyph whose sidebearing is copied.
        glyph: String,
        /// Read the opposite sidebearing of the referenced glyph.
        mirror: bool,
        /// Trailing arithmetic: ('+' | '-' | '*', value).
        op: Option<(char, f64)>,
    },
}

/// Parses a metrics key such as `=n+10`. The leading `=` is optional. Returns `None` for empty text or a malformed number.
pub fn parse_metrics_key(text: &str) -> Option<MetricsFormula> {
    let body = text.trim().trim_start_matches('=').trim();
    if body.is_empty() {
        return None;
    }
    if let Ok(v) = body.parse::<f64>() {
        return Some(MetricsFormula::Constant(v));
    }
    let (mirror, body) = match body.strip_prefix('|') {
        Some(rest) => (true, rest.trim()),
        None => (false, body),
    };
    let split = body.find(['+', '-', '*']).filter(|&i| i > 0);
    let (name, op) = match split {
        Some(i) => {
            let sign = body.as_bytes()[i] as char;
            let value = body[i + 1..].trim().parse::<f64>().ok()?;
            (body[..i].trim(), Some((sign, value)))
        }
        None => (body, None),
    };
    (!name.is_empty()).then(|| MetricsFormula::Reference {
        glyph: name.to_string(),
        mirror,
        op,
    })
}

/// Reads the left (`left == true`) or right metrics key from the glyph lib, if present.
pub fn read_metrics_key(glyph: &norad::Glyph, left: bool) -> Option<String> {
    let key = if left {
        LEFT_METRICS_KEY
    } else {
        RIGHT_METRICS_KEY
    };
    glyph
        .lib
        .get(key)
        .and_then(|v| v.as_string())
        .map(|v| v.to_string())
}

/// Writes the left (`left == true`) or right metrics key to the glyph lib. An empty or whitespace-only `value` removes the key.
pub fn write_metrics_key(glyph: &mut norad::Glyph, left: bool, value: &str) {
    let key = if left {
        LEFT_METRICS_KEY
    } else {
        RIGHT_METRICS_KEY
    };
    let value = value.trim();
    if value.is_empty() {
        glyph.lib.remove(key);
    } else {
        glyph
            .lib
            .insert(key.into(), plist::Value::String(value.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_key_parsing() {
        use MetricsFormula::*;
        assert_eq!(parse_metrics_key("=50"), Some(Constant(50.0)));
        assert_eq!(
            parse_metrics_key("=n"),
            Some(Reference {
                glyph: "n".into(),
                mirror: false,
                op: None
            })
        );
        assert_eq!(
            parse_metrics_key("=|o"),
            Some(Reference {
                glyph: "o".into(),
                mirror: true,
                op: None
            })
        );
        assert_eq!(
            parse_metrics_key("=n+10"),
            Some(Reference {
                glyph: "n".into(),
                mirror: false,
                op: Some(('+', 10.0))
            })
        );
        assert_eq!(
            parse_metrics_key("n*1.1"),
            Some(Reference {
                glyph: "n".into(),
                mirror: false,
                op: Some(('*', 1.1))
            })
        );
        assert_eq!(parse_metrics_key("  "), None);
        // A hyphenated glyph name is a name, not subtraction, only
        // when the split lands at position 0 — "beh-ar" splits at 3,
        // so this is a documented limitation: quote it as reference
        // only when no arithmetic parse works.
        assert_eq!(
            parse_metrics_key("=x-4"),
            Some(Reference {
                glyph: "x".into(),
                mirror: false,
                op: Some(('-', 4.0))
            })
        );
    }
}

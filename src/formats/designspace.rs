// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Checked Designspace decoding at the persistence boundary.
//!
//! Norad preserves its typed fields but ignores unknown XML. Reject unsupported
//! fields before decoding, so a later save cannot silently discard them.

use std::path::Path;

use norad::designspace::DesignSpaceDocument;
use quick_xml::events::Event;

/// Read a Designspace whose fields can be preserved by the source adapter.
pub fn load(path: &Path) -> Result<DesignSpaceDocument, String> {
    let xml =
        std::fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    parse(&xml).map_err(|error| format!("{}: {error}", path.display()))
}

/// Parse supported XML without silently dropping extensions or narrowing coordinates.
pub fn parse(xml: &str) -> Result<DesignSpaceDocument, String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut parents: Vec<String> = Vec::new();
    loop {
        let event = reader.read_event().map_err(|error| error.to_string())?;
        match &event {
            Event::Start(element) | Event::Empty(element) => {
                let name = String::from_utf8(element.name().as_ref().to_vec())
                    .map_err(|error| error.to_string())?;
                // Arbitrary plist metadata has its own parser and must remain opaque here.
                if !parents.iter().any(|parent| parent == "lib") {
                    let parent = parents.last().map(String::as_str).unwrap_or("");
                    let attributes: &[&str] = match (parent, name.as_str()) {
                        ("", "designspace") => &["format"],
                        ("designspace", "axes" | "sources" | "instances" | "lib")
                        | ("source" | "instance", "location")
                        | ("instance", "lib")
                        | ("rule", "conditionset") => &[],
                        ("axes", "axis") => &[
                            "name", "tag", "default", "minimum", "maximum", "hidden", "values",
                        ],
                        ("axis", "map") => &["input", "output"],
                        ("axis", "labelname") => &["xml:lang"],
                        ("sources", "source") => {
                            &["familyname", "stylename", "name", "filename", "layer"]
                        }
                        ("instances", "instance") => &[
                            "familyname",
                            "stylename",
                            "name",
                            "filename",
                            "postscriptfontname",
                            "stylemapfamilyname",
                            "stylemapstylename",
                        ],
                        ("location", "dimension") => &["name", "xvalue", "yvalue", "uservalue"],
                        ("designspace", "rules") => &["processing"],
                        ("rules", "rule") => &["name"],
                        ("rule" | "conditionset", "condition") => &["name", "minimum", "maximum"],
                        ("rule", "sub") => &["name", "with"],
                        _ => {
                            return Err(format!("unsupported Designspace element {parent}/{name}"));
                        }
                    };
                    for attribute in element.attributes() {
                        let attribute = attribute.map_err(|error| error.to_string())?;
                        let key = std::str::from_utf8(attribute.key.as_ref())
                            .map_err(|error| error.to_string())?;
                        if !attributes.contains(&key) {
                            return Err(format!("unsupported Designspace attribute {name}/{key}"));
                        }
                        if [
                            "default",
                            "minimum",
                            "maximum",
                            "input",
                            "output",
                            "xvalue",
                            "yvalue",
                            "uservalue",
                        ]
                        .contains(&key)
                        {
                            let value = attribute
                                .decoded_and_normalized_value(
                                    quick_xml::XmlVersion::Implicit1_0,
                                    reader.decoder(),
                                )
                                .map_err(|error| error.to_string())?;
                            let precise =
                                value.parse::<f64>().map_err(|error| error.to_string())?;
                            let stored = value.parse::<f32>().map_err(|error| error.to_string())?;
                            if !precise.is_finite()
                                || stored.to_string().parse::<f64>().ok() != Some(precise)
                            {
                                return Err(format!(
                                    "{name}/{key}: coordinate cannot round-trip through the Designspace adapter"
                                ));
                            }
                        }
                    }
                }
                if matches!(event, Event::Start(_)) {
                    parents.push(name);
                }
            }
            Event::End(_) => {
                parents.pop();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    quick_xml::de::from_str(xml).map_err(|error| format!("designspace: {error}"))
}

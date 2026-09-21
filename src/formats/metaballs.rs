// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UFO serialization for editable metaball groups.

pub use crate::document::model::glyph_metadata::{
    METABALLS_KEY, Metaball, MetaballGroup, MetaballLink, Metaballs,
};

/// Reads and validates live metaballs. A missing key returns an empty source.
/// Malformed or newer metadata returns an error, so a caller can preserve it untouched.
pub fn read_metaballs(glyph: &norad::Glyph) -> Result<Metaballs, String> {
    let Some(value) = glyph.lib.get(METABALLS_KEY) else {
        return Ok(Metaballs::default());
    };
    let data: Metaballs = plist::from_value(value).map_err(|e| e.to_string())?;
    data.validate()?;
    Ok(data)
}

/// Validates and writes live metaballs, returning whether the lib changed.
/// Empty sources remove the key. An error leaves the entire glyph untouched.
pub fn write_metaballs(glyph: &mut norad::Glyph, data: &Metaballs) -> Result<bool, String> {
    data.validate()?;
    if data.groups.is_empty() {
        return Ok(glyph.lib.remove(METABALLS_KEY).is_some());
    }
    let value = plist::to_value(data).map_err(|e| e.to_string())?;
    if glyph.lib.get(METABALLS_KEY) == Some(&value) {
        return Ok(false);
    }
    glyph.lib.insert(METABALLS_KEY.into(), value);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_sources_omit_links_and_v2_links_round_trip() {
        let legacy = r#"{"version":1,"groups":[{"id":7,"threshold":0.5,"balls":[{"id":1,"x":0.0,"y":0.0,"radius":100.0,"stiffness":2.0},{"id":2,"x":500.0,"y":0.0,"radius":100.0,"stiffness":2.0}]}]}"#;
        let mut source: Metaballs = serde_json::from_str(legacy).unwrap();
        source.validate().unwrap();
        assert!(source.groups[0].links.is_empty());
        let mut glyph = norad::Glyph::new("links");
        write_metaballs(&mut glyph, &source).unwrap();
        let encoded = plist::to_value(&source).unwrap();
        let groups = encoded.as_dictionary().unwrap()["groups"]
            .as_array()
            .unwrap();
        assert!(!groups[0].as_dictionary().unwrap().contains_key("links"));
        assert_eq!(read_metaballs(&glyph).unwrap(), source);
        source.version = 2;
        source.groups[0].links.push(MetaballLink {
            id: 3,
            start: 1,
            end: 2,
            width: 14.5,
        });
        write_metaballs(&mut glyph, &source).unwrap();
        let xml = glyph.encode_xml().unwrap();
        let reopened = norad::Glyph::parse_raw(&xml).unwrap();
        assert_eq!(read_metaballs(&reopened).unwrap(), source);
        let before = glyph.encode_xml().unwrap();
        source.version = 1;
        assert!(write_metaballs(&mut glyph, &source).is_err());
        assert_eq!(glyph.encode_xml().unwrap(), before);
    }
}

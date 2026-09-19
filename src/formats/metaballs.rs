// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! UFO serialization for editable metaball groups.

pub use crate::document::model::glyph_metadata::{Metaball, MetaballGroup, Metaballs};

/// Versioned glyph metadata for live metaballs; ordinary UFO anchors are unrelated.
pub const METABALLS_KEY: &str = "com.runebender.metaballs";

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

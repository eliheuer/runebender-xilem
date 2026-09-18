// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Small upstream contract reproductions; do not make UFO editing depend on these losses.

#[test]
fn upstream_layer_width_cannot_preserve_ufo_precision() {
    let width = 600.123_456_789_f64;
    let layer: babelfont::Layer = serde_json::from_value(serde_json::json!({
        "width": width,
    }))
    .expect("a width-only layer is valid Babelfont");
    assert_ne!(f64::from(layer.width), width);
}

#[test]
fn upstream_master_kerning_cannot_preserve_fractional_values() {
    let mut value = serde_json::to_value(babelfont::Master::default()).unwrap();
    value["kerning"] = serde_json::json!({"A//V": -80.5});
    assert!(serde_json::from_value::<babelfont::Master>(value).is_err());
}

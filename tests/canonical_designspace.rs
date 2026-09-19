// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Canonical variable structure must preserve supported Designspace values exactly.

use std::path::PathBuf;

use norad::Name;
use norad::designspace::{
    Axis, AxisMapping, AxisMappings, Condition, ConditionSet, DesignSpaceDocument, Dimension,
    Instance, LocalizedString, Rule, RuleProcessing, Rules, Source, Substitution,
};
use runebender::document::model::designspace::{
    CanonicalDesignspace, CanonicalLocation, SourceDescriptor, SourceOrderEntry,
    SparseSourceDescriptor,
};
use runebender::document::project::{Master, Project};
use runebender::document::var_model::Location;
use runebender::document::variable::{LayerId, SourceId};

fn dimension(name: &str, user: Option<f32>, design: Option<f32>) -> Dimension {
    Dimension {
        name: name.into(),
        uservalue: user,
        xvalue: design,
        yvalue: None,
    }
}

fn fixture() -> DesignSpaceDocument {
    let mut instance_lib = plist::Dictionary::new();
    instance_lib.insert("com.example.marker".into(), plist::Value::Integer(7.into()));
    let mut lib = plist::Dictionary::new();
    lib.insert(
        "com.example.document".into(),
        plist::Value::String("kept".into()),
    );
    DesignSpaceDocument {
        format: 5.0,
        axes: vec![
            Axis {
                name: "Weight".into(),
                tag: "wght".into(),
                default: 400.0,
                hidden: true,
                minimum: Some(100.0),
                maximum: Some(900.0),
                values: None,
                map: Some(vec![
                    AxisMapping {
                        input: 100.0,
                        output: 0.0,
                    },
                    AxisMapping {
                        input: 400.0,
                        output: 100.0,
                    },
                    AxisMapping {
                        input: 900.0,
                        output: 1000.0,
                    },
                ]),
                label_names: vec![LocalizedString {
                    language: "de".into(),
                    string: "Strichstärke".into(),
                }],
            },
            Axis {
                name: "Width".into(),
                tag: "wdth".into(),
                default: 100.0,
                hidden: false,
                minimum: Some(75.0),
                maximum: Some(125.0),
                values: None,
                map: None,
                label_names: Vec::new(),
            },
        ],
        axis_mappings: Some(AxisMappings::default()),
        rules: Rules {
            processing: RuleProcessing::Last,
            rules: vec![Rule {
                name: Some("heavy or wide".into()),
                condition_sets: vec![
                    ConditionSet {
                        conditions: vec![Condition {
                            name: "Weight".into(),
                            minimum: Some(650.25),
                            maximum: Some(1000.0),
                        }],
                    },
                    ConditionSet {
                        conditions: vec![Condition {
                            name: "Width".into(),
                            minimum: Some(110.0),
                            maximum: None,
                        }],
                    },
                ],
                substitutions: vec![Substitution {
                    name: Name::new("A").unwrap(),
                    with: Name::new("A.alt").unwrap(),
                }],
            }],
        },
        sources: vec![
            Source {
                familyname: Some("Canonical Fixture".into()),
                stylename: Some("Regular".into()),
                name: Some("regular".into()),
                filename: "Regular.ufo".into(),
                layer: None,
                location: vec![
                    dimension("Weight", None, Some(100.0)),
                    dimension("Width", Some(100.0), None),
                ],
            },
            Source {
                familyname: Some("Canonical Fixture".into()),
                stylename: Some("Intermediate".into()),
                name: Some("regular-brace".into()),
                filename: "Regular.ufo".into(),
                layer: Some("{650,110}".into()),
                location: vec![
                    dimension("Weight", None, Some(500.0)),
                    dimension("Width", Some(110.0), None),
                ],
            },
            Source {
                familyname: Some("Canonical Fixture".into()),
                stylename: Some("Bold Wide".into()),
                name: Some("bold-wide".into()),
                filename: "BoldWide.ufo".into(),
                layer: None,
                location: vec![
                    dimension("Weight", None, Some(1000.0)),
                    dimension("Width", Some(125.0), None),
                ],
            },
        ],
        instances: vec![Instance {
            familyname: Some("Canonical Fixture".into()),
            stylename: Some("Semibold Extended".into()),
            name: Some("semibold-extended".into()),
            filename: Some("instance/semibold-extended.ufo".into()),
            postscriptfontname: Some("CanonicalFixture-SemiboldExtended".into()),
            stylemapfamilyname: Some("Canonical Fixture Semibold".into()),
            stylemapstylename: Some("regular".into()),
            location: vec![
                dimension("Weight", Some(650.0), None),
                dimension("Width", None, Some(115.0)),
            ],
            lib: instance_lib,
        }],
        lib,
    }
}

#[test]
fn mapped_axes_rules_instances_and_sparse_sources_round_trip_exactly() {
    let doc = fixture();
    let canonical = CanonicalDesignspace::from_norad(&doc).unwrap();
    assert_eq!(canonical.axes().len(), 2);
    let weight = &canonical.axes()[0];
    assert_eq!(weight.design_minimum(), 0.0);
    assert_eq!(weight.design_default(), 100.0);
    assert_eq!(weight.design_maximum(), 1000.0);
    assert_eq!(weight.labels[0].value, "Strichstärke");

    assert_eq!(canonical.sources().len(), 2);
    assert_eq!(canonical.sources()[0].id(), SourceId(0));
    assert_eq!(canonical.sources()[0].display_name(), "Regular");
    assert_eq!(canonical.sources()[1].id(), SourceId(1));
    assert_eq!(canonical.sparse_sources().len(), 1);
    let sparse = &canonical.sparse_sources()[0];
    assert_eq!(sparse.layer.source, SourceId(0));
    assert_eq!(sparse.layer.name, "{650,110}");
    assert_eq!(sparse.location.design(weight), 500.0);
    assert_eq!(
        canonical.source_order(),
        [
            SourceOrderEntry::Full(SourceId(0)),
            SourceOrderEntry::Sparse(sparse.layer.clone()),
            SourceOrderEntry::Full(SourceId(1)),
        ]
    );

    assert_eq!(canonical.instances()[0].display_name(), "Semibold Extended");
    assert_eq!(canonical.rules()[0].condition_sets.len(), 2);
    assert_eq!(canonical.rules()[0].substitutions[0].replacement, "A.alt");
    assert_eq!(canonical.to_norad().unwrap(), doc);
}

#[test]
fn checked_edits_are_atomic_and_produce_semantic_compiler_inputs() {
    let mut canonical = CanonicalDesignspace::from_norad(&fixture()).unwrap();
    let axis = canonical.axes()[0].id();
    let instance = canonical.instances()[0].id();
    let rule = canonical.rules()[0].id();
    let before = canonical.clone();
    let before_inputs = canonical.compiler_structure();

    assert!(
        canonical
            .edit_checked(|draft| {
                draft.axis_mut(axis).unwrap().hidden = false;
                draft.instance_mut(instance).unwrap().style_name = Some("Extended Semibold".into());
                draft.rule_mut(rule).unwrap().condition_sets[0].conditions[0].minimum = Some(700.0);
                Ok(())
            })
            .unwrap()
    );
    assert_eq!(canonical.axes()[0].id(), axis);
    assert_eq!(canonical.instances()[0].id(), instance);
    assert_eq!(canonical.rules()[0].id(), rule);
    assert_ne!(canonical.compiler_structure(), before_inputs);

    let after_valid = canonical.clone();
    let error = canonical
        .edit_checked(|draft| {
            draft.axis_mut(axis).unwrap().coordinates.default = 400.1;
            Ok(())
        })
        .unwrap_err();
    assert!(error.contains("round-trip through Designspace"));
    assert_eq!(
        canonical, after_valid,
        "invalid edit leaked into live state"
    );
    assert_ne!(canonical, before, "valid edit did not commit");
}

#[test]
fn assigned_source_ids_and_checked_source_authoring_survive_reorder() {
    let regular = SourceId(17);
    let bold = SourceId(42);
    let mut canonical = CanonicalDesignspace::from_norad_with_source_identities(
        &fixture(),
        [
            (
                regular,
                LayerId {
                    source: regular,
                    name: "regular.default".into(),
                },
            ),
            (
                bold,
                LayerId {
                    source: bold,
                    name: "bold.default".into(),
                },
            ),
        ],
    )
    .unwrap();
    assert_eq!(
        canonical.full_source_order().collect::<Vec<_>>(),
        [regular, bold]
    );
    assert_eq!(canonical.sparse_sources()[0].layer.source, regular);

    let sparse_normalized = canonical.sparse_sources()[0]
        .location
        .to_normalized(canonical.axes())
        .unwrap();
    assert_eq!(sparse_normalized.len(), 2);
    let exact_again = CanonicalLocation::from_normalized(&sparse_normalized, canonical.axes())
        .unwrap()
        .to_normalized(canonical.axes())
        .unwrap();
    assert_eq!(exact_again, sparse_normalized);

    let new_source = SourceId(99);
    let new_normalized = Location::from([("Weight".into(), 0.25), ("Width".into(), 0.25)]);
    let new_location =
        CanonicalLocation::from_normalized(&new_normalized, canonical.axes()).unwrap();
    assert!(
        canonical
            .edit_checked(|draft| {
                assert!(draft.move_source(bold, 0)?);
                draft.source_mut(regular).unwrap().style_name = Some("Text".into());

                let sparse_layer = LayerId {
                    source: regular,
                    name: "temporary-brace".into(),
                };
                draft.insert_sparse_source(
                    SparseSourceDescriptor::new(sparse_layer, new_location.clone()),
                    draft.source_order().len(),
                )?;
                assert_eq!(
                    draft
                        .remove_sparse_at_normalized_location(&new_normalized)?
                        .len(),
                    1,
                    "promotion must remove the sparse descriptor at that location"
                );

                let source = SourceDescriptor::new(
                    new_source,
                    "Intermediate.ufo".into(),
                    new_location.clone(),
                    LayerId {
                        source: new_source,
                        name: "intermediate.default".into(),
                    },
                )?;
                draft.insert_source(source, 1)?;
                Ok(())
            })
            .unwrap()
    );
    assert_eq!(
        canonical.full_source_order().collect::<Vec<_>>(),
        [bold, new_source, regular]
    );
    assert_eq!(
        canonical.source_mut(new_source).unwrap().filename,
        "Intermediate.ufo"
    );
    assert_eq!(canonical.sources()[2].id(), regular);
    canonical.to_norad().unwrap();

    let before_remove = canonical.clone();
    assert!(
        canonical
            .edit_checked(|draft| {
                let (removed, sparse) = draft.remove_source(regular).unwrap();
                assert_eq!(removed.id(), regular);
                assert_eq!(sparse.len(), 1);
                Err("reject removal".into())
            })
            .is_err()
    );
    assert_eq!(canonical, before_remove, "rejected removal was not atomic");
}

#[test]
fn unsupported_or_ambiguous_structure_is_rejected_at_import() {
    let mut cross_axis = fixture();
    cross_axis.axis_mappings = Some(AxisMappings {
        description: Some("avar2".into()),
        mappings: vec![norad::designspace::AxisMappingEntry {
            description: None,
            input: vec![dimension("Weight", None, Some(100.0))],
            output: vec![dimension("Weight", None, Some(120.0))],
        }],
    });
    assert!(
        CanonicalDesignspace::from_norad(&cross_axis)
            .unwrap_err()
            .contains("cross-axis mappings")
    );

    let mut discrete = fixture();
    discrete.axes[0].values = Some(vec![100.0, 400.0, 900.0]);
    assert!(
        CanonicalDesignspace::from_norad(&discrete)
            .unwrap_err()
            .contains("discrete axes")
    );

    let mut anisotropic = fixture();
    anisotropic.sources[0].location[0].yvalue = Some(120.0);
    assert!(
        CanonicalDesignspace::from_norad(&anisotropic)
            .unwrap_err()
            .contains("anisotropic or ambiguous")
    );
}

#[test]
fn project_owns_the_canonical_designspace_and_snapshots_it() {
    let doc = fixture();
    let project = Project::from_designspace(doc.clone(), |filename| {
        let mut font = norad::Font::new();
        if filename == "Regular.ufo" {
            font.layers.new_layer("{650,110}").unwrap();
        }
        Ok(Master::from_font(font, PathBuf::from(filename)))
    })
    .unwrap();

    let canonical = project.document_designspace().unwrap();
    assert_eq!(canonical.to_norad().unwrap(), doc);
    assert_eq!(
        canonical.full_source_order().collect::<Vec<_>>(),
        [SourceId(0), SourceId(1)]
    );
    assert_eq!(
        project.compiler_structure().unwrap(),
        canonical.compiler_structure()
    );
    assert_eq!(project.document_snapshot().designspace(), Some(canonical));
}

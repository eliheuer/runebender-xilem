// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Guarded Project transactions for whole-glyph lifecycle operations.

use super::*;
use crate::document::CanonicalSourceStructureSnapshot;

/// Why a canonical whole-glyph transaction could not commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GlyphTransactionError {
    /// The requested edit is invalid for the captured document.
    Invalid(String),
    /// Canonical structure changed after the transaction was prepared.
    Stale,
}

impl std::fmt::Display for GlyphTransactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => formatter.write_str(message),
            Self::Stale => formatter.write_str("canonical glyph structure changed after capture"),
        }
    }
}

impl std::error::Error for GlyphTransactionError {}

#[derive(Clone, Debug)]
enum HistoryUpdate {
    None,
    Remove(String),
    Rename { old: String, new: String },
}

/// One fully staged whole-glyph replacement guarded by its canonical base.
#[derive(Clone, Debug)]
pub struct CanonicalGlyphTransaction {
    base: CanonicalSourceStructureSnapshot,
    replacement: CanonicalSourceStructureSnapshot,
    affected_layers: Vec<GlyphLayerAddress>,
    history: HistoryUpdate,
}

impl Project {
    /// Prepare an empty glyph in every source without changing the project.
    pub fn begin_add_glyph(
        &self,
        name: &str,
        width: f64,
        unicode: Option<u32>,
    ) -> Result<CanonicalGlyphTransaction, GlyphTransactionError> {
        let name = name.trim();
        let base = self.variable.source_structure_snapshot();
        let active_layer = self
            .document_sources()
            .nth(self.active)
            .expect("active source exists")
            .default_layer();
        if name.is_empty() || base.has_layer(name, &active_layer) {
            return Ok(CanonicalGlyphTransaction {
                replacement: base.clone(),
                base,
                affected_layers: Vec::new(),
                history: HistoryUpdate::None,
            });
        }
        let codepoint = match unicode {
            Some(value) => Some(char::from_u32(value).ok_or_else(|| {
                GlyphTransactionError::Invalid(format!("invalid Unicode scalar U+{value:04X}"))
            })?),
            None => (name.chars().count() == 1)
                .then(|| name.chars().next())
                .flatten(),
        };
        let mut replacement = base.clone();
        let affected_layers = replacement
            .add_empty_glyph(name, width, codepoint)
            .map_err(GlyphTransactionError::Invalid)?;
        Ok(CanonicalGlyphTransaction {
            base,
            replacement,
            affected_layers,
            history: HistoryUpdate::None,
        })
    }

    /// Prepare a duplicate with a fresh glyph identity and the first free numeric suffix.
    pub fn begin_duplicate_glyph(
        &self,
        source: &str,
    ) -> Result<(String, CanonicalGlyphTransaction), GlyphTransactionError> {
        let active_layer = self
            .document_sources()
            .nth(self.active)
            .expect("active source exists")
            .default_layer();
        if self.document_layer(source, &active_layer).is_none() {
            return Err(GlyphTransactionError::Invalid(format!(
                "missing glyph {source:?}"
            )));
        }
        let stem = source.split('.').next().unwrap_or(source);
        let mut counter = 1;
        let mut name = format!("{stem}.{counter:03}");
        while self.document_glyph(&name).is_some() {
            counter += 1;
            name = format!("{stem}.{counter:03}");
        }
        let base = self.variable.source_structure_snapshot();
        let mut replacement = base.clone();
        let affected_layers = replacement
            .duplicate_glyph(source, &name)
            .map_err(GlyphTransactionError::Invalid)?;
        Ok((
            name,
            CanonicalGlyphTransaction {
                base,
                replacement,
                affected_layers,
                history: HistoryUpdate::None,
            },
        ))
    }

    /// Prepare every missing glyph as one atomic canonical transaction.
    pub fn begin_add_missing_glyphs(
        &self,
        targets: &[(String, Option<u32>)],
        width: f64,
    ) -> Result<(usize, Option<CanonicalGlyphTransaction>), GlyphTransactionError> {
        let base = self.variable.source_structure_snapshot();
        let mut replacement = base.clone();
        let active_layer = self
            .document_sources()
            .nth(self.active)
            .expect("active source exists")
            .default_layer();
        let mut affected_layers = Vec::new();
        let mut added = 0;
        for (name, unicode) in targets {
            let name = name.trim();
            if name.is_empty() || replacement.has_layer(name, &active_layer) {
                continue;
            }
            let codepoint = match unicode {
                Some(value) => Some(char::from_u32(*value).ok_or_else(|| {
                    GlyphTransactionError::Invalid(format!("invalid Unicode scalar U+{value:04X}"))
                })?),
                None => (name.chars().count() == 1)
                    .then(|| name.chars().next())
                    .flatten(),
            };
            let added_layers = replacement
                .add_empty_glyph(name, width, codepoint)
                .map_err(GlyphTransactionError::Invalid)?;
            if !added_layers.is_empty() {
                affected_layers.extend(added_layers);
                added += 1;
            }
        }
        let transaction = (added != 0).then_some(CanonicalGlyphTransaction {
            base,
            replacement,
            affected_layers,
            history: HistoryUpdate::None,
        });
        Ok((added, transaction))
    }

    /// Prepare removal of one glyph and its direct group and kerning references.
    pub fn begin_remove_glyph(
        &self,
        name: &str,
    ) -> Result<CanonicalGlyphTransaction, GlyphTransactionError> {
        let base = self.variable.source_structure_snapshot();
        let mut replacement = base.clone();
        let affected_layers = replacement
            .remove_glyph(name)
            .map_err(GlyphTransactionError::Invalid)?;
        Ok(CanonicalGlyphTransaction {
            base,
            replacement,
            affected_layers,
            history: HistoryUpdate::Remove(name.to_owned()),
        })
    }

    /// Prepare a glyph rename with component, metrics, group and kerning references updated.
    pub fn begin_rename_glyph(
        &self,
        old: &str,
        new: &str,
    ) -> Result<CanonicalGlyphTransaction, GlyphTransactionError> {
        let new = new.trim();
        let base = self.variable.source_structure_snapshot();
        let mut replacement = base.clone();
        let affected_layers = replacement
            .rename_glyph(old, new)
            .map_err(GlyphTransactionError::Invalid)?;
        Ok(CanonicalGlyphTransaction {
            base,
            replacement,
            affected_layers,
            history: HistoryUpdate::Rename {
                old: old.to_owned(),
                new: new.to_owned(),
            },
        })
    }

    /// Commit one prepared whole-glyph replacement if its complete base is still current.
    pub fn commit_glyph_transaction(
        &mut self,
        transaction: CanonicalGlyphTransaction,
    ) -> Result<DocumentEditOutcome, GlyphTransactionError> {
        let source_metadata = transaction
            .base
            .source_ids()
            .iter()
            .copied()
            .filter(|source| {
                transaction.base.font_metadata(*source)
                    != transaction.replacement.font_metadata(*source)
            })
            .collect::<Vec<_>>();
        let dependency_names = match &transaction.history {
            HistoryUpdate::Remove(name) => vec![name.as_str()],
            HistoryUpdate::Rename { old, .. } => vec![old.as_str()],
            HistoryUpdate::None => transaction
                .affected_layers
                .iter()
                .map(|address| address.glyph.as_str())
                .collect(),
        };
        let dependent_layers = dependency_names
            .into_iter()
            .flat_map(|name| self.variable.dependent_component_layers(name))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let changed = self
            .variable
            .restore_source_structure_if_current(&transaction.base, transaction.replacement)
            .map_err(|_| GlyphTransactionError::Stale)?;
        if !changed {
            return Ok(DocumentEditOutcome::Unchanged {
                revision: self.variable.revision,
            });
        }
        match &transaction.history {
            HistoryUpdate::None => {}
            HistoryUpdate::Remove(name) => {
                self.document_history.clear_glyph(name);
                for source in &mut self.masters {
                    source.history.clear_glyph(name);
                }
            }
            HistoryUpdate::Rename { old, new } => {
                let moved = self.document_history.rename_glyph(old, new);
                debug_assert!(moved, "validated rename cannot collide with layer history");
                for source in &mut self.masters {
                    let moved = source.history.rename_glyph(old, new);
                    debug_assert!(moved, "validated rename cannot collide with source history");
                }
            }
        }
        self.refresh_glyph_projections(&transaction.history, &transaction.affected_layers);
        Ok(DocumentEditOutcome::Changed {
            revision: self.variable.revision,
            change: DocumentChange {
                affected_layers: transaction.affected_layers,
                dependent_layers,
                source_metadata,
                geometry: true,
                metrics: true,
                metadata: true,
                compilation: true,
            },
        })
    }

    /// Add one empty glyph to every source and commit one revision.
    pub fn add_document_glyph(
        &mut self,
        name: &str,
        width: f64,
        unicode: Option<u32>,
    ) -> Result<DocumentEditOutcome, GlyphTransactionError> {
        let transaction = self.begin_add_glyph(name, width, unicode)?;
        self.commit_glyph_transaction(transaction)
    }

    /// Add all missing targets in one revision and report the number inserted.
    pub fn add_missing_document_glyphs(
        &mut self,
        targets: &[(String, Option<u32>)],
        width: f64,
    ) -> Result<(usize, DocumentEditOutcome), GlyphTransactionError> {
        let (added, transaction) = self.begin_add_missing_glyphs(targets, width)?;
        let outcome = match transaction {
            Some(transaction) => self.commit_glyph_transaction(transaction)?,
            None => DocumentEditOutcome::Unchanged {
                revision: self.variable.revision,
            },
        };
        Ok((added, outcome))
    }

    /// Duplicate a glyph across all of its layers and commit one revision.
    pub fn duplicate_document_glyph(
        &mut self,
        source: &str,
    ) -> Result<(String, DocumentEditOutcome), GlyphTransactionError> {
        let (name, transaction) = self.begin_duplicate_glyph(source)?;
        let outcome = self.commit_glyph_transaction(transaction)?;
        Ok((name, outcome))
    }

    /// Remove one glyph from the canonical document and every source projection.
    pub fn remove_document_glyph(
        &mut self,
        name: &str,
    ) -> Result<DocumentEditOutcome, GlyphTransactionError> {
        let transaction = self.begin_remove_glyph(name)?;
        self.commit_glyph_transaction(transaction)
    }

    /// Rename one logical glyph and its direct references without changing its identity.
    pub fn rename_document_glyph(
        &mut self,
        old: &str,
        new: &str,
    ) -> Result<DocumentEditOutcome, GlyphTransactionError> {
        let transaction = self.begin_rename_glyph(old, new)?;
        self.commit_glyph_transaction(transaction)
    }

    fn refresh_glyph_projections(
        &mut self,
        history: &HistoryUpdate,
        affected_layers: &[GlyphLayerAddress],
    ) {
        let fonts = self
            .variable
            .source_ids
            .iter()
            .map(|source| {
                self.variable
                    .source_font(*source)
                    .expect("committed source remains projectable")
            })
            .collect::<Vec<_>>();
        let affected = affected_layers
            .iter()
            .map(|address| address.glyph.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for (source, font) in self.masters.iter_mut().zip(fonts) {
            source.font = font;
            source.dirty = true;
            source.kerning_dirty = true;
            source
                .modified_glyphs
                .extend(affected.iter().map(|name| (*name).to_owned()));
            if let HistoryUpdate::Rename { old, new } = history {
                source.modified_glyphs.remove(old);
                source.modified_glyphs.insert(new.clone());
            }
            if let HistoryUpdate::Remove(name) = history {
                source.modified_glyphs.remove(name);
            }
            source.refresh_from_font();
        }
        self.compute_compat();
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use norad::{AffineTransform, Component, Font, Glyph};

    use super::*;

    fn project() -> Project {
        let font = |width: f64| {
            let mut font = Font::new();
            let mut base = Glyph::new("A");
            base.width = width;
            base.codepoints = norad::Codepoints::new(['A']);
            base.lib
                .insert("com.example.exact".into(), plist::Value::Real(12.75));
            font.default_layer_mut().insert_glyph(base);
            let mut user = Glyph::new("Aacute");
            user.components.push(Component::new(
                norad::Name::new("A").unwrap(),
                AffineTransform::default(),
                None,
            ));
            user.lib.insert(
                crate::document::model::glyph_metadata::LEFT_METRICS_KEY.into(),
                plist::Value::String("=A+12.5".into()),
            );
            font.default_layer_mut().insert_glyph(user);
            font
        };
        let regular = Master::from_font(font(500.125), PathBuf::from("Regular.ufo"));
        let bold = Master::from_font(font(650.875), PathBuf::from("Bold.ufo"));
        let mut project = Project::from_source(regular);
        project.masters.push(bold);
        project.master_names.push("Bold".into());
        project.master_locations.push(Location::new());
        project.variable = VariableData::from_sources(&project.masters);
        project.compute_compat();
        project
    }

    #[test]
    fn add_duplicate_rename_and_remove_are_canonical() {
        let mut project = project();
        let original_id = project.document_glyph("A").unwrap().id();
        let revision = project.document_revision();

        let add = project
            .begin_add_glyph("B", 600.25, Some('B' as u32))
            .unwrap();
        assert!(matches!(
            project.commit_glyph_transaction(add).unwrap(),
            DocumentEditOutcome::Changed { .. }
        ));
        assert_eq!(project.document_revision(), revision + 1);
        assert_eq!(
            project
                .document_layer(
                    "B",
                    &project.document_sources().next().unwrap().default_layer()
                )
                .unwrap()
                .width(),
            600.25
        );

        let (copy_name, copy) = project.begin_duplicate_glyph("A").unwrap();
        project.commit_glyph_transaction(copy).unwrap();
        assert_eq!(copy_name, "A.001");
        assert_ne!(
            project.document_glyph(&copy_name).unwrap().id(),
            original_id
        );
        assert_eq!(
            project
                .sources()
                .iter()
                .map(|source| source.font.get_glyph(&copy_name).unwrap().width)
                .collect::<Vec<_>>(),
            vec![500.125, 650.875]
        );
        assert_eq!(
            project
                .document_glyph(&copy_name)
                .unwrap()
                .layer_ids()
                .next()
                .and_then(|id| project.document_layer(&copy_name, id))
                .unwrap()
                .codepoints()
                .count(),
            0
        );

        let rename = project.begin_rename_glyph("A", "A.alt").unwrap();
        project.commit_glyph_transaction(rename).unwrap();
        assert_eq!(project.document_glyph("A.alt").unwrap().id(), original_id);
        let source = project.document_sources().next().unwrap();
        let user = project
            .document_layer("Aacute", &source.default_layer())
            .unwrap();
        assert_eq!(user.components().next().unwrap().reference(), "A.alt");
        assert_eq!(user.metrics_key(true).unwrap(), Some("=A.alt+12.5"));

        let remove = project.begin_remove_glyph("A.alt").unwrap();
        project.commit_glyph_transaction(remove).unwrap();
        assert!(project.document_glyph("A.alt").is_none());
        assert!(project.sources()[0].font.get_glyph("A.alt").is_none());
    }

    #[test]
    fn add_missing_is_one_multi_source_revision() {
        let mut project = project();
        let revision = project.document_revision();
        let targets = vec![
            ("A".to_owned(), Some('A' as u32)),
            ("B".to_owned(), Some('B' as u32)),
            ("C".to_owned(), None),
            ("B".to_owned(), None),
        ];

        let (added, outcome) = project
            .add_missing_document_glyphs(&targets, 550.625)
            .unwrap();
        assert_eq!(added, 2);
        assert!(matches!(outcome, DocumentEditOutcome::Changed { .. }));
        assert_eq!(project.document_revision(), revision + 1);
        for source in project.sources() {
            assert_eq!(source.font.get_glyph("B").unwrap().width, 550.625);
            assert_eq!(source.font.get_glyph("C").unwrap().width, 550.625);
        }
    }

    #[test]
    fn add_fills_a_missing_source_without_replacing_glyph_identity() {
        let mut project = project();
        let id = project.document_glyph("A").unwrap().id();
        {
            let mut sources = project.edit_sources();
            assert!(sources[1].remove_glyph("A"));
        }
        project.active = 1;
        assert!(project.sources()[1].font.get_glyph("A").is_none());
        let revision = project.document_revision();

        let outcome = project
            .add_document_glyph("A", 700.375, Some('A' as u32))
            .unwrap();
        assert!(matches!(outcome, DocumentEditOutcome::Changed { .. }));
        assert_eq!(project.document_revision(), revision + 1);
        assert_eq!(project.document_glyph("A").unwrap().id(), id);
        assert_eq!(
            project.sources()[1].font.get_glyph("A").unwrap().width,
            700.375
        );
        assert_eq!(
            project.sources()[0].font.get_glyph("A").unwrap().width,
            500.125
        );
    }

    #[test]
    fn rejected_and_stale_transactions_leave_state_unchanged() {
        let mut project = project();
        let revision = project.document_revision();
        let no_op = project.begin_add_glyph("A", 500.0, None).unwrap();
        assert!(matches!(
            project.commit_glyph_transaction(no_op).unwrap(),
            DocumentEditOutcome::Unchanged { .. }
        ));
        assert_eq!(project.document_revision(), revision);
        assert!(project.begin_add_glyph("bad\0name", 500.0, None).is_err());
        assert_eq!(project.document_revision(), revision);

        let transaction = project.begin_rename_glyph("A", "A.alt").unwrap();
        let source = project.document_sources().next().unwrap();
        project
            .edit_document_layer("A", &source.default_layer(), |draft| {
                draft.set_width(501.0)?;
                Ok(())
            })
            .unwrap();
        let changed_revision = project.document_revision();
        assert_eq!(
            project.commit_glyph_transaction(transaction),
            Err(GlyphTransactionError::Stale)
        );
        assert_eq!(project.document_revision(), changed_revision);
        assert!(project.document_glyph("A").is_some());
        assert!(project.document_glyph("A.alt").is_none());
    }

    #[test]
    fn rename_moves_and_remove_clears_project_layer_history() {
        let mut project = project();
        let source = project.document_sources().next().unwrap();
        let old = GlyphLayerAddress {
            glyph: "A".into(),
            layer: source.default_layer(),
        };
        let mut edit = project.begin_document_layer_transaction(&old).unwrap();
        edit.draft_mut().set_width(501.25).unwrap();
        project.commit_document_layer_transaction(edit).unwrap();
        assert_eq!(
            project.document_layer_history_depth(
                &old,
                crate::document::history::HistoryDirection::Undo
            ),
            1
        );

        project.rename_document_glyph("A", "A.alt").unwrap();
        let renamed = GlyphLayerAddress {
            glyph: "A.alt".into(),
            layer: old.layer,
        };
        assert_eq!(
            project.document_layer_history_depth(
                &renamed,
                crate::document::history::HistoryDirection::Undo
            ),
            1
        );

        project.remove_document_glyph("A.alt").unwrap();
        assert_eq!(
            project.document_layer_history_depth(
                &renamed,
                crate::document::history::HistoryDirection::Undo
            ),
            0
        );
    }
}

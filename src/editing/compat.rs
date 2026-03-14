// Copyright 2025 the Runebender Xilem Authors
// SPDX-License-Identifier: Apache-2.0

//! Interpolation compatibility checking across designspace masters.
//!
//! Compares a glyph's contour structure across all masters and
//! reports mismatches (different contour counts, different point
//! counts per contour, different point types). These errors are
//! stored in the `EditSession` and drawn as red circles in the
//! editor canvas.

use crate::model::workspace::{Glyph, PointType, Workspace};
use std::sync::{Arc, RwLock};

// ================================================================
// ERROR TYPES
// ================================================================

/// A single interpolation compatibility error.
#[derive(Debug, Clone)]
pub enum CompatError {
    /// Glyph is missing from one or more masters.
    MissingGlyph {
        master_name: String,
    },

    /// Contour count differs between masters.
    ContourCountMismatch {
        master_name: String,
        expected: usize,
        actual: usize,
    },

    /// Point count differs for a specific contour.
    PointCountMismatch {
        /// Which contour (0-indexed in this master)
        contour_index: usize,
        master_name: String,
        expected: usize,
        actual: usize,
    },

    /// Point type differs at a specific position.
    PointTypeMismatch {
        contour_index: usize,
        point_index: usize,
        master_name: String,
        expected: PointType,
        actual: PointType,
    },
}

impl CompatError {
    /// Human-readable description of the error.
    pub fn description(&self) -> String {
        match self {
            Self::MissingGlyph { master_name } => {
                format!(
                    "Glyph missing from master '{master_name}'"
                )
            }
            Self::ContourCountMismatch {
                master_name,
                expected,
                actual,
            } => {
                format!(
                    "Contour count: expected {expected}, \
                     got {actual} in '{master_name}'"
                )
            }
            Self::PointCountMismatch {
                contour_index,
                master_name,
                expected,
                actual,
            } => {
                format!(
                    "Contour {contour_index}: expected \
                     {expected} points, got {actual} in \
                     '{master_name}'"
                )
            }
            Self::PointTypeMismatch {
                contour_index,
                point_index,
                master_name,
                expected,
                actual,
            } => {
                format!(
                    "Contour {contour_index}, point \
                     {point_index}: expected {expected:?}, \
                     got {actual:?} in '{master_name}'"
                )
            }
        }
    }

    /// The contour index this error relates to, if any.
    /// Used to highlight the problematic contour in the
    /// editor.
    pub fn contour_index(&self) -> Option<usize> {
        match self {
            Self::MissingGlyph { .. } => None,
            Self::ContourCountMismatch { .. } => None,
            Self::PointCountMismatch {
                contour_index, ..
            } => Some(*contour_index),
            Self::PointTypeMismatch {
                contour_index, ..
            } => Some(*contour_index),
        }
    }
}

// ================================================================
// CHECKING
// ================================================================

/// Check interpolation compatibility for a glyph across all
/// masters in a designspace. Returns an empty vec if there is
/// no designspace (single-master editing) or if all masters
/// are compatible.
///
/// `reference_glyph` is the glyph from the current active
/// master. We compare all other masters against it.
pub fn check_compat(
    glyph_name: &str,
    reference_glyph: &Glyph,
    masters: &[(String, Arc<RwLock<Workspace>>)],
) -> Vec<CompatError> {
    let mut errors = Vec::new();

    let ref_contours = &reference_glyph.contours;

    for (master_name, workspace_lock) in masters {
        let workspace = match workspace_lock.read() {
            Ok(ws) => ws,
            Err(poisoned) => poisoned.into_inner(),
        };

        let other_glyph = match workspace.glyphs.get(
            glyph_name,
        ) {
            Some(g) => g,
            None => {
                errors.push(CompatError::MissingGlyph {
                    master_name: master_name.clone(),
                });
                continue;
            }
        };

        let other_contours = &other_glyph.contours;

        // Check contour count
        if ref_contours.len() != other_contours.len() {
            errors.push(
                CompatError::ContourCountMismatch {
                    master_name: master_name.clone(),
                    expected: ref_contours.len(),
                    actual: other_contours.len(),
                },
            );
            // Can't compare point-by-point if contour
            // counts differ
            continue;
        }

        // Check each contour
        for (ci, (ref_c, other_c)) in ref_contours
            .iter()
            .zip(other_contours.iter())
            .enumerate()
        {
            // Check point count
            if ref_c.points.len() != other_c.points.len() {
                errors.push(
                    CompatError::PointCountMismatch {
                        contour_index: ci,
                        master_name: master_name.clone(),
                        expected: ref_c.points.len(),
                        actual: other_c.points.len(),
                    },
                );
                continue;
            }

            // Check point types
            for (pi, (ref_pt, other_pt)) in ref_c
                .points
                .iter()
                .zip(other_c.points.iter())
                .enumerate()
            {
                if !point_types_compatible(
                    ref_pt.point_type,
                    other_pt.point_type,
                ) {
                    errors.push(
                        CompatError::PointTypeMismatch {
                            contour_index: ci,
                            point_index: pi,
                            master_name:
                                master_name.clone(),
                            expected: ref_pt.point_type,
                            actual: other_pt.point_type,
                        },
                    );
                }
            }
        }
    }

    errors
}

/// Check if two point types are compatible for interpolation.
/// Some types are interchangeable (e.g., Curve and Line are
/// both on-curve and can interpolate).
fn point_types_compatible(a: PointType, b: PointType) -> bool {
    use PointType::*;

    match (a, b) {
        // Exact match is always fine
        (x, y) if x == y => true,

        // Off-curve control points must match
        (OffCurve, OffCurve) => true,

        // On-curve types that produce cubic segments are
        // compatible with each other
        (Curve, Line) | (Line, Curve) => true,

        // Hyper variants are compatible with each other
        (Hyper, HyperCorner) | (HyperCorner, Hyper) => true,

        // Everything else is incompatible
        _ => false,
    }
}

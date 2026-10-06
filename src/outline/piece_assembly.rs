// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Letters assembled from labeled pieces, with no model.
//!
//! A labeled sample gives one piece of ink per region: the sample's outline inside the
//! region's polygon. A typed text is then set from those pieces, each letter taking the piece
//! whose neighbors in its sample match the letter's neighbors in the text. It shows what the
//! training data says, and where a label is wrong.

use kurbo::{Affine, BezPath, Rect, Shape as _, Vec2};
use nufo::NeuralSample;

use crate::outline::label_pieces::OVERLAP;

/// Space left between words, in font units.
pub const WORD_GAP: f64 = 160.0;
/// The box drawn for a letter no sample has, in font units.
pub const MISSING_BOX: Rect = Rect::new(-240.0, -80.0, 0.0, 200.0);

/// One region's ink, with the letters it spells and their neighbors in its sample.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    /// The letters the region owns, in logical order.
    pub letters: Vec<char>,
    /// The letter written before the first owner in the same word, if any.
    pub before: Option<char>,
    /// The letter written after the last owner in the same word, if any.
    pub after: Option<char>,
    /// The sample's outline inside the region's polygon.
    pub ink: BezPath,
    /// Which sample the piece came from, unique across the font.
    pub sample: usize,
    /// The index of the first owner in the sample's text, in characters.
    pub first: u32,
    /// The index of the last owner in the sample's text, in characters.
    pub last: u32,
    /// The region came from a seed, so its polygon reaches [`OVERLAP`] into its neighbors.
    pub grown: bool,
}

/// One letter of a set text: a piece moved into place, or a box where no piece exists.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    /// The piece's ink, moved; None for a missing letter.
    pub ink: Option<BezPath>,
    /// The ink's bounds, or the missing letter's box.
    pub frame: Rect,
}

/// The pieces of one labeled sample. `outline` is the whole canvas's ink; `sample` is a
/// number that tells this sample from every other in the font.
pub fn pieces_of_sample(outline: &BezPath, item: &NeuralSample, sample: usize) -> Vec<Piece> {
    let letters = item.letters();
    let neighbors = |index: u32, step: i32| -> Option<char> {
        // The neighbor in the same word: adjacent in the text, with no space between.
        let want = i64::from(index) + i64::from(step);
        let (at, character) = letters
            .iter()
            .find(|(position, _)| i64::from(*position) == want)?;
        let _ = at;
        Some(*character)
    };
    item.regions
        .iter()
        .filter(|region| region.polygon.len() >= 3 && !region.owners.is_empty())
        .filter_map(|region| {
            let mut owners = region.owners.clone();
            owners.sort_unstable();
            let chars: Vec<char> = owners
                .iter()
                .filter_map(|owner| letters.iter().find(|(index, _)| index == owner))
                .map(|(_, character)| *character)
                .collect();
            if chars.len() != owners.len() {
                return None;
            }
            let ink = clip(outline, &region.polygon_path())?;
            Some(Piece {
                letters: chars,
                before: neighbors(owners[0], -1),
                after: neighbors(owners[owners.len() - 1], 1),
                ink,
                sample,
                first: owners[0],
                last: owners[owners.len() - 1],
                grown: region.seed.is_some(),
            })
        })
        .collect()
}

/// `outline` inside `polygon`, or None when nothing is.
fn clip(outline: &BezPath, polygon: &BezPath) -> Option<BezPath> {
    let result = linesweeper::binary_op(
        outline,
        polygon,
        linesweeper::FillRule::NonZero,
        linesweeper::BinaryOp::Intersection,
    )
    .ok()?;
    let mut ink = BezPath::new();
    for contour in result.contours() {
        ink.extend(contour.path.elements().iter().copied());
    }
    (!ink.elements().is_empty()).then_some(ink)
}

/// `text` set from `pieces`, right to left from x = 0, one entry per letter. A letter takes
/// the piece whose sample neighbors match its own; a piece that follows the previous letter's
/// piece in the same sample keeps the join between them exactly. Spaces leave a gap.
pub fn assemble(text: &str, pieces: &[Piece]) -> Vec<Placed> {
    let characters: Vec<char> = text.chars().collect();
    let mut placed = Vec::new();
    let mut cursor = 0.0;
    // The piece and move of the previous letter, to keep a sample's own joins.
    let mut previous: Option<(&Piece, Vec2)> = None;
    let mut at = 0;
    while at < characters.len() {
        let character = characters[at];
        if character.is_whitespace() {
            cursor -= WORD_GAP;
            previous = None;
            at += 1;
            continue;
        }
        let before = at
            .checked_sub(1)
            .map(|i| characters[i])
            .filter(|c| !c.is_whitespace());
        let Some(piece) = best_piece(&characters, at, before, pieces, previous.map(|p| p.0)) else {
            let frame = MISSING_BOX + Vec2::new(cursor, 0.0);
            placed.push(Placed { ink: None, frame });
            cursor = frame.x0;
            previous = None;
            at += 1;
            continue;
        };
        let bounds = piece.ink.bounding_box();
        let joined = previous
            .is_some_and(|(last, _)| last.sample == piece.sample && last.last + 1 == piece.first);
        let shift = if joined {
            previous.map(|p| p.1).unwrap_or_default()
        } else {
            // Grown pieces carry some of their neighbor's ink past the cut; pulling them
            // together by that much puts the cuts where they were.
            let reach = previous.map_or(0.0, |(last, _)| overlap_of(last))
                + if placed.is_empty() {
                    0.0
                } else {
                    overlap_of(piece)
                };
            Vec2::new(cursor - bounds.x1 + reach, 0.0)
        };
        let ink = Affine::translate(shift) * piece.ink.clone();
        let frame = bounds + shift;
        cursor = frame.x0.min(cursor);
        placed.push(Placed {
            ink: Some(ink),
            frame,
        });
        previous = Some((piece, shift));
        at += piece.letters.len();
    }
    placed
}

/// How far a piece reaches into a neighbor.
fn overlap_of(piece: &Piece) -> f64 {
    if piece.grown { OVERLAP } else { 0.0 }
}

/// The piece for the letter at `at`: the longest run of owners that matches the text, with
/// the best match of neighbors; among equals, one that continues `previous` in its sample.
fn best_piece<'a>(
    characters: &[char],
    at: usize,
    before: Option<char>,
    pieces: &'a [Piece],
    previous: Option<&Piece>,
) -> Option<&'a Piece> {
    pieces
        .iter()
        .filter(|piece| {
            let run = &characters[at..characters.len().min(at + piece.letters.len())];
            run == piece.letters.as_slice()
        })
        .max_by_key(|piece| {
            let end = at + piece.letters.len();
            let after = characters.get(end).copied().filter(|c| !c.is_whitespace());
            let continues = previous
                .is_some_and(|last| last.sample == piece.sample && last.last + 1 == piece.first);
            (
                piece.letters.len(),
                usize::from(piece.before == before) + usize::from(piece.after == after),
                continues,
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Point, Rect};

    fn square(x: f64) -> BezPath {
        Rect::new(x, 0.0, x + 100.0, 100.0).to_path(0.1)
    }

    fn piece(letters: &[char], before: Option<char>, after: Option<char>, x: f64) -> Piece {
        Piece {
            letters: letters.to_vec(),
            before,
            after,
            ink: square(x),
            sample: 0,
            first: 0,
            last: 0,
            grown: false,
        }
    }

    #[test]
    fn a_region_clips_the_sample_ink_and_knows_its_neighbors() {
        let outline = Rect::new(0.0, 0.0, 300.0, 100.0).to_path(0.1);
        let sample = NeuralSample {
            boundary: Vec::new(),
            text: "بابا".into(),
            cuts: Vec::new(),
            regions: vec![
                nufo::NeuralRegion {
                    owners: vec![1],
                    polygon: vec![
                        [100.0, -10.0],
                        [200.0, -10.0],
                        [200.0, 110.0],
                        [100.0, 110.0],
                    ],
                    seed: None,
                },
                nufo::NeuralRegion {
                    owners: vec![0],
                    polygon: vec![
                        [200.0, -10.0],
                        [300.0, -10.0],
                        [300.0, 110.0],
                        [200.0, 110.0],
                    ],
                    seed: None,
                },
            ],
        };
        let pieces = pieces_of_sample(&outline, &sample, 3);
        assert_eq!(pieces.len(), 2);
        let alif = &pieces[0];
        assert_eq!(alif.letters, vec!['ا']);
        assert_eq!((alif.before, alif.after), (Some('ب'), Some('ب')));
        let bounds = alif.ink.bounding_box();
        assert!((bounds.x0 - 100.0).abs() < 0.01 && (bounds.x1 - 200.0).abs() < 0.01);
        assert_eq!(pieces[1].before, None);
        assert_eq!(pieces[1].sample, 3);
    }

    #[test]
    fn letters_set_right_to_left_prefer_matching_neighbors_and_box_the_missing() {
        let pieces = vec![
            piece(&['ب'], None, Some('ا'), 500.0),
            piece(&['ب'], None, None, 900.0),
            piece(&['ا'], Some('ب'), None, 0.0),
        ];
        let placed = assemble("با ب", &pieces);
        assert_eq!(placed.len(), 3);
        // The first letter's right edge sits at x = 0; the next continues leftward.
        assert!((placed[0].frame.x1 - 0.0).abs() < 1e-9);
        assert!((placed[1].frame.x1 - placed[0].frame.x0).abs() < 1e-9);
        // The isolated ba after the space takes the piece with no neighbors.
        assert!(placed[2].frame.x1 < placed[1].frame.x0 - WORD_GAP + 1e-9);
        let missing = assemble("ج", &pieces);
        assert!(missing[0].ink.is_none());
        assert_eq!(missing[0].frame, MISSING_BOX);
        let _ = Point::ZERO;
        // Grown pieces pull together by their overlap, so the cuts meet.
        let mut grown = pieces.clone();
        grown[0].grown = true;
        grown[2].grown = true;
        let pulled = assemble("با", &grown);
        assert!((pulled[1].frame.x1 - pulled[0].frame.x0 - 2.0 * OVERLAP).abs() < 1e-9);
    }

    #[test]
    fn a_sample_neighbor_keeps_its_join() {
        let mut first = piece(&['ب'], None, Some('ا'), 500.0);
        first.sample = 7;
        first.first = 0;
        first.last = 0;
        let mut second = piece(&['ا'], Some('ب'), None, 350.0);
        second.sample = 7;
        second.first = 1;
        second.last = 1;
        let placed = assemble("با", &[first, second]);
        // Both moved by the same amount, so the 50-unit gap between the squares survives.
        assert!((placed[0].frame.x0 - placed[1].frame.x1 - 50.0).abs() < 1e-9);
    }
}

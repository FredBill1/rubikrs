// center_solver — solves centers for N×N cubes (N ≥ 4).
//
// Approach: Greedy commutator-based center solving.
//
// For each face (in order: U, D, F, B, R, L), we try all possible commutators
// [inner_slice, face_turn] and pick ones that strictly increase the number of
// correctly colored center pieces on the target face.
//
// This module should work for all N ≥ 4.

use rubik_core::{CubeState, Face, RotationAmount, StickerColor, TurnCommand, apply_turn_to_state};

use super::ReductionError;
use crate::center_types::{self, center_positions_on_face, CenterPiece};

/// The order in which we solve faces.
const FACE_ORDER: [Face; 6] = [
    Face::Up,
    Face::Down,
    Face::Front,
    Face::Back,
    Face::Right,
    Face::Left,
];

/// Solve all centers. Returns the sequence of turns needed.
pub fn solve_centers(state: &CubeState) -> Result<Vec<TurnCommand>, ReductionError> {
    let order = state.order.get();

    if order < 4 {
        return Err(ReductionError::InvalidOrder(format!(
            "center solver requires order >= 4, got {}",
            order
        )));
    }

    if center_types::centers_solved(state) {
        return Ok(Vec::new());
    }

    let mut current = state.clone();
    let mut all_turns: Vec<TurnCommand> = Vec::new();
    let max_iterations = (order * order * 6 * 2) as usize;
    let mut solved_faces: Vec<Face> = Vec::new();

    for &target_face in &FACE_ORDER {
        for _iter in 0..max_iterations {
            if face_centers_solved(&current, target_face, order) {
                break;
            }

            let correct_before = count_correct_on_face(&current, target_face);

            let best_turns = find_improving_commutator(
                &current, target_face, order, correct_before, &solved_faces,
            );

            match best_turns {
                Some(turns) => {
                    for &turn in &turns {
                        apply_turn_to_state(&mut current, turn).map_err(|e| {
                            ReductionError::InvalidState(format!("center turn failed: {e}"))
                        })?;
                    }
                    all_turns.extend(turns);
                }
                None => {
                    break;
                }
            }
        }

        if !face_centers_solved(&current, target_face, order) {
            return Err(ReductionError::InvalidState(format!(
                "failed to solve {:?} face after {} iterations",
                target_face, max_iterations
            )));
        }

        solved_faces.push(target_face);
    }

    Ok(all_turns)
}

/// Check if all center pieces on a given face are the correct color.
fn face_centers_solved(state: &CubeState, face: Face, order: u32) -> bool {
    let target_color = face.solved_color();
    let positions = center_positions_on_face(face, order);
    positions.iter().all(|p| p.color_in(state) == target_color)
}

/// Count how many center positions on a face have the correct color.
fn count_correct_on_face(state: &CubeState, face: Face) -> usize {
    let order = state.order.get();
    let target_color = face.solved_color();
    let positions = center_positions_on_face(face, order);
    positions
        .iter()
        .filter(|p| p.color_in(state) == target_color)
        .count()
}

/// Search for a commutator that strictly improves the target face
/// without disturbing any already-solved faces.
fn find_improving_commutator(
    state: &CubeState,
    target_face: Face,
    order: u32,
    correct_before: usize,
    solved_faces: &[Face],
) -> Option<Vec<TurnCommand>> {
    let max_depth = if order % 2 == 0 { order / 2 } else { 1 };
    for depth in 1..=max_depth {
        for &slice_face in &Face::ALL {
            let slice_cw = TurnCommand {
                face: slice_face,
                start_layer: depth,
                width: 1,
                rotation: RotationAmount::Clockwise,
            };
            let slice_ccw = slice_cw.inverse();

            // Try target face turns: CW, CCW, HalfTurn
            // Also try adjacent/solved faces with HalfTurn (preserves their appearance)
            let mut face_candidates: Vec<(Face, RotationAmount)> = vec![
                (target_face, RotationAmount::Clockwise),
                (target_face, RotationAmount::CounterClockwise),
                (target_face, RotationAmount::HalfTurn),
            ];

            // For solved faces, we can use HalfTurn which preserves the face appearance
            for &solved in solved_faces {
                if solved != target_face {
                    face_candidates.push((solved, RotationAmount::HalfTurn));
                }
            }

            // Also try target's opposite face
            let opposite = target_face.opposite();
            if !solved_faces.contains(&opposite) && opposite != target_face {
                face_candidates.push((opposite, RotationAmount::Clockwise));
                face_candidates.push((opposite, RotationAmount::CounterClockwise));
                face_candidates.push((opposite, RotationAmount::HalfTurn));
            }

            for &(face, rot) in &face_candidates {
                let face_turn = TurnCommand {
                    face,
                    start_layer: 0,
                    width: 1,
                    rotation: rot,
                };
                let face_inv = face_turn.inverse();

                for (a, a_inv) in [(slice_cw, slice_ccw), (slice_ccw, slice_cw)] {
                    // 4-move commutator
                    let commutator = vec![a, face_turn, a_inv, face_inv];
                    if try_sequence(state, &commutator, target_face, correct_before, solved_faces, order) {
                        return Some(commutator);
                    }

                    // Reverse
                    let commutator_rev = vec![face_turn, a, face_inv, a_inv];
                    if try_sequence(state, &commutator_rev, target_face, correct_before, solved_faces, order) {
                        return Some(commutator_rev);
                    }

                    // 3-move
                    let seq3 = vec![a, face_turn, a_inv];
                    if try_sequence(state, &seq3, target_face, correct_before, solved_faces, order) {
                        return Some(seq3);
                    }
                }
            }
        }
    }
    None
}

/// Simulate a sequence and check that:
/// 1. The target face strictly improves (more correct centers)
/// 2. All previously solved faces remain solved
fn try_sequence(
    state: &CubeState,
    turns: &[TurnCommand],
    target_face: Face,
    correct_before: usize,
    solved_faces: &[Face],
    order: u32,
) -> bool {
    let mut sim = state.clone();
    let mut scratch = sim.stickers.clone();
    for &turn in turns {
        if rubik_core::apply_turn_to_state_unchecked(&mut sim, turn, &mut scratch).is_err() {
            return false;
        }
    }

    // Check target face improvement
    if count_correct_on_face(&sim, target_face) <= correct_before {
        return false;
    }

    // Check solved faces are still solved
    for &f in solved_faces {
        if !face_centers_solved(&sim, f, order) {
            return false;
        }
    }

    true
}

/// Find a center piece of `target_color` on `source_face`.
pub fn find_matching_center(
    state: &CubeState,
    target_color: StickerColor,
    source_face: Face,
) -> Option<CenterPiece> {
    let order = state.order.get();
    let positions = center_positions_on_face(source_face, order);

    for piece in positions {
        if piece.color_in(state) == target_color {
            return Some(piece);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::{CubeOrder, RotationAmount, apply_turn_to_state};

    #[test]
    fn test_solved_centers_no_moves() {
        let state = CubeState::solved(CubeOrder::new(4).unwrap());
        let turns = solve_centers(&state).unwrap();
        assert!(turns.is_empty());
    }

    #[test]
    fn test_center_solver_scrambled_4x4() {
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        // [r, U] = r U r' U' — a commutator that cycles 3 center pieces
        let scramble = vec![
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::Clockwise,
            }, // r
            TurnCommand::outer(Face::Up, RotationAmount::Clockwise), // U
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::CounterClockwise,
            }, // r'
            TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise), // U'
        ];

        for &turn in &scramble {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        assert!(!crate::center_types::centers_solved(&state));

        let turns = solve_centers(&state).unwrap();
        assert!(!turns.is_empty(), "solver should produce moves");

        for &turn in &turns {
            apply_turn_to_state(&mut state, turn).unwrap();
        }
        assert!(
            crate::center_types::centers_solved(&state),
            "centers should be solved after applying solver moves"
        );
    }

    #[test]
    fn test_find_matching_center() {
        let state = CubeState::solved(CubeOrder::new(4).unwrap());
        let piece = find_matching_center(&state, rubik_core::StickerColor::White, Face::Up);
        assert!(piece.is_some());
        assert_eq!(piece.unwrap().face, Face::Up);
    }

    #[test]
    fn test_center_solver_commutator_4x4() {
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        let commutator = vec![
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::Clockwise,
            },
            TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::CounterClockwise,
            },
            TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
        ];

        for &turn in &commutator {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        assert!(!crate::center_types::centers_solved(&state));

        let turns = solve_centers(&state).unwrap();
        assert!(!turns.is_empty(), "solver should produce moves");

        for &turn in &turns {
            apply_turn_to_state(&mut state, turn).unwrap();
        }
        assert!(
            crate::center_types::centers_solved(&state),
            "centers should be solved"
        );

        assert!(
            turns.len() <= 12,
            "solver should produce at most ~12 moves for a single commutator; got {}",
            turns.len()
        );
    }

    #[test]
    fn test_center_solver_scrambled_5x5() {
        let mut state = CubeState::solved(CubeOrder::new(5).unwrap());

        // [r, U] = r U r' U' — pure commutator on 5x5 (inner slice layer 1)
        let scramble = vec![
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::Clockwise,
            },
            TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::CounterClockwise,
            },
            TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
        ];

        for &turn in &scramble {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        assert!(!crate::center_types::centers_solved(&state));

        let turns = solve_centers(&state).unwrap();
        assert!(!turns.is_empty(), "solver should produce moves");

        for &turn in &turns {
            apply_turn_to_state(&mut state, turn).unwrap();
        }
        assert!(
            crate::center_types::centers_solved(&state),
            "centers should be solved for 5x5"
        );
    }

    #[test]
    fn test_center_solver_scrambled_6x6() {
        let mut state = CubeState::solved(CubeOrder::new(6).unwrap());

        // [r, U] = r U r' U' with inner slice layer 1
        let scramble = vec![
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::Clockwise,
            },
            TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::CounterClockwise,
            },
            TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
        ];

        for &turn in &scramble {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        assert!(!crate::center_types::centers_solved(&state));

        let turns = solve_centers(&state).unwrap();
        assert!(!turns.is_empty(), "solver should produce moves for 6x6");

        for &turn in &turns {
            apply_turn_to_state(&mut state, turn).unwrap();
        }
        assert!(
            crate::center_types::centers_solved(&state),
            "centers should be solved for 6x6"
        );
    }

    #[test]
    fn test_center_solver_deterministic() {
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        let scramble = vec![
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::Clockwise,
            },
            TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::CounterClockwise,
            },
            TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
        ];

        for &turn in &scramble {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        let turns1 = solve_centers(&state).unwrap();
        let turns2 = solve_centers(&state).unwrap();
        assert_eq!(turns1, turns2, "solver must be deterministic");
    }
}

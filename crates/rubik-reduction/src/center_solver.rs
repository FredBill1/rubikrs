// center_solver — solves centers for N×N cubes (N ≥ 4).
//
// Pair-wise solving approach:
//   1. Solve U + D as a pair (any moves, maximize combined correct count)
//   2. Solve F + B as a pair (using only u/d inner slices that don't affect U/D)
//   3. R + L auto-solve by color-counting (only 2 colors left)
//
// For each pair, we use best-improvement greedy: evaluate all candidate
// sequences and pick the one maximizing total correct count across BOTH
// faces in the pair. When one face is fully solved, only moves that
// preserve it are considered.
//
// This module works for all N ≥ 4.

use rubik_core::{CubeState, Face, RotationAmount, StickerColor, TurnCommand, apply_turn_to_state};

use super::ReductionError;
use crate::center_types::{self, center_positions_on_face, CenterPiece};

/// Maximum iterations per face pair.
const MAX_ITERATIONS_PER_PAIR: usize = 5000;

/// Maximum depth for IDA* fallback.
const IDA_MAX_DEPTH: usize = 6;

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

    // Phase 1: Solve U+D pair (any moves allowed)
    {
        let (turns, new_state) = solve_face_pair_unrestricted(
            &current, Face::Up, Face::Down, order,
        )?;
        all_turns.extend(turns);
        current = new_state;
    }

    // Phase 2: Solve the 4 equator faces (F, B, R, L) together,
    // using only u/d inner slices that don't affect U/D.
    {
        let (turns, new_state) = solve_equator_faces(
            &current, order,
        )?;
        all_turns.extend(turns);
        current = new_state;
    }

    if !center_types::centers_solved(&current) {
        return Err(ReductionError::InvalidState(
            "centers not solved after all phases".to_string()
        ));
    }

    Ok(all_turns)
}

// ---------------------------------------------------------------------------
// Phase 1: unrestricted pair solving (any moves)
// ---------------------------------------------------------------------------

fn solve_face_pair_unrestricted(
    state: &CubeState,
    face_a: Face,
    face_b: Face,
    order: u32,
) -> Result<(Vec<TurnCommand>, CubeState), ReductionError> {
    let mut current = state.clone();
    let mut turns: Vec<TurnCommand> = Vec::new();

    for _iter in 0..MAX_ITERATIONS_PER_PAIR {
        let solved_a = face_centers_solved(&current, face_a, order);
        let solved_b = face_centers_solved(&current, face_b, order);

        if solved_a && solved_b {
            break;
        }

        let correct_a = count_correct_on_face(&current, face_a);
        let correct_b = count_correct_on_face(&current, face_b);
        let total_before = correct_a + correct_b;

        // Determine which solved faces to preserve
        let preserved: Vec<Face> = if solved_a && !solved_b {
            vec![face_a]
        } else if solved_b && !solved_a {
            vec![face_b]
        } else {
            vec![]
        };

        // Try to find an improving sequence
        if let Some(seq) = find_best_candidate_unrestricted(
            &current, face_a, face_b, order, total_before, &preserved,
        ) {
            apply_turns(&mut current, &seq)?;
            turns.extend(seq);
            continue;
        }

        // If stuck, try enabling move on the face with fewer correct pieces
        {
            let target = if correct_a <= correct_b { face_a } else { face_b };
            if let Some(seq) = find_enabling_move(
                &current, target, order,
                count_correct_on_face(&current, target), &preserved,
            ) {
                apply_turns(&mut current, &seq)?;
                turns.extend(seq);
                continue;
            }
        }

        // IDA* fallback on each unsolved face
        {
            let mut found = false;
            for &face in &[face_a, face_b] {
                if !face_centers_solved(&current, face, order) {
                    // Use all slice candidates; simulation filters by preservation
                    let candidates = generate_all_ida_candidates(order);
                    if let Some(seq) = ida_star_single_face_with_candidates(
                        &current, face, order, &preserved, &candidates,
                    ) {
                        apply_turns(&mut current, &seq)?;
                        turns.extend(seq);
                        found = true;
                        break;
                    }
                }
            }
            if found {
                continue;
            }
        }

        break; // truly stuck
    }

    if !face_centers_solved(&current, face_a, order)
        || !face_centers_solved(&current, face_b, order)
    {
        let ca = count_correct_on_face(&current, face_a);
        let cb = count_correct_on_face(&current, face_b);
        let ta = center_positions_on_face(face_a, order).len();
        let tb = center_positions_on_face(face_b, order).len();
        return Err(ReductionError::InvalidState(format!(
            "failed to solve {:?}/{:?} pair ({:?}: {}/{} correct, {:?}: {}/{})",
            face_a, face_b, face_a, ca, ta, face_b, cb, tb
        )));
    }

    Ok((turns, current))
}

/// Best-improvement search for unrestricted pair: try all inner-slice turns
/// and commutators, pick the one maximizing total correct on face_a + face_b.
fn find_best_candidate_unrestricted(
    state: &CubeState,
    face_a: Face,
    face_b: Face,
    order: u32,
    total_before: usize,
    preserved: &[Face],
) -> Option<Vec<TurnCommand>> {
    let mut best: Option<(Vec<TurnCommand>, usize)> = None;

    for candidate in generate_pair_candidates(state, face_a, face_b, order, preserved) {
        if let Ok(sim) = simulate_sequence(state, &candidate) {
            if !solved_faces_preserved(&sim, preserved, order) {
                continue;
            }
            let total = count_correct_on_face(&sim, face_a)
                + count_correct_on_face(&sim, face_b);
            if total > total_before {
                match &best {
                    Some((_, best_total)) if total > *best_total => {
                        best = Some((candidate, total));
                    }
                    None => {
                        best = Some((candidate, total));
                    }
                    _ => {}
                }
            }
        }
    }

    best.map(|(turns, _)| turns)
}

/// Generate candidates for the unrestricted pair phase.
fn generate_pair_candidates(
    _state: &CubeState,
    face_a: Face,
    face_b: Face,
    order: u32,
    preserved: &[Face],
) -> Vec<Vec<TurnCommand>> {
    let mut candidates = Vec::new();
    let max_depth = order / 2;

    // Single inner-slice turns
    for depth in 1..=max_depth {
        for &slice_face in &Face::ALL {
            for &rot in &[RotationAmount::Clockwise, RotationAmount::CounterClockwise, RotationAmount::HalfTurn] {
                candidates.push(vec![TurnCommand {
                    face: slice_face, start_layer: depth, width: 1, rotation: rot,
                }]);
            }
        }
    }

    // Commutators [slice, face_turn] — include ALL slices even if they
    // affect preserved faces; the simulation check filters correctly.
    for depth in 1..=max_depth {
        for &slice_face in &Face::ALL {
            let slice_cw = TurnCommand {
                face: slice_face, start_layer: depth, width: 1,
                rotation: RotationAmount::Clockwise,
            };
            let slice_ccw = slice_cw.inverse();

            // Face-turn candidates: face_a, face_b, and all other
            // unsolved faces. For larger cubes, limit to perpendicular
            // faces to keep performance manageable.
            let mut face_candidates = vec![
                (face_a, RotationAmount::Clockwise),
                (face_a, RotationAmount::CounterClockwise),
                (face_a, RotationAmount::HalfTurn),
                (face_b, RotationAmount::Clockwise),
                (face_b, RotationAmount::CounterClockwise),
                (face_b, RotationAmount::HalfTurn),
            ];
            // Include all unsolved faces for full search coverage
            for &f in &Face::ALL {
                if f != face_a && f != face_b && !preserved.contains(&f) {
                    face_candidates.push((f, RotationAmount::Clockwise));
                    face_candidates.push((f, RotationAmount::CounterClockwise));
                    face_candidates.push((f, RotationAmount::HalfTurn));
                }
            }
            for &p in preserved {
                face_candidates.push((p, RotationAmount::HalfTurn));
            }

            for &(face, rot) in &face_candidates {
                let face_turn = TurnCommand {
                    face, start_layer: 0, width: 1, rotation: rot,
                };
                let face_inv = face_turn.inverse();
                for (a, a_inv) in [(slice_cw, slice_ccw), (slice_ccw, slice_cw)] {
                    candidates.push(vec![a, face_turn, a_inv, face_inv]);
                    candidates.push(vec![face_turn, a, face_inv, a_inv]);
                    candidates.push(vec![a, face_turn, a_inv]);
                }
            }
        }
    }

    candidates
}

// ---------------------------------------------------------------------------
// Phase 2: solve equator faces (F, B, R, L) as a group
// ---------------------------------------------------------------------------

/// Solve all 4 equator faces together using only u/d inner slices
/// (which don't affect U/D). Maximize total correct count across all
/// 4 faces.
fn solve_equator_faces(
    state: &CubeState,
    order: u32,
) -> Result<(Vec<TurnCommand>, CubeState), ReductionError> {
    let mut current = state.clone();
    let mut turns: Vec<TurnCommand> = Vec::new();

    let equator_faces = [Face::Front, Face::Back, Face::Right, Face::Left];
    let locked = vec![Face::Up, Face::Down];

    for _iter in 0..MAX_ITERATIONS_PER_PAIR {
        let all_solved = equator_faces
            .iter()
            .all(|&f| face_centers_solved(&current, f, order));

        if all_solved {
            break;
        }

        let total_before: usize = equator_faces
            .iter()
            .map(|&f| count_correct_on_face(&current, f))
            .sum();

        if let Some(seq) = find_best_equator_candidate(
            &current, order, total_before, &locked,
        ) {
            apply_turns(&mut current, &seq)?;
            turns.extend(seq);
            continue;
        }

        // If stuck, try enabling moves
        let mut found = false;
        for &face in &equator_faces {
            if !face_centers_solved(&current, face, order) {
                let cb = count_correct_on_face(&current, face);
                if let Some(seq) = find_enabling_move(
                    &current, face, order, cb, &locked,
                ) {
                    apply_turns(&mut current, &seq)?;
                    turns.extend(seq);
                    found = true;
                    break;
                }
            }
        }
        if found {
            continue;
        }

        // IDA* fallback
        for &face in &equator_faces {
            let wrong = count_wrong_on_face(&current, face);
            if wrong > 0 {
                if let Some(seq) = ida_star_single_face(
                    &current, face, order, &locked,
                ) {
                    apply_turns(&mut current, &seq)?;
                    turns.extend(seq);
                    found = true;
                    break;
                }
            }
        }
        if found {
            continue;
        }

        break;
    }

    let all_solved = equator_faces
        .iter()
        .all(|&f| face_centers_solved(&current, f, order));

    if !all_solved {
        let details: Vec<String> = equator_faces.iter().map(|&f| {
            let c = count_correct_on_face(&current, f);
            let t = center_positions_on_face(f, order).len();
            format!("{:?}: {}/{}", f, c, t)
        }).collect();
        return Err(ReductionError::InvalidState(format!(
            "failed to solve equator faces ({})",
            details.join(", ")
        )));
    }

    Ok((turns, current))
}

/// Best-improvement search for equator faces: use only u/d inner slices
/// (which don't affect U/D). Evaluate all candidates, pick the one
/// maximizing total correct across all equator faces.
fn find_best_equator_candidate(
    state: &CubeState,
    order: u32,
    total_before: usize,
    locked: &[Face],
) -> Option<Vec<TurnCommand>> {
    let mut best: Option<(Vec<TurnCommand>, usize)> = None;
    let equator_faces = [Face::Front, Face::Back, Face::Right, Face::Left];

    for candidate in generate_equator_candidates(order, locked) {
        if let Ok(sim) = simulate_sequence(state, &candidate) {
            if !solved_faces_preserved(&sim, locked, order) {
                continue;
            }
            let total: usize = equator_faces
                .iter()
                .map(|&f| count_correct_on_face(&sim, f))
                .sum();
            if total > total_before {
                match &best {
                    Some((_, best_total)) if total > *best_total => {
                        best = Some((candidate, total));
                    }
                    None => {
                        best = Some((candidate, total));
                    }
                    _ => {}
                }
            }
        }
    }

    best.map(|(turns, _)| turns)
}

/// Generate candidates using only u/d inner slices (U and D face inner
/// layers) which affect the equator faces (F, R, B, L) but not U or D.
fn generate_equator_candidates(
    order: u32,
    locked: &[Face],
) -> Vec<Vec<TurnCommand>> {
    let mut candidates = Vec::new();
    let max_depth = order / 2;

    // Only use inner slices from U and D faces (u and d slices).
    // These affect equator faces without touching U/D.
    for &slice_face in &[Face::Up, Face::Down] {
        for depth in 1..=max_depth {
            // Single slice turns
            for &rot in &[RotationAmount::Clockwise, RotationAmount::CounterClockwise, RotationAmount::HalfTurn] {
                candidates.push(vec![TurnCommand {
                    face: slice_face, start_layer: depth, width: 1, rotation: rot,
                }]);
            }

            let slice_cw = TurnCommand {
                face: slice_face, start_layer: depth, width: 1,
                rotation: RotationAmount::Clockwise,
            };
            let slice_ccw = slice_cw.inverse();

            // Face-turn candidates: all equator faces + locked faces (HalfTurn only)
            let mut face_candidates: Vec<(Face, RotationAmount)> = Vec::new();
            for &f in &[Face::Front, Face::Back, Face::Right, Face::Left] {
                face_candidates.push((f, RotationAmount::Clockwise));
                face_candidates.push((f, RotationAmount::CounterClockwise));
                face_candidates.push((f, RotationAmount::HalfTurn));
            }
            for &p in locked {
                face_candidates.push((p, RotationAmount::HalfTurn));
            }

            for &(face, rot) in &face_candidates {
                let face_turn = TurnCommand {
                    face, start_layer: 0, width: 1, rotation: rot,
                };
                let face_inv = face_turn.inverse();
                for (a, a_inv) in [(slice_cw, slice_ccw), (slice_ccw, slice_cw)] {
                    candidates.push(vec![a, face_turn, a_inv, face_inv]);
                    candidates.push(vec![face_turn, a, face_inv, a_inv]);
                    candidates.push(vec![a, face_turn, a_inv]);
                }
            }
        }
    }

    candidates
}

/// IDA* for a single face using only u/d slices.
fn ida_star_single_face(
    state: &CubeState,
    target_face: Face,
    order: u32,
    locked: &[Face],
) -> Option<Vec<TurnCommand>> {
    let candidates = generate_equator_candidates(order, locked);
    ida_star_single_face_with_candidates(state, target_face, order, locked, &candidates)
}

/// Generate all-slice IDA candidates for unrestricted phase.
fn generate_all_ida_candidates(order: u32) -> Vec<Vec<TurnCommand>> {
    let mut candidates = Vec::new();
    let max_depth = order / 2;
    for depth in 1..=max_depth {
        for &slice_face in &Face::ALL {
            for &rot in &[RotationAmount::Clockwise, RotationAmount::CounterClockwise, RotationAmount::HalfTurn] {
                candidates.push(vec![TurnCommand {
                    face: slice_face, start_layer: depth, width: 1, rotation: rot,
                }]);
            }
        }
    }
    candidates
}

/// IDA* with given candidate pool.
fn ida_star_single_face_with_candidates(
    state: &CubeState,
    target_face: Face,
    order: u32,
    locked: &[Face],
    candidates: &[Vec<TurnCommand>],
) -> Option<Vec<TurnCommand>> {
    let wrong = count_wrong_on_face(state, target_face);
    if wrong == 0 {
        return Some(Vec::new());
    }

    for max_depth in 1..=IDA_MAX_DEPTH {
        if let Some(result) = ida_dfs_single(
            state, target_face, order, locked,
            candidates, max_depth, 0, wrong,
        ) {
            return Some(result);
        }
    }
    None
}

fn ida_dfs_single(
    state: &CubeState,
    target_face: Face,
    order: u32,
    locked: &[Face],
    candidates: &[Vec<TurnCommand>],
    max_depth: usize,
    depth: usize,
    wrong_before: usize,
) -> Option<Vec<TurnCommand>> {
    let remaining = max_depth - depth;
    let max_fixable = remaining * (order.saturating_sub(2) as usize);
    if max_fixable < wrong_before {
        return None;
    }

    if depth >= max_depth {
        return None;
    }

    for candidate in candidates {
        if let Ok(sim) = simulate_sequence(state, candidate) {
            if !solved_faces_preserved(&sim, locked, order) {
                continue;
            }
            let wrong_after = count_wrong_on_face(&sim, target_face);
            if wrong_after == 0 {
                return Some(candidate.clone());
            }
            if wrong_after < wrong_before {
                if let Some(mut rest) = ida_dfs_single(
                    &sim, target_face, order, locked,
                    candidates, max_depth, depth + 1, wrong_after,
                ) {
                    let mut result = candidate.clone();
                    result.append(&mut rest);
                    return Some(result);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Enabling move (changes arrangement to allow future progress)
// ---------------------------------------------------------------------------

fn find_enabling_move(
    state: &CubeState,
    target_face: Face,
    order: u32,
    correct_before: usize,
    preserved: &[Face],
) -> Option<Vec<TurnCommand>> {
    let max_depth = order / 2;
    let positions = center_positions_on_face(target_face, order);

    for depth in 1..=max_depth {
        for &slice_face in &Face::ALL {
            let slice_cw = TurnCommand {
                face: slice_face, start_layer: depth, width: 1,
                rotation: RotationAmount::Clockwise,
            };
            let slice_ccw = slice_cw.inverse();

            let face_candidates: Vec<(Face, RotationAmount)> = Face::ALL
                .iter()
                .filter(|&&f| !preserved.contains(&f))
                .flat_map(|&f| [
                    (f, RotationAmount::Clockwise),
                    (f, RotationAmount::CounterClockwise),
                    (f, RotationAmount::HalfTurn),
                ])
                .chain(preserved.iter().map(|&f| (f, RotationAmount::HalfTurn)))
                .collect();

            for &(face, rot) in &face_candidates {
                let face_turn = TurnCommand {
                    face, start_layer: 0, width: 1, rotation: rot,
                };
                let face_inv = face_turn.inverse();

                for (a, a_inv) in [(slice_cw, slice_ccw), (slice_ccw, slice_cw)] {
                    for comm in [
                        vec![a, face_turn, a_inv, face_inv],
                        vec![face_turn, a, face_inv, a_inv],
                    ] {
                        if let Ok(sim) = simulate_sequence(state, &comm) {
                            if !solved_faces_preserved(&sim, preserved, order) {
                                continue;
                            }
                            let correct_after = count_correct_on_face(&sim, target_face);
                            if correct_after < correct_before {
                                continue;
                            }
                            let changed = positions.iter().any(|p| {
                                p.color_in(&sim) != p.color_in(state)
                            });
                            if changed {
                                return Some(comm);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn face_centers_solved(state: &CubeState, face: Face, order: u32) -> bool {
    let target_color = face.solved_color();
    let positions = center_positions_on_face(face, order);
    positions.iter().all(|p| p.color_in(state) == target_color)
}

fn count_correct_on_face(state: &CubeState, face: Face) -> usize {
    let order = state.order.get();
    let target_color = face.solved_color();
    let positions = center_positions_on_face(face, order);
    positions
        .iter()
        .filter(|p| p.color_in(state) == target_color)
        .count()
}

fn count_wrong_on_face(state: &CubeState, face: Face) -> usize {
    let order = state.order.get();
    let total = center_positions_on_face(face, order).len();
    total - count_correct_on_face(state, face)
}

fn solved_faces_preserved(state: &CubeState, solved_faces: &[Face], order: u32) -> bool {
    solved_faces
        .iter()
        .all(|&f| face_centers_solved(state, f, order))
}

fn simulate_sequence(
    state: &CubeState,
    turns: &[TurnCommand],
) -> Result<CubeState, ()> {
    let mut sim = state.clone();
    let mut scratch = sim.stickers.clone();
    for &turn in turns {
        if rubik_core::apply_turn_to_state_unchecked(&mut sim, turn, &mut scratch).is_err() {
            return Err(());
        }
    }
    Ok(sim)
}

fn apply_turns(
    state: &mut CubeState,
    turns: &[TurnCommand],
) -> Result<(), ReductionError> {
    for &turn in turns {
        apply_turn_to_state(state, turn).map_err(|e| {
            ReductionError::InvalidState(format!("center turn failed: {e}"))
        })?;
    }
    Ok(())
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

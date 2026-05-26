// center_solver — solves centers for N×N cubes (N ≥ 4).
//
// Two-phase solver with greedy + BFS/IDA* fallback:
//   Phase 1: Solve U+D pair — greedy first, then BFS at shallow depth,
//            then enabling/2-ply/IDA*/shake fallbacks.
//   Phase 2: Solve equator faces (F, B, R, L) one at a time using only
//            u/d inner-slice moves — same layered fallback strategy.
//
// The BFS uses pre-generated commutator sequences as "moves" so that
// depth-2 BFS already covers 8 individual turns. Depths are kept shallow
// (2-3 strict, 3-4 break-even) to produce short solution sequences that
// don't scramble edges excessively.
//
// This module works for all N ≥ 4.

use std::collections::{HashSet, VecDeque};
use rubik_core::{CubeState, Face, RotationAmount, StickerColor, TurnCommand, apply_turn_to_state};

use super::ReductionError;
use crate::center_types::{self, center_positions_on_face, CenterPiece};

const MAX_ITERATIONS_PER_PAIR: usize = 200;

/// Hard cap on visited states during BFS to prevent runaway memory/time.
const BFS_VISITED_CAP: usize = 200_000;

/// Maximum depth for IDA* fallback.
const IDA_MAX_DEPTH: usize = 6;

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

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

    // Phase 1: Solve U+D pair — BFS with any center-affecting moves
    {
        let (turns, new_state) = solve_face_pair_bfs(
            &current, Face::Up, Face::Down, order,
        )?;
        all_turns.extend(turns);
        current = new_state;
    }

    // Phase 2: Solve equator faces one at a time — BFS with u/d-only moves
    {
        let (turns, new_state) = solve_equator_faces_bfs(
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

    // Simplify the turn sequence to reduce edge scrambling.
    // This cancels adjacent inverse moves and merges same-face same-slice turns,
    // which can significantly reduce the sequence length without changing
    // the center-solving effect.
    Ok(simplify_center_turns(&all_turns, order))
}

// ---------------------------------------------------------------------------
// Phase 1: U+D pair with BFS
// ---------------------------------------------------------------------------

fn solve_face_pair_bfs(
    state: &CubeState,
    face_a: Face,
    face_b: Face,
    order: u32,
) -> Result<(Vec<TurnCommand>, CubeState), ReductionError> {
    let mut current = state.clone();
    let mut turns: Vec<TurnCommand> = Vec::new();

    // Pre-generate all BFS moves for this order
    let bf_moves = generate_pair_bfs_moves(order);

    // Full candidate set for fallback strategies
    let full_candidates = generate_pair_candidates(order);

    for _iter in 0..MAX_ITERATIONS_PER_PAIR {
        let solved_a = face_centers_solved(&current, face_a, order);
        let solved_b = face_centers_solved(&current, face_b, order);

        if solved_a && solved_b {
            break;
        }

        let correct_a = count_correct_on_face(&current, face_a);
        let correct_b = count_correct_on_face(&current, face_b);
        let total_before = correct_a + correct_b;

        let preserved: Vec<Face> = if solved_a && !solved_b {
            vec![face_a]
        } else if solved_b && !solved_a {
            vec![face_b]
        } else {
            vec![]
        };

        // 1. Greedy best single commutator — cheap and produces short sequences.
        if let Some(seq) = find_best_candidate_unrestricted_with_candidates(
            &current, face_a, face_b, order, total_before, &preserved, &full_candidates,
        ) {
            apply_turns(&mut current, &seq)?;
            turns.extend(seq);
            continue;
        }

        // 2. BFS strict improvement at shallow depths (2-3).
        //    Keeping depth low prevents excessive edge scrambling.
        {
            let target_fn = |s: &CubeState| -> usize {
                count_correct_on_face(s, face_a) + count_correct_on_face(s, face_b)
            };
            let mut bfs_found = false;
            for bfs_depth in 2..=3 {
                if let Some(seq) = bfs_improve(
                    &current, &target_fn, total_before,
                    true, &bf_moves, bfs_depth,
                    &preserved, order,
                ) {
                    apply_turns(&mut current, &seq)?;
                    turns.extend(seq);
                    bfs_found = true;
                    break;
                }
            }
            if bfs_found {
                continue;
            }
        }

        // 3. Enabling move (allows ≤1 decrease in total to create room)
        {
            let target = if correct_a <= correct_b { face_a } else { face_b };
            if let Some(seq) = find_enabling_move_with_candidates(
                &current, target, face_a, face_b, order,
                count_correct_on_face(&current, target), total_before, &preserved, &full_candidates,
            ) {
                apply_turns(&mut current, &seq)?;
                turns.extend(seq);
                continue;
            }
        }

        // 4. BFS with break-even acceptance (depth 3-4, allow non-decreasing).
        //    Only triggered when strict BFS and greedy both fail.
        {
            let target_fn = |s: &CubeState| -> usize {
                count_correct_on_face(s, face_a) + count_correct_on_face(s, face_b)
            };
            let mut bfs_found = false;
            for bfs_depth in 3..=4 {
                if let Some(seq) = bfs_improve(
                    &current, &target_fn, total_before,
                    false, &bf_moves, bfs_depth,
                    &preserved, order,
                ) {
                    apply_turns(&mut current, &seq)?;
                    turns.extend(seq);
                    bfs_found = true;
                    break;
                }
            }
            if bfs_found {
                continue;
            }
        }

        // 5. 2-ply search — chain two commutators for improvement.
        {
            let max_correct = center_positions_on_face(face_a, order).len()
                + center_positions_on_face(face_b, order).len();
            if max_correct.saturating_sub(total_before) <= 10 {
                if let Some(seq) = find_2ply_improvement(
                    &current, face_a, face_b, order, total_before, &preserved,
                    &full_candidates, &full_candidates,
                ) {
                    apply_turns(&mut current, &seq)?;
                    turns.extend(seq);
                    continue;
                }
            }
        }

        // 6. IDA* per-face fallback — single-face inner-slice turns.
        {
            let mut found = false;
            for &face in &[face_a, face_b] {
                if !face_centers_solved(&current, face, order) {
                    let ida_candidates = generate_all_ida_candidates(order);
                    if let Some(seq) = ida_star_single_face_with_candidates(
                        &current, face, order, &preserved, &ida_candidates,
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

        // 7. Shake: non-decreasing then ≤1 decrease commutators.
        {
            if let Some(seq) = shake_pair(
                &current, face_a, face_b, order, total_before, &preserved, &full_candidates,
            ) {
                apply_turns(&mut current, &seq)?;
                turns.extend(seq);
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

// ---------------------------------------------------------------------------
// Phase 2: Equator faces with BFS (u/d-only moves)
// ---------------------------------------------------------------------------

fn solve_equator_faces_bfs(
    state: &CubeState,
    order: u32,
) -> Result<(Vec<TurnCommand>, CubeState), ReductionError> {
    let mut current = state.clone();
    let mut turns: Vec<TurnCommand> = Vec::new();

    let equator_seq = [Face::Front, Face::Right, Face::Back, Face::Left];
    let mut locked: Vec<Face> = vec![Face::Up, Face::Down];

    // Pre-generate BFS moves for equator phase
    let bf_moves = generate_equator_bfs_moves(order);

    // Full candidate set for fallbacks
    let full_candidates = generate_equator_candidates(order, &locked);

    for &face in &equator_seq {
        if face_centers_solved(&current, face, order) {
            locked.push(face);
            continue;
        }

        for _iter in 0..MAX_ITERATIONS_PER_PAIR {
            if face_centers_solved(&current, face, order) {
                break;
            }

            let correct_before = count_correct_on_face(&current, face);

            // 1. Greedy best single commutator (u/d-only moves).
            if let Some(seq) = find_best_equator_candidate_from_list(
                &current, face, order, correct_before, &locked, &full_candidates,
            ) {
                apply_turns(&mut current, &seq)?;
                turns.extend(seq);
                continue;
            }

            // 2. BFS strict improvement at shallow depths (2-4).
            {
                let target_fn = |s: &CubeState| -> usize {
                    count_correct_on_face(s, face)
                };
                let mut bfs_found = false;
                for bfs_depth in 2..=4 {
                    if let Some(seq) = bfs_improve(
                        &current, &target_fn, correct_before,
                        true, &bf_moves, bfs_depth,
                        &locked, order,
                    ) {
                        apply_turns(&mut current, &seq)?;
                        turns.extend(seq);
                        bfs_found = true;
                        break;
                    }
                }
                if bfs_found {
                    continue;
                }
            }

            // 3. BFS break-even acceptance (depth 3-5).
            {
                let target_fn = |s: &CubeState| -> usize {
                    count_correct_on_face(s, face)
                };
                let mut bfs_found = false;
                for bfs_depth in 3..=5 {
                    if let Some(seq) = bfs_improve(
                        &current, &target_fn, correct_before,
                        false, &bf_moves, bfs_depth,
                        &locked, order,
                    ) {
                        apply_turns(&mut current, &seq)?;
                        turns.extend(seq);
                        bfs_found = true;
                        break;
                    }
                }
                if bfs_found {
                    continue;
                }
            }

            // 4. Enabling move — allows ≤1 decrease.
            {
                let mut best_enable: Option<(Vec<TurnCommand>, usize)> = None;
                let positions = center_positions_on_face(face, order);
                for candidate in &full_candidates {
                    if candidate.len() < 4 { continue; }
                    if let Ok(sim) = simulate_sequence(&current, candidate) {
                        if !solved_faces_preserved(&sim, &locked, order) { continue; }
                        let total = count_correct_on_face(&sim, face);
                        if total + 1 < correct_before { continue; }
                        let changed = positions.iter().any(|p| {
                            p.color_in(&sim) != p.color_in(&current)
                        });
                        if changed {
                            match &best_enable {
                                None => best_enable = Some((candidate.clone(), total)),
                                Some((_, best_total)) if total > *best_total => {
                                    best_enable = Some((candidate.clone(), total));
                                }
                                _ => {}
                            }
                        }
                    }
                }
                if let Some((seq, _)) = best_enable {
                    apply_turns(&mut current, &seq)?;
                    turns.extend(seq);
                    continue;
                }
            }

            // 5. 2-ply search.
            {
                let max_correct = center_positions_on_face(face, order).len();
                if max_correct.saturating_sub(correct_before) <= 4 {
                    if let Some(seq) = find_2ply_improvement(
                        &current, face, face, order, correct_before,
                        &locked, &full_candidates, &full_candidates,
                    ) {
                        apply_turns(&mut current, &seq)?;
                        turns.extend(seq);
                        continue;
                    }
                }
            }

            // 6. IDA* fallback.
            let wrong = count_wrong_on_face(&current, face);
            if wrong > 0 {
                let ida_candidates = generate_equator_candidates(order, &locked);
                if let Some(seq) = ida_star_single_face_with_candidates(
                    &current, face, order, &locked, &ida_candidates,
                ) {
                    apply_turns(&mut current, &seq)?;
                    turns.extend(seq);
                    continue;
                }
            }

            // 7. Shake: non-decreasing then ≤1 decrease.
            {
                let mut shook = false;
                for candidate in &full_candidates {
                    if candidate.len() < 4 { continue; }
                    if let Ok(sim) = simulate_sequence(&current, candidate) {
                        if !solved_faces_preserved(&sim, &locked, order) { continue; }
                        let total = count_correct_on_face(&sim, face);
                        if total < correct_before { continue; }
                        let changed = center_positions_on_face(face, order)
                            .iter().any(|p| p.color_in(&sim) != p.color_in(&current));
                        if changed {
                            apply_turns(&mut current, candidate)?;
                            turns.extend_from_slice(candidate);
                            shook = true;
                            break;
                        }
                    }
                }
                if !shook {
                    for candidate in &full_candidates {
                        if candidate.len() < 4 { continue; }
                        if let Ok(sim) = simulate_sequence(&current, candidate) {
                            if !solved_faces_preserved(&sim, &locked, order) { continue; }
                            let total = count_correct_on_face(&sim, face);
                            if total < correct_before.saturating_sub(1) { continue; }
                            let changed = center_positions_on_face(face, order)
                                .iter().any(|p| p.color_in(&sim) != p.color_in(&current));
                            if changed {
                                apply_turns(&mut current, candidate)?;
                                turns.extend_from_slice(candidate);
                                shook = true;
                                break;
                            }
                        }
                    }
                }
                if shook {
                    continue;
                }
            }

            break; // stuck
        }

        if !face_centers_solved(&current, face, order) {
            let c = count_correct_on_face(&current, face);
            let t = center_positions_on_face(face, order).len();
            return Err(ReductionError::InvalidState(format!(
                "failed to solve equator face {:?}: {}/{} correct",
                face, c, t
            )));
        }

        locked.push(face);
    }

    Ok((turns, current))
}

// ---------------------------------------------------------------------------
// BFS search engine
// ---------------------------------------------------------------------------

/// BFS that searches for a sequence of pre-generated move-sequences that
/// improves the target score. Returns the FIRST sequence found (not necessarily
/// the best) at the shallowest depth where an improvement exists.
fn bfs_improve(
    state: &CubeState,
    target_fn: &dyn Fn(&CubeState) -> usize,
    score_before: usize,
    strict: bool,
    moves: &[Vec<TurnCommand>],
    max_depth: usize,
    preserved: &[Face],
    order: u32,
) -> Option<Vec<TurnCommand>> {
    if moves.is_empty() {
        return None;
    }

    let mut visited: HashSet<u64> = HashSet::with_capacity(50_000);
    let mut queue: VecDeque<(CubeState, Vec<Vec<TurnCommand>>)> = VecDeque::with_capacity(10_000);

    let initial_hash = hash_centers(state);
    visited.insert(initial_hash);
    queue.push_back((state.clone(), Vec::new()));

    let mut depth = 0;
    while depth <= max_depth && !queue.is_empty() {
        let level_size = queue.len();
        for _ in 0..level_size {
            let (s, path) = queue.pop_front().unwrap();

            for mv in moves {
                if mv.is_empty() {
                    continue;
                }

                let sim = match simulate_sequence(&s, mv) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                if !solved_faces_preserved(&sim, preserved, order) {
                    continue;
                }

                let h = hash_centers(&sim);
                if !visited.insert(h) {
                    continue;
                }

                if visited.len() > BFS_VISITED_CAP {
                    return None;
                }

                let score = target_fn(&sim);

                if strict && score > score_before {
                    return Some(flatten_path(&path, mv));
                }

                if !strict && score >= score_before && h != initial_hash {
                    return Some(flatten_path(&path, mv));
                }

                if depth < max_depth {
                    let mut new_path = path.clone();
                    new_path.push(mv.clone());
                    queue.push_back((sim, new_path));
                }
            }
        }
        depth += 1;
    }

    None
}

/// Flatten a path of move-sequences plus a final move-sequence into a single
/// Vec<TurnCommand>.
fn flatten_path(path: &[Vec<TurnCommand>], last: &[TurnCommand]) -> Vec<TurnCommand> {
    let mut result = Vec::new();
    for seq in path {
        result.extend_from_slice(seq);
    }
    result.extend_from_slice(last);
    result
}

// ---------------------------------------------------------------------------
// Center state hashing (only center stickers)
// ---------------------------------------------------------------------------

/// Hash only the center positions of a cube state.
/// This dramatically reduces the visited-set size during BFS.
fn hash_centers(state: &CubeState) -> u64 {
    let order = state.order.get() as usize;
    let last = order - 1;
    let mid = if order % 2 == 1 { Some(order / 2) } else { None };

    // Use FNV-1a for speed (not crypto, just visited-set dedup)
    let mut hash: u64 = 0xcbf29ce484222325;

    for face_idx in 0..6usize {
        let offset = face_idx * order * order;
        for row in 1..last {
            for col in 1..last {
                if let Some(m) = mid {
                    if row == m && col == m {
                        continue; // skip true center (fixed)
                    }
                }
                let idx = offset + row * order + col;
                // Hash the sticker color as a byte
                let byte = state.stickers[idx] as u8;
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
    }

    hash
}

// ---------------------------------------------------------------------------
// BFS move generation
// ---------------------------------------------------------------------------

/// Generate BFS moves for the U/D pair phase.
/// Focused set including CW, CCW, and HalfTurn variants.
fn generate_pair_bfs_moves(order: u32) -> Vec<Vec<TurnCommand>> {
    let mut moves = Vec::new();
    let max_depth = order / 2;
    let perp = [Face::Right, Face::Left, Face::Front, Face::Back];

    // Single inner-slice turns
    for &face in &perp {
        for depth in 1..=max_depth {
            for &rot in &[RotationAmount::Clockwise, RotationAmount::CounterClockwise, RotationAmount::HalfTurn] {
                moves.push(vec![TurnCommand {
                    face, start_layer: depth, width: 1, rotation: rot,
                }]);
            }
        }
    }

    // Commutators [slice, face, slice_inv, face_inv]
    for &sf in &perp {
        for d in 1..=max_depth {
            let s_cw = TurnCommand { face: sf, start_layer: d, width: 1, rotation: RotationAmount::Clockwise };
            let s_ccw = s_cw.inverse();
            let s_h2 = TurnCommand { face: sf, start_layer: d, width: 1, rotation: RotationAmount::HalfTurn };

            for &tf in &[Face::Up, Face::Down] {
                for &rot in &[RotationAmount::Clockwise, RotationAmount::CounterClockwise, RotationAmount::HalfTurn] {
                    let f_turn = TurnCommand { face: tf, start_layer: 0, width: 1, rotation: rot };
                    let f_inv = f_turn.inverse();

                    moves.push(vec![s_cw, f_turn, s_ccw, f_inv]);
                    moves.push(vec![f_turn, s_cw, f_inv, s_ccw]);
                    moves.push(vec![s_h2, f_turn, s_h2, f_inv]);
                }
            }
        }
    }

    moves
}

/// Generate BFS moves for the equator phase.
fn generate_equator_bfs_moves(order: u32) -> Vec<Vec<TurnCommand>> {
    let mut moves = Vec::new();
    let max_depth = order / 2;
    let eq = [Face::Front, Face::Back, Face::Right, Face::Left];

    // Single inner-slice turns on U and D
    for &sf in &[Face::Up, Face::Down] {
        for depth in 1..=max_depth {
            for &rot in &[RotationAmount::Clockwise, RotationAmount::CounterClockwise, RotationAmount::HalfTurn] {
                moves.push(vec![TurnCommand {
                    face: sf, start_layer: depth, width: 1, rotation: rot,
                }]);
            }
        }
    }

    // Commutators: u/d inner slice + equator face turn
    for &sf in &[Face::Up, Face::Down] {
        for d in 1..=max_depth {
            let s_cw = TurnCommand { face: sf, start_layer: d, width: 1, rotation: RotationAmount::Clockwise };
            let s_ccw = s_cw.inverse();
            let s_h2 = TurnCommand { face: sf, start_layer: d, width: 1, rotation: RotationAmount::HalfTurn };

            for &ef in &eq {
                for &rot in &[RotationAmount::Clockwise, RotationAmount::CounterClockwise, RotationAmount::HalfTurn] {
                    let f_turn = TurnCommand { face: ef, start_layer: 0, width: 1, rotation: rot };
                    let f_inv = f_turn.inverse();

                    moves.push(vec![s_cw, f_turn, s_ccw, f_inv]);
                    moves.push(vec![f_turn, s_cw, f_inv, s_ccw]);
                    moves.push(vec![s_h2, f_turn, s_h2, f_inv]);
                }
            }
        }
    }

    // Two-slice commutators u + d (different depths)
    for d1 in 1..=max_depth {
        for d2 in 1..=max_depth {
            if d1 == d2 { continue; }
            let u_cw = TurnCommand { face: Face::Up, start_layer: d1, width: 1, rotation: RotationAmount::Clockwise };
            let u_ccw = u_cw.inverse();
            let d_cw = TurnCommand { face: Face::Down, start_layer: d2, width: 1, rotation: RotationAmount::Clockwise };
            let d_ccw = d_cw.inverse();
            moves.push(vec![u_cw, d_cw, u_ccw, d_ccw]);
        }
    }

    moves
}

// ---------------------------------------------------------------------------
// Candidate generation (for greedy fallback when BFS gets stuck)
// ---------------------------------------------------------------------------

/// Generate candidates for the unrestricted pair phase.
fn generate_pair_candidates(order: u32) -> Vec<Vec<TurnCommand>> {
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

    // Commutators [slice, face_turn]
    for depth in 1..=max_depth {
        for &slice_face in &Face::ALL {
            let slice_cw = TurnCommand {
                face: slice_face, start_layer: depth, width: 1,
                rotation: RotationAmount::Clockwise,
            };
            let slice_ccw = slice_cw.inverse();

            for &face in &Face::ALL {
                for &rot in &[RotationAmount::Clockwise, RotationAmount::CounterClockwise, RotationAmount::HalfTurn] {
                    let face_turn = TurnCommand {
                        face, start_layer: 0, width: 1, rotation: rot,
                    };
                    let face_inv = face_turn.inverse();
                    candidates.push(vec![slice_cw, face_turn, slice_ccw, face_inv]);
                    candidates.push(vec![face_turn, slice_cw, face_inv, slice_ccw]);
                    candidates.push(vec![slice_cw, face_turn, slice_ccw]);
                }
            }
        }
    }

    // Two-slice commutators
    for d1 in 1..=max_depth {
        for d2 in 1..=max_depth {
            for &f1 in &[Face::Up, Face::Right, Face::Front] {
                for &f2 in &[Face::Up, Face::Right, Face::Front] {
                    if f1 as u8 >= f2 as u8 {
                        continue;
                    }
                    let s1_cw = TurnCommand {
                        face: f1, start_layer: d1, width: 1,
                        rotation: RotationAmount::Clockwise,
                    };
                    let s1_ccw = s1_cw.inverse();
                    let s2_cw = TurnCommand {
                        face: f2, start_layer: d2, width: 1,
                        rotation: RotationAmount::Clockwise,
                    };
                    let s2_ccw = s2_cw.inverse();

                    candidates.push(vec![s1_cw, s2_cw, s1_ccw, s2_ccw]);
                    candidates.push(vec![s1_cw, s2_ccw, s1_ccw, s2_cw]);
                }
            }
        }
    }

    candidates
}

/// Generate candidates using only u/d inner slices (U and D face inner
/// layers) which affect the equator faces (F, R, B, L) but not U or D.
fn generate_equator_candidates(
    order: u32,
    locked: &[Face],
) -> Vec<Vec<TurnCommand>> {
    let mut candidates = Vec::new();
    let max_depth = order / 2;

    for &slice_face in &[Face::Up, Face::Down] {
        for depth in 1..=max_depth {
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
                candidates.push(vec![slice_cw, face_turn, slice_ccw, face_inv]);
                candidates.push(vec![face_turn, slice_cw, face_inv, slice_ccw]);
                candidates.push(vec![slice_cw, face_turn, slice_ccw]);
            }
        }
    }

    // Two-slice commutators using only u/d slices
    for d1 in 1..=max_depth {
        for d2 in 1..=max_depth {
            let u_cw = TurnCommand { face: Face::Up, start_layer: d1, width: 1, rotation: RotationAmount::Clockwise };
            let u_ccw = u_cw.inverse();
            let d_cw = TurnCommand { face: Face::Down, start_layer: d2, width: 1, rotation: RotationAmount::Clockwise };
            let d_ccw = d_cw.inverse();

            candidates.push(vec![u_cw, d_cw, u_ccw, d_ccw]);
            candidates.push(vec![u_cw, d_ccw, u_ccw, d_cw]);
        }
    }

    candidates
}

// ---------------------------------------------------------------------------
// Greedy single-candidate evaluation
// ---------------------------------------------------------------------------

fn find_best_candidate_unrestricted_with_candidates(
    state: &CubeState,
    face_a: Face,
    face_b: Face,
    order: u32,
    total_before: usize,
    preserved: &[Face],
    candidates: &[Vec<TurnCommand>],
) -> Option<Vec<TurnCommand>> {
    let mut best: Option<(Vec<TurnCommand>, usize)> = None;

    for candidate in candidates {
        if let Ok(sim) = simulate_sequence(state, candidate) {
            if !solved_faces_preserved(&sim, preserved, order) {
                continue;
            }
            let total = count_correct_on_face(&sim, face_a)
                + count_correct_on_face(&sim, face_b);
            if total > total_before {
                match &best {
                    Some((_, best_total)) if total > *best_total => {
                        best = Some((candidate.clone(), total));
                    }
                    None => {
                        best = Some((candidate.clone(), total));
                    }
                    _ => {}
                }
            }
        }
    }

    best.map(|(turns, _)| turns)
}

fn find_best_equator_candidate_from_list(
    state: &CubeState,
    target_face: Face,
    order: u32,
    correct_before: usize,
    locked: &[Face],
    candidates: &[Vec<TurnCommand>],
) -> Option<Vec<TurnCommand>> {
    let mut best: Option<(Vec<TurnCommand>, usize)> = None;

    for candidate in candidates {
        if let Ok(sim) = simulate_sequence(state, candidate) {
            if !solved_faces_preserved(&sim, locked, order) {
                continue;
            }
            let correct_after = count_correct_on_face(&sim, target_face);
            if correct_after > correct_before {
                match &best {
                    Some((_, best_c)) if correct_after > *best_c => {
                        best = Some((candidate.clone(), correct_after));
                    }
                    None => {
                        best = Some((candidate.clone(), correct_after));
                    }
                    _ => {}
                }
            }
        }
    }

    best.map(|(turns, _)| turns)
}

// ---------------------------------------------------------------------------
// 2-ply search
// ---------------------------------------------------------------------------

fn find_2ply_improvement(
    state: &CubeState,
    face_a: Face,
    face_b: Face,
    order: u32,
    total_before: usize,
    preserved: &[Face],
    c1_candidates: &[Vec<TurnCommand>],
    c2_candidates: &[Vec<TurnCommand>],
) -> Option<Vec<TurnCommand>> {
    let c1_indices: Vec<usize> = c1_candidates.iter()
        .enumerate()
        .filter(|(_, c)| {
            if let Ok(sim) = simulate_sequence(state, c) {
                center_positions_on_face(face_a, order)
                    .iter().any(|p| p.color_in(&sim) != p.color_in(state))
                || center_positions_on_face(face_b, order)
                    .iter().any(|p| p.color_in(&sim) != p.color_in(state))
            } else { false }
        })
        .take(300)
        .map(|(i, _)| i)
        .collect();

    for &i in &c1_indices {
        let first = &c1_candidates[i];
        let sim1 = match simulate_sequence(state, first) {
            Ok(s) => s, Err(_) => continue,
        };
        let count_after_c1 = count_correct_on_face(&sim1, face_a)
            + count_correct_on_face(&sim1, face_b);
        if count_after_c1 < total_before.saturating_sub(6) {
            continue;
        }
        for second in c2_candidates {
            let sim2 = match simulate_sequence(&sim1, second) {
                Ok(s) => s, Err(_) => continue,
            };
            if !solved_faces_preserved(&sim2, preserved, order) {
                continue;
            }
            let total = count_correct_on_face(&sim2, face_a)
                + count_correct_on_face(&sim2, face_b);
            if total > total_before {
                let mut combined = first.clone();
                combined.extend_from_slice(second);
                return Some(combined);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Shake (make progress when stuck)
// ---------------------------------------------------------------------------

fn shake_pair(
    state: &CubeState,
    face_a: Face,
    face_b: Face,
    order: u32,
    total_before: usize,
    preserved: &[Face],
    candidates: &[Vec<TurnCommand>],
) -> Option<Vec<TurnCommand>> {
    // First pass: non-decreasing
    for candidate in candidates {
        if candidate.len() < 4 { continue; }
        if let Ok(sim) = simulate_sequence(state, candidate) {
            if !solved_faces_preserved(&sim, preserved, order) { continue; }
            let total = count_correct_on_face(&sim, face_a)
                + count_correct_on_face(&sim, face_b);
            if total < total_before { continue; }
            let changed = center_positions_on_face(face_a, order)
                .iter().any(|p| p.color_in(&sim) != p.color_in(state))
                || center_positions_on_face(face_b, order)
                    .iter().any(|p| p.color_in(&sim) != p.color_in(state));
            if changed {
                return Some(candidate.clone());
            }
        }
    }
    // Second pass: allow ≤1 decrease
    for candidate in candidates {
        if candidate.len() < 4 { continue; }
        if let Ok(sim) = simulate_sequence(state, candidate) {
            if !solved_faces_preserved(&sim, preserved, order) { continue; }
            let total = count_correct_on_face(&sim, face_a)
                + count_correct_on_face(&sim, face_b);
            if total < total_before.saturating_sub(1) { continue; }
            let changed = center_positions_on_face(face_a, order)
                .iter().any(|p| p.color_in(&sim) != p.color_in(state))
                || center_positions_on_face(face_b, order)
                    .iter().any(|p| p.color_in(&sim) != p.color_in(state));
            if changed {
                return Some(candidate.clone());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Enabling move
// ---------------------------------------------------------------------------

fn find_enabling_move_with_candidates(
    state: &CubeState,
    target_face: Face,
    face_a: Face,
    face_b: Face,
    order: u32,
    _correct_before: usize,
    total_before: usize,
    preserved: &[Face],
    candidates: &[Vec<TurnCommand>],
) -> Option<Vec<TurnCommand>> {
    let positions = center_positions_on_face(target_face, order);
    let mut best: Option<(Vec<TurnCommand>, usize)> = None;

    for candidate in candidates {
        if candidate.len() < 4 {
            continue;
        }
        if let Ok(sim) = simulate_sequence(state, candidate) {
            if !solved_faces_preserved(&sim, preserved, order) {
                continue;
            }
            let total = count_correct_on_face(&sim, face_a)
                + count_correct_on_face(&sim, face_b);
            if total + 1 < total_before {
                continue;
            }
            let changed = positions.iter().any(|p| {
                p.color_in(&sim) != p.color_in(state)
            });
            if changed {
                match &best {
                    None => best = Some((candidate.clone(), total)),
                    Some((_, best_total)) if total > *best_total => {
                        best = Some((candidate.clone(), total));
                    }
                    _ => {}
                }
            }
        }
    }
    best.map(|(seq, _)| seq)
}

// ---------------------------------------------------------------------------
// IDA* fallback
// ---------------------------------------------------------------------------

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

    for max_depth in 2..=IDA_MAX_DEPTH {
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
            let can_recurse = if wrong_after < wrong_before {
                true
            } else if wrong_after == wrong_before && wrong_before <= 2 && depth + 1 < max_depth {
                true
            } else {
                false
            };
            if can_recurse {
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
// Shared helpers
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

// ---------------------------------------------------------------------------
// Turn simplification (reduces sequence length to help the edge solver)
// ---------------------------------------------------------------------------

/// Simplify a turn sequence by canceling adjacent inverses and merging
/// same-slice adjacent turns. Canonicalises all moves to the positive face
/// (R/U/F) so that cross-face cancellations work.
fn simplify_center_turns(turns: &[TurnCommand], order: u32) -> Vec<TurnCommand> {
    if turns.len() <= 1 {
        return turns.to_vec();
    }

    let r1 = order - 1;

    // Convert to internal representation
    let mut moves: Vec<(u8, u32, i32)> = turns.iter().map(|t| {
        let q: i32 = match t.rotation {
            RotationAmount::Clockwise => 1,
            RotationAmount::HalfTurn => 2,
            RotationAmount::CounterClockwise => -1,
        };
        match t.face {
            Face::Right => (0u8, t.start_layer, q),
            Face::Left => (0u8, r1 - t.start_layer, -q),
            Face::Up => (1u8, t.start_layer, q),
            Face::Down => (1u8, r1 - t.start_layer, -q),
            Face::Front => (2u8, t.start_layer, q),
            Face::Back => (2u8, r1 - t.start_layer, -q),
        }
    }).collect();

    // Run merge pass to fixed point
    loop {
        let len_before = moves.len();
        let mut i = 0;
        while i + 1 < moves.len() {
            let (a_axis, a_depth, a_amt) = moves[i];
            let (b_axis, b_depth, b_amt) = moves[i + 1];
            if a_axis == b_axis && a_depth == b_depth {
                let sum = ((a_amt + b_amt) % 4 + 4) % 4;
                if sum == 0 {
                    moves.remove(i);
                    moves.remove(i); // i+1 shifts to i
                } else {
                    let norm = match sum { 1 => 1, 2 => 2, 3 => -1, _ => 0 };
                    moves[i] = (a_axis, a_depth, norm);
                    moves.remove(i + 1);
                }
            } else {
                i += 1;
            }
        }
        if moves.len() == len_before {
            break;
        }
    }

    // Convert back to TurnCommands
    moves.into_iter().filter_map(|(axis, depth, amt)| {
        let rotation = match amt {
            1 => RotationAmount::Clockwise,
            -1 => RotationAmount::CounterClockwise,
            2 | -2 => RotationAmount::HalfTurn,
            _ => return None,
        };
        let face = match axis {
            0 => Face::Right,
            1 => Face::Up,
            2 => Face::Front,
            _ => return None,
        };
        Some(TurnCommand { face, start_layer: depth, width: 1, rotation })
    }).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    #[test]
    fn test_hash_different_states() {
        let s1 = CubeState::solved(CubeOrder::new(4).unwrap());
        let mut s2 = s1.clone();
        apply_turn_to_state(&mut s2, TurnCommand {
            face: Face::Right, start_layer: 1, width: 1,
            rotation: RotationAmount::Clockwise,
        }).unwrap();

        assert_ne!(hash_centers(&s1), hash_centers(&s2),
            "different center states should hash differently");
    }

    #[test]
    fn test_hash_same_state() {
        let s1 = CubeState::solved(CubeOrder::new(4).unwrap());
        let s2 = CubeState::solved(CubeOrder::new(4).unwrap());
        assert_eq!(hash_centers(&s1), hash_centers(&s2),
            "identical center states should hash the same");
    }
}

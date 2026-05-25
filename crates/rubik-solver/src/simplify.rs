use rubik_core::{Face, RotationAmount, TurnCommand};

/// Internal representation: a move reduced to (axis, canonical_depth, signed_amount).
///
/// Three axes:
///   0 = X (R–L), canonical positive direction = R CW.
///   1 = Y (U–D), canonical positive direction = U CW.
///   2 = Z (F–B), canonical positive direction = F CW.
///
/// `norm_depth` is the layer index measured from the canonical positive face
/// (R / U / F).  A move on the opposite face at depth `d` maps to
/// `norm_depth = r1 - d` with the sign flipped.
///
/// `amount` is the rotation in canonical coordinates:
///   +1 → one CW quarter-turn about +axis
///   -1 → one CCW quarter-turn about +axis
///   ±2 → half-turn (sign irrelevant for output, used for merging arithmetic)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AxisMove {
    axis: u8,
    norm_depth: u32,
    amount: i32,
}

/// Normalise `amount` into {-1, 1, 2} (0 means identity / removed).
fn normalise_amount(a: i32) -> i32 {
    match a.rem_euclid(4) {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => -1,
        _ => unreachable!(),
    }
}

/// Merge two moves on the **same** (axis, norm_depth).
/// Returns `(new_amount, should_remove)`.
/// `should_remove` is true when the net effect is identity.
fn merge_amounts(a: i32, b: i32) -> (i32, bool) {
    let sum = normalise_amount(a + b);
    (sum, sum == 0)
}

// ---------------------------------------------------------------------------
// TurnCommand <-> AxisMove conversion
// ---------------------------------------------------------------------------

fn turn_to_axis(turn: &TurnCommand, r1: u32) -> AxisMove {
    let q: i32 = match turn.rotation {
        RotationAmount::Clockwise => 1,
        RotationAmount::HalfTurn => 2,
        RotationAmount::CounterClockwise => -1,
    };

    let (axis, norm_depth, amount) = match turn.face {
        // X axis — R is the canonical positive face
        Face::Right => (0, turn.start_layer, q),
        Face::Left => (0, r1 - turn.start_layer, -q),
        // Y axis — U is the canonical positive face
        Face::Up => (1, turn.start_layer, q),
        Face::Down => (1, r1 - turn.start_layer, -q),
        // Z axis — F is the canonical positive face
        Face::Front => (2, turn.start_layer, q),
        Face::Back => (2, r1 - turn.start_layer, -q),
    };

    AxisMove {
        axis,
        norm_depth,
        amount,
    }
}

fn axis_to_turn(am: AxisMove) -> TurnCommand {
    // Normalise amount before emitting
    let amount = normalise_amount(am.amount);
    debug_assert!(amount != 0, "zero amount should have been removed");

    // amount ==  1: emit from the positive face (R/U/F) at norm_depth with CW.
    // amount == -1: emit from the positive face (R/U/F) at norm_depth with CCW.
    // amount == ±2: emit from the positive face (R/U/F) at norm_depth with HalfTurn.
    let face = match am.axis {
        0 => Face::Right,
        1 => Face::Up,
        2 => Face::Front,
        _ => unreachable!(),
    };
    let rotation = match amount {
        1 => RotationAmount::Clockwise,
        -1 => RotationAmount::CounterClockwise,
        2 | -2 => RotationAmount::HalfTurn,
        _ => unreachable!(),
    };
    TurnCommand {
        face,
        start_layer: am.norm_depth,
        width: 1,
        rotation,
    }
}

// ---------------------------------------------------------------------------
// Simplification passes
// ---------------------------------------------------------------------------

/// One pass: merge / cancel **adjacent** entries that share the same
/// (axis, norm_depth).  Returns the number of merges performed.
fn merge_adjacent_pass(moves: &mut Vec<AxisMove>) -> usize {
    let mut merges = 0;
    let mut i = 0;
    while i + 1 < moves.len() {
        let a = moves[i];
        let b = moves[i + 1];
        if a.axis == b.axis && a.norm_depth == b.norm_depth {
            let (new_amount, remove) = merge_amounts(a.amount, b.amount);
            if remove {
                moves.remove(i);
                moves.remove(i); // i+1 shifted to i after first removal
            } else {
                moves[i] = AxisMove {
                    amount: new_amount,
                    ..a
                };
                moves.remove(i + 1);
            }
            merges += 1;
            // Don't advance i — the new neighbour (if any) might also merge.
        } else {
            i += 1;
        }
    }
    merges
}

/// Simplify a sequence of `TurnCommand`s by repeatedly merging / cancelling
/// adjacent moves that operate on the same physical slice (same axis *and*
/// same depth), including cross-face equivalents (e.g. `R d` = `L r1-d`).
///
/// The pass is run to a fixed point so cascading cancellations are caught.
///
/// `order` is the cube order (N).
pub fn simplify_turns(turns: &[TurnCommand], order: u32) -> Vec<TurnCommand> {
    if turns.is_empty() {
        return Vec::new();
    }

    let r1 = order - 1;

    // Convert to axis representation
    let mut moves: Vec<AxisMove> = turns.iter().map(|t| turn_to_axis(t, r1)).collect();

    // Run adjacent-merge passes to fixed point
    loop {
        let merges = merge_adjacent_pass(&mut moves);
        if merges == 0 {
            break;
        }
    }

    // Convert back to TurnCommands
    moves.into_iter().map(axis_to_turn).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::RotationAmount;

    fn cw(face: Face, depth: u32) -> TurnCommand {
        TurnCommand {
            face,
            start_layer: depth,
            width: 1,
            rotation: RotationAmount::Clockwise,
        }
    }

    fn ccw(face: Face, depth: u32) -> TurnCommand {
        TurnCommand {
            face,
            start_layer: depth,
            width: 1,
            rotation: RotationAmount::CounterClockwise,
        }
    }

    fn h2(face: Face, depth: u32) -> TurnCommand {
        TurnCommand {
            face,
            start_layer: depth,
            width: 1,
            rotation: RotationAmount::HalfTurn,
        }
    }

    #[test]
    fn empty_returns_empty() {
        assert_eq!(simplify_turns(&[], 4), vec![]);
    }

    #[test]
    fn single_turn_unchanged() {
        let turns = vec![cw(Face::Right, 0)];
        assert_eq!(simplify_turns(&turns, 4), turns);
    }

    #[test]
    fn adjacent_inverse_cancels() {
        // R R' = nothing
        let turns = vec![cw(Face::Right, 0), ccw(Face::Right, 0)];
        assert_eq!(simplify_turns(&turns, 4), vec![]);
    }

    #[test]
    fn adjacent_cw_cw_merges_to_half() {
        // R R = R2
        let turns = vec![cw(Face::Right, 0), cw(Face::Right, 0)];
        assert_eq!(simplify_turns(&turns, 4), vec![h2(Face::Right, 0)]);
    }

    #[test]
    fn adjacent_ccw_ccw_merges_to_half() {
        // R' R' = R2
        let turns = vec![ccw(Face::Right, 0), ccw(Face::Right, 0)];
        assert_eq!(simplify_turns(&turns, 4), vec![h2(Face::Right, 0)]);
    }

    #[test]
    fn cw_half_merges_to_ccw() {
        // R R2 = R'
        let turns = vec![cw(Face::Right, 0), h2(Face::Right, 0)];
        assert_eq!(simplify_turns(&turns, 4), vec![ccw(Face::Right, 0)]);
    }

    #[test]
    fn half_cw_merges_to_ccw() {
        // R2 R = R'
        let turns = vec![h2(Face::Right, 0), cw(Face::Right, 0)];
        assert_eq!(simplify_turns(&turns, 4), vec![ccw(Face::Right, 0)]);
    }

    #[test]
    fn two_half_turns_cancel() {
        // R2 R2 = nothing
        let turns = vec![h2(Face::Right, 0), h2(Face::Right, 0)];
        assert_eq!(simplify_turns(&turns, 4), vec![]);
    }

    #[test]
    fn cross_face_same_slice_cancels() {
        // On a 4x4 (r1=3):
        // R at depth 3 = leftmost layer from R side = same slice as L at depth 0
        // R CW at depth 3 and L CW at depth 0 should cancel.
        let turns = vec![cw(Face::Right, 3), cw(Face::Left, 0)];
        assert_eq!(simplify_turns(&turns, 4), vec![]);
    }

    #[test]
    fn cross_face_ccw_cw_same_slice_merges() {
        // R CCW at depth 3 and L CW at depth 0 on 4x4: both are +axis rotations.
        // CCW on R = signed -1, CW on L = signed -(-1) = +1? No wait.
        // R CCW at d=3: norm_depth 3, signed -1. L CW at d=0: norm_depth r1-0=3, signed -1.
        // Both have signed -1 → merge to -2 → HalfTurn.
        let turns = vec![ccw(Face::Right, 3), cw(Face::Left, 0)];
        assert_eq!(simplify_turns(&turns, 4), vec![h2(Face::Right, 3)]);
    }

    #[test]
    fn different_axes_do_not_merge() {
        let turns = vec![cw(Face::Right, 0), cw(Face::Up, 0)];
        assert_eq!(simplify_turns(&turns, 4), turns);
    }

    #[test]
    fn same_axis_different_depth_do_not_merge() {
        let turns = vec![cw(Face::Right, 0), cw(Face::Right, 1)];
        assert_eq!(simplify_turns(&turns, 4), turns);
    }

    #[test]
    fn cascading_cancellation() {
        // R U U' R' → R (nothing between) R' → nothing
        let turns = vec![
            cw(Face::Right, 0),
            cw(Face::Up, 0),
            ccw(Face::Up, 0),
            ccw(Face::Right, 0),
        ];
        assert_eq!(simplify_turns(&turns, 4), vec![]);
    }

    #[test]
    fn cascading_merge() {
        // R U U R → R U2 R (adjacent U U merge to U2)
        let turns = vec![
            cw(Face::Right, 0),
            cw(Face::Up, 0),
            cw(Face::Up, 0),
            cw(Face::Right, 0),
        ];
        assert_eq!(
            simplify_turns(&turns, 4),
            vec![cw(Face::Right, 0), h2(Face::Up, 0), cw(Face::Right, 0)]
        );
    }

    #[test]
    fn full_commutator_merges_adjacent() {
        // R U R' U': first pass merges nothing (different axes adjacent).
        // But the commutator structure means R and R' are separated by U.
        // Our adjacent-only pass leaves this unchanged.
        let turns = vec![
            cw(Face::Right, 0),
            cw(Face::Up, 0),
            ccw(Face::Right, 0),
            ccw(Face::Up, 0),
        ];
        assert_eq!(simplify_turns(&turns, 4), turns);
    }

    #[test]
    fn up_down_cross_face_cancel() {
        // On 4x4: U at depth 3 and D at depth 0 are the same slice.
        // U CW and D CW should cancel.
        let turns = vec![cw(Face::Up, 3), cw(Face::Down, 0)];
        assert_eq!(simplify_turns(&turns, 4), vec![]);
    }

    #[test]
    fn front_back_cross_face_cancel() {
        // On 4x4: F at depth 3 and B at depth 0 are the same slice.
        let turns = vec![cw(Face::Front, 3), cw(Face::Back, 0)];
        assert_eq!(simplify_turns(&turns, 4), vec![]);
    }

    #[test]
    fn cross_face_half_turn_merge() {
        // R HalfTurn at depth 3 and L HalfTurn at depth 0 on 4x4.
        // Both are the same physical slice. Two half turns → cancel.
        let turns = vec![h2(Face::Right, 3), h2(Face::Left, 0)];
        assert_eq!(simplify_turns(&turns, 4), vec![]);
    }

    #[test]
    fn measure_simplification_effect() {
        use rubik_core::{CubeOrder, CubeState, apply_turn_to_state};
        let order = 4u32;

        // Build a scrambled state via the solve-and-scramble approach:
        // apply some turns to a solved state, then measure solve lengths.
        let state = CubeState::solved(CubeOrder::new(order).expect("valid"));
        let mut scrambled = state.clone();
        let scramble_turns = vec![
            rubik_core::TurnCommand::outer(
                rubik_core::Face::Right,
                rubik_core::RotationAmount::Clockwise,
            ),
            rubik_core::TurnCommand::outer(
                rubik_core::Face::Up,
                rubik_core::RotationAmount::Clockwise,
            ),
            rubik_core::TurnCommand::outer(
                rubik_core::Face::Front,
                rubik_core::RotationAmount::Clockwise,
            ),
            rubik_core::TurnCommand::outer(
                rubik_core::Face::Right,
                rubik_core::RotationAmount::CounterClockwise,
            ),
        ];
        for &t in &scramble_turns {
            apply_turn_to_state(&mut scrambled, t).expect("valid");
        }

        // Solve raw (rcube-rs directly, no simplification)
        let raw = rcube_rs::solve(&scrambled, None).expect("raw solve");
        // Solve via rubik_solver::solve (includes simplification)
        let simplified = crate::solve(&scrambled).expect("simplified solve");

        println!(
            "4x4 4-turn scramble: raw={}  simplified={}  reduction={:.0}%",
            raw.len(),
            simplified.len(),
            (1.0 - simplified.len() as f64 / raw.len() as f64) * 100.0
        );
        assert!(simplified.len() <= raw.len());
    }

    #[test]
    fn cw_ccw_half_sequence() {
        // R R' R2 R' R = (R R') R2 (R' R) = R2
        // Trace: CW+CCW cancel, then H2+CCW→CW, then CW+CW→H2.
        let turns = vec![
            cw(Face::Right, 0),
            ccw(Face::Right, 0),
            h2(Face::Right, 0),
            ccw(Face::Right, 0),
            cw(Face::Right, 0),
        ];
        assert_eq!(simplify_turns(&turns, 4), vec![h2(Face::Right, 0)]);
    }
}

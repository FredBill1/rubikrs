// edge_solver — pairs edges for N×N cubes (N ≥ 4) using reduction method.
//
// Approach: d-slice edge pairing (the standard reduction method).
//
// Algorithm:
// 1. Use the inner d-slice (layer between U and D, adjacent to D) as the
//    working slice to bring matching wing pieces together.
// 2. Store paired edges on the U and D layers.
// 3. For the last few edges, use special algorithms to handle remaining cases.
//
// For N×N cubes:
//   - Pair outer wings first, then inner wings (for N > 4)
//   - Work slice by slice from the outside in
//
// Key algorithms:
//   - Standard pair: d R U R' d'  (pairs UF edge using d slice)
//   - Last two edges:  slice-flip-slice commutators

use rubik_core::{apply_turn_to_state, CubeState, Face, RotationAmount, TurnCommand};

use super::ReductionError;
use crate::edge_types;

/// Pair all edges. Returns the sequence of turns needed.
pub fn pair_edges(state: &CubeState) -> Result<Vec<TurnCommand>, ReductionError> {
    let order = state.order.get();

    if order < 4 {
        return Err(ReductionError::InvalidOrder(format!(
            "edge pairing requires order >= 4, got {}",
            order
        )));
    }

    // Check if edges are already paired (e.g., solved cube)
    if edge_types::count_paired_edge_groups(state) >= 12 {
        return Ok(Vec::new());
    }

    let mut current = state.clone();
    let mut all_turns: Vec<TurnCommand> = Vec::new();

    // Number of wing layers to pair (for 4×4: 1, for 6×6: 2, etc.)
    let num_wing_layers = (order / 2).saturating_sub(1);

    for wing_layer in 0..num_wing_layers {
        pair_wing_layer(&mut current, &mut all_turns, wing_layer, order)?;
    }

    Ok(all_turns)
}

/// Tracker that bundles a mutable cube state and an accumulated turn list.
struct StateTracker<'a> {
    state: &'a mut CubeState,
    turns: &'a mut Vec<TurnCommand>,
}

impl<'a> StateTracker<'a> {
    fn apply(&mut self, cmds: &[TurnCommand]) -> Result<(), ReductionError> {
        for &t in cmds {
            apply_turn_to_state(self.state, t).map_err(|e| {
                ReductionError::InvalidState(format!("turn failed: {e}"))
            })?;
            self.turns.push(t);
        }
        Ok(())
    }
}

/// Build an inner d-slice turn command at the given wing layer.
fn d_slice_turn(wing_layer: u32, rotation: RotationAmount) -> TurnCommand {
    TurnCommand {
        face: Face::Down,
        start_layer: 1 + wing_layer,
        width: 1,
        rotation,
    }
}

/// Pair all edge wings at a specific layer using a greedy simulation approach.
///
/// Tries all combinations of U-layer orientation × pairing sequence,
/// picks whichever increases the paired-edge count, and applies it.
fn pair_wing_layer(
    state: &mut CubeState,
    turns: &mut Vec<TurnCommand>,
    wing_layer: u32,
    _order: u32,
) -> Result<(), ReductionError> {
    let mut tracker = StateTracker { state, turns };

    let r = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
    let rp = TurnCommand::outer(Face::Right, RotationAmount::CounterClockwise);
    let l = TurnCommand::outer(Face::Left, RotationAmount::Clockwise);
    let lp = TurnCommand::outer(Face::Left, RotationAmount::CounterClockwise);
    let u = TurnCommand::outer(Face::Up, RotationAmount::Clockwise);
    let up = TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise);
    let u2 = TurnCommand::outer(Face::Up, RotationAmount::HalfTurn);
    let ds = d_slice_turn(wing_layer, RotationAmount::Clockwise);
    let dsp = d_slice_turn(wing_layer, RotationAmount::CounterClockwise);
    let ds2 = d_slice_turn(wing_layer, RotationAmount::HalfTurn);
    let d_outer = TurnCommand::outer(Face::Down, RotationAmount::Clockwise);

    // Pairing attempt candidates: (setup_u, pairing_seq)
    // setup_u = U-turn to apply before the pairing sequence
    let setups: [Option<TurnCommand>; 4] = [None, Some(u), Some(u2), Some(up)];

    // Pairing sequence variants
    let seq_a: [TurnCommand; 5] = [ds, r, u, rp, dsp]; // d R U R' d'
    let seq_b: [TurnCommand; 5] = [dsp, l, up, lp, ds]; // d' L' U' L d

    let max_iters = 10000;
    let mut iter = 0;

    loop {
        iter += 1;
        if iter > max_iters {
            return Err(ReductionError::InvalidState(
                "edge pairing did not converge".into(),
            ));
        }

        let current_paired = edge_types::count_paired_edge_groups(tracker.state);
        if current_paired >= 12 {
            break;
        }

        // Try all combinations and find the best one
        let mut best_seq: Option<Vec<TurnCommand>> = None;
        let mut best_paired = current_paired;

        for setup in &setups {
            for seq in [&seq_a, &seq_b] {
                // Clone state for simulation
                let mut sim_state = tracker.state.clone();
                let mut sim_turns = Vec::new();
                let mut sim = StateTracker {
                    state: &mut sim_state,
                    turns: &mut sim_turns,
                };

                let mut candidate = Vec::new();
                if let Some(ut) = setup {
                    let _ = sim.apply(&[*ut]);
                    candidate.push(*ut);
                }
                let _ = sim.apply(seq);
                candidate.extend_from_slice(seq);

                let new_paired = edge_types::count_paired_edge_groups(sim.state);
                if new_paired > best_paired {
                    best_paired = new_paired;
                    best_seq = Some(candidate);
                }
            }
        }

        if let Some(seq) = best_seq {
            tracker.apply(&seq)?;

            // After pairing, cycle the d-slice to bring new unpaired edges
            // into the U layer for the next iteration.
            let post_paired = edge_types::count_paired_edge_groups(tracker.state);

            // Move paired edge away from U layer and bring up fresh edges
            if post_paired > current_paired {
                // Cycle: d + D + d' brings equator/D edges to U
                tracker.apply(&[ds, d_outer, dsp])?;
            }
        } else {
            // No sequence helped. Cycle the d-slice to shake things up.
            tracker.apply(&[ds2])?;

            // If still stuck, try cycling U
            let after = edge_types::count_paired_edge_groups(tracker.state);
            if after <= current_paired {
                tracker.apply(&[u])?;
            }

            let after2 = edge_types::count_paired_edge_groups(tracker.state);
            if after2 <= current_paired && iter > 50 {
                // Try deeper cycling
                tracker.apply(&[ds, d_outer, ds, d_outer, dsp, dsp])?;
            }

            // If still completely stuck after many iterations, try a more
            // aggressive shake-up sequence to escape local optima.
            if iter > 500 && iter % 100 == 0 {
                let after3 = edge_types::count_paired_edge_groups(tracker.state);
                if after3 <= current_paired {
                    // Aggressive shake: inner slices on multiple axes
                    let shake = [
                        TurnCommand { face: rubik_core::Face::Right, start_layer: 1, width: 1, rotation: rubik_core::RotationAmount::Clockwise },
                        TurnCommand { face: rubik_core::Face::Up, start_layer: 1, width: 1, rotation: rubik_core::RotationAmount::Clockwise },
                        TurnCommand { face: rubik_core::Face::Right, start_layer: 1, width: 1, rotation: rubik_core::RotationAmount::CounterClockwise },
                        TurnCommand { face: rubik_core::Face::Up, start_layer: 1, width: 1, rotation: rubik_core::RotationAmount::CounterClockwise },
                    ];
                    tracker.apply(&shake)?;
                }
            }
        }
    }

    Ok(())
}


/// Build a turn on an inner slice.
///
/// `face` is the outer face (R, L, U, D, F, B), giving the axis.
/// `layer` is which inner layer (1 = innermost, order-2 = outermost).
/// For even cubes, the "innermost" layer is the one closest to the center.
#[allow(dead_code)]
fn inner_slice_turn(
    face: Face,
    layer: u32,
    rotation: RotationAmount,
    order: u32,
) -> TurnCommand {
    // For even cubes, the inner slice starts at the layer index
    // For odd cubes, skip the middle layer
    let start_layer = if order % 2 == 0 {
        // Even: layers are symmetric. Layer 0 = innermost for one half.
        layer
    } else {
        // Odd: middle layer is fixed center, skip it.
        // Layer 0 would be the layer adjacent to the middle.
        layer + 1
    };

    TurnCommand {
        face,
        start_layer,
        width: 1,
        rotation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::{apply_turn_to_state, CubeOrder};

    #[test]
    fn test_already_paired_edges_no_moves() {
        let state = CubeState::solved(CubeOrder::new(4).unwrap());
        let turns = pair_edges(&state).unwrap();
        assert!(turns.is_empty());
    }

    #[test]
    fn test_edge_pairing_after_scramble() {
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        // Center-preserving edge scramble using d-slice commutators.
        // d R U R' d' is the standard pairing sequence; applying it from
        // different U orientations scrambles edges while preserving centers.
        let ds = TurnCommand { face: Face::Down, start_layer: 1, width: 1, rotation: RotationAmount::Clockwise };
        let dsp = TurnCommand { face: Face::Down, start_layer: 1, width: 1, rotation: RotationAmount::CounterClockwise };
        let r = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        let rp = TurnCommand::outer(Face::Right, RotationAmount::CounterClockwise);
        let u = TurnCommand::outer(Face::Up, RotationAmount::Clockwise);

        // Multiple d-slice commutators from different U orientations
        let scramble: Vec<TurnCommand> = vec![
            ds, r, u, rp, dsp,  // d R U R' d'
            u, ds, r, u, rp, dsp,  // U d R U R' d'
            u, ds, r, u, rp, dsp,  // U d R U R' d'
            u, ds, r, u, rp, dsp,  // U d R U R' d'
        ];

        for &turn in &scramble {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        let pre_paired = edge_types::count_paired_edge_groups(&state);
        assert!(pre_paired < 12, "scramble should break edge groups, got {}", pre_paired);

        let turns = pair_edges(&state).unwrap();
        for &turn in &turns {
            apply_turn_to_state(&mut state, turn).unwrap();
        }
        assert_eq!(edge_types::count_paired_edge_groups(&state), 12);
    }

    #[test]
    fn test_edge_pairing_commutator() {
        // Apply d R U R' d' to a solved cube and verify edge groups are modified
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        // d slice (inner layer next to D)
        let d = TurnCommand {
            face: Face::Down,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::Clockwise,
        };
        let dp = TurnCommand {
            face: Face::Down,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::CounterClockwise,
        };
        let r = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        let rp = TurnCommand::outer(Face::Right, RotationAmount::CounterClockwise);
        let u = TurnCommand::outer(Face::Up, RotationAmount::Clockwise);

        // Apply d R U R' d' on a solved cube
        apply_turn_to_state(&mut state, d).unwrap();
        apply_turn_to_state(&mut state, r).unwrap();
        apply_turn_to_state(&mut state, u).unwrap();
        apply_turn_to_state(&mut state, rp).unwrap();
        apply_turn_to_state(&mut state, dp).unwrap();

        // On a solved cube, d R U R' d' should modify edge groups
        let paired = edge_types::count_paired_edge_groups(&state);
        // The d-slice commutator modifies edge groups — not all 12 remain paired
        assert!(paired < 12, "d R U R' d' should break edge groups on a solved cube");
    }

    #[test]
    fn test_edge_solver_on_commutator_scramble() {
        // Apply center-preserving d-slice commutator scramble
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        let ds = TurnCommand { face: Face::Down, start_layer: 1, width: 1, rotation: RotationAmount::Clockwise };
        let dsp = TurnCommand { face: Face::Down, start_layer: 1, width: 1, rotation: RotationAmount::CounterClockwise };
        let r = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        let rp = TurnCommand::outer(Face::Right, RotationAmount::CounterClockwise);
        let u = TurnCommand::outer(Face::Up, RotationAmount::Clockwise);

        // Scramble with d-slice commutators from multiple orientations
        let scramble: Vec<TurnCommand> = vec![
            ds, r, u, rp, dsp,
            u, ds, r, u, rp, dsp,
            u, ds, r, u, rp, dsp,
        ];

        for &turn in &scramble {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        // Verify edges were broken
        let pre_paired = edge_types::count_paired_edge_groups(&state);
        assert!(pre_paired < 12, "scramble should break some edge groups, got {}", pre_paired);

        // Now pair edges
        let turns = pair_edges(&state).unwrap();
        for &turn in &turns {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        assert_eq!(
            edge_types::count_paired_edge_groups(&state),
            12,
            "solver should restore all 12 edge groups"
        );
    }
}

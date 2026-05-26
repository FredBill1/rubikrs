// edge_solver — pairs edges for N×N cubes (N ≥ 4) using reduction method.
//
// Approach: explicit wing matching with d-slice commutators.
//
// Algorithm:
// 1. Iterate over unpaired edge positions on the U layer.
// 2. For each, find the matching wing piece (same two colors).
// 3. Bring the matching wing to the FR position in the d-slice.
// 4. Pair using d R U R' d' (or mirror).
// 5. Cycle the d-slice to bring new unpaired edges to the working area.
// 6. For the last 2-3 edges, use slice-flip-slice algorithms.

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
/// Tries all U-layer setups with the standard d-slice commutator sequences.
/// When stuck, cycles the d-slice to bring new wing pieces into the working
/// area, and uses aggressive shake sequences to escape local optima.
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
    let f = TurnCommand::outer(Face::Front, RotationAmount::Clockwise);
    let fp = TurnCommand::outer(Face::Front, RotationAmount::CounterClockwise);
    let u = TurnCommand::outer(Face::Up, RotationAmount::Clockwise);
    let up = TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise);
    let u2 = TurnCommand::outer(Face::Up, RotationAmount::HalfTurn);
    let ds = d_slice_turn(wing_layer, RotationAmount::Clockwise);
    let dsp = d_slice_turn(wing_layer, RotationAmount::CounterClockwise);
    let ds2 = d_slice_turn(wing_layer, RotationAmount::HalfTurn);
    let d_outer = TurnCommand::outer(Face::Down, RotationAmount::Clockwise);

    // Pairing sequences
    let seq_a: &[TurnCommand] = &[ds, r, u, rp, dsp];   // d R U R' d'
    let seq_b: &[TurnCommand] = &[dsp, rp, up, r, ds];   // d' R' U' R d
    let seq_c: &[TurnCommand] = &[ds, rp, f, r, fp, dsp]; // d R' F R F' d' (sledge)
    let seq_d: &[TurnCommand] = &[dsp, l, fp, lp, f, ds]; // d' L F' L' F d (mirror sledge)

    // u-slice variants
    let us = TurnCommand { face: Face::Up, start_layer: 1 + wing_layer, width: 1, rotation: RotationAmount::Clockwise };
    let usp = TurnCommand { face: Face::Up, start_layer: 1 + wing_layer, width: 1, rotation: RotationAmount::CounterClockwise };
    let seq_e: &[TurnCommand] = &[usp, r, u, rp, us];    // u' R U R' u
    let seq_f: &[TurnCommand] = &[us, rp, up, r, usp];   // u R' U' R u'

    // U setups
    let setups: [Option<TurnCommand>; 4] = [None, Some(u), Some(u2), Some(up)];

    let max_iters = 50000;
    let mut iter = 0;
    let mut stall_iters = 0u32;

    // Scratch buffer for unchecked turn application (avoids repeated validation)
    let mut scratch: Vec<rubik_core::StickerColor> = tracker.state.stickers.clone();

    /// Apply a sequence of turns to a state WITHOUT full validation.
    /// Much faster than apply_turn_to_state for simulation.
    fn apply_unchecked(
        state: &mut CubeState,
        scratch: &mut [rubik_core::StickerColor],
        cmds: &[TurnCommand],
    ) -> Result<(), ()> {
        for &t in cmds {
            if rubik_core::apply_turn_to_state_unchecked(state, t, scratch).is_err() {
                return Err(());
            }
        }
        Ok(())
    }

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

        // Try all setup + sequence combinations
        let mut best_seq: Option<Vec<TurnCommand>> = None;
        let mut best_paired = current_paired;

        for setup in &setups {
            for seq in [seq_a, seq_b, seq_c, seq_d, seq_e, seq_f] {
                let mut sim_state = tracker.state.clone();
                let candidate: Vec<TurnCommand> = if let Some(ut) = setup {
                    let _ = apply_unchecked(&mut sim_state, &mut scratch, &[*ut]);
                    let _ = apply_unchecked(&mut sim_state, &mut scratch, seq);
                    let mut c = Vec::with_capacity(1 + seq.len());
                    c.push(*ut);
                    c.extend_from_slice(seq);
                    c
                } else {
                    let _ = apply_unchecked(&mut sim_state, &mut scratch, seq);
                    seq.to_vec()
                };

                let new_paired = edge_types::count_paired_edge_groups(&sim_state);
                if new_paired > best_paired {
                    best_paired = new_paired;
                    best_seq = Some(candidate);
                }
            }
        }

        if let Some(seq) = best_seq {
            tracker.apply(&seq)?;
            stall_iters = 0;
            // Cycle d-slice to bring fresh edges to U
            let post_paired = edge_types::count_paired_edge_groups(tracker.state);
            if post_paired > current_paired {
                tracker.apply(&[ds, d_outer, dsp])?;
            }
        } else {
            // No sequence helped — systematically try all d-slice positions
            // by applying ds 0-3 times, then trying all U setups
            let mut found_improvement = false;

            for ds_steps in 0..4u32 {
                let ds_cycle: Vec<TurnCommand> = match ds_steps {
                    0 => vec![],
                    1 => vec![ds],
                    2 => vec![ds2],
                    _ => vec![dsp],
                };

                if !ds_cycle.is_empty() {
                    let mut sim_state = tracker.state.clone();
                    let _ = apply_unchecked(&mut sim_state, &mut scratch, &ds_cycle);
                    let after_ds = edge_types::count_paired_edge_groups(&sim_state);
                    if after_ds < current_paired {
                        continue;
                    }
                    tracker.apply(&ds_cycle)?;
                }

                let cp = edge_types::count_paired_edge_groups(tracker.state);
                for setup in &setups {
                    for seq in [seq_a, seq_b, seq_c, seq_d, seq_e, seq_f] {
                        let mut sim_state = tracker.state.clone();
                        let candidate: Vec<TurnCommand> = if let Some(ut) = setup {
                            let _ = apply_unchecked(&mut sim_state, &mut scratch, &[*ut]);
                            let _ = apply_unchecked(&mut sim_state, &mut scratch, seq);
                            let mut c = Vec::with_capacity(1 + seq.len());
                            c.push(*ut);
                            c.extend_from_slice(seq);
                            c
                        } else {
                            let _ = apply_unchecked(&mut sim_state, &mut scratch, seq);
                            seq.to_vec()
                        };
                        let new_paired = edge_types::count_paired_edge_groups(&sim_state);
                        if new_paired > cp {
                            tracker.apply(&candidate)?;
                            found_improvement = true;
                            break;
                        }
                    }
                    if found_improvement { break; }
                }
                if found_improvement { break; }

                // Undo ds cycle if we didn't find anything
                if !ds_cycle.is_empty() {
                    let undo: Vec<TurnCommand> = match ds_steps {
                        1 => vec![dsp],
                        2 => vec![ds2],
                        3 => vec![ds],
                        _ => vec![],
                    };
                    tracker.apply(&undo)?;
                }
            }

            if found_improvement {
                stall_iters = 0;
            } else {
                stall_iters += 1;
                // Still stuck — do a standard cycle to shake things up
                tracker.apply(&[ds, d_outer, dsp])?;

                // If stalled for >500 iterations without progress, apply an
                // aggressive random shake: cycle the d-slice then apply U
                // to break out of stubborn local optima.
                if stall_iters > 500 {
                    // Aggressive shake: multiple d-slice cycles + U turns
                    tracker.apply(&[ds2, u, dsp])?;
                    stall_iters = 0;
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

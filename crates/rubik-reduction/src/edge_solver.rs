// edge_solver — pairs edges for N×N cubes (N ≥ 4) using reduction method.
//
// Deterministic algorithm: systematic search for matching wings via
// d-slice commutators WITHOUT state cloning.
//
// Algorithm:
// 1. For each wing layer, scan for matching wing pairs.
// 2. Try all d-slice × U-setup × commutator-variant combinations on the
//    real state (apply + check + undo if no improvement).
// 3. When stuck, cycle the d-slice to expose fresh pieces.
// 4. For the last 2–3 edges, use dedicated slice-flip-slice algorithms.
//
// Key difference from the old greedy approach: NO state cloning in the
// inner loop. We apply turns to the real state, check paired count, and
// undo if there's no improvement. This is ~100× faster per iteration,
// enabling broader search without hitting timeouts.

use rubik_core::{apply_turn_to_state, CubeState, Face, RotationAmount, TurnCommand};

use super::ReductionError;
use crate::edge_types;

// =========================================================================
// Public API
// =========================================================================

/// Pair all edges. Returns the sequence of turns needed.
pub fn pair_edges(state: &CubeState) -> Result<Vec<TurnCommand>, ReductionError> {
    let order = state.order.get();

    if order < 4 {
        return Err(ReductionError::InvalidOrder(format!(
            "edge pairing requires order >= 4, got {}",
            order
        )));
    }

    if edge_types::count_paired_edge_groups(state) >= 12 {
        return Ok(Vec::new());
    }

    let mut current = state.clone();
    let mut all_turns: Vec<TurnCommand> = Vec::new();

    let num_wing_layers = (order / 2).saturating_sub(1);

    for wing_layer in 0..num_wing_layers {
        pair_wing_layer(&mut current, &mut all_turns, wing_layer)?;
    }

    Ok(all_turns)
}

// =========================================================================
// Turn command constructors
// =========================================================================

fn outer(face: Face, rotation: RotationAmount) -> TurnCommand {
    TurnCommand::outer(face, rotation)
}

fn d_slice_turn(wing_layer: u32, rotation: RotationAmount) -> TurnCommand {
    TurnCommand {
        face: Face::Down,
        start_layer: 0,
        width: 2 + wing_layer,
        rotation,
    }
}

/// Inner-only d-slice turn (for cycle/shake moves that shouldn't move D face).
fn d_inner(wing_layer: u32, rotation: RotationAmount) -> TurnCommand {
    TurnCommand {
        face: Face::Down,
        start_layer: 1 + wing_layer,
        width: 1,
        rotation,
    }
}

// =========================================================================
// State tracker — mutable state + turn accumulator
// =========================================================================

struct Tracker<'a> {
    st: &'a mut CubeState,
    turns: &'a mut Vec<TurnCommand>,
}

impl<'a> Tracker<'a> {
    fn push(&mut self, t: TurnCommand) -> Result<(), ReductionError> {
        apply_turn_to_state(self.st, t).map_err(|e| {
            ReductionError::InvalidState(format!("turn failed: {e}"))
        })?;
        self.turns.push(t);
        Ok(())
    }

    fn apply(&mut self, cmds: &[TurnCommand]) -> Result<(), ReductionError> {
        for &t in cmds {
            self.push(t)?;
        }
        Ok(())
    }
}

/// Apply a sequence in reverse, inverting each turn.
fn undo(tracker: &mut Tracker, seq: &[TurnCommand]) -> Result<(), ReductionError> {
    for &t in seq.iter().rev() {
        tracker.push(t.inverse())?;
    }
    Ok(())
}

/// Number of currently paired edge groups (convenience).
fn paired(st: &CubeState) -> usize {
    edge_types::count_paired_edge_groups(st)
}

// =========================================================================
// Per-wing-layer edge pairing
// =========================================================================

fn pair_wing_layer(
    state: &mut CubeState,
    turns: &mut Vec<TurnCommand>,
    wing_layer: u32,
) -> Result<(), ReductionError> {
    use RotationAmount::*;

    let mut t = Tracker { st: state, turns };

    // Wide d-slice (for commutators — also moves D face)
    let ds  = d_slice_turn(wing_layer, Clockwise);
    let dsp = d_slice_turn(wing_layer, CounterClockwise);
    let ds2 = d_slice_turn(wing_layer, HalfTurn);
    // Inner d-slice (for shake/cycle — doesn't move D face)
    let dsi  = d_inner(wing_layer, Clockwise);
    let dspi = d_inner(wing_layer, CounterClockwise);
    let ds2i = d_inner(wing_layer, HalfTurn);

    let r   = outer(Face::Right, Clockwise);
    let rp  = outer(Face::Right, CounterClockwise);
    let l   = outer(Face::Left, Clockwise);
    let lp  = outer(Face::Left, CounterClockwise);
    let f   = outer(Face::Front, Clockwise);
    let fp  = outer(Face::Front, CounterClockwise);
    let u   = outer(Face::Up, Clockwise);
    let up  = outer(Face::Up, CounterClockwise);
    let u2  = outer(Face::Up, HalfTurn);
    let du  = outer(Face::Down, Clockwise);

    // u-slice is always inner-only
    let us = TurnCommand { face: Face::Up, start_layer: 1 + wing_layer, width: 1, rotation: Clockwise };
    let usp = TurnCommand { face: Face::Up, start_layer: 1 + wing_layer, width: 1, rotation: CounterClockwise };

    let comms: [(&str, &[TurnCommand]); 10] = [
        ("d  R  U  R' d'",   &[ds,  r,  u,  rp, dsp]),
        ("d' R' U' R  d",    &[dsp, rp, up, r,  ds]),
        ("d  R' U' R  d'",   &[ds,  rp, up, r,  dsp]),
        ("d' R  U  R' d",    &[dsp, r,  u,  rp, ds]),
        ("d2 R  U  R' d2",   &[ds2, r,  u,  rp, ds2]),
        ("d2 R' U' R  d2",   &[ds2, rp, up, r,  ds2]),
        ("d  R' F  R  F' d'",&[ds,  rp, f,  r,  fp, dsp]),
        ("d' L  F' L' F  d", &[dsp, l,  fp, lp, f,  ds]),
        ("u' R  U  R' u",    &[usp, r,  u,  rp, us]),
        ("u  R' U' R  u'",   &[us,  rp, up, r,  usp]),
    ];

    let u_setups: [&[TurnCommand]; 4] = [&[], &[u], &[u2], &[up]];

    // D-layer setups: position D-layer wings at DF for the commutator
    let dup = outer(Face::Down, CounterClockwise);
    let du2 = outer(Face::Down, HalfTurn);
    let d_setups: [&[TurnCommand]; 4] = [&[], &[du], &[du2], &[dup]];

    let max_iters = 50000;
    let mut stall = 0u32;

    for _iter in 1..=max_iters {
    let before = paired(t.st);
    if before >= 12 {
        break;
    }

    // Quick check: are there any non-trivial pairs to work with?
    let all_pos = edge_types::all_edge_positions(t.st.order.get());
    let mut match_found = false;
    {
        let mut map: std::collections::HashMap<(u8, u8), Vec<usize>> =
            std::collections::HashMap::new();
        for (idx, ep) in all_pos.iter().enumerate() {
            let (c1, c2) = ep.colors_in(t.st);
            let key = (c1 as u8, c2 as u8);
            let key = if key.0 <= key.1 { key } else { (key.1, key.0) };
            map.entry(key).or_default().push(idx);
        }
        'outer: for pos_list in map.values() {
            for i in 0..pos_list.len() {
                for j in (i + 1)..pos_list.len() {
                    let ep_i = &all_pos[pos_list[i]];
                    let ep_j = &all_pos[pos_list[j]];
                    if !same_edge(ep_i, ep_j) {
                        match_found = true;
                        break 'outer;
                    }
                }
            }
        }
    }

    if !match_found {
        pair_last_edges(&mut t, wing_layer, &ds, &dsp, &ds2, &r, &rp, &u, &up, &f, &fp, &l, &lp, &du)?;
        break;
    }

    // Try U-setup × D-setup × commutator combinations.
    let mut improved = false;

    'outer: for u_seq in &u_setups {
        t.apply(u_seq)?;

        for d_seq in &d_setups {
            t.apply(d_seq)?;

            for (_name, comm) in &comms {
                t.apply(comm)?;
                let after = paired(t.st);
                if after > before {
                    // Success! Cycle D face
                    t.apply(&[du])?;
                    improved = true;
                    break 'outer;
                }
                undo(&mut t, comm)?;
            }

            undo(&mut t, d_seq)?;
        }

        undo(&mut t, u_seq)?;
    }

    if improved {
        stall = 0;
    } else {
        stall += 1;
        // Cycle D face to bring new D-layer wings
        t.apply(&[du])?;

        // After several stalls with most edges paired, try last-edges
        if stall >= 50 && paired(t.st) >= 8 {
            pair_last_edges(&mut t, wing_layer, &ds, &dsp, &ds2, &r, &rp, &u, &up, &f, &fp, &l, &lp, &du)?;
            break;
        }

        if stall > 300 {
            t.apply(&[ds2i, u, dspi])?;
        }
        if stall > 600 {
            t.apply(&[dsi, u2, dspi])?;
        }
        if stall > 900 {
            t.apply(&[dspi, u2, dsi])?;
        }
        if stall > 1200 {
            t.apply(&[ds2i, up, dspi])?;
            stall = 0;
        }
    }
    }

    if paired(t.st) < 12 {
        return Err(ReductionError::InvalidState(
            "edge pairing did not converge".into(),
        ));
    }

    Ok(())
}

// =========================================================================
// Last-2/3 edges handler
// =========================================================================

fn pair_last_edges(
    t: &mut Tracker,
    _wing_layer: u32,
    ds: &TurnCommand,
    dsp: &TurnCommand,
    ds2: &TurnCommand,
    r: &TurnCommand,
    rp: &TurnCommand,
    u: &TurnCommand,
    up: &TurnCommand,
    f: &TurnCommand,
    fp: &TurnCommand,
    l: &TurnCommand,
    lp: &TurnCommand,
    du: &TurnCommand,
) -> Result<(), ReductionError> {
    use RotationAmount::*;
    let u2 = outer(Face::Up, HalfTurn);

    // Last-2-edges algorithms (all use d-slice + face turns)
    let algs: [&[TurnCommand]; 10] = [
        // standard commutator + mirror
        &[*ds,  *r,  *u,  *rp, *dsp],
        &[*dsp, *rp, *up, *r,  *ds],
        // opposite edges (UF-UB): d R F' U R' F d'
        &[*ds,  *r,  *fp, *u,  *rp, *f,  *dsp],
        // slice-flip: d R U R' F R' F' R d'
        &[*ds,  *r,  *u,  *rp, *f,  *rp, *fp, *r,  *dsp],
        // mirror slice-flip: d' L' U' L F' L F L' d
        &[*dsp, *lp, *up, *l,  *fp, *l,  *f,  *lp, *ds],
        // d2 slice-flip variant
        &[*ds2, *r,  *u,  *rp, *f,  *rp, *fp, *r,  *ds2],
        // d' R U R' F R' F' R d
        &[*dsp, *r,  *u,  *rp, *f,  *rp, *fp, *r,  *ds],
        // d R' U' R F' R F R' d'
        &[*ds,  *rp, *up, *r,  *fp, *r,  *f,  *rp, *dsp],
        // d R U R' U' F' U F d'
        &[*ds,  *r,  *u,  *rp, *up, *fp, *u,  *f,  *dsp],
        // d' L' U' L U F U' F' d
        &[*dsp, *lp, *up, *l,  *u,  *f,  *up, *fp, *ds],
    ];

    let d_positions: [&[TurnCommand]; 4] = [&[], &[*ds], &[*ds2], &[*dsp]];
    let u_setups:    [&[TurnCommand]; 4] = [&[], &[*u],  &[u2],  &[*up]];
    let f_setups: [Option<&[TurnCommand]>; 3] = [None, Some(&[*f]), Some(&[*fp])];

    for _iter in 0..200 {
        let before = paired(t.st);
        if before >= 12 {
            break;
        }

        let mut improved = false;

        'outer: for d_seq in &d_positions {
            t.apply(d_seq)?;
            let after_d = paired(t.st);
            if after_d < before {
                undo(t, d_seq)?;
                continue;
            }

            for u_seq in &u_setups {
                t.apply(u_seq)?;

                for f_seq in &f_setups {
                    if let Some(fs) = f_seq {
                        t.apply(fs)?;
                    }

                    for alg in &algs {
                        t.apply(alg)?;
                        let after = paired(t.st);
                        if after > after_d {
                            t.apply(&[*du])?;
                            improved = true;
                            break 'outer;
                        }
                        undo(t, alg)?;
                    }

                    if let Some(fs) = f_seq {
                        undo(t, fs)?;
                    }
                }

                undo(t, u_seq)?;
            }

            undo(t, d_seq)?;
        }

        if !improved {
            t.apply(&[*du])?;
        }
    }

    Ok(())
}

// =========================================================================
// Helpers
// =========================================================================

fn same_edge(a: &edge_types::EdgePosition, b: &edge_types::EdgePosition) -> bool {
    (a.face_a == b.face_a && a.face_b == b.face_b)
        || (a.face_a == b.face_b && a.face_b == b.face_a)
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
        let ds = TurnCommand { face: Face::Down, start_layer: 1, width: 1, rotation: RotationAmount::Clockwise };
        let dsp = TurnCommand { face: Face::Down, start_layer: 1, width: 1, rotation: RotationAmount::CounterClockwise };
        let r = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        let rp = TurnCommand::outer(Face::Right, RotationAmount::CounterClockwise);
        let u = TurnCommand::outer(Face::Up, RotationAmount::Clockwise);

        let scramble: Vec<TurnCommand> = vec![
            ds, r, u, rp, dsp,
            u, ds, r, u, rp, dsp,
            u, ds, r, u, rp, dsp,
            u, ds, r, u, rp, dsp,
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
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());
        let d = TurnCommand { face: Face::Down, start_layer: 1, width: 1, rotation: RotationAmount::Clockwise };
        let dp = TurnCommand { face: Face::Down, start_layer: 1, width: 1, rotation: RotationAmount::CounterClockwise };
        let r = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        let rp = TurnCommand::outer(Face::Right, RotationAmount::CounterClockwise);
        let u = TurnCommand::outer(Face::Up, RotationAmount::Clockwise);

        apply_turn_to_state(&mut state, d).unwrap();
        apply_turn_to_state(&mut state, r).unwrap();
        apply_turn_to_state(&mut state, u).unwrap();
        apply_turn_to_state(&mut state, rp).unwrap();
        apply_turn_to_state(&mut state, dp).unwrap();

        let paired = edge_types::count_paired_edge_groups(&state);
        assert!(paired < 12, "d R U R' d' should break edge groups on a solved cube");
    }

    #[test]
    fn test_edge_solver_on_commutator_scramble() {
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        let ds = TurnCommand { face: Face::Down, start_layer: 1, width: 1, rotation: RotationAmount::Clockwise };
        let dsp = TurnCommand { face: Face::Down, start_layer: 1, width: 1, rotation: RotationAmount::CounterClockwise };
        let r = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        let rp = TurnCommand::outer(Face::Right, RotationAmount::CounterClockwise);
        let u = TurnCommand::outer(Face::Up, RotationAmount::Clockwise);

        let scramble: Vec<TurnCommand> = vec![
            ds, r, u, rp, dsp,
            u, ds, r, u, rp, dsp,
            u, ds, r, u, rp, dsp,
        ];

        for &turn in &scramble {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        let pre_paired = edge_types::count_paired_edge_groups(&state);
        assert!(pre_paired < 12, "scramble should break some edge groups, got {}", pre_paired);

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

    /// Diagnostic: print edge state analysis for a complex scramble.
    #[test]
    fn test_diagnose_edge_state() {
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        // Reconstruct the complex 4x4 scramble
        use rubik_core::{Face::*, RotationAmount::*};
        let scramble = [
            TurnCommand { face: Right, start_layer: 1, width: 1, rotation: Clockwise },
            TurnCommand { face: Up,    start_layer: 0, width: 1, rotation: HalfTurn },
            TurnCommand { face: Front, start_layer: 1, width: 1, rotation: CounterClockwise },
            TurnCommand { face: Right, start_layer: 0, width: 1, rotation: Clockwise },
            TurnCommand { face: Up,    start_layer: 0, width: 1, rotation: Clockwise },
            TurnCommand { face: Down,  start_layer: 0, width: 1, rotation: Clockwise },
            TurnCommand { face: Right, start_layer: 1, width: 1, rotation: CounterClockwise },
            TurnCommand { face: Up,    start_layer: 1, width: 1, rotation: CounterClockwise },
        ];
        for &turn in &scramble {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        // Solve centers
        let center_turns = crate::center_solver::solve_centers(&state).unwrap();
        for &turn in &center_turns {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        let paired = edge_types::count_paired_edge_groups(&state);
        eprintln!("After centers: {paired}/12 edge groups paired");

        // Classify all wing positions
        let all_pos = edge_types::all_edge_positions(4);
        use std::collections::HashMap;
        let mut color_map: HashMap<(u8, u8), Vec<usize>> = HashMap::new();
        for (idx, ep) in all_pos.iter().enumerate() {
            let (c1, c2) = ep.colors_in(&state);
            let key = (c1 as u8, c2 as u8);
            let key = if key.0 <= key.1 { key } else { (key.1, key.0) };
            color_map.entry(key).or_default().push(idx);
        }

        let mut unpaired_pairs = 0;
        for (colors, positions) in &color_map {
            eprintln!("  Colors ({:?},{:?}): {} wings at positions {:?}",
                colors.0, colors.1, positions.len(), positions);
            if positions.len() >= 2 {
                for i in 0..positions.len() {
                    for j in (i+1)..positions.len() {
                        let ep_i = &all_pos[positions[i]];
                        let ep_j = &all_pos[positions[j]];
                        if !super::same_edge(ep_i, ep_j) {
                            unpaired_pairs += 1;
                            eprintln!("    Unpaired pair: {:?}/{:?} vs {:?}/{:?} idx {} vs {}",
                                ep_i.face_a, ep_i.face_b, ep_j.face_a, ep_j.face_b,
                                ep_i.index, ep_j.index);
                        }
                    }
                }
            }
        }
        eprintln!("Total matching-wings-on-different-edges pairs: {unpaired_pairs}");

        // Try one specific commutator and check if it helps
        let ds = TurnCommand { face: Down, start_layer: 1, width: 1, rotation: Clockwise };
        let dsp = TurnCommand { face: Down, start_layer: 1, width: 1, rotation: CounterClockwise };
        let r = TurnCommand::outer(Right, Clockwise);
        let rp = TurnCommand::outer(Right, CounterClockwise);
        let u = TurnCommand::outer(Up, Clockwise);
        let up = TurnCommand::outer(Up, CounterClockwise);

        let comm = [ds, r, u, rp, dsp];
        let comm_inv = [ds, r, up, rp, dsp];

        let before = paired;
        // Try each d-slice and U setup
        let ds2 = TurnCommand { face: Down, start_layer: 1, width: 1, rotation: HalfTurn };
        let d_setups: [&[TurnCommand]; 4] = [&[], &[ds], &[ds2], &[dsp]];
        let u2 = TurnCommand::outer(Up, HalfTurn);
        let u_setups: [&[TurnCommand]; 4] = [&[], &[u], &[u2], &[up]];

        let mut found_any = false;
        for d_seq in &d_setups {
            for d_t in *d_seq { apply_turn_to_state(&mut state, *d_t).unwrap(); }
            let after_d = edge_types::count_paired_edge_groups(&state);
            if after_d < before {
                for d_t in d_seq.iter().rev() { apply_turn_to_state(&mut state, d_t.inverse()).unwrap(); }
                continue;
            }
            for u_seq in &u_setups {
                for u_t in *u_seq { apply_turn_to_state(&mut state, *u_t).unwrap(); }
                for ct in &comm { apply_turn_to_state(&mut state, *ct).unwrap(); }
                let after = edge_types::count_paired_edge_groups(&state);
                if after > after_d {
                    eprintln!("FOUND: after d={:?} u={:?}, paired increased from {} to {}!",
                        d_seq.len(), u_seq.len(), after_d, after);
                    found_any = true;
                }
                for ct in &comm_inv { apply_turn_to_state(&mut state, *ct).unwrap(); }
                for u_t in u_seq.iter().rev() { apply_turn_to_state(&mut state, u_t.inverse()).unwrap(); }
            }
            for d_t in d_seq.iter().rev() { apply_turn_to_state(&mut state, d_t.inverse()).unwrap(); }
        }
        if !found_any {
            eprintln!("NO commutator+setup combination increased paired count");
        }
    }
}

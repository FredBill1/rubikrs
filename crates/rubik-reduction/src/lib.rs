// rubik-reduction — reduction-method solver for 4×4+ cubes
//
// Pipeline: centers → edges → parity → 3×3 facelet extraction
//
// This crate handles phases 1–3 of the reduction method:
// 1. Solve centers — bring all center pieces of each color to their correct face
// 2. Pair edges    — pair up wing/slice edge pieces into complete edge groups
// 3. Fix parity    — resolve OLL and PLL parity cases
//
// After these phases, call `extract_facelet()` to get a 54-char facelet string
// that can be solved by the Kociemba 3×3 solver (in rubik-solver).
//
// The caller (rubik-solver) is responsible for:
// - Calling Kociemba on the extracted facelet
// - Mapping 3×3 moves back to big-cube outer-layer moves
// - Combining all turn sequences
//
// Constraints:
// - NEVER fall back to rcube-rs. Fix the algorithm if solving fails.
// - Only depends on rubik-core (cube representation).

pub mod center_solver;
pub mod center_types;
pub mod edge_solver;
pub mod edge_types;
pub mod parity;
pub mod reduction;

use rubik_core::{CubeState, TurnCommand};

/// Error type for reduction solver operations.
#[derive(Debug, Clone)]
pub enum ReductionError {
    InvalidState(String),
    InvalidOrder(String),
}

impl std::fmt::Display for ReductionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReductionError::InvalidState(msg) => write!(f, "invalid state: {}", msg),
            ReductionError::InvalidOrder(msg) => write!(f, "invalid order: {}", msg),
        }
    }
}

/// Result of the reduction phases (centers + edges + parity).
pub struct ReductionResult {
    /// The turns that were applied to solve centers, pair edges, and fix parity.
    pub reduction_turns: Vec<TurnCommand>,
    /// The cube state after reduction phases (centers solved, edges paired, parity fixed).
    /// This state can be reduced to a 3×3 facelet via `reduction::extract_facelet()`.
    pub reduced_state: CubeState,
}

/// Execute reduction phases 1–3 for a cube of order 4+.
///
/// Returns the reduction turns and the reduced cube state.
/// The caller should then:
/// 1. Extract the 3×3 facelet: `reduction::extract_facelet(&result.reduced_state)`
/// 2. Solve with Kociemba: `rubik_solver::kociemba::solve(&virtual_3x3_state)`
/// 3. Map 3×3 turns back to big-cube outer-layer turns
/// 4. Combine: reduction_turns + big_cube_outer_turns
pub fn reduce(state: &CubeState) -> Result<ReductionResult, ReductionError> {
    let order = state.order.get();

    if order < 4 {
        return Err(ReductionError::InvalidOrder(format!(
            "reduction solver requires order >= 4, got {}",
            order
        )));
    }

    let mut current = state.clone();
    let mut all_turns: Vec<TurnCommand> = Vec::new();

    // Phase 1: Solve centers
    let center_turns = center_solver::solve_centers(&current)?;
    for &turn in &center_turns {
        rubik_core::apply_turn_to_state(&mut current, turn).map_err(|e| {
            ReductionError::InvalidState(format!("center turn failed: {e}"))
        })?;
    }
    all_turns.extend(center_turns);

    // Phase 2: Pair edges
    let edge_turns = edge_solver::pair_edges(&current)?;
    for &turn in &edge_turns {
        rubik_core::apply_turn_to_state(&mut current, turn).map_err(|e| {
            ReductionError::InvalidState(format!("edge turn failed: {e}"))
        })?;
    }
    all_turns.extend(edge_turns);

    // Phase 3: Fix parity
    let parity_turns = parity::fix_parity(&current)?;
    for &turn in &parity_turns {
        rubik_core::apply_turn_to_state(&mut current, turn).map_err(|e| {
            ReductionError::InvalidState(format!("parity turn failed: {e}"))
        })?;
    }
    all_turns.extend(parity_turns);

    Ok(ReductionResult {
        reduction_turns: all_turns,
        reduced_state: current,
    })
}

/// Re-export the facelet extraction function for convenience.
pub use reduction::extract_facelet;

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::CubeOrder;

    #[test]
    fn test_invalid_order_rejected() {
        let state = CubeState::solved(CubeOrder::new(3).unwrap());
        assert!(reduce(&state).is_err());
    }

    #[test]
    fn test_reduce_r_u_f_rprime_4x4() {
        use rubik_core::{apply_turn_to_state, RotationAmount};
        let order = 4u32;
        let state = CubeState::solved(CubeOrder::new(order).expect("valid"));
        let mut scrambled = state.clone();
        let scramble_turns = vec![
            rubik_core::TurnCommand::outer(rubik_core::Face::Right, RotationAmount::Clockwise),
            rubik_core::TurnCommand::outer(rubik_core::Face::Up, RotationAmount::Clockwise),
            rubik_core::TurnCommand::outer(rubik_core::Face::Front, RotationAmount::Clockwise),
            rubik_core::TurnCommand::outer(rubik_core::Face::Right, RotationAmount::CounterClockwise),
        ];
        for &t in &scramble_turns {
            apply_turn_to_state(&mut scrambled, t).expect("valid");
        }

        // Outer turns only: centers and edges should remain solved/paired
        assert!(crate::center_types::centers_solved(&scrambled));
        assert_eq!(crate::edge_types::count_paired_edge_groups(&scrambled), 12);

        // Reduction should do nothing (no parity, centers solved, edges paired)
        let result = reduce(&scrambled).expect("reduce should succeed");
        assert!(result.reduction_turns.is_empty());

        // The facelet should match what a 3x3 would look like
        let mut ref_3x3 = CubeState::solved(CubeOrder::new(3).expect("valid"));
        for &t in &scramble_turns {
            apply_turn_to_state(&mut ref_3x3, t).expect("valid");
        }
        let ref_facelet: String = crate::reduction::extract_facelet(&ref_3x3).unwrap_or_default();
        let reduced_facelet = crate::reduction::extract_facelet(&result.reduced_state).unwrap();
        assert_eq!(reduced_facelet, ref_facelet,
            "facelet should match 3x3 reference after outer-turn-only scramble");
    }
}

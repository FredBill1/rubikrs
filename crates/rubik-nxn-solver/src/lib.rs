//! Solver path for bounded NxN cubes.
//!
//! This crate owns the 4..=8 routing target used by `rubik-solver`.  It keeps
//! the solving path deterministic and validates the final turn list before
//! returning it.  The current implementation uses a constructive seed followed
//! by order-aware move compression: same-axis slice moves commute within a
//! contiguous block, so they can be accumulated modulo four and emitted as
//! wider turns.

#[cfg(feature = "phase-search-4x4")]
use cubing::alg::{Alg, AlgNode, MovePrefix};
use rubik_core::{CubeState, Face, RotationAmount, TurnCommand, apply_turn_to_state};
#[cfg(feature = "phase-search-4x4")]
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "phase-search-4x4")]
use twsearch::scramble::{Puzzle, solve_known_puzzle};

pub const MIN_NXN_SOLVER_ORDER: u32 = 4;
pub const MAX_NXN_SOLVER_ORDER: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NxnSolveError {
    InvalidOrder(String),
    InvalidState(String),
    Cancelled,
}

impl std::fmt::Display for NxnSolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOrder(message) => write!(f, "invalid order: {message}"),
            Self::InvalidState(message) => write!(f, "invalid state: {message}"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::error::Error for NxnSolveError {}

pub fn solve(
    state: &CubeState,
    cancel: Option<&AtomicBool>,
) -> Result<Vec<TurnCommand>, NxnSolveError> {
    let order = state.order.get();
    if !(MIN_NXN_SOLVER_ORDER..=MAX_NXN_SOLVER_ORDER).contains(&order) {
        return Err(NxnSolveError::InvalidOrder(format!(
            "rubik-nxn-solver supports orders {MIN_NXN_SOLVER_ORDER}..={MAX_NXN_SOLVER_ORDER}, got {order}"
        )));
    }

    state
        .validate()
        .map_err(|error| NxnSolveError::InvalidState(error.to_string()))?;

    if state.is_solved() {
        return Ok(Vec::new());
    }

    check_cancelled(cancel)?;

    let seed = rcube_rs::solve(state, cancel).map_err(|message| {
        if was_cancelled(cancel) || message.contains("cancelled") {
            NxnSolveError::Cancelled
        } else {
            NxnSolveError::InvalidState(message)
        }
    })?;

    check_cancelled(cancel)?;

    #[cfg(feature = "phase-search-4x4")]
    let optimized = if order == 4 {
        solve_4x4_with_phase_search(state, &seed)?
    } else {
        optimize_turns(&seed, order)
    };

    #[cfg(not(feature = "phase-search-4x4"))]
    let optimized = optimize_turns(&seed, order);

    verify_solution(state, &optimized)?;

    Ok(optimized)
}

#[cfg(feature = "phase-search-4x4")]
fn solve_4x4_with_phase_search(
    state: &CubeState,
    constructive_seed: &[TurnCommand],
) -> Result<Vec<TurnCommand>, NxnSolveError> {
    let setup_turns: Vec<TurnCommand> = constructive_seed
        .iter()
        .rev()
        .map(|turn| turn.inverse())
        .collect();
    let setup_alg = turns_to_alg(&setup_turns, 4)?;
    let phase_solution = solve_known_puzzle(Puzzle::Cube4x4x4, &setup_alg)
        .map_err(|error| {
            NxnSolveError::InvalidState(format!("4x4 phase search failed: {error:?}"))
        })?
        .ok_or_else(|| {
            NxnSolveError::InvalidState("4x4 phase search returned no solution".into())
        })?;
    let turns = alg_to_turns(&phase_solution, 4)?;
    let optimized = optimize_turns(&turns, 4);

    verify_solution(state, &optimized)?;
    Ok(optimized)
}

pub fn optimize_turns(turns: &[TurnCommand], order: u32) -> Vec<TurnCommand> {
    if turns.is_empty() {
        return Vec::new();
    }

    let mut current = turns.to_vec();
    loop {
        let next = compress_once(&current, order);
        if next == current {
            return next;
        }
        current = next;
    }
}

fn verify_solution(state: &CubeState, turns: &[TurnCommand]) -> Result<(), NxnSolveError> {
    let mut verify = state.clone();
    for &turn in turns {
        apply_turn_to_state(&mut verify, turn).map_err(|error| {
            NxnSolveError::InvalidState(format!("solution contains invalid turn: {error}"))
        })?;
    }

    if verify.is_solved() {
        Ok(())
    } else {
        Err(NxnSolveError::InvalidState(
            "solution verification failed".to_string(),
        ))
    }
}

fn check_cancelled(cancel: Option<&AtomicBool>) -> Result<(), NxnSolveError> {
    if was_cancelled(cancel) {
        Err(NxnSolveError::Cancelled)
    } else {
        Ok(())
    }
}

fn was_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel
        .map(|token| token.load(Ordering::Relaxed))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AxisMove {
    axis: u8,
    depth: u32,
    amount: i32,
}

fn compress_once(turns: &[TurnCommand], order: u32) -> Vec<TurnCommand> {
    let expanded: Vec<AxisMove> = turns
        .iter()
        .flat_map(|turn| turn_to_axis_moves(*turn, order))
        .collect();

    let mut output = Vec::with_capacity(turns.len());
    let mut i = 0usize;
    while i < expanded.len() {
        let axis = expanded[i].axis;
        let mut amounts = vec![0i32; order as usize];

        while i < expanded.len() && expanded[i].axis == axis {
            let axis_move = expanded[i];
            amounts[axis_move.depth as usize] += axis_move.amount;
            i += 1;
        }

        emit_axis_block(axis, &amounts, order, &mut output);
    }

    output
}

fn emit_axis_block(axis: u8, amounts: &[i32], order: u32, output: &mut Vec<TurnCommand>) {
    let mut depth = 0usize;
    while depth < amounts.len() {
        let amount = normalise_amount(amounts[depth]);
        if amount == 0 {
            depth += 1;
            continue;
        }

        let start = depth;
        depth += 1;
        while depth < amounts.len() && normalise_amount(amounts[depth]) == amount {
            depth += 1;
        }

        emit_axis_range(
            axis,
            start as u32,
            (depth - start) as u32,
            amount,
            order,
            output,
        );
    }
}

fn emit_axis_range(
    axis: u8,
    start_layer: u32,
    width: u32,
    amount: i32,
    order: u32,
    output: &mut Vec<TurnCommand>,
) {
    if width == 0 {
        return;
    }

    let face = match axis {
        0 => Face::Right,
        1 => Face::Up,
        2 => Face::Front,
        _ => unreachable!(),
    };
    let rotation = amount_to_rotation(amount);

    if start_layer == 0 && width == order {
        output.push(TurnCommand {
            face,
            start_layer: 0,
            width: order - 1,
            rotation,
        });
        output.push(TurnCommand {
            face,
            start_layer: order - 1,
            width: 1,
            rotation,
        });
        return;
    }

    output.push(TurnCommand {
        face,
        start_layer,
        width,
        rotation,
    });
}

fn turn_to_axis_moves(turn: TurnCommand, order: u32) -> Vec<AxisMove> {
    let r1 = order - 1;
    let q = rotation_to_amount(turn.rotation);
    let mut moves = Vec::with_capacity(turn.width as usize);

    let (axis, first_depth, amount) = match turn.face {
        Face::Right => (0, turn.start_layer, q),
        Face::Left => (0, r1 - (turn.start_layer + turn.width - 1), -q),
        Face::Up => (1, turn.start_layer, q),
        Face::Down => (1, r1 - (turn.start_layer + turn.width - 1), -q),
        Face::Front => (2, turn.start_layer, q),
        Face::Back => (2, r1 - (turn.start_layer + turn.width - 1), -q),
    };

    for offset in 0..turn.width {
        moves.push(AxisMove {
            axis,
            depth: first_depth + offset,
            amount,
        });
    }

    moves
}

fn rotation_to_amount(rotation: RotationAmount) -> i32 {
    match rotation {
        RotationAmount::Clockwise => 1,
        RotationAmount::HalfTurn => 2,
        RotationAmount::CounterClockwise => -1,
    }
}

fn amount_to_rotation(amount: i32) -> RotationAmount {
    match normalise_amount(amount) {
        1 => RotationAmount::Clockwise,
        2 => RotationAmount::HalfTurn,
        -1 => RotationAmount::CounterClockwise,
        _ => unreachable!("zero amounts are not emitted"),
    }
}

fn normalise_amount(amount: i32) -> i32 {
    match amount.rem_euclid(4) {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => -1,
        _ => unreachable!(),
    }
}

#[cfg(feature = "phase-search-4x4")]
fn turns_to_alg(turns: &[TurnCommand], order: u32) -> Result<Alg, NxnSolveError> {
    let mut tokens = Vec::new();
    for &turn in turns {
        tokens.extend(turn_to_alg_tokens(turn, order)?);
    }

    Alg::from_str(&tokens.join(" ")).map_err(|error| {
        NxnSolveError::InvalidState(format!(
            "failed to encode setup alg for phase search: {error}"
        ))
    })
}

#[cfg(feature = "phase-search-4x4")]
fn turn_to_alg_tokens(turn: TurnCommand, order: u32) -> Result<Vec<String>, NxnSolveError> {
    turn.validate_for(rubik_core::CubeOrder::new(order).map_err(|error| {
        NxnSolveError::InvalidOrder(format!("cannot encode turn for order {order}: {error}"))
    })?)
    .map_err(|error| {
        NxnSolveError::InvalidState(format!(
            "cannot encode invalid turn for phase search: {error}"
        ))
    })?;

    let mut tokens = Vec::with_capacity(turn.width as usize);
    for offset in 0..turn.width {
        let mut face = turn.face;
        let mut start_layer = turn.start_layer + offset;
        let mut rotation = turn.rotation;

        if start_layer > (order - 1) / 2 {
            face = opposite_face(face);
            start_layer = order - 1 - start_layer;
            rotation = rotation.inverse();
        }

        tokens.extend(single_slice_tokens(face, start_layer, rotation));
    }
    Ok(tokens)
}

#[cfg(feature = "phase-search-4x4")]
fn single_slice_tokens(face: Face, start_layer: u32, rotation: RotationAmount) -> Vec<String> {
    let face_name = face_name(face);
    if start_layer == 0 {
        return vec![format!("{face_name}{}", rotation_suffix(rotation))];
    }

    vec![
        format!("{face_name}w{}", rotation_suffix(rotation)),
        format!("{face_name}{}", rotation_suffix(rotation.inverse())),
    ]
}

#[cfg(feature = "phase-search-4x4")]
fn rotation_suffix(rotation: RotationAmount) -> &'static str {
    match rotation {
        RotationAmount::Clockwise => "",
        RotationAmount::HalfTurn => "2",
        RotationAmount::CounterClockwise => "'",
    }
}

#[cfg(feature = "phase-search-4x4")]
fn alg_to_turns(alg: &Alg, order: u32) -> Result<Vec<TurnCommand>, NxnSolveError> {
    let mut turns = Vec::with_capacity(alg.nodes.len());
    for node in &alg.nodes {
        let AlgNode::MoveNode(move_node) = node else {
            return Err(NxnSolveError::InvalidState(format!(
                "4x4 phase search returned unsupported alg node: {node}"
            )));
        };

        let mut family = move_node.quantum.family.as_str();
        let is_wide = family.ends_with('w');
        if is_wide {
            family = &family[..family.len() - 1];
        }

        let face = parse_face_name(family).ok_or_else(|| {
            NxnSolveError::InvalidState(format!(
                "4x4 phase search returned unsupported move family: {}",
                move_node.quantum.family
            ))
        })?;

        let prefix_layer = match &move_node.quantum.prefix {
            None => None,
            Some(MovePrefix::Layer(layer)) => Some(layer.layer),
            Some(MovePrefix::Range(_)) => {
                return Err(NxnSolveError::InvalidState(format!(
                    "4x4 phase search returned unsupported ranged move: {move_node}"
                )));
            }
        };

        let (start_layer, width) = if is_wide {
            (0, prefix_layer.unwrap_or(2))
        } else {
            (prefix_layer.unwrap_or(1) - 1, 1)
        };

        let rotation = amount_to_rotation(move_node.amount);
        let turn = TurnCommand {
            face,
            start_layer,
            width,
            rotation,
        };
        turn.validate_for(rubik_core::CubeOrder::new(order).map_err(|error| {
            NxnSolveError::InvalidOrder(format!("cannot decode turn for order {order}: {error}"))
        })?)
        .map_err(|error| {
            NxnSolveError::InvalidState(format!(
                "4x4 phase search returned invalid turn {turn:?}: {error}"
            ))
        })?;
        turns.push(turn);
    }
    Ok(turns)
}

#[cfg(feature = "phase-search-4x4")]
fn face_name(face: Face) -> &'static str {
    match face {
        Face::Up => "U",
        Face::Right => "R",
        Face::Front => "F",
        Face::Down => "D",
        Face::Left => "L",
        Face::Back => "B",
    }
}

#[cfg(feature = "phase-search-4x4")]
fn parse_face_name(face: &str) -> Option<Face> {
    match face {
        "U" => Some(Face::Up),
        "R" => Some(Face::Right),
        "F" => Some(Face::Front),
        "D" => Some(Face::Down),
        "L" => Some(Face::Left),
        "B" => Some(Face::Back),
        _ => None,
    }
}

#[cfg(feature = "phase-search-4x4")]
fn opposite_face(face: Face) -> Face {
    match face {
        Face::Up => Face::Down,
        Face::Right => Face::Left,
        Face::Front => Face::Back,
        Face::Down => Face::Up,
        Face::Left => Face::Right,
        Face::Back => Face::Front,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::{CubeEngine, CubeOrder, CubeState};

    fn apply_all(state: &mut CubeState, turns: &[TurnCommand]) {
        for &turn in turns {
            apply_turn_to_state(state, turn).expect("turn should apply");
        }
    }

    #[test]
    fn packs_adjacent_same_axis_slices_into_a_wide_turn() {
        let turns = vec![
            TurnCommand {
                face: Face::Right,
                start_layer: 0,
                width: 1,
                rotation: RotationAmount::Clockwise,
            },
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::Clockwise,
            },
        ];

        let optimized = optimize_turns(&turns, 4);

        assert_eq!(
            optimized,
            vec![TurnCommand {
                face: Face::Right,
                start_layer: 0,
                width: 2,
                rotation: RotationAmount::Clockwise,
            }]
        );
    }

    #[test]
    fn cancels_same_physical_slice_across_opposite_faces() {
        let turns = vec![
            TurnCommand {
                face: Face::Right,
                start_layer: 3,
                width: 1,
                rotation: RotationAmount::Clockwise,
            },
            TurnCommand {
                face: Face::Left,
                start_layer: 0,
                width: 1,
                rotation: RotationAmount::Clockwise,
            },
        ];

        assert!(optimize_turns(&turns, 4).is_empty());
    }

    #[test]
    fn compression_preserves_state_effect() {
        let order = CubeOrder::new(4).expect("valid order");
        let turns = vec![
            TurnCommand {
                face: Face::Right,
                start_layer: 0,
                width: 1,
                rotation: RotationAmount::Clockwise,
            },
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::Clockwise,
            },
            TurnCommand {
                face: Face::Up,
                start_layer: 2,
                width: 1,
                rotation: RotationAmount::HalfTurn,
            },
            TurnCommand {
                face: Face::Right,
                start_layer: 0,
                width: 2,
                rotation: RotationAmount::CounterClockwise,
            },
        ];

        let optimized = optimize_turns(&turns, order.get());
        let mut original_state = CubeState::solved(order);
        let mut optimized_state = CubeState::solved(order);
        apply_all(&mut original_state, &turns);
        apply_all(&mut optimized_state, &optimized);

        assert_eq!(optimized_state, original_state);
        assert!(optimized.len() < turns.len());
    }

    #[test]
    fn solves_seeded_4x4_scramble() {
        let order = CubeOrder::new(4).expect("valid order");
        let mut engine = CubeEngine::new(order);
        let scramble = engine
            .scramble_with_seed(32, 2026)
            .expect("scramble should apply");
        let mut state = CubeState::solved(order);
        apply_all(&mut state, &scramble);

        let solution = solve(&state, None).expect("solve should succeed");

        let mut verify = state.clone();
        apply_all(&mut verify, &solution);
        assert!(verify.is_solved());
    }

    #[test]
    fn shortens_seeded_4x4_constructive_solution() {
        let order = CubeOrder::new(4).expect("valid order");
        let mut engine = CubeEngine::new(order);
        let scramble = engine
            .scramble_with_seed(200, 1)
            .expect("scramble should apply");
        let mut state = CubeState::solved(order);
        apply_all(&mut state, &scramble);

        let seed = rcube_rs::solve(&state, None).expect("constructive seed should solve");
        let optimized = solve(&state, None).expect("optimized solve should succeed");

        println!(
            "4x4 seed=1 constructive={} optimized={}",
            seed.len(),
            optimized.len()
        );
        assert!(optimized.len() <= seed.len());
    }
}

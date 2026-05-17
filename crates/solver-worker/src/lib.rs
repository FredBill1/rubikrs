#![forbid(unsafe_code)]

use rubik_core::{
    CubeState, Face, RotationAmount, TurnCommand, apply_turn_to_state_unchecked,
};
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use std::sync::Once;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(target_arch = "wasm32")]
static PANIC_HOOK: Once = Once::new();
const ROTATIONS: [RotationAmount; 3] = [
    RotationAmount::Clockwise,
    RotationAmount::HalfTurn,
    RotationAmount::CounterClockwise,
];

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolveOutcomeKind {
    Solved,
    Unsolved,
    Error,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveTurn {
    pub face_code: u8,
    pub rotation_code: u8,
    pub start_layer: u8,
    pub width: u8,
    pub notation: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveResponse {
    pub kind: SolveOutcomeKind,
    pub turns: Vec<SolveTurn>,
    pub explored: u64,
    pub depth_limit: u8,
    pub message: String,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn solve_state_json(state_json: &str, max_depth: u8) -> String {
    install_panic_hook();
    serde_json::to_string(&solve_state(state_json, max_depth)).expect("solve response should serialize")
}

fn solve_state(state_json: &str, max_depth: u8) -> SolveResponse {
    let state = match CubeState::from_json(state_json) {
        Ok(state) => state,
        Err(error) => {
            return SolveResponse {
                kind: SolveOutcomeKind::Error,
                turns: Vec::new(),
                explored: 0,
                depth_limit: max_depth,
                message: error.to_string(),
            };
        }
    };

    let hard_cap = hard_depth_cap(state.order.get());
    if max_depth > hard_cap {
        return SolveResponse {
            kind: SolveOutcomeKind::Error,
            turns: Vec::new(),
            explored: 0,
            depth_limit: hard_cap,
            message: format!(
                "depth limit {max_depth} exceeds the current cap of {hard_cap} for {}x{} states",
                state.order.get(),
                state.order.get()
            ),
        };
    }

    let target = CubeState::solved(state.order);
    if state == target {
        return SolveResponse {
            kind: SolveOutcomeKind::Solved,
            turns: Vec::new(),
            explored: 0,
            depth_limit: max_depth,
            message: "state is already solved".to_string(),
        };
    }

    let mut working = state;
    let mut scratch = working.stickers.clone();
    let mut path = Vec::with_capacity(usize::from(max_depth));
    let mut explored = 0_u64;

    for depth in 1..=usize::from(max_depth) {
        if dfs_solve(
            &mut working,
            &target,
            &mut scratch,
            &mut path,
            depth,
            None,
            &mut explored,
        ) {
            let move_count = path.len();
            return SolveResponse {
                kind: SolveOutcomeKind::Solved,
                turns: path.iter().copied().map(encode_turn).collect(),
                explored,
                depth_limit: max_depth,
                message: format!(
                    "found a depth-{move_count} solution after exploring {explored} nodes"
                ),
            };
        }
    }

    SolveResponse {
        kind: SolveOutcomeKind::Unsolved,
        turns: Vec::new(),
        explored,
        depth_limit: max_depth,
        message: format!(
            "no solution found within depth {max_depth}; increase the cap in a future solver slice"
        ),
    }
}

fn dfs_solve(
    working: &mut CubeState,
    target: &CubeState,
    scratch: &mut [rubik_core::StickerColor],
    path: &mut Vec<TurnCommand>,
    remaining_depth: usize,
    last_face: Option<Face>,
    explored: &mut u64,
) -> bool {
    if working == target {
        return true;
    }

    if remaining_depth == 0 {
        return false;
    }

    for face in Face::ALL {
        if Some(face) == last_face {
            continue;
        }

        for rotation in ROTATIONS {
            let turn = TurnCommand::outer(face, rotation);
            apply_turn_to_state_unchecked(working, turn, scratch).expect("outer turns should validate");
            *explored += 1;
            path.push(turn);

            if dfs_solve(
                working,
                target,
                scratch,
                path,
                remaining_depth - 1,
                Some(face),
                explored,
            ) {
                return true;
            }

            path.pop();
            apply_turn_to_state_unchecked(working, turn.inverse(), scratch)
                .expect("inverse outer turns should validate");
        }
    }

    false
}

fn hard_depth_cap(order: u8) -> u8 {
    match order {
        2 => 8,
        3 => 7,
        4 | 5 => 5,
        _ => 4,
    }
}

fn encode_turn(turn: TurnCommand) -> SolveTurn {
    SolveTurn {
        face_code: encode_face(turn.face),
        rotation_code: encode_rotation(turn.rotation),
        start_layer: turn.start_layer,
        width: turn.width,
        notation: format_turn(turn),
    }
}

fn encode_face(face: Face) -> u8 {
    match face {
        Face::Up => 0,
        Face::Right => 1,
        Face::Front => 2,
        Face::Down => 3,
        Face::Left => 4,
        Face::Back => 5,
    }
}

fn encode_rotation(rotation: RotationAmount) -> u8 {
    match rotation {
        RotationAmount::Clockwise => 0,
        RotationAmount::HalfTurn => 1,
        RotationAmount::CounterClockwise => 2,
    }
}

fn format_turn(turn: TurnCommand) -> String {
    let face = match turn.face {
        Face::Up => "U",
        Face::Right => "R",
        Face::Front => "F",
        Face::Down => "D",
        Face::Left => "L",
        Face::Back => "B",
    };

    match turn.rotation {
        RotationAmount::Clockwise => face.to_string(),
        RotationAmount::HalfTurn => format!("{face}2"),
        RotationAmount::CounterClockwise => format!("{face}'"),
    }
}

fn install_panic_hook() {
    #[cfg(target_arch = "wasm32")]
    PANIC_HOOK.call_once(console_error_panic_hook::set_once);
}

#[cfg(test)]
mod tests {
    use rubik_core::{CubeOrder, Face, RotationAmount, TurnCommand, apply_turn_to_state};

    use super::{SolveOutcomeKind, SolveResponse, solve_state_json};

    #[test]
    fn reports_solved_when_state_is_already_solved() {
        let state = rubik_core::CubeState::solved(CubeOrder::standard());
        let response: SolveResponse =
            serde_json::from_str(&solve_state_json(&state.to_json().expect("state json"), 4))
                .expect("response json");

        assert!(matches!(response.kind, SolveOutcomeKind::Solved));
        assert!(response.turns.is_empty());
    }

    #[test]
    fn finds_the_inverse_of_a_single_turn_scramble() {
        let mut state = rubik_core::CubeState::solved(CubeOrder::standard());
        apply_turn_to_state(
            &mut state,
            TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
        )
        .expect("turn should be valid");

        let response: SolveResponse =
            serde_json::from_str(&solve_state_json(&state.to_json().expect("state json"), 1))
                .expect("response json");

        assert!(matches!(response.kind, SolveOutcomeKind::Solved));
        assert_eq!(response.turns.len(), 1);
        assert_eq!(response.turns[0].notation, "F'");
    }

    #[test]
    fn rejects_depth_limits_above_the_current_cap() {
        let state = rubik_core::CubeState::solved(CubeOrder::standard());
        let response: SolveResponse =
            serde_json::from_str(&solve_state_json(&state.to_json().expect("state json"), 9))
                .expect("response json");

        assert!(matches!(response.kind, SolveOutcomeKind::Error));
        assert!(response.message.contains("cap"));
    }
}

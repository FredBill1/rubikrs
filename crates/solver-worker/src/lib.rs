#![forbid(unsafe_code)]

use rubik_core::{CubeState, Face, RotationAmount, StickerColor, TurnCommand, apply_turn_to_state_unchecked};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveRequest {
    pub state_json: String,
    pub target_depth: u8,
    #[serde(default)]
    pub allowed_faces: Vec<u8>,
    #[serde(default)]
    pub turn_history_json: Option<String>,
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
    serde_json::to_string(&solve_iterative(state_json, max_depth))
        .expect("solve response should serialize")
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn solve_request_json(request_json: &str) -> String {
    install_panic_hook();
    serde_json::to_string(&solve_request_str(request_json)).expect("solve response should serialize")
}

fn solve_iterative(state_json: &str, max_depth: u8) -> SolveResponse {
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

    if state == CubeState::solved(state.order) {
        return SolveResponse {
            kind: SolveOutcomeKind::Solved,
            turns: Vec::new(),
            explored: 0,
            depth_limit: max_depth,
            message: "state is already solved".to_string(),
        };
    }

    let mut explored = 0_u64;

    for depth in 1..=max_depth {
        let response = solve_exact_request(&state, depth, &[]);
        explored = explored.saturating_add(response.explored);

        if matches!(response.kind, SolveOutcomeKind::Solved | SolveOutcomeKind::Error) {
            return SolveResponse {
                explored,
                depth_limit: response.depth_limit,
                ..response
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

fn solve_request_str(request_json: &str) -> SolveResponse {
    let request = match serde_json::from_str::<SolveRequest>(request_json) {
        Ok(request) => request,
        Err(error) => {
            return SolveResponse {
                kind: SolveOutcomeKind::Error,
                turns: Vec::new(),
                explored: 0,
                depth_limit: 0,
                message: error.to_string(),
            };
        }
    };

    let state = match CubeState::from_json(&request.state_json) {
        Ok(state) => state,
        Err(error) => {
            return SolveResponse {
                kind: SolveOutcomeKind::Error,
                turns: Vec::new(),
                explored: 0,
                depth_limit: request.target_depth,
                message: error.to_string(),
            };
        }
    };

    if let Some(response) = solve_with_recorded_history(
        &state,
        request.target_depth,
        request.turn_history_json.as_deref(),
    ) {
        return response;
    }

    solve_exact_request(&state, request.target_depth, &request.allowed_faces)
}

fn solve_with_recorded_history(
    state: &CubeState,
    target_depth: u8,
    turn_history_json: Option<&str>,
) -> Option<SolveResponse> {
    let Some(turn_history_json) = turn_history_json else {
        return None;
    };
    let Ok(turn_history) = serde_json::from_str::<Vec<TurnCommand>>(turn_history_json) else {
        return None;
    };
    if turn_history.is_empty() {
        return None;
    }

    let replay_turns = turn_history
        .iter()
        .rev()
        .copied()
        .map(TurnCommand::inverse)
        .collect::<Vec<_>>();
    let mut replayed = state.clone();
    let mut scratch = replayed.stickers.clone();
    for turn in &replay_turns {
        apply_turn_to_state_unchecked(&mut replayed, *turn, &mut scratch).ok()?;
    }

    if replayed != CubeState::solved(state.order) {
        return None;
    }

    Some(SolveResponse {
        kind: SolveOutcomeKind::Solved,
        turns: replay_turns.iter().copied().map(encode_turn).collect(),
        explored: 0,
        depth_limit: target_depth,
        message: format!(
            "replayed the inverse of {} recorded turn(s) to solve this {}x{} state",
            replay_turns.len(),
            state.order.get(),
            state.order.get()
        ),
    })
}

fn solve_exact_request(state: &CubeState, target_depth: u8, allowed_faces: &[u8]) -> SolveResponse {
    let hard_cap = hard_depth_cap(state.order.get());
    if target_depth > hard_cap {
        return SolveResponse {
            kind: SolveOutcomeKind::Error,
            turns: Vec::new(),
            explored: 0,
            depth_limit: hard_cap,
            message: format!(
                "depth limit {target_depth} exceeds the current cap of {hard_cap} for {}x{} states",
                state.order.get(),
                state.order.get()
            ),
        };
    }

    let root_faces = match decode_allowed_faces(allowed_faces) {
        Ok(faces) => faces,
        Err(error) => {
            return SolveResponse {
                kind: SolveOutcomeKind::Error,
                turns: Vec::new(),
                explored: 0,
                depth_limit: target_depth,
                message: error,
            };
        }
    };

    let target = CubeState::solved(state.order);
    if state == &target {
        return SolveResponse {
            kind: SolveOutcomeKind::Solved,
            turns: Vec::new(),
            explored: 0,
            depth_limit: target_depth,
            message: "state is already solved".to_string(),
        };
    }

    if target_depth == 0 {
        return SolveResponse {
            kind: SolveOutcomeKind::Unsolved,
            turns: Vec::new(),
            explored: 0,
            depth_limit: 0,
            message: "state is not solved at depth 0".to_string(),
        };
    }

    let mut working = state.clone();
    let mut scratch = working.stickers.clone();
    let mut path = Vec::with_capacity(usize::from(target_depth));
    let mut explored = 0_u64;

    if search_root_faces(
        &mut working,
        &target,
        &mut scratch,
        &mut path,
        usize::from(target_depth),
        &root_faces,
        &mut explored,
    ) {
        let move_count = path.len();
        return SolveResponse {
            kind: SolveOutcomeKind::Solved,
            turns: path.iter().copied().map(encode_turn).collect(),
            explored,
            depth_limit: target_depth,
            message: format!(
                "found a depth-{move_count} solution after exploring {explored} nodes"
            ),
        };
    }

    SolveResponse {
        kind: SolveOutcomeKind::Unsolved,
        turns: Vec::new(),
        explored,
        depth_limit: target_depth,
        message: format!(
            "no solution found at exact depth {target_depth} in this worker lane"
        ),
    }
}

fn search_root_faces(
    working: &mut CubeState,
    target: &CubeState,
    scratch: &mut [StickerColor],
    path: &mut Vec<TurnCommand>,
    target_depth: usize,
    root_faces: &[Face],
    explored: &mut u64,
) -> bool {
    for &face in root_faces {
        for rotation in ROTATIONS {
            let turn = TurnCommand::outer(face, rotation);
            apply_turn_to_state_unchecked(working, turn, scratch).expect("outer turns should validate");
            *explored += 1;
            path.push(turn);

            if dfs_exact(
                working,
                target,
                scratch,
                path,
                target_depth.saturating_sub(1),
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

fn dfs_exact(
    working: &mut CubeState,
    target: &CubeState,
    scratch: &mut [StickerColor],
    path: &mut Vec<TurnCommand>,
    remaining_depth: usize,
    last_face: Option<Face>,
    explored: &mut u64,
) -> bool {
    if remaining_depth == 0 {
        return working == target;
    }

    if working == target {
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

            if dfs_exact(
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

fn decode_allowed_faces(face_codes: &[u8]) -> Result<Vec<Face>, String> {
    if face_codes.is_empty() {
        return Ok(Face::ALL.to_vec());
    }

    let mut faces = Vec::with_capacity(face_codes.len());
    for &face_code in face_codes {
        let Some(face) = decode_face(face_code) else {
            return Err(format!("unknown face code {face_code} in worker request"));
        };

        if !faces.contains(&face) {
            faces.push(face);
        }
    }

    Ok(faces)
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

fn decode_face(face_code: u8) -> Option<Face> {
    match face_code {
        0 => Some(Face::Up),
        1 => Some(Face::Right),
        2 => Some(Face::Front),
        3 => Some(Face::Down),
        4 => Some(Face::Left),
        5 => Some(Face::Back),
        _ => None,
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
    use super::{SolveOutcomeKind, SolveRequest, SolveResponse, solve_request_json, solve_state_json};
    use rubik_core::{Face, RotationAmount, TurnCommand, apply_turn_to_state};

    #[test]
    fn reports_solved_when_state_is_already_solved() {
        let state = rubik_core::CubeState::solved(rubik_core::CubeOrder::standard());
        let response: SolveResponse =
            serde_json::from_str(&solve_state_json(&state.to_json().expect("state json"), 4))
                .expect("response json");

        assert!(matches!(response.kind, SolveOutcomeKind::Solved));
        assert!(response.turns.is_empty());
    }

    #[test]
    fn finds_the_inverse_of_a_single_turn_scramble() {
        let mut state = rubik_core::CubeState::solved(rubik_core::CubeOrder::standard());
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
    fn exact_depth_lane_can_solve_when_root_face_matches() {
        let mut state = rubik_core::CubeState::solved(rubik_core::CubeOrder::standard());
        apply_turn_to_state(
            &mut state,
            TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
        )
        .expect("turn should be valid");

        let request = SolveRequest {
            state_json: state.to_json().expect("state json"),
            target_depth: 1,
            allowed_faces: vec![2],
            turn_history_json: None,
        };
        let response: SolveResponse =
            serde_json::from_str(&solve_request_json(&serde_json::to_string(&request).unwrap()))
                .expect("response json");

        assert!(matches!(response.kind, SolveOutcomeKind::Solved));
        assert_eq!(response.turns[0].notation, "F'");
    }

    #[test]
    fn exact_depth_lane_rejects_wrong_root_face_partition() {
        let mut state = rubik_core::CubeState::solved(rubik_core::CubeOrder::standard());
        apply_turn_to_state(
            &mut state,
            TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
        )
        .expect("turn should be valid");

        let request = SolveRequest {
            state_json: state.to_json().expect("state json"),
            target_depth: 1,
            allowed_faces: vec![0],
            turn_history_json: None,
        };
        let response: SolveResponse =
            serde_json::from_str(&solve_request_json(&serde_json::to_string(&request).unwrap()))
                .expect("response json");

        assert!(matches!(response.kind, SolveOutcomeKind::Unsolved));
    }

    #[test]
    fn recorded_history_can_solve_larger_orders_without_searching() {
        let order = rubik_core::CubeOrder::new(4).expect("supported order");
        let mut state = rubik_core::CubeState::solved(order);
        let turn_history = vec![
            TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Up, RotationAmount::HalfTurn),
            TurnCommand {
                face: Face::Right,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::CounterClockwise,
            },
        ];

        for turn in &turn_history {
            apply_turn_to_state(&mut state, *turn).expect("turn should be valid");
        }

        let request = SolveRequest {
            state_json: state.to_json().expect("state json"),
            target_depth: 1,
            allowed_faces: vec![0, 1, 2, 3, 4, 5],
            turn_history_json: Some(serde_json::to_string(&turn_history).expect("history json")),
        };
        let response: SolveResponse =
            serde_json::from_str(&solve_request_json(&serde_json::to_string(&request).unwrap()))
                .expect("response json");

        assert!(matches!(response.kind, SolveOutcomeKind::Solved));
        assert_eq!(response.turns.len(), turn_history.len());
        assert_eq!(response.turns[0].face_code, 1);
        assert_eq!(response.turns[0].start_layer, 1);
        assert_eq!(response.turns[0].rotation_code, 0);
        assert_eq!(response.turns[1].notation, "U2");
        assert_eq!(response.turns[2].notation, "F'");
    }

    #[test]
    fn rejects_depth_limits_above_the_current_cap() {
        let state = rubik_core::CubeState::solved(rubik_core::CubeOrder::standard());
        let response: SolveResponse =
            serde_json::from_str(&solve_state_json(&state.to_json().expect("state json"), 9))
                .expect("response json");

        assert!(matches!(response.kind, SolveOutcomeKind::Error));
        assert!(response.message.contains("cap"));
    }
}

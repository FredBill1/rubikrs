#![forbid(unsafe_code)]

use std::{
    collections::{HashMap, VecDeque},
    sync::OnceLock,
};

use kewb::{
    CubieCube as KewbCubieCube, DataTable as KewbDataTable, FaceCube as KewbFaceCube,
    Move as KewbMove, Solver as KewbSolver,
};
use rubik_core::{
    CubeOrder, CubeState, Face, RotationAmount, StickerColor, TurnCommand,
    apply_turn_to_state_unchecked,
};
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "wasm32")]
use std::sync::Once;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

#[cfg(target_arch = "wasm32")]
static PANIC_HOOK: Once = Once::new();

static KEWB_DATA_TABLE: OnceLock<KewbDataTable> = OnceLock::new();
static ORIENTATION_TURN_SEQUENCES: OnceLock<HashMap<OrientationKey, Vec<TurnCommand>>> =
    OnceLock::new();

const ROTATIONS: [RotationAmount; 3] = [
    RotationAmount::Clockwise,
    RotationAmount::HalfTurn,
    RotationAmount::CounterClockwise,
];
const RIGHT_MIDDLE_CLOCKWISE: TurnCommand = TurnCommand {
    face: Face::Right,
    start_layer: 1,
    width: 1,
    rotation: RotationAmount::Clockwise,
};
const RIGHT_MIDDLE_COUNTER_CLOCKWISE: TurnCommand = TurnCommand {
    rotation: RotationAmount::CounterClockwise,
    ..RIGHT_MIDDLE_CLOCKWISE
};
const UP_MIDDLE_CLOCKWISE: TurnCommand = TurnCommand {
    face: Face::Up,
    start_layer: 1,
    width: 1,
    rotation: RotationAmount::Clockwise,
};
const UP_MIDDLE_COUNTER_CLOCKWISE: TurnCommand = TurnCommand {
    rotation: RotationAmount::CounterClockwise,
    ..UP_MIDDLE_CLOCKWISE
};
const FRONT_MIDDLE_CLOCKWISE: TurnCommand = TurnCommand {
    face: Face::Front,
    start_layer: 1,
    width: 1,
    rotation: RotationAmount::Clockwise,
};
const FRONT_MIDDLE_COUNTER_CLOCKWISE: TurnCommand = TurnCommand {
    rotation: RotationAmount::CounterClockwise,
    ..FRONT_MIDDLE_CLOCKWISE
};
const ROTATE_X: [TurnCommand; 3] = [
    TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
    RIGHT_MIDDLE_CLOCKWISE,
    TurnCommand::outer(Face::Left, RotationAmount::CounterClockwise),
];
const ROTATE_X_PRIME: [TurnCommand; 3] = [
    TurnCommand::outer(Face::Right, RotationAmount::CounterClockwise),
    RIGHT_MIDDLE_COUNTER_CLOCKWISE,
    TurnCommand::outer(Face::Left, RotationAmount::Clockwise),
];
const ROTATE_Y: [TurnCommand; 3] = [
    TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
    UP_MIDDLE_CLOCKWISE,
    TurnCommand::outer(Face::Down, RotationAmount::CounterClockwise),
];
const ROTATE_Y_PRIME: [TurnCommand; 3] = [
    TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
    UP_MIDDLE_COUNTER_CLOCKWISE,
    TurnCommand::outer(Face::Down, RotationAmount::Clockwise),
];
const ROTATE_Z: [TurnCommand; 3] = [
    TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
    FRONT_MIDDLE_CLOCKWISE,
    TurnCommand::outer(Face::Back, RotationAmount::CounterClockwise),
];
const ROTATE_Z_PRIME: [TurnCommand; 3] = [
    TurnCommand::outer(Face::Front, RotationAmount::CounterClockwise),
    FRONT_MIDDLE_COUNTER_CLOCKWISE,
    TurnCommand::outer(Face::Back, RotationAmount::Clockwise),
];
const ORIENTATION_GENERATORS: [&[TurnCommand]; 6] = [
    &ROTATE_X,
    &ROTATE_X_PRIME,
    &ROTATE_Y,
    &ROTATE_Y_PRIME,
    &ROTATE_Z,
    &ROTATE_Z_PRIME,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AxisVector {
    x: i8,
    y: i8,
    z: i8,
}

impl AxisVector {
    const fn new(x: i8, y: i8, z: i8) -> Self {
        Self { x, y, z }
    }

    const fn dot(self, other: Self) -> i8 {
        (self.x * other.x) + (self.y * other.y) + (self.z * other.z)
    }

    const fn cross(self, other: Self) -> Self {
        Self {
            x: (self.y * other.z) - (self.z * other.y),
            y: (self.z * other.x) - (self.x * other.z),
            z: (self.x * other.y) - (self.y * other.x),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct OrientationKey([Face; 6]);

#[derive(Debug, Clone)]
struct ThreeByThreeFrame {
    canonical_to_actual: [Face; 6],
    actual_to_canonical: [Face; 6],
    actual_x: AxisVector,
    actual_y: AxisVector,
    actual_z: AxisVector,
}

impl ThreeByThreeFrame {
    fn for_state(state: &CubeState) -> Option<Self> {
        if state.order.get() != 3 || state.stickers.len() != 54 {
            return None;
        }

        let mut canonical_to_actual = [Face::Up; 6];
        let mut actual_to_canonical = [Face::Up; 6];
        let mut seen_canonical_faces = [false; 6];

        for actual_face in Face::ALL {
            let canonical_face =
                face_for_color(*state.stickers.get(face_center_index(actual_face))?);
            let canonical_index = face_index(canonical_face);
            if seen_canonical_faces[canonical_index] {
                return None;
            }

            seen_canonical_faces[canonical_index] = true;
            canonical_to_actual[canonical_index] = actual_face;
            actual_to_canonical[face_index(actual_face)] = canonical_face;
        }

        let actual_x = face_vector(canonical_to_actual[face_index(Face::Right)]);
        let actual_y = face_vector(canonical_to_actual[face_index(Face::Up)]);
        let actual_z = face_vector(canonical_to_actual[face_index(Face::Front)]);
        if actual_x.dot(actual_y) != 0
            || actual_x.dot(actual_z) != 0
            || actual_y.dot(actual_z) != 0
            || actual_x.cross(actual_y) != actual_z
        {
            return None;
        }

        Some(Self {
            canonical_to_actual,
            actual_to_canonical,
            actual_x,
            actual_y,
            actual_z,
        })
    }

    fn orientation_key(&self) -> OrientationKey {
        OrientationKey(self.canonical_to_actual)
    }

    fn normalize_state(&self, state: &CubeState) -> CubeState {
        let mut normalized = CubeState::solved(state.order);

        for face in Face::ALL {
            for row in 0..3 {
                for col in 0..3 {
                    let source_index = sticker_index(face, row, col);
                    let cubie = sticker_cubie_vector(face, row, col);
                    let canonical_face = face_from_vector(
                        self.actual_vector_to_canonical(face_vector(face)),
                    )
                    .expect("3x3 frame should map every face normal back into the canonical basis");
                    let canonical_cubie = self.actual_vector_to_canonical(cubie);
                    let (canonical_row, canonical_col) =
                        row_col_from_cubie(canonical_face, canonical_cubie);
                    let target_index = sticker_index(canonical_face, canonical_row, canonical_col);
                    normalized.stickers[target_index] = state.stickers[source_index];
                }
            }
        }

        normalized
    }

    fn normalize_allowed_faces(&self, allowed_faces: &[u8]) -> Result<Vec<u8>, String> {
        allowed_faces
            .iter()
            .map(|&face_code| {
                let Some(actual_face) = decode_face(face_code) else {
                    return Err(format!("unknown face code {face_code} in worker request"));
                };

                Ok(encode_face(
                    self.actual_to_canonical[face_index(actual_face)],
                ))
            })
            .collect()
    }

    fn remap_solution(&self, response: SolveResponse) -> SolveResponse {
        let SolveResponse {
            kind,
            turns,
            explored,
            depth_limit,
            message,
        } = response;
        if !matches!(kind, SolveOutcomeKind::Solved) {
            return SolveResponse {
                kind,
                turns,
                explored,
                depth_limit,
                message,
            };
        }

        let Some(mut remapped_turns) = turns
            .iter()
            .map(solve_turn_to_command)
            .collect::<Option<Vec<_>>>()
            .map(|turns| {
                turns
                    .into_iter()
                    .map(|turn| TurnCommand {
                        face: self.canonical_to_actual[face_index(turn.face)],
                        ..turn
                    })
                    .collect::<Vec<_>>()
            })
        else {
            return SolveResponse {
                kind: SolveOutcomeKind::Error,
                turns: Vec::new(),
                explored,
                depth_limit,
                message: "solver worker returned an undecodable 3x3 turn".to_string(),
            };
        };

        let orientation_correction = self.orientation_correction_turns();
        let correction_len = orientation_correction.len();
        remapped_turns.extend(orientation_correction);

        let mut message = message;
        if correction_len > 0 {
            message.push_str(&format!(
                "; appended {correction_len} center-frame alignment turn(s)"
            ));
        }

        SolveResponse {
            kind,
            message,
            turns: remapped_turns.into_iter().map(encode_turn).collect(),
            explored,
            depth_limit,
        }
    }

    fn orientation_correction_turns(&self) -> Vec<TurnCommand> {
        orientation_turn_sequences()
            .get(&self.orientation_key())
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .rev()
            .map(TurnCommand::inverse)
            .collect()
    }

    fn actual_vector_to_canonical(&self, actual: AxisVector) -> AxisVector {
        AxisVector::new(
            actual.dot(self.actual_x),
            actual.dot(self.actual_y),
            actual.dot(self.actual_z),
        )
    }
}

fn orientation_turn_sequences() -> &'static HashMap<OrientationKey, Vec<TurnCommand>> {
    ORIENTATION_TURN_SEQUENCES.get_or_init(|| {
        let solved = CubeState::solved(CubeOrder::standard());
        let mut sequences = HashMap::new();
        let mut queue = VecDeque::new();

        let identity_key = ThreeByThreeFrame::for_state(&solved)
            .expect("canonical solved 3x3 should always define a valid center frame")
            .orientation_key();
        sequences.insert(identity_key, Vec::new());
        queue.push_back((solved.clone(), Vec::new()));

        while let Some((state, path)) = queue.pop_front() {
            for generator in ORIENTATION_GENERATORS {
                let mut next_state = state.clone();
                let mut scratch = next_state.stickers.clone();
                for &turn in generator.iter() {
                    apply_turn_to_state_unchecked(&mut next_state, turn, &mut scratch)
                        .expect("whole-cube rotation generators should always validate on 3x3");
                }

                let frame = ThreeByThreeFrame::for_state(&next_state)
                    .expect("whole-cube rotation generators should keep the 3x3 center frame valid");
                assert_eq!(
                    frame.normalize_state(&next_state),
                    solved,
                    "whole-cube rotation generators should only rotate the solved 3x3 frame",
                );

                let key = frame.orientation_key();
                if sequences.contains_key(&key) {
                    continue;
                }

                let mut next_path = path.clone();
                next_path.extend_from_slice(generator);
                sequences.insert(key, next_path.clone());
                queue.push_back((next_state, next_path));
            }
        }

        assert_eq!(
            sequences.len(),
            24,
            "whole-cube rotation generators should enumerate all 24 possible 3x3 center orientations",
        );
        sequences
    })
}

fn solve_turn_to_command(turn: &SolveTurn) -> Option<TurnCommand> {
    Some(TurnCommand {
        face: decode_face(turn.face_code)?,
        start_layer: turn.start_layer,
        width: turn.width,
        rotation: decode_rotation(turn.rotation_code)?,
    })
}

fn face_index(face: Face) -> usize {
    match face {
        Face::Up => 0,
        Face::Right => 1,
        Face::Front => 2,
        Face::Down => 3,
        Face::Left => 4,
        Face::Back => 5,
    }
}

fn face_center_index(face: Face) -> usize {
    (face_index(face) * 9) + 4
}

fn face_vector(face: Face) -> AxisVector {
    match face {
        Face::Up => AxisVector::new(0, 1, 0),
        Face::Right => AxisVector::new(1, 0, 0),
        Face::Front => AxisVector::new(0, 0, 1),
        Face::Down => AxisVector::new(0, -1, 0),
        Face::Left => AxisVector::new(-1, 0, 0),
        Face::Back => AxisVector::new(0, 0, -1),
    }
}

fn face_from_vector(vector: AxisVector) -> Option<Face> {
    match vector {
        AxisVector { x: 0, y: 1, z: 0 } => Some(Face::Up),
        AxisVector { x: 1, y: 0, z: 0 } => Some(Face::Right),
        AxisVector { x: 0, y: 0, z: 1 } => Some(Face::Front),
        AxisVector { x: 0, y: -1, z: 0 } => Some(Face::Down),
        AxisVector { x: -1, y: 0, z: 0 } => Some(Face::Left),
        AxisVector { x: 0, y: 0, z: -1 } => Some(Face::Back),
        _ => None,
    }
}

fn face_for_color(color: StickerColor) -> Face {
    match color {
        StickerColor::White => Face::Up,
        StickerColor::Red => Face::Right,
        StickerColor::Green => Face::Front,
        StickerColor::Yellow => Face::Down,
        StickerColor::Orange => Face::Left,
        StickerColor::Blue => Face::Back,
    }
}

fn sticker_index(face: Face, row: usize, col: usize) -> usize {
    (face_index(face) * 9) + (row * 3) + col
}

fn sticker_cubie_vector(face: Face, row: usize, col: usize) -> AxisVector {
    let (x, y, z) = match face {
        Face::Up => (col as i8 - 1, 1, row as i8 - 1),
        Face::Right => (1, 1 - row as i8, 1 - col as i8),
        Face::Front => (col as i8 - 1, 1 - row as i8, 1),
        Face::Down => (col as i8 - 1, -1, 1 - row as i8),
        Face::Left => (-1, 1 - row as i8, col as i8 - 1),
        Face::Back => (1 - col as i8, 1 - row as i8, -1),
    };

    AxisVector::new(x, y, z)
}

fn row_col_from_cubie(face: Face, cubie: AxisVector) -> (usize, usize) {
    let x = usize::try_from(cubie.x + 1).expect("3x3 cubie x coordinate should stay within -1..=1");
    let y = usize::try_from(cubie.y + 1).expect("3x3 cubie y coordinate should stay within -1..=1");
    let z = usize::try_from(cubie.z + 1).expect("3x3 cubie z coordinate should stay within -1..=1");

    match face {
        Face::Up => (z, x),
        Face::Right => (2 - y, 2 - z),
        Face::Front => (2 - y, x),
        Face::Down => (2 - z, x),
        Face::Left => (2 - y, z),
        Face::Back => (2 - y, 2 - x),
    }
}

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
    serde_json::to_string(&solve_request_str(request_json))
        .expect("solve response should serialize")
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

    let three_by_three_frame = if state.order.get() == 3 {
        match ThreeByThreeFrame::for_state(&state) {
            Some(frame) => Some(frame),
            None => {
                return SolveResponse {
                    kind: SolveOutcomeKind::Error,
                    turns: Vec::new(),
                    explored: 0,
                    depth_limit: max_depth,
                    message:
                        "3x3 centers no longer form a valid cube orientation for the solver frame"
                            .to_string(),
                };
            }
        }
    } else {
        None
    };
    let normalized_state = three_by_three_frame
        .as_ref()
        .map(|frame| frame.normalize_state(&state));
    let search_state = normalized_state.as_ref().unwrap_or(&state);
    let mut explored = 0_u64;

    for depth in 1..=max_depth {
        let response = if let Some(frame) = &three_by_three_frame {
            frame.remap_solution(solve_exact_request(search_state, depth, &[]))
        } else {
            solve_exact_request(search_state, depth, &[])
        };
        explored = explored.saturating_add(response.explored);

        if matches!(
            response.kind,
            SolveOutcomeKind::Solved | SolveOutcomeKind::Error
        ) {
            return SolveResponse {
                explored,
                depth_limit: response.depth_limit,
                ..response
            };
        }
    }

    if state.order.get() == 3 {
        if let Some(mut response) = solve_three_phase_fallback(search_state) {
            if let Some(frame) = &three_by_three_frame {
                response = frame.remap_solution(response);
            }
            response.explored = explored.saturating_add(response.explored);
            return response;
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

    let three_by_three_frame = if state.order.get() == 3 {
        match ThreeByThreeFrame::for_state(&state) {
            Some(frame) => Some(frame),
            None => {
                return SolveResponse {
                    kind: SolveOutcomeKind::Error,
                    turns: Vec::new(),
                    explored: 0,
                    depth_limit: request.target_depth,
                    message:
                        "3x3 centers no longer form a valid cube orientation for the solver frame"
                            .to_string(),
                };
            }
        }
    } else {
        None
    };
    let normalized_state = three_by_three_frame
        .as_ref()
        .map(|frame| frame.normalize_state(&state));
    let search_state = normalized_state.as_ref().unwrap_or(&state);

    if request.target_depth == 0 && state.order.get() == 3 {
        return solve_three_phase_fallback(search_state)
            .map(|response| match &three_by_three_frame {
                Some(frame) => frame.remap_solution(response),
                None => response,
            })
            .unwrap_or(SolveResponse {
                kind: SolveOutcomeKind::Unsolved,
                turns: Vec::new(),
                explored: 0,
                depth_limit: 0,
                message: "two-phase fallback could not solve the current 3x3 state".to_string(),
            });
    }

    if let Some(response) = solve_with_recorded_history(
        &state,
        request.target_depth,
        request.turn_history_json.as_deref(),
    ) {
        return response;
    }

    if let Some(frame) = &three_by_three_frame {
        let normalized_allowed_faces = match frame.normalize_allowed_faces(&request.allowed_faces) {
            Ok(face_codes) => face_codes,
            Err(message) => {
                return SolveResponse {
                    kind: SolveOutcomeKind::Error,
                    turns: Vec::new(),
                    explored: 0,
                    depth_limit: request.target_depth,
                    message,
                };
            }
        };
        return frame.remap_solution(solve_exact_request(
            search_state,
            request.target_depth,
            &normalized_allowed_faces,
        ));
    }

    solve_exact_request(search_state, request.target_depth, &request.allowed_faces)
}

fn solve_three_phase_fallback(state: &CubeState) -> Option<SolveResponse> {
    if state.order.get() != 3 {
        return None;
    }

    let facelet_string = cube_state_to_facelet_string(state)?;
    let face_cube = KewbFaceCube::try_from(facelet_string.as_str()).ok()?;
    let cubie = KewbCubieCube::try_from(&face_cube).ok()?;
    let mut solver = KewbSolver::new(kewb_data_table(), 23, None);
    let solution = solver.solve(cubie)?;
    let moves = solution.get_all_moves();

    Some(SolveResponse {
        kind: SolveOutcomeKind::Solved,
        turns: moves
            .iter()
            .copied()
            .filter_map(kewb_move_to_turn)
            .map(encode_turn)
            .collect(),
        explored: 0,
        depth_limit: 23,
        message: format!(
            "two-phase fallback found a {}-move 3x3 solution",
            moves.len()
        ),
    })
}

fn kewb_data_table() -> &'static KewbDataTable {
    KEWB_DATA_TABLE.get_or_init(KewbDataTable::default)
}

fn cube_state_to_facelet_string(state: &CubeState) -> Option<String> {
    if state.order.get() != 3 || state.stickers.len() != 54 {
        return None;
    }

    let mut output = String::with_capacity(54);
    for color in &state.stickers {
        output.push(match color {
            StickerColor::White => 'U',
            StickerColor::Red => 'R',
            StickerColor::Green => 'F',
            StickerColor::Yellow => 'D',
            StickerColor::Orange => 'L',
            StickerColor::Blue => 'B',
        });
    }

    Some(output)
}

fn kewb_move_to_turn(m: KewbMove) -> Option<TurnCommand> {
    let turn = match m {
        KewbMove::U => TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
        KewbMove::U2 => TurnCommand::outer(Face::Up, RotationAmount::HalfTurn),
        KewbMove::U3 => TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
        KewbMove::D => TurnCommand::outer(Face::Down, RotationAmount::Clockwise),
        KewbMove::D2 => TurnCommand::outer(Face::Down, RotationAmount::HalfTurn),
        KewbMove::D3 => TurnCommand::outer(Face::Down, RotationAmount::CounterClockwise),
        KewbMove::R => TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
        KewbMove::R2 => TurnCommand::outer(Face::Right, RotationAmount::HalfTurn),
        KewbMove::R3 => TurnCommand::outer(Face::Right, RotationAmount::CounterClockwise),
        KewbMove::L => TurnCommand::outer(Face::Left, RotationAmount::Clockwise),
        KewbMove::L2 => TurnCommand::outer(Face::Left, RotationAmount::HalfTurn),
        KewbMove::L3 => TurnCommand::outer(Face::Left, RotationAmount::CounterClockwise),
        KewbMove::F => TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
        KewbMove::F2 => TurnCommand::outer(Face::Front, RotationAmount::HalfTurn),
        KewbMove::F3 => TurnCommand::outer(Face::Front, RotationAmount::CounterClockwise),
        KewbMove::B => TurnCommand::outer(Face::Back, RotationAmount::Clockwise),
        KewbMove::B2 => TurnCommand::outer(Face::Back, RotationAmount::HalfTurn),
        KewbMove::B3 => TurnCommand::outer(Face::Back, RotationAmount::CounterClockwise),
    };

    Some(turn)
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
        message: format!("no solution found at exact depth {target_depth} in this worker lane"),
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
            apply_turn_to_state_unchecked(working, turn, scratch)
                .expect("outer turns should validate");
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
            apply_turn_to_state_unchecked(working, turn, scratch)
                .expect("outer turns should validate");
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

fn decode_rotation(rotation_code: u8) -> Option<RotationAmount> {
    match rotation_code {
        0 => Some(RotationAmount::Clockwise),
        1 => Some(RotationAmount::HalfTurn),
        2 => Some(RotationAmount::CounterClockwise),
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

    let width = if turn.width > 1 {
        format!("{}w", turn.width)
    } else {
        String::new()
    };

    let inner = if turn.start_layer > 0 {
        format!("[{}]", turn.start_layer + 1)
    } else {
        String::new()
    };

    let rotation = match turn.rotation {
        RotationAmount::Clockwise => "",
        RotationAmount::HalfTurn => "2",
        RotationAmount::CounterClockwise => "'",
    };

    format!("{width}{face}{inner}{rotation}")
}

fn install_panic_hook() {
    #[cfg(target_arch = "wasm32")]
    PANIC_HOOK.call_once(console_error_panic_hook::set_once);
}

#[cfg(test)]
mod tests {
    use super::{
        KewbCubieCube, KewbFaceCube, KewbMove, ROTATE_X, ROTATIONS, SolveOutcomeKind, SolveRequest,
        SolveResponse, ThreeByThreeFrame, cube_state_to_facelet_string, kewb_move_to_turn,
        solve_request_json, solve_request_str, solve_state_json, solve_turn_to_command,
    };
    use rubik_core::{
        CubeOrder, CubeState, Face, RotationAmount, TurnCommand, apply_turn_to_state,
    };

    fn apply_solution_turns(state: &mut CubeState, response: &SolveResponse) {
        for turn in &response.turns {
            apply_turn_to_state(
                state,
                solve_turn_to_command(turn)
                    .expect("solver turn should decode back into a turn command"),
            )
            .expect("solver turn should apply cleanly");
        }
    }

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
        let response: SolveResponse = serde_json::from_str(&solve_request_json(
            &serde_json::to_string(&request).unwrap(),
        ))
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
        let response: SolveResponse = serde_json::from_str(&solve_request_json(
            &serde_json::to_string(&request).unwrap(),
        ))
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
        let response: SolveResponse = serde_json::from_str(&solve_request_json(
            &serde_json::to_string(&request).unwrap(),
        ))
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
    fn target_depth_zero_uses_two_phase_fallback_for_3x3() {
        let mut state = rubik_core::CubeState::solved(rubik_core::CubeOrder::standard());
        for turn in [
            TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Right, RotationAmount::HalfTurn),
            TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
            TurnCommand::outer(Face::Left, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Down, RotationAmount::HalfTurn),
            TurnCommand::outer(Face::Back, RotationAmount::CounterClockwise),
            TurnCommand::outer(Face::Front, RotationAmount::HalfTurn),
            TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
        ] {
            apply_turn_to_state(&mut state, turn).expect("turn should be valid");
        }

        let request = SolveRequest {
            state_json: state.to_json().expect("state json"),
            target_depth: 0,
            allowed_faces: vec![],
            turn_history_json: None,
        };
        let response: SolveResponse = serde_json::from_str(&solve_request_json(
            &serde_json::to_string(&request).unwrap(),
        ))
        .expect("response json");

        assert!(matches!(response.kind, SolveOutcomeKind::Solved));
        assert!(!response.turns.is_empty());
        assert!(response.message.contains("two-phase"));
    }

    #[test]
    fn multi_turn_three_by_three_state_still_maps_to_a_solvable_kewb_cubie() {
        let mut state = rubik_core::CubeState::solved(rubik_core::CubeOrder::standard());
        for turn in [
            TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Right, RotationAmount::HalfTurn),
            TurnCommand::outer(Face::Up, RotationAmount::CounterClockwise),
            TurnCommand::outer(Face::Left, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Down, RotationAmount::HalfTurn),
            TurnCommand::outer(Face::Back, RotationAmount::CounterClockwise),
            TurnCommand::outer(Face::Front, RotationAmount::HalfTurn),
            TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
        ] {
            apply_turn_to_state(&mut state, turn).expect("turn should be valid");
        }

        let facelet_string =
            cube_state_to_facelet_string(&state).expect("3x3 export should succeed");
        let face_cube =
            KewbFaceCube::try_from(facelet_string.as_str()).expect("facelet string should parse");
        let cubie = KewbCubieCube::try_from(&face_cube)
            .expect("multi-turn exported state should stay solvable as a cubie");

        assert!(cubie.is_solvable());
    }

    #[test]
    fn every_two_turn_three_by_three_state_maps_to_a_solvable_kewb_cubie() {
        for first_face in Face::ALL {
            for first_rotation in ROTATIONS {
                for second_face in Face::ALL {
                    for second_rotation in ROTATIONS {
                        let mut state =
                            rubik_core::CubeState::solved(rubik_core::CubeOrder::standard());
                        let turns = [
                            TurnCommand::outer(first_face, first_rotation),
                            TurnCommand::outer(second_face, second_rotation),
                        ];

                        for turn in turns {
                            apply_turn_to_state(&mut state, turn).expect("turn should be valid");
                        }

                        let facelet_string = cube_state_to_facelet_string(&state)
                            .expect("3x3 export should succeed");
                        let face_cube = KewbFaceCube::try_from(facelet_string.as_str())
                            .expect("facelet string should parse");
                        let cubie = KewbCubieCube::try_from(&face_cube).unwrap_or_else(|error| {
                            panic!(
                                "two-turn sequence {:?} {:?} then {:?} {:?} became invalid: {error:?}",
                                first_face, first_rotation, second_face, second_rotation
                            )
                        });

                        assert!(
                            cubie.is_solvable(),
                            "two-turn sequence {:?} {:?} then {:?} {:?} became unsolvable",
                            first_face,
                            first_rotation,
                            second_face,
                            second_rotation
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn kewb_moves_match_rubik_core_facelet_exports() {
        let moves = [
            KewbMove::U,
            KewbMove::U2,
            KewbMove::U3,
            KewbMove::D,
            KewbMove::D2,
            KewbMove::D3,
            KewbMove::R,
            KewbMove::R2,
            KewbMove::R3,
            KewbMove::L,
            KewbMove::L2,
            KewbMove::L3,
            KewbMove::F,
            KewbMove::F2,
            KewbMove::F3,
            KewbMove::B,
            KewbMove::B2,
            KewbMove::B3,
        ];

        for move_name in moves {
            let expected = KewbFaceCube::try_from(&KewbCubieCube::default().apply_move(move_name))
                .expect("kewb move should stay valid")
                .to_string();
            let mut actual_state = rubik_core::CubeState::solved(rubik_core::CubeOrder::standard());
            let mapped_turn = kewb_move_to_turn(move_name).expect("move should map to a turn");
            apply_turn_to_state(&mut actual_state, mapped_turn).expect("mapped turn should apply");
            let actual =
                cube_state_to_facelet_string(&actual_state).expect("3x3 export should succeed");

            assert_eq!(
                actual, expected,
                "kewb move {move_name:?} did not match the exported facelet string"
            );
        }
    }

    #[test]
    fn two_phase_fallback_accepts_single_turn_states_on_every_face() {
        for face in Face::ALL {
            let mut state = rubik_core::CubeState::solved(rubik_core::CubeOrder::standard());
            apply_turn_to_state(
                &mut state,
                TurnCommand::outer(face, RotationAmount::Clockwise),
            )
            .expect("turn should be valid");

            let response = solve_request_str(
                &serde_json::to_string(&SolveRequest {
                    state_json: state.to_json().expect("state should serialize"),
                    target_depth: 0,
                    allowed_faces: Vec::new(),
                    turn_history_json: None,
                })
                .expect("request should serialize"),
            );

            assert!(
                matches!(response.kind, SolveOutcomeKind::Solved),
                "expected {face:?} single-turn state to be solvable via fallback, got {:?}: {}",
                response.kind,
                response.message
            );
            assert!(
                !response.turns.is_empty(),
                "{face:?} should produce at least one turn"
            );
        }
    }

    #[test]
    fn center_slice_state_uses_fallback_and_reaches_canonical_solved_state() {
        let mut state = CubeState::solved(CubeOrder::standard());
        apply_turn_to_state(
            &mut state,
            TurnCommand {
                face: Face::Up,
                start_layer: 1,
                width: 1,
                rotation: RotationAmount::CounterClockwise,
            },
        )
        .expect("middle slice should be valid");

        let response = solve_request_str(
            &serde_json::to_string(&SolveRequest {
                state_json: state.to_json().expect("state should serialize"),
                target_depth: 0,
                allowed_faces: Vec::new(),
                turn_history_json: None,
            })
            .expect("request should serialize"),
        );

        assert!(
            matches!(response.kind, SolveOutcomeKind::Solved),
            "expected center-slice state to solve, got {:?}: {}",
            response.kind,
            response.message
        );

        apply_solution_turns(&mut state, &response);
        assert_eq!(state, CubeState::solved(CubeOrder::standard()));
    }

    #[test]
    fn rotated_solved_state_gets_alignment_turns_instead_of_false_solved() {
        let mut state = CubeState::solved(CubeOrder::standard());
        for turn in ROTATE_X {
            apply_turn_to_state(&mut state, turn).expect("whole-cube rotation proxy should apply");
        }

        let frame =
            ThreeByThreeFrame::for_state(&state).expect("rotation proxy should keep a valid frame");
        assert_eq!(
            frame.normalize_state(&state),
            CubeState::solved(CubeOrder::standard())
        );

        let response = solve_request_str(
            &serde_json::to_string(&SolveRequest {
                state_json: state.to_json().expect("state should serialize"),
                target_depth: 1,
                allowed_faces: Vec::new(),
                turn_history_json: None,
            })
            .expect("request should serialize"),
        );

        assert!(
            matches!(response.kind, SolveOutcomeKind::Solved),
            "expected rotated solved state to resolve via alignment turns, got {:?}: {}",
            response.kind,
            response.message
        );
        assert!(
            !response.turns.is_empty(),
            "rotation-aligned solved state should not report an empty solution"
        );

        apply_solution_turns(&mut state, &response);
        assert_eq!(state, CubeState::solved(CubeOrder::standard()));
    }

    #[test]
    fn center_frame_remapping_preserves_exact_lane_constraints() {
        let mut state = CubeState::solved(CubeOrder::standard());
        for turn in ROTATE_X {
            apply_turn_to_state(&mut state, turn).expect("whole-cube rotation proxy should apply");
        }
        apply_turn_to_state(
            &mut state,
            TurnCommand::outer(Face::Front, RotationAmount::Clockwise),
        )
        .expect("outer turn should apply");

        let response = solve_request_str(
            &serde_json::to_string(&SolveRequest {
                state_json: state.to_json().expect("state should serialize"),
                target_depth: 1,
                allowed_faces: vec![2],
                turn_history_json: None,
            })
            .expect("request should serialize"),
        );

        assert!(
            matches!(response.kind, SolveOutcomeKind::Solved),
            "expected normalized face partition to stay solvable, got {:?}: {}",
            response.kind,
            response.message
        );

        apply_solution_turns(&mut state, &response);
        assert_eq!(state, CubeState::solved(CubeOrder::standard()));
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

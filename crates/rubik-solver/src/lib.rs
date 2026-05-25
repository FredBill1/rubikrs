// rubik-solver — thin adapter layer over rcube-rs and min2phase (Kociemba)
// All solving logic lives in rcube-rs and the vendored min2phase; this crate provides:
// 1. wasm-bindgen JSON interface (solve_request_json)
// 2. Solver module setup (console_error_panic_hook)
// 3. SolveError / SolveRequest / SolveResponse types
// 4. Per-solve cancellation (no global static)

pub mod ida2x2;
pub mod kociemba;
mod min2phase;
mod simplify;

#[cfg(debug_assertions)]
use rubik_core::apply_turn_to_state;
use rubik_core::{CubeState, StickerColor, TurnCommand};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Per-solve cancellation token, set by solve() and read by request_cancel().
static CANCEL_TOKEN: OnceLock<Mutex<Option<Arc<AtomicBool>>>> = OnceLock::new();

fn cancel_mutex() -> &'static Mutex<Option<Arc<AtomicBool>>> {
    CANCEL_TOKEN.get_or_init(|| Mutex::new(None))
}

/// Signal the CURRENT solve to cancel. No-op if no solve is running.
pub fn request_cancel() {
    if let Some(token) = cancel_mutex().lock().expect("lock").as_ref() {
        token.store(true, Ordering::Relaxed);
    }
    // Also signal the min2phase (Kociemba) solver in case it's running
    min2phase::CANCEL_FLAG.store(true, Ordering::Relaxed);
}

pub fn solve(state: &CubeState) -> Result<Vec<TurnCommand>, SolveError> {
    let order = state.order.get();

    // Route orders 4–8 to the reduction solver
    if (4..=8).contains(&order) {
        return solve_with_reduction(state);
    }

    let token = Arc::new(AtomicBool::new(false));
    *cancel_mutex().lock().expect("lock") = Some(Arc::clone(&token));

    let turns =
        rcube_rs::solve(state, Some(&token)).map_err(|msg| SolveError::InvalidState(msg))?;

    // Clear the cancel token (drop the Arc)
    *cancel_mutex().lock().expect("lock") = None;

    let simplified = simplify::simplify_turns(&turns, state.order.get());

    // Verify simplification preserves the solution in debug builds.
    // Skip verification for large cubes (>10) because apply_turn_to_state
    // is O(turns × stickers) and becomes prohibitively slow.
    #[cfg(debug_assertions)]
    if state.order.get() <= 10 {
        let mut verify = state.clone();
        for &turn in &simplified {
            apply_turn_to_state(&mut verify, turn).map_err(|e| {
                SolveError::InvalidState(format!("simplification produced invalid turn: {e}"))
            })?;
        }
        if !verify.is_solved() {
            return Err(SolveError::InvalidState(
                "simplification broke the solution".into(),
            ));
        }
    }

    Ok(simplified)
}

/// Solve a 4×4–8×8 cube using the reduction method.
///
/// Pipeline:
/// 1. Reduce the cube (centers → edges → parity) via rubik-reduction
/// 2. Extract a 3×3 facelet from the reduced state
/// 3. Build a virtual 3×3 CubeState and solve with Kociemba
/// 4. Map 3×3 outer-layer turns back to big-cube turns
/// 5. Combine and simplify
fn solve_with_reduction(state: &CubeState) -> Result<Vec<TurnCommand>, SolveError> {
    // Phase 1–3: Reduce via rubik-reduction
    let reduction_result = rubik_reduction::reduce(state).map_err(|e| {
        SolveError::InvalidState(format!("reduction failed: {e}"))
    })?;

    // Extract 3×3 facelet from reduced state
    let facelet = rubik_reduction::extract_facelet(&reduction_result.reduced_state).map_err(|e| {
        SolveError::InvalidState(format!("facelet extraction failed: {e}"))
    })?;

    // Build a virtual 3×3 CubeState from the facelet
    let virtual_3x3 = facelet_to_cube_state(&facelet).map_err(|e| {
        SolveError::InvalidState(format!("facelet conversion failed: {e}"))
    })?;

    // Solve the virtual 3×3 with Kociemba
    let kociemba_turns = kociemba::solve(&virtual_3x3)?;

    // Map 3×3 outer-layer turns to big-cube turns
    let big_cube_turns: Vec<TurnCommand> = kociemba_turns
        .iter()
        .map(|t| rubik_reduction::reduction::map_3x3_turn_to_big_cube(t))
        .collect();

    // Combine reduction turns + 3×3 turns
    let mut all_turns = reduction_result.reduction_turns;
    all_turns.extend(big_cube_turns);

    // Simplify
    let simplified = simplify::simplify_turns(&all_turns, state.order.get());

    // Verify in debug builds
    #[cfg(debug_assertions)]
    {
        let mut verify = state.clone();
        for &turn in &simplified {
            apply_turn_to_state(&mut verify, turn).map_err(|e| {
                SolveError::InvalidState(format!("reduction simplification produced invalid turn: {e}"))
            })?;
        }
        if !verify.is_solved() {
            return Err(SolveError::InvalidState(
                "reduction solver produced incomplete solution".into(),
            ));
        }
    }

    Ok(simplified)
}

/// Build a 3×3 CubeState from a 54-character facelet string.
///
/// Facelet format: 6 faces × 9 stickers, in U-R-F-D-L-B order.
/// Characters: 'U'=White, 'R'=Red, 'F'=Green, 'D'=Yellow, 'L'=Orange, 'B'=Blue
fn facelet_to_cube_state(facelet: &str) -> Result<CubeState, String> {
    use rubik_core::CubeOrder;

    if facelet.len() != 54 {
        return Err(format!("facelet must be 54 chars, got {}", facelet.len()));
    }

    let mut stickers = Vec::with_capacity(54);
    for ch in facelet.chars() {
        let color = match ch {
            'U' => StickerColor::White,
            'R' => StickerColor::Red,
            'F' => StickerColor::Green,
            'D' => StickerColor::Yellow,
            'L' => StickerColor::Orange,
            'B' => StickerColor::Blue,
            other => return Err(format!("invalid facelet character: '{}'", other)),
        };
        stickers.push(color);
    }

    Ok(CubeState {
        version: 1,
        order: CubeOrder::new(3).map_err(|e| format!("{e}"))?,
        stickers,
    })
}

#[derive(Debug, Clone)]
pub enum SolveError {
    InvalidState(String),
    InvalidOrder(String),
    Cancelled,
}

impl std::fmt::Display for SolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SolveError::InvalidState(msg) => write!(f, "invalid state: {}", msg),
            SolveError::InvalidOrder(msg) => write!(f, "invalid order: {}", msg),
            SolveError::Cancelled => write!(f, "cancelled"),
        }
    }
}

// ---------------------------------------------------------------------------
// JSON / wasm-bindgen interface
//
// TypeScript (solver.worker.ts) sends:
//   { "stateJson": "<CubeState JSON string>" }
//
// TypeScript (main.ts) expects response:
//   { "kind": "solved"|"unsolved"|"error", "turns": [...], "explored": 0, "message": "..." }
//
// Turn format expected by TS:
//   { "faceCode": 0..5, "rotationCode": 0..2, "startLayer": N, "width": N, "notation": "..." }
//
// faceCode: 0=U, 1=R, 2=F, 3=D, 4=L, 5=B
// rotationCode: 0=CW, 1=HalfTurn, 2=CCW
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveRequest {
    /// Serialized CubeState JSON string (from export_cube_state())
    #[serde(rename = "stateJson")]
    pub state_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolveResponse {
    pub kind: String,
    pub turns: Vec<TurnCommandJson>,
    pub explored: u64,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnCommandJson {
    /// 0=U, 1=R, 2=F, 3=D, 4=L, 5=B
    #[serde(rename = "faceCode")]
    pub face_code: u8,
    /// 0=CW, 1=HalfTurn, 2=CCW
    #[serde(rename = "rotationCode")]
    pub rotation_code: u8,
    #[serde(rename = "startLayer")]
    pub start_layer: u32,
    pub width: u32,
    pub notation: String,
}

#[cfg(target_arch = "wasm32")]
use rubik_core::Face;

#[cfg(target_arch = "wasm32")]
fn face_to_face_code(face: Face) -> u8 {
    match face {
        Face::Up => 0,
        Face::Right => 1,
        Face::Front => 2,
        Face::Down => 3,
        Face::Left => 4,
        Face::Back => 5,
    }
}

#[cfg(target_arch = "wasm32")]
fn rotation_to_code(rotation: rubik_core::RotationAmount) -> u8 {
    match rotation {
        rubik_core::RotationAmount::Clockwise => 0,
        rubik_core::RotationAmount::HalfTurn => 1,
        rubik_core::RotationAmount::CounterClockwise => 2,
    }
}

#[cfg(target_arch = "wasm32")]
fn face_to_notation(face: Face, start_layer: u32) -> String {
    let base = match face {
        Face::Up => "U",
        Face::Down => "D",
        Face::Right => "R",
        Face::Left => "L",
        Face::Front => "F",
        Face::Back => "B",
    };
    if start_layer == 0 {
        base.to_string()
    } else if start_layer == 1 {
        format!("{}w", base)
    } else {
        format!("{}{}w", start_layer + 1, base)
    }
}

#[cfg(target_arch = "wasm32")]
fn turn_to_json(turn: &TurnCommand) -> TurnCommandJson {
    let rot_str = match turn.rotation {
        rubik_core::RotationAmount::Clockwise => "CW",
        rubik_core::RotationAmount::CounterClockwise => "CCW",
        rubik_core::RotationAmount::HalfTurn => "2",
    };
    TurnCommandJson {
        face_code: face_to_face_code(turn.face),
        rotation_code: rotation_to_code(turn.rotation),
        start_layer: turn.start_layer,
        width: turn.width,
        notation: format!(
            "{} {}",
            face_to_notation(turn.face, turn.start_layer),
            rot_str
        ),
    }
}

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_main() {
    console_error_panic_hook::set_once();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn request_cancel_solver() {
    request_cancel();
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn solve_request_json(request_json: &str) -> String {
    use rubik_core::apply_turn_to_state;

    let request: SolveRequest = match serde_json::from_str(request_json) {
        Ok(r) => r,
        Err(e) => {
            return serde_json::to_string(&SolveResponse {
                kind: "error".to_string(),
                turns: vec![],
                explored: 0,
                message: format!("failed to parse solve request: {e}"),
            })
            .expect("response should serialize");
        }
    };

    // Parse the CubeState from the embedded JSON string
    let state: CubeState = match serde_json::from_str(&request.state_json) {
        Ok(s) => s,
        Err(e) => {
            return serde_json::to_string(&SolveResponse {
                kind: "error".to_string(),
                turns: vec![],
                explored: 0,
                message: format!("failed to parse cube state: {e}"),
            })
            .expect("response should serialize");
        }
    };

    let order = state.order.get();
    let solve_result = if order == 3 {
        kociemba::solve(&state)
    } else if order == 2 {
        ida2x2::solve(&state)
    } else {
        solve(&state)
    };
    match solve_result {
        Ok(turns) => {
            // Verify the solution
            let mut verify = state.clone();
            let mut valid = true;
            for &turn in &turns {
                if apply_turn_to_state(&mut verify, turn).is_err() {
                    valid = false;
                    break;
                }
            }
            let solved = valid && verify.is_solved();
            let json_turns: Vec<TurnCommandJson> = turns.iter().map(|t| turn_to_json(t)).collect();
            serde_json::to_string(&SolveResponse {
                kind: if solved {
                    "solved".to_string()
                } else {
                    "unsolved".to_string()
                },
                turns: json_turns,
                explored: 0,
                message: if solved {
                    format!("solved in {} moves", turns.len())
                } else {
                    "solution verification failed".to_string()
                },
            })
            .expect("response should serialize")
        }
        Err(e) => serde_json::to_string(&SolveResponse {
            kind: "error".to_string(),
            turns: vec![],
            explored: 0,
            message: format!("{}", e),
        })
        .expect("response should serialize"),
    }
}

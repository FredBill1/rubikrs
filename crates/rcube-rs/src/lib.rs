// rcube-rs — Rust translation of the RCube Rubik's Cube solver
//
// Based on RCube (https://github.com/ShellPuppy/RCube/tree/c0e6df125db141eaf0044bf5a39cb54c942cfab2)
// Original C++ code copyright (C) ShellPuppy and contributors, GPL v3
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// Public API:
//   solve(state: &CubeState) -> Result<Vec<TurnCommand>, String>
//
// Architecture:
//   1. Convert rubik-core's CubeState to RCube's internal Cube representation
//   2. Run RCube's deterministic solver (centers→corners→edges)
//   3. Collect recorded moves and convert to rubik-core TurnCommands
//   4. Return the move sequence

pub mod constants;
mod face;
pub mod cube;

use cube::{Cube, MoveRecord};
use rubik_core::{CubeState, Face as RubikFace, RotationAmount, StickerColor, TurnCommand};
use std::sync::atomic::{AtomicBool, Ordering};

// ---------------------------------------------------------------------------
// Face mapping: RCube (0-5) → rubik-core Face enum
// Both systems use identity mapping by color:
//   RCube F(0) → rubik F(4)  [Green]
//   RCube R(1) → rubik R(0)  [Red]
//   RCube B(2) → rubik B(5)  [Blue]
//   RCube L(3) → rubik L(1)  [Orange]
//   RCube U(4) → rubik U(2)  [White]
//   RCube D(5) → rubik D(3)  [Yellow]
// ---------------------------------------------------------------------------

fn rcube_face_to_rubik(rcube_face: u8) -> RubikFace {
    match rcube_face {
        0 => RubikFace::Front,   // F
        1 => RubikFace::Right,   // R
        2 => RubikFace::Back,    // B
        3 => RubikFace::Left,    // L
        4 => RubikFace::Up,      // U
        5 => RubikFace::Down,    // D
        _ => RubikFace::Up,      // shouldn't happen
    }
}

#[allow(dead_code)]
fn rubik_face_to_rcube(face: RubikFace) -> u8 {
    match face {
        RubikFace::Front => 0,
        RubikFace::Right => 1,
        RubikFace::Back => 2,
        RubikFace::Left => 3,
        RubikFace::Up => 4,
        RubikFace::Down => 5,
    }
}

// ---------------------------------------------------------------------------
// rcube_face_index → rubik_face_index (for sticker array access)
// rubik-core uses: R=0, L=1, U=2, D=3, F=4, B=5
// ---------------------------------------------------------------------------

fn rcube_face_idx_to_rubik_face_idx(rcube_idx: u8) -> usize {
    match rcube_idx {
        0 => 2, // F (Green) → rubik Front (index 2 in ALL)
        1 => 1, // R (Red) → rubik Right
        2 => 5, // B (Blue) → rubik Back
        3 => 4, // L (Orange) → rubik Left
        4 => 0, // U (White) → rubik Up
        5 => 3, // D (Yellow) → rubik Down
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// StickerColor → u8 (for RCube's byte-based face data)
// rubik-core StickerColor: White=0, Red=1, Green=2, Yellow=3, Orange=4, Blue=5
// RCube face colors: F=0(Green), R=1(Red), B=2(Blue), L=3(Orange), U=4(White), D=5(Yellow)
// ---------------------------------------------------------------------------

fn sticker_color_to_rcube_color(sc: StickerColor) -> u8 {
    match sc {
        StickerColor::White => 4,  // U color
        StickerColor::Red => 1,    // R color
        StickerColor::Green => 0,  // F color
        StickerColor::Yellow => 5, // D color
        StickerColor::Orange => 3, // L color
        StickerColor::Blue => 2,   // B color
    }
}

/// Convert a rubik-core CubeState to an RCube Cube by copying sticker data.
fn cube_state_to_rcube(state: &CubeState) -> Cube {
    let order = state.order.get();
    let _mem_size = order.next_power_of_two();
    let mut cube = Cube::new(order);

    for rcube_face_idx in 0u8..6u8 {
        let rubik_face_idx = rcube_face_idx_to_rubik_face_idx(rcube_face_idx);
        let face_offset = rubik_face_idx * (order as usize) * (order as usize);
        // RCube side faces (F, R, B, L) number rows from D-side (row 0 = bottom),
        // while rubik-core numbers rows from U-side (row 0 = top).
        // U and D faces may also need flipping depending on coordinate convention.
        // We row-flip all faces to map rubik row r → RCube row (order-1-r).
        let r1 = order as usize - 1;

        for r in 0..order {
            for c in 0..order {
                let rubik_sticker_idx = face_offset + (r as usize) * (order as usize) + c as usize;
                let sc = state.stickers[rubik_sticker_idx];
                let rc_color = sticker_color_to_rcube_color(sc);
                // Row flip: rubik row r (top-down) → RCube row (R1-r) (bottom-up)
                cube.faces[rcube_face_idx as usize]
                    .set_rc((r1 - r as usize) as u32, c, rc_color);
            }
        }
    }

    cube
}

/// Convert a recorded move (rcube face, depth, q) to a rubik-core TurnCommand.
fn move_record_to_turn(record: &MoveRecord) -> TurnCommand {
    let face = rcube_face_to_rubik(record.face);
    let rotation = match record.q {
        1 => RotationAmount::Clockwise,
        2 => RotationAmount::HalfTurn,
        -1 | 3 => RotationAmount::CounterClockwise,
        -2 => RotationAmount::HalfTurn,
        _ => RotationAmount::Clockwise,
    };

    TurnCommand {
        face,
        start_layer: record.depth,
        width: 1,
        rotation,
    }
}

/// Main solve function: converts CubeState → RCube representation,
/// runs the solver, and returns the move sequence as TurnCommands.
///
/// If `cancel` is provided, the solver periodically checks it and
/// returns `Err("solve cancelled")` when set to true.
pub fn solve(state: &CubeState, cancel: Option<&AtomicBool>) -> Result<Vec<TurnCommand>, String> {
    let order = state.order.get();
    if order > 65536 {
        return Err(format!("cube order {} exceeds maximum 65536", order));
    }
    if order == 0 {
        return Err("cube order must be >= 1".to_string());
    }

    // Convert state to RCube representation
    let mut cube = cube_state_to_rcube(state);

    // Use a dummy AtomicBool if no cancel token provided
    let dummy = AtomicBool::new(false);
    let cancel_token = cancel.unwrap_or(&dummy);

    // Run solver
    cube.solve(cancel_token);

    if cancel_token.load(Ordering::Relaxed) {
        return Err("solve cancelled".to_string());
    }

    // Verify cube is solved in RCube's representation (sanity check)
    if !cube.is_cube_solved() {
        return Err("cube not solved after algorithm".to_string());
    }

    // Convert recorded moves to TurnCommands
    let mut turns: Vec<TurnCommand> = Vec::with_capacity(cube.recorded_moves.len());
    for record in &cube.recorded_moves {
        turns.push(move_record_to_turn(record));
    }

    Ok(turns)
}

// ---------------------------------------------------------------------------
// Rubik's face color order for solved state verification
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::{CubeOrder, apply_turn_to_state};

    #[test]
    fn test_face_mapping_roundtrip() {
        for rcube in 0u8..6 {
            let rubik = rcube_face_to_rubik(rcube);
            let back = rubik_face_to_rcube(rubik);
            assert_eq!(rcube, back, "face mapping should roundtrip");
        }
    }

    #[test]
    fn test_solve_solved_2x2() {
        let state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        let result = solve(&state, None);
        assert!(result.is_ok(), "solved 2x2 should solve trivially: {:?}", result.err());
        let turns = result.unwrap();
        // Applying solution to solved state should keep it solved
        let mut verify = state.clone();
        for &t in &turns {
            apply_turn_to_state(&mut verify, t).expect("valid turn");
        }
        let expected = CubeState::solved(CubeOrder::new(2).expect("valid"));
        assert_eq!(verify, expected, "solution should produce solved state");
    }

    #[test]
    fn test_solve_solved_3x3() {
        let state = CubeState::solved(CubeOrder::new(3).expect("valid"));
        let result = solve(&state, None);
        assert!(result.is_ok(), "solved 3x3 should solve: {:?}", result.err());
    }
}

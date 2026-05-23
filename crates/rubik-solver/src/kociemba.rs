// kociemba — adapter layer over vendored min2phase for 3x3 Kociemba two-phase solving.
//
// Provides CubeState ↔ facelet conversion and solution string → Vec<TurnCommand> parsing.

use rubik_core::{CubeState, Face, RotationAmount, StickerColor, TurnCommand};

use super::{cancel_mutex, SolveError};
use crate::min2phase;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Solve a 3x3 cube state using the Kociemba two-phase algorithm.
///
/// Only supports order == 3. For other sizes, use `super::solve()` which delegates to rcube-rs.
///
/// Cancel via `super::request_cancel()` — it sets min2phase's cancel flag which is
/// checked periodically during the search.
pub fn solve(state: &CubeState) -> Result<Vec<TurnCommand>, SolveError> {
    if state.order.get() != 3 {
        return Err(SolveError::InvalidOrder(format!(
            "kociemba solver supports order 3 only, got {}",
            state.order.get()
        )));
    }

    let facelet = cube_state_to_facelet(state);

    // Set up cancel token — same pattern as the rcube-rs solve in lib.rs.
    // request_cancel() will set both the token and min2phase::CANCEL_FLAG.
    let token = Arc::new(AtomicBool::new(false));
    *cancel_mutex().lock().expect("lock") = Some(Arc::clone(&token));

    // Ensure cancel flag starts clean
    min2phase::CANCEL_FLAG.store(false, Ordering::Relaxed);

    let result = min2phase::solve(&facelet, 21);

    // Clean up
    min2phase::CANCEL_FLAG.store(false, Ordering::Relaxed);
    *cancel_mutex().lock().expect("lock") = None;

    // Check if we were cancelled
    if token.load(Ordering::Relaxed) {
        return Err(SolveError::Cancelled);
    }

    let solution_str = result.map_err(|msg| SolveError::InvalidState(msg))?;
    parse_solution(&solution_str)
}

/// Convert a CubeState to a min2phase facelet string (54 chars, U-R-F-D-L-B order).
fn cube_state_to_facelet(state: &CubeState) -> String {
    let order = state.order.get() as usize;
    let face_size = order * order;
    let mut facelet = String::with_capacity(6 * 9);

    for (face_idx, _face) in Face::ALL.iter().enumerate() {
        let offset = face_idx * face_size;
        for i in 0..9 {
            let sticker = &state.stickers[offset + i];
            let ch = match sticker {
                StickerColor::White => 'U',
                StickerColor::Red => 'R',
                StickerColor::Green => 'F',
                StickerColor::Yellow => 'D',
                StickerColor::Orange => 'L',
                StickerColor::Blue => 'B',
            };
            facelet.push(ch);
        }
    }

    facelet
}

/// Parse a min2phase solution string into Vec<TurnCommand>.
///
/// Input: space-separated moves like "R U F' D2 L' ..."
/// Output: Vec<TurnCommand> with start_layer=0, width=1.
fn parse_solution(solution: &str) -> Result<Vec<TurnCommand>, SolveError> {
    let mut turns = Vec::new();

    for token in solution.split_whitespace() {
        let token = token.trim();
        if token.is_empty() || token == "." {
            continue;
        }

        let (face_ch, rotation) = if token.ends_with('\'') {
            (&token[..token.len() - 1], RotationAmount::CounterClockwise)
        } else if token.ends_with('2') {
            (&token[..token.len() - 1], RotationAmount::HalfTurn)
        } else {
            (token, RotationAmount::Clockwise)
        };

        let face = match face_ch {
            "U" => Face::Up,
            "R" => Face::Right,
            "F" => Face::Front,
            "D" => Face::Down,
            "L" => Face::Left,
            "B" => Face::Back,
            other => {
                return Err(SolveError::InvalidState(format!(
                    "unknown move token: '{}' in solution '{}'",
                    other, solution
                )))
            }
        };

        turns.push(TurnCommand {
            face,
            start_layer: 0,
            width: 1,
            rotation,
        });
    }

    Ok(turns)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::CubeOrder;

    #[test]
    fn test_cube_state_to_facelet_solved() {
        let state = CubeState::solved(CubeOrder::new(3).unwrap());
        let facelet = cube_state_to_facelet(&state);
        assert_eq!(
            facelet,
            "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB"
        );
    }

    #[test]
    fn test_parse_solution() {
        let turns = parse_solution("R U F' D2 L' B").unwrap();
        assert_eq!(turns.len(), 6);

        assert_eq!(turns[0].face, Face::Right);
        assert_eq!(turns[0].rotation, RotationAmount::Clockwise);

        assert_eq!(turns[1].face, Face::Up);
        assert_eq!(turns[1].rotation, RotationAmount::Clockwise);

        assert_eq!(turns[2].face, Face::Front);
        assert_eq!(turns[2].rotation, RotationAmount::CounterClockwise);

        assert_eq!(turns[3].face, Face::Down);
        assert_eq!(turns[3].rotation, RotationAmount::HalfTurn);

        assert_eq!(turns[4].face, Face::Left);
        assert_eq!(turns[4].rotation, RotationAmount::CounterClockwise);

        assert_eq!(turns[5].face, Face::Back);
        assert_eq!(turns[5].rotation, RotationAmount::Clockwise);
    }

    #[test]
    fn test_parse_solution_phase_separator() {
        let turns = parse_solution("R U . F' D2").unwrap();
        assert_eq!(turns.len(), 4);
    }
}

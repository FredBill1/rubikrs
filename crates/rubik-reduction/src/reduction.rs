// reduction — reduces a big cube to a virtual 3×3 facelet.
//
// After centers are solved and edges are paired, the cube behaves like a 3×3.
// This module extracts a virtual 3×3 facelet from the reduced big cube state,
// which can then be solved by the Kociemba 3×3 solver (in rubik-solver).
//
// The caller (rubik-solver) is responsible for:
// - Creating a virtual 3×3 CubeState from the facelet
// - Calling Kociemba on it
// - Mapping 3×3 outer-layer moves back to big-cube outer-layer moves

use rubik_core::{CubeState, Face, StickerColor, TurnCommand};

use super::ReductionError;

/// Map a 3×3 Kociemba move (outer layer only) to a big cube TurnCommand.
///
/// The Kociemba solver returns moves like "R", "U'", "F2" which are always
/// outer-layer moves on the big cube (start_layer=0, width=1).
pub fn map_3x3_turn_to_big_cube(turn: &TurnCommand) -> TurnCommand {
    TurnCommand {
        face: turn.face,
        start_layer: 0,
        width: 1,
        rotation: turn.rotation,
    }
}

/// Extract a virtual 3×3 facelet from a reduced big cube.
///
/// Returns a 54-character string in U-R-F-D-L-B order,
/// where each character represents the color of a virtual 3×3 sticker:
///   'U' = White, 'R' = Red, 'F' = Green, 'D' = Yellow, 'L' = Orange, 'B' = Blue
pub fn extract_facelet(state: &CubeState) -> Result<String, ReductionError> {
    let order = state.order.get() as usize;
    if order < 2 {
        return Err(ReductionError::InvalidOrder(format!(
            "cannot extract facelet from order {}",
            order
        )));
    }

    let mut facelet = String::with_capacity(54);
    let last = order - 1;
    let mid = if order % 2 == 1 {
        Some(order / 2)
    } else {
        None
    };

    // For each face, extract the 9 virtual stickers
    // The virtual 3×3 grid on each face:
    //   (0,0) (0,1) (0,2)   →  corner row=0, edge row=0, corner row=0
    //   (1,0) (1,1) (1,2)   →  edge col=0,  center,     edge col=2
    //   (2,0) (2,1) (2,2)   →  corner row=2, edge row=2, corner row=2
    //
    // Map these to actual sticker positions on the N×N face:
    //   (0,0) → (0, 0)           top-left corner
    //   (0,1) → (0, center_col)  top edge
    //   (0,2) → (0, last)        top-right corner
    //   (1,0) → (center_row, 0)  left edge
    //   (1,1) → (center_row, center_col) center
    //   (1,2) → (center_row, last) right edge
    //   (2,0) → (last, 0)        bottom-left corner
    //   (2,1) → (last, center_col) bottom edge
    //   (2,2) → (last, last)     bottom-right corner

    // For even cubes, "center" means any sticker in the center block area
    let center_row = if let Some(m) = mid { m } else { order / 2 - 1 };
    let center_col = center_row;

    // Virtual 3×3 positions to actual N×N positions
    let virtual_to_actual = |vr: usize, vc: usize| -> (usize, usize) {
        let row = match vr {
            0 => 0,
            1 => center_row,
            2 => last,
            _ => unreachable!(),
        };
        let col = match vc {
            0 => 0,
            1 => center_col,
            2 => last,
            _ => unreachable!(),
        };
        (row, col)
    };

    for face in Face::ALL {
        let face_idx = crate::center_types::face_to_idx(face);
        let face_offset = face_idx * order * order;

        for vr in 0..3 {
            for vc in 0..3 {
                let (row, col) = virtual_to_actual(vr, vc);
                let sticker_idx = face_offset + row * order + col;
                let sticker = state.stickers[sticker_idx];
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
    }

    Ok(facelet)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::{CubeOrder, apply_turn_to_state};

    #[test]
    fn test_extract_facelet_solved_4x4() {
        let state = CubeState::solved(CubeOrder::new(4).unwrap());
        let facelet = extract_facelet(&state).unwrap();

        // A solved 4×4 should extract as a solved 3×3 facelet
        assert_eq!(facelet.len(), 54);
        assert_eq!(
            facelet,
            "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB"
        );
    }

    #[test]
    fn test_extract_facelet_solved_5x5() {
        let state = CubeState::solved(CubeOrder::new(5).unwrap());
        let facelet = extract_facelet(&state).unwrap();
        assert_eq!(facelet.len(), 54);
        assert_eq!(
            facelet,
            "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB"
        );
    }

    #[test]
    fn test_extract_facelet_after_outer_turn_4x4() {
        use rubik_core::RotationAmount;
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        // Apply R turn
        let turn = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        apply_turn_to_state(&mut state, turn).unwrap();

        let facelet = extract_facelet(&state).unwrap();

        // After R turn on 4×4, the facelet should look like a 3×3 after R
        assert_eq!(facelet.len(), 54);

        // The R face stickers rotate in place (all Red), so the R face still has 9 Rs.
        // The U, F, D, B faces have their right columns shifted.
        // The facelet should still have 9 of each color (R turn preserves the multiset).
        let counts: Vec<usize> = ['U', 'R', 'F', 'D', 'L', 'B']
            .iter()
            .map(|&c| facelet.chars().filter(|&ch| ch == c).count())
            .collect();

        // After an outer R turn, all 6 colors still appear exactly 9 times each
        // because it's a permutation of the stickers  
        for &count in &counts {
            assert_eq!(count, 9, "Each color should appear 9 times after an R turn");
        }

        // The facelet should NOT be solved (it should differ from solved)
        assert_ne!(
            facelet,
            "UUUUUUUUURRRRRRRRRFFFFFFFFFDDDDDDDDDLLLLLLLLLBBBBBBBBB"
        );
    }

    #[test]
    fn test_map_3x3_turn_to_big_cube() {
        use rubik_core::RotationAmount;
        let turn_3x3 = TurnCommand::outer(Face::Right, RotationAmount::Clockwise);
        let big_turn = map_3x3_turn_to_big_cube(&turn_3x3);

        assert_eq!(big_turn.face, Face::Right);
        assert_eq!(big_turn.start_layer, 0);
        assert_eq!(big_turn.width, 1);
        assert_eq!(big_turn.rotation, RotationAmount::Clockwise);
    }
}

// center_types — coordinate system and utilities for center pieces on N×N cubes.
//
// A "center piece" is any sticker not on an edge or corner of the face.
// For an N×N cube:
//   - Even N: all N² stickers on a face are center pieces
//   - Odd N:  the middle sticker is the "true center" (fixed), the rest are movable centers
//
// Center pieces are identified by their (face, row, col) position on each face.
// The goal of center solving is to make every center sticker match its face's color.

use rubik_core::{CubeState, Face, StickerColor};

/// Represents a single center piece identified by its position on a face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CenterPiece {
    /// Which face this center piece is on
    pub face: Face,
    /// Row index on the face (0 = top, N-1 = bottom)
    pub row: u32,
    /// Column index on the face (0 = left, N-1 = right)
    pub col: u32,
}

/// Represents which center pieces belong to which face in the solved state.
#[derive(Debug, Clone)]
pub struct CenterState {
    /// For each face, the set of center piece positions that should be that face's color.
    /// Index by face code: 0=U, 1=R, 2=F, 3=D, 4=L, 5=B
    pub face_centers: [Vec<CenterPiece>; 6],
    /// The cube order
    pub order: u32,
}

impl CenterPiece {
    /// Returns the color of this piece in the given cube state.
    pub fn color_in(&self, state: &CubeState) -> StickerColor {
        let order = state.order.get() as usize;
        let face_idx = face_to_idx(self.face);
        let offset = face_idx * order * order;
        let sticker_idx = offset + (self.row as usize) * order + (self.col as usize);
        state.stickers[sticker_idx]
    }

    /// Returns the target face for this piece (where it should be in a solved cube).
    pub fn target_face(&self, order: u32) -> Face {
        if order % 2 == 1 {
            let mid = order / 2;
            if self.row == mid && self.col == mid {
                // True center — fixed, belongs to its current face
                return self.face;
            }
        }
        // The piece's target face is determined by its color
        // This is a placeholder — the actual solver will determine this from the piece's color
        self.face
    }
}

/// Convert Face to index 0..5 matching the sticker array layout in CubeState:
/// U=0, R=1, F=2, D=3, L=4, B=5
pub fn face_to_idx(face: Face) -> usize {
    match face {
        Face::Up => 0,
        Face::Right => 1,
        Face::Front => 2,
        Face::Down => 3,
        Face::Left => 4,
        Face::Back => 5,
    }
}

/// Convert index 0..5 back to Face
pub fn idx_to_face(idx: usize) -> Face {
    match idx {
        0 => Face::Up,
        1 => Face::Right,
        2 => Face::Front,
        3 => Face::Down,
        4 => Face::Left,
        5 => Face::Back,
        _ => panic!("invalid face index: {}", idx),
    }
}

/// Get all center piece positions for a given face on an N×N cube.
/// Center pieces are the inner-block stickers, excluding the outer ring
/// (corners and edges) and the true center for odd cubes.
///
/// For a 4×4: returns the 2×2 inner block (4 positions per face).
/// For a 5×5: returns the 8 inner positions (excluding corners, edges, and the true center).
/// For an N×N: returns positions where row and col are both in [1, N-2].
pub fn center_positions_on_face(face: Face, order: u32) -> Vec<CenterPiece> {
    let mut pieces = Vec::new();
    let mid = if order % 2 == 1 { Some(order / 2) } else { None };
    let last = order - 1;

    for row in 0..order {
        for col in 0..order {
            // Skip the outer ring: corners and edges
            if row == 0 || row == last || col == 0 || col == last {
                continue;
            }
            // Skip the true center for odd cubes
            if let Some(m) = mid {
                if row == m && col == m {
                    continue;
                }
            }
            pieces.push(CenterPiece { face, row, col });
        }
    }
    pieces
}

/// Build the complete center state for a cube: for each face, collect all center
/// positions on every face that should be that face's color.
pub fn build_center_state(state: &CubeState) -> CenterState {
    let order = state.order.get();
    let mut face_centers: [Vec<CenterPiece>; 6] = Default::default();

    for face in Face::ALL {
        let positions = center_positions_on_face(face, order);
        for piece in positions {
            let color = piece.color_in(state);
            let target_face = Face::from_solved_color(color);
            face_centers[face_to_idx(target_face)].push(piece);
        }
    }

    CenterState {
        face_centers,
        order,
    }
}

/// Count how many center pieces have the correct color on their face.
pub fn count_solved_centers(state: &CubeState) -> usize {
    let order = state.order.get();
    let mut solved = 0;

    for face in Face::ALL {
        let target_color = face.solved_color();
        let positions = center_positions_on_face(face, order);
        for piece in positions {
            if piece.color_in(state) == target_color {
                solved += 1;
            }
        }
    }
    solved
}

/// Check if all centers are solved.
pub fn centers_solved(state: &CubeState) -> bool {
    let order = state.order.get();
    let centers_per_face = (order - 2) * (order - 2);
    let total = if order % 2 == 0 {
        6 * centers_per_face as usize
    } else {
        6 * (centers_per_face as usize - 1) // subtract true center
    };
    count_solved_centers(state) == total
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::CubeOrder;

    #[test]
    fn test_center_positions_4x4() {
        // 4×4: 2×2 center block = 4 center pieces per face
        let positions = center_positions_on_face(Face::Up, 4);
        assert_eq!(positions.len(), 4);
        // Check that all positions are within bounds
        for p in &positions {
            assert!(p.row >= 1 && p.row <= 2);
            assert!(p.col >= 1 && p.col <= 2);
        }
    }

    #[test]
    fn test_center_positions_5x5() {
        // 5×5: 3×3 inner block minus 1 true center = 8 center pieces per face
        let positions = center_positions_on_face(Face::Up, 5);
        assert_eq!(positions.len(), 8);
        // Verify the middle position (2,2) is excluded
        assert!(!positions.contains(&CenterPiece {
            face: Face::Up,
            row: 2,
            col: 2
        }));
    }

    #[test]
    fn test_count_solved_centers() {
        let state = CubeState::solved(CubeOrder::new(4).unwrap());
        assert!(centers_solved(&state));
        assert_eq!(count_solved_centers(&state), 6 * 4); // 6 faces × 4 centers
    }

    #[test]
    fn test_build_center_state_solved() {
        let state = CubeState::solved(CubeOrder::new(4).unwrap());
        let center_state = build_center_state(&state);

        // In a solved state, all centers on face U should be in face_centers[0] (U)
        assert_eq!(center_state.face_centers[0].len(), 4);
        for piece in &center_state.face_centers[0] {
            assert_eq!(piece.face, Face::Up);
        }
    }
}

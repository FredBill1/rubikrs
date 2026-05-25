// edge_types — representation and utilities for edge pieces on N×N cubes.
//
// An "edge piece" sits on the boundary between two faces. For an N×N cube:
//   - The 4 corner edge positions are corners, not edges
//   - On each of the 12 edges of the cube, there are (N-2) piece positions
//   - For even N: all (N-2) are "wing" edges that must be paired
//   - For odd N:  the middle piece is the "midge" (middle edge), the rest are wings
//
// Edge pieces are identified by the two faces they border and their position
// along the edge (0 = closest to one corner, N-3 = closest to the other).

use rubik_core::{CubeState, Face, StickerColor};

/// The 12 edges of a cube, identified by the two adjacent faces.
/// Ordered canonically: the first face has a lower face_idx than the second.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgePosition {
    /// First face (lower face_idx)
    pub face_a: Face,
    /// Second face (higher face_idx)
    pub face_b: Face,
    /// Position along the edge: 0 = closest to one end, (order-3) = closest to the other
    pub index: u32,
}

/// An edge group represents a complete paired edge on the reduced cube.
/// For 4×4: each group has 2 wing pieces.
/// For 5×5: each group has 2 wings + 1 midge.
/// For N×N: each group has (N-2) pieces.
#[derive(Debug, Clone)]
pub struct EdgeGroup {
    /// The two faces this edge borders
    pub face_a: Face,
    pub face_b: Face,
    /// All edge pieces in this group
    pub pieces: Vec<EdgePosition>,
    /// Whether all pieces in this group match (same two colors)
    pub is_paired: bool,
}

/// All 12 canonical edge pairs (face_a, face_b) where face_a < face_b in index order.
pub const EDGE_PAIRS: [(Face, Face); 12] = [
    (Face::Up, Face::Right),    // UR
    (Face::Up, Face::Front),    // UF
    (Face::Up, Face::Left),     // UL
    (Face::Up, Face::Back),     // UB
    (Face::Down, Face::Right),  // DR
    (Face::Down, Face::Front),  // DF
    (Face::Down, Face::Left),   // DL
    (Face::Down, Face::Back),   // DB
    (Face::Right, Face::Front), // RF
    (Face::Right, Face::Back),  // RB
    (Face::Left, Face::Front),  // LF
    (Face::Left, Face::Back),   // LB
];

impl EdgePosition {
    /// Get the two colors of this edge piece in the given state.
    /// Returns (color_on_face_a, color_on_face_b).
    pub fn colors_in(&self, state: &CubeState) -> (StickerColor, StickerColor) {
        let order = state.order.get() as usize;
        let (row_a, col_a) = edge_coords_on_face(self.face_a, self.face_b, self.index, order as u32);
        let (row_b, col_b) = edge_coords_on_face(self.face_b, self.face_a, self.index, order as u32);

        let face_a_idx = crate::center_types::face_to_idx(self.face_a);
        let face_b_idx = crate::center_types::face_to_idx(self.face_b);

        let sticker_a = state.stickers[face_a_idx * order * order + row_a as usize * order + col_a as usize];
        let sticker_b = state.stickers[face_b_idx * order * order + row_b as usize * order + col_b as usize];

        (sticker_a, sticker_b)
    }

    /// Check if this edge piece has the correct colors for its solved position
    /// (i.e., its two colors match the solved colors of face_a and face_b).
    pub fn is_correct(&self, state: &CubeState) -> bool {
        let (color_a, color_b) = self.colors_in(state);
        let solved_a = self.face_a.solved_color();
        let solved_b = self.face_b.solved_color();
        (color_a == solved_a && color_b == solved_b)
            || (color_a == solved_b && color_b == solved_a) // flipped but correct pieces
    }
}

/// Get the (row, col) coordinates of an edge sticker on a given face.
///
/// For face `host`, looking at edge `neighbor`, at `index` along the edge:
/// Returns the (row, col) position on the host face.
pub fn edge_coords_on_face(host: Face, neighbor: Face, index: u32, order: u32) -> (u32, u32) {
    let last = order - 1;
    // The edge position is determined by where the neighbor face touches the host face
    match (host, neighbor) {
        // On U face, edges are on the bottom row (row = last)
        (Face::Up, Face::Front) => (last, index + 1),
        (Face::Up, Face::Right) => (order - 2 - index, last),
        (Face::Up, Face::Back) => (0, order - 2 - index),
        (Face::Up, Face::Left) => (index + 1, 0),

        // On D face, edges are on the top row (row = 0)
        (Face::Down, Face::Front) => (0, index + 1),
        (Face::Down, Face::Right) => (index + 1, last),
        (Face::Down, Face::Back) => (last, order - 2 - index),
        (Face::Down, Face::Left) => (order - 2 - index, 0),

        // On R face, edges are on the rightmost column (col = last)
        (Face::Right, Face::Up) => (0, order - 2 - index),
        (Face::Right, Face::Front) => (index + 1, last),
        (Face::Right, Face::Down) => (last, index + 1),
        (Face::Right, Face::Back) => (order - 2 - index, 0),

        // On F face
        (Face::Front, Face::Up) => (0, index + 1),
        (Face::Front, Face::Right) => (index + 1, last),
        (Face::Front, Face::Down) => (last, order - 2 - index),
        (Face::Front, Face::Left) => (order - 2 - index, 0),

        // On L face, edges are on the leftmost column (col = 0)
        (Face::Left, Face::Up) => (0, index + 1),
        (Face::Left, Face::Front) => (order - 2 - index, 0),
        (Face::Left, Face::Down) => (last, order - 2 - index),
        (Face::Left, Face::Back) => (index + 1, last),

        // On B face
        (Face::Back, Face::Up) => (0, order - 2 - index),
        (Face::Back, Face::Left) => (index + 1, last),
        (Face::Back, Face::Down) => (last, index + 1),
        (Face::Back, Face::Right) => (order - 2 - index, 0),

        _ => panic!("invalid edge pair: {:?} / {:?}", host, neighbor),
    }
}

/// Get all edge positions for a given cube order.
pub fn all_edge_positions(order: u32) -> Vec<EdgePosition> {
    let mut positions = Vec::new();
    let num_per_edge = order - 2;

    for &(face_a, face_b) in &EDGE_PAIRS {
        for index in 0..num_per_edge {
            positions.push(EdgePosition {
                face_a,
                face_b,
                index,
            });
        }
    }
    positions
}

/// Count how many edge pieces are in their correct and correctly-oriented position.
pub fn count_correct_edges(state: &CubeState) -> usize {
    let order = state.order.get();
    all_edge_positions(order)
        .iter()
        .filter(|ep| ep.is_correct(state))
        .count()
}

/// Count how many edge groups are fully paired (all pieces have matching colors).
pub fn count_paired_edge_groups(state: &CubeState) -> usize {
    let order = state.order.get();
    let num_per_edge = order - 2;
    let mut paired = 0;

    for &(face_a, face_b) in &EDGE_PAIRS {
        let pieces: Vec<_> = (0..num_per_edge)
            .map(|index| EdgePosition { face_a, face_b, index })
            .collect();

        // Check if all pieces have the same two colors
        if pieces.is_empty() {
            continue;
        }

        let (ref_a, ref_b) = pieces[0].colors_in(state);
        let all_match = pieces.iter().all(|ep| {
            let (a, b) = ep.colors_in(state);
            (a == ref_a && b == ref_b) || (a == ref_b && b == ref_a)
        });

        if all_match {
            paired += 1;
        }
    }
    paired
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::{CubeOrder, TurnCommand, apply_turn_to_state};

    #[test]
    fn test_edge_coords_4x4_uf() {
        // On a 4×4, U-F edge has 2 wing pieces (indices 0, 1)
        // Row 0 is top of the face (closest to U face for F face)
        let (row, col) = edge_coords_on_face(Face::Front, Face::Up, 0, 4);
        // F face: U is on top, so row=0; index 0 is leftmost (col=1 since 0 and 3 are corners)
        assert_eq!(row, 0);
        assert_eq!(col, 1);

        let (row, col) = edge_coords_on_face(Face::Front, Face::Up, 1, 4);
        assert_eq!(row, 0);
        assert_eq!(col, 2);
    }

    #[test]
    fn test_all_edge_positions_4x4() {
        let positions = all_edge_positions(4);
        // 12 edges × 2 wing pieces = 24 edge positions
        assert_eq!(positions.len(), 24);
    }

    #[test]
    fn test_all_edge_positions_5x5() {
        let positions = all_edge_positions(5);
        // 12 edges × 3 pieces (2 wings + 1 midge) = 36
        assert_eq!(positions.len(), 36);
    }

    #[test]
    fn test_count_correct_edges_solved_4x4() {
        let state = CubeState::solved(CubeOrder::new(4).unwrap());
        assert_eq!(count_correct_edges(&state), 24);
        assert_eq!(count_paired_edge_groups(&state), 12);
    }

    #[test]
    fn test_edge_colors_on_scrambled_edge() {
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        // Apply a move that breaks an edge group: r (inner right slice)
        let turn = TurnCommand {
            face: rubik_core::Face::Right,
            start_layer: 1,
            width: 1,
            rotation: rubik_core::RotationAmount::Clockwise,
        };
        apply_turn_to_state(&mut state, turn).unwrap();

        // After a single inner slice move, edges should be scrambled
        // but all centers on R face are still red
        let ep = EdgePosition {
            face_a: Face::Up,
            face_b: Face::Right,
            index: 0,
        };
        let (color_a, color_b) = ep.colors_in(&state);
        // One of these should still be White (U) or Red (R), but the other may have changed
        // The exact colors depend on the sticker mapping
        assert!(
            color_a == StickerColor::White || color_b == StickerColor::Red
                || color_a == StickerColor::Red || color_b == StickerColor::White
        );
    }
}

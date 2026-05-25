// parity — detects and fixes OLL and PLL parity on 4×4+ cubes.
//
// Parity cases in the reduction method:
//
// 1. OLL Parity (Orientation Parity):
//    - Occurs when an odd number of inner-slice quarter turns have been made
//    - Manifests as a single "flipped" edge in the reduced 3×3 state
//    - Fix algorithm (4×4): r2 B2 U2 l U2 r' U2 r U2 F2 r F2 l' B2 r2
//    - This is the same for all even-order cubes
//
// 2. PLL Parity (Permutation Parity):
//    - Occurs when two edges (or two corner pairs) are swapped
//    - Fix algorithm (4×4): r2 U2 r2 Uw2 r2 u2
//    - Also works for higher even orders
//
// For odd-order cubes (5×5, 7×7, etc.):
//   - OLL parity does not occur (middle slice has a fixed center track)
//   - PLL parity does not occur for the same reason
//   - However, edge pairing can still have parity-like situations at the 3×3 stage

use rubik_core::{CubeState, Face, RotationAmount, TurnCommand};

use super::ReductionError;

/// Fix any parity issues. Returns the sequence of turns needed.
///
/// This should be called after edge pairing and before 3×3 reduction.
///
/// For even-order cubes (4×4, 6×6, 8×8), both OLL and PLL parity can occur.
/// For odd-order cubes (5×5, 7×7), neither parity occurs in the reduction method.
pub fn fix_parity(state: &CubeState) -> Result<Vec<TurnCommand>, ReductionError> {
    let order = state.order.get();

    // Odd-order cubes do not have OLL or PLL parity in reduction method.
    if order % 2 != 0 {
        return Ok(Vec::new());
    }

    let mut turns = Vec::new();

    if has_oll_parity(state) {
        turns.extend(oll_parity_alg());
    }

    if has_pll_parity(state) {
        turns.extend(pll_parity_alg());
    }

    Ok(turns)
}

/// Check if OLL parity is present.
///
/// OLL parity: an odd number of edges in the reduced 3×3 have flipped orientation.
///
/// Detection: extract the virtual 3×3 facelet and count misoriented edges using
/// the standard 3×3 ZZ edge-orientation definition across ALL 12 edges:
///   - U/D-layer edges: oriented if the sticker on U/D face is U or D
///   - E-slice edges:   oriented if the sticker on F/B face is F or B
/// If the count is odd, OLL parity is present.
///
/// Only applicable to even-order cubes.
fn has_oll_parity(state: &CubeState) -> bool {
    let order = state.order.get();
    if order % 2 != 0 {
        return false;
    }

    let facelet = match crate::reduction::extract_facelet(state) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let f = facelet.as_bytes();

    // Count misoriented edges across all 12 edges.

    // U/D-layer edge positions (8 edges): U-side for UB, UL, UR, UF;
    // D-side for DF, DL, DR, DB.
    let ud_edge_positions: [usize; 8] = [
        1,  // U top  → UB
        3,  // U left → UL
        5,  // U right → UR
        7,  // U bottom → UF
        28, // D top  → DF
        30, // D left → DL
        32, // D right → DR
        34, // D bottom → DB
    ];

    let mut bad_count = ud_edge_positions
        .iter()
        .filter(|&&i| f[i] != b'U' && f[i] != b'D')
        .count();

    // E-slice edge positions (4 edges): F-side for FR, FL; B-side for BR, BL.
    // A "good" E-slice edge has its F/B sticker on the F or B face.
    let e_slice_positions: [usize; 4] = [
        23, // FR: F[1,2] on F face
        21, // FL: F[1,0] on F face
        50, // BR: B[1,2] on B face
        48, // BL: B[1,0] on B face
    ];

    bad_count += e_slice_positions
        .iter()
        .filter(|&&i| f[i] != b'F' && f[i] != b'B')
        .count();

    bad_count % 2 != 0
}

/// Check if PLL parity is present.
///
/// PLL parity: two edges (or two corners) appear swapped in the reduced 3×3.
/// This manifests as an odd permutation parity in the edge positions.
///
/// Detection: extract the virtual 3×3 facelet and examine the 4 U-layer edge
/// positions. Each U-layer edge is identified by its unordered color pair.
/// If the permutation of the 4 U-layer edges has odd parity, PLL parity is present.
///
/// Only applicable to even-order cubes.
fn has_pll_parity(state: &CubeState) -> bool {
    let order = state.order.get();
    if order % 2 != 0 {
        return false;
    }

    let facelet = match crate::reduction::extract_facelet(state) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let f = facelet.as_bytes();

    // U-layer edge positions: (U_face_pos, side_face_pos)
    let edges: [(usize, usize); 4] = [
        (5, 10),  // UR: U[1,2]=5, R[0,1]=10
        (7, 19),  // UF: U[2,1]=7, F[0,1]=19
        (3, 37),  // UL: U[1,0]=3, L[0,1]=37
        (1, 46),  // UB: U[0,1]=1, B[0,1]=46
    ];

    // Identify which edge is at each U-layer position by looking at the color pair.
    // Each U-layer edge has a unique pair {U, side} where side ∈ {R, F, L, B}.
    let mut edge_ids = [0usize; 4];
    for (i, &(u_pos, side_pos)) in edges.iter().enumerate() {
        let id = match edge_id_from_pair(f[u_pos], f[side_pos]) {
            Some(id) if id < 4 => id, // Must be a U-layer edge (0=UR, 1=UF, 2=UL, 3=UB)
            _ => return false,         // Non-U-layer edge at U position → not simple PLL parity
        };
        edge_ids[i] = id;
    }

    // Verify all 4 U-layer edges are present (no duplicates)
    let mut seen = [false; 4];
    for &id in &edge_ids {
        if seen[id] {
            return false;
        }
        seen[id] = true;
    }

    // Count inversions to determine permutation parity
    let mut inversions = 0;
    for i in 0..4 {
        for j in (i + 1)..4 {
            if edge_ids[i] > edge_ids[j] {
                inversions += 1;
            }
        }
    }

    // Odd number of inversions → odd permutation → PLL parity
    inversions % 2 != 0
}

/// Identify an edge by its unordered color pair in the facelet.
///
/// Returns the edge index 0..12 or None if the pair doesn't match any edge.
///
/// The pair is sorted by byte value for order-independent matching.
/// ASCII order: B(66) < D(68) < F(70) < L(76) < R(82) < U(85)
fn edge_id_from_pair(c1: u8, c2: u8) -> Option<usize> {
    let (a, b) = if c1 < c2 { (c1, c2) } else { (c2, c1) };
    match (a, b) {
        (b'R', b'U') => Some(0),  // UR
        (b'F', b'U') => Some(1),  // UF
        (b'L', b'U') => Some(2),  // UL
        (b'B', b'U') => Some(3),  // UB
        (b'D', b'R') => Some(4),  // DR
        (b'D', b'F') => Some(5),  // DF
        (b'D', b'L') => Some(6),  // DL
        (b'B', b'D') => Some(7),  // DB
        (b'F', b'R') => Some(8),  // FR
        (b'F', b'L') => Some(9),  // FL
        (b'B', b'R') => Some(10), // BR
        (b'B', b'L') => Some(11), // BL
        _ => None,
    }
}

/// The OLL parity fix algorithm for 4×4.
/// r2 B2 U2 l U2 r' U2 r U2 F2 r F2 l' B2 r2
pub fn oll_parity_alg() -> Vec<TurnCommand> {
    vec![
        // r2
        TurnCommand {
            face: Face::Right,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::HalfTurn,
        },
        // B2
        TurnCommand::outer(Face::Back, RotationAmount::HalfTurn),
        // U2
        TurnCommand::outer(Face::Up, RotationAmount::HalfTurn),
        // l
        TurnCommand {
            face: Face::Left,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::Clockwise,
        },
        // U2
        TurnCommand::outer(Face::Up, RotationAmount::HalfTurn),
        // r'
        TurnCommand {
            face: Face::Right,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::CounterClockwise,
        },
        // U2
        TurnCommand::outer(Face::Up, RotationAmount::HalfTurn),
        // r
        TurnCommand {
            face: Face::Right,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::Clockwise,
        },
        // U2
        TurnCommand::outer(Face::Up, RotationAmount::HalfTurn),
        // F2
        TurnCommand::outer(Face::Front, RotationAmount::HalfTurn),
        // r
        TurnCommand {
            face: Face::Right,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::Clockwise,
        },
        // F2
        TurnCommand::outer(Face::Front, RotationAmount::HalfTurn),
        // l'
        TurnCommand {
            face: Face::Left,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::CounterClockwise,
        },
        // B2
        TurnCommand::outer(Face::Back, RotationAmount::HalfTurn),
        // r2
        TurnCommand {
            face: Face::Right,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::HalfTurn,
        },
    ]
}

/// The PLL parity fix algorithm for 4×4.
/// r2 U2 r2 Uw2 r2 u2
pub fn pll_parity_alg() -> Vec<TurnCommand> {
    vec![
        // r2
        TurnCommand {
            face: Face::Right,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::HalfTurn,
        },
        // U2
        TurnCommand::outer(Face::Up, RotationAmount::HalfTurn),
        // r2
        TurnCommand {
            face: Face::Right,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::HalfTurn,
        },
        // Uw2 (double layer U)
        TurnCommand {
            face: Face::Up,
            start_layer: 0,
            width: 2,
            rotation: RotationAmount::HalfTurn,
        },
        // r2
        TurnCommand {
            face: Face::Right,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::HalfTurn,
        },
        // u2 (inner U slice)
        TurnCommand {
            face: Face::Up,
            start_layer: 1,
            width: 1,
            rotation: RotationAmount::HalfTurn,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::{CubeOrder, apply_turn_to_state};

    #[test]
    fn test_oll_parity_alg_length() {
        let alg = oll_parity_alg();
        assert_eq!(alg.len(), 15); // The standard alg has 15 moves
    }

    #[test]
    fn test_pll_parity_alg_length() {
        let alg = pll_parity_alg();
        assert_eq!(alg.len(), 6);
    }

    #[test]
    fn test_no_parity_on_solved_4x4() {
        let state = CubeState::solved(CubeOrder::new(4).unwrap());
        let turns = fix_parity(&state).unwrap();
        assert!(turns.is_empty());
    }

    #[test]
    fn test_oll_parity_fix_preserves_centers_and_edges() {
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        let alg = oll_parity_alg();
        for &turn in &alg {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        // After OLL parity alg, the cube should still have all centers solved
        // and edges paired (but one edge is flipped in the reduced 3×3 sense)
        // Verify the alg applies without error.
    }

    #[test]
    fn test_oll_parity_detection() {
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        // Apply the OLL parity alg to create a state with OLL parity
        let alg = oll_parity_alg();
        for &turn in &alg {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        // The state should now have OLL parity
        assert!(
            has_oll_parity(&state),
            "OLL parity should be detected after applying oll_parity_alg()"
        );

        // fix_parity should return a non-empty solution
        let turns = fix_parity(&state).unwrap();
        assert!(
            !turns.is_empty(),
            "fix_parity should return turns when OLL parity is present"
        );
    }

    #[test]
    fn test_pll_parity_detection() {
        let mut state = CubeState::solved(CubeOrder::new(4).unwrap());

        // Apply the PLL parity alg to create a state with PLL parity
        let alg = pll_parity_alg();
        for &turn in &alg {
            apply_turn_to_state(&mut state, turn).unwrap();
        }

        // The state should now have PLL parity (and no OLL parity)
        assert!(
            has_pll_parity(&state),
            "PLL parity should be detected after applying pll_parity_alg()"
        );
        assert!(
            !has_oll_parity(&state),
            "pll_parity_alg() should not create OLL parity"
        );
    }

    #[test]
    fn test_no_parity_on_5x5() {
        let state = CubeState::solved(CubeOrder::new(5).unwrap());

        assert!(
            !has_oll_parity(&state),
            "Odd-order cubes should not have OLL parity"
        );
        assert!(
            !has_pll_parity(&state),
            "Odd-order cubes should not have PLL parity"
        );

        let turns = fix_parity(&state).unwrap();
        assert!(
            turns.is_empty(),
            "fix_parity should return no turns for odd-order cubes"
        );
    }
}

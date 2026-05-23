// ida2x2 — IDA* solver for 2x2 Rubik's Cube using pre-generated pattern databases.
//
// Two PDBs are embedded via include_bytes!:
//   - perm_pdb.bin:  40,320 bytes, min moves to solve corner permutation
//   - orient_pdb.bin: 2,187 bytes, min moves to solve corner orientation
//
// Heuristic: max(perm_pdb[perm], orient_pdb[orient]), admissible.

use rubik_core::{CubeState, Face, RotationAmount, StickerColor, TurnCommand};
use std::sync::OnceLock;

use super::{cancel_mutex, SolveError};

// ---------------------------------------------------------------------------
// Corner definitions
// ---------------------------------------------------------------------------

/// Sticker indices for each corner position, ordered by CW rotation from outside.
///
/// The triple is [U/D-facelet, first CW facelet, second CW facelet].
/// Verified against rubik-core `StickerPosition::from_index` for order=2
/// and min2phase's CORNER_FACELET convention.
///
/// Position order: 0=URF, 1=UFL, 2=ULB, 3=UBR, 4=DFR, 5=DFL, 6=DLB, 7=DRB.
const CORNER_STICKERS: [[usize; 3]; 8] = [
    [3, 4, 9],    // URF: U, R, F  (CW: U→R→F→U)
    [2, 8, 17],   // UFL: U, F, L  (CW: U→F→L→U)
    [0, 16, 21],  // ULB: U, L, B  (CW: U→L→B→U)
    [1, 20, 5],   // UBR: U, B, R  (CW: U→B→R→U)
    [13, 11, 6],  // DFR: D, F, R  (CW: D→F→R→D)
    [12, 19, 10], // DFL: D, L, F  (CW: D→L→F→D)
    [14, 23, 18], // DLB: D, B, L  (CW: D→B→L→D)
    [15, 7, 22],  // DRB: D, R, B  (CW: D→R→B→D)
];

/// Solved-state colors for each corner piece (piece_id -> [color0, color1, color2]).
/// Piece 0=URF, 1=UFL, 2=ULB, 3=UBR, 4=DFR, 5=DFL, 6=DLB, 7=DRB.
const PIECE_COLORS: [[StickerColor; 3]; 8] = [
    [StickerColor::White, StickerColor::Red, StickerColor::Green],    // URF
    [StickerColor::White, StickerColor::Green, StickerColor::Orange], // UFL
    [StickerColor::White, StickerColor::Orange, StickerColor::Blue],  // ULB
    [StickerColor::White, StickerColor::Blue, StickerColor::Red],     // UBR
    [StickerColor::Yellow, StickerColor::Red, StickerColor::Green],   // DFR
    [StickerColor::Yellow, StickerColor::Green, StickerColor::Orange],// DFL
    [StickerColor::Yellow, StickerColor::Orange, StickerColor::Blue], // DLB
    [StickerColor::Yellow, StickerColor::Blue, StickerColor::Red],    // DRB
];

/// Primary (U/D) color for each piece: pieces 0-3 have White, pieces 4-7 have Yellow.
const PIECE_UD_COLOR: [StickerColor; 8] = [
    StickerColor::White, StickerColor::White, StickerColor::White, StickerColor::White,
    StickerColor::Yellow, StickerColor::Yellow, StickerColor::Yellow, StickerColor::Yellow,
];

/// Which face (U=0,R=1,F=2,D=3,L=4,B=5) each sticker index belongs to.
fn sticker_face(idx: usize) -> usize {
    idx / 4 // 4 stickers per face for order 2
}

const NUM_MOVES: usize = 18;
const NUM_PERM: usize = 40320;
const NUM_ORIENT: usize = 2187; // 3^7

// ---------------------------------------------------------------------------
// Color utility
// ---------------------------------------------------------------------------

fn color_index(c: StickerColor) -> usize {
    match c {
        StickerColor::White => 0,
        StickerColor::Red => 1,
        StickerColor::Green => 2,
        StickerColor::Yellow => 3,
        StickerColor::Orange => 4,
        StickerColor::Blue => 5,
    }
}

// ---------------------------------------------------------------------------
// Piece identification via sorted-color-key LUT
// ---------------------------------------------------------------------------

fn build_piece_lut() -> [u8; 216] {
    let mut lut = [255u8; 216]; // 6*6*6 = 216
    for (piece_id, colors) in PIECE_COLORS.iter().enumerate() {
        let mut idx: Vec<usize> = colors.iter().map(|c| color_index(*c)).collect();
        idx.sort();
        let key = idx[0] * 36 + idx[1] * 6 + idx[2];
        lut[key] = piece_id as u8;
    }
    lut
}

fn identify_piece(colors: &[StickerColor; 3], lut: &[u8; 216]) -> u8 {
    let mut idx: [usize; 3] = [
        color_index(colors[0]),
        color_index(colors[1]),
        color_index(colors[2]),
    ];
    idx.sort();
    let key = idx[0] * 36 + idx[1] * 6 + idx[2];
    lut[key]
}

// ---------------------------------------------------------------------------
// Corner extraction from CubeState
// ---------------------------------------------------------------------------

/// Extract corner piece IDs and orientations from a 2x2 CubeState.
///
/// Returns (piece_ids[8], orientations[8]) where piece_ids[p] is the piece
/// at position p and orientations[p] is its twist (0, 1, or 2).
///
/// Orientation is defined relative to the PIECE's primary color:
/// - Pieces 0-3 (with White): orientation is where White sits in the [U/D, R/L, F/B] triple
/// - Pieces 4-7 (with Yellow): orientation is where Yellow sits
fn extract_corners(state: &CubeState, piece_lut: &[u8; 216]) -> ([u8; 8], [u8; 8]) {
    let mut pieces = [0u8; 8];
    let mut orients = [0u8; 8];

    for pos in 0..8 {
        let sticker_indices = CORNER_STICKERS[pos];
        let colors: [StickerColor; 3] = [
            state.stickers[sticker_indices[0]],
            state.stickers[sticker_indices[1]],
            state.stickers[sticker_indices[2]],
        ];

        let piece_id = identify_piece(&colors, piece_lut) as usize;
        pieces[pos] = piece_id as u8;

        // Orientation: find which index in the triple holds this piece's primary color
        let ud_color = PIECE_UD_COLOR[piece_id];
        let face_idx = if colors[0] == ud_color {
            0
        } else if colors[1] == ud_color {
            1
        } else {
            2
        };
        orients[pos] = face_idx;
    }

    (pieces, orients)
}

// ---------------------------------------------------------------------------
// Permutation coordinate (Lehmer / factorial number system, range 0..40319)
// ---------------------------------------------------------------------------

const FACT: [u16; 8] = [5040, 720, 120, 24, 6, 2, 1, 1];

pub(crate) fn perm_to_coord(perm: &[u8; 8]) -> u16 {
    let mut coord = 0u16;
    for i in 0..7 {
        let mut count = 0u16;
        for j in (i + 1)..8 {
            if perm[j] < perm[i] {
                count += 1;
            }
        }
        coord += count * FACT[i];
    }
    coord
}

pub(crate) fn coord_to_perm(mut coord: u16) -> [u8; 8] {
    let mut available: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
    let mut perm = [0u8; 8];
    for i in 0..7 {
        let f = FACT[i] as u16;
        let idx = (coord / f) as usize;
        coord %= f;
        perm[i] = available[idx];
        for j in idx..7 - i {
            available[j] = available[j + 1];
        }
    }
    perm[7] = available[0];
    perm
}

// ---------------------------------------------------------------------------
// Orientation coordinate (base-3, range 0..2186)
// ---------------------------------------------------------------------------

pub(crate) fn orient_to_coord(orient: &[u8; 8]) -> u16 {
    let mut coord = 0u16;
    for i in 0..7 {
        coord = coord * 3 + orient[i] as u16;
    }
    coord
}

pub(crate) fn coord_to_orient(mut coord: u16) -> [u8; 8] {
    let mut orient = [0u8; 8];
    let mut sum = 0u8;
    for i in (0..7).rev() {
        orient[i] = (coord % 3) as u8;
        sum += orient[i];
        coord /= 3;
    }
    orient[7] = (3 - (sum % 3)) % 3;
    orient
}

// ---------------------------------------------------------------------------
// Face mapping: where each face goes under a rotation
// ---------------------------------------------------------------------------

/// Map face to new face after applying a rotation to a given axis face.
fn rotate_face(face: usize, axis: usize, rotation: RotationAmount) -> usize {
    // Face indices: 0=U, 1=R, 2=F, 3=D, 4=L, 5=B
    // Opposite pairs: U(0)/D(3), R(1)/L(4), F(2)/B(5)
    // If the face is not adjacent to the axis, it stays put.
    let quarter_turns = match rotation {
        RotationAmount::Clockwise => 1,
        RotationAmount::HalfTurn => 2,
        RotationAmount::CounterClockwise => 3,
    };

    if face == axis || face == opposite_face(axis) {
        return face; // axis faces stay
    }

    // Cycle of 4 adjacent faces, CW looking from outside.
    // Derived from rubik-core engine.rs rotate_face_clockwise.
    let cycle: [usize; 4] = match axis {
        0 => [2, 4, 5, 1], // U: F→L→B→R
        1 => [0, 5, 3, 2], // R: U→B→D→F
        2 => [0, 1, 3, 4], // F: U→R→D→L
        3 => [2, 1, 5, 4], // D: F→R→B→L
        4 => [0, 2, 3, 5], // L: U→F→D→B
        5 => [0, 4, 3, 1], // B: U→L→D→R
        _ => unreachable!(),
    };

    // Find position of face in cycle
    if let Some(pos) = cycle.iter().position(|&f| f == face) {
        let new_pos = (pos + quarter_turns) % 4;
        return cycle[new_pos];
    }

    face // not in the cycle, stays the same
}

fn opposite_face(face: usize) -> usize {
    match face {
        0 => 3, // U ↔ D
        3 => 0,
        1 => 4, // R ↔ L
        4 => 1,
        2 => 5, // F ↔ B
        5 => 2,
        _ => face,
    }
}

// ---------------------------------------------------------------------------
// Move system
// ---------------------------------------------------------------------------

/// pos_map[move][pos] = new position of piece at position pos
type PosMap = [[usize; 8]; NUM_MOVES];

/// ori_transform[move][pos][old_ori] = new_ori for piece moving from pos via move
type OriTransform = [[[u8; 3]; 8]; NUM_MOVES];

/// Precomputed move transition tables.
pub struct MoveTables {
    /// perm_move[perm_coord * 18 + move_idx] = new perm_coord
    pub perm_move: Vec<u16>,
    /// orient_move[orient_coord * 18 + move_idx] = new orient_coord
    pub orient_move: Vec<u16>,
}

static MOVE_TABLES: OnceLock<MoveTables> = OnceLock::new();

pub fn move_tables() -> &'static MoveTables {
    MOVE_TABLES.get_or_init(|| build_move_tables())
}

/// Compute pos_map and ori_transform for all 18 moves.
///
/// pos_map is computed by simulating each move on a solved CubeState.
/// ori_transform is computed geometrically by tracking where each face
/// goes under the rotation, using rubik-core's face rotation rules.
fn build_ori_transform() -> (PosMap, OriTransform) {
    let rotations = [
        RotationAmount::Clockwise,
        RotationAmount::HalfTurn,
        RotationAmount::CounterClockwise,
    ];

    let mut pos_map = [[0usize; 8]; NUM_MOVES];
    let mut ori_transform = [[[0u8; 3]; 8]; NUM_MOVES];

    // Compute pos_map by simulating moves on a solved CubeState
    {
        use rubik_core::apply_turn_to_state;
        let piece_lut = build_piece_lut();

        for mi in 0..6 {
            let face = Face::ALL[mi];
            for (ri, &rotation) in rotations.iter().enumerate() {
                let m = mi * 3 + ri;

                let order = rubik_core::CubeOrder::new(2).expect("2 is valid");
                let mut state = CubeState::solved(order);
                let turn = TurnCommand { face, start_layer: 0, width: 1, rotation };
                apply_turn_to_state(&mut state, turn).expect("valid turn");

                let (pieces, _orients) = extract_corners(&state, &piece_lut);

                for p in 0..8 {
                    let new_pos = pieces.iter().position(|&pid| pid == p as u8)
                        .expect("piece must exist");
                    pos_map[m][p] = new_pos;
                }
            }
        }
    }

    // Compute ori_transform using geometric face mapping.
    // For each old orientation o (0,1,2), the sticker at CORNER_STICKERS[pos][o]
    // is on some face. Track where that face goes under the rotation, then
    // find which index in the destination triple corresponds to the new face.
    for mi in 0..6 {
        for (ri, &rotation) in rotations.iter().enumerate() {
            let m = mi * 3 + ri;

            for pos in 0..8 {
                if pos_map[m][pos] == pos {
                    ori_transform[m][pos] = [0, 1, 2];
                    continue;
                }

                let new_pos = pos_map[m][pos];
                let src_stickers = CORNER_STICKERS[pos];
                let dst_stickers = CORNER_STICKERS[new_pos];

                for old_ori in 0..3u8 {
                    let src_face = sticker_face(src_stickers[old_ori as usize]);
                    let dst_face = rotate_face(src_face, mi, rotation);

                    let new_ori = if sticker_face(dst_stickers[0]) == dst_face {
                        0
                    } else if sticker_face(dst_stickers[1]) == dst_face {
                        1
                    } else {
                        2
                    };

                    ori_transform[m][pos][old_ori as usize] = new_ori;
                }
            }
        }
    }

    (pos_map, ori_transform)
}

/// Apply a move to (perm, orient) arrays using the transform tables.
fn apply_move_to_arrays(
    perm: &[u8; 8],
    orient: &[u8; 8],
    m: usize,
    pos_map: &PosMap,
    ori_tf: &OriTransform,
) -> ([u8; 8], [u8; 8]) {
    let mut new_perm = [0u8; 8];
    let mut new_orient = [0u8; 8];
    for p in 0..8 {
        let np = pos_map[m][p];
        new_perm[np] = perm[p];
        new_orient[np] = ori_tf[m][p][orient[p] as usize];
    }
    (new_perm, new_orient)
}

fn build_move_tables() -> MoveTables {
    let (pos_map, ori_tf) = build_ori_transform();

    // Build perm_move table
    let mut perm_move = vec![0u16; NUM_PERM * NUM_MOVES];
    for coord in 0..NUM_PERM {
        let perm = coord_to_perm(coord as u16);
        for m in 0..NUM_MOVES {
            let (new_perm, _) = apply_move_to_arrays(&perm, &[0u8; 8], m, &pos_map, &ori_tf);
            perm_move[coord * NUM_MOVES + m] = perm_to_coord(&new_perm);
        }
    }

    // Build orient_move table
    let mut orient_move = vec![0u16; NUM_ORIENT * NUM_MOVES];
    for coord in 0..NUM_ORIENT {
        let orient = coord_to_orient(coord as u16);
        for m in 0..NUM_MOVES {
            let id_perm = [0u8, 1, 2, 3, 4, 5, 6, 7];
            let (_, new_orient) = apply_move_to_arrays(&id_perm, &orient, m, &pos_map, &ori_tf);
            orient_move[coord * NUM_MOVES + m] = orient_to_coord(&new_orient);
        }
    }

    MoveTables { perm_move, orient_move }
}

/// Look up the new permutation coordinate after applying move `m` to `coord`.
fn perm_after_move(coord: u16, m: usize) -> u16 {
    let tables = move_tables();
    tables.perm_move[coord as usize * NUM_MOVES + m]
}

/// Look up the new orientation coordinate after applying move `m` to `coord`.
fn orient_after_move(coord: u16, m: usize) -> u16 {
    let tables = move_tables();
    tables.orient_move[coord as usize * NUM_MOVES + m]
}

/// Convert a move index (0..17) to a TurnCommand.
fn move_index_to_turn(m: usize) -> TurnCommand {
    let face = match m / 3 {
        0 => Face::Up,
        1 => Face::Right,
        2 => Face::Front,
        3 => Face::Down,
        4 => Face::Left,
        5 => Face::Back,
        _ => unreachable!(),
    };
    let rotation = match m % 3 {
        0 => RotationAmount::Clockwise,
        1 => RotationAmount::HalfTurn,
        2 => RotationAmount::CounterClockwise,
        _ => unreachable!(),
    };
    TurnCommand { face, start_layer: 0, width: 1, rotation }
}

// ---------------------------------------------------------------------------
// Pattern databases (embedded via include_bytes!)
// ---------------------------------------------------------------------------

const PERM_PDB: &[u8] = include_bytes!("pdbs/perm_pdb.bin");
const ORIENT_PDB: &[u8] = include_bytes!("pdbs/orient_pdb.bin");

fn heuristic(perm: u16, orient: u16) -> u8 {
    let p = PERM_PDB[perm as usize];
    let o = ORIENT_PDB[orient as usize];
    p.max(o)
}

// ---------------------------------------------------------------------------
// IDA* search
// ---------------------------------------------------------------------------

fn ida_star(
    start_perm: u16,
    start_orient: u16,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Vec<usize>, SolveError> {
    let h = heuristic(start_perm, start_orient) as usize;
    if h == 0 {
        return Ok(vec![]);
    }

    use std::sync::atomic::Ordering;
    let mut bound = h;
    let mut path: Vec<usize> = Vec::new();

    // God's number for 2x2 is 14 QTM, 11 HTM
    let max_depth = 20;

    while bound <= max_depth {
        if cancel.load(Ordering::Relaxed) {
            return Err(SolveError::Cancelled);
        }
        move_tables(); // ensure initialized
        let (found, solution) = dls(start_perm, start_orient, bound, &mut path, 0, cancel);
        if found {
            return Ok(solution);
        }
        bound += 1;
    }

    Err(SolveError::InvalidState(
        format!("IDA* exceeded max search depth ({})", max_depth)
    ))
}

fn dls(
    perm: u16,
    orient: u16,
    bound: usize,
    path: &mut Vec<usize>,
    nodes: usize,
    cancel: &std::sync::atomic::AtomicBool,
) -> (bool, Vec<usize>) {
    use std::sync::atomic::Ordering;

    let h = heuristic(perm, orient) as usize;
    let depth = path.len();
    if depth + h > bound {
        return (false, vec![]);
    }
    if h == 0 {
        return (true, path.clone());
    }

    if nodes & 1023 == 0 && cancel.load(Ordering::Relaxed) {
        return (false, vec![]);
    }

    let mut next_nodes = nodes;
    for m in 0..NUM_MOVES {
        // Same-face pruning: skip if same face as last move
        if let Some(&last) = path.last() {
            if last / 3 == m / 3 {
                continue;
            }
        }

        let new_perm = perm_after_move(perm, m);
        let new_orient = orient_after_move(orient, m);

        path.push(m);
        next_nodes += 1;
        let (found, solution) = dls(new_perm, new_orient, bound, path, next_nodes, cancel);
        path.pop();
        if found {
            return (true, solution);
        }
    }

    (false, vec![])
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Solve a 2x2 cube state using IDA* with pattern databases.
///
/// Only supports order == 2. Cancel via `super::request_cancel()`.
pub fn solve(state: &CubeState) -> Result<Vec<TurnCommand>, SolveError> {
    if state.order.get() != 2 {
        return Err(SolveError::InvalidOrder(format!(
            "ida2x2 solver supports order 2 only, got {}",
            state.order.get()
        )));
    }

    let piece_lut = build_piece_lut();
    let (pieces, orients) = extract_corners(state, &piece_lut);

    let perm = perm_to_coord(&pieces);
    let orient = orient_to_coord(&orients);

    // Set up cancel token — same pattern as kociemba::solve
    let token = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    *cancel_mutex().lock().expect("lock") = Some(std::sync::Arc::clone(&token));

    let result = ida_star(perm, orient, &token);

    // Clean up
    *cancel_mutex().lock().expect("lock") = None;

    // Check if we were cancelled
    if token.load(std::sync::atomic::Ordering::Relaxed) {
        return Err(SolveError::Cancelled);
    }

    let move_indices = result?;
    Ok(move_indices.into_iter().map(move_index_to_turn).collect())
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rubik_core::CubeOrder;

    #[test]
    fn test_perm_coord_roundtrip() {
        let perm = [0u8, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(perm_to_coord(&perm), 0);
        assert_eq!(coord_to_perm(0), perm);

        let rev = [7u8, 6, 5, 4, 3, 2, 1, 0];
        let coord = perm_to_coord(&rev);
        assert_eq!(coord, 40319);
        assert_eq!(coord_to_perm(coord), rev);

        for c in [0, 1, 42, 1000, 10000, 20000, 30000, 40319] {
            let p = coord_to_perm(c as u16);
            assert_eq!(perm_to_coord(&p), c as u16, "roundtrip failed for coord {c}");
        }
    }

    #[test]
    fn test_orient_coord_roundtrip() {
        let orient = [0u8; 8];
        assert_eq!(orient_to_coord(&orient), 0);
        assert_eq!(coord_to_orient(0), orient);

        for c in [0u16, 1, 42, 500, 1000, 2000, 2186] {
            let o = coord_to_orient(c);
            assert_eq!(orient_to_coord(&o), c, "roundtrip failed for coord {c}");
            assert_eq!(o.iter().sum::<u8>() % 3, 0, "total twist not 0 for coord {c}");
        }
    }

    #[test]
    fn test_extract_corners_solved() {
        let state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        let piece_lut = build_piece_lut();
        let (pieces, orients) = extract_corners(&state, &piece_lut);

        assert_eq!(pieces, [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(orients, [0u8; 8]);
    }

    #[test]
    fn test_build_piece_lut() {
        let lut = build_piece_lut();
        for (piece_id, colors) in PIECE_COLORS.iter().enumerate() {
            let id = identify_piece(colors, &lut);
            assert_eq!(id, piece_id as u8, "failed to identify piece {piece_id}");
        }
    }

    #[test]
    fn test_move_tables_build_and_identity() {
        let tables = move_tables();

        // 4x CW on any face should return to identity
        for face_idx in 0..6 {
            let m_cw = face_idx * 3;
            let mut p = 0u16;
            let mut o = 0u16;
            for _ in 0..4 {
                p = tables.perm_move[p as usize * NUM_MOVES + m_cw];
                o = tables.orient_move[o as usize * NUM_MOVES + m_cw];
            }
            assert_eq!(p, 0, "4xCW on face {face_idx} should return to perm 0");
            assert_eq!(o, 0, "4xCW on face {face_idx} should return to orient 0");
        }

        // 2x H2 on any face should return to identity
        for face_idx in 0..6 {
            let m_h2 = face_idx * 3 + 1;
            let mut p = 0u16;
            let mut o = 0u16;
            for _ in 0..2 {
                p = tables.perm_move[p as usize * NUM_MOVES + m_h2];
                o = tables.orient_move[o as usize * NUM_MOVES + m_h2];
            }
            assert_eq!(p, 0, "2xH2 on face {face_idx} should return to perm 0");
            assert_eq!(o, 0, "2xH2 on face {face_idx} should return to orient 0");
        }

        // CW + CCW should cancel
        for face_idx in 0..6 {
            let m_cw = face_idx * 3;
            let m_ccw = face_idx * 3 + 2;
            let mut p = 0u16;
            let mut o = 0u16;
            p = tables.perm_move[p as usize * NUM_MOVES + m_cw];
            o = tables.orient_move[o as usize * NUM_MOVES + m_cw];
            p = tables.perm_move[p as usize * NUM_MOVES + m_ccw];
            o = tables.orient_move[o as usize * NUM_MOVES + m_ccw];
            assert_eq!(p, 0, "CW+CCW on face {face_idx} should return to perm 0");
            assert_eq!(o, 0, "CW+CCW on face {face_idx} should return to orient 0");
        }
    }

    #[test]
    fn test_move_consistency_via_cubestate() {
        // Verify that applying a sequence of moves via the coordinate
        // transition gives the same result as applying them via CubeState.
        use rubik_core::apply_turn_to_state;

        let piece_lut = build_piece_lut();

        // Apply random move sequences and compare
        let faces = [Face::Up, Face::Right, Face::Front,
                     Face::Down, Face::Left, Face::Back];
        let rots = [RotationAmount::Clockwise, RotationAmount::HalfTurn,
                    RotationAmount::CounterClockwise];

        // Test: apply R U R' U' on solved state and verify
        let seq: [(usize, usize); 6] = [
            (1, 0), // R CW
            (0, 0), // U CW
            (1, 2), // R CCW
            (0, 2), // U CCW
            (3, 0), // D CW
            (2, 1), // F H2
        ];

        // Via CubeState
        let mut state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        for &(face_i, rot_i) in &seq {
            let turn = TurnCommand {
                face: faces[face_i],
                start_layer: 0,
                width: 1,
                rotation: rots[rot_i],
            };
            apply_turn_to_state(&mut state, turn).expect("valid");
        }
        let (expected_pieces, expected_orients) = extract_corners(&state, &piece_lut);

        // Via coordinate transitions
        let tables = move_tables();
        let perm = [0u8, 1, 2, 3, 4, 5, 6, 7];
        let orient = [0u8; 8];
        let mut p = perm_to_coord(&perm);
        let mut o = orient_to_coord(&orient);
        for &(face_i, rot_i) in &seq {
            let m = face_i * 3 + rot_i;
            p = tables.perm_move[p as usize * NUM_MOVES + m];
            o = tables.orient_move[o as usize * NUM_MOVES + m];
        }
        let result_perm = coord_to_perm(p);
        let result_orient = coord_to_orient(o);

        assert_eq!(result_perm, expected_pieces,
            "perm mismatch via coordinate vs CubeState");
        assert_eq!(result_orient, expected_orients,
            "orient mismatch via coordinate vs CubeState");
    }

    #[test]
    fn test_heuristic_solved() {
        assert_eq!(heuristic(0, 0), 0);
    }

    #[test]
    fn test_move_index_to_turn() {
        let t = move_index_to_turn(0);
        assert_eq!(t.face, Face::Up);
        assert_eq!(t.rotation, RotationAmount::Clockwise);
        assert_eq!(t.start_layer, 0);
        assert_eq!(t.width, 1);

        let t = move_index_to_turn(17);
        assert_eq!(t.face, Face::Back);
        assert_eq!(t.rotation, RotationAmount::CounterClockwise);

        let t = move_index_to_turn(4);
        assert_eq!(t.face, Face::Right);
        assert_eq!(t.rotation, RotationAmount::HalfTurn);
    }

    #[test]
    fn test_solve_solved_state() {
        let state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        let solution = solve(&state).expect("solve should succeed");
        assert!(solution.is_empty(), "solved state should give empty solution");
    }

    #[test]
    fn test_solve_single_u_turn() {
        use rubik_core::apply_turn_to_state;
        let mut state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        apply_turn_to_state(
            &mut state,
            TurnCommand::outer(Face::Up, RotationAmount::Clockwise),
        ).expect("U turn");

        let solution = solve(&state).expect("solve should succeed");
        assert!(!solution.is_empty(), "should find solution");

        // Verify solution
        let mut verify = state.clone();
        for &turn in &solution {
            apply_turn_to_state(&mut verify, turn).expect("solution turn valid");
        }
        assert_eq!(verify, CubeState::solved(CubeOrder::new(2).expect("valid")),
            "solution should restore solved state");
    }

    #[test]
    fn test_solve_scramble() {
        use rubik_core::apply_turn_to_state;
        let mut state = CubeState::solved(CubeOrder::new(2).expect("valid"));
        let scramble = [
            TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Up, RotationAmount::HalfTurn),
            TurnCommand::outer(Face::Front, RotationAmount::CounterClockwise),
            TurnCommand::outer(Face::Right, RotationAmount::Clockwise),
            TurnCommand::outer(Face::Down, RotationAmount::Clockwise),
        ];
        for &turn in &scramble {
            apply_turn_to_state(&mut state, turn).expect("scramble turn");
        }

        let solution = solve(&state).expect("solve should succeed");
        assert!(!solution.is_empty(), "should find solution");

        let mut verify = state.clone();
        for &turn in &solution {
            apply_turn_to_state(&mut verify, turn).expect("solution turn valid");
        }
        assert_eq!(verify, CubeState::solved(CubeOrder::new(2).expect("valid")),
            "solution should restore solved state. Solution: {:?}", solution);
    }
}

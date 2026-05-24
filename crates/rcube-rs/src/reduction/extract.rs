use rubik_core::{CubeOrder, CubeState, Face, StickerColor};

use crate::cube::Cube;

/// Build a 3x3 `CubeState` from a reduced NxN cube.
///
/// The cube must have solved centers and paired edges. The outermost
/// stickers of each face are read to form the 3x3 representation.
pub fn extract_3x3_state(cube: &Cube) -> CubeState {
    let n = cube.row_size;
    let n1 = n - 1;
    let mid = n1 / 2;

    let mut stickers = Vec::with_capacity(54);

    // rubik-core Face::ALL order: [Up, Right, Front, Down, Left, Back]
    for &rubik_face in &Face::ALL {
        let rcube_idx = rubik_face_to_rcube_idx(rubik_face);
        let face_stickers = read_3x3_face(cube, rcube_idx, n1, mid);
        stickers.extend(face_stickers);
    }

    CubeState {
        version: 1,
        order: CubeOrder::new(3).expect("3 is a valid order"),
        stickers,
    }
}

/// Read 9 stickers from an RCube face in rubik-core row-major order,
/// representing the reduced 3x3 state.
fn read_3x3_face(cube: &Cube, rcube_face_idx: u8, n1: u32, mid: u32) -> Vec<StickerColor> {
    let face = &cube.faces[rcube_face_idx as usize];
    let mut stickers = Vec::with_capacity(9);

    // rubik-core rows: 0=top, 1=middle, 2=bottom
    // rubik-core cols: 0=left, 1=middle, 2=right
    for rubik_row in 0u32..3u32 {
        for rubik_col in 0u32..3u32 {
            let (nx_row, nx_col) = map_3x3_to_nxn(rubik_row, rubik_col, n1, mid);
            // RCube stores rows bottom-up; rubik-core stores them top-down
            let rcube_row = n1 - nx_row;
            let rcube_color = face.get_rc(rcube_row, nx_col);
            stickers.push(rcube_color_to_sticker(rcube_color));
        }
    }

    stickers
}

/// Map a 3x3 position (rubik row-major coordinates) to the
/// corresponding NxN position.
fn map_3x3_to_nxn(rubik_row: u32, rubik_col: u32, n1: u32, mid: u32) -> (u32, u32) {
    let nx_row = match rubik_row {
        0 => 0,
        1 => mid,
        2 => n1,
        _ => mid,
    };
    let nx_col = match rubik_col {
        0 => 0,
        1 => mid,
        2 => n1,
        _ => mid,
    };
    (nx_row, nx_col)
}

/// Convert RCube u8 color (face identifier) to rubik-core StickerColor.
fn rcube_color_to_sticker(c: u8) -> StickerColor {
    match c {
        0 => StickerColor::Green,   // F
        1 => StickerColor::Red,     // R
        2 => StickerColor::Blue,    // B
        3 => StickerColor::Orange,  // L
        4 => StickerColor::White,   // U
        5 => StickerColor::Yellow,  // D
        _ => StickerColor::White,
    }
}

/// Map rubik-core Face to RCube face index.
fn rubik_face_to_rcube_idx(face: Face) -> u8 {
    match face {
        Face::Up => 4,
        Face::Right => 1,
        Face::Front => 0,
        Face::Down => 5,
        Face::Left => 3,
        Face::Back => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::Cube;

    #[test]
    fn extract_solved_4x4_produces_solved_3x3() {
        let cube = Cube::new(4);
        let state = extract_3x3_state(&cube);
        assert_eq!(state.order.get(), 3);

        // For a solved 4x4, the sticker counts should match a solved 3x3
        let mut counts = [0u32; 6];
        for s in &state.stickers {
            counts[*s as usize] += 1;
        }
        // Each of the 6 colors appears 9 times (6*9 = 54)
        for c in 0..6 {
            assert_eq!(counts[c], 9, "color {} should appear 9 times", c);
        }
    }

    #[test]
    fn extract_solved_5x5_produces_solved_3x3() {
        let cube = Cube::new(5);
        let state = extract_3x3_state(&cube);
        let mut counts = [0u32; 6];
        for s in &state.stickers {
            counts[*s as usize] += 1;
        }
        for c in 0..6 {
            assert_eq!(counts[c], 9);
        }
    }
}
